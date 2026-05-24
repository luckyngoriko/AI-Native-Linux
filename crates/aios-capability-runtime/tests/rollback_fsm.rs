//! T-032 integration tests — Rollback FSM + ROLLBACK_FAILED forensic
//! semantics (S10.1 §7).
//!
//! Anchors:
//! - [`aios_capability_runtime::RollbackDriver`] drives the §7.2 outcome
//!   table when an action enters [`ActionLifecycleState::Failed`].
//! - The [`aios_capability_runtime::InMemoryCapabilityRuntime::with_rollback_driver`]
//!   ctor wires the driver into the pipeline; without it, the T-031
//!   baseline is preserved.
//! - [`aios_capability_runtime::RollbackStrategy`] enumerates the §7.2
//!   adapter manifest values (`NONE`, `IDEMPOTENT_REVERSE`,
//!   `CHECKPOINT_BASED`, `EXTERNAL_REQUIRED`, plus the `UNSPECIFIED`
//!   proto3 wire-default).
//! - [`aios_capability_runtime::RollbackFailureMode`] is the closed test
//!   seam for the simulated adapter rollback outcome (production wiring
//!   is M5+).
//! - The §7.4 `ROLLBACK_FAILED` forensic path: FOREVER-retention
//!   `ROLLBACK_COMPLETED` evidence + `operator_alerts` counter
//!   increment.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::redundant_closure_for_method_calls,
    clippy::items_after_statements,
    clippy::similar_names,
    reason = "panic-on-failure is the idiomatic test signal; spec-anchor identifiers in test doc comments use SCREAMING_SNAKE_CASE per the S10.1 §7 wire form"
)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_capability_runtime::{
    adapter_manifest::AdapterActionDeclaration, apply_transition, canonical_signed_manifest_bytes,
    encode_hex_signature, fresh_context, ActionDispatchKind, ActionLifecycleState, AdapterIOMode,
    AdapterManifest, AdapterStability, CapabilityRuntime, EvidenceEmitter, InMemoryAdapterRegistry,
    InMemoryCapabilityRuntime, InMemoryEvidenceSink, RollbackDriver, RollbackFailureMode,
    RuntimeContext,
};
use aios_evidence::{EvidenceReceipt, RecordType, RetentionClass};
use aios_policy::{
    ApprovalRequirement, ApprovalScope, ApproverClass, Constraints, Decision, HydratedSubject,
    PolicyContext, PolicyDecision, PolicyError, PolicyKernel, SubjectType,
};

const TRUSTED_KEY_ID: &str = "publisher:key:t032:01";

// ---------------------------------------------------------------------------
// ScriptedKernel — deterministic ALLOW policy mock (reused from T-031).
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct ScriptedKernel {
    result: std::sync::Mutex<Result<PolicyDecision, PolicyError>>,
}

impl ScriptedKernel {
    fn allow() -> Arc<Self> {
        Arc::new(Self {
            result: std::sync::Mutex::new(Ok(make_decision(
                Decision::Allow,
                "ScopedAllow",
                Constraints::default(),
                ApprovalRequirement {
                    required: false,
                    approval_scope: ApprovalScope::ExactRequestHash,
                    ttl_seconds: 0,
                    approver_classes: vec![ApproverClass::Human],
                    require_human_co_signer: false,
                },
            ))),
        })
    }
}

#[async_trait]
impl PolicyKernel for ScriptedKernel {
    async fn evaluate_policy(
        &self,
        _envelope: &ActionEnvelope,
        _context: &PolicyContext,
    ) -> Result<PolicyDecision, PolicyError> {
        self.result.lock().expect("mutex").clone()
    }
}

fn make_decision(
    decision: Decision,
    reason: &str,
    constraints: Constraints,
    approval: ApprovalRequirement,
) -> PolicyDecision {
    use aios_action::ActionId;
    PolicyDecision {
        policy_decision_id: "poldec_t032_test_0000000000000000".to_string(),
        action_id: ActionId::new(),
        request_hash: "0".repeat(32),
        bundle_version: "polb_t032_test_v1".to_string(),
        enrichment_snapshot_id: "polb_snap_t032".to_string(),
        decision,
        reason_code: reason.to_string(),
        reason_message: format!("test {reason}"),
        constraints,
        approval,
        evidence_receipt_id: "evr_t032_test_0".to_string(),
        evaluated_at: Utc::now(),
        rules_consulted: 0,
        simulated: false,
    }
}

