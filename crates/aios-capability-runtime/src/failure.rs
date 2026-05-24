//! Failure-shape vocabulary: `ExecutionFailureReason`, `RollbackOutcome`,
//! `RuntimeErrorCode` — closed enums per S10.1 §3.6 / §3.7 / §3.8.
//!
//! Each enum is contract-grade. Adding a variant is a versioned spec change.
//! Decoders fail closed on unknown values. The serde wire form is
//! `SCREAMING_SNAKE_CASE`, matching the proto IDL in §5.1.

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// `ExecutionFailureReason` — S10.1 §3.6 closed enum, twelve values.
///
/// Populated on transitions to `FAILED` or `ROLLED_BACK` to discriminate the
/// failure cause. Mirrored into the S0.1 `Error.code` field where the
/// corresponding canonical code exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionFailureReason {
    /// `SANDBOX_APPLICATION_FAILED` — the composed `SandboxProfile`
    /// (S3.2 `ComposeProfile`) could not be applied at dispatch time.
    SandboxApplicationFailed,
    /// `ADAPTER_TIMEOUT` — the adapter exceeded its declared
    /// `adapter_timeout_seconds`.
    AdapterTimeout,
    /// `ADAPTER_PANIC` — the adapter process exited with a non-zero status or
    /// panicked mid-execution.
    AdapterPanic,
    /// `RESOURCE_BUDGET_EXCEEDED` — the action's queue class budget, the
    /// per-subject rate limit, or the AI-share cap was exceeded at dispatch.
    ResourceBudgetExceeded,
    /// `DEPENDENCY_UNREADY` — a declared adapter dependency (e.g. systemd,
    /// AIOS-FS) was not in a ready state at dispatch.
    DependencyUnready,
    /// `BACKEND_UNAVAILABLE` — an external backend the adapter required
    /// (e.g. dnf metadata, AIOS-FS WAL) was unreachable.
    BackendUnavailable,
    /// `IDEMPOTENCY_KEY_REPLAY_DETECTED` — the `idempotency_key` was reused
    /// with a different `request_hash` (S0.1 §3.3).
    IdempotencyKeyReplayDetected,
    /// `ENVELOPE_VALIDATION_FAILED` — the envelope failed schema validation,
    /// target schema validation, or trace-context validation.
    EnvelopeValidationFailed,
    /// `ROLLBACK_PRECONDITION_FAILED` — a rollback was requested but the
    /// adapter's declared `rollback_precondition` was not met.
    RollbackPreconditionFailed,
    /// `BINDING_EXPIRED` — the held `ApprovalBinding` (S5.3) or
    /// `OverrideBinding` (S5.4) was expired or revoked at dispatch.
    BindingExpired,
    /// `BINDING_VOIDED_ACTION_REVISED` — the action's canonical hash at
    /// dispatch differs from the bound `bound_action_canonical_hash`.
    BindingVoidedActionRevised,
    /// `ADAPTER_REFUSED` — the adapter ran but explicitly refused the action
    /// (e.g. precondition assertion).
    AdapterRefused,
}

/// `RollbackOutcome` — S10.1 §3.7 closed enum.
///
/// Per §7 rollback discipline: `RollbackOutcome::Failed` drives the FSM into
/// the [`crate::ActionLifecycleState::RollbackFailed`] terminal forensic
/// state; `RollbackOutcome::Succeeded` drives it into
/// [`crate::ActionLifecycleState::RolledBack`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RollbackOutcome {
    /// `NOT_ATTEMPTED` — rollback was not attempted (e.g. action succeeded;
    /// or `rollback_strategy = NONE` on the adapter).
    NotAttempted,
    /// `SUCCEEDED` — adapter rollback returned success; lifecycle transitions
    /// to `ROLLED_BACK`.
    Succeeded,
    /// `FAILED` — adapter rollback returned failure or panicked; lifecycle
    /// transitions to `ROLLBACK_FAILED`; operator alert emitted.
    Failed,
    /// `NOT_APPLICABLE` — the action was idempotent or read-only and rollback
    /// semantics do not apply (e.g. a query action).
    NotApplicable,
}

