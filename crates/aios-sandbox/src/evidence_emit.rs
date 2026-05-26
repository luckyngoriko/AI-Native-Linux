//! Sandbox evidence emission policy (S3.2 -> S3.1).
//!
//! The current S3.1 Rust vocabulary has no dedicated `SANDBOX_COMPOSED` or
//! `RESOURCE_LIMIT_EXCEEDED` variants; those events fold into `POLICY_DECISION`.
//! `SANDBOX_VIOLATION_DETECTED` folds into `SANDBOX_BUNDLE_REJECTED` (ID 408).
//! `GPU_CAPABILITY_BOUND` folds into `GPU_CAPABILITY_DENIED` (ID 73).

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S3.2 evidence vocabulary"
)]

use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use aios_evidence::{EvidenceError, EvidenceReceipt, ReceiptBuilder, ReceiptChain, RecordType};

use crate::evidence_payloads::{
    GpuCapabilityBoundPayload, ResourceLimitExceededPayload, SandboxComposedPayload,
    SandboxViolationDetectedPayload,
};
use crate::SandboxError;

/// Constitutional default subject id for sandbox evidence emissions.
pub const AIOS_SANDBOX_SUBJECT: &str = "_system:service:sandbox-composer";

/// Canonical subject reference for sandbox evidence emission.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SandboxSubjectRef(
    /// Canonical S5.1 subject id.
    pub String,
);

/// Async append-only sink for sealed, signed sandbox evidence receipts.
#[async_trait]
pub trait SandboxEvidenceLog: Send + Sync + Debug {
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
pub struct InMemorySandboxEvidenceLog {
    chain: Mutex<ReceiptChain>,
}

impl Default for InMemorySandboxEvidenceLog {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemorySandboxEvidenceLog {
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
impl SandboxEvidenceLog for InMemorySandboxEvidenceLog {
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

/// Sandbox evidence emitter with helpers for S3.2 lifecycle points.
#[derive(Clone)]
pub struct SandboxEvidenceEmitter {
    log: Arc<dyn SandboxEvidenceLog>,
    signing_key: SigningKey,
    subject: SandboxSubjectRef,
}

impl Debug for SandboxEvidenceEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SandboxEvidenceEmitter")
            .field("log", &"<dyn SandboxEvidenceLog>")
            .field("signing_key", &"<redacted>")
            .field("subject", &self.subject)
            .finish()
    }
}

impl SandboxEvidenceEmitter {
    /// Construct a new sandbox evidence emitter.
    #[must_use]
    pub fn new(
        log: Arc<dyn SandboxEvidenceLog>,
        signing_key: SigningKey,
        subject: SandboxSubjectRef,
    ) -> Self {
        Self {
            log,
            signing_key,
            subject,
        }
    }

    /// Borrow the underlying sink.
    #[must_use]
    pub fn log(&self) -> &Arc<dyn SandboxEvidenceLog> {
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
    ) -> Result<String, SandboxError>
    where
        P: Serialize + Sync,
    {
        let payload_value = serde_json::to_value(payload).map_err(|e| {
            SandboxError::EvidenceEmitFailed(format!("payload serialization failed: {e}"))
        })?;
        let retention = aios_evidence::record::retention_class_for(record_type);
        let builder = ReceiptBuilder::new(record_type, retention, self.subject.0.clone())
            .with_payload(payload_value);
        let receipt = self
            .log
            .append_signed(builder, &self.signing_key, prev_receipt_id)
            .await
            .map_err(|e| SandboxError::EvidenceEmitFailed(e.to_string()))?;
        Ok(receipt.receipt_id().as_str().to_owned())
    }

    /// Emit `SANDBOX_COMPOSED` after successful profile composition.
    ///
    /// S3.1 has no dedicated `SANDBOX_COMPOSED` `RecordType`; this folds into
    /// `POLICY_DECISION` (ID 4) with a typed payload.
    ///
    /// # Errors
    ///
    /// Returns [`SandboxError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_sandbox_composed(
        &self,
        payload: &SandboxComposedPayload,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, SandboxError> {
        self.emit(RecordType::PolicyDecision, payload, prev_receipt_id)
            .await
    }

    /// Emit `SANDBOX_VIOLATION_DETECTED` after a sandbox violation is detected.
    ///
    /// S3.1 has `SANDBOX_BUNDLE_REJECTED` (ID 408) as the closest variant;
    /// this folds into that record type with a typed payload.
    ///
    /// # Errors
    ///
    /// Returns [`SandboxError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_sandbox_violation_detected(
        &self,
        payload: &SandboxViolationDetectedPayload,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, SandboxError> {
        self.emit(RecordType::SandboxBundleRejected, payload, prev_receipt_id)
            .await
    }

