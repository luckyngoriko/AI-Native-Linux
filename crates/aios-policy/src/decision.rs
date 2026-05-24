//! Decision result types — S2.3 §4.
//!
//! `PolicyDecision` is the single output of `EvaluatePolicy` (S2.3 §3 step 12). It is
//! evidence-linked, request-hash-bound, and carries the bundle version + enrichment
//! snapshot id that produced it so the decision is fully reproducible per S2.3 §13
//! determinism.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use aios_action::ActionId;

use crate::constraints::{ApprovalRequirement, Constraints};

/// The three terminal outcomes of the decision pipeline (S2.3 §4 / §5).
///
/// `Unspecified` is reserved for proto3 wire compatibility (matches
/// `DECISION_UNSPECIFIED = 0`) and is never produced by a real evaluation —
/// every envelope produces exactly one of `Allow`, `RequireApproval`, or `Deny`
/// per the no-silent-fall-through rule of S2.3 §3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Decision {
    /// Proto3 zero-value sentinel; never produced by `EvaluatePolicy`.
    Unspecified,
    /// Action may execute (subject to attached [`Constraints`]).
    Allow,
    /// Action requires approval before execution (see [`ApprovalRequirement`]).
    RequireApproval,
    /// Action is denied; the reason is carried in [`PolicyDecision::reason_code`].
    Deny,
}

/// The full decision result returned by `EvaluatePolicy` (S2.3 §4).
///
/// All 14 fields are part of the constitutional shape; missing or renamed fields
/// break wire compatibility with the proto IDL. The future tonic-generated proto
/// struct converts into and out of this type via the conversions module that will
/// land in T-017.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDecision {
    /// `"poldec_<ULID>"` — minted per decision (S2.3 §4 field 1).
    pub policy_decision_id: String,
    /// References `ActionEnvelope.identity.action_id` (S2.3 §4 field 2).
    pub action_id: ActionId,
    /// `hex_lower(BLAKE3(canonical(request)))[:32]` (S0.1 §8.5).
    pub request_hash: String,
    /// Bundle version that produced this decision (S2.3 §4 field 4).
    pub bundle_version: String,
    /// Enrichment snapshot id for determinism (S2.3 §4 field 5 / §13).
    pub enrichment_snapshot_id: String,
    /// The terminal outcome.
    pub decision: Decision,
    /// Canonical short reason code (e.g. `"ScopedAllow"`, `"HardDeny"`).
    pub reason_code: String,
    /// English human-readable reason text.
    pub reason_message: String,
    /// Execution constraints bound to the decision (S2.3 §10).
    pub constraints: Constraints,
    /// Approval requirements when `decision == RequireApproval`.
    pub approval: ApprovalRequirement,
    /// Evidence receipt id (`"evr_..."`) recording this decision (S3.1 linkage).
    pub evidence_receipt_id: String,
    /// When the decision was finalised.
    pub evaluated_at: DateTime<Utc>,
    /// Count of policy rules consulted (for S2.3 §19 budget audit).
    pub rules_consulted: u32,
    /// True if produced by `SimulatePolicy` (S2.3 §4 field 14).
    pub simulated: bool,
}
