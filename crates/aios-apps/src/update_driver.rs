//! S12.2 §rollback — UpdateRollbackDriver async trait + InMemoryUpdateDriver.
//!
//! Drives a planned package update through the 11-state UpdateState FSM:
//! Planned→Executing→Executed→Verifying→Verified→Activating→Active,
//! with rollback for failure, verification mismatch, policy revocation,
//! user request, and regression detection.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};
use tokio::sync::RwLock;

use crate::error::AppsError;
use crate::evidence::{AppsEvidenceEmitter, UpdatePhaseRecord};
use crate::package::PackageId;
use crate::session_driver::Principal;

// ---------------------------------------------------------------------------
// UpdatePlanId
// ---------------------------------------------------------------------------

/// Canonical update plan identifier. Format: `updp_<ulid26>`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UpdatePlanId(pub String);

// ---------------------------------------------------------------------------
// UpdateState — 11-variant FSM for package updates
// ---------------------------------------------------------------------------

/// Mirrors the `aios-capability-runtime` 14-state forensic pattern in reduced
/// form for package update lifecycle tracking.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UpdateState {
    /// Plan created; not yet executing.
    Planned,
    /// Update execution is in progress.
    Executing,
    /// Update binaries have been staged.
    Executed,
    /// Verification is in progress.
    Verifying,
    /// Verification has passed.
    Verified,
    /// Activation is in progress.
    Activating,
    /// Update is live and active.
    Active,
    /// Transitioned to a terminal failure with a classified reason.
    Failed,
    /// Rollback is in progress.
    RollingBack,
    /// Rollback completed successfully.
    RolledBack,
    /// Rollback failed; manual intervention required.
    RollbackFailed,
}

// ---------------------------------------------------------------------------
// FailureClass
// ---------------------------------------------------------------------------

/// Classification carried inside `UpdateState::Failed`.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FailureClass {
    /// The execute step failed.
    ExecuteError,
    /// Verification hash mismatch detected.
    VerifyMismatch,
    /// Activation step failed.
    ActivateError,
    /// Policy denied the update.
    PolicyDenied,
}

// ---------------------------------------------------------------------------
// Request / plan / outcome types
// ---------------------------------------------------------------------------

/// Request to plan a package update.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdatePlanRequest {
    /// Target package identifier.
    pub package_id: PackageId,
    /// Current installed version.
    pub from_version: String,
    /// Target version to update to.
    pub to_version: String,
    /// Who requested the update.
    pub requester: Principal,
    /// When true the plan is returned but not persisted.
    pub dry_run: bool,
}

/// An update plan tracking a single version transition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdatePlan {
    /// Plan identifier.
    pub id: UpdatePlanId,
    /// Target package.
    pub package_id: PackageId,
    /// Source version.
    pub from_version: String,
    /// Target version.
    pub to_version: String,
    /// Current FSM state.
    pub state: UpdateState,
    /// Optional failure class when `state` is `Failed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_class: Option<FailureClass>,
    /// When this plan was created.
    pub created_at: DateTime<Utc>,
    /// When the state last changed.
    pub state_changed_at: DateTime<Utc>,
}

/// Result of a successful execute step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateOutcome {
    /// Execution metrics as opaque JSON (perf counters, elapsed wall time, etc.).
    pub execution_metrics: serde_json::Value,
    /// Number of artifact files swapped into the staged directory.
    pub artifacts_swapped: u32,
}

/// Result of a verification step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateVerification {
    /// `true` when content hash matches the expected value.
    pub hash_match: bool,
    /// List of capability declarations that differ from the prior version.
    pub capability_drift: Vec<String>,
    /// Compatibility rating 0–100 with the profile declared in the manifest.
    pub profile_compat: u8,
}

// ---------------------------------------------------------------------------
// Rollback types
// ---------------------------------------------------------------------------

