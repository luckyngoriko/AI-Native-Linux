//! [`RollbackDriver`] — async rollback FSM driver per S10.1 §7.
//!
//! After a `FAILED` transition for an action whose adapter declared
//! `rollback_strategy != NONE`, the runtime calls the adapter's
//! `Rollback(action_request_id, rollback_handle)` RPC. The driver maps the
//! returned [`RollbackOutcome`] onto the §4.2 transition table per the §7.2
//! outcome table:
//!
//! | `RollbackOutcome` | Lifecycle transition | Evidence retention | Operator alert |
//! | ----------------- | -------------------- | ------------------ | -------------- |
//! | `Succeeded`       | `FAILED → ROLLED_BACK` (T19) | `STANDARD_24M` | No |
//! | `Failed`          | `FAILED → ROLLBACK_FAILED` (T20) | `FOREVER` | Yes |
//! | `NotApplicable`   | `FAILED` stays | `STANDARD_24M` | No |
//! | `NotAttempted`    | `FAILED` stays | `STANDARD_24M` | No |
//!
//! Per §7.4, `ROLLBACK_FAILED` is a strict terminal state: there is no
//! auto-retry. The constitutional posture is "stop, alert, let an operator
//! inspect" — a failed rollback means the system holds an unknown partial
//! state and re-running rollback would compound the unknown.
//!
//! ## T-032 production seam
//!
//! T-032 does not yet wire a real adapter `Rollback(...)` RPC: the
//! production adapter dispatch lands in M5 (real systemd / dnf / fs
//! adapters). Until then, the driver is exercised via
//! [`RollbackFailureMode`] — a deterministic injection enum the test
//! harness selects. When [`RollbackDriver`] is configured with
//! [`RollbackFailureMode::SucceedSimulated`] (the default), every
//! attempted rollback returns [`RollbackOutcome::Succeeded`]; with
//! [`RollbackFailureMode::FailSimulated`], every attempted rollback
//! returns [`RollbackOutcome::Failed`]. The seam preserves the driver's
//! production shape (`async fn run_rollback`, full §7.2 outcome
//! discrimination) without prematurely binding the dispatch surface.

use aios_action::ActionEnvelope;

use crate::adapter_handle::RealAdapterHandle;
use crate::context::ActionContext;
use crate::failure::RollbackOutcome;
use crate::rollback_strategy::{RollbackFailureMode, RollbackStrategy};
use crate::status::ActionLifecycleState;

/// `RollbackDriver` — stateless async driver for the §7 rollback FSM.
///
/// The driver holds a configuration-only `failure_mode` knob today (the
/// T-032 test seam — see module docs). In M5 the driver will additionally
/// hold a timeout budget and a structured logging surface; the
/// [`Self::new_with_defaults`] ctor preserves the additive constructor
/// pattern this crate uses.
///
/// The driver itself is `Send + Sync + Clone` so a single instance can be
/// shared across `tokio` worker tasks behind `Arc<RollbackDriver>` —
/// matching the composition pattern of [`crate::EvidenceEmitter`] and
/// [`crate::dispatch_queue::DispatchQueue`].
#[derive(Debug, Clone, Default)]
pub struct RollbackDriver {
    /// Simulated outcome of the adapter's `Rollback(...)` RPC for the
    /// T-032 test seam. See module docs for the production-replacement
    /// plan in M5.
    failure_mode: RollbackFailureMode,
    /// When `true`, the pipeline's verify step is forced to drive
    /// `EXECUTING → VERIFYING → FAILED` (T15 + T18) instead of the
    /// happy-path `EXECUTING → VERIFYING → SUCCEEDED` (T15 + T17). This
    /// is the T-032 test seam for exercising the rollback FSM end-to-end
    /// without a real verification engine (which lands in T-035). In M5
    /// the real verification engine produces real verification failures
    /// against the adapter's simulation transcript and this knob
    /// disappears.
    inject_verify_failure: bool,
}

impl RollbackDriver {
    /// Construct the default driver: [`RollbackFailureMode::SucceedSimulated`].
    ///
    /// This is the seed for production wiring once M5 lands. Until then,
    /// the M4 §22 golden path relies on the default to drive the
    /// `FAILED → ROLLED_BACK` happy path when verification injects a
    /// failure.
    #[must_use]
    pub const fn new_with_defaults() -> Self {
        Self {
            failure_mode: RollbackFailureMode::SucceedSimulated,
            inject_verify_failure: false,
        }
    }

    /// Configure the simulated outcome of the adapter rollback. Returns
    /// `self` for chaining.
    ///
    /// **T-032 test seam.** See module docs.
    #[must_use]
    pub const fn with_failure_mode(mut self, mode: RollbackFailureMode) -> Self {
        self.failure_mode = mode;
        self
    }

