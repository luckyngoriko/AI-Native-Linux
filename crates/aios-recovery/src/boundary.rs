//! Async S9.1 recovery-boundary contract.

use async_trait::async_trait;

use crate::{BootPhase, RecoveryBundle, RecoveryError, RecoveryState};

/// Request to enter S9.1 recovery mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnterRecoveryRequest {
    /// Closed S9.1 recovery-entry reason label.
    pub reason: String,
    /// Optional S5.4 `OverrideBinding` id authorising operator-initiated entry.
    pub operator_grant: Option<String>,
    /// Boot phases the caller expects the recovery path to cover.
    pub expected_phases: Vec<BootPhase>,
    /// Optional degraded-subset recovery bundle loaded at entry.
    pub bundle: Option<RecoveryBundle>,
}

/// Recovery boundary surface used by S9.1 callers.
#[async_trait]
pub trait RecoveryBoundary: Send + Sync {
    /// Enter recovery mode.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError`] when recovery is already active, the request
    /// lacks a valid operator grant or documented fallback reason, or a supplied
    /// recovery bundle fails trust-chain verification.
    async fn enter_recovery(
        &self,
        request: EnterRecoveryRequest,
    ) -> Result<RecoveryState, RecoveryError>;

    /// Exit recovery mode using the opaque exit token minted at entry.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::RecoveryNotActive`] when recovery is not active
    /// or [`RecoveryError::RecoveryAuthorizationInvalid`] when the token does
    /// not match the active recovery session.
    async fn exit_recovery(&self, exit_token: &str) -> Result<RecoveryState, RecoveryError>;

    /// Return the currently observed recovery state.
    async fn current_state(&self) -> RecoveryState;

    /// Return `true` only when the current mode is `RECOVERY`.
    async fn is_recovery_active(&self) -> bool;
}
