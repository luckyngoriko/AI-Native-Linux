//! Transaction record types and read-consistency vocabulary — S1.3 §9 and §11.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

use aios_action::ActionId;

use crate::chunk::ChunkId;
use crate::id::{fresh_prefixed_ulid, validate_prefixed_ulid};
use crate::object::{ObjectId, SubjectRef};
use crate::pointer::PointerId;
use crate::version::VersionId;

/// Atomic transaction identifier: `"txn_<ULID>"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TransactionId(String);

impl TransactionId {
    /// Canonical transaction identifier prefix.
    pub const PREFIX: &'static str = "txn_";

    /// Mint a fresh transaction id.
    #[must_use]
    pub fn new() -> Self {
        Self(fresh_prefixed_ulid(Self::PREFIX))
    }

    /// Validate and adopt an externally supplied transaction id.
    ///
    /// # Errors
    ///
    /// Returns a string error when the prefix is not `txn_` or the body is not
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

impl Default for TransactionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TransactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for TransactionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Transaction state vocabulary from S1.3 §9.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransactionState {
    /// `PENDING_TX` — write and pointer operations are accumulating.
    PendingTx,
    /// `COMMITTING` — commit is in progress.
    Committing,
    /// `COMMITTED` — all pointer moves succeeded.
    Committed,
    /// `ABORTED` — transaction aborted; pointer moves did not commit.
    Aborted,
}

/// Read consistency classes from S1.3 §11.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConsistencyClass {
    /// `SNAPSHOT` — consistent snapshot across pointers at call time.
    Snapshot,
    /// `LINEARIZABLE` — latest committed state at call time.
    Linearizable,
    /// `EVENTUAL` — may lag committed state; used for cached views.
    Eventual,
}

/// Write operation recorded by a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct WriteOp {
    /// Object written by this operation.
    pub object_id: ObjectId,
    /// Version created by this operation.
    pub created_version_id: VersionId,
    /// Chunk ids written while staging this version.
    pub chunk_ids_written: Vec<ChunkId>,
}

/// Compare-and-swap pointer move operation recorded by a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct PointerMoveOp {
    /// Pointer being moved.
    pub pointer_id: PointerId,
    /// CAS expectation for the pointer's current version.
    pub expected_current_version_id: VersionId,
    /// Version the pointer should reference after commit.
    pub new_version_id: VersionId,
}

/// Atomic write/promote unit with evidence linkage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Transaction {
    /// `"txn_<ULID>"` transaction id.
    pub transaction_id: TransactionId,
    /// L4 subject that opened the transaction.
    pub subject: SubjectRef,
    /// S0.1 action id, when action-originated.
    pub action_id: Option<ActionId>,
    /// Transaction start timestamp.
    pub started_at: DateTime<Utc>,
    /// Transaction completion timestamp.
    pub completed_at: Option<DateTime<Utc>>,
    /// Transaction state.
    pub state: TransactionState,
    /// Version write operations in this transaction.
    pub writes: Vec<WriteOp>,
    /// Pointer move operations in this transaction.
    pub pointer_moves: Vec<PointerMoveOp>,
    /// Evidence receipt id emitted for the commit/abort record.
    pub evidence_receipt_id: Option<String>,
}