    /// Borrow the configured failure mode (for tests and forensic logging).
    #[must_use]
    pub const fn failure_mode(&self) -> RollbackFailureMode {
        self.failure_mode
    }

    /// Configure the verify-failure injection knob. Returns `self` for
    /// chaining. **T-032 test seam** — see field-level docs on
    /// [`Self::inject_verify_failure`].
    #[must_use]
    pub const fn with_inject_verify_failure(mut self, inject: bool) -> Self {
        self.inject_verify_failure = inject;
        self
    }

    /// `true` when the driver is configured to inject a verification
    /// failure (drives `VERIFYING → FAILED` so step 7 engages). T-032
    /// test seam.
    #[must_use]
    pub const fn inject_verify_failure(&self) -> bool {
        self.inject_verify_failure
    }

    /// Drive one rollback attempt per S10.1 §7.2.
    ///
    /// Pure async function. Does **not** mutate the [`ActionContext`]; the
    /// caller (the pipeline's `step_rollback`) applies the §4.2 transition
    /// based on the returned [`RollbackOutcome`] via
    /// [`Self::classify_terminal`].
    ///
    /// Strategy → outcome mapping:
    ///
    /// - [`RollbackStrategy::None`] / [`RollbackStrategy::Unspecified`] —
    ///   never attempts a rollback; returns [`RollbackOutcome::NotAttempted`].
    /// - [`RollbackStrategy::ExternalRequired`] — the runtime cannot
    ///   roll back; returns [`RollbackOutcome::NotApplicable`] per §7.2's
    ///   "`EXTERNAL_REQUIRED` is treated as `NOT_APPLICABLE`" rule.
    /// - [`RollbackStrategy::IdempotentReverse`] /
    ///   [`RollbackStrategy::CheckpointBased`] — simulates the adapter's
    ///   `Rollback(...)` RPC via [`Self::failure_mode`].
    ///
    /// `_envelope`, `_ctx`, `_adapter` are accepted but unused today; they
    /// are listed in the signature so the M5 production wiring is a pure
    /// body swap (the trait surface stays bit-for-bit identical).
    #[allow(
        clippy::unused_async,
        reason = "trait surface preserved across the M5 production wiring; the real adapter Rollback(...) RPC is async by construction"
    )]
    pub async fn run_rollback(
        &self,
        _envelope: &ActionEnvelope,
        _ctx: &ActionContext,
        strategy: RollbackStrategy,
        _adapter: &RealAdapterHandle,
    ) -> RollbackOutcome {
        match strategy {
            RollbackStrategy::None | RollbackStrategy::Unspecified => RollbackOutcome::NotAttempted,
            RollbackStrategy::ExternalRequired => RollbackOutcome::NotApplicable,
            RollbackStrategy::IdempotentReverse | RollbackStrategy::CheckpointBased => {
                match self.failure_mode {
                    RollbackFailureMode::SucceedSimulated => RollbackOutcome::Succeeded,
                    RollbackFailureMode::FailSimulated => RollbackOutcome::Failed,
                }
            }
        }
    }

    /// Map a [`RollbackOutcome`] onto the lifecycle terminal per §7.2's
    /// outcome table.
    ///
    /// - `Succeeded` → [`ActionLifecycleState::RolledBack`] (T19 endpoint).
    /// - `Failed` → [`ActionLifecycleState::RollbackFailed`] (T20 endpoint;
    ///   strict terminal per §7.4).
    /// - `NotApplicable` / `NotAttempted` → [`ActionLifecycleState::Failed`]
    ///   (the FSM stays in the upstream failure terminal; the runtime
    ///   emits `ROLLBACK_ATTEMPTED` with the `note` discriminator).
    ///
    /// Pure function; the §4.2 transition table is enforced by the
    /// pipeline's [`crate::apply_transition`].
    #[must_use]
    pub const fn classify_terminal(outcome: &RollbackOutcome) -> ActionLifecycleState {
        match outcome {
            RollbackOutcome::Succeeded => ActionLifecycleState::RolledBack,
            RollbackOutcome::Failed => ActionLifecycleState::RollbackFailed,
            RollbackOutcome::NotAttempted | RollbackOutcome::NotApplicable => {
                ActionLifecycleState::Failed
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use aios_action::{ActionEnvelope, Identity, Request, Trace};
    use chrono::Utc;
    use std::sync::Arc;

    fn envelope() -> ActionEnvelope {
        ActionEnvelope::new(
            Identity::new("subject:human:test", false),
            Request::new("service.restart", serde_json::json!({"name": "nginx"})),
            Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
        )
    }

    fn dummy_adapter() -> RealAdapterHandle {
        use crate::adapter_manifest::{AdapterActionDeclaration, AdapterManifest};
        use crate::dispatch::{ActionDispatchKind, AdapterIOMode, AdapterStability};
        use chrono::Duration;
        let now = Utc::now();
        let manifest = AdapterManifest {
            adapter_id: "adapter:test:dummy:0.1.0".into(),
            adapter_version: "0.1.0".into(),
            vendor: "test".into(),
            name: "dummy".into(),
            declared_stability: AdapterStability::Stable,
            io_mode: AdapterIOMode::TypedParametersOnly,
            dispatch_kind: ActionDispatchKind::SubprocessFork,
            declared_actions: vec![AdapterActionDeclaration {
                action_kind: "service.restart".into(),
                target_schema: serde_json::json!({"type": "object"}),
                response_schema: serde_json::json!({"type": "object"}),
                rollback_strategy: "IDEMPOTENT_REVERSE".into(),
                timeout_seconds: 30,
                template_string: None,
                template_substitution_variables: vec![],
            }],
            declared_invariants_supported: vec!["INV-013".into()],
            default_adapter_timeout_seconds: 60,
            default_sandbox_profile_id: "default".into(),
            adapter_signature: String::new(),
            signing_key_id: "publisher:test".into(),
            manifest_created_at: now,
            manifest_expires_at: now + Duration::days(1),
        };
        RealAdapterHandle::new(Arc::new(manifest))
    }

    fn ctx() -> ActionContext {
        use crate::dispatch::QueueClass;
        use crate::pipeline::fresh_context;
        let mut c = fresh_context(aios_action::ActionId::new(), Utc::now());
        c.status = ActionLifecycleState::Failed;
        c.queue_class = QueueClass::Interactive;
        c
    }

    #[tokio::test]
    async fn strategy_none_returns_not_attempted() {
        let d = RollbackDriver::new_with_defaults();
        let out = d
            .run_rollback(
                &envelope(),
                &ctx(),
                RollbackStrategy::None,
                &dummy_adapter(),
            )
            .await;
        assert_eq!(out, RollbackOutcome::NotAttempted);
    }

    #[tokio::test]
    async fn strategy_unspecified_returns_not_attempted() {
        let d = RollbackDriver::new_with_defaults();
        let out = d
            .run_rollback(
                &envelope(),
                &ctx(),
                RollbackStrategy::Unspecified,
                &dummy_adapter(),
            )
            .await;
        assert_eq!(out, RollbackOutcome::NotAttempted);
    }

    #[tokio::test]
    async fn strategy_external_required_returns_not_applicable() {
        let d = RollbackDriver::new_with_defaults();
        let out = d
            .run_rollback(
                &envelope(),
                &ctx(),
                RollbackStrategy::ExternalRequired,
                &dummy_adapter(),
            )
            .await;
        assert_eq!(out, RollbackOutcome::NotApplicable);
    }

    #[tokio::test]
    async fn strategy_idempotent_reverse_succeeds_by_default() {
        let d = RollbackDriver::new_with_defaults();
        let out = d
            .run_rollback(
                &envelope(),
                &ctx(),
                RollbackStrategy::IdempotentReverse,
                &dummy_adapter(),
            )
            .await;
        assert_eq!(out, RollbackOutcome::Succeeded);
    }

    #[tokio::test]
    async fn strategy_idempotent_reverse_fails_when_injected() {
        let d = RollbackDriver::new_with_defaults()
            .with_failure_mode(RollbackFailureMode::FailSimulated);
        let out = d
            .run_rollback(
                &envelope(),
                &ctx(),
                RollbackStrategy::IdempotentReverse,
                &dummy_adapter(),
            )
            .await;
        assert_eq!(out, RollbackOutcome::Failed);
    }

    #[tokio::test]
    async fn strategy_checkpoint_based_succeeds_by_default() {
        let d = RollbackDriver::new_with_defaults();
        let out = d
            .run_rollback(
                &envelope(),
                &ctx(),
                RollbackStrategy::CheckpointBased,
                &dummy_adapter(),
            )
            .await;
        assert_eq!(out, RollbackOutcome::Succeeded);
    }

    #[test]
    fn classify_terminal_truth_table() {
        assert_eq!(
            RollbackDriver::classify_terminal(&RollbackOutcome::Succeeded),
            ActionLifecycleState::RolledBack
        );
        assert_eq!(
            RollbackDriver::classify_terminal(&RollbackOutcome::Failed),
            ActionLifecycleState::RollbackFailed
        );
        assert_eq!(
            RollbackDriver::classify_terminal(&RollbackOutcome::NotAttempted),
            ActionLifecycleState::Failed
        );
        assert_eq!(
            RollbackDriver::classify_terminal(&RollbackOutcome::NotApplicable),
            ActionLifecycleState::Failed
        );
    }
}