/// Who or what triggered the rollback.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RollbackReason {
    /// Verification failed (hash mismatch / probe failure).
    VerifyFailed,
    /// Policy kernel revoked the approval.
    PolicyRevoked,
    /// Operator explicitly requested rollback.
    UserRequested,
    /// Runtime regression was detected post-activation.
    RegressionDetected,
}

/// Exit state recorded in a rollback receipt.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RollbackExitState {
    /// All artifacts reverted to the prior version.
    Reverted,
    /// Some artifacts reverted; partial state remains.
    PartialRevert,
    /// Rollback failed; the system remains in the failing state.
    RollbackFailed,
}

/// Receipt emitted after a rollback attempt completes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollbackReceipt {
    /// The plan that was rolled back.
    pub plan_id: UpdatePlanId,
    /// Version string reverted to.
    pub reverted_to: String,
    /// Wall-clock completion time.
    pub completed_at: DateTime<Utc>,
    /// Final exit state.
    pub exit_state: RollbackExitState,
}

// ---------------------------------------------------------------------------
// UpdateRollbackDriver trait
// ---------------------------------------------------------------------------

/// S12.2 §rollback — async contract for driving a package update through the
/// plan → execute → verify → activate lifecycle with rollback support.
///
/// Every state transition is validated against the closed `UpdateState` FSM;
/// invalid transitions produce [`AppsError::InvalidStateTransition`].
#[async_trait]
pub trait UpdateRollbackDriver: Send + Sync {
    /// Plan a new update. Returns the plan in `Planned` state.
    ///
    /// When `req.dry_run` is `true`, the plan is returned but NOT persisted.
    async fn plan_update(&self, req: UpdatePlanRequest) -> Result<UpdatePlan, AppsError>;

    /// Execute a planned update: stages binaries, runs pre-install checks.
    /// Transitions `Planned → Executing → Executed` (or `Failed(ExecuteError)`).
    async fn execute_update(&self, plan_id: UpdatePlanId) -> Result<UpdateOutcome, AppsError>;

    /// Verify the executed update: content hash, capability drift, profile compat.
    /// Transitions `Executed → Verifying → Verified` (or `Failed(VerifyMismatch)`).
    async fn verify_update(&self, plan_id: UpdatePlanId) -> Result<UpdateVerification, AppsError>;

    /// Activate a verified update, promoting it to the active version.
    /// Transitions `Verified → Activating → Active`.
    async fn activate_update(&self, plan_id: UpdatePlanId) -> Result<(), AppsError>;

    /// Rollback a plan from `Failed`, `Active`, or `Verified` to `RolledBack`.
    /// Transitions through `RollingBack → RolledBack` (or `RollbackFailed`).
    async fn rollback_update(
        &self,
        plan_id: UpdatePlanId,
        reason: RollbackReason,
    ) -> Result<RollbackReceipt, AppsError>;

    /// Return the current plan state (read-only).
    async fn get_update(&self, plan_id: UpdatePlanId) -> Result<UpdatePlan, AppsError>;
}

// ---------------------------------------------------------------------------
// InMemoryUpdateDriver
// ---------------------------------------------------------------------------

/// In-memory `UpdateRollbackDriver` harness backed by `RwLock<HashMap<...>>`.
///
/// Dry-run plans are not persisted; live plans are stored and transitioned
/// through the FSM with full transition validation.
#[derive(Clone, Default)]
pub struct InMemoryUpdateDriver {
    plans: Arc<RwLock<HashMap<UpdatePlanId, UpdatePlan>>>,
    emitter: Option<Arc<dyn AppsEvidenceEmitter>>,
}

impl std::fmt::Debug for InMemoryUpdateDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryUpdateDriver")
            .field("plans", &self.plans)
            .finish_non_exhaustive()
    }
}

impl InMemoryUpdateDriver {
    /// Create an empty driver.
    #[must_use]
    pub fn new() -> Self {
        Self {
            plans: Arc::new(RwLock::new(HashMap::new())),
            emitter: None,
        }
    }

