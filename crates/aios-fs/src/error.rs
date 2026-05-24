//! AIOS-FS error taxonomy.

use thiserror::Error;

use crate::chunk::ChunkId;
use crate::object::ObjectId;
use crate::pointer::PointerId;
use crate::snapshot_id::SnapshotId;
use crate::version::{VersionId, VersionState};

/// Typed AIOS-FS error surface for future reader/writer operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FsError {
    /// Object id was not present in the object catalog.
    #[error("object not found: {0}")]
    ObjectNotFound(ObjectId),

    /// Version id was not present in the version catalog.
    #[error("version not found: {0}")]
    VersionNotFound(VersionId),

    /// Pointer id was not present in the pointer catalog.
    #[error("pointer not found: {0}")]
    PointerNotFound(PointerId),

    /// Caller supplied a stale snapshot id.
    #[error("snapshot stale: expected {expected}, found {found}")]
    SnapshotStale {
        /// Current head snapshot id.
        expected: SnapshotId,
        /// Snapshot id supplied by the caller.
        found: SnapshotId,
    },

    /// Existing object writes must name at least one parent version.
    #[error("existing object write requires parent_version_ids")]
    WriteRequiresParent,

    /// Version references a chunk unknown to the chunk catalog.
    #[error("chunk unknown: {0}")]
    ChunkUnknown(ChunkId),

    /// Chunk cannot be reclaimed while active references remain.
    #[error("chunk still referenced: {chunk_id} (refcount={refcount})")]
    ChunkStillReferenced {
        /// Chunk that was requested for reclaim.
        chunk_id: ChunkId,
        /// Current non-zero reference count.
        refcount: u32,
    },

    /// Version has already had its chunk refs purged.
    #[error("version already purged: {0}")]
    VersionAlreadyPurged(VersionId),

    /// Path failed namespace validation.
    #[error("invalid AIOS path: {0}")]
    InvalidPath(String),

    /// Namespace policy rejected a mutation request.
    #[error("namespace mutation denied for {path}: {reason}")]
    NamespaceMutationDenied {
        /// Target path that was rejected.
        path: String,
        /// Human-readable rejection reason.
        reason: String,
    },

    /// Read or mutation attempted to cross quarantine boundaries.
    #[error("quarantine violation: {0}")]
    QuarantineViolation(String),

    /// Quarantine entry was requested for a version already in quarantine.
    #[error("quarantine already applied: {0}")]
    QuarantineAlreadyApplied(VersionId),

    /// Quarantine exit was requested for a version not in quarantine.
    #[error("quarantine not applied: {0}")]
    QuarantineNotApplied(VersionId),

    /// No rollback or prior stable pointer target exists for the object.
    #[error("no prior stable pointer for object: {0}")]
    NoPriorStablePointer(ObjectId),

    /// Version state transition is not permitted by S1.3.
    #[error("invalid version transition: {from:?} -> {to:?}")]
    InvalidTransition {
        /// Current version state.
        from: VersionState,
        /// Requested version state.
        to: VersionState,
    },

    /// Query source failed to parse.
    #[error("query parse error: {0}")]
    QueryParse(String),

    /// Query evaluation failed.
    #[error("query evaluation error: {0}")]
    QueryEval(String),

    /// Implementation-space binding id was not present in the binding catalog.
    #[error("implementation-space binding not found: {0}")]
    ImplSpaceBindingNotFound(String),

    /// Implementation-space binding id already exists in the binding catalog.
    #[error("implementation-space binding duplicate: {0}")]
    ImplSpaceBindingDuplicate(String),

    /// Implementation-space target could not be reached by a backend verifier.
    #[error("implementation-space target unreachable: {0}")]
    ImplSpaceTargetUnreachable(String),

    /// Implementation-space integrity verification failed.
    #[error("implementation-space integrity failed: {0}")]
    ImplSpaceIntegrityFailed(String),

    /// Evidence receipt emission failed after an FS transition.
    #[error("evidence emission failed: {0}")]
    EvidenceEmitFailed(String),

    /// Unexpected internal fault.
    #[error("aios-fs internal error: {0}")]
    Internal(String),
}
