//! T-037 integration tests for the `AiosFs` trait, `InMemoryAiosFs`,
//! `SnapshotId`, and the S1.3 §11/§12 read gates.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_fs::{
    AiosFs, ChunkId, ChunkRef, ConsistencyClass, FsContext, FsError, InMemoryAiosFs,
    ObjectWriteRequest, PointerId, SnapshotId, SubjectRef, VersionState,
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

fn new_write_request(id: &str) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: None,
        parent_version_ids: Vec::new(),
        chunks: vec![chunk_ref(b"hello"), chunk_ref(b"world")],
        metadata_delta: serde_json::json!({
            "name": "fixture object",
            "labels": ["t037", "fs"],
            "mime": "text/plain"
        }),
        action_id: None,
        subject: subject(id),
    }
}

#[tokio::test]
async fn read_object_happy_path_returns_object_version_chunks_and_snapshot() {
    let fs = InMemoryAiosFs::new();
    let written = fs
        .write_object(new_write_request("family:alice"), &context("family:alice"))
        .await
        .expect("write object");

    let read = fs
        .read_object(&written.object_id, None)
        .await
        .expect("read object");

    assert_eq!(read.object.object_id, written.object_id);
    assert_eq!(read.version.version_id, written.version_id);
    assert_eq!(read.chunks.len(), 2);
    assert_eq!(read.snapshot_id, written.snapshot_id_after);
}

#[tokio::test]
async fn read_object_with_stale_snapshot_id_returns_snapshot_stale() {
    let fs = InMemoryAiosFs::new();
    let stale = fs.snapshot().snapshot_id;
    let written = fs
        .write_object(new_write_request("family:alice"), &context("family:alice"))
        .await
        .expect("write object");

    let err = fs
        .read_object(&written.object_id, Some(&stale))
        .await
        .expect_err("stale snapshot must fail");

    match err {
        FsError::SnapshotStale { expected, found } => {
            assert_eq!(expected, written.snapshot_id_after);
            assert_eq!(found, stale);
        }
        other => panic!("expected SnapshotStale, got {other:?}"),
    }
}

#[tokio::test]
async fn read_object_with_current_snapshot_id_succeeds() {
    let fs = InMemoryAiosFs::new();
    let written = fs
        .write_object(new_write_request("family:alice"), &context("family:alice"))
        .await
        .expect("write object");

    let read = fs
        .read_object(&written.object_id, Some(&written.snapshot_id_after))
        .await
        .expect("current snapshot must read");

    assert_eq!(read.snapshot_id, written.snapshot_id_after);
}

#[tokio::test]
async fn read_object_denies_quarantined_version_for_non_recovery_subject() {
    let fs = InMemoryAiosFs::new();
    let written = fs
        .write_object(new_write_request("family:alice"), &context("family:alice"))
        .await
        .expect("write object");
    fs.force_version_state_for_harness(
        &written.version_id,
        VersionState::Quarantined,
        Some("chunk_integrity_failure".to_owned()),
    )
    .expect("force quarantine fixture");

    let err = fs
        .read_object(&written.object_id, None)
        .await
        .expect_err("non-recovery read must fail");

    assert!(matches!(err, FsError::QuarantineViolation(_)));
    assert!(err.to_string().contains("read of quarantined version"));
}

#[tokio::test]
async fn read_object_allows_quarantined_version_for_recovery_subject() {
    let fs = InMemoryAiosFs::new();
    let written = fs
        .write_object(
            new_write_request("agent:recovery:forensics"),
            &context("agent:recovery:forensics"),
        )
        .await
        .expect("write object");
    fs.force_version_state_for_harness(
        &written.version_id,
        VersionState::Quarantined,
        Some("manual_recovery_probe".to_owned()),
    )
    .expect("force quarantine fixture");

    let read = fs
        .read_object(&written.object_id, None)
        .await
        .expect("recovery subject may read quarantined version");

    assert_eq!(read.version.version_id, written.version_id);
    assert_eq!(read.version.state, VersionState::Quarantined);
}

#[tokio::test]
async fn write_object_with_no_object_id_creates_new_object() {
    let fs = InMemoryAiosFs::new();

    let written = fs
        .write_object(new_write_request("family:alice"), &context("family:alice"))
        .await
        .expect("write object");

    assert!(written.object_id.as_str().starts_with("obj_"));
    assert!(written.version_id.as_str().starts_with("ver_"));
    assert!(written.transaction_id.as_str().starts_with("txn_"));
    assert!(written.snapshot_id_after.as_str().starts_with("snap_"));
}

