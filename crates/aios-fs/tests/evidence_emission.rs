//! T-044 integration tests for S1.3 -> S3.1 evidence emission.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use ed25519_dalek::SigningKey;

use aios_action::ActionId;
use aios_evidence::{EvidenceError, EvidenceReceipt, ReceiptBuilder, RecordType};
use aios_fs::{
    record_conflict_event, ActionReceivedPayload, AiosFs, ChunkId, ChunkRef, ConflictEventPayload,
    ConflictResolutionKind, ConsistencyClass, FsContext, FsError, FsEvidenceEmitter, FsEvidenceLog,
    GcPassDriver, GcPassPayload, InMemoryAiosFs, InMemoryFsEvidenceLog, ObjectId,
    ObjectWriteRequest, QuarantineDisposition, QuarantineDriver, QuarantineEventPayload,
    QuarantineTrigger, SubjectRef, VersionId, VersionState,
};

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[44u8; 32])
}

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

fn evidence_fixture() -> (Arc<InMemoryFsEvidenceLog>, Arc<FsEvidenceEmitter>) {
    let log = Arc::new(InMemoryFsEvidenceLog::new());
    let emitter = Arc::new(FsEvidenceEmitter::new(
        log.clone(),
        signing_key(),
        subject("_system:service:aios-fs"),
    ));
    (log, emitter)
}

fn payload_as<T>(receipt: &EvidenceReceipt) -> T
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(receipt.payload().clone()).expect("payload must decode")
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

#[test]
fn action_received_payload_round_trips_through_serde_json() {
    let payload = ActionReceivedPayload {
        object_id: ObjectId::new(),
        version_id: VersionId::new(),
        transaction_id: aios_fs::TransactionId::new(),
        subject: subject("family:alice"),
        action_id: Some(ActionId::new()),
        chunks_count: 2,
        content_hash: blake3::hash(b"content").to_hex().to_string(),
    };

    let json = serde_json::to_string(&payload).expect("serialise");
    let back: ActionReceivedPayload = serde_json::from_str(&json).expect("deserialise");

    assert_eq!(back, payload);
}

#[test]
fn quarantine_event_payload_round_trips_through_serde_json() {
    let payload = QuarantineEventPayload {
        version_id: VersionId::new(),
        trigger: Some(QuarantineTrigger::IntegrityFailure),
        disposition: None,
        reason: "chunk hash mismatch".to_owned(),
        transitioned_at: Utc::now(),
    };

    let json = serde_json::to_string(&payload).expect("serialise");
    let back: QuarantineEventPayload = serde_json::from_str(&json).expect("deserialise");

    assert_eq!(back, payload);
}

#[test]
fn conflict_event_payload_round_trips_through_serde_json() {
    let payload = ConflictEventPayload {
        object_id: ObjectId::new(),
        conflict_summary: "candidate lost CAS".to_owned(),
        resolution_kind: ConflictResolutionKind::Resolve,
        occurred_at: Utc::now(),
    };

    let json = serde_json::to_string(&payload).expect("serialise");
    let back: ConflictEventPayload = serde_json::from_str(&json).expect("deserialise");

    assert_eq!(back, payload);
}

#[test]
fn gc_pass_payload_round_trips_through_serde_json() {
    let now = Utc::now();
    let payload = GcPassPayload {
        pass_id: "gcp_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        chunks_inspected: 3,
        chunks_reclaimed: 2,
        versions_inspected: 5,
        versions_purged: 4,
        started_at: now,
        completed_at: now,
    };

    let json = serde_json::to_string(&payload).expect("serialise");
    let back: GcPassPayload = serde_json::from_str(&json).expect("deserialise");

    assert_eq!(back, payload);
}

#[tokio::test]
async fn write_object_emits_action_received_receipt_with_correct_payload() {
    let (log, emitter) = evidence_fixture();
    let fs = InMemoryAiosFs::with_evidence_emitter(emitter);
    let action_id = ActionId::new();
    let mut request = write_request("family:alice", "v1", vec![chunk_ref(b"v1")]);
    request.action_id = Some(action_id.clone());

    let written = fs
        .write_object(request.clone(), &context("family:alice"))
        .await
        .expect("write object");
    let receipts = log.receipts().await;

    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].record_type(), RecordType::ActionReceived);
    assert_eq!(receipts[0].action_id(), Some(&action_id));

    let payload: ActionReceivedPayload = payload_as(&receipts[0]);
    assert_eq!(payload.object_id, written.object_id);
    assert_eq!(payload.version_id, written.version_id);
    assert_eq!(payload.transaction_id, written.transaction_id);
    assert_eq!(payload.subject, request.subject);
    assert_eq!(payload.action_id, Some(action_id));
    assert_eq!(payload.chunks_count, 1);
}

#[tokio::test]
async fn action_received_payload_contains_object_id_and_version_id_from_write_result() {
    let (log, emitter) = evidence_fixture();
    let fs = InMemoryAiosFs::with_evidence_emitter(emitter);

    let written = fs
        .write_object(
            write_request("family:alice", "v1", vec![chunk_ref(b"v1")]),
            &context("family:alice"),
        )
        .await
        .expect("write object");
    let receipts = log.receipts().await;
    let payload: ActionReceivedPayload = payload_as(&receipts[0]);

    assert_eq!(payload.object_id, written.object_id);
    assert_eq!(payload.version_id, written.version_id);
}

