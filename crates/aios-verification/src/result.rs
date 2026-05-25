//! Verification result types and status vocabulary.

use std::fmt;

use aios_action::ActionId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{EnumCount, EnumIter};

use crate::{IntentId, VerificationPrimitive};

/// Closed S2.4 verification result status vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
pub enum VerificationStatus {
    /// `VERIFICATION_PASSED` — probe ran and observed state matched.
    #[serde(rename = "VERIFICATION_PASSED")]
    Passed,
    /// `VERIFICATION_FAILED` — probe ran but observed state did not match.
    #[serde(rename = "VERIFICATION_FAILED")]
    Failed,
    /// `VERIFICATION_TIMEOUT` — probe did not complete within its budget.
    #[serde(rename = "VERIFICATION_TIMEOUT")]
    Timeout,
    /// `VERIFICATION_PROBE_ERROR` — probe could not produce a verdict.
    #[serde(rename = "VERIFICATION_PROBE_ERROR")]
    ProbeError,
    /// `VERIFICATION_SKIPPED` — engine refused to run the probe by policy.
    #[serde(rename = "VERIFICATION_SKIPPED")]
    Skipped,
}

impl VerificationStatus {
    /// Return the exact S2.4 `VERIFICATION_*` wire token.
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::Passed => "VERIFICATION_PASSED",
            Self::Failed => "VERIFICATION_FAILED",
            Self::Timeout => "VERIFICATION_TIMEOUT",
            Self::ProbeError => "VERIFICATION_PROBE_ERROR",
            Self::Skipped => "VERIFICATION_SKIPPED",
        }
    }
}

impl fmt::Display for VerificationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

/// One primitive probe result inside a verification run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimitiveResult {
    /// Closed primitive kind that produced this result.
    pub primitive_kind: VerificationPrimitive,
    /// Boolean verdict for the primitive predicate.
    pub passed: bool,
    /// Observed probe data, shaped by the primitive-specific contract.
    pub actual: Value,
    /// Expected value or predicate input relevant to this primitive.
    pub expected: Value,
    /// Probe elapsed time in milliseconds.
    pub elapsed_ms: u64,
    /// Probe error detail when no normal predicate verdict was produced.
    pub error: Option<String>,
}

/// Top-level result for one verification intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Result identifier for evidence and trace correlation.
    pub result_id: String,
    /// Verification intent id this result answers.
    pub intent_id: IntentId,
    /// Action whose postcondition was verified.
    pub action_id: ActionId,
    /// S2.4 status for the full verification result.
    pub status: VerificationStatus,
    /// Per-primitive probe results recorded by the engine.
    pub per_primitive: Vec<PrimitiveResult>,
    /// UTC start time for the verification run.
    pub started_at: DateTime<Utc>,
    /// UTC completion time for the verification run.
    pub completed_at: DateTime<Utc>,
    /// Total verification duration in milliseconds.
    pub duration_ms: u64,
    /// Optional evidence receipt id emitted by later evidence integration.
    pub evidence_receipt_id: Option<String>,
}
