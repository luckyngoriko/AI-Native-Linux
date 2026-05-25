//! In-memory S9.1 recovery-boundary harness.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Serialize;
use tokio::sync::RwLock;
use ulid::Ulid;

use crate::boundary::{EnterRecoveryRequest, RecoveryBoundary};
use crate::{RecoveryBundle, RecoveryError, RecoveryMode, RecoveryState};

const BUNDLE_SIGNATURE_PREFIX: &str = "ed25519:";

/// HashMap-backed recovery boundary used by tests and future service adapters.
pub struct InMemoryRecoveryBoundary {
    state: RwLock<RecoveryState>,
    trusted_authorities: HashMap<String, VerifyingKey>,
    active_exit_token: RwLock<Option<String>>,
}

impl Default for InMemoryRecoveryBoundary {
    fn default() -> Self {
        Self {
            state: RwLock::new(normal_state()),
            trusted_authorities: HashMap::new(),
            active_exit_token: RwLock::new(None),
        }
    }
}

impl InMemoryRecoveryBoundary {
    /// Construct a boundary in `NORMAL` mode with an empty trust store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a boundary trusting one recovery-bundle signing authority.
    #[must_use]
    pub fn with_trusted_authority(name: impl Into<String>, key: VerifyingKey) -> Self {
        let mut boundary = Self::new();
        boundary.trusted_authorities.insert(name.into(), key);
        boundary
    }

    /// Return the active opaque exit token, if recovery is active.
    ///
    /// The S9.1 type surface returns [`RecoveryState`] from `enter_recovery`;
    /// this in-memory harness exposes the minted token for tests and future
    /// adapters that need to hand it to an operator UI.
    pub async fn current_exit_token(&self) -> Option<String> {
        self.active_exit_token.read().await.clone()
    }

    fn verify_bundle(&self, bundle: &RecoveryBundle) -> Result<(), RecoveryError> {
        let verifying_key = self
            .trusted_authorities
            .get(&bundle.signing_authority)
            .ok_or_else(|| {
                RecoveryError::BundleUnknownAuthority(bundle.signing_authority.clone())
            })?;
        let signature = bundle_signature(bundle)?;
        let body = canonical_bundle_body_bytes(bundle)?;
        verifying_key
            .verify(&body, &signature)
            .map_err(|_| RecoveryError::BundleSignatureInvalid)
    }
}

#[async_trait]
impl RecoveryBoundary for InMemoryRecoveryBoundary {
    async fn enter_recovery(
        &self,
        request: EnterRecoveryRequest,
    ) -> Result<RecoveryState, RecoveryError> {
        let mut state = self.state.write().await;
        if state.mode == RecoveryMode::Recovery {
            return Err(RecoveryError::AlreadyInRecovery);
        }
        validate_entry_request(&request)?;
        if let Some(bundle) = &request.bundle {
            self.verify_bundle(bundle)?;
        }

        let exit_token = format!("rexit_{}", Ulid::new());
        let next_state = RecoveryState {
            mode: RecoveryMode::Recovery,
            entered_at: Some(Utc::now()),
            exit_planned_at: None,
            reason: Some(request.reason),
            operator_grant: request.operator_grant,
        };
        *state = next_state.clone();
        drop(state);
        let mut active_exit_token = self.active_exit_token.write().await;
        *active_exit_token = Some(exit_token);
        drop(active_exit_token);
        Ok(next_state)
    }

    async fn exit_recovery(&self, exit_token: &str) -> Result<RecoveryState, RecoveryError> {
        let mut state = self.state.write().await;
        if state.mode != RecoveryMode::Recovery {
            return Err(RecoveryError::RecoveryNotActive);
        }

        let mut active_exit_token = self.active_exit_token.write().await;
        if active_exit_token.as_deref() != Some(exit_token) {
            return Err(RecoveryError::RecoveryAuthorizationInvalid(
                "exit token mismatch".to_owned(),
            ));
        }

        let next_state = normal_state();
        *state = next_state.clone();
        drop(state);
        *active_exit_token = None;
        drop(active_exit_token);
        Ok(next_state)
    }

    async fn current_state(&self) -> RecoveryState {
        self.state.read().await.clone()
    }

