#![allow(
    missing_docs,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::significant_drop_tightening,
    clippy::unused_async,
    clippy::too_many_lines,
    clippy::module_name_repetitions,
    reason = "S8.5 FSM driver — pedantic lints deferred to T-174"
)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::error::HardwareError;
use crate::firmware::{
    FirmwareApplyStrategy, FirmwareScope, FirmwareTrustResult, FirmwareUpdateClass,
    FirmwareUpdateState,
};
use crate::ids::{DeviceId, FirmwareBlobId};

/// A firmware blob submitted for trust verification and update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareBlob {
    pub blob_id: FirmwareBlobId,
    pub update_class: FirmwareUpdateClass,
    pub scope: FirmwareScope,
    pub target_device: Option<DeviceId>,
    pub vendor_name: String,
    pub version: String,
    pub blake3_hash: String,
    pub signature: Vec<u8>,
    pub signer_fingerprint: String,
    pub published_at: DateTime<Utc>,
}

/// A single recorded transition in the firmware update lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareStageEntry {
    pub state: FirmwareUpdateState,
    pub transitioned_at: DateTime<Utc>,
    pub note: String,
}

/// The orchestrated firmware update plan, tracking the blob through the FSM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareUpdatePlan {
    pub blob: FirmwareBlob,
    pub current_state: FirmwareUpdateState,
    pub apply_strategy: FirmwareApplyStrategy,
    pub trust_result: Option<FirmwareTrustResult>,
    pub history: Vec<FirmwareStageEntry>,
    pub installed_version_before: Option<String>,
}

/// Closed vocabulary for firmware signing provenance paths (S8.5 §4.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FirmwareSigningPath {
    /// Full chain: blob signed by an AIOS publisher key in the trust registry.
    AiosPublisher,
    /// Signed by a vendor authority bridged through the AIOS vendor bridge.
    VendorThroughAiosBridge {
        vendor_authority: String,
        bridge_fingerprint: String,
    },
    /// Operator-local-signed with FOREVER evidence marker (T-173 wires it).
    OperatorLocalSigned { operator: String },
    /// Always refused — constitutional refusal.
    Unsigned,
}

/// The 7-stage FSM driver for firmware update trust (S8.5).
///
/// Drives blobs through:
///   Proposed → Verified → Approved → Staged → Applying → Applied
///
/// Failure and revert paths allow transitions from any state.
pub struct FirmwareUpdateOrchestrator {
    plans: RwLock<HashMap<FirmwareBlobId, FirmwareUpdatePlan>>,
    aios_publisher_keys: HashMap<String, VerifyingKey>,
    vendor_bridge_keys: HashMap<String, VerifyingKey>,
    operator_local_keys: HashMap<String, VerifyingKey>,
    installed_versions: RwLock<HashMap<DeviceId, String>>,
    recovery_mode_active: AtomicBool,
}

impl FirmwareUpdateOrchestrator {
    pub fn new() -> Self {
        Self {
            plans: RwLock::new(HashMap::new()),
            aios_publisher_keys: HashMap::new(),
            vendor_bridge_keys: HashMap::new(),
            operator_local_keys: HashMap::new(),
            installed_versions: RwLock::new(HashMap::new()),
            recovery_mode_active: AtomicBool::new(false),
        }
    }

    pub fn register_aios_publisher_key(&mut self, fingerprint: &str, key: VerifyingKey) {
        self.aios_publisher_keys
            .insert(fingerprint.to_string(), key);
    }

    pub fn register_vendor_bridge_key(&mut self, fingerprint: &str, key: VerifyingKey) {
        self.vendor_bridge_keys.insert(fingerprint.to_string(), key);
    }

    pub fn register_operator_local_key(&mut self, fingerprint: &str, key: VerifyingKey) {
        self.operator_local_keys
            .insert(fingerprint.to_string(), key);
    }

    pub async fn set_recovery_mode(&self, active: bool) {
        self.recovery_mode_active.store(active, Ordering::SeqCst);
    }

    pub async fn set_installed_version(&self, device: DeviceId, version: String) {
        self.installed_versions
            .write()
            .await
            .insert(device, version);
    }