#[tokio::test]
async fn write_object_existing_object_with_valid_parent_creates_chained_version() {
    let fs = InMemoryAiosFs::new();
    let first = fs
        .write_object(new_write_request("family:alice"), &context("family:alice"))
        .await
        .expect("first write");

    let second = fs
        .write_object(
            ObjectWriteRequest {
                object_id: Some(first.object_id.clone()),
                parent_version_ids: vec![first.version_id.clone()],
                chunks: vec![chunk_ref(b"second")],
                metadata_delta: serde_json::json!({ "name": "fixture object v2" }),
                action_id: None,
                subject: subject("family:alice"),
            },
            &context("family:alice"),
        )
        .await
        .expect("second write");

    assert_eq!(second.object_id, first.object_id);
    assert_ne!(second.version_id, first.version_id);

    let versions = fs
        .list_versions(&first.object_id)
        .await
        .expect("list versions");
    let chained = versions
        .iter()
        .find(|version| version.version_id == second.version_id)
        .expect("second version present");
    assert_eq!(chained.parent_version_ids, vec![first.version_id]);
}

#[tokio::test]
async fn write_object_existing_object_without_parent_returns_write_requires_parent() {
    let fs = InMemoryAiosFs::new();
    let first = fs
        .write_object(new_write_request("family:alice"), &context("family:alice"))
        .await
        .expect("first write");

    let err = fs
        .write_object(
            ObjectWriteRequest {
                object_id: Some(first.object_id),
                parent_version_ids: Vec::new(),
                chunks: vec![chunk_ref(b"second")],
                metadata_delta: serde_json::json!({ "name": "fixture object v2" }),
                action_id: None,
                subject: subject("family:alice"),
            },
            &context("family:alice"),
        )
        .await
        .expect_err("existing object writes require parent ids");

    assert_eq!(err, FsError::WriteRequiresParent);
}

#[tokio::test]
async fn list_versions_returns_all_versions_for_object() {
    let fs = InMemoryAiosFs::new();
    let first = fs
        .write_object(new_write_request("family:alice"), &context("family:alice"))
        .await
        .expect("first write");
    let second = fs
        .write_object(
            ObjectWriteRequest {
                object_id: Some(first.object_id.clone()),
                parent_version_ids: vec![first.version_id.clone()],
                chunks: vec![chunk_ref(b"second")],
                metadata_delta: serde_json::json!({}),
                action_id: None,
                subject: subject("family:alice"),
            },
            &context("family:alice"),
        )
        .await
        .expect("second write");

    let versions = fs
        .list_versions(&first.object_id)
        .await
        .expect("list versions");

    assert_eq!(versions.len(), 2);
    assert!(versions.iter().any(|v| v.version_id == first.version_id));
    assert!(versions.iter().any(|v| v.version_id == second.version_id));
}

#[tokio::test]
async fn resolve_pointer_returns_pointer_or_pointer_not_found() {
    let fs = InMemoryAiosFs::new();
    let written = fs
        .write_object(new_write_request("family:alice"), &context("family:alice"))
        .await
        .expect("write object");
    let read = fs
        .read_object(&written.object_id, None)
        .await
        .expect("read object");

    let pointer = fs
        .resolve_pointer(&read.object.current_pointer_id)
        .await
        .expect("resolve pointer");
    assert_eq!(pointer.current_version_id, written.version_id);

    let stranger = PointerId::new();
    let err = fs
        .resolve_pointer(&stranger)
        .await
        .expect_err("unknown pointer must fail");
    assert_eq!(err, FsError::PointerNotFound(stranger));
}

#[tokio::test]
async fn get_snapshot_returns_matching_object_and_pointer_counts() {
    let fs = InMemoryAiosFs::new();
    let written = fs
        .write_object(new_write_request("family:alice"), &context("family:alice"))
        .await
        .expect("write object");

    let summary = fs
        .get_snapshot(&written.snapshot_id_after)
        .await
        .expect("snapshot summary");

    assert_eq!(summary.snapshot_id, written.snapshot_id_after);
    assert_eq!(summary.object_count, 1);
    assert_eq!(summary.pointer_count, 1);
}

#[test]
fn snapshot_id_compute_is_deterministic_for_same_inputs() {
    let left = SnapshotId::compute(["obj_a"], ["ptr_a:ver_a"], ["ver_a"]);
    let right = SnapshotId::compute(["obj_a"], ["ptr_a:ver_a"], ["ver_a"]);

    assert_eq!(left, right);
    assert_eq!(left.as_str().len(), "snap_".len() + 32);
}

#[test]
fn snapshot_id_compute_is_content_addressed_for_different_inputs() {
    let left = SnapshotId::compute(["obj_a"], ["ptr_a:ver_a"], ["ver_a"]);
    let right = SnapshotId::compute(["obj_a"], ["ptr_a:ver_b"], ["ver_a"]);

    assert_ne!(left, right);
}

#[tokio::test]
async fn write_then_read_with_returned_snapshot_id_succeeds_end_to_end() {
    let fs = InMemoryAiosFs::new();
    let written = fs
        .write_object(new_write_request("family:alice"), &context("family:alice"))
        .await
        .expect("write object");

    let read = fs
        .read_object(&written.object_id, Some(&written.snapshot_id_after))
        .await
        .expect("returned snapshot id must remain current");

    assert_eq!(read.object.object_id, written.object_id);
    assert_eq!(read.version.version_id, written.version_id);
}
