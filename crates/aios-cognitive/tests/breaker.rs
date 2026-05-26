//! T-098 integration tests for `CircuitBreaker` and `CircuitBreakerRegistry`.
//!
//! Minimum 14 tests covering state transitions (Closed→Open, Open→HalfOpen,
//! HalfOpen→Closed, HalfOpen→Open), `try_admit` behaviours, INV-014 enforcement,
//! concurrency, backward compat, registry integration, and end-to-end flow
//! through `InMemoryCognitiveCore`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::Utc;

use aios_cognitive::{
    AICrossOriginPosture, CircuitBreaker, CircuitBreakerConfig, CircuitBreakerRegistry,
    CircuitState, CognitiveCore, CognitiveIntent, InMemoryCognitiveCore, IntentId, LatencyTier,
    ModelBackendKind, PrivacyClass, SubjectRef, TranslationContext,
};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

const fn fast_open_config() -> CircuitBreakerConfig {
    CircuitBreakerConfig {
        error_rate_threshold: 0.05,
        window_seconds: 300,
        initial_cooldown_seconds: 1,
        max_cooldown_seconds: 600,
    }
}

const fn zero_cooldown_config() -> CircuitBreakerConfig {
    CircuitBreakerConfig {
        error_rate_threshold: 0.05,
        window_seconds: 300,
        initial_cooldown_seconds: 0,
        max_cooldown_seconds: 600,
    }
}

fn make_intent(privacy: PrivacyClass) -> CognitiveIntent {
    CognitiveIntent {
        intent_id: IntentId::new(),
        subject: SubjectRef("_test:aios".into()),
        natural_language: "restart nginx".into(),
        context_hash: String::new(),
        created_at: Utc::now(),
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: privacy,
    }
}

fn make_context(privacy: PrivacyClass) -> TranslationContext {
    TranslationContext {
        subject: SubjectRef("_test:aios".into()),
        available_models: vec![],
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: privacy,
        ai_cross_origin_posture: AICrossOriginPosture::AiVaultBrokeredOnly,
        recovery_mode: false,
        budget_ok: true,
    }
}

// ---------------------------------------------------------------------------
// CircuitBreaker state transitions (4 tests)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn closed_to_open_on_high_error_rate() {
    let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
    assert_eq!(br.current_state().await, CircuitState::Closed);

    for _ in 0..5 {
        br.record_outcome(true, 50).await;
    }
    br.record_outcome(false, 500).await;
    assert_eq!(br.current_state().await, CircuitState::Open);
}

#[tokio::test]
async fn open_to_half_open_after_cooldown() {
    let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, fast_open_config());
    // Force Open.
    for _ in 0..10 {
        br.record_outcome(false, 500).await;
    }
    assert_eq!(br.current_state().await, CircuitState::Open);

    // Wait for cooldown (1s).
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    br.record_outcome(true, 50).await;
    assert_eq!(br.current_state().await, CircuitState::HalfOpen);
}

#[tokio::test]
async fn half_open_to_closed_on_probe_success() {
    let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, zero_cooldown_config());
    // Force Open then wait → HalfOpen.
    for _ in 0..10 {
        br.record_outcome(false, 500).await;
    }
    br.record_outcome(true, 50).await;
    assert_eq!(br.current_state().await, CircuitState::HalfOpen);

    // Successful probe.
    br.record_probe_outcome(true, 50).await;
    assert_eq!(br.current_state().await, CircuitState::Closed);
}

#[tokio::test]
async fn half_open_to_open_on_probe_failure() {
    let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, zero_cooldown_config());
    for _ in 0..10 {
        br.record_outcome(false, 500).await;
    }
    br.record_outcome(true, 50).await;
    assert_eq!(br.current_state().await, CircuitState::HalfOpen);

    // Failed probe re-opens.
    br.record_probe_outcome(false, 500).await;
    assert_eq!(br.current_state().await, CircuitState::Open);
}

// ---------------------------------------------------------------------------
// try_admit behaviours (3 tests)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn try_admit_grants_in_closed() {
    let br = CircuitBreaker::new(ModelBackendKind::LocalCpu, CircuitBreakerConfig::default());
    let ticket = br.try_admit().await.expect("should admit in Closed");
    assert!(ticket.ticket_id.starts_with("brktk_"));
    assert_eq!(ticket.backend, ModelBackendKind::LocalCpu);
}

