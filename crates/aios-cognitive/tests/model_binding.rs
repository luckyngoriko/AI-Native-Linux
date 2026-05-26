//! T-099 tests for `ModelBinding` + `ModelBindingRegistry` — INV-015 (no prompt/response
//! bodies), INV-018 (vault capability for external providers), and invocation tracking.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_cognitive::{
    CognitiveError, CognitiveModel, CognitiveModelCatalog, ModelBinding, ModelBindingRegistry,
    ModelId, ProviderClass,
};

fn make_ollama_model() -> CognitiveModel {
    CognitiveModel {
        model_id: ModelId::new(),
        provider: ProviderClass::Ollama,
        capabilities: vec!["text-generation".into()],
        max_tokens: 4_096,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: None,
        created_at: chrono::Utc::now(),
    }
}

fn make_anthropic_model() -> CognitiveModel {
    CognitiveModel {
        model_id: ModelId::new(),
        provider: ProviderClass::Anthropic,
        capabilities: vec!["text-generation".into()],
        max_tokens: 200_000,
        input_cost_per_1k: 15,
        output_cost_per_1k: 75,
        vault_capability_id: Some("vcap_test".into()),
        created_at: chrono::Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// 1. ModelBinding::new — local model, no vault → OK
// ---------------------------------------------------------------------------

#[test]
fn binding_new_local_model_without_vault_is_ok() {
    let model = make_ollama_model();
    let binding = ModelBinding::new(model, None).expect("local model must bind without vault");
    assert_eq!(binding.total_calls, 0);
    assert_eq!(binding.total_tokens_used, 0);
    assert_eq!(binding.total_cost_micros, 0);
    assert!(binding.last_used_at.is_none());
    assert!(binding.vault_capability_id.is_none());
}

// ---------------------------------------------------------------------------
// 2. INV-018: external model without vault_capability_id → error
// ---------------------------------------------------------------------------

#[test]
fn binding_new_external_without_vault_is_error() {
    let mut model = make_anthropic_model();
    model.vault_capability_id = None; // strip the vault id
    let err = ModelBinding::new(model, None)
        .expect_err("external model without vault capability must fail");
    assert!(matches!(err, CognitiveError::Internal(_)));
    let msg = err.to_string();
    assert!(
        msg.contains("vault credential required"),
        "error must mention vault credential: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 3. INV-018: external model with vault_capability_id → OK
// ---------------------------------------------------------------------------

#[test]
fn binding_new_external_with_vault_is_ok() {
    let model = make_anthropic_model();
    let binding = ModelBinding::new(model, Some("vcap_test".into()))
        .expect("external model with vault must bind");
    assert!(binding.vault_capability_id.is_some());
    assert_eq!(binding.model.provider, ProviderClass::Anthropic);
}

// ---------------------------------------------------------------------------
// 4. ModelBinding serde round-trip + INV-015: no prompt/response fields
// ---------------------------------------------------------------------------

#[test]
fn binding_serde_round_trip_no_prompt_or_response_fields() {
    let model = make_ollama_model();
    let binding = ModelBinding::new(model, None).expect("bind");

    let json = serde_json::to_string_pretty(&binding).expect("serialize");
    let reparsed: ModelBinding = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(reparsed.model.model_id, binding.model.model_id);
    assert_eq!(reparsed.total_calls, binding.total_calls);
    assert_eq!(reparsed.total_tokens_used, binding.total_tokens_used);

    // INV-015: no prompt or response fields in the JSON.
    let lower = json.to_lowercase();
    assert!(
        !lower.contains("prompt"),
        "INV-015: JSON must not contain 'prompt'"
    );
    assert!(
        !lower.contains("response"),
        "INV-015: JSON must not contain 'response'"
    );
}

// ---------------------------------------------------------------------------
// 5. Registry: bind then get
// ---------------------------------------------------------------------------

#[tokio::test]
async fn registry_bind_then_get() {
    let registry = ModelBindingRegistry::new();
    let model = make_ollama_model();
    let mid = model.model_id.clone();

    registry.bind(model, None).await.expect("bind must succeed");

    let retrieved = registry.get(&mid).await.expect("get must find binding");
    assert_eq!(retrieved.model.model_id, mid);
}

// ---------------------------------------------------------------------------
// 6. Registry: record_invocation updates stats
// ---------------------------------------------------------------------------

#[tokio::test]
async fn record_invocation_updates_stats() {
    let registry = ModelBindingRegistry::new();
    let model = make_ollama_model();
    let mid = model.model_id.clone();
    registry.bind(model, None).await.expect("bind");

    registry.record_invocation(&mid, 100, 50, 1_000).await;
    registry.record_invocation(&mid, 200, 100, 2_000).await;

    let binding = registry.get(&mid).await.expect("get");
    assert_eq!(binding.total_calls, 2);
    assert_eq!(binding.total_tokens_used, 450); // 100+50 + 200+100
    assert_eq!(binding.total_cost_micros, 3_000);
    assert!(binding.last_used_at.is_some());
}

// ---------------------------------------------------------------------------
// 7. Registry: record_invocation on unknown model is no-op
// ---------------------------------------------------------------------------

#[tokio::test]
async fn record_invocation_unknown_model_is_noop() {
    let registry = ModelBindingRegistry::new();
    let unknown = ModelId::new();
    // Must not panic.
    registry.record_invocation(&unknown, 100, 50, 1_000).await;
    assert!(registry.get(&unknown).await.is_none());
}

// ---------------------------------------------------------------------------
// 8. Registry: list returns all bindings
// ---------------------------------------------------------------------------

#[tokio::test]
async fn registry_list_returns_all() {
    let registry = ModelBindingRegistry::new();
    for _ in 0..3 {
        registry
            .bind(make_ollama_model(), None)
            .await
            .expect("bind");
    }
    let all = registry.list().await;
    assert_eq!(all.len(), 3);
}

// ---------------------------------------------------------------------------
// 9. Registry: bind rejects external model without vault (INV-018)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn registry_bind_rejects_external_without_vault() {
    let registry = ModelBindingRegistry::new();
    let mut model = make_anthropic_model();
    model.vault_capability_id = None;
    let err = registry
        .bind(model, None)
        .await
        .expect_err("external model without vault must be rejected by registry");
    assert!(matches!(err, CognitiveError::Internal(_)));
}

// ---------------------------------------------------------------------------
// 10. Core integration: with_model_catalog uses default model in provenance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn core_with_catalog_uses_default_model_in_provenance() {
    use aios_cognitive::{
        AICrossOriginPosture, CognitiveCore, CognitiveIntent, InMemoryCognitiveCore, IntentId,
        LatencyTier, PrivacyClass, SubjectRef, TranslationContext,
    };
    use std::sync::Arc;

    // Build catalog manually (async-safe) instead of with_fixtures()
    // which uses blocking_write and panics inside a tokio runtime.
    let catalog = Arc::new(CognitiveModelCatalog::new());
    let default_id = ModelId("mdl_test_default".into());
    catalog
        .register(CognitiveModel {
            model_id: default_id.clone(),
            provider: ProviderClass::Ollama,
            capabilities: vec!["text-generation".into()],
            max_tokens: 4_096,
            input_cost_per_1k: 0,
            output_cost_per_1k: 0,
            vault_capability_id: None,
            created_at: chrono::Utc::now(),
        })
        .await
        .expect("register default model");
    let bindings = Arc::new(ModelBindingRegistry::new());
    let core = InMemoryCognitiveCore::new().with_model_catalog(catalog, bindings);

    let intent = CognitiveIntent {
        intent_id: IntentId::new(),
        subject: SubjectRef("agent:dev".into()),
        natural_language: "restart nginx".into(),
        context_hash: "0000000000000000000000000000000000000000000000000000000000000000".into(),
        created_at: chrono::Utc::now(),
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Public,
    };
    let ctx = TranslationContext {
        subject: SubjectRef("agent:dev".into()),
        available_models: vec![],
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Public,
        ai_cross_origin_posture: AICrossOriginPosture::AiVaultBrokeredOnly,
        recovery_mode: false,
        budget_ok: true,
    };

    let result = core
        .translate_intent(&intent, &ctx)
        .await
        .expect("translate");
    // with_fixtures auto-sets default to Anthropic model (mdl_fixture_anthropic).
    assert_eq!(
        result.translation_provenance.model_used, "mdl_test_default",
        "model_used must be the catalog default model id"
    );
}

// ---------------------------------------------------------------------------
// 11. Core integration: without catalog uses backend stub (backward compat)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn core_without_catalog_uses_backend_stub() {
    use aios_cognitive::{
        AICrossOriginPosture, CognitiveCore, CognitiveIntent, InMemoryCognitiveCore, IntentId,
        LatencyTier, PrivacyClass, SubjectRef, TranslationContext,
    };

    let core = InMemoryCognitiveCore::new();
    let intent = CognitiveIntent {
        intent_id: IntentId::new(),
        subject: SubjectRef("agent:dev".into()),
        natural_language: "restart nginx".into(),
        context_hash: "0000000000000000000000000000000000000000000000000000000000000000".into(),
        created_at: chrono::Utc::now(),
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Public,
    };
    let ctx = TranslationContext {
        subject: SubjectRef("agent:dev".into()),
        available_models: vec![],
        latency_class: LatencyTier::T3LocalCognitive,
        privacy_class: PrivacyClass::Public,
        ai_cross_origin_posture: AICrossOriginPosture::AiVaultBrokeredOnly,
        recovery_mode: false,
        budget_ok: true,
    };

    let result = core
        .translate_intent(&intent, &ctx)
        .await
        .expect("translate");
    // Without catalog: uses backend-kind stub string.
    assert_eq!(
        result.translation_provenance.model_used, "localcpu",
        "without catalog, model_used must be the backend-kind stub"
    );
}

// ---------------------------------------------------------------------------
// 12. ModelBinding::new with local model + explicit vault_capability_id is ok
// ---------------------------------------------------------------------------

#[test]
fn binding_new_local_with_optional_vault_is_ok() {
    let model = make_ollama_model();
    let binding = ModelBinding::new(model, Some("optional_vcap".into()))
        .expect("local model with optional vault is fine");
    assert_eq!(
        binding.vault_capability_id.expect("must have vault id"),
        "optional_vcap"
    );
}

// ---------------------------------------------------------------------------
// 13. Registry: get on empty registry returns None
// ---------------------------------------------------------------------------

#[tokio::test]
async fn registry_get_empty_returns_none() {
    let registry = ModelBindingRegistry::new();
    let unknown = ModelId::new();
    assert!(registry.get(&unknown).await.is_none());
}

// ---------------------------------------------------------------------------
// 14. Registry: bind same ModelId twice overwrites
// ---------------------------------------------------------------------------

#[tokio::test]
async fn registry_bind_twice_overwrites() {
    let registry = ModelBindingRegistry::new();
    let model = make_ollama_model();
    let mid = model.model_id.clone();

    registry
        .bind(model.clone(), None)
        .await
        .expect("first bind");
    // Bind again with a different vault id.
    registry
        .bind(model, Some("new_vcap".into()))
        .await
        .expect("second bind");

    let binding = registry.get(&mid).await.expect("get");
    assert_eq!(
        binding.vault_capability_id.expect("must have vault id"),
        "new_vcap"
    );
}

// ---------------------------------------------------------------------------
// 15. record_invocation saturating arithmetic
// ---------------------------------------------------------------------------

#[tokio::test]
async fn record_invocation_saturating_arithmetic() {
    let registry = ModelBindingRegistry::new();
    let model = make_ollama_model();
    let mid = model.model_id.clone();
    registry.bind(model, None).await.expect("bind");

    // Saturate total_calls.
    registry.record_invocation(&mid, 0, 0, u64::MAX).await;

    let binding = registry.get(&mid).await.expect("get");
    assert_eq!(binding.total_calls, 1);
    assert_eq!(binding.total_cost_micros, u64::MAX);
}
