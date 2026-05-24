//! AIOS-FS evidence emission policy (S1.3 -> S3.1).

#![allow(
    clippy::module_name_repetitions,
    reason = "AIOS-FS public names mirror the spec vocabulary"
)]

use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::Serialize;
use tokio::sync::Mutex;

use aios_action::{blake3_hash, jcs_canonicalize, ActionId};
use aios_evidence::{EvidenceError, EvidenceReceipt, ReceiptBuilder, ReceiptChain, RecordType};

use crate::chunk::ChunkRef;
use crate::error::FsError;
use crate::evidence_payloads::{
    ActionReceivedPayload, ConflictEventPayload, ConflictResolutionKind, GcPassPayload,
    QuarantineEventPayload,
};
use crate::fs_trait::{ObjectWriteRequest, ObjectWriteResult};
use crate::gc::GcPassReport;
use crate::object::{ObjectId, SubjectRef};
use crate::quarantine::{QuarantineDisposition, QuarantineReceipt, QuarantineTrigger};
use crate::transaction::TransactionId;
use crate::version::VersionId;

/// Constitutional default subject id for AIOS-FS evidence emissions.
pub const AIOS_FS_SUBJECT: &str = "_system:service:aios-fs";

/// Async append-only sink for sealed, signed AIOS-FS evidence receipts.
#[async_trait]
pub trait FsEvidenceLog: Send + Sync + Debug {
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
    ) -> Result<EvidenceReceipt, EvidenceError>;
}

/// In-process evidence sink backed by a single `ReceiptChain`.
#[derive(Debug)]
pub struct InMemoryFsEvidenceLog {
    chain: Mutex<ReceiptChain>,
}

impl Default for InMemoryFsEvidenceLog {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryFsEvidenceLog {
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
}

#[async_trait]
impl FsEvidenceLog for InMemoryFsEvidenceLog {
    async fn append_signed(
        &self,
        builder: ReceiptBuilder,
        signing_key: &SigningKey,
    ) -> Result<EvidenceReceipt, EvidenceError> {
        let mut guard = self.chain.lock().await;
        let previous = guard.receipts().last().cloned();
        let receipt = builder.seal_signed(previous.as_ref(), signing_key)?;
        guard.append(receipt.clone())?;
        drop(guard);
        Ok(receipt)
    }
}

/// AIOS-FS evidence emitter with one helper per S1.3 emission point.
#[derive(Clone)]
pub struct FsEvidenceEmitter {
    log: Arc<dyn FsEvidenceLog>,
    signing_key: SigningKey,
    subject: SubjectRef,
}

impl Debug for FsEvidenceEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FsEvidenceEmitter")
            .field("log", &"<dyn FsEvidenceLog>")
            .field("signing_key", &"<redacted>")
            .field("subject", &self.subject)
            .finish()
    }
}

impl FsEvidenceEmitter {
    /// Construct a new AIOS-FS evidence emitter.
    #[must_use]
    pub fn new(log: Arc<dyn FsEvidenceLog>, signing_key: SigningKey, subject: SubjectRef) -> Self {
        Self {
            log,
            signing_key,
            subject,
        }
    }

    /// Borrow the underlying sink.
    #[must_use]
    pub fn log(&self) -> &Arc<dyn FsEvidenceLog> {
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
    ) -> Result<String, FsError>
    where
        P: Serialize + Sync,
    {
        let payload_value = serde_json::to_value(payload).map_err(|e| {
            FsError::EvidenceEmitFailed(format!("payload serialization failed: {e}"))
        })?;
        let retention = aios_evidence::record::retention_class_for(record_type);
        let mut builder = ReceiptBuilder::new(record_type, retention, self.subject.0.clone())
            .with_payload(payload_value);
        if let Some(action_id) = action_id {
            builder = builder.with_action_id(action_id.clone());
        }
        let receipt = self
            .log
            .append_signed(builder, &self.signing_key)
            .await
            .map_err(|e| FsError::EvidenceEmitFailed(e.to_string()))?;
        Ok(receipt.receipt_id().as_str().to_owned())
    }

