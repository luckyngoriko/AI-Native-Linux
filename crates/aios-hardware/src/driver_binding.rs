#![allow(missing_docs)]

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use tokio::sync::RwLock;

use crate::device_record::HardwareDeviceRecord;
use crate::driver::DriverProvenance;
use crate::error::HardwareError;
use crate::evidence::{HardwareEvidenceEmitter, WithEmitter};
use crate::ids::{DeviceId, DriverBindingId};
use crate::trust_class::DeviceTrustClass;

/// A signed driver binding associating a kernel module with a device.
///
/// Canonical bytes for Ed25519 verification: `(binding_id || device_id ||
/// driver_module_name || kernel_module_version || provenance.label() ||
/// blake3_hash)` with `\n` separators.
#[derive(Debug, Clone)]
pub struct DriverBinding {
    pub binding_id: DriverBindingId,
    pub device_id: DeviceId,
    pub driver_module_name: String,
    pub kernel_module_version: String,
    pub provenance: DriverProvenance,
    pub blake3_hash: String,
    pub signer_fingerprint: String,
    pub signature: Vec<u8>,
    pub admitted_at: DateTime<Utc>,
}

impl DriverBinding {
    /// Deterministic byte sequence signed with Ed25519.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(self.binding_id.0.as_bytes());
        bytes.push(b'\n');
        bytes.extend_from_slice(self.device_id.0.as_bytes());
        bytes.push(b'\n');
        bytes.extend_from_slice(self.driver_module_name.as_bytes());
        bytes.push(b'\n');
        bytes.extend_from_slice(self.kernel_module_version.as_bytes());
        bytes.push(b'\n');
        bytes.extend_from_slice(self.provenance.label().as_bytes());
        bytes.push(b'\n');
        bytes.extend_from_slice(self.blake3_hash.as_bytes());
        bytes
    }
}

/// Entry in the driver module blacklist.
#[derive(Debug, Clone)]
pub struct DriverBlacklistEntry {
    pub module_name: String,
    pub reason: String,
    pub effective_at: DateTime<Utc>,
}

/// Signed driver binding registry enforcing the S8.3 §3.5 provenance taxonomy.
///
/// Provenance priority (highest first): `AiosVerified > SignedKernelModule >
/// DistroProvided > OperatorLocalSigned`. `OutOfTreeBlacklisted` is a denial
/// marker — never admissible.
pub struct DriverBindingRegistry {
    bindings: RwLock<HashMap<DriverBindingId, DriverBinding>>,
    by_device: RwLock<HashMap<DeviceId, DriverBindingId>>,
    trusted_authorities: HashMap<String, VerifyingKey>,
    provenance_priority: Vec<DriverProvenance>,
    blacklist: RwLock<HashMap<String, DriverBlacklistEntry>>,
    emitter: Option<Arc<dyn HardwareEvidenceEmitter>>,
}