    /// Emit `GPU_CAPABILITY_BOUND` after a GPU capability binding is issued.
    ///
    /// S3.1 has `GPU_CAPABILITY_DENIED` (ID 73) as the closest variant;
    /// this folds into that record type with a typed payload.
    ///
    /// # Errors
    ///
    /// Returns [`SandboxError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_gpu_capability_bound(
        &self,
        payload: &GpuCapabilityBoundPayload,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, SandboxError> {
        self.emit(RecordType::GpuCapabilityDenied, payload, prev_receipt_id)
            .await
    }

    /// Emit `RESOURCE_LIMIT_EXCEEDED` when a resource limit is exceeded.
    ///
    /// S3.1 has no dedicated `RESOURCE_LIMIT_EXCEEDED` `RecordType`; this folds
    /// into `POLICY_DECISION` (ID 4) with a typed payload.
    ///
    /// # Errors
    ///
    /// Returns [`SandboxError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_resource_limit_exceeded(
        &self,
        payload: &ResourceLimitExceededPayload,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, SandboxError> {
        self.emit(RecordType::PolicyDecision, payload, prev_receipt_id)
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

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use crate::composer::SubjectRef;
    use crate::{GpuCapabilityClass, IsolationKind, NetworkPosture, ProfileId};
    use chrono::Utc;

    fn make_signing_key() -> SigningKey {
        SigningKey::from_bytes(&[
            0x42, 0xd0, 0x2c, 0x5e, 0x84, 0x82, 0x3c, 0x71, 0x85, 0xf1, 0x0e, 0x78, 0x4a, 0xdd,
            0x02, 0x9b, 0xd1, 0x4b, 0x2b, 0x6b, 0x39, 0x6a, 0xab, 0x95, 0xb8, 0x58, 0x05, 0x14,
            0xa5, 0x67, 0xe4, 0x19,
        ])
    }

    fn make_emitter() -> (
        SandboxEvidenceEmitter,
        Arc<InMemorySandboxEvidenceLog>,
        VerifyingKey,
    ) {
        let log = Arc::new(InMemorySandboxEvidenceLog::new());
        let signing_key = make_signing_key();
        let verifying_key = signing_key.verifying_key();
        let emitter = SandboxEvidenceEmitter::new(
            log.clone(),
            signing_key,
            SandboxSubjectRef(AIOS_SANDBOX_SUBJECT.to_string()),
        );
        (emitter, log, verifying_key)
    }

    #[test]
    fn aios_sandbox_subject_is_correct() {
        assert_eq!(AIOS_SANDBOX_SUBJECT, "_system:service:sandbox-composer");
    }

    #[test]
    fn sandbox_subject_ref_display() {
        let subject = SandboxSubjectRef("_system:service:sandbox-composer".into());
        assert_eq!(subject.0, "_system:service:sandbox-composer");
    }

    #[tokio::test]
    async fn in_memory_log_new_is_empty() {
        let log = InMemorySandboxEvidenceLog::new();
        assert!(log.is_empty().await);
        assert_eq!(log.len().await, 0);
    }

    #[tokio::test]
    async fn in_memory_log_default_is_empty() {
        let log = InMemorySandboxEvidenceLog::default();
        assert!(log.is_empty().await);
    }

    #[tokio::test]
    async fn emit_and_verify_chain_integrity() {
        let (emitter, log, vk) = make_emitter();
        let payload = SandboxComposedPayload {
            profile_id: ProfileId::new(),
            isolation_kind: IsolationKind::VmGuest,
            network_posture: NetworkPosture::LoopbackOnly,
            gpu_capability_class: GpuCapabilityClass::GpuBasic2d,
            composed_at: Utc::now(),
        };
        let receipt_id = emitter.emit_sandbox_composed(&payload, None).await.unwrap();
        assert!(!receipt_id.is_empty());
        assert_eq!(log.len().await, 1);
        log.verify_integrity().await.unwrap();
        log.verify_integrity_signed(&vk).await.unwrap();
    }

    #[tokio::test]
    async fn emit_sandbox_violation_detected() {
        let (emitter, log, _vk) = make_emitter();
        let payload = SandboxViolationDetectedPayload {
            action_id: "act_test".into(),
            violation_summary: "test violation".into(),
            profile_id: ProfileId::new(),
            detected_at: Utc::now(),
        };
        let receipt_id = emitter
            .emit_sandbox_violation_detected(&payload, None)
            .await
            .unwrap();
        assert!(!receipt_id.is_empty());
        assert_eq!(log.len().await, 1);
    }

    #[tokio::test]
    async fn emit_gpu_capability_bound() {
        let (emitter, log, _vk) = make_emitter();
        let payload = GpuCapabilityBoundPayload {
            binding_id: "gcb_test".into(),
            group_id: "group-test".into(),
            subject: SubjectRef::new("agent:dev"),
            gpu_capability_class: GpuCapabilityClass::GpuRich2d,
            degraded_isolation: false,
            bound_at: Utc::now(),
        };
        let receipt_id = emitter
            .emit_gpu_capability_bound(&payload, None)
            .await
            .unwrap();
        assert!(!receipt_id.is_empty());
        assert_eq!(log.len().await, 1);
    }

    #[tokio::test]
    async fn emit_resource_limit_exceeded() {
        let (emitter, log, _vk) = make_emitter();
        let payload = ResourceLimitExceededPayload {
            profile_id: ProfileId::new(),
            limit_kind: "memory_max_bytes".into(),
            requested: 2_000_000_000,
            max: 1_000_000_000,
            exceeded_at: Utc::now(),
        };
        let receipt_id = emitter
            .emit_resource_limit_exceeded(&payload, None)
            .await
            .unwrap();
        assert!(!receipt_id.is_empty());
        assert_eq!(log.len().await, 1);
    }

    #[tokio::test]
    async fn chain_linkage_across_multiple_emits() {
        let (emitter, log, vk) = make_emitter();
        let p1 = SandboxComposedPayload {
            profile_id: ProfileId::new(),
            isolation_kind: IsolationKind::VmGuest,
            network_posture: NetworkPosture::DenyAll,
            gpu_capability_class: GpuCapabilityClass::GpuPassiveDisplay,
            composed_at: Utc::now(),
        };
        let id1 = emitter.emit_sandbox_composed(&p1, None).await.unwrap();
        let p2 = ResourceLimitExceededPayload {
            profile_id: ProfileId::new(),
            limit_kind: "cpu".into(),
            requested: 200,
            max: 100,
            exceeded_at: Utc::now(),
        };
        let id2 = emitter
            .emit_resource_limit_exceeded(&p2, Some(&id1))
            .await
            .unwrap();
        assert_ne!(id1, id2);
        assert_eq!(log.len().await, 2);
        log.verify_integrity().await.unwrap();
        log.verify_integrity_signed(&vk).await.unwrap();
    }

    #[tokio::test]
    async fn prev_receipt_id_mismatch_returns_error() {
        let (emitter, _log, _vk) = make_emitter();
        let payload = SandboxComposedPayload {
            profile_id: ProfileId::new(),
            isolation_kind: IsolationKind::VmGuest,
            network_posture: NetworkPosture::DenyAll,
            gpu_capability_class: GpuCapabilityClass::GpuPassiveDisplay,
            composed_at: Utc::now(),
        };
        let err = emitter
            .emit_sandbox_composed(&payload, Some("nonexistent_id"))
            .await
            .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("evidence emit failed"), "got: {msg}");
    }

