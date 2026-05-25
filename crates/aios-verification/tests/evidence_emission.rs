//! T-070 integration tests for S2.4 -> S3.1 evidence emission.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use ed25519_dalek::SigningKey;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use aios_action::ActionId;
use aios_evidence::{EvidenceReceipt, RecordType};
use aios_verification::{
    InMemoryVerificationEngine, InMemoryVerificationEvidenceLog, LocalProbe, MockLocalProbe,
    PrimitiveExecutedPayload, PrimitiveResult, SubjectRef, VerificationContext, VerificationEngine,
    VerificationEvidenceEmitter, VerificationIntent, VerificationPrimitive,
    VerificationResultPayload, VerificationStartedPayload, VerificationStatus,
};

const SECRET_MARKER: &str = "AIOS_VERIFY_SECRET_MARKER";

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[70_u8; 32])
}

fn evidence_fixture() -> (
    Arc<InMemoryVerificationEvidenceLog>,
    Arc<VerificationEvidenceEmitter>,
) {
    let log = Arc::new(InMemoryVerificationEvidenceLog::new());
    let emitter = Arc::new(VerificationEvidenceEmitter::new(
        log.clone(),
        signing_key(),
        SubjectRef("_system:service:verification-engine".to_owned()),
    ));
    (log, emitter)
}

fn context_for(action_id: ActionId) -> VerificationContext {
    VerificationContext {
        subject: "human:operator".to_owned(),
        action_id,
        started_at: Utc::now(),
        timeout_seconds: 5,
        dry_run: true,
    }
}

fn file_exists_intent(path: &str, timeout_seconds: u32) -> TestResult<VerificationIntent> {
    Ok(VerificationIntent::new(
        ActionId::new(),
        serde_json::to_string(&vec![json!({
            "primitive": "FILE_EXISTS",
            "object_or_path": path,
        })])?,
        timeout_seconds,
    ))
}

fn eventually_timeout_intent(path: &str) -> VerificationIntent {
    VerificationIntent::new(
        ActionId::new(),
        format!(
            r#"eventually(file.exists(object_or_path="{path}"), max_duration=1ms, interval=1ms)"#
        ),
        1,
    )
}

fn engine_with_emitter(
    emitter: Arc<VerificationEvidenceEmitter>,
    path: &str,
    exists: bool,
) -> InMemoryVerificationEngine {
    let probe: Arc<dyn LocalProbe> =
        Arc::new(MockLocalProbe::default().with_file_exists(path, exists));
    InMemoryVerificationEngine::new()
        .with_local_probe(probe)
        .with_evidence_emitter(emitter)
}

fn payload_as<T>(receipt: &EvidenceReceipt) -> T
where
    T: DeserializeOwned,
{
    serde_json::from_value(receipt.payload().clone()).expect("payload must decode")
}

fn round_trip<T>(payload: &T)
where
    T: serde::Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(payload).expect("serialize");
    let back: T = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(&back, payload);
}

#[test]
fn verification_started_payload_round_trips_through_serde_json() {
    round_trip(&VerificationStartedPayload {
        intent_id: aios_verification::IntentId::new(),
        action_id: ActionId::new(),
        expression_hash: "b".repeat(64),
        primitive_count: 2,
        started_at: Utc.with_ymd_and_hms(2026, 5, 25, 9, 0, 0).single().unwrap(),
    });
}

#[test]
fn verification_result_payload_round_trips_through_serde_json() {
    round_trip(&VerificationResultPayload {
        intent_id: aios_verification::IntentId::new(),
        action_id: ActionId::new(),
        status: "PASSED".to_owned(),
        primitive_count: 2,
        passed_count: 2,
        failed_count: 0,
        duration_ms: 12,
        completed_at: Utc.with_ymd_and_hms(2026, 5, 25, 9, 0, 1).single().unwrap(),
    });
}

