//! T-080 integration tests for S9.x -> S3.1 evidence emission.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    reason = "Integration-test failures should point at the failing contract"
)]

use std::error::Error;
use std::sync::Arc;

use aios_evidence::{EvidenceReceipt, RecordType};
use aios_recovery::first_boot::FIRST_BOOT_PROVISIONING_PHASES;
use aios_recovery::{
    BootId, BootPhase, CandidateId, EnterRecoveryRequest, FirstBootCompletedPayload,
    FirstBootContext, FirstBootDriver, FirstBootPhase, FirstBootPhaseCompletedPayload,
    FirstBootStartedPayload, FirstBootStatus, InMemoryRecoveryBoundary,
    InMemoryRecoveryEvidenceLog, KernelActivatedPayload, KernelCandidate,
    KernelCandidateRegisteredPayload, KernelManifest, KernelPipelineDriver,
    KernelRolledBackPayload, RecoveryBoundary, RecoveryEnteredPayload, RecoveryError,
    RecoveryEvidenceEmitter, RecoveryExitedPayload, RecoveryMode, RecoverySubjectRef,
};
use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde::de::DeserializeOwned;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const AUTHORITY: &str = "aios-kernel-root";
const RAW_SIGNATURE_MARKER: [u8; 64] = [0xA7; 64];
const PLAINTEXT_SECRET: &str = "AIOS_RECOVERY_SECRET_MARKER_DO_NOT_EMIT";

fn fixed_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 0)
        .single()
        .expect("fixture timestamp is valid")
}

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn evidence_fixture() -> (
    Arc<InMemoryRecoveryEvidenceLog>,
    Arc<RecoveryEvidenceEmitter>,
) {
    let log = Arc::new(InMemoryRecoveryEvidenceLog::new());
    let emitter = Arc::new(RecoveryEvidenceEmitter::new(
        log.clone(),
        signing_key(80),
        RecoverySubjectRef("_system:service:recovery-coordinator".to_owned()),
    ));
    (log, emitter)
}

fn operator_request() -> EnterRecoveryRequest {
    EnterRecoveryRequest {
        reason: "OPERATOR_INITIATED".to_owned(),
        operator_grant: Some("ovr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned()),
        expected_phases: vec![BootPhase::Recovery],
        bundle: None,
    }
}

fn manifest(version: &str, requires_recovery_install: bool) -> KernelManifest {
    KernelManifest {
        version: version.to_owned(),
        min_aios_version: "0.1.0".to_owned(),
        requires_recovery_install,
        verification_intent: Some("verify dedicated kernel gates".to_owned()),
        tags: vec!["KSPP_STRICT".to_owned()],
    }
}

fn sign_manifest(manifest: &KernelManifest, sk: &SigningKey) -> TestResult<Vec<u8>> {
    let body = serde_json::to_vec(manifest)?;
    Ok(sk.sign(&body).to_bytes().to_vec())
}

fn payload_as<T>(receipt: &EvidenceReceipt) -> T
where
    T: DeserializeOwned,
{
    serde_json::from_value(receipt.payload().clone()).expect("payload must decode")
}

fn payload_json(receipt: &EvidenceReceipt) -> String {
    serde_json::to_string(receipt.payload()).expect("payload serializes")
}

fn round_trip<T>(payload: &T)
where
    T: serde::Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(payload).expect("serialize");
    let back: T = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(&back, payload);
}

async fn active_exit_token(boundary: &InMemoryRecoveryBoundary) -> TestResult<String> {
    boundary
        .current_exit_token()
        .await
        .ok_or_else(|| RecoveryError::Internal("missing exit token".to_owned()).into())
}

fn kernel_driver(
    boundary: Arc<InMemoryRecoveryBoundary>,
    emitter: Arc<RecoveryEvidenceEmitter>,
    sk: &SigningKey,
) -> KernelPipelineDriver {
    KernelPipelineDriver::with_evidence_emitter(boundary as Arc<dyn RecoveryBoundary>, emitter)
        .with_trusted_authority(AUTHORITY.to_owned(), sk.verifying_key())
}

async fn verified_candidate(
    driver: &KernelPipelineDriver,
    sk: &SigningKey,
    version: &str,
    requires_recovery_install: bool,
) -> TestResult<KernelCandidate> {
    let manifest = manifest(version, requires_recovery_install);
    let signature = sign_manifest(&manifest, sk)?;
    let candidate = driver.register_candidate(manifest, signature).await?;
    Ok(driver.verify_candidate(&candidate.candidate_id).await?)
}