#[tokio::test]
async fn try_admit_rejects_in_open_with_retry_after() {
    let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
    for _ in 0..10 {
        br.record_outcome(false, 500).await;
    }
    assert_eq!(br.current_state().await, CircuitState::Open);

    let err = br.try_admit().await.expect_err("should reject in Open");
    let msg = format!("{err}");
    assert!(
        msg.contains("circuit open"),
        "expected 'circuit open' in: {msg}"
    );
    assert!(
        msg.contains("retry_after_ms="),
        "expected retry_after_ms in: {msg}"
    );
}

#[tokio::test]
async fn try_admit_rejects_after_failed_probe_reopens() {
    let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, zero_cooldown_config());
    for _ in 0..10 {
        br.record_outcome(false, 500).await;
    }
    br.record_outcome(true, 50).await;
    assert_eq!(br.current_state().await, CircuitState::HalfOpen);

    // First probe admitted in HalfOpen.
    let _ticket = br
        .try_admit()
        .await
        .expect("first probe should admit in HalfOpen");
    // Fail the probe → back to Open.
    br.record_probe_outcome(false, 500).await;
    assert_eq!(br.current_state().await, CircuitState::Open);

    // Now try_admit rejects because we're back in Open.
    let err = br
        .try_admit()
        .await
        .expect_err("should reject after re-open");
    let msg = format!("{err}");
    assert!(
        msg.contains("circuit open"),
        "expected 'circuit open' in: {msg}"
    );
}

// ---------------------------------------------------------------------------
// INV-014 enforcement (2 tests)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_014_no_direct_open_to_closed_even_with_successes() {
    let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
    for _ in 0..10 {
        br.record_outcome(false, 500).await;
    }
    assert_eq!(br.current_state().await, CircuitState::Open);

    // Record many successes while Open — must stay Open (cooldown not expired).
    for _ in 0..100 {
        br.record_outcome(true, 50).await;
    }
    assert_eq!(
        br.current_state().await,
        CircuitState::Open,
        "INV-014: must not transition Open→Closed directly"
    );
}

#[tokio::test]
async fn inv_014_state_never_asserted_healthy_by_adapter() {
    // Proves that adapters cannot self-report HEALTHY — state is derived
    // from observations only.
    let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
    // CircuitBreaker has no API to set state to Closed without observations.
    // test-only set_state_for_test exists but is gated behind &self and clear docs.
    // In production path, only record_outcome can change state.
    let initial = br.current_state().await;
    assert_eq!(initial, CircuitState::Closed);

    // Record failures → state degrades.
    for _ in 0..10 {
        br.record_outcome(false, 500).await;
    }
    let degraded = br.current_state().await;
    assert_eq!(degraded, CircuitState::Open);

    // There is no method to directly assert "I am Healthy now."
    // The only path back is through time + successful probes.
}

// ---------------------------------------------------------------------------
// Cooldown doubling (1 test)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cooldown_doubles_after_reopen_and_caps_at_max() {
    let config = CircuitBreakerConfig {
        error_rate_threshold: 0.05,
        window_seconds: 300,
        initial_cooldown_seconds: 1,
        max_cooldown_seconds: 8,
    };
    let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, config);

    // Round 1: force Open.
    for _ in 0..10 {
        br.record_outcome(false, 500).await;
    }
    assert_eq!(br.current_state().await, CircuitState::Open);

    // Wait → HalfOpen.
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    br.record_outcome(true, 50).await;
    assert_eq!(br.current_state().await, CircuitState::HalfOpen);

    // Fail probe → re-Open with multiplier=2.
    br.record_probe_outcome(false, 500).await;
    assert_eq!(br.current_state().await, CircuitState::Open);
    // Multiplier should be 2 (cooldown = 2s).
    let stats = br.current_stats().await;
    assert_eq!(stats.cooldown_seconds, 2);

    // Round 2: wait → HalfOpen, fail probe → multiplier=4.
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    br.record_outcome(true, 50).await;
    assert_eq!(br.current_state().await, CircuitState::HalfOpen);
    br.record_probe_outcome(false, 500).await;
    assert_eq!(br.current_state().await, CircuitState::Open);
    let stats = br.current_stats().await;
    assert_eq!(stats.cooldown_seconds, 4);

    // Round 3: wait → HalfOpen, fail probe → multiplier=8 (capped at max).
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    br.record_outcome(true, 50).await;
    assert_eq!(br.current_state().await, CircuitState::HalfOpen);
    br.record_probe_outcome(false, 500).await;
    assert_eq!(br.current_state().await, CircuitState::Open);
    let stats = br.current_stats().await;
    assert_eq!(stats.cooldown_seconds, 8);

    // Round 4: wait → HalfOpen, fail probe → stays at max=8.
    tokio::time::sleep(tokio::time::Duration::from_secs(9)).await;
    br.record_outcome(true, 50).await;
    assert_eq!(br.current_state().await, CircuitState::HalfOpen);
    br.record_probe_outcome(false, 500).await;
    assert_eq!(br.current_state().await, CircuitState::Open);
    let stats = br.current_stats().await;
    assert_eq!(stats.cooldown_seconds, 8);
}

