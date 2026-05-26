//! T-105 M11 closure: §22 cognitive golden-path scenarios over the M3-M10 stack.

#![allow(
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::panic,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::unwrap_used,
    reason = "closure integration fixtures fail loudly on contract drift"
)]

use std::sync::Arc;

use aios_action::ActionEnvelope;
use aios_capability_runtime::{
    CapabilityRuntime, EvidenceEmitter, InMemoryCapabilityRuntime, InMemoryEvidenceSink,
    RuntimeContext,
};
use aios_evidence::RecordType;
use aios_recovery::{InMemoryRecoveryBoundary, RecoveryRuntimeAdapter};

use aios_cognitive::{
    AICrossOriginPosture, BackendHealthEntry, BackendHealthState, CircuitBreaker,
    CircuitBreakerConfig, CircuitBreakerRegistry, CircuitState, CognitiveCore,
    CognitiveEvidenceEmitter, CognitiveIntent, CognitiveModel, CognitiveModelCatalog,
    CognitiveSubjectRef, InMemoryCognitiveCore, IntentId, LatencyTier, ModelBackendKind,
    ModelBindingRegistry, ModelId, ModelRouter, PrivacyClass, ProviderClass, ProviderDispatcher,
    RouterState, RoutingInputs, SubjectRef, TranslationContext, AIOS_COGNITIVE_SUBJECT,
};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn make_context(posture: AICrossOriginPosture) -> TranslationContext {
    TranslationContext {
        subject: SubjectRef("human:lucky".into()),
        available_models: vec![],
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Public,
        ai_cross_origin_posture: posture,
        recovery_mode: false,
        budget_ok: true,
    }
}

fn make_intent(text: &str) -> CognitiveIntent {
    CognitiveIntent {
        intent_id: IntentId::new(),
        natural_language: text.into(),
        subject: SubjectRef("human:lucky".into()),
        context_hash: String::new(),
        privacy_class: PrivacyClass::Public,
        latency_class: LatencyTier::T3LocalCognitive,
        created_at: chrono::Utc::now(),
    }
}

fn make_model(provider: ProviderClass, vault_cap: Option<&str>) -> CognitiveModel {
    CognitiveModel {
        model_id: ModelId::new(),
        provider,
        capabilities: vec!["text-generation".into()],
        max_tokens: 4096,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: vault_cap.map(String::from),
        created_at: chrono::Utc::now(),
    }
}

fn make_health_snapshot() -> Vec<BackendHealthEntry> {
    use aios_cognitive::BackendHealthEntry;
    use aios_cognitive::CircuitBreakerStats;
    vec![
        BackendHealthEntry {
            backend_kind: ModelBackendKind::LocalCpu,
            provider_class: ProviderClass::Ollama,
            state: BackendHealthState::Healthy,
            config: CircuitBreakerConfig::default(),
            stats: CircuitBreakerStats {
                state: CircuitState::Closed,
                success_count: 100,
                failure_count: 0,
                error_rate: 0.0,
                cooldown_seconds: 0,
                last_state_change_at: chrono::Utc::now(),
                next_probe_at: None,
            },
        },
        BackendHealthEntry {
            backend_kind: ModelBackendKind::ExternalVaultBrokered,
            provider_class: ProviderClass::Anthropic,
            state: BackendHealthState::Healthy,
            config: CircuitBreakerConfig::default(),
            stats: CircuitBreakerStats {
                state: CircuitState::Closed,
                success_count: 200,
                failure_count: 0,
                error_rate: 0.0,
                cooldown_seconds: 0,
                last_state_change_at: chrono::Utc::now(),
                next_probe_at: None,
            },
        },
    ]
}

async fn set_health_from_snapshot(state: &RouterState, snapshot: &[BackendHealthEntry]) {
    for entry in snapshot {
        state.set_health(entry.backend_kind, entry.state).await;
    }
}

// ---------------------------------------------------------------------------
// Scenario 1 — cognitive readiness on boot
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_1_cognitive_core_ready_on_boot() {
    let router = Arc::new(ModelRouter::with_table(vec![]));
    let state = Arc::new(RouterState::new());
    set_health_from_snapshot(&state, &make_health_snapshot()).await;

    let breakers = Arc::new(CircuitBreakerRegistry::new_with_defaults());

    let catalog = Arc::new(
        tokio::task::spawn_blocking(CognitiveModelCatalog::with_fixtures)
            .await
            .unwrap(),
    );
    let bindings = Arc::new(ModelBindingRegistry::new());
    let dispatcher = Arc::new(ProviderDispatcher::new());

    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[0u8; 32]);
    let log: Arc<dyn aios_cognitive::CognitiveEvidenceLog> =
        Arc::new(aios_cognitive::InMemoryCognitiveEvidenceLog::new());
    let emitter = Arc::new(CognitiveEvidenceEmitter::new(
        log,
        signing_key,
        CognitiveSubjectRef(AIOS_COGNITIVE_SUBJECT.into()),
    ));

    let core = InMemoryCognitiveCore::new()
        .with_router(router, state)
        .with_breakers(breakers)
        .with_model_catalog(catalog, bindings)
        .with_provider_dispatcher(dispatcher)
        .with_evidence_emitter(emitter);

    let intents = core.list_supported_intents();
    assert_eq!(intents.len(), 2);
    assert!(intents.iter().any(|c| c.intent_kind == "service.restart"));
    assert!(intents
        .iter()
        .any(|c| c.intent_kind == "cognitive.translate"));
}