// ---------------------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------------------

fn test_signing_key() -> SigningKey {
    SigningKey::from_bytes(&[32u8; 32])
}

fn make_sink_emitter() -> (Arc<InMemoryEvidenceSink>, Arc<EvidenceEmitter>) {
    let sink = Arc::new(InMemoryEvidenceSink::new(test_signing_key()));
    let emitter = Arc::new(EvidenceEmitter::new(sink.clone()));
    (sink, emitter)
}

fn happy_envelope(subject: &str) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new(subject, false),
        Request::new("service.restart", serde_json::json!({"service": "nginx"})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn human_subject(canonical: &str) -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: canonical.to_string(),
        subject_type: SubjectType::Human,
        groups: vec!["operators".into()],
        capabilities: vec!["cap.service.manage".into()],
        session_class: "INTERNAL".into(),
        recovery_mode: false,
        is_ai: false,
    }
}

fn unsigned_manifest(adapter_id: &str, rollback_strategy: &str) -> AdapterManifest {
    let now = Utc::now();
    AdapterManifest {
        adapter_id: adapter_id.to_string(),
        adapter_version: "0.1.0".into(),
        vendor: "aios".into(),
        name: "systemd".into(),
        declared_stability: AdapterStability::Stable,
        io_mode: AdapterIOMode::TypedParametersOnly,
        dispatch_kind: ActionDispatchKind::SubprocessFork,
        declared_actions: vec![AdapterActionDeclaration {
            action_kind: "service.restart".into(),
            target_schema: serde_json::json!({"type": "object"}),
            response_schema: serde_json::json!({"type": "object"}),
            rollback_strategy: rollback_strategy.into(),
            timeout_seconds: 30,
            template_string: None,
            template_substitution_variables: vec![],
        }],
        declared_invariants_supported: vec!["INV-013".into()],
        default_adapter_timeout_seconds: 60,
        default_sandbox_profile_id: "service-restart-default".into(),
        adapter_signature: String::new(),
        signing_key_id: TRUSTED_KEY_ID.to_string(),
        manifest_created_at: now,
        manifest_expires_at: now + Duration::days(365),
    }
}

fn sign_manifest(manifest: &mut AdapterManifest, sk: &SigningKey) {
    let body = canonical_signed_manifest_bytes(manifest).expect("body");
    let sig = sk.sign(&body);
    manifest.adapter_signature = encode_hex_signature(&sig.to_bytes());
}

async fn registry_with_strategy(strategy: &str) -> Arc<InMemoryAdapterRegistry> {
    let sk = SigningKey::generate(&mut OsRng);
    let mut trusted = HashMap::new();
    trusted.insert(TRUSTED_KEY_ID.to_string(), sk.verifying_key());
    let registry = InMemoryAdapterRegistry::new(trusted);
    let mut manifest = unsigned_manifest("adapter:aios:systemd:0.1.0", strategy);
    sign_manifest(&mut manifest, &sk);
    registry
        .register(manifest, Utc::now())
        .await
        .expect("register");
    Arc::new(registry)
}

fn record_types_in_chain(chain: &[String], all: &[EvidenceReceipt]) -> Vec<RecordType> {
    chain
        .iter()
        .map(|rid| {
            all.iter()
                .find(|r| r.receipt_id().as_str() == rid)
                .map(|r| r.record_type())
                .expect("receipt in sink")
        })
        .collect()
}

fn receipt_for(chain_id: &str, all: &[EvidenceReceipt]) -> EvidenceReceipt {
    all.iter()
        .find(|r| r.receipt_id().as_str() == chain_id)
        .cloned()
        .expect("receipt in sink")
}

// ---------------------------------------------------------------------------
// 1. Strategy NONE — no rollback attempted; status stays FAILED.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn strategy_none_does_not_attempt_rollback_and_status_stays_failed() {
    let (sink, emitter) = make_sink_emitter();
    let registry = registry_with_strategy("NONE").await;
    let driver = Arc::new(
        RollbackDriver::new_with_defaults()
            .with_inject_verify_failure(true)
            .with_failure_mode(RollbackFailureMode::SucceedSimulated),
    );
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_adapter_registry(registry)
        .with_evidence_emitter(emitter)
        .with_rollback_driver(driver);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000010"),
        "polb_t032_test_v1",
        "code_t032",
    );
    let env = happy_envelope("human:lucky");

    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");
    assert_eq!(result.status, ActionLifecycleState::Failed);
    assert_eq!(
        result.rollback_outcome,
        Some(aios_capability_runtime::RollbackOutcome::NotAttempted)
    );
    assert_eq!(runtime.operator_alerts(), 0);

    let all = sink.receipts().await;
    let kinds = record_types_in_chain(&result.evidence_chain, &all);
    // ROLLBACK_COMPLETED is still emitted (as a NotAttempted record) so
    // the audit trail records the strategy was NONE.
    assert!(kinds.contains(&RecordType::RollbackCompleted));
}

