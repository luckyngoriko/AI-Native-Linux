//! S15.3 SGR-side adapter registry.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public SGR adapter-registry names are intentionally explicit"
)]

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};
use tokio::sync::RwLock;

use crate::{AdapterCapability, AdapterDeclaration, SgrError, SgrEvidenceEmitter, UnitManifest};

/// SGR-side adapter registration state used by [`SgrAdapterRegistry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdapterRegistrationState {
    /// Registration has been accepted but is not dispatchable yet.
    Pending,
    /// Adapter is signature-verified and eligible for capability matching.
    Active,
    /// Adapter remains visible but is skipped by dispatch-bound matching.
    Suspended,
    /// Terminal state; adapter remains visible for forensic lookups.
    Retired,
}

/// Signature-verified adapter entry committed to the SGR registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredAdapter {
    /// SGR-side capability summary.
    pub capability: AdapterCapability,
    /// Original adapter declaration payload.
    pub declaration: AdapterDeclaration,
    /// Wall-clock timestamp at which the registry accepted the adapter.
    pub registered_at: DateTime<Utc>,
    /// Current SGR-side registration state.
    pub state: AdapterRegistrationState,
}

/// In-memory SGR adapter registry keyed by `AdapterCapability.capability_id`.
#[derive(Debug, Default)]
pub struct SgrAdapterRegistry {
    adapters: RwLock<HashMap<String, RegisteredAdapter>>,
    trusted_authorities: HashMap<String, VerifyingKey>,
    evidence_emitter: Option<Arc<SgrEvidenceEmitter>>,
}

impl SgrAdapterRegistry {
    /// Construct an empty registry with no trusted authorities.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct an empty registry trusting one adapter signing authority.
    #[must_use]
    pub fn with_trusted_authority(name: String, key: VerifyingKey) -> Self {
        let mut trusted_authorities = HashMap::new();
        trusted_authorities.insert(name, key);
        Self {
            adapters: RwLock::new(HashMap::new()),
            trusted_authorities,
            evidence_emitter: None,
        }
    }

    /// Attach an evidence emitter while preserving the existing registry.
    #[must_use]
    pub fn with_evidence_emitter(mut self, evidence_emitter: Arc<SgrEvidenceEmitter>) -> Self {
        self.evidence_emitter = Some(evidence_emitter);
        self
    }

    /// Register a signed adapter capability and declaration.
    ///
    /// Duplicate submissions with the exact same capability and declaration
    /// are idempotent and return the already-registered entry. A duplicate
    /// `capability_id` with a different body is rejected fail-closed.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::ManifestUnknownAuthority`] when the declaration
    /// does not identify a trusted signing key, [`SgrError::ManifestSignatureInvalid`]
    /// when the capability signature cannot be verified, and [`SgrError::Internal`]
    /// for duplicate ids with different signed bodies.
    pub async fn register_adapter(
        &self,
        capability: AdapterCapability,
        declaration: AdapterDeclaration,
    ) -> Result<RegisteredAdapter, SgrError> {
        self.verify_capability_signature(&capability, &declaration)?;

        let capability_id = capability.capability_id.clone();
        let mut adapters = self.adapters.write().await;
        if let Some(existing) = adapters.get(&capability_id) {
            if existing.capability == capability && existing.declaration == declaration {
                let existing = existing.clone();
                drop(adapters);
                return Ok(existing);
            }

            drop(adapters);
            return Err(SgrError::Internal(format!(
                "adapter capability already registered: {capability_id}"
            )));
        }

        let registered = RegisteredAdapter {
            capability,
            declaration,
            registered_at: Utc::now(),
            state: AdapterRegistrationState::Active,
        };
        adapters.insert(capability_id, registered.clone());
        drop(adapters);
        if let Some(emitter) = &self.evidence_emitter {
            emitter.emit_adapter_registered(&registered, None).await?;
        }
        Ok(registered)
    }

    /// Look up an adapter by `capability_id`.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::Internal`] when no adapter exists for the supplied
    /// `capability_id`.
    pub async fn lookup_adapter(&self, capability_id: &str) -> Result<RegisteredAdapter, SgrError> {
        let adapters = self.adapters.read().await;
        let found = adapters.get(capability_id).cloned().ok_or_else(|| {
            SgrError::Internal(format!("adapter capability not found: {capability_id}"))
        });
        drop(adapters);
        found
    }

    /// Return every registered adapter, including suspended and retired.
    pub async fn list_adapters(&self) -> Vec<RegisteredAdapter> {
        let adapters = self.adapters.read().await;
        let snapshot = adapters.values().cloned().collect();
        drop(adapters);
        snapshot
    }