#[test]
fn recovery_entered_payload_round_trips_through_serde_json() {
    round_trip(&RecoveryEnteredPayload {
        from_mode: RecoveryMode::Normal,
        to_mode: RecoveryMode::Recovery,
        entered_at: fixed_time(),
        reason: Some("OPERATOR_INITIATED".to_owned()),
        operator_grant: Some("ovr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned()),
    });
}

#[test]
fn recovery_exited_payload_round_trips_through_serde_json() {
    round_trip(&RecoveryExitedPayload {
        from_mode: RecoveryMode::Recovery,
        to_mode: RecoveryMode::Normal,
        exited_at: fixed_time(),
        exit_token: "blake3:abc123".to_owned(),
    });
}

#[test]
fn first_boot_payloads_round_trip_through_serde_json() {
    let context = FirstBootContext {
        boot_id: BootId::new(),
        started_at: fixed_time(),
        completed_at: Some(fixed_time()),
        status: FirstBootStatus::Completed,
        performed_phases: FIRST_BOOT_PROVISIONING_PHASES.to_vec(),
    };

    round_trip(&FirstBootStartedPayload {
        boot_id: context.boot_id.clone(),
        started_at: context.started_at,
        expected_phases: FIRST_BOOT_PROVISIONING_PHASES.to_vec(),
    });
    round_trip(&FirstBootPhaseCompletedPayload {
        boot_id: context.boot_id.clone(),
        phase: FirstBootPhase::StageInstallerMediaVerified,
        completed_at: fixed_time(),
    });
    round_trip(&FirstBootCompletedPayload {
        boot_id: context.boot_id,
        completed_at: fixed_time(),
        total_phases: FIRST_BOOT_PROVISIONING_PHASES.len() as u64,
        skipped_phases: vec![FirstBootPhase::StageInstallerMediaVerified],
    });
}

#[test]
fn kernel_payloads_round_trip_through_serde_json() {
    let candidate_id = CandidateId::new();
    let previous_candidate_id = CandidateId::new();

    round_trip(&KernelCandidateRegisteredPayload {
        candidate_id: candidate_id.clone(),
        version: "linux-6.6.42-aios.1".to_owned(),
        kernel_blake3: "ab".repeat(32),
        signing_authority: AUTHORITY.to_owned(),
        registered_at: fixed_time(),
    });
    round_trip(&KernelActivatedPayload {
        candidate_id: candidate_id.clone(),
        version: "linux-6.6.42-aios.1".to_owned(),
        kernel_blake3: "ab".repeat(32),
        activated_at: fixed_time(),
        required_recovery: true,
    });
    round_trip(&KernelRolledBackPayload {
        candidate_id,
        previous_candidate_id,
        reason: "previous active restored".to_owned(),
        rolled_back_at: fixed_time(),
    });
}

#[tokio::test]
async fn enter_recovery_emits_recovery_boot_entered_with_mode_transition() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let boundary = InMemoryRecoveryBoundary::with_evidence_emitter(emitter);

    let state = boundary.enter_recovery(operator_request()).await?;
    let receipts = log.receipts().await;

    assert_eq!(state.mode, RecoveryMode::Recovery);
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].record_type(), RecordType::RecoveryBootEntered);
    let payload: RecoveryEnteredPayload = payload_as(&receipts[0]);
    assert_eq!(payload.from_mode, RecoveryMode::Normal);
    assert_eq!(payload.to_mode, RecoveryMode::Recovery);
    assert_eq!(payload.reason.as_deref(), Some("OPERATOR_INITIATED"));
    Ok(())
}

#[tokio::test]
async fn exit_recovery_emits_recovery_boot_exited_with_hashed_token() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let boundary = InMemoryRecoveryBoundary::with_evidence_emitter(emitter);
    let _state = boundary.enter_recovery(operator_request()).await?;
    let token = active_exit_token(&boundary).await?;

    let state = boundary.exit_recovery(&token).await?;
    let receipts = log.receipts().await;

    assert_eq!(state.mode, RecoveryMode::Normal);
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[1].record_type(), RecordType::RecoveryBootExited);
    let payload: RecoveryExitedPayload = payload_as(&receipts[1]);
    assert_eq!(payload.from_mode, RecoveryMode::Recovery);
    assert_eq!(payload.to_mode, RecoveryMode::Normal);
    assert!(payload.exit_token.starts_with("blake3:"));
    assert!(!payload_json(&receipts[1]).contains(&token));
    Ok(())
}

