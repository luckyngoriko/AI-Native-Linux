//! [`EvidenceEmitter`] — the L3 ↔ L0 evidence emission policy (S10.1 §13
//! ↔ S3.1 §11.4).
//!
//! This module is the single shim between the capability runtime's
//! eight-step lifecycle (S10.1 §3) and the append-only Evidence Log
//! (S3.1). Every §4.2 transition the runtime drives produces one typed
//! receipt; the receipts are signed (Ed25519, S3.1 §5.2 / §11.3) and
//! BLAKE3-chained (S3.1 §5.3) inside the [`EvidenceSink`], and the receipt
//! id is appended to [`crate::ActionContext::evidence_chain`] so callers
//! can reconstruct the full per-action chain from the persisted context
//! alone.
//!
//! ## Why a trait, not a concrete log
//!
//! The Evidence Log is L0 (constitutional truth) — its append surface in
//! `aios-evidence` exposes the gRPC `EvidenceLog::Append` RPC, the
//! `InMemoryEvidenceLog` backend, and the lower-level `ReceiptBuilder` +
//! `ReceiptChain` primitives. Wiring the runtime against any single one
//! of those would over-couple L3 to the wire choice. Instead the runtime
//! talks to a small async trait, [`EvidenceSink`], that hides the
//! receipt-construction details behind one method:
//!
//! ```ignore
//! async fn append_signed(&self, builder: ReceiptBuilder)
//!     -> Result<EvidenceReceipt, EvidenceError>;
//! ```
//!
//! Production wiring (T-033 gRPC server) provides an `EvidenceSink` impl
//! that fans the call out to the L0 `EvidenceLog::Append` gRPC. Tests use
//! the in-process [`InMemoryEvidenceSink`] below, which owns a
//! `ReceiptChain` + signing key and yields the appended receipts back via
//! [`InMemoryEvidenceSink::receipts`]. T-031 ships the trait + the
//! in-memory impl; T-033 ships the gRPC-backed impl.
//!
//! ## Single-writer + chain invariants
//!
//! The sink owns the chain head. Every `append_signed` call seals the
//! supplied builder linked to the chain's current tail. This keeps the
//! BLAKE3 chain (S3.1 §5.3) intact across concurrent submissions: even
//! when two `submit_action` calls race, the sink's internal lock
//! serialises the appends so each receipt's `previous_receipt_hash`
//! reflects the actual prior receipt's `link_hash()`.
//!
//! The per-action ordering in [`crate::ActionContext::evidence_chain`] is
//! preserved by the pipeline calling the emit methods in lifecycle order
//! within one `submit_action`; per-action receipts may interleave with
//! receipts from other actions in the global chain, but the global chain
//! is the canonical projection — the §4.2 §22 acceptance criterion is
//! per-action correlation, not per-action segment isolation. Per-action
//! filtering is the §17 query surface's job (T-033 gRPC).

use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use ed25519_dalek::SigningKey;
use serde::Serialize;
use tokio::sync::Mutex;

use aios_action::ActionEnvelope;
use aios_evidence::{
    EvidenceError, EvidenceReceipt, ReceiptBuilder, ReceiptChain, RecordType, RetentionClass,
};
use aios_policy::PolicyDecision;

use crate::context::ActionContext;
use crate::dispatch::{ActionDispatchKind, QueueClass};
use crate::error::RuntimeError;
use crate::evidence_payloads::{
    ActionQueuedPayload, ActionReceivedPayload, AiInteractiveQueueDowngradePayload,
    ExecutionCompletedPayload, ExecutionStartedPayload, PolicyDecisionPayload,
    RollbackCompletedPayload, RoutingDecisionPayload, VerificationResultPayload,
};
use crate::failure::{ExecutionFailureReason, RollbackOutcome};

/// Constitutional subject id for L3 evidence emissions.
///
/// Per S3.1 §11.4 the capability runtime is the only authorised emitter for
/// the §13 record types; append attempts from any other subject are
/// hard-denied at the evidence log surface.
pub const CAPABILITY_RUNTIME_SUBJECT: &str = "_system:service:capability-runtime";

// ---------------------------------------------------------------------------
// EvidenceSink trait.
// ---------------------------------------------------------------------------

