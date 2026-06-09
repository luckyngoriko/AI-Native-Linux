//! Recovery evidence emission policy (S9.x -> S3.1).
//!
//! The current S3.1 Rust vocabulary has dedicated S9.1/S9.2/S9.3 record types
//! for this crate's lifecycle. If a future reduced vocabulary only exposed
//! `RECOVERY_EVENT`, the recovery entry/exit helpers would fold there; this
//! implementation uses the closest actual `RecordType` variants present today.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S9.x evidence vocabulary"
)]

use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use aios_evidence::{EvidenceError, EvidenceReceipt, ReceiptBuilder, ReceiptChain, RecordType};

use crate::evidence_payloads::{
    FirstBootCompletedPayload, FirstBootPhaseCompletedPayload, FirstBootStartedPayload,
    KernelActivatedPayload, KernelCandidateRegisteredPayload, KernelGateResultPayload,
    KernelRolledBackPayload, RecoveryEnteredPayload, RecoveryExitedPayload,
};
use crate::first_boot::FIRST_BOOT_PROVISIONING_PHASES;
use crate::{
    CandidateId, FirstBootContext, FirstBootPhase, KernelCandidate, RecoveryError, RecoveryMode,
    RecoveryState,
};

/// Constitutional default subject id for recovery evidence emissions.
pub const AIOS_RECOVERY_SUBJECT: &str = "_system:service:recovery-coordinator";

/// Canonical subject reference for recovery evidence emission.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecoverySubjectRef(
    /// Canonical S5.1 subject id.
    pub String,
);

/// Async append-only sink for sealed, signed recovery evidence receipts.
#[async_trait]
pub trait RecoveryEvidenceLog: Send + Sync + Debug {
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
pub struct InMemoryRecoveryEvidenceLog {
    chain: Mutex<ReceiptChain>,
}

impl Default for InMemoryRecoveryEvidenceLog {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryRecoveryEvidenceLog {
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
impl RecoveryEvidenceLog for InMemoryRecoveryEvidenceLog {
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

/// Recovery evidence emitter with helpers for S9.x lifecycle points.
#[derive(Clone)]
pub struct RecoveryEvidenceEmitter {
    log: Arc<dyn RecoveryEvidenceLog>,
    signing_key: SigningKey,
    subject: RecoverySubjectRef,
}

impl Debug for RecoveryEvidenceEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryEvidenceEmitter")
            .field("log", &"<dyn RecoveryEvidenceLog>")
            .field("signing_key", &"<redacted>")
            .field("subject", &self.subject)
            .finish()
    }
}

impl RecoveryEvidenceEmitter {
    /// Construct a new recovery evidence emitter.
    #[must_use]
    pub fn new(
        log: Arc<dyn RecoveryEvidenceLog>,
        signing_key: SigningKey,
        subject: RecoverySubjectRef,
    ) -> Self {
        Self {
            log,
            signing_key,
            subject,
        }
    }

    /// Borrow the underlying sink.
    #[must_use]
    pub fn log(&self) -> &Arc<dyn RecoveryEvidenceLog> {
        &self.log
    }

