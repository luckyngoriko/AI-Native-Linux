//! T-039 integration tests for S1.3 §7.3 GC and chunk refcounts.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_fs::{
    AiosFs, ChunkId, ChunkRef, ConsistencyClass, FsContext, FsError, GcPassDriver, GcReason,
    InMemoryAiosFs, MutableAiosFs, ObjectId, ObjectWriteRequest, PointerKind,
    QuarantineDisposition, QuarantineDriver, QuarantineTrigger, SubjectRef, VersionId,
    VersionPurgeReason, VersionState,
};

fn subject(id: &str) -> SubjectRef {
    SubjectRef(id.to_owned())
}

fn context(id: &str) -> FsContext {
    FsContext {
        subject: subject(id),
        action_id: None,
        expected_snapshot_id: None,
        consistency_class: ConsistencyClass::Snapshot,
    }
}

fn chunk_ref(bytes: &[u8]) -> ChunkRef {
    ChunkRef(ChunkId::from_hash_bytes(bytes))
}

fn write_request(id: &str, name: &str, chunks: Vec<ChunkRef>) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: None,
        parent_version_ids: Vec::new(),
        chunks,
        metadata_delta: serde_json::json!({
            "name": name,
            "mime": "text/plain"
        }),
        action_id: None,
        subject: subject(id),
    }
}

fn append_request(
    object_id: ObjectId,
    parent_version_id: VersionId,
    id: &str,
    name: &str,
    chunks: Vec<ChunkRef>,
) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: Some(object_id),
        parent_version_ids: vec![parent_version_id],
        chunks,
        metadata_delta: serde_json::json!({
            "name": name,
            "mime": "text/plain"
        }),
        action_id: None,
        subject: subject(id),
    }
}

async fn write_one(fs: &InMemoryAiosFs, name: &str, chunk: ChunkRef) -> (ObjectId, VersionId) {
    let written = fs
        .write_object(
            write_request("family:alice", name, vec![chunk]),
            &context("family:alice"),
        )
        .await
        .expect("write object");

    (written.object_id, written.version_id)
}

async fn current_chunk_refcount(fs: &InMemoryAiosFs, object_id: &ObjectId) -> u32 {
    fs.read_object(object_id, None)
        .await
        .expect("read object")
        .chunks
        .first()
        .expect("chunk present")
        .ref_count
}

#[tokio::test]
async fn write_object_creates_chunk_with_refcount_one() {
    let fs = InMemoryAiosFs::new();
    let chunk = chunk_ref(b"shared");
    let (object_id, _) = write_one(&fs, "v1", chunk).await;

    assert_eq!(current_chunk_refcount(&fs, &object_id).await, 1);
}

#[tokio::test]
async fn two_writes_referencing_same_chunk_increment_refcount_to_two() {
    let fs = InMemoryAiosFs::new();
    let shared = chunk_ref(b"shared");
    write_one(&fs, "left", shared.clone()).await;
    let (right_object_id, _) = write_one(&fs, "right", shared).await;

    assert_eq!(current_chunk_refcount(&fs, &right_object_id).await, 2);
}

#[tokio::test]
async fn purge_version_decrements_referenced_chunks() {
    let fs = InMemoryAiosFs::new();
    let shared = chunk_ref(b"shared");
    let (_, purged_version_id) = write_one(&fs, "left", shared.clone()).await;
    let (remaining_object_id, _) = write_one(&fs, "right", shared.clone()).await;

    let decremented = fs
        .purge_version(&purged_version_id, VersionPurgeReason::OperatorRequested)
        .expect("purge version");

    assert_eq!(decremented, vec![shared.0]);
    assert_eq!(current_chunk_refcount(&fs, &remaining_object_id).await, 1);
}

#[tokio::test]
async fn reclaim_chunk_with_positive_refcount_returns_chunk_still_referenced() {
    let fs = InMemoryAiosFs::new();
    let chunk = chunk_ref(b"live");
    write_one(&fs, "live", chunk.clone()).await;

    let err = fs
        .reclaim_chunk(&chunk.0)
        .expect_err("live chunk must not reclaim");

    assert_eq!(
        err,
        FsError::ChunkStillReferenced {
            chunk_id: chunk.0,
            refcount: 1
        }
    );
}

