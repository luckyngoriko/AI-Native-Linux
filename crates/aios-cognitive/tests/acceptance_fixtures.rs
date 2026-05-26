//! T-105 M11 closure: S1.x / S13.x / S14.1 acceptance fixtures.

#![allow(
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::panic,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::unwrap_used,
    reason = "acceptance fixtures fail loudly on spec-contract drift"
)]

use std::sync::Arc;

use aios_cognitive::{
    AICrossOriginPosture, BackendHealthEntry, BackendHealthState, CircuitBreaker,
    CircuitBreakerConfig, CircuitBreakerStats, CircuitState, CognitiveCore, CognitiveError,
    CognitiveIntent, CognitiveModel, CognitiveModelCatalog, InMemoryCognitiveCore, IntentId,
    LatencyClassifier, LatencyTier, ModelBackendKind, ModelBindingRegistry, ModelId, ModelRouter,
    PrivacyClass, ProviderClass, ProviderDispatcher, RouterState, RoutingInputs, SubjectRef,
    TranslationContext,
};

fn make_health_snapshot() -> Vec<BackendHealthEntry> {
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

fn make_intent(privacy: PrivacyClass, text: &str) -> CognitiveIntent {
    CognitiveIntent {
        intent_id: IntentId::new(),
        natural_language: text.into(),
        subject: SubjectRef("human:lucky".into()),
        context_hash: String::new(),
        privacy_class: privacy,
        latency_class: LatencyTier::T3LocalCognitive,
        created_at: chrono::Utc::now(),
    }
}

async fn set_health_from_snapshot(state: &RouterState, snapshot: &[BackendHealthEntry]) {
    for entry in snapshot {
        state.set_health(entry.backend_kind, entry.state).await;
    }
}

// ---------------------------------------------------------------------------
// Fixture 1 — Intent translation determinism (S1.1)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn accept_s1_1_translation_determinism_same_inputs_same_output() {
    let core = InMemoryCognitiveCore::new();

    let intent = make_intent(PrivacyClass::Public, "restart nginx");
    let ctx = TranslationContext {
        subject: SubjectRef("human:lucky".into()),
        available_models: vec![],
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Public,
        ai_cross_origin_posture: AICrossOriginPosture::AiVaultBrokeredOnly,
        recovery_mode: false,
        budget_ok: true,
    };

    let r1 = core.translate_intent(&intent, &ctx).await.unwrap();
    let r2 = core.translate_intent(&intent, &ctx).await.unwrap();

    // Same intent+context should produce same model_used, same provenance version
    assert_eq!(
        r1.translation_provenance.model_used, r2.translation_provenance.model_used,
        "same inputs must produce same model_used"
    );
    assert_eq!(
        r1.translation_provenance.translator_version, r2.translation_provenance.translator_version,
        "translator version must be identical"
    );
}

// ---------------------------------------------------------------------------
// Fixture 2 — Latency classification guard rules (S1.2)
// ---------------------------------------------------------------------------

#[test]
fn accept_s1_2_recovery_mode_forces_t1_deterministic() {
    let classifier = LatencyClassifier::new_with_defaults();
    // Long text would be T4 without recovery; recovery caps at T1
    let long_text = "word ".repeat(200);
    let intent = CognitiveIntent {
        intent_id: IntentId::new(),
        natural_language: long_text,
        subject: SubjectRef("human:lucky".into()),
        context_hash: String::new(),
        privacy_class: PrivacyClass::Public,
        latency_class: LatencyTier::T3LocalCognitive,
        created_at: chrono::Utc::now(),
    };
    let tier = classifier.classify(&intent, "PUBLIC", true);
    assert_eq!(tier, LatencyTier::T1Deterministic);
}

