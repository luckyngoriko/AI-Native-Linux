//! Async Vault Broker trait and use-without-reveal DTOs (S5.2 §4-§6).
//!
//! T-047 fixes the public contract and keeps crypto intentionally simulated.
//! T-049 replaces the placeholder operation bodies with real key generation and
//! cryptographic primitives without widening this trait surface.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S5.2 vault broker vocabulary"
)]

use std::fmt;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::capability::{CapabilityClass, CapabilityId, KeyMaterialHandle, VaultCapability};
use crate::error::VaultError;
use crate::identity::SubjectRef;
use crate::key_material::KeyAlgorithm;

/// Request to issue a new vault capability and bind broker-held material to it.
#[derive(Clone, PartialEq, Eq)]
pub struct IssueCapabilityRequest {
    /// Closed S5.2 capability class to issue.
    pub class: CapabilityClass,
    /// Subject receiving the capability.
    pub issued_to: SubjectRef,
    /// Optional hard expiry for the capability.
    pub expires_at: Option<DateTime<Utc>>,
    /// Algorithm marker for the broker-held material.
    pub key_algorithm: KeyAlgorithm,
    /// Optional imported key bytes. `None` asks the broker to generate material.
    pub key_material_bytes: Option<Vec<u8>>,
}

impl fmt::Debug for IssueCapabilityRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key_material_bytes = self.key_material_bytes.as_ref().map(Vec::len);

        f.debug_struct("IssueCapabilityRequest")
            .field("class", &self.class)
            .field("issued_to", &self.issued_to)
            .field("expires_at", &self.expires_at)
            .field("key_algorithm", &self.key_algorithm)
            .field(
                "key_material_bytes",
                &RedactedKeyMaterialBytes(key_material_bytes),
            )
            .finish()
    }
}

struct RedactedKeyMaterialBytes(Option<usize>);

impl fmt::Debug for RedactedKeyMaterialBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(byte_count) => write!(f, "<{byte_count} bytes redacted>"),
            None => f.write_str("None"),
        }
    }
}

/// Request to exercise one capability without returning raw key material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseCapabilityRequest {
    /// Capability to exercise.
    pub capability_id: CapabilityId,
    /// Closed operation requested against the capability.
    pub operation: VaultOperation,
}

/// Closed T-047 operation surface for S5.2 use-without-reveal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultOperation {
    /// Encrypt bytes under a `KEY_ENCRYPT` capability.
    Encrypt {
        /// Plaintext supplied by the caller.
        plaintext: Vec<u8>,
        /// Additional authenticated data.
        aad: Vec<u8>,
    },
    /// Decrypt bytes under a `KEY_DECRYPT` capability.
    Decrypt {
        /// Ciphertext supplied by the caller.
        ciphertext: Vec<u8>,
        /// Additional authenticated data.
        aad: Vec<u8>,
    },
    /// Generate a MAC under a `MAC_GENERATE` capability.
    MacGenerate {
        /// Message to authenticate.
        message: Vec<u8>,
    },
    /// Verify a MAC under a `MAC_VERIFY` capability.
    MacVerify {
        /// Message to authenticate.
        message: Vec<u8>,
        /// Tag to verify.
        tag: Vec<u8>,
    },
    /// Derive a broker-held key handle. No T-046 capability class maps to this.
    KdfDerive {
        /// KDF context string.
        info: Vec<u8>,
        /// Requested derived key length.
        length: u32,
    },
    /// Sign bytes under a `KEY_SIGN` or `BOOTSTRAP_KEY_SIGN` capability.
    Sign {
        /// Message to sign.
        message: Vec<u8>,
    },
    /// Verify bytes under a `KEY_VERIFY` capability.
    Verify {
        /// Message that was signed.
        message: Vec<u8>,
        /// Signature to verify.
        signature: Vec<u8>,
    },
    /// Generate random bytes under a `RANDOM_GENERATE` capability.
    RandomGenerate {
        /// Number of bytes requested.
        byte_count: u32,
    },
    /// Restricted raw-secret reveal path for `SECRET_GET`.
    ///
    /// T-047 accepts the typed operation only to reject it before reveal. The
    /// recovery-mode, co-signer, one-shot implementation lands after this slice.
    SecretGet {
        /// Approval identifier for the human co-signer path.
        co_signer_approval_id: String,
    },
}

impl VaultOperation {
    pub(crate) const fn operation_kind(&self) -> &'static str {
        match self {
            Self::Encrypt { .. } => "ENCRYPT",
            Self::Decrypt { .. } => "DECRYPT",
            Self::MacGenerate { .. } => "MAC_GENERATE",
            Self::MacVerify { .. } => "MAC_VERIFY",
            Self::KdfDerive { .. } => "KDF_DERIVE",
            Self::Sign { .. } => "SIGN",
            Self::Verify { .. } => "VERIFY",
            Self::RandomGenerate { .. } => "RANDOM_GENERATE",
            Self::SecretGet { .. } => "SECRET_GET",
        }
    }
}

impl fmt::Display for VaultOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.operation_kind())
    }
}

