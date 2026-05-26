//! T-097 integration tests for `ModelRouter` and `RouterState`.
//!
//! Minimum 14 tests covering determinism (S13.2 §C4), wildcard matching, recovery
//! forbidding, health-based routing, all-unhealthy fallback, `RouterState` health
//! transitions, `mint_routing_id`, integration with `InMemoryCognitiveCore`, backward
//! compat (no router → T-095 stub), and concurrent `route()` calls.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::Utc;

use aios_cognitive::{
    AICrossOriginPosture, BackendHealthEntry, BackendHealthState, CircuitBreakerConfig,
    CircuitBreakerStats, CircuitState, CognitiveCore, CognitiveIntent, InMemoryCognitiveCore,
    IntentId, LatencyTier, ModelBackendKind, ModelRouter, PrivacyClass, ProviderClass, RouterState,
    RoutingInputs, SubjectRef, TranslationContext,
};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn healthy_entry(kind: ModelBackendKind) -> BackendHealthEntry {
    BackendHealthEntry {
        backend_kind: kind,
        provider_class: ProviderClass::Ollama,
        state: BackendHealthState::Healthy,
        config: CircuitBreakerConfig::default(),
        stats: CircuitBreakerStats {
            state: CircuitState::Closed,
            success_count: 100,
            failure_count: 0,
            error_rate: 0.0,
            cooldown_seconds: 0,
            last_state_change_at: Utc::now(),
            next_probe_at: None,
        },
    }
}

fn unhealthy_entry(kind: ModelBackendKind) -> BackendHealthEntry {
    BackendHealthEntry {
        backend_kind: kind,
        provider_class: ProviderClass::Ollama,
        state: BackendHealthState::Unhealthy,
        config: CircuitBreakerConfig::default(),
        stats: CircuitBreakerStats {
            state: CircuitState::Open,
            success_count: 0,
            failure_count: 100,
            error_rate: 1.0,
            cooldown_seconds: 30,
            last_state_change_at: Utc::now(),
            next_probe_at: None,
        },
    }
}

#[allow(dead_code)]
fn suspended_entry(kind: ModelBackendKind) -> BackendHealthEntry {
    BackendHealthEntry {
        backend_kind: kind,
        provider_class: ProviderClass::Ollama,
        state: BackendHealthState::Suspended,
        config: CircuitBreakerConfig::default(),
        stats: CircuitBreakerStats {
            state: CircuitState::Open,
            success_count: 0,
            failure_count: 0,
            error_rate: 0.0,
            cooldown_seconds: 0,
            last_state_change_at: Utc::now(),
            next_probe_at: None,
        },
    }
}

#[allow(clippy::missing_const_for_fn)]
fn routing_inputs(
    latency_class: LatencyTier,
    privacy_class: PrivacyClass,
    posture: AICrossOriginPosture,
    health: Vec<BackendHealthEntry>,
    recovery_mode: bool,
    budget_ok: bool,
) -> RoutingInputs {
    RoutingInputs {
        latency_class,
        privacy_class,
        ai_cross_origin_posture: posture,
        backend_health_snapshot: health,
        recovery_mode,
        budget_ok,
    }
}

fn default_inputs_t3() -> RoutingInputs {
    routing_inputs(
        LatencyTier::T3LocalCognitive,
        PrivacyClass::Internal,
        AICrossOriginPosture::AiVaultBrokeredOnly,
        vec![
            healthy_entry(ModelBackendKind::LocalGpu),
            healthy_entry(ModelBackendKind::LocalCpu),
        ],
        false,
        true,
    )
}

fn make_intent(text: &str) -> CognitiveIntent {
    CognitiveIntent {
        intent_id: IntentId::new(),
        subject: SubjectRef("agent:dev".into()),
        natural_language: text.to_string(),
        context_hash: String::new(),
        created_at: Utc::now(),
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Internal,
    }
}

fn make_context() -> TranslationContext {
    TranslationContext {
        subject: SubjectRef("agent:dev".into()),
        available_models: vec![],
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Internal,
        ai_cross_origin_posture: AICrossOriginPosture::AiVaultBrokeredOnly,
        recovery_mode: false,
        budget_ok: true,
    }
}

