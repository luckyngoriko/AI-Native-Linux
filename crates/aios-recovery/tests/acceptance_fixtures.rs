//! T-083 S9.1, S9.2, and S9.3 acceptance fixtures.

#![allow(
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::panic,
    reason = "integration test fixtures are intentionally direct"
)]

use std::error::Error;
use std::sync::Arc;

use aios_fs::{AiosPath, SubjectRef as FsSubjectRef};
use aios_recovery::first_boot::FIRST_BOOT_PROVISIONING_PHASES;
use aios_recovery::{
    BootPhase, CandidateState, EnterRecoveryRequest, FirstBootDriver, FirstBootPhase,
    FirstBootStatus, InMemoryRecoveryBoundary, KernelManifest, KernelPipelineDriver,
    RecoveryBoundary, RecoveryError, RecoveryGuard, RecoveryMode,
};
use ed25519_dalek::{Signer, SigningKey};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const AUTHORITY: &str = "aios-kernel-root";

fn operator_enter_request(reason: &str) -> EnterRecoveryRequest {
    EnterRecoveryRequest {
        reason: reason.to_owned(),
        operator_grant: Some("grant_t083_acceptance".to_owned()),
        expected_phases: vec![BootPhase::Recovery],
        bundle: None,
    }
}

fn fallback_enter_request(reason: &str) -> EnterRecoveryRequest {
    EnterRecoveryRequest {
        reason: reason.to_owned(),
        operator_grant: None,
        expected_phases: vec![BootPhase::Recovery],
        bundle: None,
    }
}

async fn enter_recovery(boundary: &InMemoryRecoveryBoundary) -> TestResult {
    boundary
        .enter_recovery(operator_enter_request("OPERATOR_INITIATED"))
        .await?;
    Ok(())
}

async fn exit_recovery(boundary: &InMemoryRecoveryBoundary) -> TestResult {
    let token = boundary
        .current_exit_token()
        .await
        .ok_or("missing active recovery exit token")?;
    boundary.exit_recovery(&token).await?;
    Ok(())
}

fn boundary_trait(boundary: &Arc<InMemoryRecoveryBoundary>) -> Arc<dyn RecoveryBoundary> {
    boundary.clone()
}

fn first_boot_driver(boundary: &Arc<InMemoryRecoveryBoundary>) -> FirstBootDriver {
    FirstBootDriver::new(boundary_trait(boundary))
}

fn manifest(version: &str, requires_recovery_install: bool) -> KernelManifest {
    KernelManifest {
        version: version.to_owned(),
        min_aios_version: "0.1.0".to_owned(),
        requires_recovery_install,
        verification_intent: Some("S9.3 acceptance gate witness".to_owned()),
        tags: vec!["KSPP_STRICT".to_owned()],
    }
}

fn sign_manifest(manifest: &KernelManifest, signing_key: &SigningKey) -> TestResult<Vec<u8>> {
    Ok(signing_key
        .sign(&serde_json::to_vec(manifest)?)
        .to_bytes()
        .to_vec())
}

fn kernel_pipeline(
    boundary: &Arc<InMemoryRecoveryBoundary>,
    signing_key: &SigningKey,
) -> KernelPipelineDriver {
    KernelPipelineDriver::new(boundary_trait(boundary))
        .with_trusted_authority(AUTHORITY.to_owned(), signing_key.verifying_key())
}

async fn register_verified(
    pipeline: &KernelPipelineDriver,
    signing_key: &SigningKey,
    version: &str,
    requires_recovery_install: bool,
) -> TestResult<aios_recovery::KernelCandidate> {
    let manifest = manifest(version, requires_recovery_install);
    let registered = pipeline
        .register_candidate(manifest.clone(), sign_manifest(&manifest, signing_key)?)
        .await?;
    Ok(pipeline.verify_candidate(&registered.candidate_id).await?)
}

#[tokio::test]
async fn s91_example_14_1_normal_boot_starts_in_normal_mode() {
    let boundary = InMemoryRecoveryBoundary::new();

    let state = boundary.current_state().await;

    assert_eq!(state.mode, RecoveryMode::Normal);
    assert!(!boundary.is_recovery_active().await);
}

#[tokio::test]
async fn s91_example_14_2_operator_planned_recovery_enters_and_exits_by_token() -> TestResult {
    let boundary = InMemoryRecoveryBoundary::new();

    let entered = boundary
        .enter_recovery(operator_enter_request("OPERATOR_INITIATED"))
        .await?;
    assert_eq!(entered.mode, RecoveryMode::Recovery);
    assert_eq!(entered.reason.as_deref(), Some("OPERATOR_INITIATED"));

    let wrong_token = boundary.exit_recovery("bad-token").await;
    assert!(matches!(
        wrong_token,
        Err(RecoveryError::RecoveryAuthorizationInvalid(_))
    ));
    assert!(boundary.is_recovery_active().await);

    exit_recovery(&boundary).await?;
    assert_eq!(boundary.current_state().await.mode, RecoveryMode::Normal);
    Ok(())
}

#[tokio::test]
async fn s91_example_14_3_auto_recovery_accepts_boot_failure_without_operator_grant() -> TestResult
{
    let boundary = InMemoryRecoveryBoundary::new();

    let state = boundary
        .enter_recovery(fallback_enter_request("BOOT_FAILURE_AUTO"))
        .await?;

    assert_eq!(state.mode, RecoveryMode::Recovery);
    assert_eq!(state.operator_grant, None);
    Ok(())
}

