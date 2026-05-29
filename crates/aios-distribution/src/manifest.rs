//! The `PackageManifest` contract per S11.1 §5.
//!
//! Each package ships with a signed `PackageManifest`. The manifest is the only
//! contract surface the host trusts; package contents are trusted only because the
//! manifest binds their content hash.
//!
//! # Fields (§5 proto projection)
//!
//! - **Identity:** `package_id`, `version`, `kind`, `publisher_trust`,
//!   `publisher_root_id`, `package_signing_key_id`.
//! - **Content binding:** `content_hash`, `manifest_canonical_hash`,
//!   `ed25519_signature`.
//! - **Install scope:** `installable_scope` (narrower set than the full
//!   [`InstallScope`]: `SYSTEM_ONLY`, `GROUP_ONLY`, `EITHER` — maps
//!   `GROUP_ONLY` → [`InstallScope::GroupScoped`]).
//! - **Sandbox + capability:** `required_sandbox` ([`SandboxProfileRef`]),
//!   `declared_capabilities`, `network_manifest` ([`NetworkManifestRef`]).
//! - **Lifecycle:** `issued_at`, `eol_at`, `channel`.
//! - **Repository linkage:** `originating_repository`, `mirror_url`,
//!   `mirror_semantic`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{PackageSigningKeyId, PublisherRootId};
use crate::mirror::MirrorSemantic;
use crate::package_kind::{InstallScope, PackageKind};
use crate::repository::{RepositoryKind, UpdateChannel};
use crate::trust::PublisherTrustLevel;

// ---------------------------------------------------------------------------
// Placeholder newtypes (T-197 replaces with typed cross-crate contracts)
// ---------------------------------------------------------------------------

/// Placeholder for the typed S3.2 `SandboxProfile`.
///
/// T-197 replaces this with the actual `SandboxProfile` type from the sandbox
/// composition crate (`crates/aios-sandbox`). Until then, this holds an opaque
/// canonical token (a profile name or content-addressed profile id).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxProfileRef(pub String);

/// Placeholder for the typed S8.1 `NetworkOutboundManifest`.
///
/// T-197 replaces this with the actual `NetworkOutboundManifest` type from the
/// network policy crate (`crates/aios-network`). Until then, this holds an
/// opaque canonical token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkManifestRef(pub String);

// ---------------------------------------------------------------------------
// PackageManifest — the signed contract (§5)
// ---------------------------------------------------------------------------

