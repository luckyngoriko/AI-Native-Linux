//! `ActionLifecycleState` — closed 14-state FSM per S10.1 §3.1.
//!
//! The L3-internal lifecycle is a fourteen-state closed enum. The S0.1 public
//! `ActionPhase` (five buckets: `PENDING`, `RUNNING`, `SUCCEEDED`, `FAILED`,
//! `ROLLED_BACK`) is a strict projection of this finer-grained state, computed
//! by the runtime per S0.1 §6.6.
//!
//! The fourteen-state set is **sealed**. Adding a value is a versioned spec
//! change. Bundle / envelope decoders fail closed on unknown values.
//!
//! The serde wire form is `SCREAMING_SNAKE_CASE` matching the proto IDL in
//! `03_capability_runtime_grpc.md` §3.1.

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// The 14 lifecycle states enumerated in S10.1 §3.1.
///
/// `EnumCount` provides the compile-time invariant
/// `ActionLifecycleState::COUNT == 14` asserted by the round-trip tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionLifecycleState {
    /// `CREATED` — envelope accepted by the runtime; pre-validation not yet
    /// complete. Phase projection: `PENDING`.
    Created,
    /// `POLICY_PENDING` — `EvaluatePolicyForAction` in flight; awaiting Policy
    /// Kernel decision. Phase projection: `PENDING`.
    PolicyPending,
    /// `APPROVAL_PENDING` — Policy decision was `REQUIRE_APPROVAL`; an
    /// `ApprovalRequest` is in `DRAFT` or `AWAITING_OPERATOR` (S5.3 §3.1).
    /// Phase projection: `PENDING`.
    ApprovalPending,
    /// `OVERRIDE_PENDING` — Policy decision was a non-`NonOverridableClass`
    /// hard-`DENY` and an `OverrideRequest` (S5.4 §3.1) is in `OS_REQUESTED`
    /// or `OS_AWAITING_DUAL_CONFIRM`. Phase projection: `PENDING`.
    OverridePending,
    /// `APPROVED` — a valid `ApprovalBinding` (S5.3 §5) or `OverrideBinding`
    /// (S5.4 §5) is held; not yet queued. Phase projection: `PENDING`.
    Approved,
    /// `POLICY_DENIED` — Policy Kernel returned `DENY` (hard or scoped) and no
    /// override grant is in flight. Terminal under normal flow; §4.2 T21
    /// allows operator-authored transition to `OVERRIDE_PENDING`. Phase
    /// projection: `FAILED`.
    PolicyDenied,
    /// `OVERRIDE_DENIED` — the override path itself denied
    /// (`TARGET_NOT_OVERRIDABLE`, `INSUFFICIENT_QUORUM`, `TTL_EXPIRED` —
    /// S5.4 §3.5). **Strict terminal** per §4.2 forbidden-transition table.
    /// Phase projection: `FAILED`.
    OverrideDenied,
    /// `QUEUED` — approved / override-bound and in the dispatch queue under
    /// one of the closed [`crate::QueueClass`] buckets (§3.5). Phase
    /// projection: `PENDING`.
    Queued,
    /// `EXECUTING` — adapter has been dispatched under an
    /// [`crate::ActionDispatchKind`] (§3.2) with the composed `SandboxProfile`
    /// (S3.2) applied. Phase projection: `RUNNING`.
    Executing,
    /// `VERIFYING` — adapter returned success; `VerificationEngine.RunVerification`
    /// (S2.4 §11) is in flight against the envelope's `verification_intent`.
    /// Phase projection: `RUNNING`.
    Verifying,
    /// `SUCCEEDED` — adapter executed and verification passed. **Strict
    /// terminal** per §4.2. Phase projection: `SUCCEEDED`.
    Succeeded,
    /// `FAILED` — non-rollback flow terminal. Execution or verification failed
    /// and `rollback_strategy = NONE`, or rollback was deliberately skipped.
    /// Phase projection: `FAILED`.
    Failed,
    /// `ROLLED_BACK` — adapter rollback completed successfully after an
    /// execution or verification failure. **Strict terminal** per §4.2.
    /// Phase projection: `ROLLED_BACK`.
    RolledBack,
    /// `ROLLBACK_FAILED` — terminal forensic state. Adapter rollback was
    /// attempted but the rollback itself failed; system is in a degraded
    /// state and operator intervention is required (§7.4). **Strict
    /// terminal** per §4.2. Phase projection: `FAILED`.
    RollbackFailed,
}

impl ActionLifecycleState {
    /// Returns `true` iff the state is a **strict** terminal per the §4.2
    /// forbidden-transition table — i.e. **no** outgoing transition exists.
    ///
    /// Per §4.2:
    /// > Any transition out of `SUCCEEDED`, `ROLLED_BACK`, or
    /// > `ROLLBACK_FAILED` (terminal). Any transition out of
    /// > `OVERRIDE_DENIED` (terminal).
    ///
    /// `POLICY_DENIED` and `FAILED` are **not** strict terminals: T21 allows
    /// `POLICY_DENIED → OVERRIDE_PENDING` (operator-authored override) and
    /// T19/T20 allow `FAILED → ROLLED_BACK` / `FAILED → ROLLBACK_FAILED`
    /// (rollback strategy != `NONE`).
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::RolledBack | Self::RollbackFailed | Self::OverrideDenied
        )
    }
}
