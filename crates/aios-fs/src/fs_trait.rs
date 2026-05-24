//! [`AiosFs`] async trait and read/write DTOs — S1.3 §9..§11.

#![allow(
    clippy::module_name_repetitions,
    reason = "AIOS-FS public names mirror the spec vocabulary"
)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use aios_action::ActionId;

use crate::chunk::{Chunk, ChunkRef};
use crate::error::FsError;
use crate::object::{Object, ObjectId, SubjectRef};
use crate::pointer::{Pointer, PointerId};
use crate::snapshot_id::SnapshotId;
use crate::transaction::{ConsistencyClass, TransactionId};
use crate::version::{Version, VersionId};

/// Materialized result of an object read against one SNAPSHOT head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ObjectReadResult {
    /// Stable object record.
    pub object: Object,
    /// Version reached through the object's current pointer.
    pub version: Version,
    /// Chunk metadata records referenced by the returned version.
    pub chunks: Vec<Chunk>,
    /// Snapshot id that governed the read.
    pub snapshot_id: SnapshotId,
}

/// Request to create a new object version or append a version to an existing object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ObjectWriteRequest {
    /// `None` creates a new object; `Some` appends a new version to an existing object.
    pub object_id: Option<ObjectId>,
    /// Parent versions for an existing object write.
    pub parent_version_ids: Vec<VersionId>,
    /// Ordered content chunk references for the new version.
    pub chunks: Vec<ChunkRef>,
    /// Free-form metadata delta carried by the immutable version.
    pub metadata_delta: serde_json::Value,
    /// S0.1 action id, when the write originated from an action.
    pub action_id: Option<ActionId>,
    /// L4 subject string associated with the write request.
    pub subject: SubjectRef,
}

/// Result of a successful object write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ObjectWriteResult {
    /// Object written or created.
    pub object_id: ObjectId,
    /// Version created by the write.
    pub version_id: VersionId,
    /// Transaction id minted for the atomic write/promote envelope.
    pub transaction_id: TransactionId,
    /// Head snapshot after the write was applied.
    pub snapshot_id_after: SnapshotId,
}

/// Per-call filesystem context supplied by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct FsContext {
    /// L4 subject string of the caller.
    pub subject: SubjectRef,
    /// S0.1 action id, when action-originated.
    pub action_id: Option<ActionId>,
    /// Optional optimistic SNAPSHOT expectation for write-side stale detection.
    pub expected_snapshot_id: Option<SnapshotId>,
    /// Requested consistency class.
    pub consistency_class: ConsistencyClass,
}

/// Summary of one captured AIOS-FS snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct SnapshotSummary {
    /// Content-addressed snapshot id.
    pub snapshot_id: SnapshotId,
    /// Capture timestamp.
    pub at: DateTime<Utc>,
    /// Number of objects visible in the snapshot.
    pub object_count: u64,
    /// Number of pointers visible in the snapshot.
    pub pointer_count: u64,
}

/// The AIOS-FS read/write contract — S1.3 §9..§11.
///
/// Implementations are `Send + Sync` so one handle can be shared behind an
/// `Arc<dyn AiosFs>` by the future gRPC server and policy/runtime integrations.
#[async_trait]
pub trait AiosFs: Send + Sync {
    /// Read the object through its current pointer.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::ObjectNotFound`] when the object is unknown,
    /// [`FsError::SnapshotStale`] when `snapshot_id` does not match the current
    /// head snapshot, and [`FsError::QuarantineViolation`] when the resolved version
    /// is quarantined and the caller is not recoverable under the implementation's
    /// L4 identity check.
    async fn read_object(
        &self,
        object_id: &ObjectId,
        snapshot_id: Option<&SnapshotId>,
    ) -> Result<ObjectReadResult, FsError>;

    /// Write a new object or append a new version to an existing object.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::ObjectNotFound`] for unknown existing objects,
    /// [`FsError::VersionNotFound`] for unknown parents,
    /// [`FsError::WriteRequiresParent`] when an existing-object write has no parent,
    /// and [`FsError::SnapshotStale`] when the context's expected snapshot is stale.
    async fn write_object(
        &self,
        write: ObjectWriteRequest,
        context: &FsContext,
    ) -> Result<ObjectWriteResult, FsError>;

    /// List all versions for one object.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::ObjectNotFound`] when the object is unknown.
    async fn list_versions(&self, object_id: &ObjectId) -> Result<Vec<Version>, FsError>;

    /// Resolve a pointer by id.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::PointerNotFound`] when the pointer is unknown.
    async fn resolve_pointer(&self, pointer_id: &PointerId) -> Result<Pointer, FsError>;

    /// Return a captured snapshot summary.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::Internal`] when the snapshot id has not been captured.
    async fn get_snapshot(&self, snapshot_id: &SnapshotId) -> Result<SnapshotSummary, FsError>;
}