/// `RuntimeErrorCode` — S10.1 §3.8 closed enum, twenty values.
///
/// Used in RPC-level error responses (gRPC status detail). Distinct from
/// S0.1 `Error.code`: this enum carries L3-internal failures of the
/// orchestration RPCs themselves, not failures of the actions they orchestrate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeErrorCode {
    /// `RUNTIME_OK` — reserved zero-value indicator; not an error.
    RuntimeOk,
    /// `INVALID_ENVELOPE` — the envelope failed pre-validation; details point
    /// to the offending field.
    InvalidEnvelope,
    /// `UNKNOWN_ACTION_KIND` — the `request.action` does not map to any
    /// registered adapter's declared `action_kinds`.
    UnknownActionKind,
    /// `UNKNOWN_ADAPTER` — a direct adapter lookup by id failed
    /// (`ListAdapters` / `GetAdapterCapabilities`).
    UnknownAdapter,
    /// `ADAPTER_NOT_DISPATCHABLE` — the adapter exists but is in `RETIRED`
    /// stability or is `DEGRADED` past the dispatch threshold.
    AdapterNotDispatchable,
    /// `POLICY_DECISION_UNAVAILABLE` — the Policy Kernel was unreachable or
    /// returned an internal error.
    PolicyDecisionUnavailable,
    /// `APPROVAL_BINDING_INVALID` — the presented `ApprovalBinding` failed
    /// signature, scope, or hash check.
    ApprovalBindingInvalid,
    /// `OVERRIDE_BINDING_INVALID` — the presented `OverrideBinding` failed
    /// signature, scope, or hash check.
    OverrideBindingInvalid,
    /// `BINDING_HASH_MISMATCH` — the action's canonical hash does not match
    /// the binding's `bound_action_canonical_hash`.
    BindingHashMismatch,
    /// `LIFECYCLE_ILLEGAL_TRANSITION` — a request would drive the FSM through
    /// a transition not listed in §4.
    LifecycleIllegalTransition,
    /// `LIFECYCLE_TERMINAL` — the action is in a terminal state; the
    /// requested operation is no longer valid.
    LifecycleTerminal,
    /// `IDEMPOTENCY_REPLAY` — same `idempotency_key` with a different
    /// `request_hash` (S0.1 §3.3).
    IdempotencyReplay,
    /// `QUEUE_BACKPRESSURE_REJECTED` — queue depth exceeded the health
    /// threshold and the runtime is shedding load.
    QueueBackpressureRejected,
    /// `ADAPTER_TIMEOUT_BUDGET_EXCEEDED` — the adapter would not respect a
    /// budget the manifest authoritatively requires.
    AdapterTimeoutBudgetExceeded,
    /// `VERIFICATION_GRAMMAR_REJECTED` — the envelope's `verification_intent`
    /// failed S2.4 grammar validation at submission.
    VerificationGrammarRejected,
    /// `EVIDENCE_LOG_UNAVAILABLE` — the evidence log refused an append; the
    /// runtime fails closed.
    EvidenceLogUnavailable,
    /// `EVIDENCE_TAMPER_DETECTED` — a `TAMPER_DETECTED` event from S3.1 is
    /// active; the runtime is in degraded mode.
    EvidenceTamperDetected,
    /// `RUNTIME_DEGRADED` — the runtime itself is in degraded mode (e.g.
    /// clock rewind, adapter directory unloaded).
    RuntimeDegraded,
    /// `MANIFEST_SIGNATURE_INVALID` — an adapter manifest registration failed
    /// signature verification.
    ManifestSignatureInvalid,
    /// `RUNTIME_INTERNAL` — catch-all for unexpected internal faults; details
    /// carry the trace id for forensic follow-up.
    RuntimeInternal,
}
