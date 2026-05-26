//! T-094 round-trip + invariant tests for the aios-cognitive skeleton.
//!
//! These tests anchor the constitutional shape of the core types so subsequent
//! tasks cannot silently drift the surface:
//!
//! - `LatencyTier` has exactly 5 variants (S1.2 §3).
//! - `PrivacyClass` has exactly 5 variants (S1.2 §5).
//! - `ModelBackendKind` has exactly 8 variants (S13.2 §4).
//! - `ProviderClass` has exactly 5 variants (S13.2 §5).
//! - `AICrossOriginPosture` has exactly 3 variants (S8.1 §4.9).
//! - `BackendHealthState` has exactly 5 variants (S13.2 §9.1).
//! - `CircuitState` has exactly 3 variants (S14.1 §6).

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use chrono::TimeZone;
use strum::{EnumCount, IntoEnumIterator};

use aios_cognitive::{
    AICrossOriginPosture, BackendHealthState, CircuitBreakerConfig, CircuitState, CognitiveError,
    CognitiveIntent, CognitiveModel, IntentId, LatencyTier, ModelBackendKind, ModelId,
    PrivacyClass, ProviderClass, RoutingDecision, SubjectRef, TranslationProvenance,
    TranslationResult,
};

// ---------------------------------------------------------------------------
// LatencyTier — 5 variants (S1.2 §3)
// ---------------------------------------------------------------------------

#[test]
fn latency_tier_has_exactly_five_variants() {
    assert_eq!(LatencyTier::COUNT, 5, "S1.2 §3: exactly 5 latency tiers");

    assert_eq!(LatencyTier::iter().count(), 5);
}

#[test]
fn latency_tier_all_variants_round_trip_through_serde_json() {
    // Anchor the wire form in SCREAMING_SNAKE_CASE per S1.2.
    let expected = [
        (LatencyTier::T0CachedUiState, "\"T0_CACHED_UI_STATE\""),
        (LatencyTier::T1Deterministic, "\"T1_DETERMINISTIC\""),
        (LatencyTier::T2CatalogRetrieval, "\"T2_CATALOG_RETRIEVAL\""),
        (LatencyTier::T3LocalCognitive, "\"T3_LOCAL_COGNITIVE\""),
        (
            LatencyTier::T4PowerfulReasoning,
            "\"T4_POWERFUL_REASONING\"",
        ),
    ];

    assert_eq!(expected.len(), 5);

    for (variant, wire) in expected {
        let s = serde_json::to_string(&variant).expect("serialise LatencyTier");
        assert_eq!(s, wire, "wire form mismatch for {variant:?}");
        let back: LatencyTier = serde_json::from_str(wire).expect("deserialise LatencyTier");
        assert_eq!(variant, back, "round-trip for {variant:?}");
    }
}

// ---------------------------------------------------------------------------
// PrivacyClass — 5 variants (S1.2 §5)
// ---------------------------------------------------------------------------

#[test]
fn privacy_class_has_exactly_five_variants() {
    assert_eq!(PrivacyClass::COUNT, 5, "S1.2 §5: exactly 5 privacy classes");

    assert_eq!(PrivacyClass::iter().count(), 5);
}

#[test]
fn privacy_class_all_variants_round_trip_through_serde_json() {
    let expected = [
        (PrivacyClass::Public, "\"PUBLIC\""),
        (PrivacyClass::Internal, "\"INTERNAL\""),
        (PrivacyClass::Sensitive, "\"SENSITIVE\""),
        (PrivacyClass::SecretBearing, "\"SECRET_BEARING\""),
        (PrivacyClass::Classified, "\"CLASSIFIED\""),
    ];

    for (variant, wire) in expected {
        let s = serde_json::to_string(&variant).expect("serialise PrivacyClass");
        assert_eq!(s, wire, "wire form mismatch for {variant:?}");
        let back: PrivacyClass = serde_json::from_str(wire).expect("deserialise PrivacyClass");
        assert_eq!(variant, back, "round-trip for {variant:?}");
    }
}

// ---------------------------------------------------------------------------
// ModelBackendKind — 8 variants (S13.2 §4), NOT 5 per brief
// ---------------------------------------------------------------------------

#[test]
fn model_backend_kind_has_exactly_eight_variants() {
    assert_eq!(
        ModelBackendKind::COUNT,
        8,
        "S13.2 §4: exactly 8 backend kinds (TRUST SPEC — brief said 5)"
    );

    assert_eq!(ModelBackendKind::iter().count(), 8);
}

