//! Error types for the Action Envelope.
//!
//! - [`IdError`] — failures parsing prefix-namespaced ULID identifiers (S0.1 §3.2).
//! - [`ActionErrorCode`] — closed `PascalCase` taxonomy of 35 canonical codes from S0.1 §7.3 + §13.3.
//! - [`ActionError`] — envelope-shaped error value with `code`, `message`, `retryable`, and bounded `cause` chain (S0.1 §7.1 + §7.5).
//! - [`TransitionError`] — in-process FSM invariant violations (S0.1 §6.2 / §6.6 / §6.7).
//! - [`CauseChainTooDeep`] — separate programmer-facing error for [`ActionError::with_cause`] enforcement of the §7.5 depth-8 bound.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::execution::ConditionType;
use crate::phase::ActionPhase;

/// Maximum depth of the [`ActionError::cause`] chain, per S0.1 §7.5.
///
/// `self` has depth `0`; one nested cause has depth `1`; the deepest allowed
/// chain therefore reaches depth `8`. Attaching a cause that would push the
/// resulting chain past this bound is rejected via [`CauseChainTooDeep`].
pub const MAX_CAUSE_CHAIN_DEPTH: usize = 8;

/// Failure modes for parsing prefix-namespaced ULID identifiers.
///
/// Every variant maps to a concrete violation of S0.1 §3.2:
///
/// - missing or wrong prefix,
/// - colon separator (Wave-11 sentinel for legacy/illegal input — MUST be rejected),
/// - malformed ULID body.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum IdError {
    /// The input is empty.
    #[error("identifier is empty")]
    Empty,

    /// The prefix does not match the expected namespace (e.g. `act_` was required but `intent_` was supplied).
    #[error("wrong id prefix: expected `{expected}`, got input `{got}`")]
    WrongPrefix {
        /// The expected prefix including the trailing underscore (e.g. `act_`).
        expected: &'static str,
        /// The full offending input (truncated by `Display` upstream if needed).
        got: String,
    },

    /// The input uses the legacy colon separator (`act:01H...`). Forbidden by S0.1 §3.2.
    #[error("colon-separated id forms are forbidden (Wave-11 §3.2 rule); got `{0}`")]
    ColonSeparatorForbidden(String),

    /// The ULID body did not parse (wrong length, invalid Crockford base32, etc.).
    #[error("invalid ULID body in id `{id}`: {detail}")]
    InvalidUlidBody {
        /// The offending input.
        id: String,
        /// The underlying parser error rendered as text (we deliberately do not leak the `ulid` crate's error type).
        detail: String,
    },

    /// A content-addressed id body (e.g. `tplan_<32hex>`) failed validation.
    ///
    /// Triggers when the body is not exactly 32 lowercase hex characters, per the
    /// W11-B truncation convention (S0.1 §3.2.2): `hex_lower(BLAKE3(...))[:32]`.
    #[error("invalid hex body in id `{id}`: {detail}")]
    InvalidHexBody {
        /// The offending input.
        id: String,
        /// Human-readable reason (wrong length, non-hex character, uppercase, …).
        detail: String,
    },
}

