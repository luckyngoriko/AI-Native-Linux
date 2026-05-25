//! T-075 recovery-boundary contract tests for S9.1.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "Integration-test failures should point at the failing contract"
)]

use std::error::Error;
use std::sync::Arc;

use aios_recovery::{
    BootPhase, EnterRecoveryRequest, InMemoryRecoveryBoundary, RecoveryBoundary, RecoveryBundle,
    RecoveryError, RecoveryMode,
};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const AUTHORITY: &str = "aios-recovery-root";

fn fixed_time() -> TestResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339("2026-05-25T10:00:00Z")?.with_timezone(&Utc))
}

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn operator_request(bundle: Option<RecoveryBundle>) -> EnterRecoveryRequest {
    EnterRecoveryRequest {
        reason: "OPERATOR_INITIATED".to_owned(),
        operator_grant: Some("ovr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned()),
        expected_phases: vec![BootPhase::Recovery],
        bundle,
    }
}

fn fallback_request(bundle: Option<RecoveryBundle>) -> EnterRecoveryRequest {
    EnterRecoveryRequest {
        reason: "BOOT_FAILURE_AUTO".to_owned(),
        operator_grant: None,
        expected_phases: vec![BootPhase::Recovery],
        bundle,
    }
}

fn unsigned_bundle(authority: &str) -> TestResult<RecoveryBundle> {
    Ok(RecoveryBundle {
        bundle_id: "rb_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        loaded_at: fixed_time()?,
        hard_deny_signatures: vec!["hard-deny:RecoveryRequiredForSystemMutation".to_owned()],
        override_signatures: vec!["override:STRONG_SOLO".to_owned()],
        signing_authority: authority.to_owned(),
    })
}

#[derive(Serialize)]
struct SignedRecoveryBundleBody<'a> {
    bundle_id: &'a str,
    loaded_at: &'a DateTime<Utc>,
    signing_authority: &'a str,
    hard_deny_signatures: Vec<&'a str>,
    override_signatures: Vec<&'a str>,
}

fn signed_body_bytes(bundle: &RecoveryBundle) -> TestResult<Vec<u8>> {
    let hard_deny_signatures = bundle
        .hard_deny_signatures
        .iter()
        .filter(|value| !value.starts_with("ed25519:"))
        .map(String::as_str)
        .collect();
    let override_signatures = bundle
        .override_signatures
        .iter()
        .filter(|value| !value.starts_with("ed25519:"))
        .map(String::as_str)
        .collect();
    let body = SignedRecoveryBundleBody {
        bundle_id: &bundle.bundle_id,
        loaded_at: &bundle.loaded_at,
        signing_authority: &bundle.signing_authority,
        hard_deny_signatures,
        override_signatures,
    };
    Ok(serde_json::to_vec(&body)?)
}