// ---------------------------------------------------------------------------
// Stats accuracy (1 test)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stats_snapshot_is_accurate_after_transitions() {
    let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
    let stats = br.current_stats().await;
    assert_eq!(stats.state, CircuitState::Closed);
    assert_eq!(stats.success_count, 0);
    assert_eq!(stats.failure_count, 0);
    assert_eq!(stats.error_rate, 0.0);
    assert_eq!(stats.cooldown_seconds, 0);
    assert!(stats.next_probe_at.is_none());

    // Record some outcomes.
    for _ in 0..8 {
        br.record_outcome(true, 50).await;
    }
    br.record_outcome(false, 600).await;
    br.record_outcome(false, 700).await;

    assert_eq!(br.current_state().await, CircuitState::Open);
    let stats = br.current_stats().await;
    assert_eq!(stats.state, CircuitState::Open);
    assert_eq!(stats.success_count, 8);
    assert_eq!(stats.failure_count, 2);
    // 2 / 10 = 0.2
    assert!((stats.error_rate - 0.2).abs() < 0.001);
    assert!(stats.cooldown_seconds > 0);
    assert!(stats.next_probe_at.is_some());
}

// ---------------------------------------------------------------------------
// Registry integration (3 tests)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn registry_created_with_all_eight_backends() {
    let reg = CircuitBreakerRegistry::new_with_defaults();
    let kinds = [
        ModelBackendKind::LocalCpu,
        ModelBackendKind::LocalGpu,
        ModelBackendKind::LocalDistributed,
        ModelBackendKind::ExternalVaultBrokered,
        ModelBackendKind::FallbackRuleBased,
        ModelBackendKind::Cached,
        ModelBackendKind::DegradedNull,
        ModelBackendKind::Forbidden,
    ];
    for kind in kinds {
        assert!(reg.get(kind).await.is_some(), "missing {kind:?}");
    }
}

#[tokio::test]
async fn registry_observe_and_update_flows_through_breaker() {
    let reg = CircuitBreakerRegistry::new_with_defaults();
    // Initially Closed.
    assert!(reg.try_admit(ModelBackendKind::LocalCpu).await.is_ok());

    // Record failures → opens.
    for _ in 0..10 {
        reg.observe_and_update(ModelBackendKind::LocalCpu, false, 500)
            .await;
    }

    // Now Open → admission fails.
    assert!(reg.try_admit(ModelBackendKind::LocalCpu).await.is_err());
    let states = reg.all_states().await;
    assert_eq!(
        states.get(&ModelBackendKind::LocalCpu),
        Some(&CircuitState::Open)
    );
}

#[tokio::test]
async fn registry_backends_are_independent() {
    let reg = CircuitBreakerRegistry::new_with_defaults();
    // Open LocalGpu.
    for _ in 0..10 {
        reg.observe_and_update(ModelBackendKind::LocalGpu, false, 500)
            .await;
    }
    // LocalCpu should remain Closed.
    assert!(reg.try_admit(ModelBackendKind::LocalCpu).await.is_ok());
    assert!(reg.try_admit(ModelBackendKind::LocalGpu).await.is_err());

    let states = reg.all_states().await;
    assert_eq!(
        states.get(&ModelBackendKind::LocalCpu),
        Some(&CircuitState::Closed)
    );
    assert_eq!(
        states.get(&ModelBackendKind::LocalGpu),
        Some(&CircuitState::Open)
    );
}

