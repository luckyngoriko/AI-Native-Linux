//! T-031 integration tests — Evidence Log ⇄ Capability Runtime (S10.1 ↔ S3.1).
//!
//! Anchors:
//! - [`aios_capability_runtime::InMemoryCapabilityRuntime::with_evidence_emitter`]
//!   wires an [`aios_capability_runtime::EvidenceEmitter`] backed by an
//!   [`aios_capability_runtime::InMemoryEvidenceSink`]; every §4.2 transition
//!   appends one Ed25519-signed receipt to the sink and the receipt id is
//!   recorded on [`aios_capability_runtime::ActionContext::evidence_chain`].
//! - Happy path: ACTION_RECEIVED → POLICY_DECISION → ACTION_DISPATCHED(queue)
//!   → ROUTING_DECISION → EXECUTION_STARTED → EXECUTION_COMPLETED →
//!   VERIFICATION_RESULT, terminating in SUCCEEDED.
//! - DENY path: ACTION_RECEIVED → POLICY_DECISION(DENY), terminal at
//!   POLICY_DENIED, no execute/verify emissions.
//! - REQUIRE_APPROVAL path: ACTION_RECEIVED → POLICY_DECISION(REQUIRE_APPROVAL),
//!   pause at APPROVAL_PENDING, no execute emission.
//! - AI subject + INTERACTIVE downgrade: chain contains
//!   AI_INTERACTIVE_QUEUE_DOWNGRADE.
//! - BLAKE3 chain integrity: every receipt's previous_receipt_hash matches
//!   the prior receipt's link_hash().
//! - Backward compatibility: no emitter configured → evidence_chain empty.
//! - Ed25519 signatures verify on every emitted receipt.
//! - Concurrent submissions produce coherent per-action chains.
//! - ROUTING_DECISION payload includes adapter_id + dispatch_kind.
//! - POLICY_DECISION payload includes policy_decision_id from aios-policy.
//! - Determinism: same envelope + decision → byte-identical payload.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::redundant_closure_for_method_calls,
    clippy::items_after_statements,
    reason = "panic-on-failure is the idiomatic test signal; spec-anchor identifiers in test doc comments use SCREAMING_SNAKE_CASE per the S3.1 RecordType wire form"
)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_capability_runtime::{
    adapter_manifest::AdapterActionDeclaration, canonical_signed_manifest_bytes,
    encode_hex_signature, ActionDispatchKind, ActionLifecycleState, AdapterIOMode, AdapterManifest,
    AdapterStability, CapabilityRuntime, EvidenceEmitter, EvidenceSink, ExecutionFailureReason,
    InMemoryAdapterRegistry, InMemoryCapabilityRuntime, InMemoryEvidenceSink, RuntimeContext,
    RuntimeError, CAPABILITY_RUNTIME_SUBJECT,
};
use aios_evidence::{EvidenceError, EvidenceReceipt, ReceiptBuilder, RecordType, RetentionClass};
use aios_policy::{
    ApprovalRequirement, ApprovalScope, ApproverClass, Constraints, Decision, HydratedSubject,
    PolicyContext, PolicyDecision, PolicyError, PolicyKernel, SubjectType,
};

const TRUSTED_KEY_ID: &str = "publisher:key:test:01";