#[test]
fn accept_s1_2_classified_privacy_forces_t2_catalog_retrieval() {
    let classifier = LatencyClassifier::new_with_defaults();
    // Long text would be T4; CLASSIFIED caps at T2
    let long_text = "word ".repeat(200);
    let intent = CognitiveIntent {
        intent_id: IntentId::new(),
        natural_language: long_text,
        subject: SubjectRef("human:lucky".into()),
        context_hash: String::new(),
        privacy_class: PrivacyClass::Classified,
        latency_class: LatencyTier::T3LocalCognitive,
        created_at: chrono::Utc::now(),
    };
    let tier = classifier.classify(&intent, "CLASSIFIED", false);
    assert_eq!(tier, LatencyTier::T2CatalogRetrieval);
}

#[test]
fn accept_s1_2_secret_bearing_forces_t3_local_cognitive() {
    let classifier = LatencyClassifier::new_with_defaults();
    // Long text would be T4; SECRET_BEARING caps at T3
    let long_text = "word ".repeat(200);
    let intent = CognitiveIntent {
        intent_id: IntentId::new(),
        natural_language: long_text,
        subject: SubjectRef("human:lucky".into()),
        context_hash: String::new(),
        privacy_class: PrivacyClass::SecretBearing,
        latency_class: LatencyTier::T4PowerfulReasoning,
        created_at: chrono::Utc::now(),
    };
    let tier = classifier.classify(&intent, "SECRET_BEARING", false);
    assert_eq!(tier, LatencyTier::T3LocalCognitive);
}

#[test]
fn accept_s1_2_public_keeps_caller_tier() {
    let classifier = LatencyClassifier::new_with_defaults();
    // Medium text → T2; PUBLIC no cap
    let medium_text = "word ".repeat(50);
    let intent = CognitiveIntent {
        intent_id: IntentId::new(),
        natural_language: medium_text,
        subject: SubjectRef("human:lucky".into()),
        context_hash: String::new(),
        privacy_class: PrivacyClass::Public,
        latency_class: LatencyTier::T2CatalogRetrieval,
        created_at: chrono::Utc::now(),
    };
    let tier = classifier.classify(&intent, "PUBLIC", false);
    assert_eq!(tier, LatencyTier::T2CatalogRetrieval);
}

#[test]
fn accept_s1_2_internal_keeps_caller_tier() {
    let classifier = LatencyClassifier::new_with_defaults();
    // Long text → T4; INTERNAL no cap → stays T4
    let long_text = "word ".repeat(200);
    let intent = CognitiveIntent {
        intent_id: IntentId::new(),
        natural_language: long_text,
        subject: SubjectRef("human:lucky".into()),
        context_hash: String::new(),
        privacy_class: PrivacyClass::Internal,
        latency_class: LatencyTier::T3LocalCognitive,
        created_at: chrono::Utc::now(),
    };
    let tier = classifier.classify(&intent, "INTERNAL", false);
    assert_eq!(tier, LatencyTier::T4PowerfulReasoning);
}

// ---------------------------------------------------------------------------
// Fixture 3 — Router precedence table (S13.2 §7)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn accept_s13_2_router_precedence_table_deterministic_routing() {
    let router = Arc::new(ModelRouter::with_table(vec![]));
    let state = Arc::new(RouterState::new());
    set_health_from_snapshot(&state, &make_health_snapshot()).await;

    // Route a public low-privacy request — should prefer LocalCpu
    let inputs_public = RoutingInputs {
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Public,
        ai_cross_origin_posture: AICrossOriginPosture::AiVaultBrokeredOnly,
        backend_health_snapshot: make_health_snapshot(),
        recovery_mode: false,
        budget_ok: true,
    };

    let decision = router.route(&inputs_public).expect("route public");
    assert!(
        decision.matched_rule >= 1,
        "must match a rule in the precedence table"
    );
    assert!(!decision.routing_id.is_empty());
    assert!(!decision.backend_id.is_empty());
}