#[tokio::test]
async fn run_provisioning_emits_started_per_phase_and_completed() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let driver =
        FirstBootDriver::with_evidence_emitter(boundary as Arc<dyn RecoveryBoundary>, emitter);

    let context = driver.run_provisioning().await?;
    let receipts = log.receipts().await;

    assert_eq!(context.status, FirstBootStatus::Completed);
    assert_eq!(receipts.len(), FIRST_BOOT_PROVISIONING_PHASES.len() + 2);
    assert_eq!(receipts[0].record_type(), RecordType::FirstBootStarted);
    for (idx, phase) in FIRST_BOOT_PROVISIONING_PHASES.iter().copied().enumerate() {
        assert_eq!(
            receipts[idx + 1].record_type(),
            RecordType::FirstBootStageCompleted
        );
        let payload: FirstBootPhaseCompletedPayload = payload_as(&receipts[idx + 1]);
        assert_eq!(payload.phase, phase);
    }
    assert_eq!(
        receipts.last().expect("completion receipt").record_type(),
        RecordType::FirstBootComplete
    );
    let payload: FirstBootCompletedPayload =
        payload_as(receipts.last().expect("completion receipt"));
    assert_eq!(
        payload.total_phases,
        FIRST_BOOT_PROVISIONING_PHASES.len() as u64
    );
    assert!(payload.skipped_phases.is_empty());
    Ok(())
}

#[tokio::test]
async fn register_candidate_emits_kernel_candidate_registered_payload() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let sk = signing_key(81);
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let driver = kernel_driver(boundary, emitter, &sk);
    let manifest = manifest("linux-6.6.42-aios.2", false);
    let signature = sign_manifest(&manifest, &sk)?;

    let candidate = driver.register_candidate(manifest, signature).await?;
    let receipts = log.receipts().await;

    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].record_type(), RecordType::KernelPipelineStarted);
    let payload: KernelCandidateRegisteredPayload = payload_as(&receipts[0]);
    assert_eq!(payload.candidate_id, candidate.candidate_id);
    assert_eq!(payload.signing_authority, AUTHORITY);
    Ok(())
}

#[tokio::test]
async fn activate_candidate_emits_kernel_activated_payload() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let sk = signing_key(82);
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let driver = kernel_driver(boundary, emitter, &sk);
    let candidate = verified_candidate(&driver, &sk, "linux-6.6.42-aios.3", false).await?;

    let active = driver.activate_candidate(&candidate.candidate_id).await?;
    let receipts = log.receipts().await;

    assert_eq!(receipts.len(), 3);
    assert_eq!(receipts[2].record_type(), RecordType::KernelPromotedToA);
    let payload: KernelActivatedPayload = payload_as(&receipts[2]);
    assert_eq!(payload.candidate_id, active.candidate_id);
    assert!(!payload.required_recovery);
    Ok(())
}

#[tokio::test]
async fn rollback_candidate_emits_kernel_rolled_back_with_previous_candidate_id() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let sk = signing_key(83);
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let driver = kernel_driver(boundary, emitter, &sk);
    let first = verified_candidate(&driver, &sk, "linux-6.6.42-aios.4", false).await?;
    let _active_first = driver.activate_candidate(&first.candidate_id).await?;
    let second = verified_candidate(&driver, &sk, "linux-6.6.42-aios.5", false).await?;
    let _active_second = driver.activate_candidate(&second.candidate_id).await?;

    let rolled_back = driver.rollback_candidate(&second.candidate_id).await?;
    let receipts = log.receipts().await;

    assert_eq!(
        receipts.last().expect("rollback").record_type(),
        RecordType::KernelRollbackPerformed
    );
    let payload: KernelRolledBackPayload = payload_as(receipts.last().expect("rollback"));
    assert_eq!(payload.candidate_id, rolled_back.candidate_id);
    assert_eq!(payload.previous_candidate_id, first.candidate_id);
    Ok(())
}

#[tokio::test]
async fn blake3_chain_links_second_receipt_to_first_receipt_hash() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let boundary = InMemoryRecoveryBoundary::with_evidence_emitter(emitter);
    let _state = boundary.enter_recovery(operator_request()).await?;
    let token = active_exit_token(&boundary).await?;
    let _state = boundary.exit_recovery(&token).await?;
    let receipts = log.receipts().await;

    assert_eq!(
        receipts[1].previous_receipt_hash(),
        Some(receipts[0].link_hash()?.as_str())
    );
    log.verify_integrity().await?;
    Ok(())
}

#[tokio::test]
async fn inv_018_emitted_payloads_do_not_contain_raw_signature_bytes() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let sk = signing_key(84);
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let driver = kernel_driver(boundary, emitter, &sk);
    let invalid_manifest = manifest("linux-6.6.42-aios.6", false);

    let _candidate = driver
        .register_candidate(invalid_manifest, RAW_SIGNATURE_MARKER.to_vec())
        .await
        .expect_err("known raw signature is intentionally invalid");
    assert!(log.receipts().await.is_empty());

    let manifest = manifest("linux-6.6.42-aios.7", false);
    let signature = sign_manifest(&manifest, &sk)?;
    let _candidate = driver.register_candidate(manifest, signature).await?;

    for receipt in log.receipts().await {
        let bytes = serde_json::to_vec(receipt.payload())?;
        assert!(!bytes
            .windows(RAW_SIGNATURE_MARKER.len())
            .any(|w| w == RAW_SIGNATURE_MARKER));
    }
    Ok(())
}

