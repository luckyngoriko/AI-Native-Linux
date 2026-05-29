//! Catalog primitives for the trust root chain per S11.1 §4.5.
//!
//! Two AIOS-root-signed catalogs hold the chain state on every host:
//!
//! - [`PublisherCatalog`] — publisher root entries keyed by `PublisherRootId`.
//! - [`SigningKeyCatalog`] — per-publisher signing-key entries keyed by
//!   `PackageSigningKeyId`.
//!
//! Both catalogs are pure in-memory for T-188. Signed-delta refresh and
//! catalog hashing (`pubcat_<hex>` / `pksigcat_…`) are deferred to T-191+.

use chrono::{DateTime, Utc};

use crate::ids::{PackageSigningKeyId, PublisherRootId};
use crate::trust_chain::PackageSigningKey;
use crate::trust_chain::PublisherRoot;

// ---------------------------------------------------------------------------
// PublisherCatalog — AIOS-root-signed publisher registry (§4.5)
// ---------------------------------------------------------------------------

/// AIOS-root-signed publisher catalog.
///
/// Holds all publisher root entries that the AIOS root has admitted. Lookup
/// by `PublisherRootId` returns the entry or `None` if the publisher is
/// unknown. An absent entry at verification time produces `TrustChainBroken`.
#[derive(Debug, Clone)]
pub struct PublisherCatalog {
    entries: Vec<PublisherRoot>,
}

impl PublisherCatalog {
    /// Creates a new publisher catalog from a vector of entries.
    #[must_use]
    pub const fn new(entries: Vec<PublisherRoot>) -> Self {
        Self { entries }
    }

    /// Returns the number of entries in the catalog.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the catalog contains no entries.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Looks up a publisher root entry by ID.
    ///
    /// Returns `None` if no entry with the given `PublisherRootId` exists.
    #[must_use]
    pub fn lookup(&self, id: &PublisherRootId) -> Option<&PublisherRoot> {
        self.entries.iter().find(|e| e.publisher_root_id == *id)
    }

    /// Returns `true` if the publisher is active at the given `now` timestamp.
    ///
    /// A publisher is active when:
    /// - It exists in the catalog.
    /// - `retired_at` is `None` or in the future relative to `now`.
    ///
    /// Returns `false` for absent publishers or those with `retired_at` in the
    /// past.
    #[must_use]
    pub fn is_active(&self, id: &PublisherRootId, now: &DateTime<Utc>) -> bool {
        self.lookup(id)
            .is_some_and(|entry| entry.retired_at.is_none_or(|retired| retired > *now))
    }

    /// Returns a mutable reference to a publisher root entry by ID.
    ///
    /// Returns `None` if no entry with the given `PublisherRootId` exists.
    #[must_use]
    pub fn get_mut(&mut self, id: &PublisherRootId) -> Option<&mut PublisherRoot> {
        self.entries.iter_mut().find(|e| e.publisher_root_id == *id)
    }

    /// Returns all entries in the catalog (for enumeration / listing).
    #[must_use]
    pub fn entries(&self) -> &[PublisherRoot] {
        &self.entries
    }
}

impl Default for PublisherCatalog {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

// ---------------------------------------------------------------------------
// SigningKeyCatalog — per-publisher signing-key registry (§4.5)
// ---------------------------------------------------------------------------

/// Per-publisher signing-key catalog, publisher-root-signed.
///
/// Each publisher maintains its own signing-key catalog containing all active
/// and expired (but un-revoked) signing keys. The catalog is indexed by
/// `PackageSigningKeyId`.
#[derive(Debug, Clone)]
pub struct SigningKeyCatalog {
    /// The vendor this catalog belongs to (the `<vendor>` segment of the
    /// publisher root ID).
    pub vendor: String,
    entries: Vec<PackageSigningKey>,
}

impl SigningKeyCatalog {
    /// Creates a new signing-key catalog for a vendor.
    #[must_use]
    pub const fn new(vendor: String, entries: Vec<PackageSigningKey>) -> Self {
        Self { vendor, entries }
    }

    /// Returns the number of entries in the catalog.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the catalog contains no entries.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Looks up a signing key entry by ID.
    ///
    /// Returns `None` if no entry with the given `PackageSigningKeyId` exists.
    #[must_use]
    pub fn lookup(&self, id: &PackageSigningKeyId) -> Option<&PackageSigningKey> {
        self.entries
            .iter()
            .find(|e| e.package_signing_key_id == *id)
    }

    /// Returns `true` if the signing key is revoked relative to the given
    /// `issued_at` timestamp.
    ///
    /// Per S11.1 §4.5: a signing key whose `revoked_at` predates the manifest's
    /// `issued_at` is treated as revoked → `TrustChainBroken`. A key revoked
    /// AFTER `issued_at` still verifies (continuity rule).
    #[must_use]
    pub fn is_revoked_at(&self, id: &PackageSigningKeyId, issued_at: &DateTime<Utc>) -> bool {
        self.lookup(id).is_some_and(|entry| {
            entry
                .revoked_at
                .is_some_and(|revoked| revoked <= *issued_at)
        })
    }

    /// Returns a mutable reference to a signing key entry by ID.
    ///
    /// Returns `None` if no entry with the given `PackageSigningKeyId` exists.
    #[must_use]
    pub fn get_mut(&mut self, id: &PackageSigningKeyId) -> Option<&mut PackageSigningKey> {
        self.entries
            .iter_mut()
            .find(|e| e.package_signing_key_id == *id)
    }

    /// Revokes all active (non-revoked) signing keys in this catalog and returns
    /// the IDs of the revoked keys.
    ///
    /// Used by reactive `KeyCompromise` rotation per S11.1 §11.
    #[must_use]
    pub fn revoke_all_active(&mut self, revoked_at: DateTime<Utc>) -> Vec<PackageSigningKeyId> {
        let mut revoked = Vec::new();
        for entry in &mut self.entries {
            if entry.revoked_at.is_none() {
                entry.revoked_at = Some(revoked_at);
                revoked.push(entry.package_signing_key_id.clone());
            }
        }
        revoked
    }
}
