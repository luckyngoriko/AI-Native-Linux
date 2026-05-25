//! SGR evidence emission policy (S15.x -> S3.1).
//!
//! The current S3.1 Rust vocabulary exposes dedicated SGR variants for unit
//! lifecycle, graph convergence, and adapter registration. It does not expose
//! `DEPENDENCY_DECLARED`, so dependency-edge declarations are folded into
//! `GRAPH_EVALUATED` with a typed `DependencyDeclaredPayload`.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S15.x evidence vocabulary"
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
    AdapterRegisteredPayload, DependencyDeclaredPayload, GraphConvergedPayload, UnitFailedPayload,
    UnitRegisteredPayload, UnitStartedPayload, UnitStoppedPayload,
};
use crate::{
    AdapterDeclaration, DependencyEdge, DesiredState, GraphState, RegisteredAdapter, ServiceUnit,
    SgrError,
};

/// Constitutional default subject id for SGR evidence emissions.
pub const AIOS_SGR_SUBJECT: &str = "_system:service:service-graph-runtime";

/// Canonical subject reference for SGR evidence emission.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SgrSubjectRef(
    /// Canonical S5.1 subject id.
    pub String,
);

/// Async append-only sink for sealed, signed SGR evidence receipts.
#[async_trait]
pub trait SgrEvidenceLog: Send + Sync + Debug {
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
pub struct InMemorySgrEvidenceLog {
    chain: Mutex<ReceiptChain>,
}

impl Default for InMemorySgrEvidenceLog {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemorySgrEvidenceLog {
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
impl SgrEvidenceLog for InMemorySgrEvidenceLog {
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

/// SGR evidence emitter with helpers for S15.x lifecycle points.
#[derive(Clone)]
pub struct SgrEvidenceEmitter {
    log: Arc<dyn SgrEvidenceLog>,
    signing_key: SigningKey,
    subject: SgrSubjectRef,
}

impl Debug for SgrEvidenceEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SgrEvidenceEmitter")
            .field("log", &"<dyn SgrEvidenceLog>")
            .field("signing_key", &"<redacted>")
            .field("subject", &self.subject)
            .finish()
    }
}

impl SgrEvidenceEmitter {
    /// Construct a new SGR evidence emitter.
    #[must_use]
    pub fn new(
        log: Arc<dyn SgrEvidenceLog>,
        signing_key: SigningKey,
        subject: SgrSubjectRef,
    ) -> Self {
        Self {
            log,
            signing_key,
            subject,
        }
    }

    /// Borrow the underlying sink.
    #[must_use]
    pub fn log(&self) -> &Arc<dyn SgrEvidenceLog> {
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
    ) -> Result<String, SgrError>
    where
        P: Serialize + Sync,
    {
        let payload_value = serde_json::to_value(payload).map_err(|e| {
            SgrError::EvidenceEmitFailed(format!("payload serialization failed: {e}"))
        })?;
        let retention = aios_evidence::record::retention_class_for(record_type);
        let builder = ReceiptBuilder::new(record_type, retention, self.subject.0.clone())
            .with_payload(payload_value);
        let receipt = self
            .log
            .append_signed(builder, &self.signing_key, prev_receipt_id)
            .await
            .map_err(|e| SgrError::EvidenceEmitFailed(e.to_string()))?;
        Ok(receipt.receipt_id().as_str().to_owned())
    }

    /// Emit `UNIT_REGISTERED` after successful graph admission.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_unit_registered(
        &self,
        unit: &ServiceUnit,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, SgrError> {
        let payload = UnitRegisteredPayload {
            unit_id: unit.unit_id.clone(),
            kind: unit.manifest.unit_kind,
            name: unit.manifest.display_name.clone(),
            signing_authority: authority_with_signature_prefix(
                &unit.manifest.publisher_root_id,
                &unit.manifest.publisher_signature,
            ),
            registered_at: unit.last_transition_at,
        };
        self.emit(RecordType::UnitRegistered, &payload, prev_receipt_id)
            .await
    }