// ---------------------------------------------------------------------------
// 2. Strategy IDEMPOTENT_REVERSE + simulated success → ROLLED_BACK.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn idempotent_reverse_succeeds_drives_rolled_back() {
    let (sink, emitter) = make_sink_emitter();
    let registry = registry_with_strategy("IDEMPOTENT_REVERSE").await;
    let driver = Arc::new(
        RollbackDriver::new_with_defaults()
            .with_inject_verify_failure(true)
            .with_failure_mode(RollbackFailureMode::SucceedSimulated),
    );
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_adapter_registry(registry)
        .with_evidence_emitter(emitter)
        .with_rollback_driver(driver);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000011"),
        "polb_t032_test_v1",
        "code_t032",
    );
    let env = happy_envelope("human:lucky");
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");
    assert_eq!(result.status, ActionLifecycleState::RolledBack);
    assert_eq!(
        result.rollback_outcome,
        Some(aios_capability_runtime::RollbackOutcome::Succeeded)
    );
    assert_eq!(runtime.operator_alerts(), 0);

    let all = sink.receipts().await;
    let kinds = record_types_in_chain(&result.evidence_chain, &all);
    assert!(kinds.contains(&RecordType::RollbackCompleted));

    // ROLLBACK_COMPLETED for a Succeeded outcome should NOT be FOREVER.
    let rb_id = result
        .evidence_chain
        .iter()
        .find(|rid| {
            all.iter().any(|r| {
                r.receipt_id().as_str() == rid.as_str()
                    && r.record_type() == RecordType::RollbackCompleted
            })
        })
        .expect("rollback receipt id");
    let receipt = receipt_for(rb_id, &all);
    assert_ne!(
        receipt.retention_class(),
        RetentionClass::Forever,
        "succeeded rollback must not be FOREVER-retention"
    );
}

// ---------------------------------------------------------------------------
// 3. Strategy CHECKPOINT_BASED + simulated success → ROLLED_BACK.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn checkpoint_based_succeeds_drives_rolled_back() {
    let (_sink, emitter) = make_sink_emitter();
    let registry = registry_with_strategy("CHECKPOINT_BASED").await;
    let driver = Arc::new(
        RollbackDriver::new_with_defaults()
            .with_inject_verify_failure(true)
            .with_failure_mode(RollbackFailureMode::SucceedSimulated),
    );
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_adapter_registry(registry)
        .with_evidence_emitter(emitter)
        .with_rollback_driver(driver);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000012"),
        "polb_t032_test_v1",
        "code_t032",
    );
    let env = happy_envelope("human:lucky");
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");
    assert_eq!(result.status, ActionLifecycleState::RolledBack);
}

// ---------------------------------------------------------------------------
// 4. Strategy EXTERNAL_REQUIRED → NotApplicable, FAILED stays, no alert.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn external_required_stays_failed_and_is_not_applicable() {
    let (_sink, emitter) = make_sink_emitter();
    let registry = registry_with_strategy("EXTERNAL_REQUIRED").await;
    let driver = Arc::new(
        RollbackDriver::new_with_defaults()
            .with_inject_verify_failure(true)
            .with_failure_mode(RollbackFailureMode::SucceedSimulated),
    );
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_adapter_registry(registry)
        .with_evidence_emitter(emitter)
        .with_rollback_driver(driver);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000013"),
        "polb_t032_test_v1",
        "code_t032",
    );
    let env = happy_envelope("human:lucky");
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");
    assert_eq!(result.status, ActionLifecycleState::Failed);
    assert_eq!(
        result.rollback_outcome,
        Some(aios_capability_runtime::RollbackOutcome::NotApplicable)
    );
    assert_eq!(runtime.operator_alerts(), 0);
}