// ---------------------------------------------------------------------------
// ScriptedKernel — deterministic policy mock.
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

    fn deny() -> Arc<Self> {
        Arc::new(Self {
            result: std::sync::Mutex::new(Ok(make_decision(
                Decision::Deny,
                "DefaultDeny",
                Constraints::default(),
                ApprovalRequirement::default(),
            ))),
        })
    }

    fn require_approval() -> Arc<Self> {
        Arc::new(Self {
            result: std::sync::Mutex::new(Ok(make_decision(
                Decision::RequireApproval,
                "RequireApproval",
                Constraints::default(),
                ApprovalRequirement {
                    required: true,
                    approval_scope: ApprovalScope::ExactRequestHash,
                    ttl_seconds: 300,
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
        policy_decision_id: "poldec_t031_test_0000000000000000".to_string(),
        action_id: ActionId::new(),
        request_hash: "0".repeat(32),
        bundle_version: "polb_t031_test_v1".to_string(),
        enrichment_snapshot_id: "polb_snap_t031".to_string(),
        decision,
        reason_code: reason.to_string(),
        reason_message: format!("test {reason}"),
        constraints,
        approval,
        evidence_receipt_id: "evr_t031_test_0".to_string(),
        evaluated_at: Utc::now(),
        rules_consulted: 0,
        simulated: false,
    }
}

// ---------------------------------------------------------------------------
// Failing sink — used for the EvidenceEmitFailed propagation test.
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct FailingSink;

#[async_trait]
impl EvidenceSink for FailingSink {
    async fn append_signed(
        &self,
        _builder: ReceiptBuilder,
    ) -> Result<EvidenceReceipt, EvidenceError> {
        Err(EvidenceError::EncodingFailed("forced sink failure".into()))
    }
}

// ---------------------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------------------

fn test_signing_key() -> SigningKey {
    SigningKey::from_bytes(&[31u8; 32])
}

fn make_sink_emitter() -> (Arc<InMemoryEvidenceSink>, Arc<EvidenceEmitter>) {
    let sink = Arc::new(InMemoryEvidenceSink::new(test_signing_key()));
    let emitter = Arc::new(EvidenceEmitter::new(sink.clone()));
    (sink, emitter)
}

fn happy_envelope(subject: &str, is_ai: bool) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new(subject, is_ai),
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

fn ai_subject(canonical: &str) -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: canonical.to_string(),
        subject_type: SubjectType::Agent,
        groups: Vec::new(),
        capabilities: Vec::new(),
        session_class: "INTERNAL".into(),
        recovery_mode: false,
        is_ai: true,
    }
}

fn unsigned_manifest(adapter_id: &str) -> AdapterManifest {
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
            rollback_strategy: "IDEMPOTENT_REAPPLY".into(),
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

async fn registry_with_systemd() -> (SigningKey, Arc<InMemoryAdapterRegistry>) {
    let sk = SigningKey::generate(&mut OsRng);
    let mut trusted = HashMap::new();
    trusted.insert(TRUSTED_KEY_ID.to_string(), sk.verifying_key());
    let registry = InMemoryAdapterRegistry::new(trusted);
    let mut manifest = unsigned_manifest("adapter:aios:systemd:0.1.0");
    sign_manifest(&mut manifest, &sk);
    registry
        .register(manifest, Utc::now())
        .await
        .expect("register systemd");
    (sk, Arc::new(registry))
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

// ---------------------------------------------------------------------------
// 1. Happy path — full evidence chain reaches SUCCEEDED.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn happy_path_full_chain_reaches_succeeded_with_seven_receipts() {
    let (sink, emitter) = make_sink_emitter();
    let (_sk, registry) = registry_with_systemd().await;
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_adapter_registry(registry)
        .with_evidence_emitter(emitter);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000000"),
        "polb_t031_test_v1",
        "code_t031",
    );
    let env = happy_envelope("human:lucky", false);

    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");
    assert_eq!(result.status, ActionLifecycleState::Succeeded);

    // Per-action chain: ACTION_RECEIVED → POLICY_DECISION →
    // ACTION_DISPATCHED(queued) → ROUTING_DECISION → EXECUTION_STARTED →
    // EXECUTION_COMPLETED → VERIFICATION_RESULT. 7 receipts.
    let all = sink.receipts().await;
    let chain = result.evidence_chain.clone();
    assert_eq!(chain.len(), 7, "per-action chain length, got {chain:?}");

    let kinds = record_types_in_chain(&chain, &all);
    assert_eq!(
        kinds,
        vec![
            RecordType::ActionReceived,
            RecordType::PolicyDecision,
            RecordType::ActionDispatched,
            RecordType::RoutingDecision,
            RecordType::ExecutionStarted,
            RecordType::ExecutionCompleted,
            RecordType::VerificationResult,
        ]
    );
}

// ---------------------------------------------------------------------------
// 2. DENY path — chain stops after POLICY_DECISION.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deny_path_emits_only_action_received_and_policy_decision() {
    let (sink, emitter) = make_sink_emitter();
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::deny())
        .with_evidence_emitter(emitter);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000001"),
        "polb_t031_test_v1",
        "code_t031",
    );
    let env = happy_envelope("human:lucky", false);

    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");
    assert_eq!(result.status, ActionLifecycleState::PolicyDenied);

    let all = sink.receipts().await;
    let chain = result.evidence_chain;
    assert_eq!(chain.len(), 2, "deny path chain length");
    let kinds = record_types_in_chain(&chain, &all);
    assert_eq!(
        kinds,
        vec![RecordType::ActionReceived, RecordType::PolicyDecision]
    );
}