    #[tokio::test]
    async fn receipts_snapshot_returns_all() {
        let (emitter, log, _vk) = make_emitter();
        let p1 = SandboxComposedPayload {
            profile_id: ProfileId::new(),
            isolation_kind: IsolationKind::ProcessContainer,
            network_posture: NetworkPosture::DenyAll,
            gpu_capability_class: GpuCapabilityClass::GpuPassiveDisplay,
            composed_at: Utc::now(),
        };
        let p2 = ResourceLimitExceededPayload {
            profile_id: ProfileId::new(),
            limit_kind: "fd".into(),
            requested: 500,
            max: 256,
            exceeded_at: Utc::now(),
        };
        emitter.emit_sandbox_composed(&p1, None).await.unwrap();
        emitter
            .emit_resource_limit_exceeded(&p2, None)
            .await
            .unwrap();
        let receipts = log.receipts().await;
        assert_eq!(receipts.len(), 2);
    }

    #[tokio::test]
    async fn debug_impl_redacts_signing_key() {
        let (emitter, _log, _vk) = make_emitter();
        let debug = format!("{emitter:?}");
        assert!(debug.contains("<redacted>"), "should show redacted marker");
        // The value must be redacted — the raw key bytes must not appear
        assert!(
            !debug.contains("SigningKey"),
            "should not show raw key type"
        );
    }

    #[test]
    fn sandbox_subject_ref_serde_round_trip() {
        let subject = SandboxSubjectRef("_system:service:sandbox-composer".into());
        let json = serde_json::to_string(&subject).unwrap();
        assert_eq!(json, "\"_system:service:sandbox-composer\"");
        let back: SandboxSubjectRef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, subject);
    }
}