/// Async append-only sink for sealed, Ed25519-signed [`EvidenceReceipt`]s.
///
/// The sink owns the chain head and the signing key custodian (a direct
/// [`SigningKey`] today; the S5.2 `VaultSignCapability` once secrets-as-
/// capabilities lands). It is the single writer through which every
/// runtime-emitted receipt flows.
///
/// Implementations MUST:
/// - Seal the supplied [`ReceiptBuilder`] linked to the chain's current
///   tail (so `previous_receipt_hash` matches `tail.link_hash()`).
/// - Sign the sealed receipt with the sink's custodied key
///   (`seal_signed`).
/// - Append the sealed receipt to the underlying chain atomically with
///   the link computation (no torn writes).
/// - Return the sealed [`EvidenceReceipt`] so the caller can record its
///   `receipt_id()` on the per-action [`ActionContext::evidence_chain`].
#[async_trait]
pub trait EvidenceSink: Send + Sync + Debug {
    /// Seal, sign, and append a receipt. Returns the sealed
    /// [`EvidenceReceipt`] (with `signature` populated and the chain link
    /// fixed).
    ///
    /// # Errors
    ///
    /// Returns [`EvidenceError`] on subject validation failure, seal
    /// failure, chain-link mismatch, or storage / network failure.
    /// Callers map this onto [`RuntimeError::EvidenceEmitFailed`].
    async fn append_signed(
        &self,
        builder: ReceiptBuilder,
    ) -> Result<EvidenceReceipt, EvidenceError>;
}

// ---------------------------------------------------------------------------
// InMemoryEvidenceSink — test / in-process backend.
// ---------------------------------------------------------------------------

/// In-process [`EvidenceSink`] backed by a [`ReceiptChain`] guarded by a
/// `tokio::sync::Mutex`.
///
/// Used by the T-031 integration tests and by the M4 §22 golden path
/// in-process wiring before T-033's gRPC adapter lands. Production
/// deployments back the sink with the L0 `aios-evidence` gRPC service
/// via a separate `RemoteEvidenceSink` shim (T-033).
#[derive(Debug)]
pub struct InMemoryEvidenceSink {
    signing_key: Arc<SigningKey>,
    /// Single-writer chain. The async `Mutex` serialises concurrent
    /// `append_signed` calls and guarantees the chain's link invariant
    /// across racing `submit_action`s.
    chain: Mutex<ReceiptChain>,
}

impl InMemoryEvidenceSink {
    /// Construct a fresh sink with the supplied signing key.
    ///
    /// Production: the key arrives from S5.2 Vault Broker for the
    /// `_system:service:capability-runtime` subject. Tests build an
    /// ephemeral keypair from a fixed seed for determinism.
    #[must_use]
    pub fn new(signing_key: SigningKey) -> Self {
        Self {
            signing_key: Arc::new(signing_key),
            chain: Mutex::new(ReceiptChain::new()),
        }
    }

    /// Snapshot every receipt currently on the chain. Cheap clone — the
    /// chain stores `Vec<EvidenceReceipt>` so the snapshot is owned by
    /// the caller.
    pub async fn receipts(&self) -> Vec<EvidenceReceipt> {
        self.chain.lock().await.receipts().to_vec()
    }

    /// Count of receipts currently on the chain.
    pub async fn len(&self) -> usize {
        self.chain.lock().await.receipts().len()
    }

    /// `true` iff the chain has no receipts yet.
    pub async fn is_empty(&self) -> bool {
        self.chain.lock().await.receipts().is_empty()
    }

    /// Verify the chain's BLAKE3 link integrity (S3.1 §5.3 step 1).
    /// Returns `Ok(())` if every link recomputes; `Err(...)` on the first
    /// mismatch (carrying the offending index).
    ///
    /// # Errors
    ///
    /// See [`aios_evidence::chain::ReceiptChain::verify_integrity`].
    pub async fn verify_integrity(&self) -> Result<(), EvidenceError> {
        self.chain.lock().await.verify_integrity()
    }

    /// Verifying-key view derived from the configured signing key.
    /// Used by tests to verify every appended receipt's Ed25519
    /// signature.
    #[must_use]
    pub fn verifying_key(&self) -> ed25519_dalek::VerifyingKey {
        self.signing_key.verifying_key()
    }
}

#[async_trait]
impl EvidenceSink for InMemoryEvidenceSink {
    async fn append_signed(
        &self,
        builder: ReceiptBuilder,
    ) -> Result<EvidenceReceipt, EvidenceError> {
        let mut guard = self.chain.lock().await;
        let previous = guard.receipts().last().cloned();
        let receipt = builder.seal_signed(previous.as_ref(), &self.signing_key)?;
        guard.append(receipt.clone())?;
        drop(guard);
        Ok(receipt)
    }
}

// ---------------------------------------------------------------------------
// EvidenceEmitter — the emission policy.
// ---------------------------------------------------------------------------

