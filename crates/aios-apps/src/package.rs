//! S12.2 Package Object Model — closed enums and newtype identifiers.
//!
//! Defines the on-disk truth of an installed package: what kind of object it
//! is, what content it contains, what state it is in, and how rollback is
//! governed.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Canonical package identifier. Format: `pkg_<ulid26>`.
///
/// S12.2 §3 — the package id is the primary key for all package objects
/// across system, group, and user install scopes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageId(pub String);

// ---------------------------------------------------------------------------
// Closed enums — S12.2 §3.1–§3.4
// ---------------------------------------------------------------------------

/// S12.2 §3.1 — the kind of package object on disk. Eight values, exhaustive.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PackageObjectKind {
    /// A live, currently-active package on the host.
    InstalledPackage,
    /// An update bundle that has passed verification but is not yet promoted.
    StagedUpdate,
    /// A snapshot of a previously-active package retained for rollback.
    RollbackReserve,
    /// Tombstone only; binaries purged; manifest header retained for audit.
    Retired,
    /// Held inert pending operator review.
    Quarantined,
    /// Created by the install pipeline before content-hash check completes.
    Draft,
    /// A standalone probe object owned by S2.4 verification grammar.
    VerificationProbe,
    /// Upstream attestations shipped by the publisher; not executable.
    EvidenceBundle,
}

/// S12.2 §3.2 — the content classification of every file inside a package
/// object directory. Ten values, exhaustive.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PackageContentKind {
    /// ELF / Mach-O / PE / WASM / interpreter scripts.
    CodeBinaries,
    /// Read-only data: locale bundles, fonts, images, model weights.
    DataAssets,
    /// Default configuration files shipped by the publisher.
    Configuration,
    /// The single writable subdirectory the package may use at runtime.
    PrivateStateDir,
    /// Probe binaries shipped by the publisher used by S2.4.
    VerificationProbes,
    /// Small JSON manifest listing prior versions for rollback.
    RollbackPointers,
    /// Pointer to upstream EVIDENCE_BUNDLE object if any.
    EvidenceBundleRef,
    /// The composed SandboxProfile artifact (frozen output of S3.2 §5).
    SandboxProfile,
    /// The frozen NetworkOutboundManifest for this package version.
    NetworkOutboundManifest,
    /// The frozen list of capabilities the manifest declared.
    DeclaredCapabilitiesList,
}

/// S12.2 §3.3 — the state of a package object on disk **after** the install
/// pipeline completes. Distinct from S11.1 `PackageInstallState`. Eight values.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PackageObjectState {
    /// Created by the install pipeline before content-hash check completes.
    Draft,
    /// Object on disk; verification probes passed; not yet run.
    Installed,
    /// A STAGED_UPDATE for an already-installed package.
    Staged,
    /// Currently running or available to be launched.
    Active,
    /// Was ACTIVE; replaced by a newer version through promotion.
    Superseded,
    /// Was ACTIVE; rolled back; tombstone retained for forensic analysis.
    RolledBack,
    /// Held inert pending operator review.
    Quarantined,
    /// Tombstone; binaries purged; terminal state.
    Retired,
}

/// S12.2 §3.4 — the rollback policy declared by the publisher at install time.
/// Four values.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RollbackKind {
    /// Publisher declared this package is never rollback-safe.
    Never,
    /// Rollback supported only to the immediately-prior SUPERSEDED peer.
    SingleStep,
    /// Rollback supported to any SUPERSEDED peer within thirty-day window.
    MultiVersion,
    /// Rollback supported only under recovery mode.
    RecoveryOnly,
}

// ---------------------------------------------------------------------------
// PackageRecord — the top-level struct for an installed package object
// ---------------------------------------------------------------------------

/// A package object record combining identity, kind, state, and rollback policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRecord {
    /// Canonical package identifier (`pkg_<ulid26>`).
    pub package_id: PackageId,
    /// The kind of package object on disk.
    pub kind: PackageObjectKind,
    /// The content classification of files within this package.
    pub content_kinds: Vec<PackageContentKind>,
    /// Current post-install state.
    pub state: PackageObjectState,
    /// Rollback policy declared at install time.
    pub rollback_kind: RollbackKind,
    /// When the package was installed.
    pub installed_at: DateTime<Utc>,
    /// When the state last changed.
    pub state_changed_at: DateTime<Utc>,
}