#[test]
fn model_backend_kind_all_variants_round_trip_through_serde_json() {
    let expected = [
        (ModelBackendKind::LocalCpu, "\"LOCAL_CPU\""),
        (ModelBackendKind::LocalGpu, "\"LOCAL_GPU\""),
        (ModelBackendKind::LocalDistributed, "\"LOCAL_DISTRIBUTED\""),
        (
            ModelBackendKind::ExternalVaultBrokered,
            "\"EXTERNAL_VAULT_BROKERED\"",
        ),
        (
            ModelBackendKind::FallbackRuleBased,
            "\"FALLBACK_RULE_BASED\"",
        ),
        (ModelBackendKind::Cached, "\"CACHED\""),
        (ModelBackendKind::DegradedNull, "\"DEGRADED_NULL\""),
        (ModelBackendKind::Forbidden, "\"FORBIDDEN\""),
    ];

    assert_eq!(expected.len(), 8);

    for (variant, wire) in expected {
        let s = serde_json::to_string(&variant).expect("serialise ModelBackendKind");
        assert_eq!(s, wire, "wire form mismatch for {variant:?}");
        let back: ModelBackendKind =
            serde_json::from_str(wire).expect("deserialise ModelBackendKind");
        assert_eq!(variant, back, "round-trip for {variant:?}");
    }
}

// ---------------------------------------------------------------------------
// ProviderClass — 5 variants (S13.2 §5)
// ---------------------------------------------------------------------------

#[test]
fn provider_class_has_exactly_five_variants() {
    assert_eq!(
        ProviderClass::COUNT,
        5,
        "S13.2 §5: exactly 5 provider classes"
    );

    assert_eq!(ProviderClass::iter().count(), 5);
}

#[test]
fn provider_class_all_variants_round_trip_through_serde_json() {
    let expected = [
        (ProviderClass::Anthropic, "\"ANTHROPIC\""),
        (ProviderClass::Openai, "\"OPENAI\""),
        (ProviderClass::Ollama, "\"OLLAMA\""),
        (ProviderClass::Vllm, "\"VLLM\""),
        (
            ProviderClass::OtherVaultBrokered,
            "\"OTHER_VAULT_BROKERED\"",
        ),
    ];

    for (variant, wire) in expected {
        let s = serde_json::to_string(&variant).expect("serialise ProviderClass");
        assert_eq!(s, wire, "wire form mismatch for {variant:?}");
        let back: ProviderClass = serde_json::from_str(wire).expect("deserialise ProviderClass");
        assert_eq!(variant, back, "round-trip for {variant:?}");
    }
}

// ---------------------------------------------------------------------------
// AICrossOriginPosture — 3 variants (S8.1 §4.9)
// ---------------------------------------------------------------------------

#[test]
fn ai_cross_origin_posture_has_exactly_three_variants() {
    assert_eq!(
        AICrossOriginPosture::COUNT,
        3,
        "S8.1 §4.9: exactly 3 AI cross-origin postures"
    );

    assert_eq!(AICrossOriginPosture::iter().count(), 3);
}

#[test]
fn ai_cross_origin_posture_all_variants_round_trip_through_serde_json() {
    let expected = [
        (
            AICrossOriginPosture::AiVaultBrokeredOnly,
            "\"AI_VAULT_BROKERED_ONLY\"",
        ),
        (AICrossOriginPosture::AiNoExternal, "\"AI_NO_EXTERNAL\""),
        (AICrossOriginPosture::AiLoopbackOnly, "\"AI_LOOPBACK_ONLY\""),
    ];

    for (variant, wire) in expected {
        let s = serde_json::to_string(&variant).expect("serialise AICrossOriginPosture");
        assert_eq!(s, wire, "wire form mismatch for {variant:?}");
        let back: AICrossOriginPosture =
            serde_json::from_str(wire).expect("deserialise AICrossOriginPosture");
        assert_eq!(variant, back, "round-trip for {variant:?}");
    }
}

// ---------------------------------------------------------------------------
// BackendHealthState — 5 variants (S13.2 §9.1), NOT 4 per brief
// ---------------------------------------------------------------------------

#[test]
fn backend_health_state_has_exactly_five_variants() {
    assert_eq!(
        BackendHealthState::COUNT,
        5,
        "S13.2 §9.1: exactly 5 health states (TRUST SPEC — brief said 4)"
    );

    assert_eq!(BackendHealthState::iter().count(), 5);
}