    /// Attach an evidence emitter to this driver.
    ///
    /// After this call, every successful phase transition will emit an
    /// evidence record.
    #[must_use]
    pub fn with_emitter(mut self, emitter: Arc<dyn AppsEvidenceEmitter>) -> Self {
        self.emitter = Some(emitter);
        self
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

impl InMemoryUpdateDriver {
    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn next_plan_id() -> UpdatePlanId {
        UpdatePlanId(format!(
            "updp_{}",
            ulid::Ulid::new().to_string().to_lowercase()
        ))
    }

    /// Load a plan, erroring with `UpdatePlanNotFound` when absent.
    async fn load_plan(&self, plan_id: &UpdatePlanId) -> Result<UpdatePlan, AppsError> {
        self.plans
            .read()
            .await
            .get(plan_id)
            .cloned()
            .ok_or_else(|| AppsError::UpdatePlanNotFound(plan_id.0.clone()))
    }

    /// Persist a plan under write lock.
    async fn store_plan(&self, plan: UpdatePlan) {
        self.plans.write().await.insert(plan.id.clone(), plan);
    }
}

// ---------------------------------------------------------------------------
// FSM transition validation
// ---------------------------------------------------------------------------

/// Check whether the transition `from → to` is legal in the update FSM.
const fn is_legal_transition(from: UpdateState, to: UpdateState) -> bool {
    use UpdateState::{
        Activating, Active, Executed, Executing, Failed, Planned, RollbackFailed, RolledBack,
        RollingBack, Verified, Verifying,
    };
    matches!(
        (from, to),
        (Planned, Executing | Failed)
            | (Executing, Executed | Failed)
            | (Executed, Verifying | RollingBack)
            | (Verifying, Verified | Failed)
            | (Verified, Activating | RollingBack)
            | (Activating, Active | Failed)
            | (Failed | Active, RollingBack)
            | (RollingBack, RolledBack | RollbackFailed),
    )
}

/// Transition a plan's state with validation.
fn apply_transition(plan: &mut UpdatePlan, to: UpdateState) -> Result<(), AppsError> {
    if !is_legal_transition(plan.state, to) {
        return Err(AppsError::InvalidStateTransition {
            from: plan.state.to_string(),
            to: to.to_string(),
        });
    }
    plan.state = to;
    plan.state_changed_at = Utc::now();
    Ok(())
}

// ---------------------------------------------------------------------------
// Trait impl
// ---------------------------------------------------------------------------

#[async_trait]
impl UpdateRollbackDriver for InMemoryUpdateDriver {
    async fn plan_update(&self, req: UpdatePlanRequest) -> Result<UpdatePlan, AppsError> {
        let plan = UpdatePlan {
            id: Self::next_plan_id(),
            package_id: req.package_id,
            from_version: req.from_version,
            to_version: req.to_version,
            state: UpdateState::Planned,
            failure_class: None,
            created_at: Self::now(),
            state_changed_at: Self::now(),
        };

        if !req.dry_run {
            self.store_plan(plan.clone()).await;
        }

        Ok(plan)
    }

    async fn execute_update(&self, plan_id: UpdatePlanId) -> Result<UpdateOutcome, AppsError> {
        let mut plan = self.load_plan(&plan_id).await?;

        // Transition Planned → Executing
        apply_transition(&mut plan, UpdateState::Executing)?;
        self.store_plan(plan.clone()).await;

        // Simulate execution work, then transition Executing → Executed.
        apply_transition(&mut plan, UpdateState::Executed)?;
        self.store_plan(plan.clone()).await;

        if let Some(ref emitter) = self.emitter {
            emitter
                .emit_update_event(&plan.id, &plan.package_id, UpdatePhaseRecord::Executed)
                .await?;
        }

        Ok(UpdateOutcome {
            execution_metrics: serde_json::json!({"elapsed_ms": 42}),
            artifacts_swapped: 128,
        })
    }

    async fn verify_update(&self, plan_id: UpdatePlanId) -> Result<UpdateVerification, AppsError> {
        let mut plan = self.load_plan(&plan_id).await?;

        // Transition Executed → Verifying
        apply_transition(&mut plan, UpdateState::Verifying)?;
        self.store_plan(plan.clone()).await;

        // Simulate verification.
        let verification = UpdateVerification {
            hash_match: true,
            capability_drift: vec![],
            profile_compat: 100,
        };

        if verification.hash_match {
            apply_transition(&mut plan, UpdateState::Verified)?;
        } else {
            plan.failure_class = Some(FailureClass::VerifyMismatch);
            apply_transition(&mut plan, UpdateState::Failed)?;
        }

        self.store_plan(plan.clone()).await;

        if let Some(ref emitter) = self.emitter {
            let phase = if verification.hash_match {
                UpdatePhaseRecord::Verified
            } else {
                UpdatePhaseRecord::Failed(FailureClass::VerifyMismatch)
            };
            emitter
                .emit_update_event(&plan.id, &plan.package_id, phase)
                .await?;
        }

        Ok(verification)
    }

    async fn activate_update(&self, plan_id: UpdatePlanId) -> Result<(), AppsError> {
        let mut plan = self.load_plan(&plan_id).await?;

        // Transition Verified → Activating
        apply_transition(&mut plan, UpdateState::Activating)?;
        self.store_plan(plan.clone()).await;

        // Transition Activating → Active
        apply_transition(&mut plan, UpdateState::Active)?;
        self.store_plan(plan.clone()).await;

        if let Some(ref emitter) = self.emitter {
            emitter
                .emit_update_event(&plan.id, &plan.package_id, UpdatePhaseRecord::Activated)
                .await?;
        }

        Ok(())
    }

    async fn rollback_update(
        &self,
        plan_id: UpdatePlanId,
        reason: RollbackReason,
    ) -> Result<RollbackReceipt, AppsError> {
        let mut plan = self.load_plan(&plan_id).await?;

        let reverted_to = plan.from_version.clone();

        // Transition → RollingBack
        apply_transition(&mut plan, UpdateState::RollingBack)?;
        self.store_plan(plan.clone()).await;

        // Transition RollingBack → RolledBack
        apply_transition(&mut plan, UpdateState::RolledBack)?;
        self.store_plan(plan.clone()).await;

        if let Some(ref emitter) = self.emitter {
            emitter
                .emit_update_event(
                    &plan.id,
                    &plan.package_id,
                    UpdatePhaseRecord::RolledBack(reason),
                )
                .await?;
        }

        Ok(RollbackReceipt {
            plan_id,
            reverted_to,
            completed_at: Self::now(),
            exit_state: RollbackExitState::Reverted,
        })
    }

    async fn get_update(&self, plan_id: UpdatePlanId) -> Result<UpdatePlan, AppsError> {
        self.load_plan(&plan_id).await
    }
}

// ---------------------------------------------------------------------------
// Unit tests (inline)
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    /// All 11 variants serialize and deserialize correctly.
    #[test]
    fn update_state_serde_round_trips() {
        for state in &[
            UpdateState::Planned,
            UpdateState::Executing,
            UpdateState::Executed,
            UpdateState::Verifying,
            UpdateState::Verified,
            UpdateState::Activating,
            UpdateState::Active,
            UpdateState::Failed,
            UpdateState::RollingBack,
            UpdateState::RolledBack,
            UpdateState::RollbackFailed,
        ] {
            let json = serde_json::to_string(state).expect("serialize");
            let back: UpdateState = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*state, back, "round-trip failed for {state}");
        }
    }