    /// Propose a firmware blob for the update FSM. State = Proposed.
    pub async fn propose(
        &self,
        blob: FirmwareBlob,
        apply_strategy: FirmwareApplyStrategy,
    ) -> Result<FirmwareUpdatePlan, HardwareError> {
        let mut plans = self.plans.write().await;
        if plans.contains_key(&blob.blob_id) {
            return Err(HardwareError::Internal(
                "duplicate firmware blob id".to_string(),
            ));
        }
        let plan = FirmwareUpdatePlan {
            blob: blob.clone(),
            current_state: FirmwareUpdateState::Proposed,
            apply_strategy,
            trust_result: None,
            history: vec![FirmwareStageEntry {
                state: FirmwareUpdateState::Proposed,
                transitioned_at: Utc::now(),
                note: "blob proposed".to_string(),
            }],
            installed_version_before: None,
        };
        plans.insert(blob.blob_id.clone(), plan.clone());
        Ok(plan)
    }

    /// Verify the blob's signature and version. Advances Proposed → Verified on success.
    pub async fn verify(
        &self,
        blob_id: &FirmwareBlobId,
    ) -> Result<FirmwareTrustResult, HardwareError> {
        let mut plans = self.plans.write().await;
        let plan = plans
            .get_mut(blob_id)
            .ok_or_else(|| HardwareError::Internal(format!("blob not found: {blob_id:?}")))?;

        if plan.current_state != FirmwareUpdateState::Proposed {
            return Err(HardwareError::Internal(format!(
                "invalid firmware transition {:?} -> Verified",
                plan.current_state
            )));
        }

        let msg = build_signing_message(&plan.blob);

        let trust_result = if plan.blob.signature.is_empty() {
            FirmwareTrustResult::UnsignedRefused
        } else if let Some(vk) = self.aios_publisher_keys.get(&plan.blob.signer_fingerprint) {
            let sig = decode_sig(&plan.blob.signature)?;
            if vk.verify_strict(&msg, &sig).is_ok() {
                FirmwareTrustResult::AiosPublisherSigned
            } else {
                FirmwareTrustResult::RevokedKey
            }
        } else if let Some(vk) = self.vendor_bridge_keys.get(&plan.blob.signer_fingerprint) {
            let sig = decode_sig(&plan.blob.signature)?;
            if vk.verify_strict(&msg, &sig).is_ok() {
                FirmwareTrustResult::VendorSignedThroughAiosBridge
            } else {
                FirmwareTrustResult::RevokedKey
            }
        } else if let Some(vk) = self.operator_local_keys.get(&plan.blob.signer_fingerprint) {
            let sig = decode_sig(&plan.blob.signature)?;
            if vk.verify_strict(&msg, &sig).is_ok() {
                FirmwareTrustResult::OperatorLocalSigned
            } else {
                FirmwareTrustResult::RevokedKey
            }
        } else {
            FirmwareTrustResult::RevokedKey
        };

        match trust_result {
            FirmwareTrustResult::UnsignedRefused => {
                plan.trust_result = Some(FirmwareTrustResult::UnsignedRefused);
                plan.current_state = FirmwareUpdateState::Failed;
                plan.history.push(FirmwareStageEntry {
                    state: FirmwareUpdateState::Failed,
                    transitioned_at: Utc::now(),
                    note: "unsigned blob — constitutional refusal".to_string(),
                });
                Err(HardwareError::FirmwareUnsigned(blob_id.clone()))
            }
            FirmwareTrustResult::RevokedKey => {
                plan.trust_result = Some(FirmwareTrustResult::RevokedKey);
                plan.current_state = FirmwareUpdateState::Failed;
                let reason = if plan.blob.signature.is_empty() {
                    "unsigned blob"
                } else if !self
                    .aios_publisher_keys
                    .contains_key(&plan.blob.signer_fingerprint)
                    && !self
                        .vendor_bridge_keys
                        .contains_key(&plan.blob.signer_fingerprint)
                    && !self
                        .operator_local_keys
                        .contains_key(&plan.blob.signer_fingerprint)
                {
                    "unknown or revoked signer"
                } else {
                    "ed25519 verify failed"
                };
                plan.history.push(FirmwareStageEntry {
                    state: FirmwareUpdateState::Failed,
                    transitioned_at: Utc::now(),
                    note: format!("signature verification failed: {reason}"),
                });
                Err(HardwareError::FirmwareSignatureInvalid {
                    blob: blob_id.clone(),
                    reason: reason.to_string(),
                })
            }
            _ => {
                // Check version regression before declaring success.
                if let Some(ref target) = plan.blob.target_device {
                    let installed = self.installed_versions.read().await;
                    if let Some(installed_ver) = installed.get(target) {
                        if plan.blob.version <= *installed_ver {
                            plan.trust_result = Some(FirmwareTrustResult::VersionRegression);
                            plan.current_state = FirmwareUpdateState::Failed;
                            plan.history.push(FirmwareStageEntry {
                                state: FirmwareUpdateState::Failed,
                                transitioned_at: Utc::now(),
                                note: format!(
                                    "version regression: attempted {} <= installed {}",
                                    plan.blob.version, installed_ver
                                ),
                            });
                            return Err(HardwareError::FirmwareVersionRegression {
                                blob: blob_id.clone(),
                                attempted: plan.blob.version.clone(),
                                installed: installed_ver.clone(),
                            });
                        }
                    }
                }

                // Recovery-mode override: allow operator-local-signed when publisher
                // registry is empty, even if normal verification would have failed.
                if self.recovery_mode_active.load(Ordering::SeqCst)
                    && self.aios_publisher_keys.is_empty()
                    && trust_result == FirmwareTrustResult::OperatorLocalSigned
                {
                    // pass — allowed under recovery override
                }

                plan.trust_result = Some(trust_result);
                plan.current_state = FirmwareUpdateState::Verified;
                plan.history.push(FirmwareStageEntry {
                    state: FirmwareUpdateState::Verified,
                    transitioned_at: Utc::now(),
                    note: format!("verified as {trust_result:?}"),
                });
                Ok(trust_result)
            }
        }
    }

