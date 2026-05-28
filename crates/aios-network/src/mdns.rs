//! mDNS / Avahi gating (S8.4 §7).
//!
//! Closed [`MdnsAvahiPosture`] with 4 variants enforcing DENY_DEFAULT everywhere
//! except explicit operator authorisation.  [`MdnsGate`] verifies Ed25519-signed
//! [`MdnsAdvertisementAllowlist`] entries and enforces per-posture deny logic.
//! Recovery and airgap postures hard-deny regardless of allowlists.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::error::NetworkPolicyError;
use crate::evidence::{NetworkEvidenceEmitter, WithEmitter};
use crate::ids::SubjectId;

// ---------------------------------------------------------------------------
// MdnsAvahiPosture — closed 4-variant (S8.4 §7)
// ---------------------------------------------------------------------------

/// Closed mDNS/Avahi posture vocabulary (S8.4 §7).
///
/// Four variants: default-deny for normal boot, hard-deny for recovery/airgap,
/// and operator-authorised (explicit allowlist).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdnsAvahiPosture {
    /// Default in NORMAL boot — deny all mDNS advertisements.
    DenyDefault,
    /// Hard-coded in recovery mode — always denies; ignores operator-authorised entries.
    RecoveryDenied,
    /// Explicit operator decision — only advertisements in the named allowlist pass.
    OperatorAuthorised {
        /// The allowlist identifier.
        allowlist_id: String,
    },
    /// When host posture is Airgap — denies regardless of allowlist.
    AirgapDenied,
}

// ---------------------------------------------------------------------------
// MdnsAdvertisement
// ---------------------------------------------------------------------------

/// A single mDNS advertisement entry (S8.4 §7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MdnsAdvertisement {
    /// Unique advertisement identifier.
    pub advertisement_id: String,
    /// Service type string (e.g. `"_http._tcp"`).
    pub service_type: String,
    /// Human-readable instance name.
    pub instance_name: String,
    /// Port the service listens on.
    pub port: u16,
    /// When the operator authorised this advertisement.
    pub authorised_at: DateTime<Utc>,
    /// Who authorised the advertisement.
    pub authoriser: SubjectId,
}

// ---------------------------------------------------------------------------
// MdnsAdvertisementAllowlist
// ---------------------------------------------------------------------------

/// An Ed25519-signed allowlist of permitted mDNS advertisements (S8.4 §7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MdnsAdvertisementAllowlist {
    /// Unique allowlist identifier.
    pub allowlist_id: String,
    /// The advertisements permitted by this allowlist.
    pub advertisements: Vec<MdnsAdvertisement>,
    /// When the allowlist was signed.
    pub signed_at: DateTime<Utc>,
    /// Hex fingerprint of the signing authority.
    pub signer_fingerprint: String,
    /// Ed25519 signature over the canonical payload.
    pub signature: Vec<u8>,
}

impl MdnsAdvertisementAllowlist {
    /// Canonical bytes signed by the authority:
    /// `allowlist_id || ad[0].canonical() || … || signed_at`.
    fn signing_payload(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(self.allowlist_id.as_bytes());
        for ad in &self.advertisements {
            payload.extend_from_slice(ad.advertisement_id.as_bytes());
            payload.extend_from_slice(ad.service_type.as_bytes());
            payload.extend_from_slice(ad.instance_name.as_bytes());
            payload.extend_from_slice(&ad.port.to_le_bytes());
        }
        payload.extend_from_slice(self.signed_at.to_rfc3339().as_bytes());
        payload
    }
}

// ---------------------------------------------------------------------------
// MdnsGate
// ---------------------------------------------------------------------------

/// Central mDNS / Avahi gate (S8.4 §7).
///
/// Holds the current posture, registered allowlists, and trusted signing
/// authorities.  [`MdnsGate::check_advertisement`] enforces per-posture deny
/// logic; the `OperatorAuthorised` path requires a matching entry in the
/// named, Ed25519-verified allowlist.
pub struct MdnsGate {
    /// Current mDNS posture.
    current_posture: RwLock<MdnsAvahiPosture>,
    /// Registered allowlists keyed by `allowlist_id`.
    allowlists: RwLock<HashMap<String, MdnsAdvertisementAllowlist>>,
    /// Trusted signing authorities keyed by hex fingerprint.
    trusted_authorities: RwLock<HashMap<String, VerifyingKey>>,
    /// Optional evidence emitter.
    emitter: RwLock<Option<Arc<dyn NetworkEvidenceEmitter>>>,
}