    async fn is_recovery_active(&self) -> bool {
        self.state.read().await.mode == RecoveryMode::Recovery
    }
}

const fn normal_state() -> RecoveryState {
    RecoveryState {
        mode: RecoveryMode::Normal,
        entered_at: None,
        exit_planned_at: None,
        reason: None,
        operator_grant: None,
    }
}

fn validate_entry_request(request: &EnterRecoveryRequest) -> Result<(), RecoveryError> {
    if !request
        .expected_phases
        .contains(&crate::BootPhase::Recovery)
    {
        return Err(RecoveryError::RecoveryAuthorizationInvalid(
            "expected phases must include RECOVERY".to_owned(),
        ));
    }
    let has_operator_grant = request
        .operator_grant
        .as_deref()
        .is_some_and(|grant| !grant.trim().is_empty());
    if has_operator_grant || is_s91_fallback_reason(&request.reason) {
        return Ok(());
    }
    Err(RecoveryError::RecoveryAuthorizationInvalid(
        "operator grant or S9.1 fallback entry reason required".to_owned(),
    ))
}

fn is_s91_fallback_reason(reason: &str) -> bool {
    matches!(
        reason.trim(),
        "BOOT_FAILURE_AUTO"
            | "INVARIANT_BUNDLE_SIGNATURE_FAILURE"
            | "POLICY_BUNDLE_CORRUPTION"
            | "AIOSFS_ROOT_UNRESOLVABLE"
            | "VAULT_ROOT_KEY_UNAVAILABLE"
            | "IDENTITY_BUNDLE_FAILURE"
            | "EVIDENCE_LOG_TAMPER_DETECTED"
            | "KERNEL_IMAGE_TAMPER_DETECTED"
            | "FIRMWARE_TAMPER_DETECTED"
    )
}

#[derive(Serialize)]
struct SignedRecoveryBundleBody<'a> {
    bundle_id: &'a str,
    loaded_at: &'a DateTime<Utc>,
    signing_authority: &'a str,
    hard_deny_signatures: Vec<&'a str>,
    override_signatures: Vec<&'a str>,
}

fn canonical_bundle_body_bytes(bundle: &RecoveryBundle) -> Result<Vec<u8>, RecoveryError> {
    let hard_deny_signatures = signed_material(&bundle.hard_deny_signatures);
    let override_signatures = signed_material(&bundle.override_signatures);
    let body = SignedRecoveryBundleBody {
        bundle_id: &bundle.bundle_id,
        loaded_at: &bundle.loaded_at,
        signing_authority: &bundle.signing_authority,
        hard_deny_signatures,
        override_signatures,
    };
    serde_json::to_vec(&body)
        .map_err(|err| RecoveryError::Internal(format!("recovery bundle serialise: {err}")))
}

fn signed_material(values: &[String]) -> Vec<&str> {
    values
        .iter()
        .filter(|value| !value.starts_with(BUNDLE_SIGNATURE_PREFIX))
        .map(String::as_str)
        .collect()
}

fn bundle_signature(bundle: &RecoveryBundle) -> Result<Signature, RecoveryError> {
    let mut signatures = bundle
        .hard_deny_signatures
        .iter()
        .chain(bundle.override_signatures.iter())
        .filter_map(|value| value.strip_prefix(BUNDLE_SIGNATURE_PREFIX));
    let Some(signature_hex) = signatures.next() else {
        return Err(RecoveryError::BundleSignatureInvalid);
    };
    if signatures.next().is_some() {
        return Err(RecoveryError::BundleSignatureInvalid);
    }
    let signature_bytes = decode_hex_signature(signature_hex)?;
    Ok(Signature::from_bytes(&signature_bytes))
}

fn decode_hex_signature(hex: &str) -> Result<[u8; 64], RecoveryError> {
    if hex.len() != 128 {
        return Err(RecoveryError::BundleSignatureInvalid);
    }
    let mut bytes = [0_u8; 64];
    for (idx, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        bytes[idx] = (hex_value(pair[0])? << 4) | hex_value(pair[1])?;
    }
    Ok(bytes)
}

const fn hex_value(byte: u8) -> Result<u8, RecoveryError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(RecoveryError::BundleSignatureInvalid),
    }
}
