//! T-045 M5 golden-path walk for the composed AIOS-FS stack.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::too_many_lines,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use ed25519_dalek::SigningKey;

use aios_action::ActionId;
use aios_evidence::RecordType;
use aios_fs::{
    materialize_view, parse_query, AiosFs, AiosPath, ChunkId, ChunkRef, ConsistencyClass,
    FsContext, FsEvidenceEmitter, GcPassDriver, ImplSpace, ImplSpaceBinding, ImplSpaceSource,
    ImplSpaceTarget, InMemoryAiosFs, InMemoryFsEvidenceLog, InMemoryImplSpace, IntegrityState,
    MutableAiosFs, NamespaceClass, NamespacePolicy, ObjectId, ObjectWriteRequest, PointerKind,
    QuarantineDisposition, QuarantineDriver, QuarantineTrigger, SubjectRef, VersionId,
    VersionPurgeReason, VersionState,
};

struct M5Stack {
    fs: InMemoryAiosFs,
    evidence_log: Arc<InMemoryFsEvidenceLog>,
    quarantine_driver: QuarantineDriver<InMemoryAiosFs>,
    gc_driver: GcPassDriver,
    impl_space: InMemoryImplSpace,
}

fn subject(id: &str) -> SubjectRef {
    SubjectRef(id.to_owned())
}

fn context(id: &str) -> FsContext {
    FsContext {
        subject: subject(id),
        action_id: Some(ActionId::new()),
        expected_snapshot_id: None,
        consistency_class: ConsistencyClass::Snapshot,
    }
}

fn chunk_ref(bytes: &[u8]) -> ChunkRef {
    ChunkRef(ChunkId::from_hash_bytes(bytes))
}

fn write_request(name: &str, chunks: Vec<ChunkRef>) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: None,
        parent_version_ids: Vec::new(),
        chunks,
        metadata_delta: serde_json::json!({
            "name": name,
            "mime": "text/markdown",
            "kind": "FILE",
            "policy_tags": ["doc", "personal"],
            "scope": { "kind": "GROUP", "group_id": "family" }
        }),
        action_id: Some(ActionId::new()),
        subject: subject("family:alice"),
    }
}

fn append_request(
    object_id: ObjectId,
    parent_version_id: VersionId,
    name: &str,
    chunks: Vec<ChunkRef>,
) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: Some(object_id),
        parent_version_ids: vec![parent_version_id],
        chunks,
        metadata_delta: serde_json::json!({
            "name": name,
            "mime": "text/markdown",
            "policy_tags": ["doc", "personal"]
        }),
        action_id: Some(ActionId::new()),
        subject: subject("family:alice"),
    }
}

fn m5_stack() -> M5Stack {
    let evidence_log = Arc::new(InMemoryFsEvidenceLog::new());
    let evidence_emitter = Arc::new(FsEvidenceEmitter::new(
        evidence_log.clone(),
        SigningKey::from_bytes(&[45u8; 32]),
        subject("_system:service:aios-fs"),
    ));
    let fs = InMemoryAiosFs::with_evidence_emitter(evidence_emitter.clone());
    let quarantine_driver =
        QuarantineDriver::with_evidence_emitter(fs.clone(), evidence_emitter.clone());
    let gc_driver = GcPassDriver::with_evidence_emitter(1024, 1024, evidence_emitter);
    let impl_space = InMemoryImplSpace::new();

    M5Stack {
        fs,
        evidence_log,
        quarantine_driver,
        gc_driver,
        impl_space,
    }
}

async fn write_doc(stack: &M5Stack, name: &str, bytes: &[u8]) -> aios_fs::ObjectWriteResult {
    let path = AiosPath::new("/aios/groups/family/shared/docs/foo.md");
    NamespacePolicy::can_mutate(&path, &subject("family:alice"), false, false)
        .expect("S4.1 namespace policy admits human group-space write");

    let written = stack
        .fs
        .write_object(
            write_request(name, vec![chunk_ref(bytes)]),
            &context("family:alice"),
        )
        .await
        .expect("write object");

    stack
        .impl_space
        .record_binding(ImplSpaceBinding {
            binding_id: format!("ispb_{}", ulid::Ulid::new()),
            object_or_chunk_id: ImplSpaceSource::Object(written.object_id.clone()),
            target: ImplSpaceTarget::AiosFsManaged {
                handle: path.as_str().to_owned(),
            },
            created_at: chrono::Utc::now(),
            created_by: subject("family:alice"),
            last_verified_at: None,
            integrity_state: IntegrityState::Verified,
        })
        .await
        .expect("record implementation-space binding");

    written
}

#[tokio::test]
async fn phase_1_mount_aios_stub_serves_prefixed_paths() {
    let stack = m5_stack();
    let path = AiosPath::new("/aios/groups/family/shared/docs/foo.md");

    assert!(path.as_str().starts_with("/aios/"));
    assert_eq!(path.namespace_class(), Some(NamespaceClass::GroupShared));
    NamespacePolicy::can_mutate(&path, &subject("family:alice"), false, false)
        .expect("human subject can mutate group shared namespace");

    let written = write_doc(&stack, "foo.md", b"# hello\n").await;
    let read = stack
        .fs
        .read_object(&written.object_id, None)
        .await
        .expect("read object through AiosFs after mount stub");
    assert_eq!(read.version.version_id, written.version_id);
}

