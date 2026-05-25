//! Async verification engine trait and per-run context.

use aios_action::ActionId;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{VerificationError, VerificationIntent, VerificationPrimitive, VerificationResult};

/// Per-call context supplied to a [`VerificationEngine`] run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationContext {
    /// Subject requesting or owning the verification run.
    pub subject: String,
    /// Action whose postcondition is being verified.
    pub action_id: ActionId,
    /// Caller-observed UTC start time for the run.
    pub started_at: DateTime<Utc>,
    /// Caller-supplied timeout budget in whole seconds.
    pub timeout_seconds: u32,
    /// `true` when the run is simulated and must not perform real probes.
    pub dry_run: bool,
}

/// Async S2.4 verification engine contract.
///
/// Implementations are `Send + Sync` so one engine can be shared behind
/// `Arc<dyn VerificationEngine>` by future gRPC and runtime integrations.
#[async_trait]
pub trait VerificationEngine: Send + Sync {
    /// Run one verification intent against the supplied context.
    ///
    /// # Errors
    ///
    /// Returns [`VerificationError`] when the intent expression cannot be parsed,
    /// references a primitive outside the closed S2.4 vocabulary, or a future
    /// primitive implementation fails before producing a result.
    async fn run_verification(
        &self,
        intent: &VerificationIntent,
        context: &VerificationContext,
    ) -> Result<VerificationResult, VerificationError>;

    /// Return the closed S2.4 primitive vocabulary supported by this engine.
    async fn list_primitives(&self) -> Vec<VerificationPrimitive>;
}