#[test]
fn backend_health_state_all_variants_round_trip_through_serde_json() {
    let expected = [
        (BackendHealthState::Healthy, "\"HEALTHY\""),
        (BackendHealthState::DegradedLatency, "\"DEGRADED_LATENCY\""),
        (
            BackendHealthState::DegradedAvailability,
            "\"DEGRADED_AVAILABILITY\"",
        ),
        (BackendHealthState::Unhealthy, "\"UNHEALTHY\""),
        (BackendHealthState::Suspended, "\"SUSPENDED\""),
    ];

    assert_eq!(expected.len(), 5);

    for (variant, wire) in expected {
        let s = serde_json::to_string(&variant).expect("serialise BackendHealthState");
        assert_eq!(s, wire, "wire form mismatch for {variant:?}");
        let back: BackendHealthState =
            serde_json::from_str(wire).expect("deserialise BackendHealthState");
        assert_eq!(variant, back, "round-trip for {variant:?}");
    }
}

// ---------------------------------------------------------------------------
// CircuitState — 3 variants (S14.1 §6)
// ---------------------------------------------------------------------------

#[test]
fn circuit_state_has_exactly_three_variants() {
    assert_eq!(CircuitState::COUNT, 3, "S14.1 §6: exactly 3 circuit states");

    assert_eq!(CircuitState::iter().count(), 3);
}

#[test]
fn circuit_state_all_variants_round_trip_through_serde_json() {
    // CircuitState uses PascalCase serde form.
    let expected = [
        (CircuitState::Closed, "\"Closed\""),
        (CircuitState::Open, "\"Open\""),
        (CircuitState::HalfOpen, "\"HalfOpen\""),
    ];

    for (variant, wire) in expected {
        let s = serde_json::to_string(&variant).expect("serialise CircuitState");
        assert_eq!(s, wire, "wire form mismatch for {variant:?}");
        let back: CircuitState = serde_json::from_str(wire).expect("deserialise CircuitState");
        assert_eq!(variant, back, "round-trip for {variant:?}");
    }
}

// ---------------------------------------------------------------------------
// CircuitBreakerConfig — default values round-trip
// ---------------------------------------------------------------------------

#[test]
fn circuit_breaker_config_defaults_round_trip() {
    let config = CircuitBreakerConfig::default();
    assert!((config.error_rate_threshold - 0.05).abs() < f64::EPSILON);
    assert_eq!(config.window_seconds, 300);
    assert_eq!(config.initial_cooldown_seconds, 30);
    assert_eq!(config.max_cooldown_seconds, 600);

    let json = serde_json::to_string(&config).expect("serialise CircuitBreakerConfig");
    let back: CircuitBreakerConfig =
        serde_json::from_str(&json).expect("deserialise CircuitBreakerConfig");
    assert!((config.error_rate_threshold - back.error_rate_threshold).abs() < f64::EPSILON);
    assert_eq!(config.window_seconds, back.window_seconds);
    assert_eq!(
        config.initial_cooldown_seconds,
        back.initial_cooldown_seconds
    );
    assert_eq!(config.max_cooldown_seconds, back.max_cooldown_seconds);

    assert!(json.contains("\"error_rate_threshold\":0.05"));
    assert!(json.contains("\"window_seconds\":300"));
}

// ---------------------------------------------------------------------------
// CognitiveIntent — full round-trip
// ---------------------------------------------------------------------------

#[test]
fn cognitive_intent_round_trips_through_serde_json() {
    let intent = CognitiveIntent {
        intent_id: IntentId::new(),
        subject: SubjectRef("agent:dev:01HX0000000000000000000000".to_string()),
        natural_language: "restart the nginx service".to_string(),
        context_hash: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            .to_string(),
        created_at: chrono::Utc
            .with_ymd_and_hms(2026, 5, 25, 12, 0, 0)
            .single()
            .expect("fixture timestamp is valid"),
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Internal,
    };

    let json = serde_json::to_string(&intent).expect("serialise CognitiveIntent");
    let back: CognitiveIntent = serde_json::from_str(&json).expect("deserialise CognitiveIntent");
    assert_eq!(intent.intent_id, back.intent_id);
    assert_eq!(intent.subject, back.subject);
    assert_eq!(intent.natural_language, back.natural_language);
    assert_eq!(intent.context_hash, back.context_hash);
    assert_eq!(intent.latency_class, back.latency_class);
    assert_eq!(intent.privacy_class, back.privacy_class);

    // Anchor the intent_id prefix.
    assert!(
        json.contains("cogi_"),
        "IntentId must use `cogi_` ULID prefix"
    );
}

// ---------------------------------------------------------------------------
// TranslationProvenance — full round-trip
// ---------------------------------------------------------------------------