/// The canonical, **closed** `PascalCase` taxonomy of action-lifecycle error codes
/// from S0.1 §7.3 plus the `InvalidTargetPath` extension from §13.3.
///
/// Exactly **35 variants** — no `Other`, no string fallback. Every code that may
/// appear in an `ActionError` on the wire must be one of these. The serde
/// representation uses the exact `PascalCase` spec strings (e.g. `"PolicyDenied"`,
/// `"EnvelopeMalformed"`, `"AdapterDoesNotSupportSimulate"`).
///
/// The variants are grouped exactly as in §7.3:
/// **Validation (11) · Policy (5) · Authorization (3) · Execution (8) ·
/// Verification (3) · Rollback (2) · Infrastructure (3) = 35**.
///
/// `retryable` is **not** a property of the code itself — it is a per-instance flag
/// set by the originating component (S0.1 §7.4). For the codes whose `retryable`
/// value is fixed by the spec, [`Self::retryable_default`] returns `Some(_)`. For
/// the two "depends on context" codes (`AdapterFailure` and `Internal`) it returns
/// `None` — callers MUST supply an explicit `retryable` value.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionErrorCode {
    // ---- Validation (11) ---- (S0.1 §7.3, before policy)
    /// Proto deserialization or schema validation failed.
    EnvelopeMalformed,
    /// `request.reason` is empty.
    MissingReason,
    /// `request.action` is not in a known namespace or format.
    InvalidActionName,
    /// No adapter handles this action.
    UnknownAdapter,
    /// `request.subject` does not match the `<type>:<name>` pattern.
    InvalidSubject,
    /// `idempotency_key` is empty, too long (>128 chars), or contains invalid characters.
    InvalidIdempotencyKey,
    /// Same `idempotency_key` already bound to a different `hash(request)` (S0.1 §3.3 rule 2).
    IdempotencyConflict,
    /// `envelope.schema_version` is not supported by the Capability Runtime.
    SchemaVersionUnsupported,
    /// `request.target` does not match the adapter's target schema.
    TargetSchemaInvalid,
    /// `request.sandbox_profile_id` does not exist.
    SandboxProfileUnknown,
    /// `request.target` failed namespace-path resolution or mismatched the resolved record (S0.1 §13.3).
    InvalidTargetPath,

    // ---- Policy (5) ---- (S0.1 §7.3)
    /// Policy Kernel returned `deny`.
    PolicyDenied,
    /// A human or agent explicitly denied an approval request.
    ApprovalDenied,
    /// Approval TTL exceeded before grant.
    ApprovalExpired,
    /// An emergency lockout is active and rejects new actions.
    EmergencyLockout,
    /// Capability Runtime cannot reach the Policy Kernel.
    PolicyKernelUnavailable,

    // ---- Authorization / identity (3) ---- (S0.1 §7.3)
    /// The subject could not be authenticated.
    SubjectUnauthenticated,
    /// The subject lacks the required capability.
    SubjectUnauthorized,
    /// The Vault Broker denied a secret operation.
    SecretAccessDenied,

    // ---- Execution (8) ---- (S0.1 §7.3, RUNNING → FAILED)
    /// Adapter returned a generic error. `retryable` depends on the adapter.
    AdapterFailure,
    /// Adapter process exited mid-execution.
    AdapterCrashed,
    /// Adapter exceeded its execution timeout.
    AdapterTimeout,
    /// `dry_run=SIMULATE` was requested but the adapter lacks the simulate capability.
    AdapterDoesNotSupportSimulate,
    /// Adapter attempted an operation outside its sandbox profile.
    SandboxViolation,
    /// OOM, disk full, file-handle exhaustion, or similar resource pressure.
    ResourceExhausted,
    /// Explicit cancellation via `action.cancel`.
    Cancelled,
    /// System shutdown or higher-priority interrupt preempted the action.
    Preempted,

    // ---- Verification (3) ---- (S0.1 §7.3)
    /// One or more verification intents failed.
    VerificationFailed,
    /// Verification did not complete in time.
    VerificationTimeout,
    /// The requested verification grammar / type is not implemented.
    VerificationGrammarUnsupported,

    // ---- Rollback (2) ---- (S0.1 §7.3)
    /// Rollback attempted but failed — system in a degraded state (§7.7).
    RollbackFailed,
    /// No rollback path is registered for this action.
    RollbackNotPossible,

    // ---- Infrastructure (3) ---- (S0.1 §7.3)
    /// The evidence log refused a write.
    EvidenceWriteFailed,
    /// The Capability Runtime is degraded or restarting.
    RuntimeUnavailable,
    /// Unexpected CR bug; details in `message`. `retryable` depends on context.
    Internal,
}