    /// Approve a verified plan. Advances Verified → Approved.
    pub async fn approve(&self, blob_id: &FirmwareBlobId) -> Result<(), HardwareError> {
        let mut plans = self.plans.write().await;
        let plan = plans
            .get_mut(blob_id)
            .ok_or_else(|| HardwareError::Internal(format!("blob not found: {blob_id:?}")))?;

        if plan.current_state != FirmwareUpdateState::Verified {
            return Err(HardwareError::Internal(format!(
                "invalid firmware transition {:?} -> Approved",
                plan.current_state
            )));
        }

        if plan.trust_result == Some(FirmwareTrustResult::ConstitutionalRefusal) {
            return Err(HardwareError::FirmwareRefusedConstitutional {
                blob: blob_id.clone(),
                reason: "constitutional refusal blocks approval".to_string(),
            });
        }

        plan.current_state = FirmwareUpdateState::Approved;
        plan.history.push(FirmwareStageEntry {
            state: FirmwareUpdateState::Approved,
            transitioned_at: Utc::now(),
            note: "approved by operator".to_string(),
        });
        Ok(())
    }

    /// Stage an approved plan. Advances Approved → Staged.
    pub async fn stage(&self, blob_id: &FirmwareBlobId) -> Result<(), HardwareError> {
        let mut plans = self.plans.write().await;
        let plan = plans
            .get_mut(blob_id)
            .ok_or_else(|| HardwareError::Internal(format!("blob not found: {blob_id:?}")))?;

        if plan.current_state != FirmwareUpdateState::Approved {
            return Err(HardwareError::Internal(format!(
                "invalid firmware transition {:?} -> Staged",
                plan.current_state
            )));
        }

        plan.current_state = FirmwareUpdateState::Staged;
        plan.history.push(FirmwareStageEntry {
            state: FirmwareUpdateState::Staged,
            transitioned_at: Utc::now(),
            note: "staged for apply".to_string(),
        });
        Ok(())
    }

    /// Apply a staged plan. Behaviour depends on `apply_strategy`.
    pub async fn apply(&self, blob_id: &FirmwareBlobId) -> Result<(), HardwareError> {
        let mut plans = self.plans.write().await;
        let plan = plans
            .get_mut(blob_id)
            .ok_or_else(|| HardwareError::Internal(format!("blob not found: {blob_id:?}")))?;

        if plan.current_state != FirmwareUpdateState::Staged {
            return Err(HardwareError::Internal(format!(
                "invalid firmware transition {:?} -> Applying",
                plan.current_state
            )));
        }

        match plan.apply_strategy {
            FirmwareApplyStrategy::Deferred => Err(HardwareError::Internal(
                "deferred strategy: apply_at_next_boot or trigger recovery window".to_string(),
            )),
            FirmwareApplyStrategy::Atomic => {
                // Staged → Applying
                plan.current_state = FirmwareUpdateState::Applying;
                plan.history.push(FirmwareStageEntry {
                    state: FirmwareUpdateState::Applying,
                    transitioned_at: Utc::now(),
                    note: "atomic apply started".to_string(),
                });
                // Applying → Applied
                plan.current_state = FirmwareUpdateState::Applied;
                plan.history.push(FirmwareStageEntry {
                    state: FirmwareUpdateState::Applied,
                    transitioned_at: Utc::now(),
                    note: "atomic apply completed".to_string(),
                });
                // Update installed version
                if let Some(ref target) = plan.blob.target_device {
                    self.installed_versions
                        .write()
                        .await
                        .insert(target.clone(), plan.blob.version.clone());
                }
                Ok(())
            }
            FirmwareApplyStrategy::Staged => {
                // Staged → Applying only
                plan.current_state = FirmwareUpdateState::Applying;
                plan.history.push(FirmwareStageEntry {
                    state: FirmwareUpdateState::Applying,
                    transitioned_at: Utc::now(),
                    note: "staged apply started — requires finalize".to_string(),
                });
                Ok(())
            }
        }
    }

