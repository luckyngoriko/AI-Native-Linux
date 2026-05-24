//! Vault evidence emission policy (S5.2/S5.4 -> S3.1).

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S5.2/S5.4 evidence vocabulary"
)]

use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::Serialize;
use tokio::sync::Mutex;

use aios_evidence::{EvidenceError, EvidenceReceipt, ReceiptBuilder, ReceiptChain, RecordType};

use crate::capability::{CapabilityId, VaultCapability};
use crate::error::VaultError;
use crate::evidence_payloads::{
    CapabilityExpiredPayload, CapabilityIssuedPayload, CapabilityRevokedPayload,
    CapabilityUsedPayload, OverrideConsumedPayload, OverrideGrantedPayload, OverrideRevokedPayload,
};
use crate::identity::SubjectRef;
use crate::override_class::OverrideBinding;

/// Constitutional default subject id for vault evidence emissions.
pub const AIOS_VAULT_SUBJECT: &str = "_system:service:vault-broker";

/// Async append-only sink for sealed, signed vault evidence receipts.
#[async_trait]
pub trait VaultEvidenceLog: Send + Sync + Debug {
    /// Seal, sign, and append a receipt builder.
    ///
    /// # Errors
    ///
    /// Returns [`EvidenceError`] when sealing, signing, chain validation, or
    /// backend storage fails.
    async fn append_signed(
        &self,
        builder: ReceiptBuilder,
        signing_key: &SigningKey,
        expected_previous_receipt_id: Option<&str>,
    ) -> Result<EvidenceReceipt, EvidenceError>;
}

/// In-process evidence sink backed by a single `ReceiptChain`.
#[derive(Debug)]
pub struct InMemoryVaultEvidenceLog {
    chain: Mutex<ReceiptChain>,
}

impl Default for InMemoryVaultEvidenceLog {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryVaultEvidenceLog {
    /// Construct an empty in-memory evidence log.
    #[must_use]
    pub fn new() -> Self {
        Self {
            chain: Mutex::new(ReceiptChain::new()),
        }
    }

    /// Snapshot every receipt currently on the chain.
    pub async fn receipts(&self) -> Vec<EvidenceReceipt> {
        self.chain.lock().await.receipts().to_vec()
    }

    /// Count of receipts currently on the chain.
    pub async fn len(&self) -> usize {
        self.chain.lock().await.len()
    }

    /// `true` iff the chain has no receipts yet.
    pub async fn is_empty(&self) -> bool {
        self.chain.lock().await.is_empty()
    }

    /// Verify BLAKE3 hash-chain integrity.
    ///
    /// # Errors
    ///
    /// Returns [`EvidenceError`] on the first chain-link mismatch.
    pub async fn verify_integrity(&self) -> Result<(), EvidenceError> {
        self.chain.lock().await.verify_integrity()
    }

    /// Verify BLAKE3 hash-chain integrity and each receipt signature.
    ///
    /// # Errors
    ///
    /// Returns [`EvidenceError`] on the first chain or signature failure.
    pub async fn verify_integrity_signed(
        &self,
        verifying_key: &VerifyingKey,
    ) -> Result<(), EvidenceError> {
        self.chain
            .lock()
            .await
            .verify_integrity_signed(verifying_key)
    }
}

#[async_trait]
impl VaultEvidenceLog for InMemoryVaultEvidenceLog {
    async fn append_signed(
        &self,
        builder: ReceiptBuilder,
        signing_key: &SigningKey,
        expected_previous_receipt_id: Option<&str>,
    ) -> Result<EvidenceReceipt, EvidenceError> {
        let mut guard = self.chain.lock().await;
        let previous = guard.receipts().last().cloned();
        validate_expected_previous_receipt(previous.as_ref(), expected_previous_receipt_id)?;
        let receipt = builder.seal_signed(previous.as_ref(), signing_key)?;
        guard.append(receipt.clone())?;
        drop(guard);
        Ok(receipt)
    }
}

/// Vault evidence emitter with one helper per S5.2/S5.4 emission point.
#[derive(Clone)]
pub struct VaultEvidenceEmitter {
    log: Arc<dyn VaultEvidenceLog>,
    signing_key: SigningKey,
    subject: SubjectRef,
}

impl Debug for VaultEvidenceEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultEvidenceEmitter")
            .field("log", &"<dyn VaultEvidenceLog>")
            .field("signing_key", &"<redacted>")
            .field("subject", &self.subject)
            .finish()
    }
}

impl VaultEvidenceEmitter {
    /// Construct a new vault evidence emitter.
    #[must_use]
    pub fn new(
        log: Arc<dyn VaultEvidenceLog>,
        signing_key: SigningKey,
        subject: SubjectRef,
    ) -> Self {
        Self {
            log,
            signing_key,
            subject,
        }
    }

    /// Borrow the underlying sink.
    #[must_use]
    pub fn log(&self) -> &Arc<dyn VaultEvidenceLog> {
        &self.log
    }

