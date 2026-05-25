//! Typed payload structs for verification evidence emissions (S2.4 -> S3.1).
//!
//! The S3.1 enum exposes `VERIFICATION_RESULT` (ID 10) but not a dedicated
//! `VERIFICATION_STARTED` or per-primitive execution record. T-070 keeps the
//! payloads typed and JSON-round-trippable while the emitter maps all three
//! shapes onto the available S3.1 record type.
//!
//! INV-015 discipline: these payloads carry identifiers, status/counts,
//! timestamps, and redacted error classes only. They never include raw
//! primitive `actual` or `expected` values.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S2.4/S3.1 evidence vocabulary"
)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use aios_action::ActionId;

use crate::{IntentId, PrimitiveResult, VerificationPrimitive, VerificationStatus};

/// Payload emitted at verification-run start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct VerificationStartedPayload {
    /// Verification intent id being evaluated.
    pub intent_id: IntentId,
    /// Action whose postcondition is being verified.
    pub action_id: ActionId,
    /// BLAKE3 hash of the verification expression; raw expression is omitted.
    pub expression_hash: String,
    /// Number of primitive probes in the parsed expression.
    pub primitive_count: u64,
    /// Caller-supplied run start time.
    pub started_at: DateTime<Utc>,
}

/// Payload emitted when a verification run completes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct VerificationResultPayload {
    /// Verification intent id this result answers.
    pub intent_id: IntentId,
    /// Action whose postcondition was verified.
    pub action_id: ActionId,
    /// S3.1 status token: `PASSED`, `FAILED`, `TIMEOUT`, `PROBE_ERROR`, or `SKIPPED`.
    pub status: String,
    /// Number of primitive probe results recorded.
    pub primitive_count: u64,
    /// Number of primitive probe results with `passed = true`.
    pub passed_count: u64,
    /// Number of primitive probe results with `passed = false`.
    pub failed_count: u64,
    /// Total verification duration in milliseconds.
    pub duration_ms: u64,
    /// Completion timestamp from the verification result.
    pub completed_at: DateTime<Utc>,
}

impl VerificationResultPayload {
    /// Build the evidence payload projection from an execution result.
    #[must_use]
    pub fn from_result(result: &crate::VerificationResult) -> Self {
        let passed_count = count_len(
            result
                .per_primitive
                .iter()
                .filter(|primitive| primitive.passed)
                .count(),
        );
        let primitive_count = count_len(result.per_primitive.len());
        Self {
            intent_id: result.intent_id.clone(),
            action_id: result.action_id.clone(),
            status: verification_status_token(result.status).to_owned(),
            primitive_count,
            passed_count,
            failed_count: primitive_count.saturating_sub(passed_count),
            duration_ms: result.duration_ms,
            completed_at: result.completed_at,
        }
    }
}

/// Payload emitted for an optional per-primitive execution marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct PrimitiveExecutedPayload {
    /// Parent verification intent id.
    pub intent_id: IntentId,
    /// Closed primitive kind that produced this result.
    pub primitive_kind: VerificationPrimitive,
    /// Boolean verdict for the primitive predicate.
    pub passed: bool,
    /// Probe elapsed time in milliseconds.
    pub elapsed_ms: u64,
    /// Redacted error class when no normal predicate verdict was produced.
    pub error: Option<String>,
}

impl PrimitiveExecutedPayload {
    /// Build the redacted evidence projection from a primitive result.
    #[must_use]
    pub fn from_result(intent_id: &IntentId, result: &PrimitiveResult) -> Self {
        Self {
            intent_id: intent_id.clone(),
            primitive_kind: result.primitive_kind,
            passed: result.passed,
            elapsed_ms: result.elapsed_ms,
            error: result.error.as_deref().map(redacted_error_class),
        }
    }
}

/// Return the S3.1 compact status token for a S2.4 result status.
#[must_use]
pub const fn verification_status_token(status: VerificationStatus) -> &'static str {
    match status {
        VerificationStatus::Passed => "PASSED",
        VerificationStatus::Failed => "FAILED",
        VerificationStatus::Timeout => "TIMEOUT",
        VerificationStatus::ProbeError => "PROBE_ERROR",
        VerificationStatus::Skipped => "SKIPPED",
    }
}

fn redacted_error_class(error: &str) -> String {
    if error
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("timeout"))
    {
        return "TIMEOUT".to_owned();
    }
    if error
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("short") || word.eq_ignore_ascii_case("circuited"))
    {
        return "SHORT_CIRCUITED".to_owned();
    }
    "PROBE_ERROR".to_owned()
}

pub(crate) fn count_len(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
}
