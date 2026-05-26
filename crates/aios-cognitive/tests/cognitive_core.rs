//! T-095 tests for `CognitiveCore` trait + `InMemoryCognitiveCore` implementation.
//!
//! Minimum 12 tests covering the full trait surface, determinism, INV-002 enforcement,
//! concurrent access, and serde round-trips.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::Utc;

use aios_cognitive::{
    AICrossOriginPosture, CognitiveCore, CognitiveError, CognitiveIntent, CognitiveModel,
    InMemoryCognitiveCore, IntentCapability, IntentId, LatencyTier, ModelId, PrivacyClass,
    ProviderClass, SubjectRef, TranslationContext,
};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn make_intent(privacy: PrivacyClass) -> CognitiveIntent {
    CognitiveIntent {
        intent_id: IntentId::new(),
        subject: SubjectRef("agent:dev".into()),
        natural_language: "restart nginx".into(),
        context_hash: "0000000000000000000000000000000000000000000000000000000000000000".into(),
        created_at: Utc::now(),
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: privacy,
    }
}

fn make_context() -> TranslationContext {
    TranslationContext {
        subject: SubjectRef("agent:dev".into()),
        available_models: vec![],
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Public,
        ai_cross_origin_posture: AICrossOriginPosture::AiVaultBrokeredOnly,
        recovery_mode: false,
        budget_ok: true,
    }
}

// ---------------------------------------------------------------------------
// 1. new — InMemoryCognitiveCore::new() works
// ---------------------------------------------------------------------------

#[test]
fn new_creates_empty_core() {
    let core = InMemoryCognitiveCore::new();
    let intents = core.list_supported_intents();
    assert!(!intents.is_empty(), "must have supported intents");
}

// ---------------------------------------------------------------------------
// 2. valid_intent — translate_intent returns Ok
// ---------------------------------------------------------------------------

#[tokio::test]
async fn translate_valid_intent_returns_ok() {
    let core = InMemoryCognitiveCore::new();
    let intent = make_intent(PrivacyClass::Public);
    let ctx = make_context();

    let result = core
        .translate_intent(&intent, &ctx)
        .await
        .expect("translate_intent must succeed");

    assert_eq!(result.intent_id, intent.intent_id);
    assert!(result.routing_decision_id.is_some());
}

// ---------------------------------------------------------------------------
// 3. determinism — same intent + context -> same result id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn translate_intent_is_deterministic() {
    let core = InMemoryCognitiveCore::new();
    let intent = make_intent(PrivacyClass::Public);
    let ctx = make_context();

    let r1 = core
        .translate_intent(&intent, &ctx)
        .await
        .expect("first call must succeed");
    let r2 = core
        .translate_intent(&intent, &ctx)
        .await
        .expect("second call must succeed");

    // Same intent_id, same provenance metadata.
    assert_eq!(r1.intent_id, r2.intent_id);
    assert_eq!(
        r1.translation_provenance.translator_version,
        r2.translation_provenance.translator_version
    );
    assert_eq!(
        r1.translation_provenance.model_used,
        r2.translation_provenance.model_used
    );
}

// ---------------------------------------------------------------------------
// 4. version — translator_version == "0.1.0-T095"
// ---------------------------------------------------------------------------

#[tokio::test]
async fn translation_provenance_has_correct_version() {
    let core = InMemoryCognitiveCore::new();
    let intent = make_intent(PrivacyClass::Public);
    let ctx = make_context();

    let result = core.translate_intent(&intent, &ctx).await.expect("ok");
    assert_eq!(
        result.translation_provenance.translator_version, "0.1.0-T098",
        "translator_version must match T-098"
    );
    assert_eq!(result.translation_provenance.model_used, "localcpu");
    assert_eq!(result.translation_provenance.tokens_in, 0);
    assert_eq!(result.translation_provenance.tokens_out, 0);
    assert!(result
        .translation_provenance
        .model_signed_response
        .is_none());
}

