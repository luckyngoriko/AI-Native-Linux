//! T-099 tests for `CognitiveModelCatalog` — registration, lookup, default model,
//! provider-to-backend mapping, fixtures, and error paths.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_cognitive::{
    CognitiveError, CognitiveModel, CognitiveModelCatalog, ModelBackendKind, ModelId, ProviderClass,
};

// ---------------------------------------------------------------------------
// 1. new catalog is empty
// ---------------------------------------------------------------------------

#[test]
fn new_catalog_is_empty() {
    let catalog = CognitiveModelCatalog::new();
    // Synchronous inspection via public API: list() requires async, but we
    // can verify the catalog was constructed without panicking.
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let models = rt.block_on(catalog.list());
    assert!(models.is_empty(), "new catalog must be empty");
}

// ---------------------------------------------------------------------------
// 2. register then lookup
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_then_lookup() {
    let catalog = CognitiveModelCatalog::new();
    let model = CognitiveModel {
        model_id: ModelId::new(),
        provider: ProviderClass::Ollama,
        capabilities: vec!["text-generation".into()],
        max_tokens: 4_096,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: None,
        created_at: chrono::Utc::now(),
    };
    let mid = model.model_id.clone();

    catalog
        .register(model)
        .await
        .expect("register must succeed");
    let found = catalog
        .lookup(&mid)
        .await
        .expect("lookup must find registered model");
    assert_eq!(found.model_id, mid);
    assert_eq!(found.provider, ProviderClass::Ollama);
}

// ---------------------------------------------------------------------------
// 3. register rejects duplicate
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_rejects_duplicate() {
    let catalog = CognitiveModelCatalog::new();
    let model = CognitiveModel {
        model_id: ModelId::new(),
        provider: ProviderClass::Ollama,
        capabilities: vec!["text-generation".into()],
        max_tokens: 4_096,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: None,
        created_at: chrono::Utc::now(),
    };

    catalog
        .register(model.clone())
        .await
        .expect("first register must succeed");
    let err = catalog
        .register(model)
        .await
        .expect_err("duplicate register must fail");
    assert!(matches!(err, CognitiveError::NoMatchingCapability(_)));
}

// ---------------------------------------------------------------------------
// 4. lookup unknown model
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lookup_unknown_model_is_error() {
    let catalog = CognitiveModelCatalog::new();
    let unknown = ModelId::new();
    let err = catalog
        .lookup(&unknown)
        .await
        .expect_err("unknown lookup must fail");
    assert!(matches!(err, CognitiveError::NoMatchingCapability(_)));
}

// ---------------------------------------------------------------------------
// 5. list returns all registered
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_returns_all_registered() {
    let catalog = CognitiveModelCatalog::new();
    for _ in 0..3 {
        catalog
            .register(CognitiveModel {
                model_id: ModelId::new(),
                provider: ProviderClass::Ollama,
                capabilities: vec!["text-generation".into()],
                max_tokens: 4_096,
                input_cost_per_1k: 0,
                output_cost_per_1k: 0,
                vault_capability_id: None,
                created_at: chrono::Utc::now(),
            })
            .await
            .expect("register");
    }
    let all = catalog.list().await;
    assert_eq!(all.len(), 3);
}

