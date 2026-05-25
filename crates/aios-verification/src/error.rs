//! Error taxonomy for the S2.4 verification typed core.

use thiserror::Error;

use crate::{IntentId, VerificationPrimitive};

/// Errors surfaced by verification intent validation and future execution.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VerificationError {
    /// Requested primitive is outside the closed S2.4 vocabulary.
    #[error("unknown verification primitive: {0}")]
    UnknownPrimitive(String),
    /// Verification intent failed structural validation.
    #[error("invalid verification intent: {0}")]
    InvalidIntent(String),
    /// Verification exceeded its timeout budget.
    #[error("verification intent {intent_id} exceeded timeout after {after_ms} ms")]
    TimeoutExceeded {
        /// Intent that exceeded its timeout budget.
        intent_id: IntentId,
        /// Elapsed time at timeout detection.
        after_ms: u64,
    },
    /// Primitive probe failed before producing a predicate verdict.
    #[error("verification primitive {primitive} failed to execute: {reason}")]
    PrimitiveExecutionFailed {
        /// Primitive whose probe failed.
        primitive: VerificationPrimitive,
        /// Human-readable execution failure detail.
        reason: String,
    },
    /// Verification expression parsing failed.
    #[error("verification intent parse failed: {0}")]
    IntentParseFailed(String),
    /// Internal verification-engine invariant failed.
    #[error("verification internal error: {0}")]
    Internal(String),
}
