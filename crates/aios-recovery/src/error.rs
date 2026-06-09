//! Error taxonomy for the S9 recovery typed core.

use thiserror::Error;

use crate::{CandidateId, CandidateState, FirstBootPhase};

/// Errors surfaced by recovery-boundary and kernel-candidate validation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RecoveryError {
    /// Operation requires recovery mode but the host is not in recovery.
    #[error("recovery mode is not active")]
    RecoveryNotActive,
    /// Recovery entry was requested while recovery is already active.
    #[error("recovery mode is already active")]
    AlreadyInRecovery,
    /// Recovery entry or exit lacked the required operator/fallback authority.
    #[error("recovery authorization invalid: {0}")]
    RecoveryAuthorizationInvalid(String),
    /// Recovery-only namespace mutation was attempted outside recovery mode.
    #[error("recovery-only path mutation denied for {path}: {reason}")]
    RecoveryOnlyPathMutationDenied {
        /// Target path that was rejected.
        path: String,
        /// Human-readable rejection reason.
        reason: String,
    },
    /// AI subject attempted to mutate an AI-locked namespace path.
    #[error("AI path mutation denied for {path}")]
    AiPathMutationDenied {
        /// Target path that was rejected.
        path: String,
    },
    /// Recovery bundle signature verification failed.
    #[error("recovery bundle signature is invalid")]
    BundleSignatureInvalid,
    /// Recovery bundle signing authority is not trusted.
    #[error("unknown recovery bundle signing authority: {0}")]
    BundleUnknownAuthority(String),
    /// First-boot has already reached its terminal completion marker.
    #[error("first-boot has already completed")]
    FirstBootAlreadyCompleted,
    /// First-boot stage transition is forbidden by S9.2.
    #[error("invalid first-boot phase transition: {from:?} -> {to:?}")]
    InvalidPhaseTransition {
        /// Current or expected first-boot phase.
        from: FirstBootPhase,
        /// Requested next first-boot phase.
        to: FirstBootPhase,
    },
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
    /// Kernel candidate signing authority is not trusted.
    #[error("unknown kernel candidate signing authority: {0}")]
    KernelUnknownAuthority(String),
    /// Evidence receipt emission failed.
    #[error("evidence emission failed: {0}")]
    EvidenceEmitFailed(String),
    /// Internal recovery invariant failed.
    #[error("recovery internal error: {0}")]
    Internal(String),
    /// Self-healing policy validation failure.
    #[error("self-healing policy invalid: {0}")]
    SelfHealingPolicyInvalid(String),
    /// Self-healing action denied because recovery mode is not active.
    #[error("self-healing action denied — recovery not active for component: {0}")]
    SelfHealingRecoveryNotActive(String),
    /// Self-healing component not found in the policy registry.
    #[error("self-healing component unknown: {0}")]
    SelfHealingComponentUnknown(String),
    /// Self-healing retry limit exceeded, escalation required.
    #[error("self-healing escalation required for component {component}: {reason}")]
    SelfHealingEscalationRequired {
        /// Component id that needs escalation.
        component: String,
        /// Human-readable reason.
        reason: String,
    },
}