    /// Finalize a staged apply. Advances Applying → Applied.
    pub async fn finalize_staged_apply(
        &self,
        blob_id: &FirmwareBlobId,
    ) -> Result<(), HardwareError> {
        let mut plans = self.plans.write().await;
        let plan = plans
            .get_mut(blob_id)
            .ok_or_else(|| HardwareError::Internal(format!("blob not found: {blob_id:?}")))?;

        if plan.current_state != FirmwareUpdateState::Applying {
            return Err(HardwareError::Internal(format!(
                "invalid firmware transition {:?} -> Applied",
                plan.current_state
            )));
        }

        plan.current_state = FirmwareUpdateState::Applied;
        plan.history.push(FirmwareStageEntry {
            state: FirmwareUpdateState::Applied,
            transitioned_at: Utc::now(),
            note: "staged apply finalized".to_string(),
        });

        if let Some(ref target) = plan.blob.target_device {
            self.installed_versions
                .write()
                .await
                .insert(target.clone(), plan.blob.version.clone());
        }
        Ok(())
    }

    /// Revert from any state to Reverted. Records reason.
    pub async fn revert(
        &self,
        blob_id: &FirmwareBlobId,
        reason: &str,
    ) -> Result<(), HardwareError> {
        let mut plans = self.plans.write().await;
        let plan = plans
            .get_mut(blob_id)
            .ok_or_else(|| HardwareError::Internal(format!("blob not found: {blob_id:?}")))?;

        plan.current_state = FirmwareUpdateState::Reverted;
        plan.history.push(FirmwareStageEntry {
            state: FirmwareUpdateState::Reverted,
            transitioned_at: Utc::now(),
            note: reason.to_string(),
        });
        Ok(())
    }

    /// Transition from any state to Failed. Records reason.
    pub async fn fail(&self, blob_id: &FirmwareBlobId, reason: &str) -> Result<(), HardwareError> {
        let mut plans = self.plans.write().await;
        let plan = plans
            .get_mut(blob_id)
            .ok_or_else(|| HardwareError::Internal(format!("blob not found: {blob_id:?}")))?;

        plan.current_state = FirmwareUpdateState::Failed;
        plan.history.push(FirmwareStageEntry {
            state: FirmwareUpdateState::Failed,
            transitioned_at: Utc::now(),
            note: reason.to_string(),
        });
        Ok(())
    }

    /// Look up a plan by blob id.
    pub async fn get_plan(&self, blob_id: &FirmwareBlobId) -> Option<FirmwareUpdatePlan> {
        self.plans.read().await.get(blob_id).cloned()
    }

    /// List all plans currently tracked by the orchestrator.
    pub async fn list_plans(&self) -> Vec<FirmwareUpdatePlan> {
        self.plans.read().await.values().cloned().collect()
    }
}

impl Default for FirmwareUpdateOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

fn build_signing_message(blob: &FirmwareBlob) -> Vec<u8> {
    let mut msg = Vec::new();
    msg.extend_from_slice(blob.blob_id.0.as_bytes());
    msg.extend_from_slice(blob.update_class.label().as_bytes());
    msg.extend_from_slice(blob.scope.label().as_bytes());
    msg.extend_from_slice(blob.version.as_bytes());
    msg.extend_from_slice(blob.blake3_hash.as_bytes());
    msg
}

fn decode_sig(bytes: &[u8]) -> Result<Signature, HardwareError> {
    let arr: [u8; 64] = bytes
        .try_into()
        .map_err(|_| HardwareError::Internal("signature must be 64 bytes".to_string()))?;
    Ok(Signature::from_bytes(&arr))
}
