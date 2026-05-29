//! Repository and update-channel vocabularies per S11.1 §3.2 / §3.3.
//!
//! `RepositoryKind` classifies the five repository sources from which a package
//! may be fetched.  `UpdateChannel` governs which version stream a package
//! follows and whether auto-update is permitted.

use serde::{Deserialize, Serialize};

/// Closed enum — 5 repository classes per S11.1 §3.2.
///
/// | Variant             | S11.1 label          | Admitted trust | Recovery-only? |
/// |---------------------|----------------------|----------------|----------------|
/// | `AiosRootRepo`      | `AIOS_ROOT_REPO`     | `AiosRoot`     | Conditional    |
/// | `AiosVerifiedRepo`  | `AIOS_VERIFIED_REPO` | `Verified`     | No             |
/// | `AiosCommunityRepo` | `AIOS_COMMUNITY_REPO`| `Community`    | No             |
/// | `AiosRecoveryRepo`  | `AIOS_RECOVERY_REPO` | `AiosRoot`     | Yes            |
/// | `ExternalBridge`    | `EXTERNAL_BRIDGE`    | `Community`    | No             |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryKind {
    /// Canonical AIOS-root-published packages.
    AiosRootRepo,
    /// Publishers at `Verified` trust — default ecosystem repository.
    AiosVerifiedRepo,
    /// Publishers at `Community` trust — tight sandbox defaults.
    AiosCommunityRepo,
    /// Recovery-critical packages (invariants, policy, identity, kernels).
    AiosRecoveryRepo,
    /// Bridges to Flathub, OCI registries, distro repos — never above `Community`.
    ExternalBridge,
}

/// Closed enum — 4 channels per S11.1 §3.3.
///
/// | Variant              | S11.1 label            | Auto-update? |
/// |----------------------|------------------------|--------------|
/// | `Stable`             | `STABLE`               | Yes          |
/// | `Beta`               | `BETA`                 | Opt-in only  |
/// | `RecoveryCritical`   | `RECOVERY_CRITICAL`    | Forbidden    |
/// | `DeprecatedRetention`| `DEPRECATED_RETENTION` | Forbidden    |
///
/// `DeprecatedRetention` carries the semantics from spec §3.3: no new versions
/// are published on this channel, existing installs continue, and auto-quarantine
/// triggers on the package's `eol_at` if set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    /// Default — full review, auto-update permitted within publisher's window.
    Stable,
    /// Publisher-marked beta — explicit operator opt-in per package.
    Beta,
    /// Only valid for `AiosRecoveryRepo` — updates require recovery-mode approval.
    RecoveryCritical,
    /// No new versions — existing installs continue until `eol_at` triggers.
    DeprecatedRetention,
}