#[tokio::test]
async fn reclaim_chunk_with_zero_refcount_removes_chunk() {
    let fs = InMemoryAiosFs::new();
    let chunk = chunk_ref(b"dead");
    let (object_id, version_id) = write_one(&fs, "dead", chunk.clone()).await;
    fs.purge_version(&version_id, VersionPurgeReason::OperatorRequested)
        .expect("purge version");

    fs.reclaim_chunk(&chunk.0).expect("reclaim chunk");
    let err = fs
        .read_object(&object_id, None)
        .await
        .expect_err("reclaimed chunk must be unknown on read");

    assert_eq!(err, FsError::ChunkUnknown(chunk.0));
}

#[tokio::test]
async fn run_pass_reclaims_orphan_zero_ref_chunks() {
    let fs = InMemoryAiosFs::new();
    let chunk = chunk_ref(b"orphan");
    let (object_id, version_id) = write_one(&fs, "orphan", chunk.clone()).await;
    fs.purge_version(&version_id, VersionPurgeReason::OperatorRequested)
        .expect("purge version");

    let report = GcPassDriver::new_with_defaults()
        .run_pass(&fs)
        .await
        .expect("run gc pass");

    assert_eq!(report.chunks_reclaimed, 1);
    assert!(report.reasons.contains(&GcReason::OrphanChunkReclaimed {
        chunk_id: chunk.0.clone()
    }));
    let err = fs
        .read_object(&object_id, None)
        .await
        .expect_err("reclaimed chunk must be unknown on read");
    assert_eq!(err, FsError::ChunkUnknown(chunk.0));
}

#[tokio::test]
async fn run_pass_purges_retired_versions() {
    let fs = InMemoryAiosFs::new();
    let chunk = chunk_ref(b"retired");
    let (_, version_id) = write_one(&fs, "retired", chunk).await;
    fs.force_version_state_for_harness(&version_id, VersionState::RetiredVersion, None)
        .expect("retire version");

    let report = GcPassDriver::new_with_defaults()
        .run_pass(&fs)
        .await
        .expect("run gc pass");

    assert_eq!(report.versions_purged, 1);
    assert!(report.reasons.contains(&GcReason::VersionPurged {
        version_id: version_id.clone(),
        reason: VersionPurgeReason::Retired
    }));
    assert_eq!(
        fs.purge_version(&version_id, VersionPurgeReason::OperatorRequested),
        Err(FsError::VersionAlreadyPurged(version_id))
    );
}

#[tokio::test]
async fn run_pass_report_counts_reclaimed_chunks_and_purged_versions() {
    let fs = InMemoryAiosFs::new();
    let (left_object_id, left_version_id) = write_one(&fs, "left", chunk_ref(b"left")).await;
    let (_, right_version_id) = write_one(&fs, "right", chunk_ref(b"right")).await;
    fs.force_version_state_for_harness(&left_version_id, VersionState::RetiredVersion, None)
        .expect("retire left");
    fs.force_version_state_for_harness(&right_version_id, VersionState::RetiredVersion, None)
        .expect("retire right");

    let report = GcPassDriver::new_with_defaults()
        .run_pass(&fs)
        .await
        .expect("run gc pass");

    assert!(report.pass_id.starts_with("gcp_"));
    assert!(report.completed_at >= report.started_at);
    assert_eq!(report.versions_inspected, 2);
    assert_eq!(report.versions_purged, 2);
    assert_eq!(report.chunks_inspected, 2);
    assert_eq!(report.chunks_reclaimed, 2);
    assert_eq!(report.reasons.len(), 4);
    assert!(matches!(
        fs.read_object(&left_object_id, None).await,
        Err(FsError::ChunkUnknown(_))
    ));
}

