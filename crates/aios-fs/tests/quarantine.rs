//! T-038 integration tests for S1.3 §12 quarantine entry/exit semantics.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use strum::IntoEnumIterator;

use aios_fs::{
    AiosFs, ChunkId, ChunkRef, ConsistencyClass, FsContext, FsError, InMemoryAiosFs,
    ObjectWriteRequest, PointerId, PointerKind, QuarantineDisposition, QuarantineDriver,
    QuarantineTrigger, SubjectRef, VersionId, VersionState,
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

fn new_write_request(id: &str, name: &str) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: None,
        parent_version_ids: Vec::new(),
        chunks: vec![chunk_ref(name.as_bytes())],
        metadata_delta: serde_json::json!({
            "name": name,
            "mime": "text/plain"
        }),
        action_id: None,
        subject: subject(id),
    }
}

fn append_write_request(
    object_id: aios_fs::ObjectId,
    parent_version_id: VersionId,
    id: &str,
    name: &str,
) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: Some(object_id),
        parent_version_ids: vec![parent_version_id],
        chunks: vec![chunk_ref(name.as_bytes())],
        metadata_delta: serde_json::json!({
            "name": name,
            "mime": "text/plain"
        }),
        action_id: None,
        subject: subject(id),
    }
}

async fn write_chain(
    fs: &InMemoryAiosFs,
    subject_id: &str,
) -> (
    aios_fs::ObjectId,
    VersionId,
    VersionId,
    VersionId,
    PointerId,
) {
    let first = fs
        .write_object(new_write_request(subject_id, "v1"), &context(subject_id))
        .await
        .expect("write v1");
    let current_pointer_id = fs
        .read_object(&first.object_id, None)
        .await
        .expect("read current pointer")
        .object
        .current_pointer_id;
    let second = fs
        .write_object(
            append_write_request(
                first.object_id.clone(),
                first.version_id.clone(),
                subject_id,
                "v2",
            ),
            &context(subject_id),
        )
        .await
        .expect("write v2");
    let third = fs
        .write_object(
            append_write_request(
                first.object_id.clone(),
                second.version_id.clone(),
                subject_id,
                "v3",
            ),
            &context(subject_id),
        )
        .await
        .expect("write v3");

    (
        first.object_id,
        first.version_id,
        second.version_id,
        third.version_id,
        current_pointer_id,
    )
}

#[tokio::test]
async fn enter_validation_failure_quarantines_version_and_populates_fields() {
    let fs = InMemoryAiosFs::new();
    let driver = QuarantineDriver::new(fs.clone());
    let (object_id, stable_version_id, target_version_id, _, _) =
        write_chain(&fs, "family:alice").await;
    fs.force_pointer_for_harness(&object_id, PointerKind::Rollback, &stable_version_id)
        .expect("rollback pointer");

    driver
        .enter(
            &target_version_id,
            QuarantineTrigger::ValidationFailure,
            "schema validation failed",
            &fs,
        )
        .await
        .expect("enter quarantine");

    let versions = fs.list_versions(&object_id).await.expect("list versions");
    let quarantined = versions
        .iter()
        .find(|version| version.version_id == target_version_id)
        .expect("target version present");

    assert_eq!(quarantined.state, VersionState::Quarantined);
    assert!(quarantined.quarantined_at.is_some());
    assert_eq!(
        quarantined.quarantine_reason.as_deref(),
        Some("schema validation failed")
    );
}

#[tokio::test]
async fn enter_emits_receipt_with_trigger() {
    let fs = InMemoryAiosFs::new();
    let driver = QuarantineDriver::new(fs.clone());
    let (object_id, stable_version_id, target_version_id, _, _) =
        write_chain(&fs, "family:alice").await;
    fs.force_pointer_for_harness(&object_id, PointerKind::Rollback, &stable_version_id)
        .expect("rollback pointer");

    let receipt = driver
        .enter(
            &target_version_id,
            QuarantineTrigger::IntegrityFailure,
            "chunk hash mismatch",
            &fs,
        )
        .await
        .expect("enter quarantine");

    assert!(receipt.quarantine_id.starts_with("qnt_"));
    assert_eq!(receipt.version_id, target_version_id);
    assert_eq!(receipt.trigger, Some(QuarantineTrigger::IntegrityFailure));
    assert_eq!(receipt.disposition, None);
    assert_eq!(receipt.reason, "chunk hash mismatch");
}