// ---------------------------------------------------------------------------
// Scenario 2 — intent translation produces typed envelope with provenance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_2_intent_translation_produces_typed_action_envelope() {
    let router = Arc::new(ModelRouter::with_table(vec![]));
    let state = Arc::new(RouterState::new());
    set_health_from_snapshot(&state, &make_health_snapshot()).await;

    let breakers = Arc::new(CircuitBreakerRegistry::new_with_defaults());

    let core = InMemoryCognitiveCore::new()
        .with_router(router, state)
        .with_breakers(breakers);

    let intent = make_intent("restart the nginx web server");
    let ctx = make_context(AICrossOriginPosture::AiVaultBrokeredOnly);

    let result = core
        .translate_intent(&intent, &ctx)
        .await
        .expect("translation should succeed");

    // INV-002: produced_action is a typed ActionEnvelope
    assert_eq!(result.intent_id, intent.intent_id);
    assert!(result.produced_action.identity.is_ai);

    // Provenance marker injected per INV-002
    let provenance = result
        .produced_action
        .request
        .target
        .get("cognitive_provenance")
        .and_then(|v| v.as_str());
    assert!(
        provenance.is_some(),
        "cognitive_provenance marker must be present in envelope"
    );

    assert!(!result.translation_provenance.model_used.is_empty());
    assert!(result.routing_decision_id.is_some());

    // Verify cache hit on second call
    let cached = core
        .get_translation(&intent.intent_id)
        .await
        .expect("cached result should be retrievable");
    assert_eq!(cached.intent_id, intent.intent_id);
}

// ---------------------------------------------------------------------------
// Scenario 3 — circuit breaker full lifecycle: trip → open → half-open → closed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_3_circuit_breaker_full_lifecycle() {
    let config = CircuitBreakerConfig {
        error_rate_threshold: 0.05,
        window_seconds: 300,
        initial_cooldown_seconds: 1,
        max_cooldown_seconds: 600,
    };

    let breaker = CircuitBreaker::new(ModelBackendKind::ExternalVaultBrokered, config);

    // Phase 1: breaker starts Closed
    assert_eq!(breaker.current_state().await, CircuitState::Closed);

    // Phase 2: trip the breaker with high error rate
    for _ in 0..5 {
        breaker.record_outcome(true, 50).await;
    }
    breaker.record_outcome(false, 500).await;
    assert_eq!(breaker.current_state().await, CircuitState::Open);

    // Phase 3: try_admit is rejected while Open
    let rejected = breaker.try_admit().await;
    assert!(rejected.is_err());

    // Phase 4: wait for cooldown → HalfOpen
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    breaker.record_outcome(true, 50).await;
    assert_eq!(breaker.current_state().await, CircuitState::HalfOpen);

    // Phase 5: successful probe → Closed
    breaker.record_probe_outcome(true, 50).await;
    assert_eq!(breaker.current_state().await, CircuitState::Closed);

    // Phase 6: stats reflect final state
    let stats = breaker.current_stats().await;
    assert_eq!(stats.state, CircuitState::Closed);
    assert_eq!(stats.cooldown_seconds, 0);
    assert!(stats.next_probe_at.is_none());
}

// ---------------------------------------------------------------------------
// Scenario 4 — router determinism: same inputs → same output
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_4_router_determinism_same_inputs_same_output() {
    let router = Arc::new(ModelRouter::with_table(vec![]));
    let state = Arc::new(RouterState::new());
    set_health_from_snapshot(&state, &make_health_snapshot()).await;

    let inputs = RoutingInputs {
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Public,
        ai_cross_origin_posture: AICrossOriginPosture::AiVaultBrokeredOnly,
        backend_health_snapshot: make_health_snapshot(),
        recovery_mode: false,
        budget_ok: true,
    };

    let d1 = router.route(&inputs).expect("first route");
    let d2 = router.route(&inputs).expect("second route");
    let d3 = router.route(&inputs).expect("third route");

    assert_eq!(d1.chosen_backend, d2.chosen_backend);
    assert_eq!(d2.chosen_backend, d3.chosen_backend);
    assert_eq!(d1.provider_class, d2.provider_class);
    assert_eq!(d2.provider_class, d3.provider_class);
    // Different routing ids (unique per call)
    assert_ne!(d1.routing_id, d2.routing_id);
}

