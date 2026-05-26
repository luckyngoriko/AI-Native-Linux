//! Integration tests for cognitive evidence emission (T-102).
//!
//! Covers INV-015, INV-018, BLAKE3 chain coherence, Ed25519 signatures,
//! backward compatibility (no emitter), concurrent emission, and end-to-end
//! emission through all four wired components.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::float_cmp,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::Utc;
use ed25519_dalek::SigningKey;
use rand_core::OsRng;

use aios_cognitive::{
    AICrossOriginPosture, BackendHealthEntry, BackendHealthState, CircuitBreaker,
    CircuitBreakerConfig, CircuitState, CognitiveCore, CognitiveError, CognitiveEvidenceEmitter,
    CognitiveIntent, CognitiveModel, CognitiveSubjectRef, InMemoryCognitiveCore,
    InMemoryCognitiveEvidenceLog, IntentId, LatencyTier, ModelBackendKind, ModelId, ModelRouter,
    PrivacyClass, ProviderClass, ProviderDispatcher, RouterState, RoutingInputs, SubjectRef,
    TranslationContext, AIOS_COGNITIVE_SUBJECT,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn signing_key() -> SigningKey {
    SigningKey::generate(&mut OsRng)
}

fn test_subject() -> CognitiveSubjectRef {
    CognitiveSubjectRef(AIOS_COGNITIVE_SUBJECT.to_string())
}

fn emitter_with_log() -> (
    CognitiveEvidenceEmitter,
    Arc<InMemoryCognitiveEvidenceLog>,
    SigningKey,
) {
    let sk = signing_key();
    let log = Arc::new(InMemoryCognitiveEvidenceLog::new());
    let emitter = CognitiveEvidenceEmitter::new(log.clone(), sk.clone(), test_subject());
    (emitter, log, sk)
}

fn default_breaker() -> CircuitBreaker {
    CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default())
}

fn default_router() -> ModelRouter {
    ModelRouter::new_with_defaults()
}

fn stub_backend_stats() -> aios_cognitive::CircuitBreakerStats {
    aios_cognitive::CircuitBreakerStats {
        state: CircuitState::Closed,
        success_count: 0,
        failure_count: 0,
        error_rate: 0.0,
        cooldown_seconds: 0,
        last_state_change_at: Utc::now(),
        next_probe_at: None,
    }
}

fn test_subject_ref() -> SubjectRef {
    SubjectRef("_system:test:evidence".into())
}

fn default_translation_context() -> TranslationContext {
    TranslationContext {
        subject: test_subject_ref(),
        available_models: vec![],
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Internal,
        ai_cross_origin_posture: AICrossOriginPosture::AiVaultBrokeredOnly,
        recovery_mode: false,
        budget_ok: true,
    }
}

fn default_intent() -> CognitiveIntent {
    CognitiveIntent {
        intent_id: IntentId::new(),
        subject: test_subject_ref(),
        natural_language: "test evidence emission".into(),
        context_hash: String::new(),
        created_at: Utc::now(),
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Internal,
    }
}

fn healthy_backend_snapshot() -> Vec<BackendHealthEntry> {
    use strum::IntoEnumIterator;
    ModelBackendKind::iter()
        .map(|kind| BackendHealthEntry {
            backend_kind: kind,
            provider_class: ProviderClass::Ollama,
            state: BackendHealthState::Healthy,
            config: CircuitBreakerConfig::default(),
            stats: stub_backend_stats(),
        })
        .collect()
}

fn routing_inputs() -> RoutingInputs {
    RoutingInputs {
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Internal,
        ai_cross_origin_posture: AICrossOriginPosture::AiVaultBrokeredOnly,
        backend_health_snapshot: healthy_backend_snapshot(),
        recovery_mode: false,
        budget_ok: true,
    }
}

// ---------------------------------------------------------------------------
// INV-015 / INV-018 compliance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_015_no_prompt_bodies_in_model_call_evidence() {
    let (emitter, log, _sk) = emitter_with_log();
    emitter
        .emit_model_call("mdl_01", "rtdg_01", 100, 50, 42, 200)
        .await
        .unwrap();
    let receipts = log.receipts().await;
    let payload_str = serde_json::to_string(&receipts[0].payload()).unwrap();
    assert!(
        !payload_str.contains("AIOS_COGNITIVE_SECRET_PROMPT"),
        "INV-015 violation: prompt body leaked into evidence payload"
    );
    assert!(
        !payload_str.contains("prompt"),
        "INV-015 violation: 'prompt' field found in evidence payload"
    );
    assert!(
        !payload_str.contains("response_body"),
        "INV-015 violation: 'response_body' field found in evidence payload"
    );
}