// ---------------------------------------------------------------------------
// 5. Any strategy + simulated rollback failure → ROLLBACK_FAILED + alert + FOREVER.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rollback_failure_drives_terminal_rollback_failed_and_alerts() {
    let (sink, emitter) = make_sink_emitter();
    let registry = registry_with_strategy("IDEMPOTENT_REVERSE").await;
    let driver = Arc::new(
        RollbackDriver::new_with_defaults()
            .with_inject_verify_failure(true)
            .with_failure_mode(RollbackFailureMode::FailSimulated),
    );
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_adapter_registry(registry)
        .with_evidence_emitter(emitter)
        .with_rollback_driver(driver);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000014"),
        "polb_t032_test_v1",
        "code_t032",
    );
    let env = happy_envelope("human:lucky");
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");
    assert_eq!(result.status, ActionLifecycleState::RollbackFailed);
    assert_eq!(
        result.rollback_outcome,
        Some(aios_capability_runtime::RollbackOutcome::Failed)
    );
    assert_eq!(runtime.operator_alerts(), 1);

    let all = sink.receipts().await;
    let rb_id = result
        .evidence_chain
        .iter()
        .find(|rid| {
            all.iter().any(|r| {
                r.receipt_id().as_str() == rid.as_str()
                    && r.record_type() == RecordType::RollbackCompleted
            })
        })
        .expect("rollback receipt id");
    let receipt = receipt_for(rb_id, &all);
    assert_eq!(
        receipt.retention_class(),
        RetentionClass::Forever,
        "ROLLBACK_FAILED must emit FOREVER retention per §7.4"
    );
}

// ---------------------------------------------------------------------------
// 6. ROLLBACK_FAILED is terminal — apply_transition refuses outbound moves.
// ---------------------------------------------------------------------------

#[test]
fn rollback_failed_is_strictly_terminal() {
    let now = Utc::now();
    let action_id = aios_action::ActionId::new();
    let mut ctx = fresh_context(action_id, now);
    // Drive FAILED → ROLLBACK_FAILED through the legal §4.2 path.
    ctx.status = ActionLifecycleState::Failed;
    apply_transition(&mut ctx, ActionLifecycleState::RollbackFailed, now)
        .expect("FAILED -> ROLLBACK_FAILED is T20 and legal");
    // Any outbound transition must error.
    for target in [
        ActionLifecycleState::Succeeded,
        ActionLifecycleState::RolledBack,
        ActionLifecycleState::Failed,
        ActionLifecycleState::Created,
        ActionLifecycleState::PolicyPending,
    ] {
        assert!(
            apply_transition(&mut ctx, target, now).is_err(),
            "ROLLBACK_FAILED → {target:?} must be rejected"
        );
    }
}

// ---------------------------------------------------------------------------
// 7. ROLLED_BACK is terminal — apply_transition refuses outbound moves.
// ---------------------------------------------------------------------------

#[test]
fn rolled_back_is_strictly_terminal() {
    let now = Utc::now();
    let action_id = aios_action::ActionId::new();
    let mut ctx = fresh_context(action_id, now);
    ctx.status = ActionLifecycleState::Failed;
    apply_transition(&mut ctx, ActionLifecycleState::RolledBack, now)
        .expect("FAILED -> ROLLED_BACK is T19 and legal");
    for target in [
        ActionLifecycleState::Succeeded,
        ActionLifecycleState::RollbackFailed,
        ActionLifecycleState::Failed,
    ] {
        assert!(
            apply_transition(&mut ctx, target, now).is_err(),
            "ROLLED_BACK → {target:?} must be rejected"
        );
    }
}

// ---------------------------------------------------------------------------
// 8. Backward compat — no rollback driver → T-031 baseline (SUCCEEDED).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn no_rollback_driver_preserves_t031_succeeded_baseline() {
    let (_sink, emitter) = make_sink_emitter();
    let registry = registry_with_strategy("IDEMPOTENT_REVERSE").await;
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_adapter_registry(registry)
        .with_evidence_emitter(emitter);
    // No `.with_rollback_driver(...)` — the T-031 baseline must hold.

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000015"),
        "polb_t032_test_v1",
        "code_t032",
    );
    let env = happy_envelope("human:lucky");
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");
    assert_eq!(result.status, ActionLifecycleState::Succeeded);
    assert_eq!(result.rollback_outcome, None);
    assert_eq!(runtime.operator_alerts(), 0);
}

