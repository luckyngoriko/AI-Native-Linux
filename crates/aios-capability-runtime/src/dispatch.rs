//! Dispatch-shape vocabulary: `ActionDispatchKind`, `AdapterIOMode`,
//! `AdapterStability`, `QueueClass` — all closed enums per S10.1 §3.2 / §3.3 /
//! §3.4 / §3.5.
//!
//! Each enum is contract-grade. Adding a variant is a versioned spec change.
//! Decoders fail closed on unknown values. The serde wire form is
//! `SCREAMING_SNAKE_CASE`, matching the proto IDL in §5.1 / §10.1.

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// `ActionDispatchKind` — S10.1 §3.2 closed enum.
///
/// The runtime decides how the action is handed to its adapter. The decision
/// is a function of the adapter manifest, the action's `Risk` flags
/// (S0.1 §4.7), the policy decision's `Constraints.sandbox_profile_id`
/// (S2.3 §10), and the action subject's `is_ai` flag. See the closed decision
/// table in §3.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionDispatchKind {
    /// `IN_PROCESS_RPC` — adapter handler runs inside the runtime's process.
    /// Reserved for low-latency, no-mutation adapters declared `STABLE` and
    /// `TYPED_PARAMETERS_ONLY`.
    InProcessRpc,
    /// `SUBPROCESS_FORK` — per-action subprocess; default for filesystem
    /// mutation, service control, package operations on host-bounded adapters.
    SubprocessFork,
    /// `ISOLATED_SANDBOX` — full sandbox per S3.2 `SandboxProfile`. Required
    /// for any AI-origin action (subject `is_ai = true`). Required for any
    /// action whose `risk.privileged` is true.
    IsolatedSandbox,
    /// `DRY_RUN` — no mutation; adapter produces a simulation transcript
    /// only. Forced by `request.dry_run = SIMULATE` per S0.1 §9.3.
    DryRun,
}

/// `AdapterIOMode` — S10.1 §3.3 closed enum.
///
/// Free-form shell command input is **not** a value. The L3 invariant
/// (adapters must not accept free-form shell commands as primary input) and
/// INV-013 (AI cannot perform system admin operations) preclude it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdapterIOMode {
    /// `TYPED_PARAMETERS_ONLY` — adapter accepts `request.target` as a typed
    /// proto/JSON struct validated against the adapter manifest's per-action
    /// `target_schema`. Default mode.
    TypedParametersOnly,
    /// `TEMPLATE_PARAMETERS` — adapter accepts a typed template (also
    /// schema-validated) with closed substitution variables bound from
    /// `request.target`. Used by adapters that legitimately need to construct
    /// command lines without exposing free-form shell to the caller.
    TemplateParameters,
}

/// `AdapterStability` — S10.1 §3.4 closed enum.
///
/// Stability is a property of the adapter, not of any individual action.
/// Stability transitions are operator-only typed actions and themselves flow
/// through the runtime (`runtime.adapter.set_stability`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdapterStability {
    /// `REGISTERED` — manifest accepted; not yet promoted past the initial
    /// registration barrier; treated as `EXPERIMENTAL` for dispatch purposes.
    Registered,
    /// `EXPERIMENTAL` — functional but not yet hardened. AI-origin actions
    /// targeting `EXPERIMENTAL` adapters are forced to `DRY_RUN` by default;
    /// an explicit policy clearance is required for live execution.
    Experimental,
    /// `STABLE` — hardened. Eligible for `IN_PROCESS_RPC` dispatch. Default
    /// for production adapters.
    Stable,
    /// `DEPRECATED` — still accepted for execution but emits
    /// `ADAPTER_DEPRECATED_DISPATCH` on every call; new actions targeting it
    /// are discouraged by the runtime's tooling.
    Deprecated,
    /// `RETIRED` — no new dispatches accepted. `ListAdapters` still returns
    /// the adapter for forensic reasons. Action submissions targeting a
    /// `RETIRED` adapter fail with `UnknownAdapter`.
    Retired,
}

/// `QueueClass` — S10.1 §3.5 closed enum.
///
/// Each registered action is dispatched through exactly one queue class.
/// `AI_INTERACTIVE` is **not** a value: AI subjects attempting to submit on
/// [`QueueClass::Interactive`] are silently downgraded to
/// [`QueueClass::AgentProposal`] and an `AI_INTERACTIVE_QUEUE_DOWNGRADE`
/// evidence record is emitted (§13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QueueClass {
    /// `INTERACTIVE` — operator-initiated (`subject_type` ∈ `human`) and
    /// `request.environment != AIR_GAPPED`. p95 queue wait < 200 ms.
    Interactive,
    /// `AGENT_PROPOSAL` — AI-initiated (subject `is_ai = true`). Fairness-
    /// bounded; capped at 50% of total queue capacity. p95 queue wait < 2 s.
    AgentProposal,
    /// `BACKGROUND` — scheduled jobs, timers, application-internal cleanup
    /// actions. Yielding to higher classes. p95 queue wait < 30 s.
    Background,
    /// `RECOVERY_PRIORITY` — any action while `host.recovery_mode = true`;
    /// preempts all other classes. Recovery-mode only. p95 queue wait < 200 ms.
    RecoveryPriority,
}