#[tokio::test]
async fn inv_018_no_signing_key_bytes_in_any_emission() {
    let (emitter, log, _sk) = emitter_with_log();
    // Emit all 4 record types
    emitter
        .emit_model_call("m1", "r1", 10, 5, 0, 100)
        .await
        .unwrap();
    emitter
        .emit_routing_decision("r1", ModelBackendKind::LocalGpu, "abc", "1.0")
        .await
        .unwrap();
    emitter
        .emit_circuit_breaker_tripped(
            ModelBackendKind::LocalGpu,
            CircuitState::Closed,
            CircuitState::Open,
            0.15,
        )
        .await
        .unwrap();
    emitter
        .emit_ai_direct_internet_denied("m1", AICrossOriginPosture::AiNoExternal, "test")
        .await
        .unwrap();

    for receipt in log.receipts().await {
        let payload_str = serde_json::to_string(&receipt.payload()).unwrap();
        assert!(
            !payload_str.contains("secret_key"),
            "INV-018 violation: 'secret_key' leaked in payload"
        );
        assert!(
            !payload_str.contains("private_key"),
            "INV-018 violation: 'private_key' leaked in payload"
        );
        assert!(
            !payload_str.contains("signing_key"),
            "INV-018 violation: 'signing_key' leaked in payload"
        );
    }
}

// ---------------------------------------------------------------------------
// BLAKE3 chain coherence
// ---------------------------------------------------------------------------

#[tokio::test]
async fn blake3_chain_coherence_across_ten_emissions() {
    let (emitter, log, sk) = emitter_with_log();
    let vk = sk.verifying_key();

    for i in 0u32..10 {
        emitter
            .emit_model_call("mdl_01", "rtdg_01", i, i * 2, 0, 50)
            .await
            .unwrap();
    }

    assert_eq!(log.len().await, 10);
    log.verify_integrity().await.unwrap();
    log.verify_integrity_signed(&vk).await.unwrap();
}

#[tokio::test]
async fn chain_integrity_verified_before_any_tamper() {
    let (emitter, log, sk) = emitter_with_log();
    let vk = sk.verifying_key();

    emitter
        .emit_model_call("m1", "r1", 10, 5, 0, 100)
        .await
        .unwrap();
    emitter
        .emit_model_call("m2", "r2", 20, 10, 0, 200)
        .await
        .unwrap();

    // Verify chain integrity — no tampering has occurred.
    log.verify_integrity().await.unwrap();
    log.verify_integrity_signed(&vk).await.unwrap();
}

// ---------------------------------------------------------------------------
// Ed25519 signature verification
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ed25519_all_receipts_signed_and_verifiable() {
    let (emitter, log, sk) = emitter_with_log();
    let vk = sk.verifying_key();

    emitter
        .emit_model_call("m1", "r1", 10, 5, 0, 100)
        .await
        .unwrap();
    emitter
        .emit_routing_decision("r1", ModelBackendKind::LocalGpu, "abc", "1.0")
        .await
        .unwrap();
    emitter
        .emit_circuit_breaker_tripped(
            ModelBackendKind::LocalGpu,
            CircuitState::Closed,
            CircuitState::Open,
            0.15,
        )
        .await
        .unwrap();
    emitter
        .emit_ai_direct_internet_denied("m1", AICrossOriginPosture::AiNoExternal, "test")
        .await
        .unwrap();

    assert_eq!(log.len().await, 4);
    log.verify_integrity_signed(&vk).await.unwrap();
}

#[tokio::test]
async fn signature_fails_with_wrong_verifying_key() {
    let (emitter, log, _sk) = emitter_with_log();
    let wrong_sk = signing_key();
    let bad_vk = wrong_sk.verifying_key();

    emitter
        .emit_model_call("m1", "r1", 10, 5, 0, 100)
        .await
        .unwrap();
    let result = log.verify_integrity_signed(&bad_vk).await;
    assert!(result.is_err(), "wrong verifying key should fail");
}

// ---------------------------------------------------------------------------
// Backward compatibility — emitter = None
// ---------------------------------------------------------------------------

#[tokio::test]
async fn router_without_emitter_still_routes() {
    let router = default_router();
    let inputs = routing_inputs();
    let decision = router.route(&inputs).unwrap();
    assert!(!decision.routing_id.is_empty());
}

#[tokio::test]
async fn circuit_breaker_without_emitter_still_transitions() {
    let br = default_breaker();
    for _ in 0..10 {
        br.record_outcome(false, 500).await;
    }
    assert_eq!(br.current_state().await, CircuitState::Open);
}

