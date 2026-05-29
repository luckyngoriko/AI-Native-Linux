//! Install-state FSM and verification-result vocabularies per S11.1 §3.6 / §3.7.
//!
//! `PackageInstallState` is a 10-state closed FSM that governs the lifecycle of
//! a package from operator-initiated draft through validation, approval, install,
//! active runtime, quarantine, and eventual removal or failure.
//!
//! `PackageVerificationResult` is the closed set of ten outcomes returned by each
//! step in the install pipeline that can fail.

use serde::{Deserialize, Serialize};

/// Closed FSM — 10 states per S11.1 §3.6.
///
/// | Variant            | S11.1 label          | Semantics                                                                      |
/// |--------------------|----------------------|--------------------------------------------------------------------------------|
/// | `Draft`            | `DRAFT`              | Operator initiated install; not yet validated.                                 |
/// | `Validating`       | `VALIDATING`         | Signature, manifest, capability check in progress.                             |
/// | `AwaitingApproval` | `AWAITING_APPROVAL`  | Policy returned `request_approval` (S5.3); `EXACT_ACTION` binding.             |
/// | `Approved`         | `APPROVED`           | Approval granted; binding consumed; ready to install.                          |
/// | `Installing`       | `INSTALLING`         | Atomic install in progress.                                                    |
/// | `Active`           | `ACTIVE`             | Live; subject to runtime monitoring.                                          |
/// | `Quarantined`      | `QUARANTINED`        | Manifest violation, signature failure, deplatform event, capability-lie,       |
/// |                    |                      | or runtime breach. Hold state, NOT terminal.                                   |
/// | `Uninstalling`     | `UNINSTALLING`       | Active uninstall in progress; bindings revoked; files removed.                 |
/// | `Removed`          | `REMOVED`            | TERMINAL: fully uninstalled.                                                   |
/// | `InstallFailed`    | `INSTALL_FAILED`     | TERMINAL: install aborted before `Active`.                                     |
///
/// Terminal states (no outgoing transitions): `Removed`, `InstallFailed`.
/// `Active` and `Quarantined` are NOT terminal per spec §3.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageInstallState {
    /// Operator initiated install; not yet validated.
    Draft,
    /// Signature, manifest, capability check in progress.
    Validating,
    /// Policy returned `request_approval` (S5.3); approval prompt via `EXACT_ACTION`.
    AwaitingApproval,
    /// Approval granted; binding consumed; ready to install.
    Approved,
    /// Atomic install in progress (writing files, running install hooks).
    Installing,
    /// Live; subject to runtime monitoring (capability-lie audit, health checks).
    Active,
    /// Manifest violation, signature failure, deplatform event, capability-lie, or runtime breach.
    Quarantined,
    /// Active uninstall in progress; capability bindings revoked; files removed.
    Uninstalling,
    /// Terminal: package fully uninstalled.
    Removed,
    /// Terminal: install aborted before `Active`.
    InstallFailed,
}

impl PackageInstallState {
    /// Returns the canonical S11.1 label for this state.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Validating => "VALIDATING",
            Self::AwaitingApproval => "AWAITING_APPROVAL",
            Self::Approved => "APPROVED",
            Self::Installing => "INSTALLING",
            Self::Active => "ACTIVE",
            Self::Quarantined => "QUARANTINED",
            Self::Uninstalling => "UNINSTALLING",
            Self::Removed => "REMOVED",
            Self::InstallFailed => "INSTALL_FAILED",
        }
    }

    /// Returns `true` if this is a terminal state (no outgoing transitions).
    ///
    /// Per spec §3.6, only `Removed` and `InstallFailed` are terminal.
    /// `Active` and `Quarantined` are NOT terminal — they transition forward
    /// to other states.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Removed | Self::InstallFailed)
    }
}

