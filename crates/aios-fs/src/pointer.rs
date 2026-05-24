//! Pointer record types and pointer-kind vocabulary — S1.3 §8.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

use crate::id::{fresh_prefixed_ulid, validate_prefixed_ulid};
use crate::object::ObjectId;
use crate::transaction::TransactionId;
use crate::version::VersionId;

/// Mutable pointer identifier: `"ptr_<ULID>"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PointerId(String);

impl PointerId {
    /// Canonical pointer identifier prefix.
    pub const PREFIX: &'static str = "ptr_";

    /// Mint a fresh pointer id.
    #[must_use]
    pub fn new() -> Self {
        Self(fresh_prefixed_ulid(Self::PREFIX))
    }

    /// Validate and adopt an externally supplied pointer id.
    ///
    /// # Errors
    ///
    /// Returns a string error when the prefix is not `ptr_` or the body is not
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

impl Default for PointerId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PointerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for PointerId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Active pointer kinds from S1.3 §8.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PointerKind {
    /// `CURRENT` — active version shown in normal views.
    Current,
    /// `STABLE` — last verified stable version.
    Stable,
    /// `CANDIDATE` — staged version awaiting verification.
    Candidate,
    /// `ROLLBACK` — version to restore if promotion fails.
    Rollback,
    /// `QUARANTINE` — version isolated after validation failure.
    Quarantine,
}

/// Mutable named reference to a version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Pointer {
    /// `"ptr_<ULID>"` pointer id.
    pub pointer_id: PointerId,
    /// Owning object id.
    pub object_id: ObjectId,
    /// Pointer kind.
    pub kind: PointerKind,
    /// Version currently referenced by this pointer.
    pub current_version_id: VersionId,
    /// Last successful promotion timestamp.
    pub last_promoted_at: DateTime<Utc>,
    /// Transaction that last promoted this pointer.
    pub last_promoted_by_transaction_id: TransactionId,
}
