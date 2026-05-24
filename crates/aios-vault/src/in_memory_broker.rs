//! In-memory [`VaultBroker`](crate::VaultBroker) harness for T-047.
//!
//! The harness stores `(VaultCapability, KeyMaterial)` tuples privately and
//! returns only public capability records or operation outputs.

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
use rand_core::{OsRng, RngCore};
use tokio::sync::RwLock;
use x25519_dalek::StaticSecret;

use crate::audit::CapabilityAuditLog;
use crate::broker::{
    operation_matches_class, IssueCapabilityRequest, UseCapabilityRequest, UseCapabilityResult,
    VaultBroker, VaultOperation,
};
use crate::capability::{
    CapabilityClass, CapabilityId, CapabilityState, KeyMaterialHandle, VaultCapability,
};
use crate::crypto;
use crate::error::VaultError;
use crate::evidence_emit::VaultEvidenceEmitter;
use crate::identity::SubjectRef;
use crate::key_material::{KeyAlgorithm, KeyMaterial};

/// HashMap-backed in-process Vault Broker used by tests and successor slices.
#[derive(Debug, Clone, Default)]
pub struct InMemoryVaultBroker {
    pub(crate) capabilities: Arc<RwLock<HashMap<CapabilityId, (VaultCapability, KeyMaterial)>>>,
    derived_key_materials: Arc<RwLock<HashMap<KeyMaterialHandle, KeyMaterial>>>,
    audit_log: Option<Arc<CapabilityAuditLog>>,
    evidence_emitter: Option<Arc<VaultEvidenceEmitter>>,
}

impl InMemoryVaultBroker {
    /// Construct an empty in-memory broker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach a capability lifecycle audit log.
    #[must_use]
    pub fn with_audit_log(mut self, log: Arc<CapabilityAuditLog>) -> Self {
        self.audit_log = Some(log);
        self
    }