/// Closed enum — 10 outcomes per S11.1 §3.7.
///
/// Every step in the install pipeline that can fail returns one of these
/// outcomes.  All failures emit evidence per the S11.1 §17 evidence record
/// type table.
///
/// | Variant                | S11.1 label            | Trigger                                                        |
/// |------------------------|------------------------|----------------------------------------------------------------|
/// | `VerifiedAiosRoot`     | `VERIFIED_AIOS_ROOT`   | Chain ends at AIOS root; trust level from publisher catalog.   |
/// | `VerifiedPublisher`    | `VERIFIED_PUBLISHER`   | Chain valid; publisher in Verified or Community trust.         |
/// | `SignatureFailed`      | `SIGNATURE_FAILED`     | Ed25519 verify failed at any chain hop.                        |
/// | `TrustChainBroken`     | `TRUST_CHAIN_BROKEN`   | Publisher root not signed by AIOS root, revoked, or absent.    |
/// | `TrustChainTooDeep`    | `TRUST_CHAIN_TOO_DEEP` | More than three signature hops from AIOS root.                 |
/// | `PublisherDeplatformed`| `PUBLISHER_DEPLATFORMED`| Publisher in `Deplatformed` state at fetch time.              |
/// | `HashMismatch`         | `HASH_MISMATCH`        | `BLAKE3(content)` differs from `manifest.content_hash`.        |
/// | `ManifestForged`       | `MANIFEST_FORGED`      | Manifest fields tampered post-sign.                            |
/// | `CapabilityLie`        | `CAPABILITY_LIE`       | Declared capabilities differ from runtime-observed at audit.   |
/// | `BundleTampered`       | `BUNDLE_TAMPERED`      | Executable bits in `THEME`, archive corruption, hook escape.   |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageVerificationResult {
    /// Chain ends at AIOS root; trust level inherited from publisher catalog.
    VerifiedAiosRoot,
    /// Chain valid; publisher signature good; publisher in Verified or Community trust.
    VerifiedPublisher,
    /// Ed25519 verify failed at any chain hop.
    SignatureFailed,
    /// Publisher root not signed by AIOS root, or revoked, or absent from publisher catalog.
    TrustChainBroken,
    /// More than three signature hops from AIOS root to package signing key.
    TrustChainTooDeep,
    /// Publisher in `Deplatformed` state at fetch time.
    PublisherDeplatformed,
    /// `BLAKE3(content)` differs from `manifest.content_hash`.
    HashMismatch,
    /// Manifest fields tampered post-sign (trust level claim inconsistent with catalog, etc.).
    ManifestForged,
    /// Declared capabilities differ from runtime-observed capabilities at first-run audit.
    CapabilityLie,
    /// Executable bits in a `THEME`, archive corruption, hook escape attempt, or similar tamper.
    BundleTampered,
}

impl PackageVerificationResult {
    /// Returns the canonical S11.1 label for this verification result.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::VerifiedAiosRoot => "VERIFIED_AIOS_ROOT",
            Self::VerifiedPublisher => "VERIFIED_PUBLISHER",
            Self::SignatureFailed => "SIGNATURE_FAILED",
            Self::TrustChainBroken => "TRUST_CHAIN_BROKEN",
            Self::TrustChainTooDeep => "TRUST_CHAIN_TOO_DEEP",
            Self::PublisherDeplatformed => "PUBLISHER_DEPLATFORMED",
            Self::HashMismatch => "HASH_MISMATCH",
            Self::ManifestForged => "MANIFEST_FORGED",
            Self::CapabilityLie => "CAPABILITY_LIE",
            Self::BundleTampered => "BUNDLE_TAMPERED",
        }
    }

    /// Returns `true` if this result represents a successful verification.
    ///
    /// Per spec §3.7, only `VerifiedAiosRoot` and `VerifiedPublisher` are
    /// success outcomes. All others indicate a failed verification step.
    #[must_use]
    pub const fn is_success(self) -> bool {
        matches!(self, Self::VerifiedAiosRoot | Self::VerifiedPublisher)
    }
}
