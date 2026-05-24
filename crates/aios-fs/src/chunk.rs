//! Chunk record types — S1.3 §3.2 and §7.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::validate_chunk_id;

/// Content-addressed chunk identifier: `"chk_" + full BLAKE3-256 lowercase hex`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChunkId(String);

impl ChunkId {
    /// Canonical chunk identifier prefix.
    pub const PREFIX: &'static str = "chk_";

    /// Derive a chunk id from content bytes.
    #[must_use]
    pub fn from_hash_bytes(bytes: &[u8]) -> Self {
        Self(format!("{}{}", Self::PREFIX, blake3::hash(bytes).to_hex()))
    }

    /// Validate and adopt an externally supplied chunk id.
    ///
    /// # Errors
    ///
    /// Returns a string error when the prefix is not `chk_`, the hash body is
    /// not 64 characters, or the body is not lowercase hex.
    pub fn parse(input: &str) -> Result<Self, String> {
        validate_chunk_id(input).map(Self)
    }

    /// Borrow the canonical string form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ChunkId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Ordered version reference to a chunk.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChunkRef(
    /// Referenced content-addressed chunk id.
    pub ChunkId,
);

/// Content-addressed chunk metadata record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Chunk {
    /// `"chk_<64 lowercase hex chars>"` content address.
    pub chunk_id: ChunkId,
    /// Stored byte length.
    pub size_bytes: u64,
    /// Monotonic reference count from active versions.
    pub ref_count: u32,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}
