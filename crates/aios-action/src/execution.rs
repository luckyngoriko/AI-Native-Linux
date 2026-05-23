//! Execution payload — "what the runtime observes and records" (S0.1 §5).
//!
//! Capability Runtime is the **sole writer** of `Execution` per S0.1 §6.8. Callers read.
//! Following the K8s `spec`/`status` separation, `Execution` is the `status` half of the
//! envelope (S0.1 §2.1 table row 3).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::phase::ActionPhase;

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
/// `type_` is a canonical condition vocabulary entry (`PolicyEvaluated`, `Sandboxed`,
/// `Executed`, ...) per S0.1 §5.3; adapter-specific conditions are namespaced
/// `<adapter_id>.<TypeName>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    /// Condition type — vocabulary entry from S0.1 §5.3.
    ///
    /// Named `type_` because `type` is a Rust keyword; serialised as `type` to match the proto.
    #[serde(rename = "type")]
    pub type_: String,

    /// Current observed status.
    pub status: ConditionStatus,

    /// Wall-clock time the condition was observed (`last_transition_time` in S0.1 §5.1).
    pub observed_at: DateTime<Utc>,

    /// Human-readable detail; renderers may surface this.
    pub message: String,
}

/// Runtime-observed state — phase + condition list + a few timestamps.
///
/// The full S0.1 §5.1 `Execution` message (`capability_runtime_id`, `attempts`, `result`, `error`,
/// `verification_results`, `evidence_receipt_ids`, ...) is populated incrementally in
/// tasks T-002 through T-006. T-001 ships the lifecycle-critical fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Execution {
    /// Coarse-grained phase. Initialised to [`ActionPhase::Pending`] on envelope construction.
    pub phase: ActionPhase,

    /// When the Capability Runtime began executing (`phase = Running`). `None` while pending.
    pub started_at: Option<DateTime<Utc>>,

    /// When the envelope reached a terminal phase. `None` until terminal.
    pub ended_at: Option<DateTime<Utc>>,

    /// Sandbox profile actually applied per S0.1 §9.2 (`max_restriction` of policy, caller, adapter default).
    pub sandbox_profile_id: Option<String>,

    /// Append-only list of conditions; vocabulary in S0.1 §5.3.
    pub conditions: Vec<Condition>,
}

impl Execution {
    /// Fresh execution block for a newly created envelope — `Pending` with no observations.
    #[must_use]
    pub const fn pending() -> Self {
        Self {
            phase: ActionPhase::Pending,
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