#[tokio::test]
async fn enter_with_rollback_pointer_moves_stable_and_current_to_rollback_target() {
    let fs = InMemoryAiosFs::new();
    let driver = QuarantineDriver::new(fs.clone());
    let (object_id, rollback_target, _, target_version_id, current_pointer_id) =
        write_chain(&fs, "family:alice").await;
    let stable_pointer_id = fs
        .force_pointer_for_harness(&object_id, PointerKind::Stable, &target_version_id)
        .expect("stable pointer");
    fs.force_pointer_for_harness(&object_id, PointerKind::Rollback, &rollback_target)
        .expect("rollback pointer");

    driver
        .enter(
            &target_version_id,
            QuarantineTrigger::PolicyViolation,
            "classifier upgraded privacy class",
            &fs,
        )
        .await
        .expect("enter quarantine");

    let stable_pointer = fs
        .resolve_pointer(&stable_pointer_id)
        .await
        .expect("resolve stable pointer");
    let current_pointer = fs
        .resolve_pointer(&current_pointer_id)
        .await
        .expect("resolve current pointer");

    assert_eq!(stable_pointer.current_version_id, rollback_target);
    assert_eq!(current_pointer.current_version_id, rollback_target);
}

#[tokio::test]
async fn enter_without_rollback_pointer_moves_stable_to_prior_stable_parent() {
    let fs = InMemoryAiosFs::new();
    let driver = QuarantineDriver::new(fs.clone());
    let (object_id, prior_stable_version_id, target_version_id, _, _) =
        write_chain(&fs, "family:alice").await;
    let stable_pointer_id = fs
        .force_pointer_for_harness(&object_id, PointerKind::Stable, &target_version_id)
        .expect("stable pointer");

    driver
        .enter(
            &target_version_id,
            QuarantineTrigger::ValidationFailure,
            "signature mismatch",
            &fs,
        )
        .await
        .expect("enter quarantine");

    let stable_pointer = fs
        .resolve_pointer(&stable_pointer_id)
        .await
        .expect("resolve stable pointer");

    assert_eq!(stable_pointer.current_version_id, prior_stable_version_id);
}

#[tokio::test]
async fn enter_without_prior_stable_returns_no_prior_stable_pointer() {
    let fs = InMemoryAiosFs::new();
    let driver = QuarantineDriver::new(fs.clone());
    let written = fs
        .write_object(
            new_write_request("family:alice", "v1"),
            &context("family:alice"),
        )
        .await
        .expect("write v1");

    let err = driver
        .enter(
            &written.version_id,
            QuarantineTrigger::ValidationFailure,
            "first version cannot fall back",
            &fs,
        )
        .await
        .expect_err("no prior stable pointer");

    assert_eq!(err, FsError::NoPriorStablePointer(written.object_id));
}

#[tokio::test]
async fn enter_on_already_quarantined_version_returns_quarantine_already_applied() {
    let fs = InMemoryAiosFs::new();
    let driver = QuarantineDriver::new(fs.clone());
    let (object_id, stable_version_id, target_version_id, _, _) =
        write_chain(&fs, "family:alice").await;
    fs.force_pointer_for_harness(&object_id, PointerKind::Rollback, &stable_version_id)
        .expect("rollback pointer");

    driver
        .enter(
            &target_version_id,
            QuarantineTrigger::OperatorManual,
            "manual hold",
            &fs,
        )
        .await
        .expect("first enter");
    let err = driver
        .enter(
            &target_version_id,
            QuarantineTrigger::OperatorManual,
            "manual hold again",
            &fs,
        )
        .await
        .expect_err("already quarantined");

    assert_eq!(err, FsError::QuarantineAlreadyApplied(target_version_id));
}

#[tokio::test]
async fn exit_released_returns_version_to_verified_and_clears_reason() {
    let fs = InMemoryAiosFs::new();
    let driver = QuarantineDriver::new(fs.clone());
    let (object_id, stable_version_id, target_version_id, _, _) =
        write_chain(&fs, "family:alice").await;
    fs.force_pointer_for_harness(&object_id, PointerKind::Rollback, &stable_version_id)
        .expect("rollback pointer");
    driver
        .enter(
            &target_version_id,
            QuarantineTrigger::OperatorManual,
            "manual hold",
            &fs,
        )
        .await
        .expect("enter quarantine");

    let receipt = driver
        .exit(
            &target_version_id,
            QuarantineDisposition::Released,
            &subject("_system:recovery:lucky"),
        )
        .await
        .expect("exit quarantine");
    let versions = fs.list_versions(&object_id).await.expect("list versions");
    let released = versions
        .iter()
        .find(|version| version.version_id == target_version_id)
        .expect("target version present");

    assert_eq!(receipt.disposition, Some(QuarantineDisposition::Released));
    assert_eq!(released.state, VersionState::Verified);
    assert_eq!(released.quarantine_reason, None);
    assert_eq!(released.quarantined_at, None);
}