fn hex_signature(bytes: &[u8; 64]) -> String {
    let mut out = String::with_capacity(128);
    for byte in bytes {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

fn signed_bundle(authority: &str, sk: &SigningKey) -> TestResult<RecoveryBundle> {
    let mut bundle = unsigned_bundle(authority)?;
    let body = signed_body_bytes(&bundle)?;
    let signature = sk.sign(&body);
    bundle
        .hard_deny_signatures
        .push(format!("ed25519:{}", hex_signature(&signature.to_bytes())));
    Ok(bundle)
}

async fn active_exit_token(boundary: &InMemoryRecoveryBoundary) -> TestResult<String> {
    boundary
        .current_exit_token()
        .await
        .ok_or_else(|| RecoveryError::Internal("missing exit token".to_owned()).into())
}

#[tokio::test]
async fn new_boundary_starts_in_normal_mode() -> TestResult {
    let boundary = InMemoryRecoveryBoundary::new();

    let state = boundary.current_state().await;

    assert_eq!(state.mode, RecoveryMode::Normal);
    assert!(!boundary.is_recovery_active().await);
    Ok(())
}

#[tokio::test]
async fn enter_recovery_with_valid_request_sets_recovery_and_mints_exit_token() -> TestResult {
    let boundary = InMemoryRecoveryBoundary::new();

    let state = boundary.enter_recovery(operator_request(None)).await?;

    assert_eq!(state.mode, RecoveryMode::Recovery);
    assert!(active_exit_token(&boundary).await?.starts_with("rexit_"));
    Ok(())
}

#[tokio::test]
async fn enter_recovery_with_bundle_missing_signature_rejects() -> TestResult {
    let sk = signing_key(11);
    let boundary = InMemoryRecoveryBoundary::with_trusted_authority(AUTHORITY, sk.verifying_key());
    let request = operator_request(Some(unsigned_bundle(AUTHORITY)?));

    let err = boundary
        .enter_recovery(request)
        .await
        .expect_err("unsigned recovery bundle must reject");

    assert!(matches!(err, RecoveryError::BundleSignatureInvalid));
    Ok(())
}

#[tokio::test]
async fn enter_recovery_with_bundle_unknown_authority_rejects() -> TestResult {
    let sk = signing_key(12);
    let boundary = InMemoryRecoveryBoundary::new();
    let request = operator_request(Some(signed_bundle("unknown-authority", &sk)?));

    let err = boundary
        .enter_recovery(request)
        .await
        .expect_err("unknown recovery bundle authority must reject");

    assert!(
        matches!(err, RecoveryError::BundleUnknownAuthority(ref name) if name == "unknown-authority")
    );
    Ok(())
}

#[tokio::test]
async fn enter_recovery_with_valid_bundle_signature_succeeds() -> TestResult {
    let sk = signing_key(13);
    let boundary = InMemoryRecoveryBoundary::with_trusted_authority(AUTHORITY, sk.verifying_key());
    let request = operator_request(Some(signed_bundle(AUTHORITY, &sk)?));

    let state = boundary.enter_recovery(request).await?;

    assert_eq!(state.mode, RecoveryMode::Recovery);
    Ok(())
}

#[tokio::test]
async fn enter_recovery_when_already_in_recovery_rejects() -> TestResult {
    let boundary = InMemoryRecoveryBoundary::new();
    let _state = boundary.enter_recovery(operator_request(None)).await?;

    let err = boundary
        .enter_recovery(operator_request(None))
        .await
        .expect_err("re-entry must reject");

    assert!(matches!(err, RecoveryError::AlreadyInRecovery));
    Ok(())
}

#[tokio::test]
async fn enter_recovery_without_operator_grant_or_fallback_rejects() -> TestResult {
    let boundary = InMemoryRecoveryBoundary::new();
    let mut request = operator_request(None);
    request.operator_grant = None;

    let err = boundary
        .enter_recovery(request)
        .await
        .expect_err("operator-initiated recovery without grant must reject");

    assert!(matches!(
        err,
        RecoveryError::RecoveryAuthorizationInvalid(_)
    ));
    Ok(())
}

#[tokio::test]
async fn enter_recovery_with_spec_fallback_reason_without_operator_grant_succeeds() -> TestResult {
    let boundary = InMemoryRecoveryBoundary::new();

    let state = boundary.enter_recovery(fallback_request(None)).await?;

    assert_eq!(state.mode, RecoveryMode::Recovery);
    assert_eq!(state.operator_grant, None);
    Ok(())
}

#[tokio::test]
async fn exit_recovery_with_wrong_token_rejects() -> TestResult {
    let boundary = InMemoryRecoveryBoundary::new();
    let _state = boundary.enter_recovery(operator_request(None)).await?;

    let err = boundary
        .exit_recovery("wrong-token")
        .await
        .expect_err("wrong exit token must reject");

    assert!(matches!(
        err,
        RecoveryError::RecoveryAuthorizationInvalid(_)
    ));
    assert_eq!(boundary.current_state().await.mode, RecoveryMode::Recovery);
    Ok(())
}

#[tokio::test]
async fn exit_recovery_with_correct_token_returns_to_normal() -> TestResult {
    let boundary = InMemoryRecoveryBoundary::new();
    let _state = boundary.enter_recovery(operator_request(None)).await?;
    let token = active_exit_token(&boundary).await?;

    let state = boundary.exit_recovery(&token).await?;

    assert_eq!(state.mode, RecoveryMode::Normal);
    assert!(boundary.current_exit_token().await.is_none());
    Ok(())
}

#[tokio::test]
async fn exit_recovery_when_not_active_rejects() -> TestResult {
    let boundary = InMemoryRecoveryBoundary::new();

    let err = boundary
        .exit_recovery("unused")
        .await
        .expect_err("normal-mode exit must reject");

    assert!(matches!(err, RecoveryError::RecoveryNotActive));
    Ok(())
}

#[tokio::test]
async fn current_state_returns_current_mode() -> TestResult {
    let boundary = InMemoryRecoveryBoundary::new();
    assert_eq!(boundary.current_state().await.mode, RecoveryMode::Normal);

    let _state = boundary.enter_recovery(operator_request(None)).await?;

    assert_eq!(boundary.current_state().await.mode, RecoveryMode::Recovery);
    Ok(())
}

#[tokio::test]
async fn is_recovery_active_is_true_only_while_mode_is_recovery() -> TestResult {
    let boundary = InMemoryRecoveryBoundary::new();
    assert!(!boundary.is_recovery_active().await);

    let _state = boundary.enter_recovery(operator_request(None)).await?;
    assert!(boundary.is_recovery_active().await);

    let token = active_exit_token(&boundary).await?;
    let _state = boundary.exit_recovery(&token).await?;
    assert!(!boundary.is_recovery_active().await);
    Ok(())
}

#[tokio::test]
async fn end_to_end_through_trait_object() -> TestResult {
    let concrete = Arc::new(InMemoryRecoveryBoundary::new());
    let boundary: Arc<dyn RecoveryBoundary> = concrete.clone();

    let entered = boundary.enter_recovery(operator_request(None)).await?;
    let token = active_exit_token(&concrete).await?;
    let exited = boundary.exit_recovery(&token).await?;

    assert_eq!(entered.mode, RecoveryMode::Recovery);
    assert_eq!(exited.mode, RecoveryMode::Normal);
    Ok(())
}