/// Result of exercising a capability without revealing broker-held key bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum UseCapabilityResult {
    /// Encryption output and broker-generated nonce metadata.
    Encrypted {
        /// Simulated ciphertext in T-047.
        ciphertext: Vec<u8>,
        /// Simulated nonce metadata in T-047.
        nonce: Vec<u8>,
        /// Additional authenticated data returned for caller correlation.
        aad: Vec<u8>,
    },
    /// Decryption output.
    Decrypted {
        /// Simulated plaintext in T-047.
        plaintext: Vec<u8>,
    },
    /// MAC generation output.
    MacGenerated {
        /// Simulated MAC tag in T-047.
        tag: Vec<u8>,
    },
    /// MAC verification result.
    MacVerified {
        /// Simulated verification boolean in T-047.
        valid: bool,
    },
    /// KDF output as a broker-held key handle, never raw derived bytes.
    KdfDerived {
        /// Handle to the broker-held derived key.
        derived_key_handle: KeyMaterialHandle,
    },
    /// Signature output.
    Signed {
        /// Simulated signature bytes in T-047.
        signature: Vec<u8>,
    },
    /// Signature verification result.
    Verified {
        /// Simulated verification boolean in T-047.
        valid: bool,
    },
    /// Random generation output.
    RandomGenerated {
        /// Simulated random bytes in T-047.
        random_bytes: Vec<u8>,
    },
}

/// The Vault Broker contract — S5.2 capability issue/use/list/revoke.
///
/// Implementations are `Send + Sync` so production servers can hold one broker
/// behind `Arc<dyn VaultBroker>`. Operation methods return outputs but never
/// return raw broker-held key material, preserving INV-018 at the type surface.
#[async_trait]
pub trait VaultBroker: Send + Sync {
    /// Issue a capability and bind it to broker-private key material.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when issuance preconditions fail.
    async fn issue_capability(
        &self,
        request: IssueCapabilityRequest,
    ) -> Result<VaultCapability, VaultError>;

    /// Exercise a capability through the use-without-reveal operation surface.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::CapabilityNotFound`] for unknown ids,
    /// [`VaultError::CapabilityExpired`] for expired capabilities,
    /// [`VaultError::CapabilityRevoked`] for revoked capabilities, and
    /// [`VaultError::OperationClassMismatch`] for class/operation mismatches.
    async fn use_capability(
        &self,
        request: UseCapabilityRequest,
    ) -> Result<UseCapabilityResult, VaultError>;

    /// List public capability records issued to one subject.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] if the backing catalog cannot be read.
    async fn list_capabilities(
        &self,
        subject: &SubjectRef,
    ) -> Result<Vec<VaultCapability>, VaultError>;

    /// Revoke a capability.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::CapabilityNotFound`] when the id is unknown and
    /// [`VaultError::InvalidTransition`] when the state cannot be revoked.
    async fn revoke_capability(
        &self,
        capability_id: &CapabilityId,
        revoked_by: &SubjectRef,
    ) -> Result<(), VaultError>;
}

pub(crate) const fn operation_matches_class(
    capability_class: CapabilityClass,
    operation: &VaultOperation,
) -> bool {
    match capability_class {
        CapabilityClass::KeySign | CapabilityClass::BootstrapKeySign => {
            matches!(operation, VaultOperation::Sign { .. })
        }
        CapabilityClass::KeyVerify => matches!(operation, VaultOperation::Verify { .. }),
        CapabilityClass::KeyEncrypt => matches!(operation, VaultOperation::Encrypt { .. }),
        CapabilityClass::KeyDecrypt => matches!(operation, VaultOperation::Decrypt { .. }),
        CapabilityClass::MacGenerate => matches!(operation, VaultOperation::MacGenerate { .. }),
        CapabilityClass::MacVerify => matches!(operation, VaultOperation::MacVerify { .. }),
        CapabilityClass::RandomGenerate => {
            matches!(operation, VaultOperation::RandomGenerate { .. })
        }
        CapabilityClass::SecretGet => matches!(operation, VaultOperation::SecretGet { .. }),
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        reason = "panic-on-failure is the idiomatic test signal"
    )]

    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::capability::{CapabilityState, KeyMaterialHandle};
    use crate::key_material::KeyMaterial;

    #[test]
    fn vault_capability_never_serializes_key_bytes() {
        let key_material = KeyMaterial {
            algorithm: KeyAlgorithm::Aes256Gcm,
            created_at: sample_time(),
            bytes: b"super-secret-key-material".to_vec(),
        };
        let capability = VaultCapability {
            capability_id: CapabilityId::parse("cap_01HXY8K2JPQ7N3M4R5S6T7V8W9")
                .expect("capability id"),
            class: CapabilityClass::KeyEncrypt,
            issued_to: SubjectRef("family:alice".to_owned()),
            issued_at: sample_time(),
            expires_at: None,
            state: CapabilityState::Active,
            key_material_handle: KeyMaterialHandle("vault-internal:slot-7".to_owned()),
        };
        let _broker_private_pair = (capability.clone(), key_material);

        let json = serde_json::to_string(&capability).expect("serialize capability");

        assert!(!json.contains("super-secret-key-material"));
        assert!(!json.contains("key_material_bytes"));
        assert!(!json.contains("\"bytes\""));
    }

    fn sample_time() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 24, 12, 0, 0)
            .single()
            .expect("fixture timestamp is valid")
    }
}