// ---------------------------------------------------------------------------
// 6. list_by_provider filters correctly
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_by_provider_filters() {
    let catalog = CognitiveModelCatalog::new();
    catalog
        .register(CognitiveModel {
            model_id: ModelId::new(),
            provider: ProviderClass::Ollama,
            capabilities: vec!["text-generation".into()],
            max_tokens: 4_096,
            input_cost_per_1k: 0,
            output_cost_per_1k: 0,
            vault_capability_id: None,
            created_at: chrono::Utc::now(),
        })
        .await
        .expect("register ollama");
    catalog
        .register(CognitiveModel {
            model_id: ModelId::new(),
            provider: ProviderClass::Anthropic,
            capabilities: vec!["text-generation".into()],
            max_tokens: 200_000,
            input_cost_per_1k: 15,
            output_cost_per_1k: 75,
            vault_capability_id: Some("vcap_test".into()),
            created_at: chrono::Utc::now(),
        })
        .await
        .expect("register anthropic");

    let ollama_models = catalog.list_by_provider(ProviderClass::Ollama).await;
    assert_eq!(ollama_models.len(), 1);
    assert_eq!(ollama_models[0].provider, ProviderClass::Ollama);

    let anthropic_models = catalog.list_by_provider(ProviderClass::Anthropic).await;
    assert_eq!(anthropic_models.len(), 1);
    assert_eq!(anthropic_models[0].provider, ProviderClass::Anthropic);

    let vllm_models = catalog.list_by_provider(ProviderClass::Vllm).await;
    assert!(vllm_models.is_empty());
}

// ---------------------------------------------------------------------------
// 7. find_for_backend maps ProviderClass → ModelBackendKind per S13.2
// ---------------------------------------------------------------------------

#[tokio::test]
async fn find_for_backend_maps_correctly() {
    // Build catalog manually to avoid with_fixtures() blocking_write panic
    // inside the tokio runtime.
    let catalog = CognitiveModelCatalog::new();
    catalog
        .register(CognitiveModel {
            model_id: ModelId::new(),
            provider: ProviderClass::Ollama,
            capabilities: vec!["text-generation".into()],
            max_tokens: 4_096,
            input_cost_per_1k: 0,
            output_cost_per_1k: 0,
            vault_capability_id: None,
            created_at: chrono::Utc::now(),
        })
        .await
        .expect("register ollama");
    catalog
        .register(CognitiveModel {
            model_id: ModelId::new(),
            provider: ProviderClass::Vllm,
            capabilities: vec!["text-generation".into()],
            max_tokens: 32_768,
            input_cost_per_1k: 0,
            output_cost_per_1k: 0,
            vault_capability_id: None,
            created_at: chrono::Utc::now(),
        })
        .await
        .expect("register vllm");
    catalog
        .register(CognitiveModel {
            model_id: ModelId::new(),
            provider: ProviderClass::Anthropic,
            capabilities: vec!["text-generation".into()],
            max_tokens: 200_000,
            input_cost_per_1k: 15,
            output_cost_per_1k: 75,
            vault_capability_id: Some("vcap_test".into()),
            created_at: chrono::Utc::now(),
        })
        .await
        .expect("register anthropic");

    let cpu_model = catalog
        .find_for_backend(ModelBackendKind::LocalCpu)
        .await
        .expect("must find LocalCpu model");
    assert_eq!(cpu_model.provider, ProviderClass::Ollama);

    let gpu_model = catalog
        .find_for_backend(ModelBackendKind::LocalGpu)
        .await
        .expect("must find LocalGpu model");
    assert_eq!(gpu_model.provider, ProviderClass::Vllm);

    let external = catalog
        .find_for_backend(ModelBackendKind::ExternalVaultBrokered)
        .await
        .expect("must find ExternalVaultBrokered model");
    assert!(matches!(
        external.provider,
        ProviderClass::Anthropic | ProviderClass::Openai | ProviderClass::OtherVaultBrokered
    ));
}

// ---------------------------------------------------------------------------
// 8. set_default and get_default round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn set_default_and_get_default() {
    let catalog = CognitiveModelCatalog::new();
    let model = CognitiveModel {
        model_id: ModelId::new(),
        provider: ProviderClass::Ollama,
        capabilities: vec!["text-generation".into()],
        max_tokens: 4_096,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: None,
        created_at: chrono::Utc::now(),
    };
    let mid = model.model_id.clone();
    catalog.register(model).await.expect("register");

    // register auto-set it as default (first model)
    let default = catalog.get_default().await.expect("must have default");
    assert_eq!(default.model_id, mid);

    // Register a second model and explicitly set it as default.
    let model2 = CognitiveModel {
        model_id: ModelId::new(),
        provider: ProviderClass::Vllm,
        capabilities: vec!["text-generation".into()],
        max_tokens: 32_768,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: None,
        created_at: chrono::Utc::now(),
    };
    let mid2 = model2.model_id.clone();
    catalog.register(model2).await.expect("register");
    catalog.set_default(&mid2).await.expect("set_default");

    let new_default = catalog.get_default().await.expect("must have default");
    assert_eq!(new_default.model_id, mid2);
}

