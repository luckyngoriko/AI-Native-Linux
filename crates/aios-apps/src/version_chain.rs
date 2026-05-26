//! S12.2 §5 — VersionChain for ordered package version history.
//!
//! Each `VersionChainEntry` records a registration event; the chain enforces
//! parent-version linkage and supports rollback through state transitions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

use crate::error::AppsError;
use crate::package::PackageId;

// ---------------------------------------------------------------------------
// PackageState — the runtime state of a version in the chain
// ---------------------------------------------------------------------------

/// Runtime state of a package version within a `VersionChain`.
///
/// Distinct from `PackageObjectState` (§3.3), which describes the on-disk
/// object; this enum governs which version is the currently-active head.
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
pub enum PackageState {
    /// The currently-active version of this package.
    Active,
    /// A version that is registered but not currently active.
    Inactive,
    /// Was ACTIVE; a rollback has marked this version for replacement.
    RollbackRequired,
}

// ---------------------------------------------------------------------------
// VersionChainEntry
// ---------------------------------------------------------------------------

/// One entry in a package's version chain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionChainEntry {
    /// The package id for this version.
    pub package_id: PackageId,
    /// Semver version string.
    pub version: String,
    /// When this version was registered.
    pub registered_at: DateTime<Utc>,
    /// The version this entry chains from (`None` for the initial version).
    pub parent_version: Option<String>,
    /// Current runtime state of this version.
    pub state: PackageState,
}

// ---------------------------------------------------------------------------
// VersionChain
// ---------------------------------------------------------------------------

/// Ordered version chain for a single package name.
///
/// Each call to [`append`](Self::append) links the new entry to the current
/// head. [`rollback_to`](Self::rollback_to) flips the active head through
/// state transitions.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionChain {
    entries: Vec<VersionChainEntry>,
}

impl VersionChain {
    /// Create an empty version chain.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Append a new entry to the chain.
    ///
    /// If the chain is non-empty, `entry.parent_version` must equal the
    /// version of the current last entry (the head).
    ///
    /// # Errors
    ///
    /// Returns `ValidationFailed` when the parent version does not match the
    /// current chain head.
    pub fn append(&mut self, entry: VersionChainEntry) -> Result<(), AppsError> {
        if let Some(head) = self.entries.last() {
            match &entry.parent_version {
                Some(ref parent) if parent == &head.version => {}
                _ => {
                    return Err(AppsError::ValidationFailed(format!(
                        "version chain parent mismatch: entry parent {:?} does not match head {}",
                        entry.parent_version, head.version,
                    )));
                }
            }
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Return the last entry whose state is [`PackageState::Active`], if any.
    #[must_use]
    pub fn current_active(&self) -> Option<&VersionChainEntry> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.state == PackageState::Active)
    }

    /// Rollback the active head to a previous version.
    ///
    /// Marks the current ACTIVE entry as `RollbackRequired` and sets the
    /// target version to `Active`. If no entry is currently ACTIVE, only
    /// the target is set to `Active`.
    ///
    /// # Errors
    ///
    /// Returns `ValidationFailed` when no entry matches `version`.
    pub fn rollback_to(&mut self, version: &str) -> Result<(), AppsError> {
        let target_idx = self
            .entries
            .iter()
            .position(|e| e.version == version)
            .ok_or_else(|| {
                AppsError::ValidationFailed(format!(
                    "rollback target version not found in chain: {version}"
                ))
            })?;

        // Mark the current ACTIVE entry → RollbackRequired.
        if let Some(active) = self
            .entries
            .iter_mut()
            .rev()
            .find(|e| e.state == PackageState::Active)
        {
            active.state = PackageState::RollbackRequired;
        }

        // Mark target → Active.
        self.entries[target_idx].state = PackageState::Active;

        Ok(())
    }

    /// Return the number of entries in this chain.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return `true` if the chain is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return a reference to the entries (test seam).
    #[must_use]
    pub fn entries(&self) -> &[VersionChainEntry] {
        &self.entries
    }
}