/// The L3 evidence emission policy.
///
/// Wraps an [`EvidenceSink`] and exposes one method per pipeline transition
/// that needs an evidence record. Each method:
///
/// 1. Builds a typed payload (see [`crate::evidence_payloads`]).
/// 2. Round-trips the payload through `serde_json::to_value` to keep the
///    on-chain payload an opaque `serde_json::Value` (per S3.1 §29.6 the
///    per-`RecordType` proto payloads are queued for Wave 14+).
/// 3. Composes a [`ReceiptBuilder`] with the appropriate `RecordType` and
///    `RetentionClass` (from [`RecordType::retention_class_for`] —
///    overridable per-call only on the `_failed` paths that force
///    `FOREVER` per the spec).
/// 4. Delegates to [`EvidenceSink::append_signed`] and records the
///    returned `receipt_id` on `ActionContext::evidence_chain`.
///
/// On any [`EvidenceError`] the emitter returns
/// [`RuntimeError::EvidenceEmitFailed`] — the caller (pipeline) maps that
/// onto a `FAILED` short-circuit per S10.1 §12.6 (evidence chain
/// failure during execute → action paused, INV-014 invoked).
#[derive(Clone)]
pub struct EvidenceEmitter {
    sink: Arc<dyn EvidenceSink>,
}

impl Debug for EvidenceEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvidenceEmitter")
            .field("sink", &"<dyn EvidenceSink>")
            .finish()
    }
}

impl EvidenceEmitter {
    /// Construct a fresh emitter over the supplied [`EvidenceSink`].
    #[must_use]
    pub fn new(sink: Arc<dyn EvidenceSink>) -> Self {
        Self { sink }
    }

    /// Borrow the underlying sink — used by tests to inspect appended
    /// receipts directly.
    #[must_use]
    pub fn sink(&self) -> &Arc<dyn EvidenceSink> {
        &self.sink
    }

    /// Internal — serialise a typed payload to JSON, build the receipt,
    /// append via the sink, and record the receipt id on the context.
    async fn emit<P: Serialize + Sync>(
        &self,
        record_type: RecordType,
        _envelope: &ActionEnvelope,
        ctx: &mut ActionContext,
        payload: &P,
    ) -> Result<String, RuntimeError> {
        let payload_value = serde_json::to_value(payload).map_err(|e| {
            RuntimeError::EvidenceEmitFailed(format!("payload serialization failed: {e}"))
        })?;
        let retention = aios_evidence::record::retention_class_for(record_type);
        let builder = ReceiptBuilder::new(record_type, retention, CAPABILITY_RUNTIME_SUBJECT)
            .with_action_id(ctx.action_id.clone())
            .with_payload(payload_value);
        let receipt = self
            .sink
            .append_signed(builder)
            .await
            .map_err(|e| RuntimeError::EvidenceEmitFailed(e.to_string()))?;
        let receipt_id = receipt.receipt_id().as_str().to_string();
        ctx.evidence_chain.push(receipt_id.clone());
        Ok(receipt_id)
    }

    /// Internal — like [`Self::emit`] but with an explicit retention
    /// override (used by the `_failed` paths that pin `FOREVER`).
    async fn emit_with_retention<P: Serialize + Sync>(
        &self,
        record_type: RecordType,
        retention: RetentionClass,
        _envelope: &ActionEnvelope,
        ctx: &mut ActionContext,
        payload: &P,
    ) -> Result<String, RuntimeError> {
        let payload_value = serde_json::to_value(payload).map_err(|e| {
            RuntimeError::EvidenceEmitFailed(format!("payload serialization failed: {e}"))
        })?;
        let builder = ReceiptBuilder::new(record_type, retention, CAPABILITY_RUNTIME_SUBJECT)
            .with_action_id(ctx.action_id.clone())
            .with_payload(payload_value);
        let receipt = self
            .sink
            .append_signed(builder)
            .await
            .map_err(|e| RuntimeError::EvidenceEmitFailed(e.to_string()))?;
        let receipt_id = receipt.receipt_id().as_str().to_string();
        ctx.evidence_chain.push(receipt_id.clone());
        Ok(receipt_id)
    }