#[tokio::test]
async fn run_pass_with_empty_fs_is_noop() {
    let fs = InMemoryAiosFs::new();

    let report = GcPassDriver::new_with_defaults()
        .run_pass(&fs)
        .await
        .expect("run gc pass");

    assert!(report.pass_id.starts_with("gcp_"));
    assert!(report.completed_at >= report.started_at);
    assert_eq!(report.chunks_inspected, 0);
    assert_eq!(report.chunks_reclaimed, 0);
    assert_eq!(report.versions_inspected, 0);
    assert_eq!(report.versions_purged, 0);
    assert!(report.reasons.is_empty());
}

#[tokio::test]
async fn quarantine_purged_disposition_becomes_gc_reclaimable_retired_version() {
    let fs = InMemoryAiosFs::new();
    let driver = QuarantineDriver::new(fs.clone());
    let first = fs
        .write_object(
            write_request("family:alice", "v1", vec![chunk_ref(b"v1")]),
            &context("family:alice"),
        )
        .await
        .expect("write first");
    let second = fs
        .write_object(
            append_request(
                first.object_id.clone(),
                first.version_id.clone(),
                "family:alice",
                "v2",
                vec![chunk_ref(b"v2")],
            ),
            &context("family:alice"),
        )
        .await
        .expect("write second");
    fs.force_pointer_for_harness(&first.object_id, PointerKind::Rollback, &first.version_id)
        .expect("rollback pointer");
    driver
        .enter(
            &second.version_id,
            QuarantineTrigger::OperatorManual,
            "operator hold",
            &fs,
        )
        .await
        .expect("enter quarantine");
    driver
        .exit(
            &second.version_id,
            QuarantineDisposition::Purged,
            &subject("_system:recovery:lucky"),
        )
        .await
        .expect("exit quarantine");

    let versions = fs
        .list_versions(&first.object_id)
        .await
        .expect("list versions");
    let retired = versions
        .iter()
        .find(|version| version.version_id == second.version_id)
        .expect("second version present");
    assert_eq!(retired.state, VersionState::RetiredVersion);

    let report = GcPassDriver::new_with_defaults()
        .run_pass(&fs)
        .await
        .expect("run gc pass");

    assert_eq!(report.versions_purged, 1);
    assert_eq!(report.chunks_reclaimed, 1);
}

#[tokio::test]
async fn shared_chunk_survives_until_last_referencing_version_is_purged() {
    let fs = InMemoryAiosFs::new();
    let shared = chunk_ref(b"shared-ref");
    let (_, first_version_id) = write_one(&fs, "first", shared.clone()).await;
    let (_, second_version_id) = write_one(&fs, "second", shared.clone()).await;
    let (third_object_id, third_version_id) = write_one(&fs, "third", shared).await;

    fs.purge_version(&first_version_id, VersionPurgeReason::OperatorRequested)
        .expect("purge first");
    fs.purge_version(&second_version_id, VersionPurgeReason::OperatorRequested)
        .expect("purge second");
    assert_eq!(current_chunk_refcount(&fs, &third_object_id).await, 1);
    let first_report = GcPassDriver::new_with_defaults()
        .run_pass(&fs)
        .await
        .expect("run gc pass");
    assert_eq!(first_report.chunks_reclaimed, 0);

    fs.purge_version(&third_version_id, VersionPurgeReason::OperatorRequested)
        .expect("purge third");
    let second_report = GcPassDriver::new_with_defaults()
        .run_pass(&fs)
        .await
        .expect("run gc pass");
    assert_eq!(second_report.chunks_reclaimed, 1);
}

#[tokio::test]
async fn purge_version_on_already_purged_version_returns_version_already_purged() {
    let fs = InMemoryAiosFs::new();
    let (_, version_id) = write_one(&fs, "once", chunk_ref(b"once")).await;
    fs.purge_version(&version_id, VersionPurgeReason::OperatorRequested)
        .expect("purge once");

    let err = fs
        .purge_version(&version_id, VersionPurgeReason::OperatorRequested)
        .expect_err("second purge must fail");

    assert_eq!(err, FsError::VersionAlreadyPurged(version_id));
}