#[tokio::test]
async fn exit_purged_retires_version_because_version_state_has_no_purged_variant() {
    let fs = InMemoryAiosFs::new();
    let driver = QuarantineDriver::new(fs.clone());
    let (object_id, stable_version_id, target_version_id, _, _) =
        write_chain(&fs, "family:alice").await;
    fs.force_pointer_for_harness(&object_id, PointerKind::Rollback, &stable_version_id)
        .expect("rollback pointer");
    driver
        .enter(
            &target_version_id,
            QuarantineTrigger::OperatorManual,
            "manual hold",
            &fs,
        )
        .await
        .expect("enter quarantine");

    let receipt = driver
        .exit(
            &target_version_id,
            QuarantineDisposition::Purged,
            &subject("_system:recovery:lucky"),
        )
        .await
        .expect("exit quarantine");
    let versions = fs.list_versions(&object_id).await.expect("list versions");
    let purged = versions
        .iter()
        .find(|version| version.version_id == target_version_id)
        .expect("target version present");

    assert_eq!(receipt.disposition, Some(QuarantineDisposition::Purged));
    assert_eq!(purged.state, VersionState::RetiredVersion);
    assert_eq!(purged.quarantine_reason, None);
}

#[tokio::test]
async fn exit_on_non_quarantined_version_returns_quarantine_not_applied() {
    let fs = InMemoryAiosFs::new();
    let driver = QuarantineDriver::new(fs.clone());
    let written = fs
        .write_object(
            new_write_request("family:alice", "v1"),
            &context("family:alice"),
        )
        .await
        .expect("write v1");

    let err = driver
        .exit(
            &written.version_id,
            QuarantineDisposition::Released,
            &subject("_system:recovery:lucky"),
        )
        .await
        .expect_err("not quarantined");

    assert_eq!(err, FsError::QuarantineNotApplied(written.version_id));
}

#[tokio::test]
async fn read_denial_gate_still_denies_quarantined_version_after_driver_entry() {
    let fs = InMemoryAiosFs::new();
    let driver = QuarantineDriver::new(fs.clone());
    let (object_id, stable_version_id, target_version_id, _, _) =
        write_chain(&fs, "family:alice").await;
    fs.force_pointer_for_harness(&object_id, PointerKind::Rollback, &stable_version_id)
        .expect("rollback pointer");
    let quarantine_pointer_id = fs
        .force_pointer_for_harness(&object_id, PointerKind::Quarantine, &target_version_id)
        .expect("quarantine pointer");

    driver
        .enter(
            &target_version_id,
            QuarantineTrigger::AttestationFailure,
            "merge proposal rejected",
            &fs,
        )
        .await
        .expect("enter quarantine");
    fs.force_object_current_pointer_for_harness(&object_id, &quarantine_pointer_id)
        .expect("current pointer fixture");

    let err = fs
        .read_object(&object_id, None)
        .await
        .expect_err("non-recovery read must fail");

    assert!(matches!(err, FsError::QuarantineViolation(_)));
}

#[tokio::test]
async fn recovery_created_quarantined_version_remains_readable_with_reason() {
    let fs = InMemoryAiosFs::new();
    let driver = QuarantineDriver::new(fs.clone());
    let (object_id, stable_version_id, target_version_id, _, _) =
        write_chain(&fs, "agent:recovery:forensics").await;
    fs.force_pointer_for_harness(&object_id, PointerKind::Rollback, &stable_version_id)
        .expect("rollback pointer");
    let quarantine_pointer_id = fs
        .force_pointer_for_harness(&object_id, PointerKind::Quarantine, &target_version_id)
        .expect("quarantine pointer");

    driver
        .enter(
            &target_version_id,
            QuarantineTrigger::PolicyViolation,
            "classifier upgraded privacy class",
            &fs,
        )
        .await
        .expect("enter quarantine");
    fs.force_object_current_pointer_for_harness(&object_id, &quarantine_pointer_id)
        .expect("current pointer fixture");

    let read = fs
        .read_object(&object_id, None)
        .await
        .expect("recovery read may inspect quarantined version");

    assert_eq!(read.version.version_id, target_version_id);
    assert_eq!(read.version.state, VersionState::Quarantined);
    assert_eq!(
        read.version.quarantine_reason.as_deref(),
        Some("classifier upgraded privacy class")
    );
}

#[test]
fn quarantine_trigger_has_at_least_five_spec_variants() {
    assert!(QuarantineTrigger::iter().count() >= 5);
    assert!(QuarantineTrigger::iter().any(|v| v == QuarantineTrigger::ValidationFailure));
    assert!(QuarantineTrigger::iter().any(|v| v == QuarantineTrigger::IntegrityFailure));
    assert!(QuarantineTrigger::iter().any(|v| v == QuarantineTrigger::PolicyViolation));
    assert!(QuarantineTrigger::iter().any(|v| v == QuarantineTrigger::AttestationFailure));
    assert!(QuarantineTrigger::iter().any(|v| v == QuarantineTrigger::OperatorManual));
}