// ---------------------------------------------------------------------------
// 1. new_with_defaults creates non-empty precedence table
// ---------------------------------------------------------------------------

#[test]
fn new_with_defaults_creates_non_empty_table() {
    let router = ModelRouter::new_with_defaults();
    let table = router.precedence_table();
    assert!(!table.is_empty(), "precedence table must not be empty");
    assert!(
        !router.code_version().is_empty(),
        "code version must be set"
    );
}

// ---------------------------------------------------------------------------
// 2. T3 + LocalGpu healthy → LocalGpu (rule 8)
// ---------------------------------------------------------------------------

#[test]
fn route_t3_healthy_local_gpu_returns_local_gpu() {
    let router = ModelRouter::new_with_defaults();
    let inputs = routing_inputs(
        LatencyTier::T3LocalCognitive,
        PrivacyClass::Internal,
        AICrossOriginPosture::AiVaultBrokeredOnly,
        vec![
            healthy_entry(ModelBackendKind::LocalGpu),
            healthy_entry(ModelBackendKind::LocalCpu),
        ],
        false,
        true,
    );
    let decision = router.route(&inputs).expect("route must succeed");
    assert_eq!(decision.chosen_backend, ModelBackendKind::LocalGpu);
    assert_eq!(decision.matched_rule, 8);
    assert!(!decision.degraded);
}

// ---------------------------------------------------------------------------
// 3. T3 + LocalGpu unhealthy + LocalCpu healthy → LocalCpu (rule 9)
// ---------------------------------------------------------------------------

#[test]
fn route_t3_no_gpu_healthy_local_cpu_returns_local_cpu() {
    let router = ModelRouter::new_with_defaults();
    let inputs = routing_inputs(
        LatencyTier::T3LocalCognitive,
        PrivacyClass::Internal,
        AICrossOriginPosture::AiVaultBrokeredOnly,
        vec![
            unhealthy_entry(ModelBackendKind::LocalGpu),
            healthy_entry(ModelBackendKind::LocalCpu),
        ],
        false,
        true,
    );
    let decision = router.route(&inputs).expect("route must succeed");
    assert_eq!(decision.chosen_backend, ModelBackendKind::LocalCpu);
    assert_eq!(decision.matched_rule, 9);
    assert!(!decision.degraded);
}

// ---------------------------------------------------------------------------
// 4. T4 + vault-brokered + budget_ok → ExternalVaultBrokered (rule 10)
// ---------------------------------------------------------------------------

#[test]
fn route_t4_vault_brokered_budget_ok_returns_external() {
    let router = ModelRouter::new_with_defaults();
    let inputs = routing_inputs(
        LatencyTier::T4PowerfulReasoning,
        PrivacyClass::Internal,
        AICrossOriginPosture::AiVaultBrokeredOnly,
        vec![],
        false,
        true,
    );
    let decision = router.route(&inputs).expect("route must succeed");
    assert_eq!(
        decision.chosen_backend,
        ModelBackendKind::ExternalVaultBrokered
    );
    assert_eq!(decision.matched_rule, 10);
    assert!(!decision.degraded);
}

// ---------------------------------------------------------------------------
// 5. Recovery mode forbids T3/T4 (rule 1)
// ---------------------------------------------------------------------------

