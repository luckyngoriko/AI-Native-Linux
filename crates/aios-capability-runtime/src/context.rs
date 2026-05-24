//! `ActionContext` — per-action internal runtime context.
//!
//! Lightweight container the orchestration RPCs thread through the lifecycle
//! pipeline. T-026 lands the **shape only**; the pipeline driver,
//! transition validation, and persistence wiring (S10.1 §4.3) belong to
//! `T-027` (`CapabilityRuntime` trait + lifecycle pipeline).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use aios_action::ActionId;

use crate::dispatch::{ActionDispatchKind, QueueClass};
use crate::failure::{ExecutionFailureReason, RollbackOutcome};
use crate::status::ActionLifecycleState;

/// `ActionContext` — the runtime's per-action working memory.
///
/// Distinct from the S0.1 `ActionEnvelope` (the wire shape): the envelope is
/// what callers submit, the context is what the runtime maintains while the
/// FSM advances. Persistence to AIOS-FS per §4.3 is queued for T-027.
///
/// `evidence_chain` collects the ids of evidence records emitted for this
/// action; T-031 wires the actual emission path and the §16 / §17 record
/// catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionContext {
    /// The S0.1 envelope's `action_id` — owning key for this context.
    pub action_id: ActionId,
    /// Current state in the §3.1 fourteen-state FSM.
    pub status: ActionLifecycleState,
    /// The decided dispatch kind per the §3.2 closed decision table. Unset
    /// before the queue-enrolment transition; defaults to
    /// [`ActionDispatchKind::SubprocessFork`] until the runtime decides.
    pub dispatch_kind: ActionDispatchKind,
    /// The queue class enrolment per §3.5.
    pub queue_class: QueueClass,
    /// Wall-clock at context creation (envelope acceptance time).
    pub created_at: DateTime<Utc>,
    /// Wall-clock of the most recent FSM transition.
    pub last_updated_at: DateTime<Utc>,
    /// Populated when the FSM transitions to `FAILED` or `ROLLED_BACK`.
    /// `None` until a failure occurs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ExecutionFailureReason>,
    /// Populated when the rollback path executes. `None` while the action is
    /// pre-execution or successfully completed without rollback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_outcome: Option<RollbackOutcome>,
    /// Evidence receipt ids accumulated across the lifecycle. Initially
    /// empty; appended on every state transition (T-031 wires the emission).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_chain: Vec<String>,
}

impl ActionContext {
    /// Construct a fresh context at the start of the lifecycle (state
    /// `CREATED`, no error, no rollback outcome, empty evidence chain).
    ///
    /// `created_at` and `last_updated_at` are seeded to the same `now` value
    /// the caller supplies (no system-clock side effects — determinism per
    /// the workspace test discipline).
    #[must_use]
    pub const fn new(
        action_id: ActionId,
        dispatch_kind: ActionDispatchKind,
        queue_class: QueueClass,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            action_id,
            status: ActionLifecycleState::Created,
            dispatch_kind,
            queue_class,
            created_at: now,
            last_updated_at: now,
            error: None,
            rollback_outcome: None,
            evidence_chain: Vec::new(),
        }
    }
}