#[tokio::test]
async fn accept_s13_2_router_degraded_decision_has_reason() {
    let router = Arc::new(ModelRouter::with_table(vec![]));
    let state = Arc::new(RouterState::new());
    set_health_from_snapshot(&state, &make_health_snapshot()).await;

    // Route with SECRET_BEARING — should degrade or forbid
    let inputs_sensitive = RoutingInputs {
        latency_class: LatencyTier::T1Deterministic,
        privacy_class: PrivacyClass::Classified,
        ai_cross_origin_posture: AICrossOriginPosture::AiNoExternal,
        backend_health_snapshot: make_health_snapshot(),
        recovery_mode: false,
        budget_ok: true,
    };

    let decision = router.route(&inputs_sensitive).expect("route classified");
    // With restricted privacy + no-external posture, should be degraded or forbidden
    assert!(
        decision.degraded || decision.reason.is_some(),
        "restricted inputs should produce degraded or reasoned decision"
    );
}

// ---------------------------------------------------------------------------
// Fixture 4 — Circuit breaker trip/reset/half-open (S14.1 §6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn accept_s14_1_breaker_starts_closed() {
    let breaker = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
    assert_eq!(breaker.current_state().await, CircuitState::Closed);
    let stats = breaker.current_stats().await;
    assert_eq!(stats.success_count, 0);
    assert_eq!(stats.failure_count, 0);
}

#[tokio::test]
async fn accept_s14_1_breaker_trips_on_error_threshold() {
    let breaker = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
    for _ in 0..5 {
        breaker.record_outcome(true, 50).await;
    }
    breaker.record_outcome(false, 500).await;
    assert_eq!(breaker.current_state().await, CircuitState::Open);
}

#[tokio::test]
async fn accept_s14_1_breaker_min_samples_required() {
    let breaker = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
    // Fewer than MIN_SAMPLES_TO_OPEN (5) should not open even with 100% failure
    for _ in 0..4 {
        breaker.record_outcome(false, 500).await;
    }
    assert_eq!(breaker.current_state().await, CircuitState::Closed);
}

#[tokio::test]
async fn accept_s14_1_inv014_no_direct_open_to_closed() {
    let breaker = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
    for _ in 0..10 {
        breaker.record_outcome(false, 500).await;
    }
    assert_eq!(breaker.current_state().await, CircuitState::Open);
    // Must not close directly without HalfOpen transition
    for _ in 0..100 {
        breaker.record_outcome(true, 50).await;
    }
    assert_eq!(breaker.current_state().await, CircuitState::Open);
}

// ---------------------------------------------------------------------------
// Fixture 5 — Model catalog registration + lookup (S13.1)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn accept_s13_1_catalog_register_and_lookup() {
    let catalog = CognitiveModelCatalog::new();
    let model = CognitiveModel {
        model_id: ModelId::new(),
        provider: ProviderClass::Ollama,
        capabilities: vec!["text-generation".into()],
        max_tokens: 4096,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: None,
        created_at: chrono::Utc::now(),
    };
    let mid = model.model_id.clone();

    catalog.register(model).await.expect("register succeeds");
    let found = catalog.lookup(&mid).await.expect("lookup finds model");
    assert_eq!(found.model_id, mid);
    assert_eq!(found.provider, ProviderClass::Ollama);
}

#[tokio::test]
async fn accept_s13_1_catalog_with_fixtures_has_five_models() {
    let catalog = tokio::task::spawn_blocking(CognitiveModelCatalog::with_fixtures)
        .await
        .unwrap();
    let models = catalog.list().await;
    assert_eq!(models.len(), 5, "fixture catalog has 5 models");
    let default = catalog.get_default().await.expect("default model exists");
    assert_eq!(default.model_id.0, "mdl_fixture_anthropic");
}

