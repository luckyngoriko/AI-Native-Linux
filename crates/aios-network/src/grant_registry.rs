//! Outbound grant registry with Ed25519 signature verification (S8.1 §4, INV I7+I8).
//!
//! `OutboundGrantRegistry` maintains a set of trusted signing authorities,
//! per-subject append-only manifests, and a tombstone log. Every appended
//! grant must carry a valid Ed25519 signature from a registered authority
//! over canonical bytes (INV I7). Manifests cannot shrink in-place —
//! reduction happens via [`GrantTombstone`] (INV I8).

use std::collections::HashMap;

use chrono::Utc;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use tokio::sync::RwLock;

use crate::allowlist::AllowlistEntry;
use crate::error::NetworkPolicyError;
use crate::ids::SubjectId;
use crate::outbound_grant::{GrantTombstone, NetworkOutboundManifest, OutboundGrant};

/// Registry of outbound grants with multi-authority Ed25519 trust.
pub struct OutboundGrantRegistry {
    /// Trusted authorities keyed by hex-encoded public key fingerprint.
    trusted_authorities: HashMap<String, VerifyingKey>,
    /// Per-subject append-only manifests.
    manifests: RwLock<HashMap<SubjectId, NetworkOutboundManifest>>,
    /// Append-only tombstone log.
    tombstones: RwLock<Vec<GrantTombstone>>,
}

impl OutboundGrantRegistry {
    /// Create an empty registry with no trusted authorities.
    #[must_use]
    pub fn new() -> Self {
        Self {
            trusted_authorities: HashMap::new(),
            manifests: RwLock::new(HashMap::new()),
            tombstones: RwLock::new(Vec::new()),
        }
    }

    /// Register a trusted signing authority.
    ///
    /// The `fingerprint` should be a stable, human-recognisable identifier
    /// for the key (e.g. hex-encoded public key bytes or a named alias).
    pub fn register_authority(&mut self, fingerprint: &str, key: VerifyingKey) {
        self.trusted_authorities
            .insert(fingerprint.to_string(), key);
    }

    /// Append a grant to its subject's manifest (INV I7 + INV I8).
    ///
    /// # INV I7 — Ed25519 signature verification
    ///
    /// The grant's `signature` is verified against the canonical signing
    /// bytes using the verifying key of the authority identified by
    /// `signer_fingerprint`.
    ///
    /// # INV I8 — append-only manifesto
    ///
    /// If the new grant attempts to shrink `expires_at` relative to an
    /// existing effective grant for the same subject (mid-grant shrink),
    /// returns `ManifestMutationForbidden`.  Reduction happens via
    /// `revoke_grant` + a fresh narrower grant, not in-place mutation.
    ///
    /// # Errors
    ///
    /// Returns `GrantSignatureInvalid` when the fingerprint is not
    /// registered as a trusted authority, the signature is not 64 bytes,
    /// or the Ed25519 verification fails. Returns
    /// `ManifestMutationForbidden` when the new grant attempts to shrink
    /// the effective expiry window of an existing grant. Returns
    /// `Internal` on lock poisoning.
    pub async fn append_grant(&self, grant: OutboundGrant) -> Result<(), NetworkPolicyError> {
        // --- INV I7: signature verification ---
        let vk = self
            .trusted_authorities
            .get(&grant.signer_fingerprint)
            .ok_or_else(|| NetworkPolicyError::GrantSignatureInvalid {
                grant_id: grant.grant_id.clone(),
                reason: "unknown authority".into(),
            })?;

        let sig_array: [u8; 64] = grant.signature.as_slice().try_into().map_err(|_| {
            NetworkPolicyError::GrantSignatureInvalid {
                grant_id: grant.grant_id.clone(),
                reason: "signature is not 64 bytes".into(),
            }
        })?;

        let signature = Signature::from_bytes(&sig_array);
        let message = grant.canonical_signing_bytes();
        vk.verify_strict(&message, &signature).map_err(|_| {
            NetworkPolicyError::GrantSignatureInvalid {
                grant_id: grant.grant_id.clone(),
                reason: "ed25519 verify failed".into(),
            }
        })?;

        // --- INV I8: no in-place shrink ---
        {
            let manifests = self.manifests.read().await;
            if let Some(existing) = manifests.get(&grant.subject) {
                for existing_grant in &existing.grants {
                    let shrink_attempt = match (&grant.expires_at, &existing_grant.expires_at) {
                        (Some(new_exp), Some(existing_exp)) => new_exp < existing_exp,
                        (Some(_), None) => true,
                        (None, _) => false,
                    };
                    if shrink_attempt {
                        return Err(NetworkPolicyError::ManifestMutationForbidden(
                            "cannot shrink in-place — issue revoke + fresh grant".into(),
                        ));
                    }
                }
            }
            drop(manifests);
        }

        // --- Append grant to subject's manifest ---
        let mut manifests = self.manifests.write().await;
        let manifest =
            manifests
                .entry(grant.subject.clone())
                .or_insert_with(|| NetworkOutboundManifest {
                    subject: grant.subject.clone(),
                    grants: Vec::new(),
                    manifest_id: format!("manifest-{}", grant.subject.0),
                    created_at: Utc::now(),
                    last_appended_at: Utc::now(),
                });
        manifest.append_grant(grant);
        drop(manifests);
        Ok(())
    }

