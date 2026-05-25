//! Verification evidence emission policy (S2.4 -> S3.1).
//!
//! S3.1 currently exposes `RecordType::VerificationResult` (wire ID 10) but no
//! dedicated `VERIFICATION_STARTED` or `PRIMITIVE_EXECUTED` variants. Per the
//! T-070 scope, those helper methods still exist and are folded into
//! `VERIFICATION_RESULT` receipts with distinct typed JSON payload shapes.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S2.4/S3.1 evidence vocabulary"
)]

use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use aios_action::ActionId;
use aios_evidence::{EvidenceError, EvidenceReceipt, ReceiptBuilder, ReceiptChain, RecordType};

use crate::engine::VerificationContext;
use crate::evidence_payloads::{
    count_len, PrimitiveExecutedPayload, VerificationResultPayload, VerificationStartedPayload,
};
use crate::executor::primitive_count;
use crate::{compile_intent, IntentId, PrimitiveResult, VerificationError, VerificationIntent};

/// Constitutional default subject id for verification evidence emissions.
pub const AIOS_VERIFICATION_SUBJECT: &str = "_system:service:verification-engine";

/// Canonical subject reference for verification evidence emission.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubjectRef(
    /// Canonical S5.1 subject id.
    pub String,
);

/// Async append-only sink for sealed, signed verification evidence receipts.
#[async_trait]
pub trait VerificationEvidenceLog: Send + Sync + Debug {
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
pub struct InMemoryVerificationEvidenceLog {
    chain: Mutex<ReceiptChain>,
}

impl Default for InMemoryVerificationEvidenceLog {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryVerificationEvidenceLog {
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
impl VerificationEvidenceLog for InMemoryVerificationEvidenceLog {
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

/// Verification evidence emitter with helpers for the S2.4 run lifecycle.
#[derive(Clone)]
pub struct VerificationEvidenceEmitter {
    log: Arc<dyn VerificationEvidenceLog>,
    signing_key: SigningKey,
    subject: SubjectRef,
}

impl Debug for VerificationEvidenceEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerificationEvidenceEmitter")
            .field("log", &"<dyn VerificationEvidenceLog>")
            .field("signing_key", &"<redacted>")
            .field("subject", &self.subject)
            .finish()
    }
}

impl VerificationEvidenceEmitter {
    /// Construct a new verification evidence emitter.
    #[must_use]
    pub fn new(
        log: Arc<dyn VerificationEvidenceLog>,
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
    pub fn log(&self) -> &Arc<dyn VerificationEvidenceLog> {
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
        action_id: Option<&ActionId>,
        payload: &P,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, VerificationError>
    where
        P: Serialize + Sync,
    {
        let payload_value = serde_json::to_value(payload).map_err(|e| {
            VerificationError::EvidenceEmitFailed(format!("payload serialization failed: {e}"))
        })?;
        let retention = aios_evidence::record::retention_class_for(record_type);
        let mut builder = ReceiptBuilder::new(record_type, retention, self.subject.0.clone())
            .with_payload(payload_value);
        if let Some(action_id) = action_id {
            builder = builder.with_action_id(action_id.clone());
        }
        let receipt = self
            .log
            .append_signed(builder, &self.signing_key, prev_receipt_id)
            .await
            .map_err(|e| VerificationError::EvidenceEmitFailed(e.to_string()))?;
        Ok(receipt.receipt_id().as_str().to_owned())
    }

    /// Emit the verification-start marker.
    ///
    /// There is no dedicated S3.1 `VERIFICATION_STARTED` variant in the current
    /// Rust vocabulary, so this emits `VERIFICATION_RESULT` with a
    /// [`VerificationStartedPayload`].
    ///
    /// # Errors
    ///
    /// Returns [`VerificationError::EvidenceEmitFailed`] when payload
    /// serialization or evidence append fails.
    pub async fn emit_verification_started(
        &self,
        intent: &VerificationIntent,
        context: &VerificationContext,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, VerificationError> {
        let grammar = compile_intent(intent)?;
        let payload = VerificationStartedPayload {
            intent_id: intent.intent_id.clone(),
            action_id: intent.action_id.clone(),
            expression_hash: intent.expression_hash.clone(),
            primitive_count: count_len(primitive_count(&grammar)),
            started_at: context.started_at,
        };
        self.emit(
            RecordType::VerificationResult,
            Some(&payload.action_id),
            &payload,
            prev_receipt_id,
        )
        .await
    }

    /// Emit `VERIFICATION_RESULT` (S3.1 ID 10) after a run completes.
    ///
    /// # Errors
    ///
    /// Returns [`VerificationError::EvidenceEmitFailed`] when evidence append
    /// fails.
    pub async fn emit_verification_result(
        &self,
        _intent: &VerificationIntent,
        result: &crate::VerificationResult,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, VerificationError> {
        let payload = VerificationResultPayload::from_result(result);
        self.emit(
            RecordType::VerificationResult,
            Some(&payload.action_id),
            &payload,
            prev_receipt_id,
        )
        .await
    }

    /// Emit an optional per-primitive execution marker.
    ///
    /// There is no dedicated S3.1 primitive-executed record type in the current
    /// Rust vocabulary, so this emits `VERIFICATION_RESULT` with a redacted
    /// [`PrimitiveExecutedPayload`].
    ///
    /// # Errors
    ///
    /// Returns [`VerificationError::EvidenceEmitFailed`] when evidence append
    /// fails.
    pub async fn emit_primitive_executed(
        &self,
        intent_id: &IntentId,
        primitive_result: &PrimitiveResult,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, VerificationError> {
        let payload = PrimitiveExecutedPayload::from_result(intent_id, primitive_result);
        self.emit(
            RecordType::VerificationResult,
            None,
            &payload,
            prev_receipt_id,
        )
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