impl DriverBindingRegistry {
    /// Creates a new registry with the default provenance priority order.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bindings: RwLock::new(HashMap::new()),
            by_device: RwLock::new(HashMap::new()),
            trusted_authorities: HashMap::new(),
            provenance_priority: vec![
                DriverProvenance::AiosVerified,
                DriverProvenance::SignedKernelModule,
                DriverProvenance::DistroProvided,
                DriverProvenance::OperatorLocalSigned,
            ],
            blacklist: RwLock::new(HashMap::new()),
            emitter: None,
        }
    }

    /// Registers a trusted signing authority keyed by fingerprint.
    pub fn register_authority(&mut self, fingerprint: &str, key: VerifyingKey) {
        self.trusted_authorities.insert(fingerprint.to_owned(), key);
    }

    /// Admits a signed driver binding after verifying its Ed25519 signature,
    /// checking provenance admissibility, and enforcing priority ordering.
    ///
    /// # Errors
    ///
    /// Returns `DriverBindingFailed` if the provenance is blacklisted, the
    /// module is on the deny-list, the authority is unknown, the signature
    /// is invalid, or a higher-priority binding already exists for the device.
    #[allow(clippy::too_many_lines)]
    pub async fn admit_binding(&self, binding: DriverBinding) -> Result<(), HardwareError> {
        // Reject OutOfTreeBlacklisted provenance unconditionally.
        if binding.provenance == DriverProvenance::OutOfTreeBlacklisted {
            let reason = "blacklisted provenance cannot be admitted";
            if let Some(ref e) = self.emitter {
                if let Err(emit_err) = e
                    .emit_driver_binding_rejected(
                        &binding.device_id,
                        reason,
                        Some(binding.provenance),
                    )
                    .await
                {
                    tracing::warn!(%emit_err, "Failed to emit driver_binding_rejected evidence");
                }
            }
            return Err(HardwareError::DriverBindingFailed {
                device: binding.device_id,
                reason: reason.into(),
            });
        }

        // Reject if module is on the deny-list.
        {
            let blacklist = self.blacklist.read().await;
            if blacklist.contains_key(&binding.driver_module_name) {
                let reason = "module on blacklist";
                if let Some(ref e) = self.emitter {
                    if let Err(emit_err) = e
                        .emit_driver_binding_rejected(
                            &binding.device_id,
                            reason,
                            Some(binding.provenance),
                        )
                        .await
                    {
                        tracing::warn!(%emit_err, "Failed to emit driver_binding_rejected evidence");
                    }
                }
                return Err(HardwareError::DriverBindingFailed {
                    device: binding.device_id,
                    reason: reason.into(),
                });
            }
        }

        // Look up the verifying key by signer fingerprint.
        let vk = self
            .trusted_authorities
            .get(&binding.signer_fingerprint)
            .ok_or_else(|| HardwareError::DriverBindingFailed {
                device: binding.device_id.clone(),
                reason: "unknown authority".into(),
            })?;

        // Verify Ed25519 signature over canonical bytes.
        let sig = Signature::from_slice(&binding.signature).map_err(|_| {
            HardwareError::DriverBindingFailed {
                device: binding.device_id.clone(),
                reason: "ed25519 verify failed".into(),
            }
        })?;

        let canonical = binding.canonical_bytes();
        if vk.verify_strict(&canonical, &sig).is_err() {
            let reason = "ed25519 verify failed";
            if let Some(ref e) = self.emitter {
                if let Err(emit_err) = e
                    .emit_driver_binding_rejected(
                        &binding.device_id,
                        reason,
                        Some(binding.provenance),
                    )
                    .await
                {
                    tracing::warn!(%emit_err, "Failed to emit driver_binding_rejected evidence");
                }
            }
            return Err(HardwareError::DriverBindingFailed {
                device: binding.device_id,
                reason: reason.into(),
            });
        }

        let new_priority = self.priority_of(binding.provenance);

        // Atomically check priority and insert.
        {
            let mut by_device = self.by_device.write().await;
            let mut bindings = self.bindings.write().await;

            if let Some(existing_id) = by_device.get(&binding.device_id) {
                if let Some(existing) = bindings.get(existing_id) {
                    let existing_priority = self.priority_of(existing.provenance);
                    if new_priority > existing_priority {
                        let reason = "lower-priority provenance cannot supersede";
                        if let Some(ref e) = self.emitter {
                            if let Err(emit_err) = e
                                .emit_driver_binding_rejected(
                                    &binding.device_id,
                                    reason,
                                    Some(binding.provenance),
                                )
                                .await
                            {
                                tracing::warn!(
                                    %emit_err,
                                    "Failed to emit driver_binding_rejected evidence"
                                );
                            }
                        }
                        return Err(HardwareError::DriverBindingFailed {
                            device: binding.device_id,
                            reason: reason.into(),
                        });
                    }
                }
            }

            let binding_id = binding.binding_id.clone();
            let device_id = binding.device_id.clone();
            bindings.insert(binding_id.clone(), binding.clone());
            drop(bindings);
            by_device.insert(device_id, binding_id);
            drop(by_device);

            if let Some(ref e) = self.emitter {
                if let Err(emit_err) = e.emit_driver_binding_admitted(&binding).await {
                    tracing::warn!(%emit_err, "Failed to emit driver_binding_admitted evidence");
                }
            }
        }

        Ok(())
    }

    /// Looks up the binding for a device.
    pub async fn lookup_binding(&self, device_id: &DeviceId) -> Option<DriverBinding> {
        let by_device = self.by_device.read().await;
        let binding_id = by_device.get(device_id).cloned()?;
        drop(by_device);
        let bindings = self.bindings.read().await;
        bindings.get(&binding_id).cloned()
    }

    /// Returns all admitted bindings.
    pub async fn list_bindings(&self) -> Vec<DriverBinding> {
        let bindings = self.bindings.read().await;
        bindings.values().cloned().collect()
    }

    /// Revokes a binding by id, removing it from both indices.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if the binding id is unknown.
    pub async fn revoke_binding(
        &self,
        binding_id: &DriverBindingId,
        reason: &str,
    ) -> Result<(), HardwareError> {
        let mut bindings = self.bindings.write().await;
        let binding = bindings.remove(binding_id).ok_or_else(|| {
            HardwareError::Internal(format!("revoke: unknown binding {binding_id:?}: {reason}"))
        })?;
        drop(bindings);
        self.by_device.write().await.remove(&binding.device_id);
        Ok(())
    }

    /// Adds a module name to the deny-list.
    ///
    /// # Errors
    ///
    /// Currently infallible (returns `Ok(())` unconditionally).
    pub async fn add_to_blacklist(
        &self,
        module_name: &str,
        reason: &str,
    ) -> Result<(), HardwareError> {
        let entry = DriverBlacklistEntry {
            module_name: module_name.to_owned(),
            reason: reason.to_owned(),
            effective_at: Utc::now(),
        };
        self.blacklist
            .write()
            .await
            .insert(module_name.to_owned(), entry);
        Ok(())
    }

    /// Returns true if the module name is on the deny-list.
    pub async fn is_blacklisted(&self, module_name: &str) -> bool {
        let blacklist = self.blacklist.read().await;
        blacklist.contains_key(module_name)
    }

    /// Returns the priority index of a provenance.
    ///
    /// Lower index = higher priority. `OutOfTreeBlacklisted` returns
    /// `usize::MAX` — it is never admissible.
    #[must_use]
    pub fn priority_of(&self, p: DriverProvenance) -> usize {
        if p == DriverProvenance::OutOfTreeBlacklisted {
            return usize::MAX;
        }
        self.provenance_priority
            .iter()
            .position(|&pp| pp == p)
            .map_or(usize::MAX, |idx| idx)
    }

    /// Upgrades a `HardwareDeviceRecord`'s `driver_provenance` and
    /// `trust_class` based on the admitted binding for the device.
    ///
    /// Mapping: `AiosVerified→RootSigned`, `SignedKernelModule→VendorSigned`,
    /// `DistroProvided→CommunitySigned`, `OperatorLocalSigned→OperatorLocal`.
    /// Devices with no binding are left `Untrusted`.
    ///
    /// # Errors
    ///
    /// Currently infallible (returns `Ok(())` unconditionally).
    pub async fn upgrade_record_trust(
        &self,
        record: &mut HardwareDeviceRecord,
    ) -> Result<(), HardwareError> {
        if let Some(binding) = self.lookup_binding(&record.device_id).await {
            record.driver_provenance = Some(binding.provenance);
            record.trust_class = match binding.provenance {
                DriverProvenance::AiosVerified => DeviceTrustClass::RootSigned,
                DriverProvenance::SignedKernelModule => DeviceTrustClass::VendorSigned,
                DriverProvenance::DistroProvided => DeviceTrustClass::CommunitySigned,
                DriverProvenance::OperatorLocalSigned => DeviceTrustClass::OperatorLocal,
                DriverProvenance::OutOfTreeBlacklisted => DeviceTrustClass::Untrusted,
            };
        } else {
            record.driver_provenance = None;
            record.trust_class = DeviceTrustClass::Untrusted;
        }
        Ok(())
    }
}

impl WithEmitter for DriverBindingRegistry {
    fn with_emitter(mut self, emitter: Option<Arc<dyn HardwareEvidenceEmitter>>) -> Self {
        self.emitter = emitter;
        self
    }
}

impl Default for DriverBindingRegistry {
    fn default() -> Self {
        Self::new()
    }
}