// ---------------------------------------------------------------------------
// 5. INV-002: produced ActionEnvelope has is_ai == true
// ---------------------------------------------------------------------------

#[tokio::test]
async fn produced_action_envelope_has_ai_subject() {
    let core = InMemoryCognitiveCore::new();
    let intent = make_intent(PrivacyClass::Public);
    let ctx = make_context();

    let result = core.translate_intent(&intent, &ctx).await.expect("ok");

    // INV-002: AI proposes, never executes. The envelope must carry is_ai = true.
    assert!(
        result.produced_action.identity.is_ai,
        "INV-002: ActionEnvelope must carry is_ai = true"
    );
    assert_eq!(
        result.produced_action.identity.subject_canonical_id,
        intent.subject.0
    );
}

// ---------------------------------------------------------------------------
// 6. get_translation — retrieve cached translation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_translation_returns_cached_result() {
    let core = InMemoryCognitiveCore::new();
    let intent = make_intent(PrivacyClass::Public);
    let ctx = make_context();

    let produced = core
        .translate_intent(&intent, &ctx)
        .await
        .expect("translate must succeed");

    let retrieved = core
        .get_translation(&intent.intent_id)
        .await
        .expect("get_translation must succeed");

    assert_eq!(retrieved.intent_id, produced.intent_id);
    assert_eq!(
        retrieved.translation_provenance.translator_version,
        produced.translation_provenance.translator_version
    );
}

// ---------------------------------------------------------------------------
// 7. unknown_intent — get_translation on unknown intent_id returns error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_translation_unknown_intent_is_error() {
    let core = InMemoryCognitiveCore::new();
    let unknown_id = IntentId::new();

    let err = core
        .get_translation(&unknown_id)
        .await
        .expect_err("unknown intent must fail");

    assert!(matches!(err, CognitiveError::NoMatchingCapability(_)));
}

// ---------------------------------------------------------------------------
// 8. list_supported_intents — non-empty Vec
// ---------------------------------------------------------------------------

#[test]
fn list_supported_intents_is_non_empty() {
    let core = InMemoryCognitiveCore::new();
    let intents = core.list_supported_intents();
    assert!(
        !intents.is_empty(),
        "must have at least one supported intent"
    );

    // Each intent capability must have the required fields populated.
    for cap in &intents {
        assert!(!cap.intent_kind.is_empty());
        assert!(!cap.description.is_empty());
        assert!(!cap.produces_action_type.is_empty());
        assert!(cap.max_tokens_estimate > 0);
    }
}

// ---------------------------------------------------------------------------
// 9. with_models — pre-populates catalog
// ---------------------------------------------------------------------------

#[test]
fn with_models_populates_catalog() {
    let model = CognitiveModel {
        model_id: ModelId::new(),
        provider: ProviderClass::Anthropic,
        capabilities: vec!["text-generation".into()],
        max_tokens: 200_000,
        input_cost_per_1k: 15,
        output_cost_per_1k: 75,
        vault_capability_id: None,
        created_at: Utc::now(),
    };

    let core = InMemoryCognitiveCore::with_models(vec![model]);
    let intents = core.list_supported_intents();
    assert!(
        !intents.is_empty(),
        "with_models must still support intents"
    );
}

// ---------------------------------------------------------------------------
// 10. serde round-trips — TranslationContext and IntentCapability
// ---------------------------------------------------------------------------

#[test]
fn translation_context_serde_round_trip() {
    let ctx = make_context();

    let json = serde_json::to_string(&ctx).expect("serialize must succeed");
    let reparsed: TranslationContext =
        serde_json::from_str(&json).expect("deserialize must succeed");

    assert_eq!(reparsed.subject, ctx.subject);
    assert_eq!(reparsed.latency_class, ctx.latency_class);
    assert_eq!(reparsed.privacy_class, ctx.privacy_class);
    assert_eq!(reparsed.recovery_mode, ctx.recovery_mode);
    assert_eq!(reparsed.budget_ok, ctx.budget_ok);
}