    /// Return the `Ed25519` verifying key for receipts emitted by this emitter.
    #[must_use]
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    pub(crate) async fn emit<P>(
        &self,
        record_type: RecordType,
        payload: &P,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, RecoveryError>
    where
        P: Serialize + Sync,
    {
        let payload_value = serde_json::to_value(payload).map_err(|e| {
            RecoveryError::EvidenceEmitFailed(format!("payload serialization failed: {e}"))
        })?;
        let retention = aios_evidence::record::retention_class_for(record_type);
        let builder = ReceiptBuilder::new(record_type, retention, self.subject.0.clone())
            .with_payload(payload_value);
        let receipt = self
            .log
            .append_signed(builder, &self.signing_key, prev_receipt_id)
            .await
            .map_err(|e| RecoveryError::EvidenceEmitFailed(e.to_string()))?;
        Ok(receipt.receipt_id().as_str().to_owned())
    }

    /// Emit `RECOVERY_BOOT_ENTERED` after successful recovery entry.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_recovery_entered(
        &self,
        state: &RecoveryState,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, RecoveryError> {
        let payload = RecoveryEnteredPayload {
            from_mode: RecoveryMode::Normal,
            to_mode: state.mode,
            entered_at: state.entered_at.unwrap_or_else(Utc::now),
            reason: state.reason.clone(),
            operator_grant: state.operator_grant.clone(),
        };
        self.emit(RecordType::RecoveryBootEntered, &payload, prev_receipt_id)
            .await
    }

    /// Emit `RECOVERY_BOOT_EXITED` after successful recovery exit.
    ///
    /// Callers that have the raw opaque token should prefer
    /// [`Self::emit_recovery_exited_with_exit_token`] so the payload carries the
    /// token hash. This method keeps the required helper surface and emits a
    /// redacted placeholder hash, never raw token material.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_recovery_exited(
        &self,
        state: &RecoveryState,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, RecoveryError> {
        self.emit_recovery_exited_with_token_hash(
            state,
            hash_exit_token("<not-captured>"),
            prev_receipt_id,
        )
        .await
    }

    /// Emit `RECOVERY_BOOT_EXITED` with a BLAKE3 hash of the raw exit token.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_recovery_exited_with_exit_token(
        &self,
        state: &RecoveryState,
        exit_token: &str,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, RecoveryError> {
        self.emit_recovery_exited_with_token_hash(
            state,
            hash_exit_token(exit_token),
            prev_receipt_id,
        )
        .await
    }

    async fn emit_recovery_exited_with_token_hash(
        &self,
        state: &RecoveryState,
        exit_token: String,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, RecoveryError> {
        let payload = RecoveryExitedPayload {
            from_mode: RecoveryMode::Recovery,
            to_mode: state.mode,
            exited_at: Utc::now(),
            exit_token,
        };
        self.emit(RecordType::RecoveryBootExited, &payload, prev_receipt_id)
            .await
    }

    /// Emit `FIRST_BOOT_STARTED`.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_first_boot_started(
        &self,
        context: &FirstBootContext,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, RecoveryError> {
        let payload = FirstBootStartedPayload {
            boot_id: context.boot_id.clone(),
            started_at: context.started_at,
            expected_phases: FIRST_BOOT_PROVISIONING_PHASES.to_vec(),
        };
        self.emit(RecordType::FirstBootStarted, &payload, prev_receipt_id)
            .await
    }

    /// Emit `FIRST_BOOT_STAGE_COMPLETED`.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_first_boot_phase_completed(
        &self,
        phase: FirstBootPhase,
        context: &FirstBootContext,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, RecoveryError> {
        let payload = FirstBootPhaseCompletedPayload {
            boot_id: context.boot_id.clone(),
            phase,
            completed_at: Utc::now(),
        };
        self.emit(
            RecordType::FirstBootStageCompleted,
            &payload,
            prev_receipt_id,
        )
        .await
    }

    /// Emit `FIRST_BOOT_COMPLETE`.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_first_boot_completed(
        &self,
        context: &FirstBootContext,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, RecoveryError> {
        self.emit_first_boot_completed_with_skipped(context, Vec::new(), prev_receipt_id)
            .await
    }

    /// Emit `FIRST_BOOT_COMPLETE` with skipped-phase accounting.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_first_boot_completed_with_skipped(
        &self,
        context: &FirstBootContext,
        skipped_phases: Vec<FirstBootPhase>,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, RecoveryError> {
        let payload = FirstBootCompletedPayload {
            boot_id: context.boot_id.clone(),
            completed_at: context.completed_at.unwrap_or_else(Utc::now),
            total_phases: context.performed_phases.len() as u64,
            skipped_phases,
        };
        self.emit(RecordType::FirstBootComplete, &payload, prev_receipt_id)
            .await
    }

    /// Emit kernel-candidate registration evidence.
    ///
    /// S3.1 currently has no `KERNEL_CANDIDATE_REGISTERED` record type; the
    /// closest S9.3 lifecycle record is `KERNEL_PIPELINE_STARTED`.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_kernel_candidate_registered(
        &self,
        candidate: &KernelCandidate,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, RecoveryError> {
        let payload = KernelCandidateRegisteredPayload {
            candidate_id: candidate.candidate_id.clone(),
            version: candidate.version.clone(),
            kernel_blake3: candidate.kernel_blake3.clone(),
            signing_authority: candidate.signing_authority.clone(),
            registered_at: candidate.registered_at,
        };
        self.emit(RecordType::KernelPipelineStarted, &payload, prev_receipt_id)
            .await
    }

    /// Emit the shallow T-080 `KERNEL_GATE_RESULT` gate-pass witness.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_kernel_gate_result(
        &self,
        candidate: &KernelCandidate,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, RecoveryError> {
        let payload = KernelGateResultPayload {
            candidate_id: candidate.candidate_id.clone(),
            version: candidate.version.clone(),
            kernel_blake3: candidate.kernel_blake3.clone(),
            result: "GATE_PASSED".to_owned(),
            completed_at: Utc::now(),
        };
        self.emit(RecordType::KernelGateResult, &payload, prev_receipt_id)
            .await
    }

    /// Emit `KERNEL_PROMOTED_TO_A`.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_kernel_activated(
        &self,
        candidate: &KernelCandidate,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, RecoveryError> {
        let payload = KernelActivatedPayload {
            candidate_id: candidate.candidate_id.clone(),
            version: candidate.version.clone(),
            kernel_blake3: candidate.kernel_blake3.clone(),
            activated_at: Utc::now(),
            required_recovery: candidate.manifest.requires_recovery_install,
        };
        self.emit(RecordType::KernelPromotedToA, &payload, prev_receipt_id)
            .await
    }

    /// Emit `KERNEL_ROLLBACK_PERFORMED`.
    ///
    /// Callers that know the restored candidate id should prefer
    /// [`Self::emit_kernel_rolled_back_to_previous`].
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_kernel_rolled_back(
        &self,
        candidate: &KernelCandidate,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, RecoveryError> {
        self.emit_kernel_rolled_back_to_previous(
            candidate,
            candidate.candidate_id.clone(),
            "rollback performed",
            prev_receipt_id,
        )
        .await
    }

    /// Emit `KERNEL_ROLLBACK_PERFORMED` with the restored candidate id.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_kernel_rolled_back_to_previous(
        &self,
        candidate: &KernelCandidate,
        previous_candidate_id: CandidateId,
        reason: &str,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, RecoveryError> {
        let payload = KernelRolledBackPayload {
            candidate_id: candidate.candidate_id.clone(),
            previous_candidate_id,
            reason: reason.to_owned(),
            rolled_back_at: Utc::now(),
        };
        self.emit(
            RecordType::KernelRollbackPerformed,
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

fn hash_exit_token(exit_token: &str) -> String {
    format!("blake3:{}", blake3::hash(exit_token.as_bytes()).to_hex())
}
