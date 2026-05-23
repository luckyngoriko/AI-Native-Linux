//! Lifecycle phase enum + transition FSM (S0.1 §4 / §6).
//!
//! Five buckets — three of them terminal — with exactly six valid transitions enumerated in S0.1 §6.2:
//!
//! | #   | From       | To           |
//! | --- | ---------- | ------------ |
//! | T2-T5 | Pending  | Failed       |
//! | T6  | Pending    | Running      |
//! | T7  | Running    | Succeeded    |
//! | T8-T9 | Running  | Failed       |
//! | T10 | Running    | `RolledBack` |
//!
//! Anything else — in particular `Succeeded -> RolledBack` — is **forbidden** (S0.1 §6.2 last
//! paragraph + §6.3 terminality invariant). Post-hoc undo is a new envelope, not a transition.

use serde::{Deserialize, Serialize};

/// Coarse-grained lifecycle phase. Fine-grained facts live on `Execution::conditions`.
///
/// Serialised in `SCREAMING_SNAKE_CASE` to match the proto enum names in S0.1 §5.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionPhase {
    /// Envelope accepted; awaiting policy / approval / queue.
    Pending,
    /// Adapter is executing or verification is running.
    Running,
    /// Terminal: action executed and fully verified.
    Succeeded,
    /// Terminal: any failure path (policy denial, execution error, verification failure, ...).
    Failed,
    /// Terminal: explicit rollback path completed. Distinct from `Failed` per S0.1 §5.2.
    RolledBack,
}

impl ActionPhase {
    /// Returns `true` for the three terminal phases (`Succeeded`, `Failed`, `RolledBack`).
    ///
    /// Per S0.1 §6.3, no further transitions are valid once a phase is terminal.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::RolledBack)
    }

    /// Returns `true` if a transition from `self` to `next` is one of the six allowed
    /// transitions enumerated in S0.1 §6.2.
    ///
    /// All other transitions — including any departure from a terminal phase and the
    /// classic-mistake `Succeeded -> RolledBack` — return `false`.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        // The six allowed transitions per S0.1 §6.2:
        //   T2-T5  PENDING -> FAILED      (policy denied / approval expired / idempotency conflict / cancelled)
        //   T6     PENDING -> RUNNING     (policy accepted, adapter ready, queue dequeued)
        //   T7     RUNNING -> SUCCEEDED   (executed AND verified)
        //   T8-T9  RUNNING -> FAILED      (execution error / verification failed, no rollback)
        //   T10    RUNNING -> ROLLED_BACK (failure + auto-rollback succeeded)
        // Anything else (terminal -> * and PENDING -> {SUCCEEDED, ROLLED_BACK}) is forbidden.
        matches!(
            (self, next),
            (Self::Pending, Self::Failed | Self::Running)
                | (
                    Self::Running,
                    Self::Succeeded | Self::Failed | Self::RolledBack
                )
        )
    }
}

#[cfg(test)]
mod tests {
    use super::ActionPhase::{Failed, Pending, RolledBack, Running, Succeeded};

    #[test]
    fn terminal_classification_matches_s01_section_6_3() {
        assert!(!Pending.is_terminal());
        assert!(!Running.is_terminal());
        assert!(Succeeded.is_terminal());
        assert!(Failed.is_terminal());
        assert!(RolledBack.is_terminal());
    }

    #[test]
    fn exactly_the_six_s01_section_6_2_transitions_are_valid() {
        // The six allowed transitions.
        assert!(Pending.can_transition_to(Running)); // T6
        assert!(Pending.can_transition_to(Failed)); // T2-T5
        assert!(Running.can_transition_to(Succeeded)); // T7
        assert!(Running.can_transition_to(Failed)); // T8-T9
        assert!(Running.can_transition_to(RolledBack)); // T10

        // Identity (self-loop) is NOT allowed — phase transitions are progress events.
        assert!(!Pending.can_transition_to(Pending));
        assert!(!Running.can_transition_to(Running));

        // Pending may not skip Running to reach a verification-bound terminal.
        assert!(!Pending.can_transition_to(Succeeded));
        assert!(!Pending.can_transition_to(RolledBack));

        // The classic mistake: post-hoc rollback of a succeeded action is a NEW envelope,
        // not a transition. S0.1 §6.2 last paragraph forbids this explicitly.
        assert!(!Succeeded.can_transition_to(RolledBack));
        assert!(!Succeeded.can_transition_to(Failed));
        assert!(!Succeeded.can_transition_to(Running));

        // Terminal phases are truly terminal (S0.1 §6.3 + §6.7 monotonicity).
        for terminal in [Succeeded, Failed, RolledBack] {
            for any in [Pending, Running, Succeeded, Failed, RolledBack] {
                assert!(
                    !terminal.can_transition_to(any),
                    "terminal phase {terminal:?} must not transition to {any:?}",
                );
            }
        }
    }
}
