//! T-077 dedicated-kernel pipeline driver tests for S9.3.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "Integration-test failures should point at the failing contract"
)]

use std::error::Error;
use std::sync::Arc;

use aios_recovery::{
    BootPhase, CandidateId, CandidateState, EnterRecoveryRequest, InMemoryRecoveryBoundary,
    KernelCandidate, KernelManifest, KernelPipelineDriver, RecoveryBoundary, RecoveryError,
};
use ed25519_dalek::{Signer, SigningKey};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const AUTHORITY: &str = "aios-kernel-root";

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
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

fn boundary() -> Arc<InMemoryRecoveryBoundary> {
    Arc::new(InMemoryRecoveryBoundary::new())
}

fn driver(boundary: Arc<InMemoryRecoveryBoundary>, sk: &SigningKey) -> KernelPipelineDriver {
    KernelPipelineDriver::new(boundary as Arc<dyn RecoveryBoundary>)
        .with_trusted_authority(AUTHORITY.to_owned(), sk.verifying_key())
}

async fn enter_recovery(boundary: &InMemoryRecoveryBoundary) -> TestResult {
    let _state = boundary
        .enter_recovery(EnterRecoveryRequest {
            reason: "BOOT_FAILURE_AUTO".to_owned(),
            operator_grant: None,
            expected_phases: vec![BootPhase::Recovery],
            bundle: None,
        })
        .await?;
    Ok(())
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

async fn active_candidate(
    driver: &KernelPipelineDriver,
    sk: &SigningKey,
    version: &str,
    requires_recovery_install: bool,
) -> TestResult<KernelCandidate> {
    let candidate = verified_candidate(driver, sk, version, requires_recovery_install).await?;
    Ok(driver.activate_candidate(&candidate.candidate_id).await?)
}

#[tokio::test]
async fn register_candidate_with_valid_signature_enters_built() -> TestResult {
    let sk = signing_key(11);
    let driver = driver(boundary(), &sk);
    let manifest = manifest("linux-6.6.42-aios.1", false);
    let signature = sign_manifest(&manifest, &sk)?;

    let candidate = driver.register_candidate(manifest, signature).await?;

    assert_eq!(candidate.state, CandidateState::Built);
    assert_eq!(candidate.signing_authority, AUTHORITY);
    assert_eq!(candidate.signature_ed25519.len(), 64);
    assert_eq!(candidate.kernel_blake3.len(), 64);
    Ok(())
}

#[tokio::test]
async fn register_candidate_bad_signature_yields_kernel_signature_invalid() -> TestResult {
    let sk = signing_key(12);
    let driver = driver(boundary(), &sk);
    let manifest = manifest("linux-6.6.42-aios.2", false);
    let mut signature = sign_manifest(&manifest, &sk)?;
    signature[0] ^= 0x01;

    let err = driver
        .register_candidate(manifest, signature)
        .await
        .expect_err("tampered signature must reject");

    assert!(matches!(err, RecoveryError::KernelSignatureInvalid));
    Ok(())
}

#[tokio::test]
async fn register_candidate_without_trusted_authorities_rejects() -> TestResult {
    let sk = signing_key(13);
    let boundary = boundary();
    let driver = KernelPipelineDriver::new(boundary as Arc<dyn RecoveryBoundary>);
    let manifest = manifest("linux-6.6.42-aios.3", false);
    let signature = sign_manifest(&manifest, &sk)?;

    let err = driver
        .register_candidate(manifest, signature)
        .await
        .expect_err("empty trust store must reject");

    assert!(matches!(err, RecoveryError::KernelUnknownAuthority(_)));
    Ok(())
}

#[tokio::test]
async fn verify_candidate_transitions_built_through_gating_to_gate_passed() -> TestResult {
    let sk = signing_key(14);
    let driver = driver(boundary(), &sk);
    let manifest = manifest("linux-6.6.42-aios.4", false);
    let signature = sign_manifest(&manifest, &sk)?;
    let candidate = driver.register_candidate(manifest, signature).await?;

    let verified = driver.verify_candidate(&candidate.candidate_id).await?;

    assert_eq!(verified.state, CandidateState::GatePassed);
    Ok(())
}

#[tokio::test]
async fn activate_candidate_gate_passed_to_a_promoted_when_recovery_not_required() -> TestResult {
    let sk = signing_key(15);
    let driver = driver(boundary(), &sk);
    let candidate = verified_candidate(&driver, &sk, "linux-6.6.42-aios.5", false).await?;

    let active = driver.activate_candidate(&candidate.candidate_id).await?;

    assert_eq!(active.state, CandidateState::APromoted);
    assert_eq!(driver.get_active().await, Some(active));
    Ok(())
}

#[tokio::test]
async fn activate_candidate_requiring_recovery_rejects_outside_recovery() -> TestResult {
    let sk = signing_key(16);
    let driver = driver(boundary(), &sk);
    let candidate = verified_candidate(&driver, &sk, "linux-6.6.42-aios.6", true).await?;

    let err = driver
        .activate_candidate(&candidate.candidate_id)
        .await
        .expect_err("recovery-required promotion must reject outside recovery");

    assert!(matches!(err, RecoveryError::RecoveryNotActive));
    Ok(())
}

#[tokio::test]
async fn activate_candidate_requiring_recovery_succeeds_inside_recovery() -> TestResult {
    let sk = signing_key(17);
    let boundary = boundary();
    enter_recovery(&boundary).await?;
    let driver = driver(boundary, &sk);
    let candidate = verified_candidate(&driver, &sk, "linux-6.6.42-aios.7", true).await?;

    let active = driver.activate_candidate(&candidate.candidate_id).await?;

    assert_eq!(active.state, CandidateState::APromoted);
    Ok(())
}

#[tokio::test]
async fn rollback_candidate_marks_active_rollback_and_restores_previous_active() -> TestResult {
    let sk = signing_key(18);
    let driver = driver(boundary(), &sk);
    let first = active_candidate(&driver, &sk, "linux-6.6.42-aios.8", false).await?;
    let second = active_candidate(&driver, &sk, "linux-6.6.42-aios.9", false).await?;

    let rolled_back = driver.rollback_candidate(&second.candidate_id).await?;

    assert_eq!(rolled_back.state, CandidateState::Rollback);
    assert_eq!(
        driver.get_active().await.map(|c| c.candidate_id),
        Some(first.candidate_id)
    );
    Ok(())
}

#[tokio::test]
async fn rollback_candidate_without_previous_active_rejects() -> TestResult {
    let sk = signing_key(19);
    let driver = driver(boundary(), &sk);
    let active = active_candidate(&driver, &sk, "linux-6.6.42-aios.10", false).await?;

    let err = driver
        .rollback_candidate(&active.candidate_id)
        .await
        .expect_err("rollback without a tracked previous active cannot restore");

    assert!(matches!(err, RecoveryError::Internal(_)));
    assert_eq!(
        driver
            .get_active()
            .await
            .map(|candidate| candidate.candidate_id),
        Some(active.candidate_id)
    );
    Ok(())
}

#[tokio::test]
async fn retire_candidate_gate_passed_to_retired() -> TestResult {
    let sk = signing_key(20);
    let driver = driver(boundary(), &sk);
    let candidate = verified_candidate(&driver, &sk, "linux-6.6.42-aios.11", false).await?;

    driver
        .retire_candidate(&candidate.candidate_id, "operator archived stale candidate")
        .await?;
    let retired = driver
        .list_candidates()
        .await
        .into_iter()
        .find(|item| item.candidate_id == candidate.candidate_id)
        .ok_or_else(|| RecoveryError::CandidateNotFound(candidate.candidate_id.clone()))?;

    assert_eq!(retired.state, CandidateState::Retired);
    Ok(())
}

#[tokio::test]
async fn retire_current_active_rejects_without_successor() -> TestResult {
    let sk = signing_key(21);
    let driver = driver(boundary(), &sk);
    let active = active_candidate(&driver, &sk, "linux-6.6.42-aios.12", false).await?;

    let err = driver
        .retire_candidate(&active.candidate_id, "operator tried to retire slot A")
        .await
        .expect_err("current slot A cannot be retired without successor");

    assert!(matches!(
        err,
        RecoveryError::InvalidCandidateTransition {
            from: CandidateState::APromoted,
            to: CandidateState::Retired
        }
    ));
    Ok(())
}

#[tokio::test]
async fn invalid_transition_built_to_a_promoted_rejects() -> TestResult {
    let sk = signing_key(22);
    let driver = driver(boundary(), &sk);
    let manifest = manifest("linux-6.6.42-aios.13", false);
    let signature = sign_manifest(&manifest, &sk)?;
    let candidate = driver.register_candidate(manifest, signature).await?;

    let err = driver
        .activate_candidate(&candidate.candidate_id)
        .await
        .expect_err("promotion before verification must reject");

    assert!(matches!(
        err,
        RecoveryError::InvalidCandidateTransition {
            from: CandidateState::Built,
            to: CandidateState::APromoted
        }
    ));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_activate_same_candidate_allows_only_one_winner() -> TestResult {
    let sk = signing_key(23);
    let driver = Arc::new(driver(boundary(), &sk));
    let candidate = verified_candidate(&driver, &sk, "linux-6.6.42-aios.14", false).await?;
    let candidate_id = Arc::new(candidate.candidate_id);

    let left_driver = Arc::clone(&driver);
    let left_id = Arc::clone(&candidate_id);
    let right_driver = Arc::clone(&driver);
    let right_id = Arc::clone(&candidate_id);
    let left = tokio::spawn(async move { left_driver.activate_candidate(&left_id).await });
    let right = tokio::spawn(async move { right_driver.activate_candidate(&right_id).await });

    let results = [left.await?, right.await?];
    let success_count = results.iter().filter(|result| result.is_ok()).count();
    let invalid_count = results
        .iter()
        .filter(|result| {
            matches!(
                result,
                Err(RecoveryError::InvalidCandidateTransition {
                    from: CandidateState::APromoted,
                    to: CandidateState::APromoted
                })
            )
        })
        .count();

    assert_eq!(success_count, 1);
    assert_eq!(invalid_count, 1);
    Ok(())
}

#[tokio::test]
async fn get_active_reflects_current_active_id() -> TestResult {
    let sk = signing_key(24);
    let driver = driver(boundary(), &sk);
    assert_eq!(driver.get_active().await, None);

    let first = active_candidate(&driver, &sk, "linux-6.6.42-aios.15", false).await?;
    assert_eq!(driver.get_active().await, Some(first.clone()));

    let second = active_candidate(&driver, &sk, "linux-6.6.42-aios.16", false).await?;
    assert_eq!(driver.get_active().await, Some(second));
    Ok(())
}

#[tokio::test]
async fn list_candidates_returns_registered_candidates() -> TestResult {
    let sk = signing_key(25);
    let driver = driver(boundary(), &sk);
    let first = verified_candidate(&driver, &sk, "linux-6.6.42-aios.17", false).await?;
    let second = verified_candidate(&driver, &sk, "linux-6.6.42-aios.18", false).await?;

    let ids: Vec<CandidateId> = driver
        .list_candidates()
        .await
        .into_iter()
        .map(|candidate| candidate.candidate_id)
        .collect();

    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&first.candidate_id));
    assert!(ids.contains(&second.candidate_id));
    Ok(())
}
