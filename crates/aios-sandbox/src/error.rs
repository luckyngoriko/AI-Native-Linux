use thiserror::Error;

use crate::{IsolationKind, ProfileId};

/// Closed error taxonomy for the L6 Sandbox Composition layer (S3.2).
///
/// Every error path in sandbox profile management maps to one of these
/// variants. The taxonomy is closed; adding a variant is a versioned
/// spec change.
#[derive(Debug, Error)]
pub enum SandboxError {
    /// The requested profile was not found.
    #[error("profile not found: {0}")]
    ProfileNotFound(ProfileId),

    /// The profile schema is invalid or violates structural constraints.
    #[error("invalid profile: {0}")]
    InvalidProfile(String),

    /// The profile manifest signature (Ed25519) failed verification.
    #[error("manifest signature invalid")]
    ManifestSignatureInvalid,

    /// The signing authority for this profile is not in the trust store.
    #[error("manifest signed by unknown authority: {0}")]
    ManifestUnknownAuthority(String),

    /// A resource limit constraint was violated.
    #[error("resource limit violation: {limit} = {requested} exceeds max {max}")]
    ResourceLimitsViolation {
        /// The name of the limit that was violated.
        limit: String,
        /// The requested value.
        requested: u64,
        /// The maximum allowed value.
        max: u64,
    },

    /// A GPU policy constraint was violated.
    #[error("GPU policy violation: {0}")]
    GpuPolicyViolation(String),

    /// The requested isolation kind is not supported on this host.
    #[error("isolation kind {kind:?} not supported: {reason}")]
    IsolationKindNotSupported {
        /// The requested isolation kind.
        kind: IsolationKind,
        /// Why it is not supported.
        reason: String,
    },

    /// A syscall was not in the allowlist for this isolation kind.
    #[error("syscall `{syscall}` not allowed under isolation kind {isolation_kind:?}")]
    SyscallNotAllowed {
        /// The syscall that was denied.
        syscall: String,
        /// The isolation kind under which this syscall is forbidden.
        isolation_kind: IsolationKind,
    },

    /// An internal error occurred (programmer error — should not reach the user).
    #[error("internal sandbox error: {0}")]
    Internal(String),

    /// Evidence emission failed.
    #[error("evidence emit failed: {0}")]
    EvidenceEmitFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_error_variant_has_non_empty_display() {
        let profile_id = ProfileId::new();
        let variants: &[SandboxError] = &[
            SandboxError::ProfileNotFound(profile_id),
            SandboxError::InvalidProfile("test".into()),
            SandboxError::ManifestSignatureInvalid,
            SandboxError::ManifestUnknownAuthority("test-authority".into()),
            SandboxError::ResourceLimitsViolation {
                limit: "cpu".into(),
                requested: 200,
                max: 100,
            },
            SandboxError::GpuPolicyViolation("test".into()),
            SandboxError::IsolationKindNotSupported {
                kind: IsolationKind::VmGuest,
                reason: "no KVM".into(),
            },
            SandboxError::SyscallNotAllowed {
                syscall: "mount".into(),
                isolation_kind: IsolationKind::ProcessContainer,
            },
            SandboxError::Internal("test".into()),
            SandboxError::EvidenceEmitFailed("test".into()),
        ];

        for err in variants {
            let msg = format!("{err}");
            assert!(!msg.is_empty(), "empty Display for {err:?}");
        }
    }
}