#[test]
fn intent_capability_serde_round_trip() {
    let cap = IntentCapability {
        intent_kind: "service.restart".into(),
        description: "Restart a service".into(),
        requires_latency_tier: LatencyTier::T2CatalogRetrieval,
        produces_action_type: "service.restart".into(),
        max_tokens_estimate: 1024,
    };

    let json = serde_json::to_string(&cap).expect("serialize must succeed");
    let reparsed: IntentCapability = serde_json::from_str(&json).expect("deserialize must succeed");

    assert_eq!(reparsed.intent_kind, cap.intent_kind);
    assert_eq!(reparsed.description, cap.description);
    assert_eq!(reparsed.requires_latency_tier, cap.requires_latency_tier);
    assert_eq!(reparsed.produces_action_type, cap.produces_action_type);
    assert_eq!(reparsed.max_tokens_estimate, cap.max_tokens_estimate);
}

// ---------------------------------------------------------------------------
// 11. trait_dyn — CognitiveCore usable as Arc<dyn CognitiveCore>
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cognitive_core_trait_dyn_works() {
    let core: Arc<dyn CognitiveCore> = Arc::new(InMemoryCognitiveCore::new());
    let intent = make_intent(PrivacyClass::Public);
    let ctx = make_context();

    let result = core
        .translate_intent(&intent, &ctx)
        .await
        .expect("trait object call must succeed");

    assert_eq!(result.intent_id, intent.intent_id);

    let supported = core.list_supported_intents();
    assert!(!supported.is_empty());
}

// ---------------------------------------------------------------------------
// 12. concurrent — 3 concurrent translate_intent calls succeed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_translate_3_tasks_all_succeed() {
    let core = Arc::new(InMemoryCognitiveCore::new());
    let ctx = make_context();

    let mut handles = Vec::new();
    for i in 0..3 {
        let core = Arc::clone(&core);
        let intent = CognitiveIntent {
            intent_id: IntentId::new(),
            subject: SubjectRef(format!("agent:dev{i}")),
            natural_language: format!("task {i}"),
            context_hash: "0000000000000000000000000000000000000000000000000000000000000000".into(),
            created_at: Utc::now(),
            latency_class: LatencyTier::T3LocalCognitive,
            privacy_class: PrivacyClass::Public,
        };
        let ctx = TranslationContext {
            subject: SubjectRef(format!("agent:dev{i}")),
            ..ctx.clone()
        };

        handles.push(tokio::spawn(async move {
            core.translate_intent(&intent, &ctx).await
        }));
    }

    for handle in handles {
        let result = handle
            .await
            .expect("task must not panic")
            .expect("translate must succeed");
        assert!(
            result.produced_action.identity.is_ai,
            "INV-002 must hold in concurrent path"
        );
    }
}

// ---------------------------------------------------------------------------
// 13. privacy_routing — secret-bearing intents route to FallbackRuleBased
// ---------------------------------------------------------------------------

#[tokio::test]
async fn secret_bearing_intent_routes_to_fallback() {
    let core = InMemoryCognitiveCore::new();
    let intent = make_intent(PrivacyClass::SecretBearing);
    let ctx = TranslationContext {
        privacy_class: PrivacyClass::SecretBearing,
        ..make_context()
    };

    let result = core.translate_intent(&intent, &ctx).await.expect("ok");

    // Secret-bearing: routed to FallbackRuleBased, degraded.
    let rid = result.routing_decision_id.expect("must have routing id");
    assert!(rid.starts_with("rtdg_"));
    assert_eq!(
        result.translation_provenance.model_used,
        "fallbackrulebased"
    );
    assert_eq!(result.translation_provenance.tokens_in, 0);
    assert_eq!(result.translation_provenance.tokens_out, 0);
}