    /// Every FailureClass variant round-trips through serde.
    #[test]
    fn failure_class_serde_round_trips() {
        for fc in &[
            FailureClass::ExecuteError,
            FailureClass::VerifyMismatch,
            FailureClass::ActivateError,
            FailureClass::PolicyDenied,
        ] {
            let json = serde_json::to_string(fc).expect("serialize");
            let back: FailureClass = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*fc, back);
        }
    }

    /// Happy-path transitions are all legal.
    #[test]
    fn happy_path_transitions_are_legal() {
        assert!(is_legal_transition(
            UpdateState::Planned,
            UpdateState::Executing
        ));
        assert!(is_legal_transition(
            UpdateState::Executing,
            UpdateState::Executed
        ));
        assert!(is_legal_transition(
            UpdateState::Executed,
            UpdateState::Verifying
        ));
        assert!(is_legal_transition(
            UpdateState::Verifying,
            UpdateState::Verified
        ));
        assert!(is_legal_transition(
            UpdateState::Verified,
            UpdateState::Activating
        ));
        assert!(is_legal_transition(
            UpdateState::Activating,
            UpdateState::Active
        ));
    }

    /// Executing → Failed is legal.
    #[test]
    fn executing_to_failed_is_legal() {
        assert!(is_legal_transition(
            UpdateState::Executing,
            UpdateState::Failed
        ));
    }

    /// Verifying → Failed is legal.
    #[test]
    fn verifying_to_failed_is_legal() {
        assert!(is_legal_transition(
            UpdateState::Verifying,
            UpdateState::Failed
        ));
    }

    /// Activating → Failed is legal.
    #[test]
    fn activating_to_failed_is_legal() {
        assert!(is_legal_transition(
            UpdateState::Activating,
            UpdateState::Failed
        ));
    }

    /// Failed → RollingBack is legal.
    #[test]
    fn failed_to_rolling_back_is_legal() {
        assert!(is_legal_transition(
            UpdateState::Failed,
            UpdateState::RollingBack
        ));
    }

    /// Active → RollingBack is legal (regression / user request).
    #[test]
    fn active_to_rolling_back_is_legal() {
        assert!(is_legal_transition(
            UpdateState::Active,
            UpdateState::RollingBack
        ));
    }

    /// RollingBack → RolledBack is legal.
    #[test]
    fn rolling_back_to_rolled_back_is_legal() {
        assert!(is_legal_transition(
            UpdateState::RollingBack,
            UpdateState::RolledBack
        ));
    }

    /// RollingBack → RollbackFailed is legal.
    #[test]
    fn rolling_back_to_rollback_failed_is_legal() {
        assert!(is_legal_transition(
            UpdateState::RollingBack,
            UpdateState::RollbackFailed
        ));
    }

    /// Direct Planned → Executed is illegal (must go through Executing).
    #[test]
    fn planned_to_executed_is_illegal() {
        assert!(!is_legal_transition(
            UpdateState::Planned,
            UpdateState::Executed
        ));
    }

    /// Verified → Active is illegal (must go through Activating).
    #[test]
    fn verified_to_active_is_illegal() {
        assert!(!is_legal_transition(
            UpdateState::Verified,
            UpdateState::Active
        ));
    }

    /// RolledBack → anything is illegal (terminal).
    #[test]
    fn rolled_back_is_terminal() {
        for target in &[
            UpdateState::Planned,
            UpdateState::Executing,
            UpdateState::Executed,
            UpdateState::Verifying,
            UpdateState::Verified,
            UpdateState::Activating,
            UpdateState::Active,
            UpdateState::Failed,
            UpdateState::RollingBack,
            UpdateState::RollbackFailed,
        ] {
            assert!(
                !is_legal_transition(UpdateState::RolledBack, *target),
                "RolledBack → {target} should be illegal"
            );
        }
    }

    /// State count matches the 11-variant spec.
    #[test]
    fn update_state_has_11_variants() {
        use strum::EnumCount;
        assert_eq!(UpdateState::COUNT, 11);
    }

    /// FailureClass has exactly 4 variants.
    #[test]
    fn failure_class_has_4_variants() {
        use strum::EnumCount;
        assert_eq!(FailureClass::COUNT, 4);
    }
}
