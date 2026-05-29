//! Trust root chain primitives per S11.1 §4.
//!
//! This module defines the three-tier signing chain anchored at the AIOS root
//! key: AIOS root → publisher root → package signing key → signed payload.
//! The canonical chain has exactly three signatures; longer chains are rejected
//! with `TrustChainTooDeep`.
//!
//! # Structure
//!
//! - [`AiosRootKey`] — tier-1 anchor (firmware-pinned public key).
//! - [`PublisherRoot`] — tier-2 catalog entry with identity, key, and trust level.
//! - [`PackageSigningKey`] — tier-3 catalog entry with validity window.
//! - [`LinkSignature`] — thin newtype around a 64-byte Ed25519 signature
//!   representing a catalog link (AIOS root → publisher root, or publisher
//!   root → signing key).
//! - [`SignedPayload`] — opaque signed payload with signature and key references
//!   (the manifest signature landing zone for T-189).

use chrono::{DateTime, Utc};
use ed25519_dalek::VerifyingKey;

use crate::ids::{PackageSigningKeyId, PublisherRootId};
use crate::trust::PublisherTrustLevel;

// ---------------------------------------------------------------------------
// Tier-1 anchor — firmware-pinned AIOS root key
// ---------------------------------------------------------------------------

/// Tier-1 trust anchor: the firmware-pinned AIOS root public key.
///
/// Per S11.1 §4.1, the AIOS root key is an Ed25519 keypair generated at first
/// boot; its public key is embedded in the firmware/installer bundle and
/// verified at boot by a bootloader stage. This struct holds only the public
/// key — the private material lives in the vault (`vault://aios/system/root_signing`).
///
/// The AIOS root **never** signs packages directly; it signs only publisher
/// roots and publisher catalog versions. Any payload purportedly signed by
/// this key directly (bypassing the publisher tier) is rejected with
/// `TrustChainBroken`.
#[derive(Debug, Clone)]
pub struct AiosRootKey {
    /// The firmware-pinned Ed25519 public key for the AIOS root.
    pub public_key: VerifyingKey,
}

impl AiosRootKey {
    /// Creates a new AIOS root key from an Ed25519 verifying key.
    #[must_use]
    pub const fn new(public_key: VerifyingKey) -> Self {
        Self { public_key }
    }
}

// ---------------------------------------------------------------------------
// Tier-2 — publisher root catalog entry (§4.2)
// ---------------------------------------------------------------------------

/// Tier-2 catalog entry: a publisher root registered in the AIOS-root-signed
/// publisher catalog per S11.1 §4.2 / §4.5.
///
/// Each publisher root entry binds an Ed25519 public key to a publisher
/// identity, a trust level, and an activation window. The AIOS root's Ed25519
/// signature over the [`canonical_entry_bytes`](Self::canonical_entry_bytes)
/// is the tier-1→tier-2 link.
#[derive(Debug, Clone)]
pub struct PublisherRoot {
    /// Publisher root identifier — `pub:<vendor>` per S11.1 §4.2.
    pub publisher_root_id: PublisherRootId,
    /// Ed25519 public key of this publisher root.
    pub public_key: VerifyingKey,
    /// Trust level assigned by the AIOS root at onboarding.
    pub trust_level: PublisherTrustLevel,
    /// Optional pointer to the onboarding evidence record.
    pub onboarding_evidence_pointer: Option<String>,
    /// When this publisher root became active (signed by AIOS root).
    pub activated_at: DateTime<Utc>,
    /// If set, the publisher root is retired and new installs are rejected.
    /// Per §4.2, `retired_at` in the past → `TrustChainBroken`.
    pub retired_at: Option<DateTime<Utc>>,
}

impl PublisherRoot {
    /// Returns the canonical bytes that the AIOS root signed for this entry.
    ///
    /// Format (newline-delimited):
    /// ```text
    /// <32-byte verifying key bytes>
    /// <publisher_root_id>
    /// <trust_level label>
    /// <activated_at RFC 3339>
    /// ```
    ///
    /// `retired_at` and `onboarding_evidence_pointer` are NOT included in the
    /// canonical representation — they are catalog metadata updated after the
    /// original signing.
    #[must_use]
    pub fn canonical_entry_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(128);
        buf.extend_from_slice(self.public_key.as_bytes());
        buf.push(b'\n');
        buf.extend_from_slice(self.publisher_root_id.0.as_bytes());
        buf.push(b'\n');
        buf.extend_from_slice(self.trust_level.label().as_bytes());
        buf.push(b'\n');
        buf.extend_from_slice(self.activated_at.to_rfc3339().as_bytes());
        buf
    }
}