// ---------------------------------------------------------------------------
// 9. set_default rejects unregistered model
// ---------------------------------------------------------------------------

#[tokio::test]
async fn set_default_rejects_unknown_model() {
    let catalog = CognitiveModelCatalog::new();
    let unknown = ModelId::new();
    let err = catalog
        .set_default(&unknown)
        .await
        .expect_err("set_default on unknown must fail");
    assert!(matches!(err, CognitiveError::NoMatchingCapability(_)));
}

// ---------------------------------------------------------------------------
// 10. with_fixtures pre-loads 5 canonical models
// ---------------------------------------------------------------------------

#[test]
fn with_fixtures_preloads_five_models() {
    let catalog = CognitiveModelCatalog::with_fixtures();
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let models = rt.block_on(catalog.list());
    assert_eq!(models.len(), 5);

    // Verify one of each provider class.
    let providers: Vec<ProviderClass> = models.iter().map(|m| m.provider).collect();
    assert!(providers.contains(&ProviderClass::Anthropic));
    assert!(providers.contains(&ProviderClass::Openai));
    assert!(providers.contains(&ProviderClass::Ollama));
    assert!(providers.contains(&ProviderClass::Vllm));
    assert!(providers.contains(&ProviderClass::OtherVaultBrokered));
}

// ---------------------------------------------------------------------------
// 11. with_fixtures has a default model
// ---------------------------------------------------------------------------

#[test]
fn with_fixtures_has_default_model() {
    let catalog = CognitiveModelCatalog::with_fixtures();
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let default = rt.block_on(catalog.get_default());
    assert!(default.is_some(), "with_fixtures must have a default model");
}

// ---------------------------------------------------------------------------
// 12. register auto-sets default on first model
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_auto_sets_default_on_first() {
    let catalog = CognitiveModelCatalog::new();
    assert!(catalog.get_default().await.is_none());

    let model = CognitiveModel {
        model_id: ModelId::new(),
        provider: ProviderClass::Ollama,
        capabilities: vec!["text-generation".into()],
        max_tokens: 4_096,
        input_cost_per_1k: 0,
        output_cost_per_1k: 0,
        vault_capability_id: None,
        created_at: chrono::Utc::now(),
    };
    catalog.register(model).await.expect("register");
    assert!(catalog.get_default().await.is_some());
}

// ---------------------------------------------------------------------------
// 13. requires_vault_capability truth table
// ---------------------------------------------------------------------------

#[test]
fn requires_vault_capability_truth_table() {
    // External providers require vault; local ones do not.
    assert!(CognitiveModelCatalog::requires_vault_capability(
        ProviderClass::Anthropic
    ));
    assert!(CognitiveModelCatalog::requires_vault_capability(
        ProviderClass::Openai
    ));
    assert!(CognitiveModelCatalog::requires_vault_capability(
        ProviderClass::OtherVaultBrokered
    ));
    assert!(!CognitiveModelCatalog::requires_vault_capability(
        ProviderClass::Ollama
    ));
    assert!(!CognitiveModelCatalog::requires_vault_capability(
        ProviderClass::Vllm
    ));
}

// ---------------------------------------------------------------------------
// 14. with_fixtures each model is lookup-able
// ---------------------------------------------------------------------------

#[test]
fn with_fixtures_each_model_lookupable() {
    let catalog = CognitiveModelCatalog::with_fixtures();
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let models = rt.block_on(catalog.list());

    for m in &models {
        let found = rt
            .block_on(catalog.lookup(&m.model_id))
            .expect("fixture model must be lookup-able");
        assert_eq!(found.model_id, m.model_id);
    }
}