impl ActionErrorCode {
    /// Returns the spec-mandated default for [`ActionError::retryable`] per S0.1 §7.3.
    ///
    /// * `Some(false)` — the spec fixes `retryable=false` for this code.
    /// * `Some(true)` — the spec fixes `retryable=true` for this code.
    /// * `None` — the spec says "depends" (`AdapterFailure`, `Internal`).
    ///
    /// **Informational only.** [`ActionError::new`] never consults this; callers
    /// always pass `retryable` explicitly, in line with §7.4 ("Only the originating
    /// component (CR or adapter) sets `retryable`").
    #[must_use]
    #[allow(clippy::match_same_arms)] // grouping mirrors spec §7.3 layout — preserve semantic structure
    pub const fn retryable_default(&self) -> Option<bool> {
        match self {
            // Validation — all fixed false
            Self::EnvelopeMalformed
            | Self::MissingReason
            | Self::InvalidActionName
            | Self::UnknownAdapter
            | Self::InvalidSubject
            | Self::InvalidIdempotencyKey
            | Self::IdempotencyConflict
            | Self::SchemaVersionUnsupported
            | Self::TargetSchemaInvalid
            | Self::SandboxProfileUnknown
            | Self::InvalidTargetPath => Some(false),

            // Policy
            Self::PolicyDenied | Self::ApprovalDenied | Self::EmergencyLockout => Some(false),
            Self::ApprovalExpired | Self::PolicyKernelUnavailable => Some(true),

            // Authorization
            Self::SubjectUnauthenticated | Self::SubjectUnauthorized | Self::SecretAccessDenied => {
                Some(false)
            }

            // Execution
            Self::AdapterFailure | Self::Internal => None, // "depends"
            Self::AdapterCrashed
            | Self::AdapterTimeout
            | Self::ResourceExhausted
            | Self::Preempted => Some(true),
            Self::AdapterDoesNotSupportSimulate | Self::SandboxViolation | Self::Cancelled => {
                Some(false)
            }

            // Verification
            Self::VerificationFailed | Self::VerificationGrammarUnsupported => Some(false),
            Self::VerificationTimeout => Some(true),

            // Rollback
            Self::RollbackFailed | Self::RollbackNotPossible => Some(false),

            // Infrastructure
            Self::EvidenceWriteFailed | Self::RuntimeUnavailable => Some(true),
        }
    }
}

impl fmt::Display for ActionErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::EnvelopeMalformed => "EnvelopeMalformed",
            Self::MissingReason => "MissingReason",
            Self::InvalidActionName => "InvalidActionName",
            Self::UnknownAdapter => "UnknownAdapter",
            Self::InvalidSubject => "InvalidSubject",
            Self::InvalidIdempotencyKey => "InvalidIdempotencyKey",
            Self::IdempotencyConflict => "IdempotencyConflict",
            Self::SchemaVersionUnsupported => "SchemaVersionUnsupported",
            Self::TargetSchemaInvalid => "TargetSchemaInvalid",
            Self::SandboxProfileUnknown => "SandboxProfileUnknown",
            Self::InvalidTargetPath => "InvalidTargetPath",
            Self::PolicyDenied => "PolicyDenied",
            Self::ApprovalDenied => "ApprovalDenied",
            Self::ApprovalExpired => "ApprovalExpired",
            Self::EmergencyLockout => "EmergencyLockout",
            Self::PolicyKernelUnavailable => "PolicyKernelUnavailable",
            Self::SubjectUnauthenticated => "SubjectUnauthenticated",
            Self::SubjectUnauthorized => "SubjectUnauthorized",
            Self::SecretAccessDenied => "SecretAccessDenied",
            Self::AdapterFailure => "AdapterFailure",
            Self::AdapterCrashed => "AdapterCrashed",
            Self::AdapterTimeout => "AdapterTimeout",
            Self::AdapterDoesNotSupportSimulate => "AdapterDoesNotSupportSimulate",
            Self::SandboxViolation => "SandboxViolation",
            Self::ResourceExhausted => "ResourceExhausted",
            Self::Cancelled => "Cancelled",
            Self::Preempted => "Preempted",
            Self::VerificationFailed => "VerificationFailed",
            Self::VerificationTimeout => "VerificationTimeout",
            Self::VerificationGrammarUnsupported => "VerificationGrammarUnsupported",
            Self::RollbackFailed => "RollbackFailed",
            Self::RollbackNotPossible => "RollbackNotPossible",
            Self::EvidenceWriteFailed => "EvidenceWriteFailed",
            Self::RuntimeUnavailable => "RuntimeUnavailable",
            Self::Internal => "Internal",
        };
        f.write_str(name)
    }
}

/// Canonical envelope-shaped error value per S0.1 §7.1.
///
/// This is the value carried inside `Execution.error` on a `Failed` / `RolledBack`
/// envelope, and the type returned by all `Result<_, ActionError>` surfaces in the
/// Capability Runtime gRPC API (§13.5).
///
/// Construction discipline (§7.4):
///
/// - `code` is from the closed [`ActionErrorCode`] taxonomy.
/// - `retryable` is **per-instance** and set by the originating component. Wrapping
///   the error in a cause chain does **not** override the outer `retryable`.
/// - `cause` chain is bounded at [`MAX_CAUSE_CHAIN_DEPTH`] (§7.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionError {
    /// The canonical taxonomy code (§7.3 + §13.3).
    pub code: ActionErrorCode,
    /// Human-readable message. Free-form text; not part of the closed enum.
    pub message: String,
    /// Whether the caller may safely retry with the same `idempotency_key` (§7.4).
    pub retryable: bool,
    /// Optional more-specific underlying error (§7.5). Outermost error is the
    /// highest-level cause; deeper levels are more specific. Bounded at depth 8.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<Box<Self>>,
}