#[tokio::test]
async fn quarantine_enter_emits_quarantine_event_with_trigger() {
    let (log, emitter) = evidence_fixture();
    let fs = InMemoryAiosFs::new();
    let driver = QuarantineDriver::with_evidence_emitter(fs.clone(), emitter);
    let (object_id, stable_version_id, target_version_id) = write_quarantine_chain(&fs).await;
    fs.force_pointer_for_harness(
        &object_id,
        aios_fs::PointerKind::Rollback,
        &stable_version_id,
    )
    .expect("rollback pointer");

    driver
        .enter(
            &target_version_id,
            QuarantineTrigger::IntegrityFailure,
            "chunk hash mismatch",
            &fs,
        )
        .await
        .expect("enter quarantine");
    let receipts = log.receipts().await;

    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].record_type(), RecordType::QuarantineEvent);
    let payload: QuarantineEventPayload = payload_as(&receipts[0]);
    assert_eq!(payload.version_id, target_version_id);
    assert_eq!(payload.trigger, Some(QuarantineTrigger::IntegrityFailure));
    assert_eq!(payload.disposition, None);
    assert_eq!(payload.reason, "chunk hash mismatch");
}

#[tokio::test]
async fn quarantine_exit_emits_quarantine_event_with_disposition() {
    let (log, emitter) = evidence_fixture();
    let fs = InMemoryAiosFs::new();
    let driver = QuarantineDriver::with_evidence_emitter(fs.clone(), emitter);
    let (object_id, stable_version_id, target_version_id) = write_quarantine_chain(&fs).await;
    fs.force_pointer_for_harness(
        &object_id,
        aios_fs::PointerKind::Rollback,
        &stable_version_id,
    )
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

    driver
        .exit(
            &target_version_id,
            QuarantineDisposition::Released,
            &subject("_system:recovery:lucky"),
        )
        .await
        .expect("exit quarantine");
    let receipts = log.receipts().await;

    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[1].record_type(), RecordType::QuarantineEvent);
    let payload: QuarantineEventPayload = payload_as(&receipts[1]);
    assert_eq!(payload.version_id, target_version_id);
    assert_eq!(payload.trigger, None);
    assert_eq!(payload.disposition, Some(QuarantineDisposition::Released));
    assert!(payload.reason.contains("_system:recovery:lucky"));
}

#[tokio::test]
async fn gc_pass_run_emits_gc_pass_receipt_with_counts() {
    let (log, emitter) = evidence_fixture();
    let fs = InMemoryAiosFs::new();
    let chunk = chunk_ref(b"gc");
    let (_, version_id) = write_one(&fs, "gc", chunk).await;
    fs.force_version_state_for_harness(&version_id, VersionState::RetiredVersion, None)
        .expect("retire version");

    let report = GcPassDriver::with_evidence_emitter(1024, 1024, emitter)
        .run_pass(&fs)
        .await
        .expect("run gc pass");
    let receipts = log.receipts().await;

    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].record_type(), RecordType::GcPass);
    let payload: GcPassPayload = payload_as(&receipts[0]);
    assert_eq!(payload.pass_id, report.pass_id);
    assert_eq!(payload.chunks_inspected, report.chunks_inspected);
    assert_eq!(payload.chunks_reclaimed, report.chunks_reclaimed);
    assert_eq!(payload.versions_inspected, report.versions_inspected);
    assert_eq!(payload.versions_purged, report.versions_purged);
}

#[tokio::test]
async fn gc_pass_payload_versions_purged_matches_report_counts() {
    let (log, emitter) = evidence_fixture();
    let fs = InMemoryAiosFs::new();
    let (_, left_version_id) = write_one(&fs, "left", chunk_ref(b"left")).await;
    let (_, right_version_id) = write_one(&fs, "right", chunk_ref(b"right")).await;
    fs.force_version_state_for_harness(&left_version_id, VersionState::RetiredVersion, None)
        .expect("retire left");
    fs.force_version_state_for_harness(&right_version_id, VersionState::RetiredVersion, None)
        .expect("retire right");

    let report = GcPassDriver::with_evidence_emitter(1024, 1024, emitter)
        .run_pass(&fs)
        .await
        .expect("run gc pass");
    let receipts = log.receipts().await;
    let payload: GcPassPayload = payload_as(&receipts[0]);

    assert_eq!(report.versions_purged, 2);
    assert_eq!(payload.versions_purged, report.versions_purged);
}