// ---------------------------------------------------------------------------
// Tier-3 — package signing key catalog entry (§4.3)
// ---------------------------------------------------------------------------

/// Tier-3 catalog entry: a package signing key registered in a publisher's
/// signing-key catalog per S11.1 §4.3 / §4.5.
///
/// Each signing key is an Ed25519 public key with a validity window. The
/// publisher root's Ed25519 signature over the
/// [`canonical_entry_bytes`](Self::canonical_entry_bytes) is the tier-2→tier-3
/// link.
#[derive(Debug, Clone)]
pub struct PackageSigningKey {
    /// Package signing key identifier — `pks:<vendor>:<role>` per S11.1 §4.3.
    pub package_signing_key_id: PackageSigningKeyId,
    /// Ed25519 public key of this signing key.
    pub public_key: VerifyingKey,
    /// Start of the key's validity window.
    pub valid_from: DateTime<Utc>,
    /// Optional end of the key's validity window.
    pub valid_until: Option<DateTime<Utc>>,
    /// If set, the key has been revoked as of this timestamp.
    /// Per §4.5, `revoked_at ≤ issued_at` → `TrustChainBroken`.
    pub revoked_at: Option<DateTime<Utc>>,
}

impl PackageSigningKey {
    /// Returns the canonical bytes that the publisher root signed for this entry.
    ///
    /// Format (newline-delimited):
    /// ```text
    /// <32-byte verifying key bytes>
    /// <package_signing_key_id>
    /// <valid_from RFC 3339>
    /// ```
    ///
    /// `valid_until` and `revoked_at` are NOT included in the canonical
    /// representation — they are catalog metadata updated after the original
    /// signing.
    #[must_use]
    pub fn canonical_entry_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(128);
        buf.extend_from_slice(self.public_key.as_bytes());
        buf.push(b'\n');
        buf.extend_from_slice(self.package_signing_key_id.0.as_bytes());
        buf.push(b'\n');
        buf.extend_from_slice(self.valid_from.to_rfc3339().as_bytes());
        buf
    }
}

// ---------------------------------------------------------------------------
// Link signatures — thin newtypes
// ---------------------------------------------------------------------------

/// A thin newtype around a 64-byte Ed25519 signature representing a catalog
/// link (AIOS root → publisher root, or publisher root → signing key).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkSignature(pub Vec<u8>);

// ---------------------------------------------------------------------------
// Signed payload — the manifest-signature landing zone
// ---------------------------------------------------------------------------

/// An opaque signed payload entering the trust chain verifier.
///
/// T-188 treats the payload as opaque bytes. T-189 will populate the `payload`
/// field with the ASCII bytes of the manifest's `manifest_canonical_hash` and
/// the `signature` field with `PackageManifest.ed25519_signature`.
#[derive(Debug, Clone)]
pub struct SignedPayload {
    /// The bytes that were signed (opaque in T-188).
    pub payload: Vec<u8>,
    /// The Ed25519 signature over `payload`, made by the package signing key.
    pub signature: Vec<u8>,
    /// The package signing key that made the signature.
    pub package_signing_key_id: PackageSigningKeyId,
    /// The publisher root that vouches for the signing key.
    pub publisher_root_id: PublisherRootId,
}

// ---------------------------------------------------------------------------
// Chain depth constant
// ---------------------------------------------------------------------------

/// Maximum allowed signature hops in a trust chain from AIOS root to payload.
///
/// Per S11.1 §4.4, the canonical chain has exactly three signatures. Chains
/// requiring more than three hops are rejected with `TrustChainTooDeep`.
pub const MAX_CHAIN_DEPTH: usize = 3;

/// Returns the depth (number of signature hops) for the canonical three-tier
/// chain: AIOS root → publisher root → package signing key → payload = 3 hops.
///
/// This is a constant function for T-188; future work may introduce
/// intermediate signing patterns that increase depth.
#[must_use]
pub const fn canonical_depth() -> usize {
    3
}