// ---------------------------------------------------------------------------
// 3. REQUIRE_APPROVAL path — short-circuits at APPROVAL_PENDING.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn require_approval_path_emits_only_received_and_policy_decision() {
    let (sink, emitter) = make_sink_emitter();
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::require_approval())
        .with_evidence_emitter(emitter);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000002"),
        "polb_t031_test_v1",
        "code_t031",
    );
    let env = happy_envelope("human:lucky", false);
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");
    assert_eq!(result.status, ActionLifecycleState::ApprovalPending);

    let all = sink.receipts().await;
    let chain = result.evidence_chain;
    assert_eq!(chain.len(), 2);
    let kinds = record_types_in_chain(&chain, &all);
    assert_eq!(
        kinds,
        vec![RecordType::ActionReceived, RecordType::PolicyDecision]
    );
}

// ---------------------------------------------------------------------------
// 4. AI-INTERACTIVE downgrade — AI_INTERACTIVE_QUEUE_DOWNGRADE present.
//    Note: the dispatcher's select_queue_class already returns AGENT_PROPOSAL
//    for is_ai=true subjects (§11.4 downgrade is folded into the selection).
//    To trigger the marker we drive an AI envelope with a pre-set INTERACTIVE
//    queue_class on the context — but the runtime always seeds INTERACTIVE
//    on fresh_context. The marker fires iff (is_ai && ctx.queue_class ==
//    Interactive) at queue-step time. Because step_queue_with_engine_and_emit
//    reads the pre-enrolment queue_class (which is the fresh_context default
//    INTERACTIVE) before delegating to step_queue_with_engine, an AI submission
//    *does* fire the marker. Validate that.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ai_subject_emits_interactive_queue_downgrade_marker() {
    let (sink, emitter) = make_sink_emitter();
    let (_sk, registry) = registry_with_systemd().await;

    use aios_capability_runtime::DispatchQueue;
    let queue = Arc::new(DispatchQueue::new_with_defaults());

    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_adapter_registry(registry)
        .with_dispatch_queue(queue)
        .with_evidence_emitter(emitter);

    let rctx = RuntimeContext::from_subject(
        ai_subject("ai:agent-1:01HX0000000000000000000003"),
        "polb_t031_test_v1",
        "code_t031",
    );
    let env = happy_envelope("ai:agent-1", true);

    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");
    assert_eq!(result.status, ActionLifecycleState::Succeeded);

    let all = sink.receipts().await;
    let chain = result.evidence_chain;
    let kinds = record_types_in_chain(&chain, &all);
    assert!(
        kinds.contains(&RecordType::AiInteractiveQueueDowngrade),
        "expected AiInteractiveQueueDowngrade in {kinds:?}"
    );
}

