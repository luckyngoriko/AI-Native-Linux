//! Error types for the Action Envelope.
//!
//! - [`IdError`] — failures parsing prefix-namespaced ULID identifiers (S0.1 §3.2).
//! - [`ActionError`] — subset of the canonical `PascalCase` error taxonomy (S0.1 §7).
//!
//! The full ~30-code taxonomy from S0.1 §7 is implemented in task T-005; this module
//! ships the variants that appear directly on the lifecycle hot path so downstream
//! crates (`aios-capability-runtime`, `aios-policy`) can already match on them.

use thiserror::Error;

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

/// Subset of the canonical `PascalCase` action-lifecycle error taxonomy from S0.1 §7.
///
/// The full set (~30 codes spanning validation, policy, authorization, execution,
/// verification, rollback, and infrastructure) lands in T-005; this enum carries the
/// variants required by the lifecycle FSM (T-004) and the early Capability Runtime
/// integration tests (T-006).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ActionError {
    /// An action identifier failed to parse or validate (S0.1 §7.3 — validation group).
    #[error("invalid action id: {0}")]
    InvalidActionId(#[from] IdError),

    /// The `request.subject` does not match the `<type>:<name>` pattern (S0.1 §7.3).
    #[error("invalid subject: {0}")]
    InvalidSubject(String),

    /// The request payload failed schema or shape validation (S0.1 §7.3 catch-all for malformed input).
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// Same `idempotency_key` was reused with a different `hash(request)` (S0.1 §3.3 rule 2).
    #[error("idempotency conflict: key `{key}` already bound to a different request hash")]
    IdempotencyConflict {
        /// The idempotency key that collided.
        key: String,
    },

    /// Policy Kernel returned `deny` (S0.1 §7.3 — policy group).
    #[error("policy denied: {0}")]
    PolicyDenied(String),

    /// Policy required human approval (S0.1 §7.3).
    #[error("approval required: {0}")]
    ApprovalRequired(String),

    /// Approval TTL exceeded before grant (S0.1 §7.3).
    #[error("approval expired: {0}")]
    ApprovalExpired(String),

    /// No adapter is currently available to handle this action (S0.1 §7.3 — infrastructure group).
    #[error("adapter unavailable: {0}")]
    AdapterUnavailable(String),

    /// Adapter execution failed (S0.1 §7.3 — execution group, generic case).
    #[error("execution failed: {0}")]
    ExecutionFailed(String),

    /// One or more verification intents failed (S0.1 §7.3 — verification group).
    #[error("verification failed: {0}")]
    VerificationFailed(String),

    /// Rollback was attempted but failed — the most dangerous code in S0.1 §7.3 / §7.7.
    #[error("rollback failed (system in degraded state): {0}")]
    RollbackFailed(String),
}