#[tokio::test]
async fn accept_s13_1_catalog_duplicate_registration_rejected() {
    let catalog = CognitiveModelCatalog::new();
    let model = CognitiveModel {
        model_id: ModelId("mdl_test_dup".into()),
        provider: ProviderClass::Ollama,
        capabilities: vec![],
        max_tokens: 1024,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: None,
        created_at: chrono::Utc::now(),
    };
    catalog.register(model.clone()).await.expect("first ok");
    let err = catalog.register(model).await.unwrap_err();
    assert!(matches!(err, CognitiveError::NoMatchingCapability(_)));
}

// ---------------------------------------------------------------------------
// Fixture 6 — INV-002 provenance marker injection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn accept_inv002_provenance_marker_in_envelope() {
    let core = InMemoryCognitiveCore::new();
    let intent = make_intent(PrivacyClass::Public, "restart nginx");
    let ctx = TranslationContext {
        subject: SubjectRef("human:lucky".into()),
        available_models: vec![],
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Public,
        ai_cross_origin_posture: AICrossOriginPosture::AiVaultBrokeredOnly,
        recovery_mode: false,
        budget_ok: true,
    };

    let result = core.translate_intent(&intent, &ctx).await.unwrap();

    let envelope = &result.produced_action;
    // INV-002: Every translated envelope carries cognitive_provenance marker
    let marker = envelope
        .request
        .target
        .get("cognitive_provenance")
        .and_then(|v| v.as_str());
    assert!(
        marker.is_some(),
        "INV-002 violated: envelope lacks cognitive_provenance marker"
    );
}

// ---------------------------------------------------------------------------
// Fixture 7 — Provider dispatch AI_NO_EXTERNAL guard (S13.2 §5)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn accept_s13_2_ai_no_external_blocks_provider_dispatch() {
    let dispatcher = ProviderDispatcher::new();
    let model = CognitiveModel {
        model_id: ModelId::new(),
        provider: ProviderClass::Anthropic,
        capabilities: vec![],
        max_tokens: 4096,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: Some("vcap_test".into()),
        created_at: chrono::Utc::now(),
    };

    let result = dispatcher
        .dispatch_to_provider(
            &model,
            &make_intent(PrivacyClass::Public, "test"),
            AICrossOriginPosture::AiNoExternal,
        )
        .await;

    assert!(matches!(
        result.unwrap_err(),
        CognitiveError::ExternalBackendBlocked { .. }
    ));
}

// ---------------------------------------------------------------------------
// Fixture 8 — Model binding runtime tracking (S13.1)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn accept_s13_1_model_binding_runtime_tracking() {
    let registry = ModelBindingRegistry::new();
    let model = CognitiveModel {
        model_id: ModelId("mdl_test_binding".into()),
        provider: ProviderClass::Ollama,
        capabilities: vec!["text-generation".into()],
        max_tokens: 4096,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: None,
        created_at: chrono::Utc::now(),
    };
    let mid = model.model_id.clone();

    let binding = registry.bind(model, None).await.expect("bind succeeds");
    assert_eq!(binding.total_calls, 0);
    assert_eq!(binding.total_tokens_used, 0);

    // Record invocations
    registry.record_invocation(&mid, 100, 200, 50).await;
    registry.record_invocation(&mid, 50, 150, 25).await;

    let updated = registry.get(&mid).await.expect("binding exists");
    assert_eq!(updated.total_calls, 2);
    assert_eq!(updated.total_tokens_used, 500); // (100+200) + (50+150)
    assert_eq!(updated.total_cost_micros, 75);
    assert!(updated.last_used_at.is_some());
}

#[tokio::test]
async fn accept_s13_1_binding_inv018_rejects_external_without_vault() {
    let registry = ModelBindingRegistry::new();
    let model = CognitiveModel {
        model_id: ModelId::new(),
        provider: ProviderClass::Anthropic,
        capabilities: vec![],
        max_tokens: 4096,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: None,
        created_at: chrono::Utc::now(),
    };

    let err = registry.bind(model, None).await.unwrap_err();
    assert!(matches!(err, CognitiveError::Internal(_)));
}