#[tokio::test]
async fn record_conflict_event_helper_emits_conflict_event_with_resolution_kind() {
    let (log, emitter) = evidence_fixture();
    let object_id = ObjectId::new();

    let receipt_id = record_conflict_event(&emitter, &object_id, "candidate lost CAS", "resolve")
        .await
        .expect("record conflict");
    let receipts = log.receipts().await;

    assert_eq!(receipts.len(), 1);
    assert_eq!(receipt_id, receipts[0].receipt_id().as_str());
    assert_eq!(receipts[0].record_type(), RecordType::ConflictEvent);
    let payload: ConflictEventPayload = payload_as(&receipts[0]);
    assert_eq!(payload.object_id, object_id);
    assert_eq!(payload.conflict_summary, "candidate lost CAS");
    assert_eq!(payload.resolution_kind, ConflictResolutionKind::Resolve);
}

#[tokio::test]
async fn blake3_chain_links_second_receipt_to_first_receipt_canonical_bytes() {
    let (log, emitter) = evidence_fixture();
    let fs = InMemoryAiosFs::with_evidence_emitter(emitter);

    fs.write_object(
        write_request("family:alice", "left", vec![chunk_ref(b"left")]),
        &context("family:alice"),
    )
    .await
    .expect("write left");
    fs.write_object(
        write_request("family:alice", "right", vec![chunk_ref(b"right")]),
        &context("family:alice"),
    )
    .await
    .expect("write right");

    let receipts = log.receipts().await;
    assert_eq!(receipts.len(), 2);
    assert_eq!(
        receipts[1].previous_receipt_hash(),
        Some(receipts[0].link_hash().expect("first link hash").as_str())
    );
    log.verify_integrity().await.expect("chain integrity");
}

#[tokio::test]
async fn no_emitter_configured_preserves_write_behaviour_and_emits_zero_receipts() {
    let log = InMemoryFsEvidenceLog::new();
    let fs = InMemoryAiosFs::new();

    let written = fs
        .write_object(
            write_request("family:alice", "v1", vec![chunk_ref(b"v1")]),
            &context("family:alice"),
        )
        .await
        .expect("write object");
    let read = fs
        .read_object(&written.object_id, None)
        .await
        .expect("read object");

    assert_eq!(read.version.version_id, written.version_id);
    assert!(log.is_empty().await);
}

#[tokio::test]
async fn emitted_receipts_verify_with_emitter_ed25519_key() {
    let (log, emitter) = evidence_fixture();
    let fs = InMemoryAiosFs::with_evidence_emitter(emitter.clone());

    fs.write_object(
        write_request("family:alice", "v1", vec![chunk_ref(b"v1")]),
        &context("family:alice"),
    )
    .await
    .expect("write object");
    let receipts = log.receipts().await;

    assert!(receipts[0].is_signed());
    receipts[0]
        .verify_signature(&emitter.verifying_key())
        .expect("signature verifies");
}

#[tokio::test]
async fn evidence_emission_failure_propagates_as_fs_error() {
    let emitter = Arc::new(FsEvidenceEmitter::new(
        Arc::new(FailingFsEvidenceLog),
        signing_key(),
        subject("_system:service:aios-fs"),
    ));
    let fs = InMemoryAiosFs::with_evidence_emitter(emitter);

    let err = fs
        .write_object(
            write_request("family:alice", "v1", vec![chunk_ref(b"v1")]),
            &context("family:alice"),
        )
        .await
        .expect_err("failing evidence log must propagate");

    assert!(matches!(err, FsError::EvidenceEmitFailed(_)));
}

#[tokio::test]
async fn concurrent_writes_produce_coherent_receipts_without_chain_corruption() {
    let (log, emitter) = evidence_fixture();
    let fs = Arc::new(InMemoryAiosFs::with_evidence_emitter(emitter));
    let left = Arc::clone(&fs);
    let right = Arc::clone(&fs);

    let left_task = tokio::spawn(async move {
        left.write_object(
            write_request("family:alice", "left", vec![chunk_ref(b"left")]),
            &context("family:alice"),
        )
        .await
    });
    let right_task = tokio::spawn(async move {
        right
            .write_object(
                write_request("family:bob", "right", vec![chunk_ref(b"right")]),
                &context("family:bob"),
            )
            .await
    });

    left_task.await.expect("left join").expect("left write");
    right_task.await.expect("right join").expect("right write");

    let receipts = log.receipts().await;
    assert_eq!(receipts.len(), 2);
    assert!(receipts
        .iter()
        .all(|receipt| receipt.record_type() == RecordType::ActionReceived));
    log.verify_integrity().await.expect("chain integrity");
}

async fn write_quarantine_chain(fs: &InMemoryAiosFs) -> (ObjectId, VersionId, VersionId) {
    let first = fs
        .write_object(
            write_request("family:alice", "v1", vec![chunk_ref(b"v1")]),
            &context("family:alice"),
        )
        .await
        .expect("write v1");
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
        .expect("write v2");

    (first.object_id, first.version_id, second.version_id)
}

#[derive(Debug)]
struct FailingFsEvidenceLog;

#[async_trait]
impl FsEvidenceLog for FailingFsEvidenceLog {
    async fn append_signed(
        &self,
        _builder: ReceiptBuilder,
        _signing_key: &SigningKey,
    ) -> Result<EvidenceReceipt, EvidenceError> {
        Err(EvidenceError::EncodingFailed(
            "injected evidence append failure".to_owned(),
        ))
    }
}