#[tokio::test]
async fn inv_015_emitted_payloads_do_not_contain_plaintext_secrets() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let sk = signing_key(85);
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let driver = kernel_driver(boundary, emitter, &sk);
    let mut manifest = manifest("linux-6.6.42-aios.8", false);
    manifest.tags = vec![format!(
        "hash:{}",
        blake3::hash(PLAINTEXT_SECRET.as_bytes()).to_hex()
    )];
    let signature = sign_manifest(&manifest, &sk)?;
    let candidate = driver.register_candidate(manifest, signature).await?;
    let _verified = driver.verify_candidate(&candidate.candidate_id).await?;

    for receipt in log.receipts().await {
        assert!(!payload_json(&receipt).contains(PLAINTEXT_SECRET));
    }
    Ok(())
}

#[tokio::test]
async fn emitted_receipts_verify_with_emitter_ed25519_key() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let verifying_key = emitter.verifying_key();
    let boundary = InMemoryRecoveryBoundary::with_evidence_emitter(emitter);

    let _state = boundary.enter_recovery(operator_request()).await?;

    for receipt in log.receipts().await {
        assert!(receipt.is_signed());
        receipt.verify_signature(&verifying_key)?;
    }
    Ok(())
}

#[tokio::test]
async fn no_emitter_configured_preserves_existing_success_paths() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let first_boot = FirstBootDriver::new(boundary.clone() as Arc<dyn RecoveryBoundary>);
    let sk = signing_key(86);
    let kernel = KernelPipelineDriver::new(boundary.clone() as Arc<dyn RecoveryBoundary>)
        .with_trusted_authority(AUTHORITY.to_owned(), sk.verifying_key());

    let _state = boundary.enter_recovery(operator_request()).await?;
    let token = active_exit_token(&boundary).await?;
    let _state = boundary.exit_recovery(&token).await?;
    let _context = first_boot.run_provisioning().await?;
    let candidate = verified_candidate(&kernel, &sk, "linux-6.6.42-aios.9", false).await?;
    let _active = kernel.activate_candidate(&candidate.candidate_id).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn concurrent_recovery_enter_exit_operations_keep_chain_coherent() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let mut tasks = Vec::new();

    for _idx in 0..3 {
        let emitter = Arc::clone(&emitter);
        tasks.push(tokio::spawn(async move {
            let boundary = InMemoryRecoveryBoundary::with_evidence_emitter(emitter);
            let _state = boundary.enter_recovery(operator_request()).await?;
            let token = boundary
                .current_exit_token()
                .await
                .ok_or_else(|| RecoveryError::Internal("missing exit token".to_owned()))?;
            let _state = boundary.exit_recovery(&token).await?;
            Ok::<(), RecoveryError>(())
        }));
    }

    for task in tasks {
        task.await??;
    }

    let receipts = log.receipts().await;
    assert_eq!(receipts.len(), 6);
    log.verify_integrity().await?;
    for receipt in receipts {
        receipt.verify_signature(&signing_key(80).verifying_key())?;
    }
    Ok(())
}

#[tokio::test]
async fn full_recovery_kernel_lifecycle_emits_ordered_receipts_with_chain_links() -> TestResult {
    let (log, emitter) = evidence_fixture();
    let sk = signing_key(87);
    let boundary = Arc::new(InMemoryRecoveryBoundary::with_evidence_emitter(Arc::clone(
        &emitter,
    )));
    let driver = kernel_driver(Arc::clone(&boundary), emitter, &sk);

    let _state = boundary.enter_recovery(operator_request()).await?;
    let manifest = manifest("linux-6.6.42-aios.10", true);
    let signature = sign_manifest(&manifest, &sk)?;
    let candidate = driver.register_candidate(manifest, signature).await?;
    let verified = driver.verify_candidate(&candidate.candidate_id).await?;
    let _active = driver.activate_candidate(&verified.candidate_id).await?;
    let token = active_exit_token(&boundary).await?;
    let _state = boundary.exit_recovery(&token).await?;
    let receipts = log.receipts().await;

    assert_eq!(receipts.len(), 5);
    assert_eq!(receipts[0].record_type(), RecordType::RecoveryBootEntered);
    assert_eq!(receipts[1].record_type(), RecordType::KernelPipelineStarted);
    assert_eq!(receipts[2].record_type(), RecordType::KernelGateResult);
    assert_eq!(receipts[3].record_type(), RecordType::KernelPromotedToA);
    assert_eq!(receipts[4].record_type(), RecordType::RecoveryBootExited);
    for pair in receipts.windows(2) {
        assert_eq!(
            pair[1].previous_receipt_hash(),
            Some(pair[0].link_hash()?.as_str())
        );
    }
    log.verify_integrity().await?;
    Ok(())
}
