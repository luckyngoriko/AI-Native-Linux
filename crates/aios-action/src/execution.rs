//! Execution payload — "what the runtime observes and records" (S0.1 §5).
//!
//! Capability Runtime is the **sole writer** of `Execution` per S0.1 §6.8. Callers read.
//! Following the K8s `spec`/`status` separation, `Execution` is the `status` half of the
//! envelope (S0.1 §2.1 table row 3).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::phase::ActionPhase;

/// Closed canonical condition-type vocabulary from S0.1 §5.3.
///
/// The 13 variants are an authoritative closed set — adding a new type is a deliberate
/// versioned spec change, not a downstream-extension point, so `#[non_exhaustive]` is
/// deliberately **not** used. Adapter-specific conditions are not represented in this
/// enum; they are carried as namespaced strings in a separate channel (out of scope for
/// T-004).
///
/// Names are serialised in `PascalCase` to match the proto canonical labels listed in
/// S0.1 §5.3 (e.g. `PolicyEvaluated`, `RollbackPossible`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConditionType {
    /// Policy Kernel returned a decision (any decision — `Allow`, `Deny`, `RequireApproval`).
    PolicyEvaluated,
    /// Policy decision was `Allow` (not `RequireApproval`, not `Deny`).
    PolicyAccepted,
    /// Policy returned `RequireApproval`; an approval prompt was created.
    ApprovalRequired,
    /// An approval matching the action's `request_hash` was attached.
    ApprovalGranted,
    /// Approval TTL elapsed before consumption.
    ApprovalExpired,
    /// Sandbox profile composed and bound to the running adapter.
    Sandboxed,
    /// Adapter returned success from `ExecuteAction`.
    Executed,
    /// Post-execution verification passed (all verification intents).
    Verified,
    /// Rollback is mechanically possible for this action.
    RollbackPossible,
    /// A rollback has been successfully applied.
    RolledBack,
    /// Request was deduplicated against an earlier envelope (idempotency cache hit).
    Idempotent,
    /// `DryRunMode = Simulate` and the simulation completed.
    Simulated,
    /// Caller used a deprecated schema field; the action proceeded with a migration hint.
    DeprecatedFieldUsed,
}

/// Status of an individual lifecycle condition (S0.1 §5.1 `enum ConditionStatus`).
///
/// Following the K8s-style three-valued status: `Unknown` is meaningful — it means
/// "the runtime has not yet observed this fact", which is distinct from `False`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConditionStatus {
    /// The condition is observed and holds.
    True,
    /// The condition is observed and does not hold.
    False,
    /// The condition has not yet been observed.
    Unknown,
}

/// A timestamped fact about the envelope's execution (S0.1 §5.1 `message Condition`,
/// vocabulary in §5.3).
///
/// `condition_type` is drawn from the closed [`ConditionType`] vocabulary (S0.1 §5.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    /// Condition type — closed vocabulary entry from S0.1 §5.3.
    ///
    /// Serialised as `type` to match the proto field name.
    #[serde(rename = "type")]
    pub condition_type: ConditionType,

    /// Current observed status.
    pub status: ConditionStatus,

    /// Wall-clock time the condition was observed (`last_transition_time` in S0.1 §5.1).
    pub observed_at: DateTime<Utc>,

    /// Human-readable detail; renderers may surface this.
    pub message: String,
}

impl Condition {
    /// Convenience constructor for the common `status = True` case used by the
    /// Capability Runtime as it walks the lifecycle.
    #[must_use]
    pub fn now_true(condition_type: ConditionType, message: impl Into<String>) -> Self {
        Self {
            condition_type,
            status: ConditionStatus::True,
            observed_at: Utc::now(),
            message: message.into(),
        }
    }
}

/// Runtime-observed state — phase + condition list + a few timestamps.
///
/// The full S0.1 §5.1 `Execution` message (`capability_runtime_id`, `attempts`, `result`, `error`,
/// `verification_results`, `evidence_receipt_ids`, ...) is populated incrementally in
/// tasks T-002 through T-006. T-001/T-004 ship the lifecycle-critical fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Execution {
    /// Coarse-grained phase. Initialised to [`ActionPhase::Pending`] on envelope construction.
    pub phase: ActionPhase,

    /// Wall-clock time of the last phase transition (S0.1 §5.1 `phase_changed_at`).
    ///
    /// Monotonically non-decreasing across transitions per S0.1 §6.7.
    pub phase_changed_at: DateTime<Utc>,

    /// When the Capability Runtime began executing (`phase = Running`). `None` while pending.
    pub started_at: Option<DateTime<Utc>>,

    /// When the envelope reached a terminal phase. `None` until terminal.
    pub ended_at: Option<DateTime<Utc>>,

    /// Sandbox profile actually applied per S0.1 §9.2 (`max_restriction` of policy, caller, adapter default).
    pub sandbox_profile_id: Option<String>,

    /// Append-only list of conditions; vocabulary in S0.1 §5.3.
    ///
    /// Monotonicity invariant (S0.1 §6.7): conditions are added, not removed; a condition
    /// once observed `True` cannot subsequently be set `False` for the same `condition_type`.
    pub conditions: Vec<Condition>,
}

impl Execution {
    /// Fresh execution block for a newly created envelope — `Pending` with no observations.
    ///
    /// Stamps `phase_changed_at` with the current wall-clock time so that the next
    /// transition's timestamp is guaranteed `>=` the creation timestamp.
    #[must_use]
    pub fn pending() -> Self {
        Self {
            phase: ActionPhase::Pending,
            phase_changed_at: Utc::now(),
            started_at: None,
            ended_at: None,
            sandbox_profile_id: None,
            conditions: Vec::new(),
        }
    }
}

impl Default for Execution {
    fn default() -> Self {
        Self::pending()
    }
}
