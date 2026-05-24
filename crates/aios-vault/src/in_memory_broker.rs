//! In-memory [`VaultBroker`](crate::VaultBroker) harness for T-047.
//!
//! The harness stores `(VaultCapability, KeyMaterial)` tuples privately and
//! returns only public capability records or operation outputs. It performs no
//! real cryptography in T-047; T-049 replaces the deterministic placeholders.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S5.2 vault broker vocabulary"
)]
#![allow(
    clippy::significant_drop_tightening,
    reason = "the write guard is held through capability state validation and mutation so use/revoke decisions stay atomic"
)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;

use crate::broker::{
    operation_matches_class, IssueCapabilityRequest, UseCapabilityRequest, UseCapabilityResult,
    VaultBroker, VaultOperation,
};
use crate::capability::{CapabilityId, CapabilityState, KeyMaterialHandle, VaultCapability};
use crate::error::VaultError;
use crate::identity::SubjectRef;
use crate::key_material::KeyMaterial;

const SIMULATED_BYTES: &[u8] = b"operation_simulated";

/// HashMap-backed in-process Vault Broker used by tests and successor slices.
#[derive(Debug, Clone, Default)]
pub struct InMemoryVaultBroker {
    capabilities: Arc<RwLock<HashMap<CapabilityId, (VaultCapability, KeyMaterial)>>>,
}

impl InMemoryVaultBroker {
    /// Construct an empty in-memory broker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl VaultBroker for InMemoryVaultBroker {
    async fn issue_capability(
        &self,
        request: IssueCapabilityRequest,
    ) -> Result<VaultCapability, VaultError> {
        let IssueCapabilityRequest {
            class,
            issued_to,
            expires_at,
            key_algorithm,
            key_material_bytes,
        } = request;
        let now = Utc::now();
        let capability_id = CapabilityId::new();
        let key_material_handle =
            KeyMaterialHandle(format!("vault-internal:{}", capability_id.as_str()));
        // T-047 accepts the closed T-046 class enum through this generic
        // harness. First-boot-only `BOOTSTRAP_KEY_SIGN` admission and
        // class/material-kind checks land with the dedicated successor paths.
        // T-049 lands real KeyGen. T-047 stores caller-supplied bytes or a
        // zero-byte placeholder when generation is requested.
        let key_material = KeyMaterial {
            algorithm: key_algorithm,
            created_at: now,
            bytes: key_material_bytes.unwrap_or_default(),
        };
        let capability = VaultCapability {
            capability_id: capability_id.clone(),
            class,
            issued_to,
            issued_at: now,
            expires_at,
            state: CapabilityState::Active,
            key_material_handle,
        };

        self.capabilities
            .write()
            .await
            .insert(capability_id, (capability.clone(), key_material));

        Ok(capability)
    }

    async fn use_capability(
        &self,
        request: UseCapabilityRequest,
    ) -> Result<UseCapabilityResult, VaultError> {
        let UseCapabilityRequest {
            capability_id,
            operation,
        } = request;
        let key_material_handle = {
            let mut store = self.capabilities.write().await;
            let (capability, _key_material) = store
                .get_mut(&capability_id)
                .ok_or_else(|| VaultError::CapabilityNotFound(capability_id.clone()))?;

            match capability.state {
                CapabilityState::Active => {}
                CapabilityState::Expired => {
                    return Err(VaultError::CapabilityExpired(capability_id));
                }
                CapabilityState::Revoked => {
                    return Err(VaultError::CapabilityRevoked(capability_id));
                }
                state => {
                    return Err(VaultError::InvalidTransition {
                        from: state,
                        to: CapabilityState::Active,
                    });
                }
            }

            if capability
                .expires_at
                .is_some_and(|expires_at| expires_at <= Utc::now())
            {
                capability.state = CapabilityState::Expired;
                return Err(VaultError::CapabilityExpired(capability_id));
            }

            if !operation_matches_class(capability.class, &operation) {
                return Err(VaultError::OperationClassMismatch {
                    capability_class: capability.class,
                    operation_kind: operation.operation_kind().to_owned(),
                });
            }

            capability.key_material_handle.clone()
        };

        simulate_operation(&key_material_handle, operation)
    }

    async fn list_capabilities(
        &self,
        subject: &SubjectRef,
    ) -> Result<Vec<VaultCapability>, VaultError> {
        let store = self.capabilities.read().await;
        Ok(store
            .values()
            .filter(|(capability, _key_material)| capability.issued_to == *subject)
            .map(|(capability, _key_material)| capability.clone())
            .collect())
    }

    async fn revoke_capability(
        &self,
        capability_id: &CapabilityId,
        _revoked_by: &SubjectRef,
    ) -> Result<(), VaultError> {
        {
            let mut store = self.capabilities.write().await;
            let (capability, _key_material) = store
                .get_mut(capability_id)
                .ok_or_else(|| VaultError::CapabilityNotFound(capability_id.clone()))?;

            if capability.state != CapabilityState::Active {
                return Err(VaultError::InvalidTransition {
                    from: capability.state,
                    to: CapabilityState::Revoked,
                });
            }

            capability.state = CapabilityState::Revoked;
        }

        Ok(())
    }
}

fn simulate_operation(
    key_material_handle: &KeyMaterialHandle,
    operation: VaultOperation,
) -> Result<UseCapabilityResult, VaultError> {
    let simulated = SIMULATED_BYTES.to_vec();

    match operation {
        VaultOperation::Encrypt { aad, .. } => Ok(UseCapabilityResult::Encrypted {
            ciphertext: simulated.clone(),
            nonce: simulated,
            aad,
        }),
        VaultOperation::Decrypt { .. } => Ok(UseCapabilityResult::Decrypted {
            plaintext: simulated,
        }),
        VaultOperation::MacGenerate { .. } => {
            Ok(UseCapabilityResult::MacGenerated { tag: simulated })
        }
        VaultOperation::MacVerify { .. } => Ok(UseCapabilityResult::MacVerified { valid: true }),
        VaultOperation::KdfDerive { .. } => Ok(UseCapabilityResult::KdfDerived {
            derived_key_handle: KeyMaterialHandle(format!("{}:derived", key_material_handle.0)),
        }),
        VaultOperation::Sign { .. } => Ok(UseCapabilityResult::Signed {
            signature: simulated,
        }),
        VaultOperation::Verify { .. } => Ok(UseCapabilityResult::Verified { valid: true }),
        VaultOperation::RandomGenerate { .. } => Ok(UseCapabilityResult::RandomGenerated {
            random_bytes: simulated,
        }),
        VaultOperation::SecretGet { .. } => Err(VaultError::OperationUnsupportedInT047(operation)),
    }
}
