//! AIOS-FS error taxonomy.

use thiserror::Error;

use crate::object::ObjectId;
use crate::pointer::PointerId;
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

    /// Path failed namespace validation.
    #[error("invalid AIOS path: {0}")]
    InvalidPath(String),

    /// Read or mutation attempted to cross quarantine boundaries.
    #[error("quarantine violation: {0}")]
    QuarantineViolation(String),

    /// Version state transition is not permitted by S1.3.
    #[error("invalid version transition: {from:?} -> {to:?}")]
    InvalidTransition {
        /// Current version state.
        from: VersionState,
        /// Requested version state.
        to: VersionState,
    },

    /// Unexpected internal fault.
    #[error("aios-fs internal error: {0}")]
    Internal(String),
}
