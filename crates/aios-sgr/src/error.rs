//! SGR error taxonomy.

use thiserror::Error;

use crate::{UnitId, UnitState};

/// Errors surfaced by the SGR typed core.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SgrError {
    /// Unit id was not found in the runtime graph.
    #[error("unit not found: {0}")]
    UnitNotFound(UnitId),
    /// Unit id is already registered in the runtime graph.
    #[error("unit already registered: {0}")]
    UnitAlreadyRegistered(UnitId),
    /// Dependency solver found a cycle.
    #[error("dependency cycle detected: {0:?}")]
    DependencyCycleDetected(Vec<UnitId>),
    /// Unit state transition is forbidden by S15.1.
    #[error("invalid unit state transition: {from} -> {to}")]
    InvalidStateTransition {
        /// Current unit state.
        from: UnitState,
        /// Requested next unit state.
        to: UnitState,
    },
    /// Manifest signature verification failed.
    #[error("unit manifest signature is invalid")]
    ManifestSignatureInvalid,
    /// Manifest signing authority is unknown.
    #[error("unknown unit manifest authority: {0}")]
    ManifestUnknownAuthority(String),
    /// Dependency target unit is not registered.
    #[error("dependency target not registered: {0}")]
    DependencyTargetNotRegistered(UnitId),
    /// Unit adapter requirements could not be satisfied.
    #[error("adapter capability mismatch for {manifest}: missing {missing:?}")]
    AdapterCapabilityMismatch {
        /// Unit manifest whose adapter requirements were evaluated.
        manifest: UnitId,
        /// Required capability strings that no active adapter provided.
        missing: Vec<String>,
    },
    /// Adapter is suspended and cannot be selected for dispatch.
    #[error("adapter suspended: {0}")]
    AdapterSuspended(String),
    /// SGR evidence receipt emission failed.
    #[error("SGR evidence emission failed: {0}")]
    EvidenceEmitFailed(String),
    /// Internal SGR invariant failed.
    #[error("SGR internal error: {0}")]
    Internal(String),
}
