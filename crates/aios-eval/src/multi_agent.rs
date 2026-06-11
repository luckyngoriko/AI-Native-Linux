use crate::enums::MultiAgentState;
use ulid::Ulid;

/// Coordination record for a multi‑agent task execution loop.
///
/// # INV‑016 Separation of Duties
///
/// - The Reviewer MUST NOT equal the Executor.
/// - The Reviewer MUST NOT equal the Planner.
/// - The validator subject MUST NOT be an AI agent (always `SYSTEM_SERVICE`).
#[derive(Debug, Clone)]
pub struct MultiAgentCoordination {
    /// Unique coordination identifier (prefix `"mac_"` + ULID).
    pub coordination_id: String,
    /// The task intent being executed by this coordination.
    pub task_intent_id: String,
    /// Subject ID of the Planner agent.
    pub planner_subject_id: String,
    /// Subject ID of the Executor agent.
    pub executor_subject_id: String,
    /// Subject ID of the Reviewer (MUST be `SYSTEM_SERVICE`).
    pub reviewer_subject_id: String,
    /// Current coordination state.
    pub state: MultiAgentState,
}

impl MultiAgentCoordination {
    /// Creates a new coordination record in the `Planning` state with a fresh
    /// ULID.
    #[must_use]
    pub fn new(
        task_intent_id: impl Into<String>,
        planner_subject_id: impl Into<String>,
        executor_subject_id: impl Into<String>,
        reviewer_subject_id: impl Into<String>,
    ) -> Self {
        Self {
            coordination_id: format!("mac_{}", Ulid::new()),
            task_intent_id: task_intent_id.into(),
            planner_subject_id: planner_subject_id.into(),
            executor_subject_id: executor_subject_id.into(),
            reviewer_subject_id: reviewer_subject_id.into(),
            state: MultiAgentState::Planning,
        }
    }

    /// Validates the INV‑016 separation‑of‑duties invariants.
    ///
    /// # Errors
    ///
    /// Returns a specific error string if any of the following are violated:
    ///
    /// - `reviewer_subject_id == executor_subject_id`
    /// - `reviewer_subject_id == planner_subject_id`
    pub fn validate_separation(&self) -> Result<(), String> {
        if self.reviewer_subject_id == self.executor_subject_id {
            return Err(format!(
                "INV-016 violation: reviewer ({}) must not equal executor ({})",
                self.reviewer_subject_id, self.executor_subject_id
            ));
        }
        if self.reviewer_subject_id == self.planner_subject_id {
            return Err(format!(
                "INV-016 violation: reviewer ({}) must not equal planner ({})",
                self.reviewer_subject_id, self.planner_subject_id
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    #[test]
    fn new_coordination_starts_in_planning_state() {
        let mac = MultiAgentCoordination::new("t1", "planner-1", "executor-1", "reviewer-sys");
        assert_eq!(mac.state, MultiAgentState::Planning);
    }

    #[test]
    fn validate_separation_passes_with_distinct_roles() {
        let mac = MultiAgentCoordination::new("t1", "planner-1", "executor-1", "reviewer-sys");
        assert!(mac.validate_separation().is_ok());
    }

    #[test]
    fn validate_separation_rejects_reviewer_equals_executor() {
        let mac = MultiAgentCoordination::new("t1", "planner-1", "agent-x", "agent-x");
        let err = mac.validate_separation().unwrap_err();
        assert!(err.contains("reviewer"));
        assert!(err.contains("executor"));
        assert!(err.contains("INV-016"));
    }

    #[test]
    fn validate_separation_rejects_reviewer_equals_planner() {
        let mac = MultiAgentCoordination::new("t1", "agent-x", "executor-1", "agent-x");
        let err = mac.validate_separation().unwrap_err();
        assert!(err.contains("reviewer"));
        assert!(err.contains("planner"));
        assert!(err.contains("INV-016"));
    }

    #[test]
    fn validate_separation_rejects_both_violations() {
        let mac = MultiAgentCoordination::new("t1", "agent-x", "agent-x", "agent-x");
        let err = mac.validate_separation().unwrap_err();
        // First check is reviewer == executor, so that's the error we get
        assert!(err.contains("INV-016"));
    }

    #[test]
    fn coordination_id_starts_with_mac_prefix() {
        let mac = MultiAgentCoordination::new("t1", "p1", "e1", "r-sys");
        assert!(mac.coordination_id.starts_with("mac_"));
    }
}
