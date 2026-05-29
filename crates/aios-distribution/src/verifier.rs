//! Trust chain verifier — walks manifest→signing-key→publisher-root→AIOS-root
//! per S11.1 §4.4.
//!
//! The [`TrustChainVerifier`] implements the six-step verification walk defined
//! in the spec: verify each Ed25519 hop, enforce the exactly-three-signature
//! canonical shape, reject bypass and revoked/absent keys, and flag deplatformed
//! publishers.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, VerifyingKey};

use crate::catalog::{PublisherCatalog, SigningKeyCatalog};
use crate::install_state::PackageVerificationResult;
use crate::trust::PublisherTrustLevel;
use crate::trust_chain::{
    canonical_depth, AiosRootKey, LinkSignature, PackageSigningKey, SignedPayload, MAX_CHAIN_DEPTH,
};

// ---------------------------------------------------------------------------
// TrustChainVerifier
// ---------------------------------------------------------------------------

/// Verifies a signed payload against the three-tier trust root chain.
///
/// Holds references to the AIOS root key and the catalogs. The `verify` method
/// walks the chain from payload back to root, returning a
/// [`PackageVerificationResult`] per S11.1 §3.7.
pub struct TrustChainVerifier<'a> {
    /// The firmware-pinned AIOS root public key (tier-1 anchor).
    aios_root: &'a AiosRootKey,
    /// The AIOS-root-signed publisher catalog.
    publisher_catalog: &'a PublisherCatalog,
    /// Per-publisher signing-key catalogs keyed by vendor name.
    signing_catalogs: &'a HashMap<String, SigningKeyCatalog>,
    /// Maximum allowed chain depth (defaults to [`MAX_CHAIN_DEPTH`]).
    /// Configurable for testing only — production code uses the default.
    max_depth: usize,
}

impl<'a> TrustChainVerifier<'a> {
    /// Creates a new verifier borrowing the root key and catalogs.
    ///
    /// The maximum chain depth defaults to [`MAX_CHAIN_DEPTH`] (3).
    #[must_use]
    pub const fn new(
        aios_root: &'a AiosRootKey,
        publisher_catalog: &'a PublisherCatalog,
        signing_catalogs: &'a HashMap<String, SigningKeyCatalog>,
    ) -> Self {
        Self {
            aios_root,
            publisher_catalog,
            signing_catalogs,
            max_depth: MAX_CHAIN_DEPTH,
        }
    }

    /// Creates a verifier with a custom maximum chain depth.
    ///
    /// This is exposed for testing the depth guard. Production code should
    /// use [`new`](Self::new) which defaults to [`MAX_CHAIN_DEPTH`].
    #[doc(hidden)]
    #[must_use]
    pub const fn with_max_depth(
        aios_root: &'a AiosRootKey,
        publisher_catalog: &'a PublisherCatalog,
        signing_catalogs: &'a HashMap<String, SigningKeyCatalog>,
        max_depth: usize,
    ) -> Self {
        Self {
            aios_root,
            publisher_catalog,
            signing_catalogs,
            max_depth,
        }
    }