    /// Emit `ACTION_RECEIVED` after a successful object write.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::EvidenceEmitFailed`] when payload serialization or
    /// evidence append fails.
    pub async fn emit_action_received(
        &self,
        write: &ObjectWriteRequest,
        write_result: &ObjectWriteResult,
        txn_id: &TransactionId,
    ) -> Result<String, FsError> {
        let payload = ActionReceivedPayload {
            object_id: write_result.object_id.clone(),
            version_id: write_result.version_id.clone(),
            transaction_id: txn_id.clone(),
            subject: write.subject.clone(),
            action_id: write.action_id.clone(),
            chunks_count: count_len(write.chunks.len()),
            content_hash: content_hash_for_chunk_refs(&write.chunks)?,
        };
        self.emit(
            RecordType::ActionReceived,
            payload.action_id.as_ref(),
            &payload,
        )
        .await
    }

    /// Emit a quarantine-entry `QUARANTINE_EVENT`.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::EvidenceEmitFailed`] when the evidence append fails.
    pub async fn emit_quarantine_enter(
        &self,
        version_id: &VersionId,
        trigger: QuarantineTrigger,
        reason: &str,
        receipt: &QuarantineReceipt,
    ) -> Result<String, FsError> {
        let payload = QuarantineEventPayload {
            version_id: version_id.clone(),
            trigger: Some(trigger),
            disposition: None,
            reason: reason.to_owned(),
            transitioned_at: receipt.transitioned_at,
        };
        self.emit(RecordType::QuarantineEvent, None, &payload).await
    }

    /// Emit a quarantine-exit `QUARANTINE_EVENT`.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::EvidenceEmitFailed`] when the evidence append fails.
    pub async fn emit_quarantine_exit(
        &self,
        version_id: &VersionId,
        disposition: QuarantineDisposition,
        receipt: &QuarantineReceipt,
    ) -> Result<String, FsError> {
        let payload = QuarantineEventPayload {
            version_id: version_id.clone(),
            trigger: None,
            disposition: Some(disposition),
            reason: receipt.reason.clone(),
            transitioned_at: receipt.transitioned_at,
        };
        self.emit(RecordType::QuarantineEvent, None, &payload).await
    }

    /// Emit a `CONFLICT_EVENT` for the future conflict-resolution driver.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::EvidenceEmitFailed`] when `resolution` is not a
    /// closed conflict lifecycle token or when evidence append fails.
    pub async fn emit_conflict_event(
        &self,
        object_id: &ObjectId,
        conflict_summary: &str,
        resolution: &str,
    ) -> Result<String, FsError> {
        let resolution_kind =
            ConflictResolutionKind::parse_token(resolution).map_err(FsError::EvidenceEmitFailed)?;
        let payload = ConflictEventPayload {
            object_id: object_id.clone(),
            conflict_summary: conflict_summary.to_owned(),
            resolution_kind,
            occurred_at: chrono::Utc::now(),
        };
        self.emit(RecordType::ConflictEvent, None, &payload).await
    }

    /// Emit `GC_PASS` after a successful GC pass.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::EvidenceEmitFailed`] when the evidence append fails.
    pub async fn emit_gc_pass(&self, report: &GcPassReport) -> Result<String, FsError> {
        let payload = GcPassPayload {
            pass_id: report.pass_id.clone(),
            chunks_inspected: report.chunks_inspected,
            chunks_reclaimed: report.chunks_reclaimed,
            versions_inspected: report.versions_inspected,
            versions_purged: report.versions_purged,
            started_at: report.started_at,
            completed_at: report.completed_at,
        };
        self.emit(RecordType::GcPass, None, &payload).await
    }
}

/// Record a conflict event through the supplied emitter.
///
/// # Errors
///
/// Returns [`FsError::EvidenceEmitFailed`] when `resolution` is unknown or the
/// evidence append fails.
pub async fn record_conflict_event(
    emitter: &FsEvidenceEmitter,
    object_id: &ObjectId,
    conflict_summary: &str,
    resolution: &str,
) -> Result<String, FsError> {
    emitter
        .emit_conflict_event(object_id, conflict_summary, resolution)
        .await
}

fn content_hash_for_chunk_refs(chunk_refs: &[ChunkRef]) -> Result<String, FsError> {
    let ordered_chunk_ids: Vec<&str> = chunk_refs
        .iter()
        .map(|chunk_ref| chunk_ref.0.as_str())
        .collect();
    let canonical = jcs_canonicalize(&ordered_chunk_ids).map_err(|e| {
        FsError::EvidenceEmitFailed(format!("chunk-ref canonicalization failed: {e}"))
    })?;
    Ok(blake3_hash(canonical.as_bytes()))
}

fn count_len(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
}