impl MdnsGate {
    /// Create a new gate defaulting to [`MdnsAvahiPosture::DenyDefault`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_posture: RwLock::new(MdnsAvahiPosture::DenyDefault),
            allowlists: RwLock::new(HashMap::new()),
            trusted_authorities: RwLock::new(HashMap::new()),
            emitter: RwLock::new(None),
        }
    }

    /// Register a trusted signing authority by its hex fingerprint.
    pub async fn register_authority(&self, fingerprint: &str, key: VerifyingKey) {
        self.trusted_authorities
            .write()
            .await
            .insert(fingerprint.to_owned(), key);
    }

    /// Set the current mDNS posture.
    ///
    /// # Errors
    ///
    /// Never fails for T-159 — always returns `Ok(())`.
    pub async fn set_posture(&self, new: MdnsAvahiPosture) -> Result<(), NetworkPolicyError> {
        *self.current_posture.write().await = new;
        if let Some(ref e) = *self.emitter.read().await {
            let posture = self.current_posture.read().await;
            let _ = e.emit_mdns_posture_changed(&posture).await;
        }
        Ok(())
    }

    /// Admit an Ed25519-signed allowlist into the gate.
    ///
    /// Verifies the signature against the authority identified by
    /// `signer_fingerprint`.  On success the allowlist is stored and active
    /// for lookups triggered by [`MdnsAvahiPosture::OperatorAuthorised`].
    ///
    /// # Errors
    ///
    /// Returns [`NetworkPolicyError::Internal`] with `"invalid mDNS allowlist
    /// signature"` on unknown authority, bad signature bytes, or a failed
    /// signature verification.
    pub async fn admit_allowlist(
        &self,
        list: MdnsAdvertisementAllowlist,
    ) -> Result<(), NetworkPolicyError> {
        let payload = list.signing_payload();

        let authorities = self.trusted_authorities.read().await;
        let vk = authorities.get(&list.signer_fingerprint).ok_or_else(|| {
            NetworkPolicyError::Internal(
                "invalid mDNS allowlist signature: unknown authority".into(),
            )
        })?;

        let sig = Signature::from_slice(&list.signature).map_err(|_| {
            NetworkPolicyError::Internal(
                "invalid mDNS allowlist signature: invalid signature bytes".into(),
            )
        })?;

        vk.verify_strict(&payload, &sig).map_err(|_| {
            NetworkPolicyError::Internal(
                "invalid mDNS allowlist signature: ed25519 verify failed".into(),
            )
        })?;
        drop(authorities);

        self.allowlists
            .write()
            .await
            .insert(list.allowlist_id.clone(), list);
        Ok(())
    }

    /// Check whether an mDNS advertisement is permitted under the current posture.
    ///
    /// # Errors
    ///
    /// | Posture              | Result                                     |
    /// |---------------------|--------------------------------------------|
    /// | `RecoveryDenied`     | `MdnsAdvertisementDenied("recovery-denied")` |
    /// | `AirgapDenied`       | `MdnsAdvertisementDenied("airgap-denied")`   |
    /// | `DenyDefault`        | `MdnsAdvertisementDenied("default-deny")`    |
    /// | `OperatorAuthorised` | Checks allowlist; `MdnsAdvertisementDenied("not in allowlist")` on miss |
    pub async fn check_advertisement(
        &self,
        service_type: &str,
        instance_name: &str,
        port: u16,
    ) -> Result<(), NetworkPolicyError> {
        let posture = self.current_posture.read().await;
        match &*posture {
            MdnsAvahiPosture::RecoveryDenied => {
                let reason = "recovery-denied";
                if let Some(ref e) = *self.emitter.read().await {
                    let _ = e
                        .emit_mdns_advertisement_rejected(service_type, instance_name, port, reason)
                        .await;
                }
                Err(NetworkPolicyError::MdnsAdvertisementDenied(reason.into()))
            }
            MdnsAvahiPosture::AirgapDenied => {
                let reason = "airgap-denied";
                if let Some(ref e) = *self.emitter.read().await {
                    let _ = e
                        .emit_mdns_advertisement_rejected(service_type, instance_name, port, reason)
                        .await;
                }
                Err(NetworkPolicyError::MdnsAdvertisementDenied(reason.into()))
            }
            MdnsAvahiPosture::DenyDefault => {
                let reason = "default-deny";
                if let Some(ref e) = *self.emitter.read().await {
                    let _ = e
                        .emit_mdns_advertisement_rejected(service_type, instance_name, port, reason)
                        .await;
                }
                Err(NetworkPolicyError::MdnsAdvertisementDenied(reason.into()))
            }
            MdnsAvahiPosture::OperatorAuthorised { allowlist_id } => {
                let ads = {
                    let allowlists = self.allowlists.read().await;
                    allowlists
                        .get(allowlist_id)
                        .map(|l| l.advertisements.clone())
                };
                let Some(list_ads) = ads else {
                    let reason = "not in allowlist";
                    if let Some(ref e) = *self.emitter.read().await {
                        let _ = e
                            .emit_mdns_advertisement_rejected(
                                service_type,
                                instance_name,
                                port,
                                reason,
                            )
                            .await;
                    }
                    return Err(NetworkPolicyError::MdnsAdvertisementDenied(reason.into()));
                };
                let found = list_ads.iter().any(|ad| {
                    ad.service_type == service_type
                        && ad.instance_name == instance_name
                        && ad.port == port
                });
                if found {
                    Ok(())
                } else {
                    let reason = "not in allowlist";
                    if let Some(ref e) = *self.emitter.read().await {
                        let _ = e
                            .emit_mdns_advertisement_rejected(
                                service_type,
                                instance_name,
                                port,
                                reason,
                            )
                            .await;
                    }
                    Err(NetworkPolicyError::MdnsAdvertisementDenied(reason.into()))
                }
            }
        }
    }
}

impl WithEmitter for MdnsGate {
    fn with_emitter(mut self, emitter: Option<Arc<dyn NetworkEvidenceEmitter>>) -> Self {
        self.emitter = RwLock::new(emitter);
        self
    }
}

impl Default for MdnsGate {
    #[allow(clippy::missing_const_for_fn)]
    fn default() -> Self {
        Self::new()
    }
}