// ---------------------------------------------------------------------------
// 5. BLAKE3 chain integrity — every receipt links to the prior one's link_hash.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn blake3_chain_integrity_holds_across_emissions() {
    let (sink, emitter) = make_sink_emitter();
    let (_sk, registry) = registry_with_systemd().await;
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_adapter_registry(registry)
        .with_evidence_emitter(emitter);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000004"),
        "polb_t031_test_v1",
        "code_t031",
    );
    let env = happy_envelope("human:lucky", false);
    runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");

    sink.verify_integrity()
        .await
        .expect("BLAKE3 chain verifies end-to-end");
}

// ---------------------------------------------------------------------------
// 6. Ed25519 signatures verify on every emitted receipt.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ed25519_signatures_verify_on_every_emitted_receipt() {
    let (sink, emitter) = make_sink_emitter();
    let (_sk, registry) = registry_with_systemd().await;
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_adapter_registry(registry)
        .with_evidence_emitter(emitter);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000005"),
        "polb_t031_test_v1",
        "code_t031",
    );
    let env = happy_envelope("human:lucky", false);
    runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");

    let vk = sink.verifying_key();
    let all = sink.receipts().await;
    assert!(!all.is_empty());
    for r in &all {
        assert!(r.is_signed(), "every receipt must be signed");
        r.verify_signature(&vk).expect("signature verifies");
    }
}

// ---------------------------------------------------------------------------
// 7. Backward compat — no emitter configured → evidence_chain empty.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn backward_compat_no_emitter_keeps_chain_empty() {
    let runtime = InMemoryCapabilityRuntime::new().with_policy_kernel(ScriptedKernel::allow());
    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000006"),
        "polb_t031_test_v1",
        "code_t031",
    );
    let env = happy_envelope("human:lucky", false);
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");
    assert_eq!(result.status, ActionLifecycleState::Succeeded);
    assert!(
        result.evidence_chain.is_empty(),
        "no emitter → no evidence appended"
    );
}

// ---------------------------------------------------------------------------
// 8. EvidenceEmitFailed propagates from a failing sink and short-circuits.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn evidence_emit_failed_propagates_from_failing_sink() {
    let sink: Arc<dyn EvidenceSink> = Arc::new(FailingSink);
    let emitter = Arc::new(EvidenceEmitter::new(sink));
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_evidence_emitter(emitter);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX0000000000000000000007"),
        "polb_t031_test_v1",
        "code_t031",
    );
    let env = happy_envelope("human:lucky", false);

    let err = runtime
        .submit_action(&env, &rctx)
        .await
        .expect_err("failing sink must propagate");
    match err {
        RuntimeError::EvidenceEmitFailed(_) => {}
        other => panic!("expected EvidenceEmitFailed, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 9. Concurrent submissions — per-action chains do not cross-contaminate.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_submissions_produce_coherent_per_action_chains() {
    let (sink, emitter) = make_sink_emitter();
    let (_sk, registry) = registry_with_systemd().await;
    let runtime = Arc::new(
        InMemoryCapabilityRuntime::new()
            .with_policy_kernel(ScriptedKernel::allow())
            .with_adapter_registry(registry)
            .with_evidence_emitter(emitter),
    );

    let mut handles = Vec::new();
    for i in 0..6 {
        let runtime = runtime.clone();
        let subject = format!("human:lucky-{i}");
        handles.push(tokio::spawn(async move {
            let rctx = RuntimeContext::from_subject(
                human_subject(&format!("{subject}:01HX0000000000000000000099")),
                "polb_t031_test_v1",
                "code_t031",
            );
            let env = happy_envelope(&subject, false);
            runtime
                .submit_action(&env, &rctx)
                .await
                .expect("submit_action")
        }));
    }

    let mut contexts = Vec::new();
    for h in handles {
        contexts.push(h.await.expect("join"));
    }

    // Every action reaches SUCCEEDED.
    for c in &contexts {
        assert_eq!(c.status, ActionLifecycleState::Succeeded);
        assert_eq!(c.evidence_chain.len(), 7, "per-action chain length");
    }

    // Action ids are distinct; per-action chains do not share receipt ids.
    let mut all_ids = std::collections::HashSet::new();
    for c in &contexts {
        for rid in &c.evidence_chain {
            assert!(all_ids.insert(rid.clone()), "duplicate receipt id");
        }
    }

    // Global chain verifies.
    sink.verify_integrity().await.expect("global chain ok");
}

// ---------------------------------------------------------------------------
// 10. ROUTING_DECISION payload carries adapter_id + dispatch_kind.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn routing_decision_payload_carries_adapter_id_and_dispatch_kind() {
    let (sink, emitter) = make_sink_emitter();
    let (_sk, registry) = registry_with_systemd().await;
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_adapter_registry(registry)
        .with_evidence_emitter(emitter);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX000000000000000000000A"),
        "polb_t031_test_v1",
        "code_t031",
    );
    let env = happy_envelope("human:lucky", false);
    runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");

    let all = sink.receipts().await;
    let routing = all
        .iter()
        .find(|r| r.record_type() == RecordType::RoutingDecision)
        .expect("routing decision present");
    let payload = routing.payload();
    let obj = payload.as_object().expect("routing payload is object");
    assert!(
        obj.contains_key("adapter_id"),
        "routing payload missing adapter_id: {obj:?}"
    );
    assert!(
        obj.contains_key("dispatch_kind"),
        "routing payload missing dispatch_kind: {obj:?}"
    );
    assert!(
        obj.contains_key("action_kind"),
        "routing payload missing action_kind: {obj:?}"
    );
}