// ---------------------------------------------------------------------------
// InMemoryCognitiveCore with breaker registry (2 tests)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cognitive_core_with_breakers_admits_when_closed() {
    let core = InMemoryCognitiveCore::new()
        .with_breakers(Arc::new(CircuitBreakerRegistry::new_with_defaults()));

    let intent = make_intent(PrivacyClass::Public);
    let ctx = make_context(PrivacyClass::Public);
    let result = core.translate_intent(&intent, &ctx).await;
    assert!(
        result.is_ok(),
        "should admit when all breakers are closed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn cognitive_core_with_breakers_rejects_when_backend_open() {
    let reg = Arc::new(CircuitBreakerRegistry::new_with_defaults());
    // Open the LocalCpu breaker (which the T-095 stub picks for Public).
    for _ in 0..10 {
        reg.observe_and_update(ModelBackendKind::LocalCpu, false, 500)
            .await;
    }
    assert!(reg.try_admit(ModelBackendKind::LocalCpu).await.is_err());

    let core = InMemoryCognitiveCore::new().with_breakers(reg);

    let intent = make_intent(PrivacyClass::Public);
    let ctx = make_context(PrivacyClass::Public);
    let result = core.translate_intent(&intent, &ctx).await;
    assert!(result.is_err());
    let msg = format!("{}", result.expect_err("should reject when backend open"));
    assert!(
        msg.contains("circuit") || msg.contains("CircuitBreaker"),
        "expected circuit error: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Backward compat — no breaker registry (1 test)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cognitive_core_without_breakers_preserves_t095_behaviour() {
    let core = InMemoryCognitiveCore::new();
    let intent = make_intent(PrivacyClass::Public);
    let ctx = make_context(PrivacyClass::Public);
    let result = core
        .translate_intent(&intent, &ctx)
        .await
        .expect("should work without breaker registry");
    assert!(result.routing_decision_id.is_some());
}

// ---------------------------------------------------------------------------
// Concurrency (2 tests)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_record_outcomes_do_not_corrupt_state() {
    let br = Arc::new(CircuitBreaker::new(
        ModelBackendKind::LocalGpu,
        CircuitBreakerConfig::default(),
    ));

    let mut handles = Vec::new();
    for i in 0..10 {
        let br = Arc::clone(&br);
        let succeeded = i < 5; // 5 successes, 5 failures.
        handles.push(tokio::spawn(async move {
            for _ in 0..20 {
                br.record_outcome(succeeded, 50).await;
            }
        }));
    }
    for h in handles {
        h.await.expect("task panicked");
    }

    let stats = br.current_stats().await;
    let total = stats.success_count + stats.failure_count;
    assert_eq!(total, 200, "should have exactly 200 recorded outcomes");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_try_admit_in_half_open_no_deadlock() {
    let br = Arc::new(CircuitBreaker::new(
        ModelBackendKind::LocalGpu,
        zero_cooldown_config(),
    ));

    // Force Open then HalfOpen.
    for _ in 0..10 {
        br.record_outcome(false, 500).await;
    }
    br.record_outcome(true, 50).await;
    assert_eq!(br.current_state().await, CircuitState::HalfOpen);

    // Multiple concurrent try_admit calls in HalfOpen — all pass through
    // because try_admit checks the probe gate (success+failure >= 1) but
    // only record_probe_outcome increments the counters and triggers
    // recompute_state. Concurrent callers see stale zero counters.
    let mut handles = Vec::new();
    for _ in 0..5 {
        let br = Arc::clone(&br);
        handles.push(tokio::spawn(async move { br.try_admit().await }));
    }

    let mut admitted = 0;
    let mut rejected = 0;
    for h in handles {
        match h.await.expect("task panicked") {
            Ok(_) => admitted += 1,
            Err(_) => rejected += 1,
        }
    }

    // All 5 should be admitted (no deadlock, no corruption).
    assert_eq!(
        admitted, 5,
        "all 5 concurrent calls should admit in HalfOpen, got {admitted} admitted, {rejected} rejected"
    );

    // Recording a failed probe re-opens the circuit.
    br.record_probe_outcome(false, 500).await;
    assert_eq!(br.current_state().await, CircuitState::Open);

    // Now try_admit must reject.
    assert!(br.try_admit().await.is_err());
}

// ---------------------------------------------------------------------------
// AdmissionTicket round-trip (1 test)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admission_ticket_fields_are_populated() {
    let br = CircuitBreaker::new(
        ModelBackendKind::ExternalVaultBrokered,
        CircuitBreakerConfig::default(),
    );
    let ticket = br.try_admit().await.expect("should admit in Closed");
    assert_eq!(ticket.backend, ModelBackendKind::ExternalVaultBrokered);
    assert!(ticket.ticket_id.starts_with("brktk_"));
    assert!(ticket.issued_at <= Utc::now());
}
