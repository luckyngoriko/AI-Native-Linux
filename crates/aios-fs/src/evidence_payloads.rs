//! Typed payload structs for AIOS-FS evidence emissions (S1.3 -> S3.1).

#![allow(
    clippy::module_name_repetitions,
    reason = "AIOS-FS public names mirror the spec vocabulary"
)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use aios_action::ActionId;

use crate::object::{ObjectId, SubjectRef};
use crate::quarantine::{QuarantineDisposition, QuarantineTrigger};
use crate::transaction::TransactionId;
use crate::version::VersionId;

/// Payload for `RecordType::ActionReceived` emitted by `write_object`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ActionReceivedPayload {
    /// Object written or created by the FS write.
    pub object_id: ObjectId,
    /// Version created by the FS write.
    pub version_id: VersionId,
    /// Transaction minted for the atomic write/promote envelope.
    pub transaction_id: TransactionId,
    /// Subject associated with the write request.
    pub subject: SubjectRef,
    /// Optional S0.1 action id that originated the write.
    pub action_id: Option<ActionId>,
    /// Number of chunk references in the new version.
    pub chunks_count: u64,
    /// Deterministic content hash projected from the ordered chunk refs.
    pub content_hash: String,
}

/// Payload for `RecordType::QuarantineEvent` entry and exit transitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct QuarantineEventPayload {
    /// Version affected by the quarantine transition.
    pub version_id: VersionId,
    /// Entry trigger; `None` for exit transitions.
    pub trigger: Option<QuarantineTrigger>,
    /// Exit disposition; `None` for entry transitions.
    pub disposition: Option<QuarantineDisposition>,
    /// Entry reason or exit operator note.
    pub reason: String,
    /// Timestamp at which the transition was applied.
    pub transitioned_at: DateTime<Utc>,
}

/// Conflict-resolution lifecycle kind carried by `CONFLICT_EVENT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConflictResolutionKind {
    /// Conflict was opened.
    Open,
    /// Resolution was proposed.
    Propose,
    /// Proposal or merge result was validated.
    Validate,
    /// Conflict was resolved.
    Resolve,
    /// Conflict was abandoned.
    Abandon,
}

impl ConflictResolutionKind {
    /// Parse the conflict lifecycle token used by future conflict drivers.
    ///
    /// # Errors
    ///
    /// Returns an explanatory string when `input` is not one of the closed
    /// `OPEN` / `PROPOSE` / `VALIDATE` / `RESOLVE` / `ABANDON` family tokens.
    pub fn parse_token(input: &str) -> Result<Self, String> {
        let normalized = input.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        match normalized.as_str() {
            "open" | "opened" => Ok(Self::Open),
            "propose" | "proposed" | "proposal" => Ok(Self::Propose),
            "validate" | "validated" | "validation" => Ok(Self::Validate),
            "resolve" | "resolved" | "auto_merged" | "user_resolved" => Ok(Self::Resolve),
            "abandon" | "abandoned" => Ok(Self::Abandon),
            _ => Err(format!("unknown conflict resolution kind `{input}`")),
        }
    }
}

impl TryFrom<&str> for ConflictResolutionKind {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse_token(value)
    }
}

/// Payload for `RecordType::ConflictEvent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ConflictEventPayload {
    /// Object whose pointer promotion or resolution path observed the conflict.
    pub object_id: ObjectId,
    /// Human-readable non-secret conflict summary.
    pub conflict_summary: String,
    /// Closed conflict lifecycle kind.
    pub resolution_kind: ConflictResolutionKind,
    /// Timestamp at which the event was recorded.
    pub occurred_at: DateTime<Utc>,
}

/// Payload for `RecordType::GcPass`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct GcPassPayload {
    /// Fresh GC pass id.
    pub pass_id: String,
    /// Number of zero-ref chunk candidates inspected.
    pub chunks_inspected: u64,
    /// Number of chunks reclaimed by the pass.
    pub chunks_reclaimed: u64,
    /// Number of retired version candidates inspected.
    pub versions_inspected: u64,
    /// Number of retired versions purged by the pass.
    pub versions_purged: u64,
    /// Pass start timestamp.
    pub started_at: DateTime<Utc>,
    /// Pass completion timestamp.
    pub completed_at: DateTime<Utc>,
}