impl ActionError {
    /// Construct a new error with an explicit `retryable` flag.
    ///
    /// `retryable` is **never** inferred from the code — see [`ActionErrorCode::retryable_default`]
    /// for the informational mapping, but per S0.1 §7.4 only the originating component
    /// may decide retryability.
    #[must_use]
    pub fn new(code: ActionErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
            cause: None,
        }
    }

    /// Attach a more-specific underlying cause (S0.1 §7.5).
    ///
    /// Rejects the attach if the resulting cause chain would exceed
    /// [`MAX_CAUSE_CHAIN_DEPTH`] (depth 8). The returned [`CauseChainTooDeep`] is a
    /// programmer-facing error (separate from `ActionError`) because exceeding the
    /// bound is a bug in the wrapping component, not a lifecycle failure code.
    ///
    /// # Errors
    ///
    /// Returns [`CauseChainTooDeep`] when `1 + cause.cause_chain_depth() > MAX_CAUSE_CHAIN_DEPTH`.
    pub fn with_cause(mut self, cause: Self) -> Result<Self, CauseChainTooDeep> {
        let new_depth = cause.cause_chain_depth().saturating_add(1);
        if new_depth > MAX_CAUSE_CHAIN_DEPTH {
            return Err(CauseChainTooDeep {
                observed_depth: new_depth,
            });
        }
        self.cause = Some(Box::new(cause));
        Ok(self)
    }

    /// Depth of the `cause` chain rooted at `self`.
    ///
    /// `self` with `cause: None` has depth `0`; `self` with one nested cause has
    /// depth `1`; iteratively the chain length is counted (no recursion).
    #[must_use]
    pub fn cause_chain_depth(&self) -> usize {
        let mut depth = 0usize;
        let mut current = self.cause.as_deref();
        while let Some(next) = current {
            depth = depth.saturating_add(1);
            current = next.cause.as_deref();
        }
        depth
    }
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ActionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause
            .as_deref()
            .map(|c| c as &(dyn std::error::Error + 'static))
    }
}

/// Programmer-facing error returned by [`ActionError::with_cause`] when the §7.5
/// depth-8 bound would be exceeded.
///
/// This is deliberately **not** an `ActionError` variant: hitting the bound means a
/// wrapping component is being abused (e.g. an infinite-wrap bug), not a lifecycle
/// outcome that should travel on the wire as a typed action error.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("cause chain depth {observed_depth} exceeds the §7.5 maximum of {MAX_CAUSE_CHAIN_DEPTH}")]
pub struct CauseChainTooDeep {
    /// The depth that would have resulted from the rejected attach.
    pub observed_depth: usize,
}

/// `IdError` → `ActionError` mapping.
///
/// A malformed prefix-namespaced identifier surfaces during envelope validation
/// (before policy), so it maps to [`ActionErrorCode::EnvelopeMalformed`] with
/// `retryable=false` (validation codes are fixed-false per §7.3).
impl From<IdError> for ActionError {
    fn from(err: IdError) -> Self {
        Self::new(ActionErrorCode::EnvelopeMalformed, err.to_string(), false)
    }
}

/// `TransitionError` → `ActionError` mapping.
///
/// FSM invariant violations (illegal transition, terminal violation, monotonicity
/// violation, phase-conditions mismatch) are Capability-Runtime-side state-machine
/// bugs. Per §7.3 Infrastructure group, [`ActionErrorCode::Internal`] is exactly
/// "Unexpected CR bug; details in `details`" — the original `TransitionError`
/// rendering becomes the `message`. `retryable=false` because retrying an FSM
/// invariant violation does not help.
impl From<TransitionError> for ActionError {
    fn from(err: TransitionError) -> Self {
        Self::new(ActionErrorCode::Internal, err.to_string(), false)
    }
}