// ---------------------------------------------------------------------------
// 11. POLICY_DECISION payload carries policy_decision_id.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn policy_decision_payload_carries_policy_decision_id_from_kernel() {
    let (sink, emitter) = make_sink_emitter();
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_evidence_emitter(emitter);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX000000000000000000000B"),
        "polb_t031_test_v1",
        "code_t031",
    );
    let env = happy_envelope("human:lucky", false);
    runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");

    let all = sink.receipts().await;
    let policy = all
        .iter()
        .find(|r| r.record_type() == RecordType::PolicyDecision)
        .expect("policy decision present");
    let obj = policy.payload().as_object().expect("policy payload object");
    assert_eq!(
        obj.get("policy_decision_id").and_then(|v| v.as_str()),
        Some("poldec_t031_test_0000000000000000")
    );
    assert_eq!(obj.get("decision").and_then(|v| v.as_str()), Some("ALLOW"));
    assert_eq!(
        obj.get("bundle_version").and_then(|v| v.as_str()),
        Some("polb_t031_test_v1")
    );
}

// ---------------------------------------------------------------------------
// 12. Determinism — same envelope + same decision → byte-identical payloads
//      (modulo timestamps + receipt ids).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn determinism_payloads_match_across_two_submissions_modulo_ids() {
    async fn submit_once() -> Vec<serde_json::Value> {
        let (sink, emitter) = make_sink_emitter();
        let (_sk, registry) = registry_with_systemd().await;
        let runtime = InMemoryCapabilityRuntime::new()
            .with_policy_kernel(ScriptedKernel::allow())
            .with_adapter_registry(registry)
            .with_evidence_emitter(emitter);
        let rctx = RuntimeContext::from_subject(
            human_subject("human:lucky:01HX000000000000000000000C"),
            "polb_t031_test_v1",
            "code_t031",
        );
        let env = happy_envelope("human:lucky", false);
        let result = runtime
            .submit_action(&env, &rctx)
            .await
            .expect("submit_action");
        let all = sink.receipts().await;
        result
            .evidence_chain
            .iter()
            .map(|rid| {
                all.iter()
                    .find(|r| r.receipt_id().as_str() == rid)
                    .expect("receipt")
                    .payload()
                    .clone()
            })
            .collect()
    }

    let a = submit_once().await;
    let b = submit_once().await;
    assert_eq!(a.len(), b.len());
    // For each payload, the fields that are deterministic-per-envelope must
    // match. We compare a stable subset: record-shape keys minus
    // received_at (wall-clock) and lifecycle_state_after (always equal but
    // included to anchor the structural shape).
    for (i, (pa, pb)) in a.iter().zip(b.iter()).enumerate() {
        // Sanity: both are objects.
        let oa = pa.as_object().expect("a obj");
        let ob = pb.as_object().expect("b obj");
        let stable: Vec<&str> = oa
            .keys()
            .filter(|k| k.as_str() != "received_at" && k.as_str() != "lifecycle_state_after")
            .map(String::as_str)
            .collect();
        for key in stable {
            assert_eq!(oa.get(key), ob.get(key), "drift in payload[{i}][{key}]");
        }
    }
}