// ---------------------------------------------------------------------------
// Scenario 5 — AI_NO_EXTERNAL blocks vault-brokered dispatch
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_5_ai_no_external_blocks_external_dispatch() {
    let evidence_log = Arc::new(aios_cognitive::InMemoryCognitiveEvidenceLog::new());
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[0u8; 32]);
    let emitter = Arc::new(CognitiveEvidenceEmitter::new(
        evidence_log.clone(),
        signing_key,
        CognitiveSubjectRef(AIOS_COGNITIVE_SUBJECT.into()),
    ));
    let dispatcher = Arc::new(ProviderDispatcher::new().with_evidence_emitter(emitter));

    let model = make_model(ProviderClass::Anthropic, Some("vcap_test"));

    let result = dispatcher
        .dispatch_to_provider(
            &model,
            &make_intent("summarize this document"),
            AICrossOriginPosture::AiNoExternal,
        )
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        aios_cognitive::CognitiveError::ExternalBackendBlocked { posture } => {
            assert_eq!(posture, AICrossOriginPosture::AiNoExternal);
        }
        other => panic!("expected ExternalBackendBlocked, got {other:?}"),
    }

    // Verify AI_DIRECT_INTERNET_DENIED evidence was emitted
    let entries = evidence_log.receipts().await;
    assert!(
        entries
            .iter()
            .any(|e| e.record_type() == RecordType::AiDirectInternetDenied),
        "AI_DIRECT_INTERNET_DENIED evidence must be recorded"
    );
}

// ---------------------------------------------------------------------------
// Scenario 6 — end-to-end cognitive → runtime pipeline
// ---------------------------------------------------------------------------

struct AllowAllKernel;

#[async_trait::async_trait]
impl aios_policy::PolicyKernel for AllowAllKernel {
    async fn evaluate_policy(
        &self,
        _envelope: &ActionEnvelope,
        _ctx: &aios_policy::PolicyContext,
    ) -> Result<aios_policy::PolicyDecision, aios_policy::PolicyError> {
        use aios_action::ActionId;
        Ok(aios_policy::PolicyDecision {
            policy_decision_id: "poldec_test".into(),
            action_id: ActionId::new(),
            request_hash: "0".repeat(32),
            bundle_version: "test-v1".into(),
            enrichment_snapshot_id: "polb_snap_test".into(),
            decision: aios_policy::Decision::Allow,
            reason_code: "test_allow".into(),
            reason_message: "test allow all".into(),
            constraints: aios_policy::Constraints::default(),
            approval: aios_policy::ApprovalRequirement {
                required: false,
                approval_scope: aios_policy::ApprovalScope::ExactRequestHash,
                ttl_seconds: 0,
                approver_classes: vec![aios_policy::ApproverClass::Human],
                require_human_co_signer: false,
            },
            evidence_receipt_id: "evr_test_0".into(),
            evaluated_at: chrono::Utc::now(),
            rules_consulted: 1,
            simulated: false,
        })
    }
}

#[tokio::test]
async fn scenario_6_e2e_cognitive_to_runtime_pipeline() {
    // ── Build cognitive core ──
    let router = Arc::new(ModelRouter::with_table(vec![]));
    let router_state = Arc::new(RouterState::new());
    set_health_from_snapshot(&router_state, &make_health_snapshot()).await;

    let breakers = Arc::new(CircuitBreakerRegistry::new_with_defaults());

    let core = Arc::new(
        InMemoryCognitiveCore::new()
            .with_router(router, router_state)
            .with_breakers(breakers),
    );

    // ── Build runtime ──
    let recovery = Arc::new(InMemoryRecoveryBoundary::new());
    let policy_kernel = Arc::new(AllowAllKernel);

    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[0u8; 32]);
    let evidence_sink = Arc::new(InMemoryEvidenceSink::new(signing_key));
    let evidence_emitter = Arc::new(EvidenceEmitter::new(evidence_sink.clone()));

    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(policy_kernel)
        .with_recovery_hook(Arc::new(RecoveryRuntimeAdapter::new(recovery)))
        .with_evidence_emitter(evidence_emitter);

    // ── Cognitive translates ──
    let intent = make_intent("restart the nginx service");
    let ctx = make_context(AICrossOriginPosture::AiVaultBrokeredOnly);

    let translation = core
        .translate_intent(&intent, &ctx)
        .await
        .expect("translation succeeds");

    // INV-002: verify provenance marker
    let provenance = translation
        .produced_action
        .request
        .target
        .get("cognitive_provenance")
        .and_then(|v| v.as_str());
    assert!(
        provenance.is_some(),
        "cognitive envelope must carry provenance marker"
    );

    // ── Submit to runtime ──
    let runtime_ctx = RuntimeContext::new("human:lucky", "test-v1", "0.1.0-T105");

    let action_ctx = runtime
        .submit_action(&translation.produced_action, &runtime_ctx)
        .await
        .expect("runtime accepts cognitive-produced envelope");

    assert!(
        !action_ctx.evidence_chain.is_empty(),
        "action should produce evidence"
    );

    // ── Verify cognitive cache ──
    let cached = core
        .get_translation(&intent.intent_id)
        .await
        .expect("translation cached");
    assert_eq!(cached.intent_id, intent.intent_id);
}