#[tokio::test]
async fn provider_dispatcher_without_emitter_still_dispatches() {
    let dispatcher = ProviderDispatcher::new();
    let model = CognitiveModel {
        model_id: ModelId::new(),
        provider: ProviderClass::Ollama,
        capabilities: vec![],
        max_tokens: 4096,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: None,
        created_at: Utc::now(),
    };
    let intent = default_intent();
    let result = dispatcher
        .dispatch_to_provider(&model, &intent, AICrossOriginPosture::AiVaultBrokeredOnly)
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn in_memory_core_without_emitter_still_translates() {
    let core = InMemoryCognitiveCore::new();
    let intent = default_intent();
    let ctx = default_translation_context();
    let result = core.translate_intent(&intent, &ctx).await;
    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// Router evidence emission
// ---------------------------------------------------------------------------

#[tokio::test]
async fn router_emits_routing_decision_when_emitter_configured() {
    let (emitter, log, sk) = emitter_with_log();
    let vk = sk.verifying_key();
    let router = default_router().with_evidence_emitter(Arc::new(emitter));
    let inputs = routing_inputs();

    let decision = router.route(&inputs).unwrap();
    assert!(!decision.routing_id.is_empty());

    // Give the spawned task time to complete.
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let receipts = log.receipts().await;
    assert!(
        !receipts.is_empty(),
        "router should have emitted a ROUTING_DECISION receipt"
    );
    log.verify_integrity_signed(&vk).await.unwrap();
}

// ---------------------------------------------------------------------------
// Circuit breaker evidence emission
// ---------------------------------------------------------------------------

#[tokio::test]
async fn breaker_emits_on_closed_to_open_transition() {
    let (emitter, log, sk) = emitter_with_log();
    let vk = sk.verifying_key();
    let br = default_breaker().with_evidence_emitter(Arc::new(emitter));

    // Push enough failures to trip the breaker.
    for _ in 0..10 {
        br.record_outcome(false, 500).await;
    }

    assert_eq!(br.current_state().await, CircuitState::Open);
    let receipts = log.receipts().await;
    assert!(
        !receipts.is_empty(),
        "breaker should have emitted on Closed→Open"
    );
    log.verify_integrity_signed(&vk).await.unwrap();
}

#[tokio::test]
async fn breaker_emits_on_half_open_to_closed_transition() {
    let (emitter, log, sk) = emitter_with_log();
    let vk = sk.verifying_key();
    let config = CircuitBreakerConfig {
        initial_cooldown_seconds: 0,
        ..CircuitBreakerConfig::default()
    };
    let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, config)
        .with_evidence_emitter(Arc::new(emitter));

    // Force Open → HalfOpen → successfully probe → Closed.
    for _ in 0..10 {
        br.record_outcome(false, 500).await;
    }
    br.record_outcome(true, 50).await; // transition to HalfOpen
    assert_eq!(br.current_state().await, CircuitState::HalfOpen);

    // Successful probe → Closed.
    br.record_probe_outcome(true, 50).await;
    assert_eq!(br.current_state().await, CircuitState::Closed);

    let receipts = log.receipts().await;
    assert!(
        receipts.len() >= 2,
        "expected at least 2 receipts, got {}",
        receipts.len()
    );
    log.verify_integrity_signed(&vk).await.unwrap();
}

#[tokio::test]
async fn breaker_emits_on_half_open_to_open_transition() {
    let (emitter, log, sk) = emitter_with_log();
    let vk = sk.verifying_key();
    let config = CircuitBreakerConfig {
        initial_cooldown_seconds: 0,
        ..CircuitBreakerConfig::default()
    };
    let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, config)
        .with_evidence_emitter(Arc::new(emitter));

    // Force Open.
    for _ in 0..10 {
        br.record_outcome(false, 500).await;
    }
    br.record_outcome(true, 50).await; // HalfOpen
    assert_eq!(br.current_state().await, CircuitState::HalfOpen);

    // Failed probe → re-Open.
    br.record_probe_outcome(false, 500).await;
    assert_eq!(br.current_state().await, CircuitState::Open);

    let receipts = log.receipts().await;
    assert!(
        receipts.len() >= 2,
        "expected at least 2 receipts, got {}",
        receipts.len()
    );
    log.verify_integrity_signed(&vk).await.unwrap();
}

// ---------------------------------------------------------------------------
// Provider dispatcher evidence emission
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispatcher_emits_ai_direct_internet_denied() {
    let (emitter, log, sk) = emitter_with_log();
    let vk = sk.verifying_key();
    let dispatcher = ProviderDispatcher::new().with_evidence_emitter(Arc::new(emitter));

    let model = CognitiveModel {
        model_id: ModelId::new(),
        provider: ProviderClass::Anthropic,
        capabilities: vec![],
        max_tokens: 4096,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: None,
        created_at: Utc::now(),
    };
    let intent = default_intent();

    let result = dispatcher
        .dispatch_to_provider(&model, &intent, AICrossOriginPosture::AiNoExternal)
        .await;

    assert!(matches!(
        result.unwrap_err(),
        CognitiveError::ExternalBackendBlocked { .. }
    ));

    let receipts = log.receipts().await;
    assert!(
        !receipts.is_empty(),
        "dispatcher should have emitted AI_DIRECT_INTERNET_DENIED"
    );
    log.verify_integrity_signed(&vk).await.unwrap();
}