    /// Revoke a grant by appending a tombstone record (INV I8).
    ///
    /// Locates the grant in any subject's manifest and records a
    /// [`GrantTombstone`].  The grant remains in the manifest but is
    /// excluded from [`Self::effective_allowlist`].
    ///
    /// # Errors
    ///
    /// Returns `Internal` when the grant ID is not found in any manifest.
    pub async fn revoke_grant(
        &self,
        grant_id: &str,
        revoker: SubjectId,
        reason: &str,
    ) -> Result<GrantTombstone, NetworkPolicyError> {
        // Verify the grant exists in some manifest.
        let manifests = self.manifests.read().await;

        let found = manifests
            .values()
            .any(|m| m.grants.iter().any(|g| g.grant_id == grant_id));

        if !found {
            return Err(NetworkPolicyError::Internal(format!(
                "grant {grant_id} not found in any manifest"
            )));
        }
        drop(manifests);

        let tombstone = GrantTombstone {
            revoked_grant_id: grant_id.to_string(),
            revoked_at: Utc::now(),
            revoker,
            reason: reason.to_string(),
        };

        let mut tombstones = self.tombstones.write().await;
        tombstones.push(tombstone.clone());
        drop(tombstones);
        Ok(tombstone)
    }

    /// Get the manifest for a subject, if one exists.
    pub async fn get_manifest(&self, subject: &SubjectId) -> Option<NetworkOutboundManifest> {
        let manifests = self.manifests.read().await;
        manifests.get(subject).cloned()
    }

    /// List all manifests.
    pub async fn list_manifests(&self) -> Vec<NetworkOutboundManifest> {
        let guard = self.manifests.read().await;
        guard.values().cloned().collect()
    }

    /// List all tombstone records.
    pub async fn list_tombstones(&self) -> Vec<GrantTombstone> {
        let guard = self.tombstones.read().await;
        guard.clone()
    }

    /// Compute the effective allowlist for a subject.
    ///
    /// Returns the **union** of all non-tombstoned grants' allowlist
    /// entries.  Tombstones suppress their matching grant IDs.  If the
    /// subject has no manifests or all grants are tombstoned, returns
    /// an empty vector.
    pub async fn effective_allowlist(&self, subject: &SubjectId) -> Vec<AllowlistEntry> {
        let manifests = self.manifests.read().await;
        let tombstones = self.tombstones.read().await;

        let tombstoned_ids: Vec<&str> = tombstones
            .iter()
            .map(|t| t.revoked_grant_id.as_str())
            .collect();

        let mut entries = Vec::new();
        if let Some(manifest) = manifests.get(subject) {
            for grant in &manifest.grants {
                if tombstoned_ids.contains(&grant.grant_id.as_str()) {
                    continue;
                }
                for entry in &grant.allowlist {
                    entries.push(entry.clone());
                }
            }
        }
        drop(manifests);
        drop(tombstones);

        entries
    }
}

impl Default for OutboundGrantRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience: mint a fresh Ed25519 keypair for test or authority setup.
#[must_use]
pub fn generate_keypair() -> (SigningKey, VerifyingKey) {
    let sk = SigningKey::generate(&mut rand_core::OsRng);
    let vk = sk.verifying_key();
    (sk, vk)
}

/// Sign an [`OutboundGrant`] in-place using the provided signing key.
///
/// Computes the canonical signing bytes, signs them with Ed25519, and
/// stores the result in `grant.signature`.
pub fn sign_grant(grant: &mut OutboundGrant, sk: &SigningKey) {
    let message = grant.canonical_signing_bytes();
    grant.signature = sk.sign(&message).to_vec();
}

/// Derive a human-readable fingerprint from a verifying key.
///
/// Uses the hex-encoded first 16 bytes of the key for readability.
#[must_use]
pub fn fingerprint_from_vk(vk: &VerifyingKey) -> String {
    bytes_to_hex(&vk.as_bytes()[..16])
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        })
}