/// Failure modes for the lifecycle FSM transitions and monotonicity invariants
/// (S0.1 §6.2 + §6.6 + §6.7).
///
/// Returned by `ActionEnvelope::transition_to`, `ActionEnvelope::add_condition`, and
/// `ActionEnvelope::validate_phase_conditions`. Distinct from [`ActionError`] because
/// these are programmer-facing invariant violations inside the in-process state
/// machine, not the canonical `PascalCase` error envelope codes that travel on the wire.
/// A blanket [`From`] impl maps them to [`ActionErrorCode::Internal`] when they need
/// to be surfaced as a wire-shape `ActionError`.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TransitionError {
    /// The requested phase transition is not one of the six edges enumerated in S0.1 §6.2.
    ///
    /// In particular, `Succeeded -> RolledBack`, `Pending -> Succeeded`, and every
    /// `terminal -> *` transition is rejected here.
    #[error("illegal phase transition: {from:?} -> {to:?} (S0.1 §6.2)")]
    IllegalTransition {
        /// Source phase.
        from: ActionPhase,
        /// Attempted destination phase.
        to: ActionPhase,
    },

    /// The envelope is already in a terminal phase; no further transitions are allowed
    /// (S0.1 §6.3 terminality invariant).
    #[error("envelope is already in a terminal phase; no further transitions allowed (S0.1 §6.3)")]
    TerminalPhase,

    /// A monotonicity invariant from S0.1 §6.7 was violated — typically a timestamp
    /// regression (`observed_at` earlier than the previous condition's `observed_at`)
    /// or an attempt to flip a `True` condition to `False` for the same `ConditionType`.
    #[error("monotonicity violation: {0}")]
    MonotonicityViolation(String),

    /// The condition set on the envelope does not match the canonical set required by
    /// its current phase (S0.1 §6.6 phase ↔ conditions consistency).
    #[error(
        "phase ↔ conditions mismatch: phase {phase:?} requires conditions {missing:?} to be True"
    )]
    PhaseConditionMismatch {
        /// The phase whose required conditions were not all satisfied.
        phase: ActionPhase,
        /// The conditions that must be `True` but are missing or not `True`.
        missing: Vec<ConditionType>,
    },
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::too_many_lines
)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The complete list of 35 codes in S0.1 §7.3 + §13.3 order. Kept here so the
    /// taxonomy round-trip test breaks loudly if the closed enum ever drifts.
    const ALL_CODES: [(ActionErrorCode, &str); 35] = [
        // Validation (11)
        (ActionErrorCode::EnvelopeMalformed, "EnvelopeMalformed"),
        (ActionErrorCode::MissingReason, "MissingReason"),
        (ActionErrorCode::InvalidActionName, "InvalidActionName"),
        (ActionErrorCode::UnknownAdapter, "UnknownAdapter"),
        (ActionErrorCode::InvalidSubject, "InvalidSubject"),
        (
            ActionErrorCode::InvalidIdempotencyKey,
            "InvalidIdempotencyKey",
        ),
        (ActionErrorCode::IdempotencyConflict, "IdempotencyConflict"),
        (
            ActionErrorCode::SchemaVersionUnsupported,
            "SchemaVersionUnsupported",
        ),
        (ActionErrorCode::TargetSchemaInvalid, "TargetSchemaInvalid"),
        (
            ActionErrorCode::SandboxProfileUnknown,
            "SandboxProfileUnknown",
        ),
        (ActionErrorCode::InvalidTargetPath, "InvalidTargetPath"),
        // Policy (5)
        (ActionErrorCode::PolicyDenied, "PolicyDenied"),
        (ActionErrorCode::ApprovalDenied, "ApprovalDenied"),
        (ActionErrorCode::ApprovalExpired, "ApprovalExpired"),
        (ActionErrorCode::EmergencyLockout, "EmergencyLockout"),
        (
            ActionErrorCode::PolicyKernelUnavailable,
            "PolicyKernelUnavailable",
        ),
        // Authorization (3)
        (
            ActionErrorCode::SubjectUnauthenticated,
            "SubjectUnauthenticated",
        ),
        (ActionErrorCode::SubjectUnauthorized, "SubjectUnauthorized"),
        (ActionErrorCode::SecretAccessDenied, "SecretAccessDenied"),
        // Execution (8)
        (ActionErrorCode::AdapterFailure, "AdapterFailure"),
        (ActionErrorCode::AdapterCrashed, "AdapterCrashed"),
        (ActionErrorCode::AdapterTimeout, "AdapterTimeout"),
        (
            ActionErrorCode::AdapterDoesNotSupportSimulate,
            "AdapterDoesNotSupportSimulate",
        ),
        (ActionErrorCode::SandboxViolation, "SandboxViolation"),
        (ActionErrorCode::ResourceExhausted, "ResourceExhausted"),
        (ActionErrorCode::Cancelled, "Cancelled"),
        (ActionErrorCode::Preempted, "Preempted"),
        // Verification (3)
        (ActionErrorCode::VerificationFailed, "VerificationFailed"),
        (ActionErrorCode::VerificationTimeout, "VerificationTimeout"),
        (
            ActionErrorCode::VerificationGrammarUnsupported,
            "VerificationGrammarUnsupported",
        ),
        // Rollback (2)
        (ActionErrorCode::RollbackFailed, "RollbackFailed"),
        (ActionErrorCode::RollbackNotPossible, "RollbackNotPossible"),
        // Infrastructure (3)
        (ActionErrorCode::EvidenceWriteFailed, "EvidenceWriteFailed"),
        (ActionErrorCode::RuntimeUnavailable, "RuntimeUnavailable"),
        (ActionErrorCode::Internal, "Internal"),
    ];

    #[test]
    fn taxonomy_has_exactly_thirty_five_codes() {
        assert_eq!(ALL_CODES.len(), 35);
    }

    #[test]
    fn every_code_serializes_to_its_spec_string() {
        for (code, expected) in ALL_CODES {
            let s = serde_json::to_string(&code).expect("serialize");
            // serde encodes a unit variant as a JSON string literal.
            assert_eq!(
                s,
                format!("\"{expected}\""),
                "code {code:?} did not serialize to {expected}"
            );
        }
    }

    #[test]
    fn every_code_round_trips_through_json() {
        for (code, _) in ALL_CODES {
            let s = serde_json::to_string(&code).expect("serialize");
            let back: ActionErrorCode = serde_json::from_str(&s).expect("deserialize");
            assert_eq!(back, code);
        }
    }

    #[test]
    fn display_matches_spec_string() {
        for (code, expected) in ALL_CODES {
            assert_eq!(code.to_string(), expected);
        }
    }

    #[test]
    fn retryable_default_validation_group_all_false() {
        for code in [
            ActionErrorCode::EnvelopeMalformed,
            ActionErrorCode::MissingReason,
            ActionErrorCode::InvalidActionName,
            ActionErrorCode::UnknownAdapter,
            ActionErrorCode::InvalidSubject,
            ActionErrorCode::InvalidIdempotencyKey,
            ActionErrorCode::IdempotencyConflict,
            ActionErrorCode::SchemaVersionUnsupported,
            ActionErrorCode::TargetSchemaInvalid,
            ActionErrorCode::SandboxProfileUnknown,
            ActionErrorCode::InvalidTargetPath,
        ] {
            assert_eq!(code.retryable_default(), Some(false), "code = {code:?}");
        }
    }

    #[test]
    fn retryable_default_policy_group_per_spec() {
        assert_eq!(
            ActionErrorCode::PolicyDenied.retryable_default(),
            Some(false)
        );
        assert_eq!(
            ActionErrorCode::ApprovalDenied.retryable_default(),
            Some(false)
        );
        assert_eq!(
            ActionErrorCode::EmergencyLockout.retryable_default(),
            Some(false)
        );
        assert_eq!(
            ActionErrorCode::ApprovalExpired.retryable_default(),
            Some(true)
        );
        assert_eq!(
            ActionErrorCode::PolicyKernelUnavailable.retryable_default(),
            Some(true)
        );
    }

    #[test]
    fn retryable_default_depends_codes_return_none() {
        assert_eq!(ActionErrorCode::AdapterFailure.retryable_default(), None);
        assert_eq!(ActionErrorCode::Internal.retryable_default(), None);
    }

    #[test]
    fn retryable_default_execution_group_other_codes() {
        for code in [
            ActionErrorCode::AdapterCrashed,
            ActionErrorCode::AdapterTimeout,
            ActionErrorCode::ResourceExhausted,
            ActionErrorCode::Preempted,
        ] {
            assert_eq!(code.retryable_default(), Some(true), "code = {code:?}");
        }
        for code in [
            ActionErrorCode::AdapterDoesNotSupportSimulate,
            ActionErrorCode::SandboxViolation,
            ActionErrorCode::Cancelled,
        ] {
            assert_eq!(code.retryable_default(), Some(false), "code = {code:?}");
        }
    }

    #[test]
    fn retryable_default_infrastructure_group_per_spec() {
        assert_eq!(
            ActionErrorCode::EvidenceWriteFailed.retryable_default(),
            Some(true)
        );
        assert_eq!(
            ActionErrorCode::RuntimeUnavailable.retryable_default(),
            Some(true)
        );
    }

    #[test]
    fn constructor_does_not_consult_retryable_default() {
        // AdapterFailure is "depends"; constructor MUST accept whatever the caller passes.
        let e = ActionError::new(ActionErrorCode::AdapterFailure, "boom", true);
        assert!(e.retryable);
        assert_eq!(e.code, ActionErrorCode::AdapterFailure);
        assert_eq!(e.message, "boom");
        assert!(e.cause.is_none());

        // Even for fixed-true codes, the caller's flag wins.
        let e = ActionError::new(ActionErrorCode::PolicyKernelUnavailable, "down", false);
        assert!(!e.retryable);
    }

    #[test]
    fn cause_chain_depth_counts_correctly() {
        let leaf = ActionError::new(ActionErrorCode::SecretAccessDenied, "leaf", false);
        assert_eq!(leaf.cause_chain_depth(), 0);

        let one = ActionError::new(ActionErrorCode::SandboxViolation, "mid", false)
            .with_cause(leaf)
            .expect("depth 1 must succeed");
        assert_eq!(one.cause_chain_depth(), 1);

        // Build depth 3.
        let three = ActionError::new(ActionErrorCode::AdapterFailure, "top", false)
            .with_cause(
                ActionError::new(ActionErrorCode::SandboxViolation, "mid", false)
                    .with_cause(ActionError::new(
                        ActionErrorCode::SecretAccessDenied,
                        "leaf",
                        false,
                    ))
                    .expect("d1"),
            )
            .expect("d2");
        assert_eq!(three.cause_chain_depth(), 2);
        // The top has 2 nested causes -> depth 2; with `three` itself it is 2 (chain length).
    }

    #[test]
    fn cause_chain_accepts_depth_one_through_eight() {
        // Build successively deeper chains. The deepest allowed call is `with_cause`
        // on an inner of depth 7 (resulting chain depth 8).
        let mut inner = ActionError::new(ActionErrorCode::Internal, "leaf", false);
        for level in 1..=7 {
            inner = ActionError::new(ActionErrorCode::AdapterFailure, format!("L{level}"), false)
                .with_cause(inner)
                .expect("each step <= 8 must succeed");
        }
        assert_eq!(inner.cause_chain_depth(), 7);

        // Now wrap once more to reach the chain-depth-8 ceiling.
        let at_max = ActionError::new(ActionErrorCode::AdapterFailure, "L8", false)
            .with_cause(inner)
            .expect("depth 8 must succeed (it is the inclusive max)");
        assert_eq!(at_max.cause_chain_depth(), 8);
    }

    #[test]
    fn cause_chain_rejects_depth_nine() {
        // Build a chain of depth exactly 8...
        let mut inner = ActionError::new(ActionErrorCode::Internal, "leaf", false);
        for level in 1..=8 {
            inner = ActionError::new(ActionErrorCode::AdapterFailure, format!("L{level}"), false)
                .with_cause(inner)
                .expect("<=8 ok");
        }
        assert_eq!(inner.cause_chain_depth(), 8);

        // ...then wrap once more — that would be depth 9 and MUST be rejected.
        let attempt =
            ActionError::new(ActionErrorCode::AdapterFailure, "L9", false).with_cause(inner);
        let err = attempt.expect_err("wrapping past depth 8 must fail");
        assert_eq!(err.observed_depth, 9);
    }

    #[test]
    fn from_id_error_maps_to_envelope_malformed_not_retryable() {
        let id_err = IdError::Empty;
        let e: ActionError = id_err.clone().into();
        assert_eq!(e.code, ActionErrorCode::EnvelopeMalformed);
        assert!(!e.retryable);
        assert_eq!(e.message, id_err.to_string());
        assert!(e.cause.is_none());

        let id_err2 = IdError::ColonSeparatorForbidden("act:01H".to_owned());
        let e2: ActionError = id_err2.clone().into();
        assert_eq!(e2.code, ActionErrorCode::EnvelopeMalformed);
        assert!(!e2.retryable);
        assert_eq!(e2.message, id_err2.to_string());
    }

    #[test]
    fn from_transition_error_maps_to_internal_not_retryable() {
        let t = TransitionError::IllegalTransition {
            from: ActionPhase::Pending,
            to: ActionPhase::Succeeded,
        };
        let e: ActionError = t.clone().into();
        assert_eq!(e.code, ActionErrorCode::Internal);
        assert!(!e.retryable);
        assert_eq!(e.message, t.to_string());
        assert!(e.cause.is_none());

        let t2 = TransitionError::TerminalPhase;
        let t2_msg = t2.to_string();
        let e2: ActionError = t2.into();
        assert_eq!(e2.code, ActionErrorCode::Internal);
        assert!(!e2.retryable);
        assert_eq!(e2.message, t2_msg);

        let t3 = TransitionError::MonotonicityViolation("clock went backwards".into());
        let t3_msg = t3.to_string();
        let e3: ActionError = t3.into();
        assert_eq!(e3.code, ActionErrorCode::Internal);
        assert!(!e3.retryable);
        assert_eq!(e3.message, t3_msg);

        let t4 = TransitionError::PhaseConditionMismatch {
            phase: ActionPhase::Succeeded,
            missing: vec![ConditionType::Verified],
        };
        let t4_msg = t4.to_string();
        let e4: ActionError = t4.into();
        assert_eq!(e4.code, ActionErrorCode::Internal);
        assert!(!e4.retryable);
        assert_eq!(e4.message, t4_msg);
    }

    #[test]
    fn serde_round_trip_of_three_deep_cause_chain_mirrors_spec_example() {
        // S0.1 §7.5 example: AdapterFailure -> SandboxViolation -> SecretAccessDenied (cause: null).
        let leaf = ActionError::new(
            ActionErrorCode::SecretAccessDenied,
            "vault broker policy denied raw read",
            false,
        );
        let mid = ActionError::new(
            ActionErrorCode::SandboxViolation,
            "adapter attempted secret read outside vault broker",
            false,
        )
        .with_cause(leaf)
        .expect("d1");
        let top = ActionError::new(
            ActionErrorCode::AdapterFailure,
            "systemd adapter failed to restart nginx",
            false,
        )
        .with_cause(mid)
        .expect("d2");

        let serialized = serde_json::to_value(&top).expect("serialize");
        let back: ActionError = serde_json::from_value(serialized.clone()).expect("deserialize");
        assert_eq!(back, top);

        // Shape check against the §7.5 example.
        assert_eq!(serialized["code"], json!("AdapterFailure"));
        assert_eq!(serialized["cause"]["code"], json!("SandboxViolation"));
        assert_eq!(
            serialized["cause"]["cause"]["code"],
            json!("SecretAccessDenied")
        );
        // Innermost cause must be absent (None) — not present as null — given our
        // skip_serializing_if = Option::is_none. Confirm by checking the leaf has no
        // "cause" key at all.
        assert!(serialized["cause"]["cause"].get("cause").is_none());
    }

    #[test]
    fn display_renders_code_and_message() {
        let e = ActionError::new(ActionErrorCode::PolicyDenied, "missing audit log", false);
        assert_eq!(e.to_string(), "PolicyDenied: missing audit log");
    }

    #[test]
    fn std_error_source_walks_to_cause() {
        let leaf = ActionError::new(ActionErrorCode::SecretAccessDenied, "leaf", false);
        let top = ActionError::new(ActionErrorCode::AdapterFailure, "top", false)
            .with_cause(leaf)
            .expect("d1");
        let source = std::error::Error::source(&top).expect("must have source");
        assert!(source.to_string().contains("SecretAccessDenied"));
    }

    #[test]
    fn cause_chain_too_deep_carries_observed_depth() {
        let mut inner = ActionError::new(ActionErrorCode::Internal, "leaf", false);
        for level in 1..=8 {
            inner = ActionError::new(ActionErrorCode::AdapterFailure, format!("L{level}"), false)
                .with_cause(inner)
                .expect("<=8 ok");
        }
        match ActionError::new(ActionErrorCode::AdapterFailure, "L9", false).with_cause(inner) {
            Err(CauseChainTooDeep { observed_depth }) => assert_eq!(observed_depth, 9),
            Ok(_) => panic!("should have rejected"),
        }
    }
}