    /// Find the first active adapter that satisfies every unit requirement.
    ///
    /// T-088 cannot read a typed `UnitManifest.requires` field because the
    /// T-084 `UnitManifest` shape does not contain one and is frozen for this
    /// task. Until the typed field lands, the registry reads the same list from
    /// `manifest.adapter_target.requires`.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::Internal`] when the compatibility `requires` value
    /// is present but is not a JSON list of strings.
    pub async fn find_adapter_for_unit(
        &self,
        manifest: &UnitManifest,
    ) -> Result<Option<RegisteredAdapter>, SgrError> {
        let required = manifest_required_capabilities(manifest)?;
        if required.is_empty() {
            return Ok(None);
        }

        let adapters = self.adapters.read().await;
        let mut candidates = adapters
            .values()
            .filter(|adapter| adapter.state == AdapterRegistrationState::Active)
            .cloned()
            .collect::<Vec<_>>();
        drop(adapters);
        candidates.sort_by(|left, right| {
            left.registered_at.cmp(&right.registered_at).then_with(|| {
                left.capability
                    .capability_id
                    .cmp(&right.capability.capability_id)
            })
        });

        let found = candidates.into_iter().find(|adapter| {
            required
                .iter()
                .all(|required| adapter.capability.provides.contains(required))
        });
        Ok(found)
    }

    /// Mark an adapter as suspended while keeping it visible to lookup/list.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::Internal`] when `capability_id` is unknown.
    pub async fn suspend_adapter(
        &self,
        capability_id: &str,
        _reason: &str,
    ) -> Result<(), SgrError> {
        let mut adapters = self.adapters.write().await;
        let adapter = adapters.get_mut(capability_id).ok_or_else(|| {
            SgrError::Internal(format!("adapter capability not found: {capability_id}"))
        })?;
        adapter.state = AdapterRegistrationState::Suspended;
        drop(adapters);
        Ok(())
    }

    /// Mark an adapter as retired while keeping it visible to lookup/list.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::Internal`] when `capability_id` is unknown.
    pub async fn retire_adapter(&self, capability_id: &str) -> Result<(), SgrError> {
        let mut adapters = self.adapters.write().await;
        let adapter = adapters.get_mut(capability_id).ok_or_else(|| {
            SgrError::Internal(format!("adapter capability not found: {capability_id}"))
        })?;
        adapter.state = AdapterRegistrationState::Retired;
        drop(adapters);
        Ok(())
    }

    fn verify_capability_signature(
        &self,
        capability: &AdapterCapability,
        declaration: &AdapterDeclaration,
    ) -> Result<(), SgrError> {
        let signing_key_id = declaration_signing_key_id(declaration)?;
        let verifying_key = self
            .trusted_authorities
            .get(signing_key_id)
            .ok_or_else(|| SgrError::ManifestUnknownAuthority(signing_key_id.to_owned()))?;
        let signature_bytes: [u8; 64] = capability
            .manifest_signature_ed25519
            .as_slice()
            .try_into()
            .map_err(|_| SgrError::ManifestSignatureInvalid)?;
        let signature = Signature::from_bytes(&signature_bytes);
        let body = canonical_capability_bytes(capability)?;

        verifying_key
            .verify(&body, &signature)
            .map_err(|_| SgrError::ManifestSignatureInvalid)
    }
}

#[derive(Debug, Serialize)]
struct SignedCapabilityBody<'a> {
    capability_id: &'a str,
    provides: &'a [String],
    requires: &'a [String],
    risk_template: &'a str,
}

impl<'a> From<&'a AdapterCapability> for SignedCapabilityBody<'a> {
    fn from(capability: &'a AdapterCapability) -> Self {
        Self {
            capability_id: &capability.capability_id,
            provides: &capability.provides,
            requires: &capability.requires,
            risk_template: &capability.risk_template,
        }
    }
}

fn canonical_capability_bytes(capability: &AdapterCapability) -> Result<Vec<u8>, SgrError> {
    let body = SignedCapabilityBody::from(capability);
    serde_json::to_vec(&body)
        .map_err(|err| SgrError::Internal(format!("adapter capability serialise: {err}")))
}

fn declaration_signing_key_id(declaration: &AdapterDeclaration) -> Result<&str, SgrError> {
    match declaration {
        AdapterDeclaration::Manifest(manifest) => Ok(&manifest.signing_key_id),
        AdapterDeclaration::Capability(_) => Err(SgrError::ManifestUnknownAuthority(
            "adapter declaration missing signing_key_id".to_owned(),
        )),
    }
}

fn manifest_required_capabilities(manifest: &UnitManifest) -> Result<Vec<String>, SgrError> {
    let Some(value) = manifest.adapter_target.get("requires") else {
        return Ok(Vec::new());
    };

    let Some(values) = value.as_array() else {
        return Err(SgrError::Internal(format!(
            "unit manifest requires must be a list: {}",
            manifest.unit_id
        )));
    };

    values
        .iter()
        .map(|value| {
            value.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                SgrError::Internal(format!(
                    "unit manifest requires entries must be strings: {}",
                    manifest.unit_id
                ))
            })
        })
        .collect()
}