    /// Emit `UNIT_STARTED` after a unit reaches `RUNNING`.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_unit_started(
        &self,
        unit: &ServiceUnit,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, SgrError> {
        let payload = UnitStartedPayload {
            unit_id: unit.unit_id.clone(),
            started_at: unit.last_transition_at,
        };
        self.emit(RecordType::UnitStarted, &payload, prev_receipt_id)
            .await
    }

    /// Emit `UNIT_STOPPED` after a unit reaches `STOPPED`.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_unit_stopped(
        &self,
        unit: &ServiceUnit,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, SgrError> {
        let payload = UnitStoppedPayload {
            unit_id: unit.unit_id.clone(),
            stopped_at: unit.last_transition_at,
            requested_by_desired_state: unit.manifest.desired_state == DesiredState::Stopped,
        };
        self.emit(RecordType::UnitStopped, &payload, prev_receipt_id)
            .await
    }

    /// Emit `UNIT_FAILED` after a unit reaches `FAILED`.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_unit_failed(
        &self,
        unit: &ServiceUnit,
        reason: &str,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, SgrError> {
        let payload = UnitFailedPayload {
            unit_id: unit.unit_id.clone(),
            reason: reason.to_owned(),
            failed_at: unit.last_transition_at,
        };
        self.emit(RecordType::UnitFailed, &payload, prev_receipt_id)
            .await
    }

    /// Emit dependency declaration evidence.
    ///
    /// S3.1 has no `DEPENDENCY_DECLARED` variant; this folds into the closest
    /// S15.2 graph-change record, `GRAPH_EVALUATED`, with a typed payload.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_dependency_declared(
        &self,
        edge: &DependencyEdge,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, SgrError> {
        let payload = DependencyDeclaredPayload {
            from: edge.from_unit_id.clone(),
            to: edge.to_unit_id.clone(),
            kind: edge.kind,
            declared_at: Utc::now(),
        };
        self.emit(RecordType::GraphEvaluated, &payload, prev_receipt_id)
            .await
    }

    /// Emit `GRAPH_CONVERGED` after convergence is observed.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_graph_converged(
        &self,
        graph_state: GraphState,
        unit_count: u64,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, SgrError> {
        let payload = GraphConvergedPayload {
            graph_state,
            unit_count,
            converged_at: Utc::now(),
        };
        self.emit(RecordType::GraphConverged, &payload, prev_receipt_id)
            .await
    }

    /// Emit `ADAPTER_REGISTERED` after successful adapter registry admission.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::EvidenceEmitFailed`] when evidence append fails.
    pub async fn emit_adapter_registered(
        &self,
        adapter: &RegisteredAdapter,
        prev_receipt_id: Option<&str>,
    ) -> Result<String, SgrError> {
        let payload = AdapterRegisteredPayload {
            capability_id: adapter.capability.capability_id.clone(),
            registered_at: adapter.registered_at,
            signing_authority: adapter_signing_authority(adapter),
        };
        self.emit(RecordType::AdapterRegistered, &payload, prev_receipt_id)
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

fn adapter_signing_authority(adapter: &RegisteredAdapter) -> String {
    match &adapter.declaration {
        AdapterDeclaration::Manifest(manifest) => authority_with_signature_prefix(
            &manifest.signing_key_id,
            &adapter.capability.manifest_signature_ed25519,
        ),
        AdapterDeclaration::Capability(capability) => authority_with_signature_prefix(
            &capability.capability_id,
            &capability.manifest_signature_ed25519,
        ),
    }
}

fn authority_with_signature_prefix(authority: &str, signature: &[u8]) -> String {
    let prefix = hex_prefix(signature, 8);
    if prefix.is_empty() {
        return authority.to_owned();
    }
    format!("{authority}:sig:{prefix}")
}

fn hex_prefix(bytes: &[u8], max_bytes: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(max_bytes.saturating_mul(2));
    for byte in bytes.iter().take(max_bytes) {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}
