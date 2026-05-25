//! Error taxonomy for the S9 recovery typed core.

use thiserror::Error;

use crate::{CandidateId, CandidateState};

/// Errors surfaced by recovery-boundary and kernel-candidate validation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RecoveryError {
    /// Operation requires recovery mode but the host is not in recovery.
    #[error("recovery mode is not active")]
    RecoveryNotActive,
    /// Recovery entry was requested while recovery is already active.
    #[error("recovery mode is already active")]
    AlreadyInRecovery,
    /// Recovery bundle signature verification failed.
    #[error("recovery bundle signature is invalid")]
    BundleSignatureInvalid,
    /// Recovery bundle signing authority is not trusted.
    #[error("unknown recovery bundle signing authority: {0}")]
    BundleUnknownAuthority(String),
    /// First-boot has already reached its terminal completion marker.
    #[error("first-boot has already completed")]
    FirstBootAlreadyCompleted,
    /// Kernel candidate id was not found.
    #[error("kernel candidate not found: {0}")]
    CandidateNotFound(CandidateId),
    /// Candidate state transition is forbidden by S9.3.
    #[error("invalid kernel candidate transition: {from} -> {to}")]
    InvalidCandidateTransition {
        /// Current candidate state.
        from: CandidateState,
        /// Requested next candidate state.
        to: CandidateState,
    },
    /// Kernel candidate signature verification failed.
    #[error("kernel candidate signature is invalid")]
    KernelSignatureInvalid,
    /// Internal recovery invariant failed.
    #[error("recovery internal error: {0}")]
    Internal(String),
}