#[test]
fn primitive_executed_payload_round_trips_through_serde_json() {
    round_trip(&PrimitiveExecutedPayload {
        intent_id: aios_verification::IntentId::new(),
        primitive_kind: VerificationPrimitive::FileExists,
        passed: true,
        elapsed_ms: 3,
        error: None,
    });
}

#[tokio::test]
async fn run_verification_with_evidence_emitter_emits_started_and_result() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let engine = engine_with_emitter(emitter, "/tmp/aios-ok", true);
    let intent = file_exists_intent("/tmp/aios-ok", 5)?;
    let context = context_for(intent.action_id.clone());

    let result = engine.run_verification(&intent, &context).await?;
    let receipts = log.receipts().await;

    assert_eq!(result.status, VerificationStatus::Passed);
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0].record_type(), RecordType::VerificationResult);
    assert_eq!(receipts[1].record_type(), RecordType::VerificationResult);
    let started: VerificationStartedPayload = payload_as(&receipts[0]);
    let completed: VerificationResultPayload = payload_as(&receipts[1]);
    assert_eq!(started.intent_id, intent.intent_id);
    assert_eq!(completed.intent_id, intent.intent_id);
    assert_eq!(completed.status, "PASSED");
    assert_eq!(
        result.evidence_receipt_id.as_deref(),
        Some(receipts[1].receipt_id().as_str())
    );
    Ok(())
}

#[tokio::test]
async fn verification_result_receipt_has_correct_identity_and_status() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let engine = engine_with_emitter(emitter, "/tmp/aios-ok", true);
    let intent = file_exists_intent("/tmp/aios-ok", 5)?;
    let context = context_for(intent.action_id.clone());

    engine.run_verification(&intent, &context).await?;
    let receipts = log.receipts().await;
    let payload: VerificationResultPayload = payload_as(&receipts[1]);

    assert_eq!(payload.intent_id, intent.intent_id);
    assert_eq!(payload.action_id, intent.action_id);
    assert_eq!(payload.status, "PASSED");
    assert_eq!(payload.primitive_count, 1);
    assert_eq!(payload.passed_count, 1);
    assert_eq!(payload.failed_count, 0);
    Ok(())
}

#[tokio::test]
async fn run_verification_failed_status_emits_failed_result_payload() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let engine = engine_with_emitter(emitter, "/tmp/aios-missing", false);
    let intent = file_exists_intent("/tmp/aios-missing", 5)?;
    let context = context_for(intent.action_id.clone());

    let result = engine.run_verification(&intent, &context).await?;
    let receipts = log.receipts().await;
    let payload: VerificationResultPayload = payload_as(&receipts[1]);

    assert_eq!(result.status, VerificationStatus::Failed);
    assert_eq!(payload.status, "FAILED");
    assert_eq!(payload.failed_count, 1);
    Ok(())
}

#[tokio::test]
async fn run_verification_timeout_status_emits_timeout_result_payload() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let engine = engine_with_emitter(emitter, "/tmp/aios-timeout", false);
    let intent = eventually_timeout_intent("/tmp/aios-timeout");
    let context = context_for(intent.action_id.clone());

    let result = engine.run_verification(&intent, &context).await?;
    let receipts = log.receipts().await;
    let payload: VerificationResultPayload = payload_as(&receipts[1]);

    assert_eq!(result.status, VerificationStatus::Timeout);
    assert_eq!(payload.status, "TIMEOUT");
    Ok(())
}

#[tokio::test]
async fn blake3_chain_links_result_to_started_receipt() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let engine = engine_with_emitter(emitter, "/tmp/aios-ok", true);
    let intent = file_exists_intent("/tmp/aios-ok", 5)?;
    let context = context_for(intent.action_id.clone());

    engine.run_verification(&intent, &context).await?;
    let receipts = log.receipts().await;

    assert_eq!(
        receipts[1].previous_receipt_hash(),
        Some(receipts[0].link_hash()?.as_str())
    );
    log.verify_integrity().await?;
    Ok(())
}