    /// Verifies a signed payload through the three-tier trust chain.
    ///
    /// # Parameters
    ///
    /// - `input` — the signed payload (opaque bytes + signature + key references).
    /// - `publisher_root_link_sig` — the AIOS root's Ed25519 signature over the
    ///   publisher root entry's [`PublisherRoot::canonical_entry_bytes`].
    /// - `signing_key_link_sig` — the publisher root's Ed25519 signature over the
    ///   signing key entry's [`PackageSigningKey::canonical_entry_bytes`].
    /// - `issued_at` — when the payload was issued (used for revocation checks).
    /// - `now` — current time (used for retirement / revocation checks).
    ///
    /// # Verification steps (S11.1 §4.4)
    ///
    /// 1. Look up publisher root → absent → `TrustChainBroken`.
    /// 2. Check publisher retirement status.
    /// 3. Check publisher deplatformed status.
    /// 4. Look up signing key → absent → `TrustChainBroken`.
    /// 5. Check signing key revocation status.
    /// 6. Verify publisher-root link sig against AIOS root key.
    /// 7. Verify signing-key link sig against publisher root key.
    /// 8. Verify payload sig against signing key.
    /// 9. Reject direct AIOS-root-signed bypass.
    /// 10. Enforce `MAX_CHAIN_DEPTH`.
    /// 11. Return success variant based on trust level.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn verify(
        &self,
        input: &SignedPayload,
        publisher_root_link_sig: &LinkSignature,
        signing_key_link_sig: &LinkSignature,
        issued_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> PackageVerificationResult {
        // Step 1: look up publisher root in catalog
        let Some(publisher_root) = self.publisher_catalog.lookup(&input.publisher_root_id) else {
            return PackageVerificationResult::TrustChainBroken;
        };

        // Step 2: check publisher retirement
        if let Some(retired_at) = publisher_root.retired_at {
            if retired_at <= now {
                return PackageVerificationResult::TrustChainBroken;
            }
        }

        // Step 3: check deplatformed
        if publisher_root.trust_level == PublisherTrustLevel::Deplatformed {
            return PackageVerificationResult::PublisherDeplatformed;
        }

        // Step 4: find the signing catalog for this publisher's vendor
        let vendor = vendor_from_publisher_root_id(&input.publisher_root_id);
        let Some(signing_catalog) = self.signing_catalogs.get(vendor) else {
            return PackageVerificationResult::TrustChainBroken;
        };

        // Step 5: look up signing key
        let Some(signing_key) = signing_catalog.lookup(&input.package_signing_key_id) else {
            return PackageVerificationResult::TrustChainBroken;
        };

        // Step 6: check signing key revocation
        if let Some(revoked_at) = signing_key.revoked_at {
            if revoked_at <= issued_at {
                return PackageVerificationResult::TrustChainBroken;
            }
        }

        // Step 7: verify publisher-root link sig (AIOS root → publisher root)
        let publisher_canonical = publisher_root.canonical_entry_bytes();
        if !verify_ed25519(
            &publisher_canonical,
            &publisher_root_link_sig.0,
            &self.aios_root.public_key,
        ) {
            return PackageVerificationResult::SignatureFailed;
        }

        // Step 8: verify signing-key link sig (publisher root → signing key)
        let signing_canonical = signing_key.canonical_entry_bytes();
        if !verify_ed25519(
            &signing_canonical,
            &signing_key_link_sig.0,
            &publisher_root.public_key,
        ) {
            return PackageVerificationResult::SignatureFailed;
        }

        // Step 9: verify payload signature (signing key → payload)
        if !verify_ed25519(&input.payload, &input.signature, &signing_key.public_key) {
            return PackageVerificationResult::SignatureFailed;
        }

        // Step 10: bypass detection — AIOS root must not sign payloads directly
        if is_bypass(&self.aios_root.public_key, signing_key) {
            return PackageVerificationResult::TrustChainBroken;
        }

        // Step 11: depth enforcement
        if canonical_depth() > self.max_depth {
            return PackageVerificationResult::TrustChainTooDeep;
        }

        // Step 12: classify success
        if publisher_root.trust_level == PublisherTrustLevel::AiosRoot {
            PackageVerificationResult::VerifiedAiosRoot
        } else {
            PackageVerificationResult::VerifiedPublisher
        }
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Verifies an Ed25519 signature. Returns `true` if the signature is valid.
fn verify_ed25519(payload: &[u8], signature_bytes: &[u8], verifying_key: &VerifyingKey) -> bool {
    let Ok(sig) = Signature::from_slice(signature_bytes) else {
        return false;
    };
    verifying_key.verify_strict(payload, &sig).is_ok()
}

/// Extracts the vendor name from a `PublisherRootId`.
///
/// The format is `pub:<vendor>`. Returns the `<vendor>` segment, or the full
/// ID string as a fallback if the prefix is missing.
fn vendor_from_publisher_root_id(id: &crate::ids::PublisherRootId) -> &str {
    if let Some(rest) = id.0.strip_prefix("pub:") {
        rest
    } else {
        &id.0
    }
}

/// Returns `true` if the signing key's public key equals the AIOS root's
/// public key — a bypass attempt where the AIOS root signed the payload
/// directly, skipping the publisher tier.
fn is_bypass(aios_root_pubkey: &VerifyingKey, signing_key: &PackageSigningKey) -> bool {
    signing_key.public_key.as_bytes() == aios_root_pubkey.as_bytes()
}