    /// Return the Ed25519 verifying key for receipts emitted by this emitter.
    #[must_use]
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    async fn emit<P>(
        &self,
        record_type: RecordType,
        payload: &P,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, VaultError>
    where
        P: Serialize + Sync,
    {
        let payload_value = serde_json::to_value(payload).map_err(|e| {
            VaultError::EvidenceEmitFailed(format!("payload serialization failed: {e}"))
        })?;
        let retention = aios_evidence::record::retention_class_for(record_type);
        let builder = ReceiptBuilder::new(record_type, retention, self.subject.0.clone())
            .with_payload(payload_value);
        let receipt = self
            .log
            .append_signed(builder, &self.signing_key, prev_receipt_id)
            .await
            .map_err(|e| VaultError::EvidenceEmitFailed(e.to_string()))?;
        Ok(receipt.receipt_id().as_str().to_owned())
    }

    /// Emit `VAULT_CAPABILITY_ISSUED` after successful capability issuance.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::EvidenceEmitFailed`] when payload serialization or
    /// evidence append fails.
    pub async fn emit_capability_issued(
        &self,
        capability: &VaultCapability,
        issued_to: &SubjectRef,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, VaultError> {
        let payload = CapabilityIssuedPayload {
            capability_id: capability.capability_id.clone(),
            class: capability.class,
            issued_to: issued_to.clone(),
            issued_at: capability.issued_at,
            expires_at: capability.expires_at,
        };
        self.emit(RecordType::VaultCapabilityIssued, &payload, prev_receipt_id)
            .await
    }

    /// Emit the redacted use-without-reveal operation event.
    ///
    /// The S3.1 Rust vocabulary names this `VAULT_OPERATION`; there is no
    /// `CAPABILITY_USED` variant in the current closed enum.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_capability_used(
        &self,
        capability_id: &CapabilityId,
        op_kind: &str,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, VaultError> {
        let payload = CapabilityUsedPayload {
            capability_id: capability_id.clone(),
            operation_kind: operation_kind_variant_name(op_kind),
            used_at: Utc::now(),
            subject: self.subject.clone(),
        };
        self.emit(RecordType::VaultOperation, &payload, prev_receipt_id)
            .await
    }

    /// Emit `VAULT_CAPABILITY_REVOKED`.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_capability_revoked(
        &self,
        capability_id: &CapabilityId,
        revoked_by: &SubjectRef,
        reason: &str,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, VaultError> {
        let payload = CapabilityRevokedPayload {
            capability_id: capability_id.clone(),
            revoked_by: revoked_by.clone(),
            reason: reason.to_owned(),
            revoked_at: Utc::now(),
        };
        self.emit(
            RecordType::VaultCapabilityRevoked,
            &payload,
            prev_receipt_id,
        )
        .await
    }

    /// Emit an expiration transition as redacted `VAULT_OPERATION`.
    ///
    /// S5.2 §4 says normal expiration has no dedicated lifecycle record and the
    /// current S3.1 enum has no `VAULT_CAPABILITY_EXPIRED` variant.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_capability_expired(
        &self,
        capability_id: &CapabilityId,
        expired_at: DateTime<Utc>,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, VaultError> {
        let payload = CapabilityExpiredPayload {
            capability_id: capability_id.clone(),
            expired_at,
        };
        self.emit(RecordType::VaultOperation, &payload, prev_receipt_id)
            .await
    }

    /// Emit `OVERRIDE_GRANTED`.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_override_granted(
        &self,
        binding: &OverrideBinding,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, VaultError> {
        let payload = OverrideGrantedPayload {
            binding_id: binding.binding_id.clone(),
            class: binding.class,
            granted_by: binding.granted_by.clone(),
            target_action_id: binding.target_action_id.clone(),
            granted_at: binding.granted_at,
            expires_at: binding.expires_at,
        };
        self.emit(RecordType::OverrideGranted, &payload, prev_receipt_id)
            .await
    }

    /// Emit `OVERRIDE_CONSUMED`.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_override_consumed(
        &self,
        binding_id: &str,
        consumer: &SubjectRef,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, VaultError> {
        let payload = OverrideConsumedPayload {
            binding_id: binding_id.to_owned(),
            consumer: consumer.clone(),
            consumed_at: Utc::now(),
        };
        self.emit(RecordType::OverrideConsumed, &payload, prev_receipt_id)
            .await
    }

    /// Emit `OVERRIDE_REVOKED`.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_override_revoked(
        &self,
        binding_id: &str,
        revoker: &SubjectRef,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, VaultError> {
        let payload = OverrideRevokedPayload {
            binding_id: binding_id.to_owned(),
            revoker: revoker.clone(),
            revoked_at: Utc::now(),
        };
        self.emit(RecordType::OverrideRevoked, &payload, prev_receipt_id)
            .await
    }
}

fn validate_expected_previous_receipt(
    previous: Option<&EvidenceReceipt>,
    expected_previous_receipt_id: Option<&str>,
) -> Result<(), EvidenceError> {
    let Some(expected) = expected_previous_receipt_id else {
        return Ok(());
    };
    let Some(previous) = previous else {
        return Err(EvidenceError::EncodingFailed(format!(
            "expected previous receipt id {expected}, but chain is empty"
        )));
    };
    if previous.receipt_id().as_str() == expected {
        return Ok(());
    }
    Err(EvidenceError::EncodingFailed(format!(
        "expected previous receipt id {expected}, but chain tail is {}",
        previous.receipt_id().as_str()
    )))
}

fn operation_kind_variant_name(operation_kind: &str) -> String {
    match operation_kind {
        "ENCRYPT" | "Encrypt" => "Encrypt".to_owned(),
        "DECRYPT" | "Decrypt" => "Decrypt".to_owned(),
        "MAC_GENERATE" | "MacGenerate" => "MacGenerate".to_owned(),
        "MAC_VERIFY" | "MacVerify" => "MacVerify".to_owned(),
        "KDF_DERIVE" | "KdfDerive" => "KdfDerive".to_owned(),
        "SIGN" | "Sign" => "Sign".to_owned(),
        "VERIFY" | "Verify" => "Verify".to_owned(),
        "RANDOM_GENERATE" | "RandomGenerate" => "RandomGenerate".to_owned(),
        "SECRET_GET" | "SecretGet" => "SecretGet".to_owned(),
        other => other.to_owned(),
    }
}