    /// Emit `ACTION_RECEIVED` (S3.1 §4 ID 1) after step 1
    /// (`ValidateAction`) succeeds and drives `CREATED → POLICY_PENDING`
    /// (T2).
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::EvidenceEmitFailed`] on sink failure.
    pub async fn emit_action_received(
        &self,
        envelope: &ActionEnvelope,
        ctx: &mut ActionContext,
    ) -> Result<String, RuntimeError> {
        let payload = ActionReceivedPayload {
            action_kind: envelope.request.action.clone(),
            subject_canonical_id: envelope.identity.subject_canonical_id.clone(),
            is_ai: envelope.identity.is_ai,
            received_at: ctx.created_at,
            lifecycle_state_after: ctx.status,
        };
        self.emit(RecordType::ActionReceived, envelope, ctx, &payload)
            .await
    }

    /// Emit `POLICY_DECISION` (S3.1 §4 ID 4) after step 2
    /// (`EvaluatePolicyForAction`) completes.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::EvidenceEmitFailed`] on sink failure.
    pub async fn emit_policy_decision(
        &self,
        envelope: &ActionEnvelope,
        ctx: &mut ActionContext,
        policy_decision: &PolicyDecision,
    ) -> Result<String, RuntimeError> {
        let decision_token = format!("{:?}", policy_decision.decision).to_uppercase();
        let payload = PolicyDecisionPayload {
            policy_decision_id: policy_decision.policy_decision_id.clone(),
            decision: decision_token,
            reason_code: policy_decision.reason_code.clone(),
            bundle_version: policy_decision.bundle_version.clone(),
            lifecycle_state_after: ctx.status,
        };
        self.emit(RecordType::PolicyDecision, envelope, ctx, &payload)
            .await
    }

    /// Emit `ROUTING_DECISION` (S3.1 §4 ID 3) after step 5 selects an
    /// adapter and the dispatcher picks the §3.2 dispatch kind.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::EvidenceEmitFailed`] on sink failure.
    pub async fn emit_routing_decision(
        &self,
        envelope: &ActionEnvelope,
        ctx: &mut ActionContext,
        adapter_id: &str,
        dispatch_kind: ActionDispatchKind,
    ) -> Result<String, RuntimeError> {
        let payload = RoutingDecisionPayload {
            adapter_id: adapter_id.to_string(),
            action_kind: envelope.request.action.clone(),
            dispatch_kind,
        };
        self.emit(RecordType::RoutingDecision, envelope, ctx, &payload)
            .await
    }

    /// Emit the action-queued marker after step 4 (`step_queue`) enrols
    /// the action in its [`QueueClass`] bucket (T12 — APPROVED → QUEUED).
    ///
    /// Per the brief STOP-condition and the S3.1 vocabulary (which does
    /// not include `ACTION_QUEUED` at any wire ID), this is emitted
    /// under [`RecordType::ActionDispatched`] (S3.1 §4 ID 116) with the
    /// `dispatched: false` field marking the queue-enrolment phase. The
    /// real dispatch is recorded via `EXECUTION_STARTED` later.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::EvidenceEmitFailed`] on sink failure.
    pub async fn emit_action_queued(
        &self,
        envelope: &ActionEnvelope,
        ctx: &mut ActionContext,
        queue_class: QueueClass,
    ) -> Result<String, RuntimeError> {
        let payload = ActionQueuedPayload {
            queue_class,
            dispatched: false,
            subject_canonical_id: envelope.identity.subject_canonical_id.clone(),
        };
        self.emit(RecordType::ActionDispatched, envelope, ctx, &payload)
            .await
    }

    /// Emit `EXECUTION_STARTED` (S3.1 §4 ID 8) at T13 — `QUEUED →
    /// EXECUTING`, i.e. the moment the dispatcher hands the envelope to
    /// the adapter handle.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::EvidenceEmitFailed`] on sink failure.
    pub async fn emit_execution_started(
        &self,
        envelope: &ActionEnvelope,
        ctx: &mut ActionContext,
    ) -> Result<String, RuntimeError> {
        let payload = ExecutionStartedPayload {
            dispatch_kind: ctx.dispatch_kind,
            queue_class: ctx.queue_class,
        };
        self.emit(RecordType::ExecutionStarted, envelope, ctx, &payload)
            .await
    }

    /// Emit `EXECUTION_COMPLETED` (S3.1 §4 ID 9) after the adapter
    /// returns (success or failure). `outcome` is the closed
    /// `AdapterResult` token from §6.3.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::EvidenceEmitFailed`] on sink failure.
    pub async fn emit_execution_completed(
        &self,
        envelope: &ActionEnvelope,
        ctx: &mut ActionContext,
        outcome: &str,
    ) -> Result<String, RuntimeError> {
        let payload = ExecutionCompletedPayload {
            outcome: outcome.to_string(),
            lifecycle_state_after: ctx.status,
        };
        self.emit(RecordType::ExecutionCompleted, envelope, ctx, &payload)
            .await
    }

    /// Emit `VERIFICATION_RESULT` (S3.1 §4 ID 10) after the verification
    /// engine runs. `passed` drives the §4.2 T17 / T18 branch.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::EvidenceEmitFailed`] on sink failure.
    pub async fn emit_verification_result(
        &self,
        envelope: &ActionEnvelope,
        ctx: &mut ActionContext,
        passed: bool,
    ) -> Result<String, RuntimeError> {
        let payload = VerificationResultPayload {
            passed,
            lifecycle_state_after: ctx.status,
        };
        self.emit(RecordType::VerificationResult, envelope, ctx, &payload)
            .await
    }

    /// Emit `ROLLBACK_COMPLETED` (S3.1 §4 ID 11) at the rollback FSM's
    /// terminal state.
    ///
    /// The `ROLLBACK_FAILED` outcome is pinned to `FOREVER` retention
    /// per S10.1 §7.4 / §13 — operator alert + adapter degradation
    /// follow but are out of T-031 scope.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::EvidenceEmitFailed`] on sink failure.
    pub async fn emit_rollback_completed(
        &self,
        envelope: &ActionEnvelope,
        ctx: &mut ActionContext,
        outcome: RollbackOutcome,
        triggering_reason: Option<ExecutionFailureReason>,
    ) -> Result<String, RuntimeError> {
        let payload = RollbackCompletedPayload {
            outcome,
            triggering_reason,
            lifecycle_state_after: ctx.status,
        };
        if matches!(outcome, RollbackOutcome::Failed) {
            self.emit_with_retention(
                RecordType::RollbackCompleted,
                RetentionClass::Forever,
                envelope,
                ctx,
                &payload,
            )
            .await
        } else {
            self.emit(RecordType::RollbackCompleted, envelope, ctx, &payload)
                .await
        }
    }

    /// Emit `AI_INTERACTIVE_QUEUE_DOWNGRADE` (S3.1 §25.2 ID 129) when an
    /// AI subject's `INTERACTIVE` submission is silently downgraded to
    /// `AGENT_PROPOSAL` per §11.4.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::EvidenceEmitFailed`] on sink failure.
    pub async fn emit_ai_interactive_queue_downgrade(
        &self,
        envelope: &ActionEnvelope,
        ctx: &mut ActionContext,
    ) -> Result<String, RuntimeError> {
        let payload = AiInteractiveQueueDowngradePayload {
            subject_canonical_id: envelope.identity.subject_canonical_id.clone(),
            original_queue_class: QueueClass::Interactive,
            effective_queue_class: QueueClass::AgentProposal,
        };
        self.emit(
            RecordType::AiInteractiveQueueDowngrade,
            envelope,
            ctx,
            &payload,
        )
        .await
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn test_signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    #[tokio::test]
    async fn in_memory_sink_appends_and_signs_a_single_receipt() {
        let sink = InMemoryEvidenceSink::new(test_signing_key());
        let builder = ReceiptBuilder::new(
            RecordType::ActionReceived,
            RetentionClass::Standard24M,
            CAPABILITY_RUNTIME_SUBJECT,
        )
        .with_payload(serde_json::json!({"k": "v"}));
        let r = sink.append_signed(builder).await.expect("append");
        assert!(r.is_signed());
        r.verify_signature(&sink.verifying_key()).expect("verify");
        assert_eq!(sink.len().await, 1);
    }

    #[tokio::test]
    async fn in_memory_sink_chains_two_receipts() {
        let sink = InMemoryEvidenceSink::new(test_signing_key());
        let b1 = ReceiptBuilder::new(
            RecordType::ActionReceived,
            RetentionClass::Standard24M,
            CAPABILITY_RUNTIME_SUBJECT,
        )
        .with_payload(serde_json::json!({"k": 1}));
        let r1 = sink.append_signed(b1).await.expect("r1");

        let b2 = ReceiptBuilder::new(
            RecordType::PolicyDecision,
            RetentionClass::Standard24M,
            CAPABILITY_RUNTIME_SUBJECT,
        )
        .with_payload(serde_json::json!({"k": 2}));
        let r2 = sink.append_signed(b2).await.expect("r2");

        // r2 must link back to r1.
        let prev = r2.previous_receipt_hash().expect("prev present");
        assert_eq!(prev, r1.link_hash().expect("r1 link"));
        sink.verify_integrity().await.expect("chain ok");
    }

    #[tokio::test]
    async fn in_memory_sink_is_empty_before_any_append() {
        let sink = InMemoryEvidenceSink::new(test_signing_key());
        assert!(sink.is_empty().await);
        assert_eq!(sink.len().await, 0);
    }
}