#[test]
fn translation_provenance_round_trips_through_serde_json() {
    let provenance = TranslationProvenance {
        translator_version: "aios-cognitive/0.1.0-T099".to_string(),
        model_used: "claude-sonnet-4-6".to_string(),
        tokens_in: 1234,
        tokens_out: 567,
        model_signed_response: Some("raw model output here".to_string()),
    };

    let json = serde_json::to_string(&provenance).expect("serialise TranslationProvenance");
    let back: TranslationProvenance =
        serde_json::from_str(&json).expect("deserialise TranslationProvenance");
    assert_eq!(provenance.translator_version, back.translator_version);
    assert_eq!(provenance.model_used, back.model_used);
    assert_eq!(provenance.tokens_in, back.tokens_in);
    assert_eq!(provenance.tokens_out, back.tokens_out);
    assert_eq!(provenance.model_signed_response, back.model_signed_response);
}

// ---------------------------------------------------------------------------
// TranslationResult — full round-trip
// ---------------------------------------------------------------------------

#[test]
fn translation_result_round_trips_through_serde_json() {
    use aios_action::{ActionEnvelope, Identity, Request, Trace};

    let envelope = ActionEnvelope::new(
        Identity {
            subject_canonical_id: "agent:dev:01HX0000000000000000000000".to_string(),
            is_ai: true,
            session_id: None,
        },
        Request {
            action: "service.restart".to_string(),
            target: serde_json::json!({"service": "nginx"}),
            idempotency_key: None,
            parent_action_id: None,
            dry_run: aios_action::DryRunMode::default(),
        },
        Trace {
            trace_id: "0123456789abcdef0123456789abcdef".to_string(),
            span_id: "0123456789abcdef".to_string(),
            parent_span_id: None,
        },
    );

    let result = TranslationResult {
        intent_id: IntentId::new(),
        produced_action: envelope,
        routing_decision_id: Some("rtdg_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_string()),
        verification_intent: Some("verify nginx is running".to_string()),
        translation_provenance: TranslationProvenance {
            translator_version: "aios-cognitive/0.1.0-T099".to_string(),
            model_used: "claude-sonnet-4-6".to_string(),
            tokens_in: 500,
            tokens_out: 200,
            model_signed_response: None,
        },
        translated_at: chrono::Utc
            .with_ymd_and_hms(2026, 5, 25, 12, 0, 1)
            .single()
            .expect("fixture timestamp is valid"),
    };

    let json = serde_json::to_string(&result).expect("serialise TranslationResult");
    let back: TranslationResult =
        serde_json::from_str(&json).expect("deserialise TranslationResult");
    assert_eq!(result.intent_id, back.intent_id);
    assert_eq!(result.routing_decision_id, back.routing_decision_id);
    assert_eq!(result.verification_intent, back.verification_intent);
    assert_eq!(result.translation_provenance, back.translation_provenance);
    // ActionEnvelope round-trip fields checked structurally.
    assert_eq!(
        result.produced_action.identity.subject_canonical_id,
        back.produced_action.identity.subject_canonical_id
    );
    assert_eq!(
        result.produced_action.request.action,
        back.produced_action.request.action
    );
}

// ---------------------------------------------------------------------------
// RoutingDecision — full round-trip
// ---------------------------------------------------------------------------

#[test]
fn routing_decision_round_trips_through_serde_json() {
    let decision = RoutingDecision {
        routing_id: "rtdg_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_string(),
        chosen_backend: ModelBackendKind::LocalGpu,
        provider_class: ProviderClass::Ollama,
        backend_id: "adapter_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_string(),
        matched_rule: 7,
        degraded: false,
        reason: None,
        decided_at: chrono::Utc
            .with_ymd_and_hms(2026, 5, 25, 12, 0, 0)
            .single()
            .expect("fixture timestamp is valid"),
    };

    let json = serde_json::to_string(&decision).expect("serialise RoutingDecision");
    let back: RoutingDecision = serde_json::from_str(&json).expect("deserialise RoutingDecision");
    assert_eq!(decision.routing_id, back.routing_id);
    assert_eq!(decision.chosen_backend, back.chosen_backend);
    assert_eq!(decision.provider_class, back.provider_class);
    assert_eq!(decision.backend_id, back.backend_id);
    assert_eq!(decision.matched_rule, back.matched_rule);
    assert_eq!(decision.degraded, back.degraded);
    assert_eq!(decision.reason, back.reason);

    assert!(json.contains("rtdg_"), "routing_id must use `rtdg_` prefix");
    assert!(json.contains("\"matched_rule\":7"));
}

// ---------------------------------------------------------------------------
// CognitiveModel — full round-trip
// ---------------------------------------------------------------------------

#[test]
fn cognitive_model_round_trips_through_serde_json() {
    let model = CognitiveModel {
        model_id: ModelId::new(),
        provider: ProviderClass::Anthropic,
        capabilities: vec!["text-generation".to_string(), "code-completion".to_string()],
        max_tokens: 200_000,
        input_cost_per_1k: 15,
        output_cost_per_1k: 75,
        vault_capability_id: Some("vcap_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_string()),
        created_at: chrono::Utc
            .with_ymd_and_hms(2026, 5, 25, 12, 0, 0)
            .single()
            .expect("fixture timestamp is valid"),
    };

    let json = serde_json::to_string(&model).expect("serialise CognitiveModel");
    let back: CognitiveModel = serde_json::from_str(&json).expect("deserialise CognitiveModel");
    assert_eq!(model.model_id, back.model_id);
    assert_eq!(model.provider, back.provider);
    assert_eq!(model.capabilities, back.capabilities);
    assert_eq!(model.max_tokens, back.max_tokens);
    assert_eq!(model.input_cost_per_1k, back.input_cost_per_1k);
    assert_eq!(model.output_cost_per_1k, back.output_cost_per_1k);
    assert_eq!(model.vault_capability_id, back.vault_capability_id);

    assert!(json.contains("mdl_"), "ModelId must use `mdl_` ULID prefix");
}

// ---------------------------------------------------------------------------
// IntentId / ModelId newtype round-trips
// ---------------------------------------------------------------------------

#[test]
fn intent_id_newtype_round_trips_through_serde_json() {
    let id = IntentId::new();
    assert!(
        id.0.starts_with("cogi_"),
        "IntentId must start with `cogi_`"
    );

    let json = serde_json::to_string(&id).expect("serialise IntentId");
    let back: IntentId = serde_json::from_str(&json).expect("deserialise IntentId");
    assert_eq!(id, back);

    // Transparent wire form: just the string.
    assert!(json.starts_with("\"cogi_"));
    assert!(json.ends_with('"'));
}

#[test]
fn model_id_newtype_round_trips_through_serde_json() {
    let id = ModelId::new();
    assert!(id.0.starts_with("mdl_"), "ModelId must start with `mdl_`");

    let json = serde_json::to_string(&id).expect("serialise ModelId");
    let back: ModelId = serde_json::from_str(&json).expect("deserialise ModelId");
    assert_eq!(id, back);

    assert!(json.starts_with("\"mdl_"));
    assert!(json.ends_with('"'));
}

// ---------------------------------------------------------------------------
// CognitiveError — canonical Display strings
// ---------------------------------------------------------------------------

#[test]
fn cognitive_error_display_strings_match_canonical_text() {
    assert_eq!(
        CognitiveError::IntentParseFailed("malformed input".to_string()).to_string(),
        "intent parse failed: malformed input"
    );
    assert_eq!(
        CognitiveError::NoMatchingCapability("unknown.verb".to_string()).to_string(),
        "no matching capability for intent: unknown.verb"
    );
    assert_eq!(
        CognitiveError::TranslationRefused("action would delete recovery path".to_string())
            .to_string(),
        "translation refused: action would delete recovery path"
    );
    assert_eq!(
        CognitiveError::AmbiguousIntent("two capabilities match service.restart".to_string())
            .to_string(),
        "ambiguous intent: two capabilities match service.restart"
    );
    assert_eq!(
        CognitiveError::LatencyPrivacyConflict(
            "T4_POWERFUL_REASONING with SECRET_BEARING".to_string()
        )
        .to_string(),
        "latency tier incompatible with privacy class: T4_POWERFUL_REASONING with SECRET_BEARING"
    );
    assert_eq!(
        CognitiveError::NoRouteAvailable("all backends unhealthy".to_string()).to_string(),
        "no route available: all backends unhealthy"
    );
    assert_eq!(
        CognitiveError::CircuitBreakerOpen("LocalGpu".to_string()).to_string(),
        "circuit breaker open for backend: LocalGpu"
    );
    assert_eq!(
        CognitiveError::ModelResponseInvalid("missing required field `action`".to_string())
            .to_string(),
        "model response invalid: missing required field `action`"
    );
    assert_eq!(
        CognitiveError::Internal("assertion failed: x > 0".to_string()).to_string(),
        "internal cognitive error: assertion failed: x > 0"
    );
}
