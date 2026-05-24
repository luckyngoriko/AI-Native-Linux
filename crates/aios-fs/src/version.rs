//! Version record types — S1.3 §6 and §12.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

use aios_action::ActionId;

use crate::chunk::ChunkRef;
use crate::id::{fresh_prefixed_ulid, validate_prefixed_ulid};
use crate::object::ObjectId;
use crate::transaction::TransactionId;

/// Immutable version identifier: `"ver_<ULID>"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VersionId(String);

impl VersionId {
    /// Canonical version identifier prefix.
    pub const PREFIX: &'static str = "ver_";

    /// Mint a fresh version id.
    #[must_use]
    pub fn new() -> Self {
        Self(fresh_prefixed_ulid(Self::PREFIX))
    }

    /// Validate and adopt an externally supplied version id.
    ///
    /// # Errors
    ///
    /// Returns a string error when the prefix is not `ver_` or the body is not
    /// a valid ULID.
    pub fn parse(input: &str) -> Result<Self, String> {
        validate_prefixed_ulid(input, Self::PREFIX).map(Self)
    }

    /// Borrow the canonical string form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for VersionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for VersionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for VersionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Immutable version lifecycle state from S1.3 §6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VersionState {
    /// `STAGED` — written but not yet verified.
    Staged,
    /// `VERIFIED` — verification passed; eligible for promotion.
    Verified,
    /// `QUARANTINED` — isolated due to validation, integrity, or policy failure.
    Quarantined,
    /// `RETIRED_VERSION` — superseded; readable for audit but not promotable.
    RetiredVersion,
}

/// Immutable AIOS-FS version record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Version {
    /// `"ver_<ULID>"` immutable version id.
    pub version_id: VersionId,
    /// Owning object id.
    pub object_id: ObjectId,
    /// Parent version ids; multiple parents are allowed for merge resolution.
    pub parent_version_ids: Vec<VersionId>,
    /// Ordered content chunk references.
    pub chunk_refs: Vec<ChunkRef>,
    /// Full BLAKE3-256 lowercase hex of canonical concatenated content.
    pub content_hash: String,
    /// Free-form metadata delta for this immutable version.
    pub metadata_delta: serde_json::Value,
    /// S0.1 action id that created the version, if action-originated.
    pub created_by_action_id: Option<ActionId>,
    /// Transaction id that created the version.
    pub created_by_transaction_id: Option<TransactionId>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Version state.
    pub state: VersionState,
    /// Quarantine entry timestamp.
    pub quarantined_at: Option<DateTime<Utc>>,
    /// Quarantine reason text.
    pub quarantine_reason: Option<String>,
}