// ---------------------------------------------------------------------------
// 9. NOT_ATTEMPTED is the legitimate outcome for strategy NONE
//    and does NOT increment operator_alerts.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn not_attempted_does_not_increment_operator_alerts() {
    let (_sink, emitter) = make_sink_emitter();
    let registry = registry_with_strategy("NONE").await;
    let driver = Arc::new(
        RollbackDriver::new_with_defaults()
            .with_inject_verify_failure(true)
            .with_failure_mode(RollbackFailureMode::FailSimulated),
    );
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_adapter_registry(registry)
        .with_evidence_emitter(emitter)
        .with_rollback_driver(driver);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000016"),
        "polb_t032_test_v1",
        "code_t032",
    );
    let env = happy_envelope("human:lucky");
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");
    assert_eq!(result.status, ActionLifecycleState::Failed);
    assert_eq!(
        result.rollback_outcome,
        Some(aios_capability_runtime::RollbackOutcome::NotAttempted)
    );
    assert_eq!(runtime.operator_alerts(), 0);
}

// ---------------------------------------------------------------------------
// 10. classify_terminal truth table covers all four RollbackOutcome variants.
// ---------------------------------------------------------------------------

#[test]
fn classify_terminal_truth_table_is_exhaustive() {
    use aios_capability_runtime::RollbackOutcome;
    assert_eq!(
        RollbackDriver::classify_terminal(&RollbackOutcome::Succeeded),
        ActionLifecycleState::RolledBack
    );
    assert_eq!(
        RollbackDriver::classify_terminal(&RollbackOutcome::Failed),
        ActionLifecycleState::RollbackFailed
    );
    assert_eq!(
        RollbackDriver::classify_terminal(&RollbackOutcome::NotAttempted),
        ActionLifecycleState::Failed
    );
    assert_eq!(
        RollbackDriver::classify_terminal(&RollbackOutcome::NotApplicable),
        ActionLifecycleState::Failed
    );
}

// ---------------------------------------------------------------------------
// 11. End-to-end ROLLED_BACK chain integrity (Blake3 + Ed25519).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rolled_back_chain_passes_integrity_and_signature_checks() {
    let (sink, emitter) = make_sink_emitter();
    let registry = registry_with_strategy("IDEMPOTENT_REVERSE").await;
    let driver = Arc::new(
        RollbackDriver::new_with_defaults()
            .with_inject_verify_failure(true)
            .with_failure_mode(RollbackFailureMode::SucceedSimulated),
    );
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_adapter_registry(registry)
        .with_evidence_emitter(emitter)
        .with_rollback_driver(driver);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000017"),
        "polb_t032_test_v1",
        "code_t032",
    );
    let env = happy_envelope("human:lucky");
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");
    assert_eq!(result.status, ActionLifecycleState::RolledBack);

    sink.verify_integrity()
        .await
        .expect("BLAKE3 chain verifies end-to-end across rollback");

    // Every receipt's Ed25519 signature must verify.
    let key = sink.verifying_key();
    for r in sink.receipts().await {
        r.verify_signature(&key).expect("ed25519 verify");
    }
}

// ---------------------------------------------------------------------------
// 12. Rollback strategy roundtrip end-to-end — every strategy lands
//     a coherent terminal status.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_strategy_lands_a_coherent_terminal_status() {
    let cases: &[(&str, ActionLifecycleState)] = &[
        ("NONE", ActionLifecycleState::Failed),
        ("IDEMPOTENT_REVERSE", ActionLifecycleState::RolledBack),
        ("CHECKPOINT_BASED", ActionLifecycleState::RolledBack),
        ("EXTERNAL_REQUIRED", ActionLifecycleState::Failed),
    ];
    for (strategy, expected) in cases {
        let (_sink, emitter) = make_sink_emitter();
        let registry = registry_with_strategy(strategy).await;
        let driver = Arc::new(
            RollbackDriver::new_with_defaults()
                .with_inject_verify_failure(true)
                .with_failure_mode(RollbackFailureMode::SucceedSimulated),
        );
        let runtime = InMemoryCapabilityRuntime::new()
            .with_policy_kernel(ScriptedKernel::allow())
            .with_adapter_registry(registry)
            .with_evidence_emitter(emitter)
            .with_rollback_driver(driver);

        let rctx = RuntimeContext::from_subject(
            human_subject("human:lucky:01HX0000000000000000000099"),
            "polb_t032_test_v1",
            "code_t032",
        );
        let env = happy_envelope("human:lucky");
        let result = runtime
            .submit_action(&env, &rctx)
            .await
            .expect("submit_action");
        assert_eq!(
            result.status, *expected,
            "strategy {strategy} expected {expected:?}, got {:?}",
            result.status
        );
    }
}