#[tokio::test]
async fn inv_015_receipts_do_not_contain_raw_expected_secret_marker() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let engine = engine_with_emitter(emitter, SECRET_MARKER, false);
    let intent = file_exists_intent(SECRET_MARKER, 5)?;
    let context = context_for(intent.action_id.clone());

    engine.run_verification(&intent, &context).await?;
    let receipts = log.receipts().await;

    for receipt in receipts {
        let bytes = serde_json::to_vec(&receipt)?;
        let text = String::from_utf8(bytes)?;
        assert!(!text.contains(SECRET_MARKER));
    }
    Ok(())
}

#[tokio::test]
async fn no_evidence_emitter_keeps_backward_compatibility() -> TestResult {
    let probe: Arc<dyn LocalProbe> =
        Arc::new(MockLocalProbe::default().with_file_exists("/tmp/aios-ok", true));
    let engine = InMemoryVerificationEngine::new().with_local_probe(probe);
    let intent = file_exists_intent("/tmp/aios-ok", 5)?;
    let context = context_for(intent.action_id.clone());

    let result = engine.run_verification(&intent, &context).await?;

    assert_eq!(result.status, VerificationStatus::Passed);
    assert_eq!(result.evidence_receipt_id, None);
    Ok(())
}

#[tokio::test]
async fn emitted_receipts_are_ed25519_signed() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let engine = engine_with_emitter(Arc::clone(&emitter), "/tmp/aios-ok", true);
    let intent = file_exists_intent("/tmp/aios-ok", 5)?;
    let context = context_for(intent.action_id.clone());

    engine.run_verification(&intent, &context).await?;
    let receipts = log.receipts().await;

    assert!(receipts.iter().all(EvidenceReceipt::is_signed));
    log.verify_integrity_signed(&emitter.verifying_key())
        .await?;
    Ok(())
}

#[tokio::test]
async fn primitive_executed_emission_omits_actual_and_expected_values() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let primitive_result = PrimitiveResult {
        primitive_kind: VerificationPrimitive::FileExists,
        passed: false,
        actual: json!({"observed": SECRET_MARKER}),
        expected: json!({"expected": SECRET_MARKER}),
        elapsed_ms: 7,
        error: Some(format!("PROBE_ERROR: {SECRET_MARKER}")),
    };

    emitter
        .emit_primitive_executed(&aios_verification::IntentId::new(), &primitive_result, None)
        .await?;
    let receipt = log.receipts().await.remove(0);
    let payload: PrimitiveExecutedPayload = payload_as(&receipt);
    let text = serde_json::to_string(&receipt)?;

    assert_eq!(payload.primitive_kind, VerificationPrimitive::FileExists);
    assert!(!text.contains("actual"));
    assert!(!text.contains("expected"));
    assert!(!text.contains(SECRET_MARKER));
    Ok(())
}

#[tokio::test]
async fn concurrent_run_verification_emits_ten_coherent_receipts() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let engine = Arc::new(engine_with_emitter(emitter, "/tmp/aios-ok", true));
    let mut handles = Vec::new();

    for _ in 0..5 {
        let engine = Arc::clone(&engine);
        handles.push(tokio::spawn(async move {
            let intent = file_exists_intent("/tmp/aios-ok", 5)?;
            let context = context_for(intent.action_id.clone());
            engine.run_verification(&intent, &context).await?;
            TestResult::Ok(())
        }));
    }

    for handle in handles {
        handle.await??;
    }
    let receipts = log.receipts().await;
    let started_count = receipts
        .iter()
        .filter(|receipt| payload_has_key(receipt.payload(), "started_at"))
        .count();
    let result_count = receipts
        .iter()
        .filter(|receipt| payload_has_key(receipt.payload(), "completed_at"))
        .count();

    assert_eq!(receipts.len(), 10);
    assert_eq!(started_count, 5);
    assert_eq!(result_count, 5);
    log.verify_integrity().await?;
    Ok(())
}

fn payload_has_key(payload: &Value, key: &str) -> bool {
    payload
        .as_object()
        .is_some_and(|object| object.contains_key(key))
}