// ---------------------------------------------------------------------------
// End-to-end emission through InMemoryCognitiveCore
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_core_emits_model_call_and_routing_decision() {
    let (emitter, log, sk) = emitter_with_log();
    let vk = sk.verifying_key();

    let router = Arc::new(default_router());
    let router_state = Arc::new(RouterState::new());

    // Seed router state with healthy backends.
    for kind in <ModelBackendKind as strum::IntoEnumIterator>::iter() {
        router_state
            .set_health(kind, BackendHealthState::Healthy)
            .await;
    }

    let provider_dispatcher = Arc::new(ProviderDispatcher::new());

    let core = InMemoryCognitiveCore::new()
        .with_router(router, router_state)
        .with_provider_dispatcher(provider_dispatcher)
        .with_evidence_emitter(Arc::new(emitter));

    let intent = default_intent();
    let ctx = default_translation_context();

    let result = core.translate_intent(&intent, &ctx).await;
    assert!(
        result.is_ok(),
        "translation should succeed: {:?}",
        result.err()
    );

    let receipts = log.receipts().await;
    assert!(
        !receipts.is_empty(),
        "core should have emitted evidence receipts"
    );
    log.verify_integrity_signed(&vk).await.unwrap();
}

// ---------------------------------------------------------------------------
// Concurrent emission
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_emission_does_not_corrupt_chain() {
    let (emitter, log, sk) = emitter_with_log();
    let vk = sk.verifying_key();
    let emitter = Arc::new(emitter);

    let mut handles = Vec::new();
    for i in 0u32..20 {
        let e = Arc::clone(&emitter);
        handles.push(tokio::spawn(async move {
            e.emit_model_call(
                &format!("mdl_{i}"),
                &format!("rtdg_{i}"),
                i,
                i * 2,
                u64::from(i) * 100,
                50,
            )
            .await
            .unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(log.len().await, 20);
    log.verify_integrity().await.unwrap();
    log.verify_integrity_signed(&vk).await.unwrap();
}

#[tokio::test]
async fn concurrent_emission_across_record_types() {
    let (emitter, log, sk) = emitter_with_log();
    let vk = sk.verifying_key();
    let emitter = Arc::new(emitter);

    let e1 = Arc::clone(&emitter);
    let h1 = tokio::spawn(async move {
        for _ in 0..5 {
            e1.emit_model_call("m1", "r1", 10, 5, 0, 100).await.unwrap();
        }
    });

    let e2 = Arc::clone(&emitter);
    let h2 = tokio::spawn(async move {
        for _ in 0..5 {
            e2.emit_routing_decision("r1", ModelBackendKind::LocalGpu, "h1", "v1")
                .await
                .unwrap();
        }
    });

    let e3 = Arc::clone(&emitter);
    let h3 = tokio::spawn(async move {
        for _ in 0..5 {
            e3.emit_circuit_breaker_tripped(
                ModelBackendKind::LocalGpu,
                CircuitState::Closed,
                CircuitState::Open,
                0.1,
            )
            .await
            .unwrap();
        }
    });

    h1.await.unwrap();
    h2.await.unwrap();
    h3.await.unwrap();

    assert_eq!(log.len().await, 15);
    log.verify_integrity_signed(&vk).await.unwrap();
}

// ---------------------------------------------------------------------------
// Emitter standalone (no component wiring)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emitter_standalone_all_four_record_types_work() {
    let (emitter, log, sk) = emitter_with_log();
    let vk = sk.verifying_key();

    let id1 = emitter
        .emit_model_call("mdl_a", "rtdg_a", 10, 20, 100, 50)
        .await
        .unwrap();
    assert!(!id1.is_empty());

    let id2 = emitter
        .emit_routing_decision(
            "rtdg_b",
            ModelBackendKind::ExternalVaultBrokered,
            "hash_x",
            "2.0",
        )
        .await
        .unwrap();
    assert!(!id2.is_empty());

    let id3 = emitter
        .emit_circuit_breaker_tripped(
            ModelBackendKind::LocalCpu,
            CircuitState::Closed,
            CircuitState::Open,
            0.25,
        )
        .await
        .unwrap();
    assert!(!id3.is_empty());

    let id4 = emitter
        .emit_ai_direct_internet_denied("mdl_c", AICrossOriginPosture::AiNoExternal, "blocked")
        .await
        .unwrap();
    assert!(!id4.is_empty());

    assert_eq!(log.len().await, 4);
    log.verify_integrity_signed(&vk).await.unwrap();
}