#[tokio::test]
async fn phase_2_create_versioned_object_returns_ids_and_snapshot() {
    let stack = m5_stack();

    let written = write_doc(&stack, "foo.md", b"# versioned\n").await;

    assert!(written.object_id.as_str().starts_with("obj_"));
    assert!(written.version_id.as_str().starts_with("ver_"));
    assert!(written.snapshot_id_after.as_str().starts_with("snap_"));
    assert!(!stack
        .impl_space
        .resolve(&ImplSpaceSource::Object(written.object_id.clone()))
        .await
        .expect("resolve impl-space binding")
        .is_empty());
}

#[tokio::test]
async fn phase_3_resolve_through_semantic_view() {
    let stack = m5_stack();
    let written = write_doc(&stack, "foo.md", b"# docs\n").await;
    write_doc(&stack, "other.md", b"# other\n").await;
    let query = parse_query("object.policy_tags contains \"doc\"").expect("query parses");

    let view = materialize_view(&query, &stack.fs, None)
        .await
        .expect("materialize view");
    let matched: Vec<_> = view
        .matched
        .iter()
        .map(|obj| obj.object_id.clone())
        .collect();

    assert!(matched.contains(&written.object_id));
    assert_eq!(matched.len(), 2);
}

#[tokio::test]
async fn phase_4_evidence_chain_links_action_received_receipts() {
    let stack = m5_stack();

    write_doc(&stack, "left.md", b"left").await;
    write_doc(&stack, "right.md", b"right").await;

    let receipts = stack.evidence_log.receipts().await;
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0].record_type(), RecordType::ActionReceived);
    assert_eq!(receipts[1].record_type(), RecordType::ActionReceived);
    assert_eq!(
        receipts[1].previous_receipt_hash(),
        Some(receipts[0].link_hash().expect("first link hash").as_str())
    );
    stack
        .evidence_log
        .verify_integrity()
        .await
        .expect("BLAKE3 receipt chain verifies");
}

#[tokio::test]
async fn phase_5_quarantine_round_trip_release_restores_stable_version() {
    let stack = m5_stack();
    let first = write_doc(&stack, "stable.md", b"stable").await;
    let second = stack
        .fs
        .write_object(
            append_request(
                first.object_id.clone(),
                first.version_id.clone(),
                "target.md",
                vec![chunk_ref(b"target")],
            ),
            &context("family:alice"),
        )
        .await
        .expect("write target version");
    stack
        .fs
        .force_pointer_for_harness(&first.object_id, PointerKind::Rollback, &first.version_id)
        .expect("rollback pointer");
    let quarantine_pointer_id = stack
        .fs
        .force_pointer_for_harness(
            &first.object_id,
            PointerKind::Quarantine,
            &second.version_id,
        )
        .expect("quarantine pointer");

    stack
        .quarantine_driver
        .enter(
            &second.version_id,
            QuarantineTrigger::IntegrityFailure,
            "chunk hash mismatch",
            &stack.fs,
        )
        .await
        .expect("enter quarantine");
    stack
        .fs
        .force_object_current_pointer_for_harness(&first.object_id, &quarantine_pointer_id)
        .expect("force current to quarantined version");

    let err = stack
        .fs
        .read_object(&first.object_id, None)
        .await
        .expect_err("non-recovery read must fail while quarantined");
    assert!(matches!(err, aios_fs::FsError::QuarantineViolation(_)));

    stack
        .quarantine_driver
        .exit(
            &second.version_id,
            QuarantineDisposition::Released,
            &subject("_system:recovery:lucky"),
        )
        .await
        .expect("release quarantine");
    let read = stack
        .fs
        .read_object(&first.object_id, None)
        .await
        .expect("released version reads again");
    assert_eq!(read.version.version_id, second.version_id);
    assert_eq!(read.version.state, VersionState::Verified);

    let receipts = stack.evidence_log.receipts().await;
    assert!(receipts
        .iter()
        .any(|receipt| receipt.record_type() == RecordType::QuarantineEvent));
}

#[tokio::test]
async fn phase_6_gc_shared_chunk_survives_until_last_reference_then_reclaims() {
    let stack = m5_stack();
    let shared = chunk_ref(b"shared-doc-chunk");
    let first = stack
        .fs
        .write_object(
            write_request("first.md", vec![shared.clone()]),
            &context("family:alice"),
        )
        .await
        .expect("write first");
    let second = stack
        .fs
        .write_object(
            append_request(
                first.object_id.clone(),
                first.version_id.clone(),
                "second.md",
                vec![shared.clone()],
            ),
            &context("family:alice"),
        )
        .await
        .expect("write second");

    assert_eq!(
        stack
            .fs
            .read_object(&first.object_id, None)
            .await
            .expect("read current")
            .chunks[0]
            .ref_count,
        2
    );

    stack
        .fs
        .purge_version(&first.version_id, VersionPurgeReason::OperatorRequested)
        .expect("purge older version");
    let no_reclaim = stack
        .gc_driver
        .run_pass(&stack.fs)
        .await
        .expect("run first gc pass");
    assert_eq!(no_reclaim.chunks_reclaimed, 0);
    assert_eq!(
        stack
            .fs
            .read_object(&first.object_id, None)
            .await
            .expect("read current after first purge")
            .chunks[0]
            .ref_count,
        1
    );

    stack
        .fs
        .purge_version(&second.version_id, VersionPurgeReason::OperatorRequested)
        .expect("purge newer version");
    let reclaimed = stack
        .gc_driver
        .run_pass(&stack.fs)
        .await
        .expect("run second gc pass");
    assert_eq!(reclaimed.chunks_reclaimed, 1);
    assert!(stack
        .evidence_log
        .receipts()
        .await
        .iter()
        .any(|receipt| receipt.record_type() == RecordType::GcPass));
}