#[tokio::test]
async fn s91_adversarial_recovery_only_mutation_requires_recovery_mode() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let guard = RecoveryGuard::new(boundary_trait(&boundary));
    let path = AiosPath::new("/aios/system/policy/active.bundle");
    let subject = FsSubjectRef("_system:recovery:operator".to_owned());

    let denied = guard.check_mutation(&path, &subject, false).await;
    assert!(matches!(
        denied,
        Err(RecoveryError::RecoveryOnlyPathMutationDenied { .. })
    ));

    enter_recovery(&boundary).await?;
    guard.check_mutation(&path, &subject, false).await?;
    Ok(())
}

#[tokio::test]
async fn s92_example_13_1_standard_fresh_install_happy_path_completes_all_stages() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let driver = first_boot_driver(&boundary);

    let context = driver.run_provisioning().await?;

    assert_eq!(context.status, FirstBootStatus::Completed);
    assert_eq!(context.performed_phases, FIRST_BOOT_PROVISIONING_PHASES);
    assert!(!context
        .performed_phases
        .contains(&FirstBootPhase::StageFailedRequiresRecovery));
    assert_eq!(driver.stage_records().await.len(), 14);
    Ok(())
}

#[tokio::test]
async fn s92_example_13_3_mid_stage_resume_is_idempotent_after_completion() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let driver = first_boot_driver(&boundary);

    let first = driver.run_provisioning().await?;
    let records_after_first = driver.stage_records().await;
    let second = driver.run_provisioning().await?;

    assert_eq!(second.boot_id, first.boot_id);
    assert_eq!(second.status, FirstBootStatus::Completed);
    assert_eq!(driver.stage_records().await, records_after_first);
    Ok(())
}

#[tokio::test]
async fn s92_adversarial_terminal_commit_requires_prior_phases() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let driver = first_boot_driver(&boundary);

    let err = driver
        .mark_complete()
        .await
        .expect_err("terminal commit must reject missing prior stages");

    assert!(matches!(
        err,
        RecoveryError::InvalidPhaseTransition {
            from: FirstBootPhase::StageInstallerMediaVerified,
            to: FirstBootPhase::StageFirstBootComplete
        }
    ));
}

#[tokio::test]
async fn s92_adversarial_failure_enters_terminal_failed_requires_recovery_state() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let driver = first_boot_driver(&boundary);

    driver
        .fail_stage(
            FirstBootPhase::StageInstallerMediaVerified,
            "installer media hash mismatch",
        )
        .await?;
    let context = driver.current_context().await;

    assert_eq!(context.status, FirstBootStatus::Failed);
    assert!(context
        .performed_phases
        .contains(&FirstBootPhase::StageInstallerMediaVerified));
    assert!(context
        .performed_phases
        .contains(&FirstBootPhase::StageFailedRequiresRecovery));
    Ok(())
}

#[tokio::test]
async fn s93_example_17_1_bootstrap_register_verify_activate_kernel_candidate() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let signing_key = SigningKey::from_bytes(&[93_u8; 32]);
    let pipeline = kernel_pipeline(&boundary, &signing_key);

    let verified = register_verified(&pipeline, &signing_key, "0.1.0-s93-bootstrap", false).await?;
    let active = pipeline.activate_candidate(&verified.candidate_id).await?;

    assert_eq!(verified.state, CandidateState::GatePassed);
    assert_eq!(active.state, CandidateState::APromoted);
    assert_eq!(
        pipeline
            .get_active()
            .await
            .map(|candidate| candidate.candidate_id),
        Some(active.candidate_id)
    );
    Ok(())
}

#[tokio::test]
async fn s93_adversarial_gate_result_forgery_invalid_signature_rejected() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let trusted_key = SigningKey::from_bytes(&[94_u8; 32]);
    let wrong_key = SigningKey::from_bytes(&[95_u8; 32]);
    let pipeline = kernel_pipeline(&boundary, &trusted_key);
    let manifest = manifest("0.1.0-s93-forgery", false);

    let err = pipeline
        .register_candidate(manifest.clone(), sign_manifest(&manifest, &wrong_key)?)
        .await
        .expect_err("forged kernel signature must be rejected");

    assert_eq!(err, RecoveryError::KernelSignatureInvalid);
    Ok(())
}

#[tokio::test]
async fn s93_promotion_requires_recovery_when_manifest_is_recovery_only() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let signing_key = SigningKey::from_bytes(&[96_u8; 32]);
    let pipeline = kernel_pipeline(&boundary, &signing_key);
    let verified =
        register_verified(&pipeline, &signing_key, "0.1.0-s93-recovery-only", true).await?;

    let denied = pipeline.activate_candidate(&verified.candidate_id).await;
    assert_eq!(denied, Err(RecoveryError::RecoveryNotActive));

    enter_recovery(&boundary).await?;
    let active = pipeline.activate_candidate(&verified.candidate_id).await?;
    assert_eq!(active.state, CandidateState::APromoted);
    Ok(())
}

#[tokio::test]
async fn s93_rollback_restores_previous_active_candidate() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let signing_key = SigningKey::from_bytes(&[97_u8; 32]);
    let pipeline = kernel_pipeline(&boundary, &signing_key);

    let first = register_verified(&pipeline, &signing_key, "0.1.0-s93-a", false).await?;
    let first_active = pipeline.activate_candidate(&first.candidate_id).await?;
    let second = register_verified(&pipeline, &signing_key, "0.1.0-s93-b", false).await?;
    let second_active = pipeline.activate_candidate(&second.candidate_id).await?;

    let rolled_back = pipeline
        .rollback_candidate(&second_active.candidate_id)
        .await?;

    assert_eq!(first_active.state, CandidateState::APromoted);
    assert_eq!(rolled_back.state, CandidateState::Rollback);
    assert_eq!(
        pipeline
            .get_active()
            .await
            .map(|candidate| candidate.candidate_id),
        Some(first_active.candidate_id)
    );
    Ok(())
}
