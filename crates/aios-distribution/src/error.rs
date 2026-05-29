//! Distribution error taxonomy per S11.1 error catalogue.
//!
//! `DistributionErrorCode` is the closed 15-code catalogue that every
//! distribution operation returns.  `DistributionError` pairs each code
//! with a structured `thiserror` payload so callers can match on the
//! code or inspect the human-readable message.

use serde::{Deserialize, Serialize};

/// Closed error code catalogue — 15 codes.
///
/// Every distribution operation that can fail returns one of these codes.
/// The catalogue is exhaustive for T-187; additional codes (e.g. for
/// bridge-admission failures) land in T-191+.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistributionErrorCode {
    /// Package ID not found in any active repository index.
    PackageNotFound,
    /// Publisher ID absent from the active publisher catalog.
    PublisherNotFound,
    /// Ed25519 signature verification failed at any chain hop.
    SignatureInvalid,
    /// Trust chain depth exceeds 3 hops from AIOS root.
    TrustChainTooDeep,
    /// Publisher is in `Deplatformed` state — all new installs rejected.
    PublisherDeplatformed,
    /// `BLAKE3(content)` does not match the signed `content_hash`.
    ContentHashMismatch,
    /// Manifest fields are structurally invalid or inconsistent.
    ManifestMalformed,
    /// Package origin repository does not match the kind admitted for its trust level.
    RepositoryKindMismatch,
    /// Version downgrade detected — blocked by monotonic version counter.
    DowngradeAttempt,
    /// Package signing key has been revoked (`revoked_at ≤ issued_at`).
    RevokedKey,
    /// Install state transition is not permitted by the FSM.
    InstallStateInvalidTransition,
    /// A mirror attempted to re-sign a package — forbidden by §10.
    MirrorReSignAttempt,
    /// First-run capability audit detected sustained drift.
    CapabilityLieDetected,
    /// A takedown is active for this publisher or package.
    TakedownActive,
    /// Unclassified internal error — should not leak to callers.
    Internal,
}

/// Distribution error enumeration with structured payloads.
///
/// Each variant carries a human-readable description.  The [`code`](Self::code)
/// method returns the corresponding [`DistributionErrorCode`] so callers can
/// match on the code without parsing the description string.
#[derive(Debug, thiserror::Error)]
pub enum DistributionError {
    /// Package not found in any active repository index.
    #[error("package not found: {0}")]
    PackageNotFound(String),

    /// Publisher not found in the active publisher catalog.
    #[error("publisher not found: {0}")]
    PublisherNotFound(String),

    /// Ed25519 signature verification failed.
    #[error("signature invalid: {0}")]
    SignatureInvalid(String),

    /// Trust chain depth exceeds 3 hops.
    #[error("trust chain too deep: {0}")]
    TrustChainTooDeep(String),

    /// Publisher is deplatformed — all new installs rejected.
    #[error("publisher deplatformed: {0}")]
    PublisherDeplatformed(String),

    /// Content hash does not match the signed manifest hash.
    #[error("content hash mismatch: {0}")]
    ContentHashMismatch(String),

    /// Manifest fields are structurally invalid.
    #[error("manifest malformed: {0}")]
    ManifestMalformed(String),

    /// Repository kind does not match the trust-level admission rules.
    #[error("repository kind mismatch: {0}")]
    RepositoryKindMismatch(String),

    /// Version downgrade blocked by monotonic counter.
    #[error("downgrade attempt: {0}")]
    DowngradeAttempt(String),

    /// Package signing key revoked.
    #[error("revoked key: {0}")]
    RevokedKey(String),

    /// Install state transition is not permitted.
    #[error("install state invalid transition: {0}")]
    InstallStateInvalidTransition(String),

    /// Mirror attempted to re-sign — forbidden by §10.
    #[error("mirror re-sign attempt: {0}")]
    MirrorReSignAttempt(String),

    /// First-run capability audit detected drift.
    #[error("capability lie detected: {0}")]
    CapabilityLieDetected(String),

    /// Takedown active for this publisher or package.
    #[error("takedown active: {0}")]
    TakedownActive(String),

    /// Unclassified internal error.
    #[error("internal error: {0}")]
    Internal(String),
}

impl DistributionError {
    /// Returns the [`DistributionErrorCode`] for this error variant.
    ///
    /// This lets callers match on the code without parsing the description.
    #[must_use]
    pub const fn code(&self) -> DistributionErrorCode {
        match self {
            Self::PackageNotFound(_) => DistributionErrorCode::PackageNotFound,
            Self::PublisherNotFound(_) => DistributionErrorCode::PublisherNotFound,
            Self::SignatureInvalid(_) => DistributionErrorCode::SignatureInvalid,
            Self::TrustChainTooDeep(_) => DistributionErrorCode::TrustChainTooDeep,
            Self::PublisherDeplatformed(_) => DistributionErrorCode::PublisherDeplatformed,
            Self::ContentHashMismatch(_) => DistributionErrorCode::ContentHashMismatch,
            Self::ManifestMalformed(_) => DistributionErrorCode::ManifestMalformed,
            Self::RepositoryKindMismatch(_) => DistributionErrorCode::RepositoryKindMismatch,
            Self::DowngradeAttempt(_) => DistributionErrorCode::DowngradeAttempt,
            Self::RevokedKey(_) => DistributionErrorCode::RevokedKey,
            Self::InstallStateInvalidTransition(_) => {
                DistributionErrorCode::InstallStateInvalidTransition
            }
            Self::MirrorReSignAttempt(_) => DistributionErrorCode::MirrorReSignAttempt,
            Self::CapabilityLieDetected(_) => DistributionErrorCode::CapabilityLieDetected,
            Self::TakedownActive(_) => DistributionErrorCode::TakedownActive,
            Self::Internal(_) => DistributionErrorCode::Internal,
        }
    }
}