    /// Attach a vault evidence emitter.
    #[must_use]
    pub fn with_evidence_emitter(mut self, evidence_emitter: Arc<VaultEvidenceEmitter>) -> Self {
        self.evidence_emitter = Some(evidence_emitter);
        self
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
        validate_key_algorithm_for_class(class, key_algorithm)?;

        let now = Utc::now();
        let capability_id = CapabilityId::new();
        let key_material_handle =
            KeyMaterialHandle(format!("vault-internal:{}", capability_id.as_str()));
        let key_material = KeyMaterial {
            algorithm: key_algorithm,
            created_at: now,
            bytes: key_material_bytes.unwrap_or_else(|| generate_key_material(key_algorithm)),
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

        if let Some(audit_log) = &self.audit_log {
            audit_log.record_issue(
                capability.capability_id.clone(),
                capability.issued_to.clone(),
            );
        }

        if let Some(evidence_emitter) = &self.evidence_emitter {
            evidence_emitter
                .emit_capability_issued(&capability, &capability.issued_to, None)
                .await?;
        }

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
        let operation_kind = operation.operation_kind().to_owned();
        let (capability_class, key_material) = {
            let mut store = self.capabilities.write().await;
            let (capability, key_material) = store
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
                .is_some_and(|expires_at| expires_at < Utc::now())
            {
                capability.state = CapabilityState::Expired;
                if let Some(audit_log) = &self.audit_log {
                    audit_log.record_expire(&capability_id);
                }
                return Err(VaultError::CapabilityExpired(capability_id));
            }

            if !operation_matches_class(capability.class, &operation)
                && !operation_matches_t049_extension(capability.class, &operation)
            {
                return Err(VaultError::OperationClassMismatch {
                    capability_class: capability.class,
                    operation_kind: operation.operation_kind().to_owned(),
                });
            }

            (capability.class, key_material.clone())
        };

        let result = self
            .execute_operation(capability_class, key_material, operation)
            .await?;

        if let Some(audit_log) = &self.audit_log {
            audit_log.record_use(&capability_id, operation_kind.clone());
        }

        if let Some(evidence_emitter) = &self.evidence_emitter {
            evidence_emitter
                .emit_capability_used(&capability_id, &operation_kind, None)
                .await?;
        }

        Ok(result)
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
        revoked_by: &SubjectRef,
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

        if let Some(audit_log) = &self.audit_log {
            audit_log.record_revoke(capability_id, revoked_by.clone());
        }

        if let Some(evidence_emitter) = &self.evidence_emitter {
            evidence_emitter
                .emit_capability_revoked(capability_id, revoked_by, "admin_request", None)
                .await?;
        }

        Ok(())
    }
}

impl InMemoryVaultBroker {
    async fn execute_operation(
        &self,
        _capability_class: CapabilityClass,
        key_material: KeyMaterial,
        operation: VaultOperation,
    ) -> Result<UseCapabilityResult, VaultError> {
        match operation {
            VaultOperation::Encrypt { plaintext, aad }
                if key_material.algorithm == KeyAlgorithm::Aes256Gcm =>
            {
                let encrypted = crypto::aes_gcm::encrypt(&key_material.bytes, &plaintext, &aad)?;
                Ok(UseCapabilityResult::Encrypted {
                    ciphertext: encrypted.ciphertext,
                    nonce: encrypted.nonce,
                    aad,
                })
            }
            VaultOperation::Decrypt { ciphertext, aad }
                if key_material.algorithm == KeyAlgorithm::Aes256Gcm =>
            {
                Ok(UseCapabilityResult::Decrypted {
                    plaintext: crypto::aes_gcm::decrypt(&key_material.bytes, &ciphertext, &aad)?,
                })
            }
            VaultOperation::MacGenerate { message }
                if key_material.algorithm == KeyAlgorithm::HmacSha256 =>
            {
                Ok(UseCapabilityResult::MacGenerated {
                    tag: crypto::hmac::generate(&key_material.bytes, &message)?,
                })
            }
            VaultOperation::MacVerify { message, tag }
                if key_material.algorithm == KeyAlgorithm::HmacSha256 =>
            {
                Ok(UseCapabilityResult::MacVerified {
                    valid: crypto::hmac::verify(&key_material.bytes, &message, &tag)?,
                })
            }
            VaultOperation::KdfDerive { info, length }
                if key_material.algorithm == KeyAlgorithm::HkdfSha256 =>
            {
                let derived = crypto::hkdf::derive(&key_material.bytes, &info, length)?;
                let handle = derived.handle;
                self.derived_key_materials.write().await.insert(
                    handle.clone(),
                    KeyMaterial {
                        algorithm: KeyAlgorithm::HkdfSha256,
                        created_at: Utc::now(),
                        bytes: derived.bytes,
                    },
                );
                Ok(UseCapabilityResult::KdfDerived {
                    derived_key_handle: handle,
                })
            }
            VaultOperation::KdfDerive { .. } => Err(VaultError::KeyAlgorithmMismatch {
                expected: KeyAlgorithm::HkdfSha256,
                found: key_material.algorithm,
            }),

            VaultOperation::Sign { message } if key_material.algorithm == KeyAlgorithm::Ed25519 => {
                Ok(UseCapabilityResult::Signed {
                    signature: crypto::ed25519::sign(&key_material.bytes, &message)?,
                })
            }
            VaultOperation::Verify { message, signature }
                if key_material.algorithm == KeyAlgorithm::Ed25519 =>
            {
                Ok(UseCapabilityResult::Verified {
                    valid: crypto::ed25519::verify(&key_material.bytes, &message, &signature)?,
                })
            }
            VaultOperation::RandomGenerate { byte_count } => {
                let byte_count = usize::try_from(byte_count).map_err(|_| {
                    VaultError::CryptoError(
                        "random byte count is not representable on this target".to_owned(),
                    )
                })?;
                let mut random_bytes = vec![0_u8; byte_count];
                OsRng.fill_bytes(&mut random_bytes);
                Ok(UseCapabilityResult::RandomGenerated { random_bytes })
            }

            operation => Err(VaultError::OperationUnsupportedInT049(operation)),
        }
    }
}

fn validate_key_algorithm_for_class(
    class: CapabilityClass,
    found: KeyAlgorithm,
) -> Result<(), VaultError> {
    match class {
        CapabilityClass::KeySign
        | CapabilityClass::KeyVerify
        | CapabilityClass::BootstrapKeySign => expect_algorithm(KeyAlgorithm::Ed25519, found),
        CapabilityClass::KeyEncrypt | CapabilityClass::KeyDecrypt
            if matches!(
                found,
                KeyAlgorithm::Aes256Gcm | KeyAlgorithm::HkdfSha256 | KeyAlgorithm::X25519
            ) =>
        {
            Ok(())
        }
        CapabilityClass::KeyEncrypt | CapabilityClass::KeyDecrypt => {
            expect_algorithm(KeyAlgorithm::Aes256Gcm, found)
        }
        CapabilityClass::MacGenerate | CapabilityClass::MacVerify => {
            expect_algorithm(KeyAlgorithm::HmacSha256, found)
        }
        CapabilityClass::RandomGenerate | CapabilityClass::SecretGet => Ok(()),
    }
}

fn expect_algorithm(expected: KeyAlgorithm, found: KeyAlgorithm) -> Result<(), VaultError> {
    if expected == found {
        Ok(())
    } else {
        Err(VaultError::KeyAlgorithmMismatch { expected, found })
    }
}

fn generate_key_material(algorithm: KeyAlgorithm) -> Vec<u8> {
    match algorithm {
        KeyAlgorithm::Aes256Gcm => crypto::aes_gcm::generate_key(),
        KeyAlgorithm::HmacSha256 => crypto::hmac::generate_key(),
        KeyAlgorithm::HkdfSha256 => crypto::hkdf::generate_ikm(),
        KeyAlgorithm::Ed25519 => crypto::ed25519::generate_signing_key(),
        KeyAlgorithm::X25519 => generate_x25519_static_secret(),
    }
}

fn generate_x25519_static_secret() -> Vec<u8> {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    StaticSecret::from(bytes).to_bytes().to_vec()
}

const fn operation_matches_t049_extension(
    capability_class: CapabilityClass,
    operation: &VaultOperation,
) -> bool {
    matches!(
        (capability_class, operation),
        (
            CapabilityClass::KeyEncrypt,
            VaultOperation::KdfDerive { .. }
        )
    )
}