// ---------------------------------------------------------------------------
// 13. Emitted receipts subject canonical id matches the constitutional
//      capability-runtime subject (S3.1 §11.4).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_emitted_receipt_uses_capability_runtime_subject() {
    let (sink, emitter) = make_sink_emitter();
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::deny())
        .with_evidence_emitter(emitter);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX000000000000000000000D"),
        "polb_t031_test_v1",
        "code_t031",
    );
    let env = happy_envelope("human:lucky", false);
    runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");

    let all = sink.receipts().await;
    assert!(!all.is_empty());
    for r in &all {
        assert_eq!(
            r.subject_canonical_id(),
            CAPABILITY_RUNTIME_SUBJECT,
            "every receipt MUST emit under the L3 runtime subject (S3.1 §11.4)"
        );
    }
}

// ---------------------------------------------------------------------------
// 14. Validation failure (empty action kind) — the runtime short-circuits
//      to FAILED with EnvelopeValidationFailed and emits NO evidence because
//      ACTION_RECEIVED fires only on validation success (T2).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn envelope_validation_failure_emits_no_evidence() {
    let (sink, emitter) = make_sink_emitter();
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_evidence_emitter(emitter);

    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX000000000000000000000E"),
        "polb_t031_test_v1",
        "code_t031",
    );
    // Empty action kind — step_validate FAILs.
    let env = ActionEnvelope::new(
        Identity::new("human:lucky", false),
        Request::new("", serde_json::json!({})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    );
    let result = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");

    assert_eq!(result.status, ActionLifecycleState::Failed);
    assert_eq!(
        result.error,
        Some(ExecutionFailureReason::EnvelopeValidationFailed)
    );
    assert!(
        sink.is_empty().await,
        "no receipts emitted on pre-validation failure"
    );
    assert!(result.evidence_chain.is_empty());
}

// ---------------------------------------------------------------------------
// 15. EXECUTION_COMPLETED / VERIFICATION_RESULT payload anchor.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execution_completed_and_verification_result_payload_shapes() {
    let (sink, emitter) = make_sink_emitter();
    let (_sk, registry) = registry_with_systemd().await;
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(ScriptedKernel::allow())
        .with_adapter_registry(registry)
        .with_evidence_emitter(emitter);
    let rctx = RuntimeContext::from_subject(
        human_subject("human:lucky:01HX000000000000000000000F"),
        "polb_t031_test_v1",
        "code_t031",
    );
    let env = happy_envelope("human:lucky", false);
    runtime
        .submit_action(&env, &rctx)
        .await
        .expect("submit_action");

    let all = sink.receipts().await;
    let exec = all
        .iter()
        .find(|r| r.record_type() == RecordType::ExecutionCompleted)
        .expect("execution_completed present");
    let exec_obj = exec.payload().as_object().expect("exec obj");
    assert_eq!(
        exec_obj.get("outcome").and_then(|v| v.as_str()),
        Some("ADAPTER_OK")
    );

    let verify = all
        .iter()
        .find(|r| r.record_type() == RecordType::VerificationResult)
        .expect("verification_result present");
    let verify_obj = verify.payload().as_object().expect("verify obj");
    assert_eq!(
        verify_obj.get("passed").and_then(|v| v.as_bool()),
        Some(true)
    );

    // VerificationResult retention is STANDARD_24M per S3.1 default.
    assert_eq!(verify.retention_class(), RetentionClass::Standard24M);
}