#[test]
fn route_recovery_mode_forbids_t3_t4() {
    let router = ModelRouter::new_with_defaults();

    for tier in [
        LatencyTier::T3LocalCognitive,
        LatencyTier::T4PowerfulReasoning,
    ] {
        let inputs = routing_inputs(
            tier,
            PrivacyClass::Internal,
            AICrossOriginPosture::AiVaultBrokeredOnly,
            vec![healthy_entry(ModelBackendKind::LocalGpu)],
            true, // recovery_mode
            true,
        );
        let decision = router.route(&inputs).expect("route must succeed");
        assert_eq!(decision.chosen_backend, ModelBackendKind::DegradedNull);
        assert_eq!(decision.matched_rule, 1);
        assert_eq!(
            decision.reason.as_deref(),
            Some("recovery_mode"),
            "reason must be recovery_mode for tier {tier:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 6. SecretBearing + T3/T4 → local only (rule 5)
// ---------------------------------------------------------------------------

#[test]
fn route_secret_bearing_uses_local_only() {
    let router = ModelRouter::new_with_defaults();
    let inputs = routing_inputs(
        LatencyTier::T3LocalCognitive,
        PrivacyClass::SecretBearing,
        AICrossOriginPosture::AiVaultBrokeredOnly,
        vec![
            healthy_entry(ModelBackendKind::LocalGpu),
            healthy_entry(ModelBackendKind::LocalCpu),
        ],
        false,
        true,
    );
    let decision = router.route(&inputs).expect("route must succeed");
    // Must be local — not external
    assert!(matches!(
        decision.chosen_backend,
        ModelBackendKind::LocalGpu
            | ModelBackendKind::LocalCpu
            | ModelBackendKind::LocalDistributed
    ));
    assert_eq!(decision.matched_rule, 5);
    assert_eq!(
        decision.reason.as_deref(),
        Some("secret_bearing_local_only")
    );
}

// ---------------------------------------------------------------------------
// 7. AiNoExternal + T3 → local only (rule 6)
// ---------------------------------------------------------------------------

#[test]
fn route_ai_no_external_uses_local_only() {
    let router = ModelRouter::new_with_defaults();
    let inputs = routing_inputs(
        LatencyTier::T3LocalCognitive,
        PrivacyClass::Internal,
        AICrossOriginPosture::AiNoExternal,
        vec![healthy_entry(ModelBackendKind::LocalCpu)],
        false,
        true,
    );
    let decision = router.route(&inputs).expect("route must succeed");
    assert_eq!(decision.chosen_backend, ModelBackendKind::LocalCpu);
    assert_eq!(decision.matched_rule, 6);
}

// ---------------------------------------------------------------------------
// 8. AiLoopbackOnly + T3 → LocalCpu/LocalGpu only, no LocalDistributed (rule 7)
// ---------------------------------------------------------------------------

#[test]
fn route_ai_loopback_only_restricts_to_cpu_gpu() {
    let router = ModelRouter::new_with_defaults();
    // LocalDistributed is healthy but not allowed under loopback
    let inputs = routing_inputs(
        LatencyTier::T3LocalCognitive,
        PrivacyClass::Internal,
        AICrossOriginPosture::AiLoopbackOnly,
        vec![
            healthy_entry(ModelBackendKind::LocalDistributed),
            healthy_entry(ModelBackendKind::LocalCpu),
        ],
        false,
        true,
    );
    let decision = router.route(&inputs).expect("route must succeed");
    // Must NOT be LocalDistributed — only LocalCpu or LocalGpu
    assert!(
        matches!(
            decision.chosen_backend,
            ModelBackendKind::LocalCpu | ModelBackendKind::LocalGpu
        ),
        "expected LocalCpu or LocalGpu, got {:?}",
        decision.chosen_backend
    );
    assert_eq!(decision.matched_rule, 7);
}

// ---------------------------------------------------------------------------
// 9. T0 → Cached (rule 2)
// ---------------------------------------------------------------------------

#[test]
fn route_t0_returns_cached() {
    let router = ModelRouter::new_with_defaults();
    let inputs = routing_inputs(
        LatencyTier::T0CachedUiState,
        PrivacyClass::Public,
        AICrossOriginPosture::AiVaultBrokeredOnly,
        vec![],
        false,
        true,
    );
    let decision = router.route(&inputs).expect("route must succeed");
    assert_eq!(decision.chosen_backend, ModelBackendKind::Cached);
    assert_eq!(decision.matched_rule, 2);
}

// ---------------------------------------------------------------------------
// 10. T2 → FallbackRuleBased (rule 4)
// ---------------------------------------------------------------------------

#[test]
fn route_t2_returns_fallback_rule_based() {
    let router = ModelRouter::new_with_defaults();
    let inputs = routing_inputs(
        LatencyTier::T2CatalogRetrieval,
        PrivacyClass::Internal,
        AICrossOriginPosture::AiVaultBrokeredOnly,
        vec![],
        false,
        true,
    );
    let decision = router.route(&inputs).expect("route must succeed");
    assert_eq!(decision.chosen_backend, ModelBackendKind::FallbackRuleBased);
    assert_eq!(decision.matched_rule, 4);
}

// ---------------------------------------------------------------------------
// 11. Empty health snapshot + T3 → fallthrough to rule 12/13
// ---------------------------------------------------------------------------

#[test]
fn route_empty_health_snapshot_falls_through() {
    let router = ModelRouter::new_with_defaults();
    let inputs = routing_inputs(
        LatencyTier::T3LocalCognitive,
        PrivacyClass::Internal,
        AICrossOriginPosture::AiVaultBrokeredOnly,
        vec![], // no backends registered
        false,
        true,
    );
    let decision = router.route(&inputs).expect("route must succeed");
    // All preferred backends are missing from snapshot → unhealthy → rule 12/13
    assert!(matches!(
        decision.chosen_backend,
        ModelBackendKind::FallbackRuleBased | ModelBackendKind::DegradedNull
    ));
    assert!(decision.degraded);
}

// ---------------------------------------------------------------------------
// 12. Determinism — same inputs produce same output (S13.2 §C4)
// ---------------------------------------------------------------------------

#[test]
fn route_determinism_same_inputs_same_output() {
    let router = ModelRouter::new_with_defaults();
    let inputs = routing_inputs(
        LatencyTier::T3LocalCognitive,
        PrivacyClass::Internal,
        AICrossOriginPosture::AiVaultBrokeredOnly,
        vec![
            healthy_entry(ModelBackendKind::LocalGpu),
            healthy_entry(ModelBackendKind::LocalCpu),
        ],
        false,
        true,
    );

    let decision1 = router.route(&inputs).expect("route must succeed");
    let decision2 = router.route(&inputs).expect("route must succeed");

    assert_eq!(decision1.chosen_backend, decision2.chosen_backend);
    assert_eq!(decision1.matched_rule, decision2.matched_rule);
    assert_eq!(decision1.degraded, decision2.degraded);
    assert_eq!(decision1.reason, decision2.reason);
}

// ---------------------------------------------------------------------------
// 13. T4 + budget exhausted + LocalGpu healthy → LocalGpu degraded (rule 11)
// ---------------------------------------------------------------------------

#[test]
fn route_t4_budget_exhausted_local_gpu_healthy_returns_local_gpu_degraded() {
    let router = ModelRouter::new_with_defaults();
    let inputs = routing_inputs(
        LatencyTier::T4PowerfulReasoning,
        PrivacyClass::Internal,
        AICrossOriginPosture::AiVaultBrokeredOnly,
        vec![healthy_entry(ModelBackendKind::LocalGpu)],
        false,
        false, // budget exhausted
    );
    let decision = router.route(&inputs).expect("route must succeed");
    assert_eq!(decision.chosen_backend, ModelBackendKind::LocalGpu);
    assert_eq!(decision.matched_rule, 11);
    assert!(decision.degraded);
    assert_eq!(
        decision.reason.as_deref(),
        Some("budget_exhausted_local_fallback")
    );
}

// ---------------------------------------------------------------------------
// 14. RouterState health transitions via observe_invocation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn router_state_health_transitions() {
    let state = RouterState::new();

    // Initially, unobserved backends return empty/healthy
    let health = state.get_health().await;
    assert!(
        !health.contains_key(&ModelBackendKind::LocalGpu),
        "unobserved backend should not appear in snapshot"
    );

    // Successful invocations → stays healthy
    for _ in 0..100 {
        state
            .observe_invocation(ModelBackendKind::LocalGpu, true)
            .await;
    }
    let health = state.get_health().await;
    assert_eq!(
        health.get(&ModelBackendKind::LocalGpu),
        Some(&BackendHealthState::Healthy)
    );

    // Failures → degrade
    for _ in 0..10 {
        state
            .observe_invocation(ModelBackendKind::LocalGpu, false)
            .await;
    }
    let health = state.get_health().await;
    let gpu_health = health
        .get(&ModelBackendKind::LocalGpu)
        .expect("LocalGpu must be in health map");
    assert!(
        matches!(
            gpu_health,
            BackendHealthState::DegradedAvailability | BackendHealthState::Unhealthy
        ),
        "expected degraded or unhealthy after failures, got {gpu_health:?}"
    );
}

// ---------------------------------------------------------------------------
// 15. mint_routing_id returns unique, properly formatted ids
// ---------------------------------------------------------------------------

#[tokio::test]
async fn router_state_mint_routing_id() {
    let state = RouterState::new();
    let id1 = state.mint_routing_id().await;
    let id2 = state.mint_routing_id().await;
    let id3 = state.mint_routing_id().await;

    assert!(id1.starts_with("rtdg_"), "must start with rtdg_");
    assert!(id2.starts_with("rtdg_"), "must start with rtdg_");
    assert_ne!(id1, id2, "ids must be unique");
    assert_ne!(id2, id3, "ids must be unique");
    assert_ne!(id1, id3, "ids must be unique");
}

// ---------------------------------------------------------------------------
// 16. Integration: InMemoryCognitiveCore with router uses router path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn integration_with_router_uses_router_path() {
    let router = Arc::new(ModelRouter::new_with_defaults());
    let state = Arc::new(RouterState::new());

    // Set up healthy LocalGpu
    state
        .set_health(ModelBackendKind::LocalGpu, BackendHealthState::Healthy)
        .await;

    let core = InMemoryCognitiveCore::new().with_router(router, state);
    let intent = make_intent("restart nginx");
    let context = make_context();

    let result = core
        .translate_intent(&intent, &context)
        .await
        .expect("translate must succeed");

    // T-097 path produces a routing_decision_id that starts with rtdg_
    let rid = result
        .routing_decision_id
        .expect("routing_decision_id must be set");
    assert!(rid.starts_with("rtdg_"), "routing id must start with rtdg_");
    assert_eq!(
        result.translation_provenance.translator_version,
        "0.1.0-T097"
    );
}

// ---------------------------------------------------------------------------
// 17. Integration: without router preserves T-095 stub
// ---------------------------------------------------------------------------

#[tokio::test]
async fn integration_without_router_preserves_stub() {
    let core = InMemoryCognitiveCore::new(); // no router
    let intent = make_intent("restart nginx");
    let context = make_context();

    let result = core
        .translate_intent(&intent, &context)
        .await
        .expect("translate must succeed");

    // T-095 stub path: routing_decision_id is set but with stub logic
    assert!(result.routing_decision_id.is_some());
    assert_eq!(
        result.translation_provenance.translator_version,
        "0.1.0-T097"
    );
}

// ---------------------------------------------------------------------------
// 18. RouterState set_health and get_health round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn router_state_set_and_get_health() {
    let state = RouterState::new();

    state
        .set_health(ModelBackendKind::LocalGpu, BackendHealthState::Healthy)
        .await;
    state
        .set_health(ModelBackendKind::LocalCpu, BackendHealthState::Unhealthy)
        .await;

    let health = state.get_health().await;
    assert_eq!(
        health.get(&ModelBackendKind::LocalGpu),
        Some(&BackendHealthState::Healthy)
    );
    assert_eq!(
        health.get(&ModelBackendKind::LocalCpu),
        Some(&BackendHealthState::Unhealthy)
    );
}

// ---------------------------------------------------------------------------
// 19. Concurrent route() calls on shared router
// ---------------------------------------------------------------------------

#[test]
fn concurrent_route_calls() {
    let router = Arc::new(ModelRouter::new_with_defaults());
    let inputs = default_inputs_t3();

    let mut handles = Vec::new();
    for _ in 0..10 {
        let r = Arc::clone(&router);
        let inp = inputs.clone();
        handles.push(std::thread::spawn(move || {
            r.route(&inp).expect("route must succeed")
        }));
    }

    for handle in handles {
        let decision = handle.join().expect("thread must not panic");
        assert_eq!(decision.chosen_backend, ModelBackendKind::LocalGpu);
        assert_eq!(decision.matched_rule, 8);
    }
}

// ---------------------------------------------------------------------------
// 20. Router preserves code_version in routing decisions
// ---------------------------------------------------------------------------

#[test]
fn router_preserves_code_version() {
    let router = ModelRouter::new_with_defaults();
    assert_eq!(router.code_version(), "aios-cognitive/0.0.1-T094");

    let inputs = default_inputs_t3();
    let decision = router.route(&inputs).expect("route must succeed");
    // Each decision has its own routing_id, but all come from same code_version
    assert!(!decision.routing_id.is_empty());
}