/// The signed package manifest per S11.1 §5.
///
/// Nineteen fields grouped by role:
///
/// | Group              | Fields                                                                   |
/// |--------------------|--------------------------------------------------------------------------|
/// | Identity           | `package_id`, `version`, `kind`, `publisher_trust`, `publisher_root_id`, |
/// |                    | `package_signing_key_id`                                                 |
/// | Content binding    | `content_hash`, `manifest_canonical_hash`, `ed25519_signature`           |
/// | Install scope      | `installable_scope` — narrower set (`SYSTEM_ONLY`, `GROUP_ONLY`,         |
/// |                    | `EITHER`); maps `GROUP_ONLY` → [`InstallScope::GroupScoped`]             |
/// | Sandbox+capability | `required_sandbox`, `declared_capabilities`, `network_manifest`          |
/// | Lifecycle          | `issued_at`, `eol_at`, `channel`                                        |
/// | Repository linkage | `originating_repository`, `mirror_url`, `mirror_semantic`                |
///
/// The manifest is the **only** contract surface the host trusts. Package
/// contents are trusted only because the manifest binds their content hash
/// (`content_hash`). The `ed25519_signature` is computed over the ASCII bytes
/// of the lowercase-hex `manifest_canonical_hash` string (§5.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageManifest {
    // ── Identity (§5 fields 1–6) ─────────────────────────────────────────
    /// Package identifier — `pkg:<vendor>:<name>`, regex
    /// `^pkg:[a-z0-9-]{1,64}:[a-z0-9-]{1,128}$`.
    pub package_id: String,

    /// Semantic version per semver.org (`MAJOR.MINOR.PATCH[-prerelease][+build]`).
    pub version: String,

    /// Closed enum — one of nine [`PackageKind`] values.
    pub kind: PackageKind,

    /// Claimed publisher trust level; cross-checked against the publisher catalog
    /// at verification time.
    pub publisher_trust: PublisherTrustLevel,

    /// Publisher root identifier — `pub:<vendor>` per S11.1 §4.2.
    pub publisher_root_id: PublisherRootId,

    /// Package signing key identifier — `pks:<vendor>:<role>` per S11.1 §4.3.
    pub package_signing_key_id: PackageSigningKeyId,

    // ── Content binding (§5 fields 7–9) ─────────────────────────────────
    /// 32-char lowercase hex (128 bits of BLAKE3) of the package content bytes.
    pub content_hash: String,

    /// 32-char lowercase hex (128 bits of BLAKE3) of the canonical JSON
    /// projection of this manifest with `ed25519_signature` cleared.
    pub manifest_canonical_hash: String,

    /// 64-byte Ed25519 signature over the ASCII bytes of the lowercase-hex
    /// `manifest_canonical_hash` string, made by the package signing key.
    pub ed25519_signature: Vec<u8>,

    // ── Install scope (§5 field 10) ─────────────────────────────────────
    /// Narrower set per §5.1: `SYSTEM_ONLY`, `GROUP_ONLY`, `EITHER`.
    ///
    /// Maps to [`InstallScope`] values:
    /// - `SYSTEM_ONLY` → [`InstallScope::SystemOnly`]
    /// - `GROUP_ONLY`  → [`InstallScope::GroupScoped`]
    /// - `EITHER`      → [`InstallScope::Either`]
    ///
    /// `USER_SCOPED` is **not** a valid manifest `installable_scope` — a
    /// manifest claiming `UserScoped` is rejected with `ManifestForged`.
    pub installable_scope: InstallScope,

    // ── Sandbox + capability (§5 fields 11–13) ──────────────────────────
    /// Placeholder [`SandboxProfileRef`] — T-197 replaces with typed S3.2
    /// `SandboxProfile`.
    pub required_sandbox: SandboxProfileRef,

    /// Capability identifiers the package will request at runtime.
    /// Empty is permitted only for [`PackageKind::Theme`].
    pub declared_capabilities: Vec<String>,

    /// Placeholder [`NetworkManifestRef`] — T-197 replaces with typed S8.1
    /// `NetworkOutboundManifest`.
    pub network_manifest: NetworkManifestRef,

    // ── Lifecycle (§5 fields 14–16) ─────────────────────────────────────
    /// When this manifest was issued (signed by the package signing key).
    pub issued_at: DateTime<Utc>,

    /// Optional end-of-life date. When set and `eol_at <= now()`, the
    /// package is auto-quarantined by the install pipeline (T-190).
    pub eol_at: Option<DateTime<Utc>>,

    /// Update channel — one of four [`UpdateChannel`] values.
    pub channel: UpdateChannel,

    // ── Repository linkage (§5 fields 17–19) ────────────────────────────
    /// Originating repository kind; set at fetch time, not by the publisher.
    pub originating_repository: RepositoryKind,

    /// Mirror URL recorded by the fetcher for evidence and mirror-blacklist
    /// tracking.
    pub mirror_url: String,

    /// Mirror semantic — `ORIGIN` / `CACHED` / `LOCAL`.
    pub mirror_semantic: MirrorSemantic,
}

impl PackageManifest {
    /// Returns a builder-style reference to construct a manifest in tests.
    ///
    /// This method exists only to signal that construction should go through
    /// the canonical path. Production code constructs manifests via
    /// deserialisation; test code may mutate fields directly.
    #[doc(hidden)]
    #[must_use]
    pub fn empty_stub() -> Self {
        Self {
            package_id: String::new(),
            version: String::new(),
            kind: PackageKind::App,
            publisher_trust: PublisherTrustLevel::Community,
            publisher_root_id: PublisherRootId(String::new()),
            package_signing_key_id: PackageSigningKeyId(String::new()),
            content_hash: String::new(),
            manifest_canonical_hash: String::new(),
            ed25519_signature: Vec::new(),
            installable_scope: InstallScope::Either,
            required_sandbox: SandboxProfileRef(String::new()),
            declared_capabilities: Vec::new(),
            network_manifest: NetworkManifestRef(String::new()),
            issued_at: Utc::now(),
            eol_at: None,
            channel: UpdateChannel::Stable,
            originating_repository: RepositoryKind::AiosVerifiedRepo,
            mirror_url: String::new(),
            mirror_semantic: MirrorSemantic::Origin,
        }
    }
}
