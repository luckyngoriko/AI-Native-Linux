use std::collections::HashMap;

use chrono::Utc;
use tokio::sync::RwLock;

use crate::error::CognitiveError;
use crate::model::{CognitiveModel, ModelId};
use crate::routing::{ModelBackendKind, ProviderClass};

/// Maps each `ProviderClass` to its canonical `ModelBackendKind` per S13.2.
const fn provider_to_backend(provider: ProviderClass) -> ModelBackendKind {
    match provider {
        ProviderClass::Anthropic | ProviderClass::Openai | ProviderClass::OtherVaultBrokered => {
            ModelBackendKind::ExternalVaultBrokered
        }
        ProviderClass::Ollama => ModelBackendKind::LocalCpu,
        ProviderClass::Vllm => ModelBackendKind::LocalGpu,
    }
}

/// Returns `true` when the provider class requires a vault capability id per INV-018.
const fn is_external_provider(provider: ProviderClass) -> bool {
    matches!(
        provider,
        ProviderClass::Anthropic | ProviderClass::Openai | ProviderClass::OtherVaultBrokered
    )
}

/// The cognitive model catalog — registers, looks up, and lists
/// `CognitiveModel` instances. Also tracks a default model for the
/// translator to populate `TranslationProvenance.model_used`.
pub struct CognitiveModelCatalog {
    models: RwLock<HashMap<ModelId, CognitiveModel>>,
    default_model: RwLock<Option<ModelId>>,
}

impl CognitiveModelCatalog {
    /// Create an empty catalog.
    #[must_use]
    pub fn new() -> Self {
        Self {
            models: RwLock::new(HashMap::new()),
            default_model: RwLock::new(None),
        }
    }

    /// Create a catalog pre-loaded with one canonical model per `ProviderClass`
    /// variant. Useful for tests and the prototype golden path.
    #[must_use]
    pub fn with_fixtures() -> Self {
        let catalog = Self::new();
        // We register synchronously through the internal lock-free path so
        // that the caller does not need an async context for construction.
        let mut models = catalog.models.blocking_write();
        let fixtures = vec![
            CognitiveModel {
                model_id: ModelId("mdl_fixture_anthropic".into()),
                provider: ProviderClass::Anthropic,
                capabilities: vec!["text-generation".into(), "code-completion".into()],
                max_tokens: 200_000,
                input_cost_per_1k: 15_000,
                output_cost_per_1k: 75_000,
                vault_capability_id: Some("vcap_fixture_anthropic".into()),
                created_at: Utc::now(),
            },
            CognitiveModel {
                model_id: ModelId("mdl_fixture_openai".into()),
                provider: ProviderClass::Openai,
                capabilities: vec!["text-generation".into(), "code-completion".into()],
                max_tokens: 128_000,
                input_cost_per_1k: 10_000,
                output_cost_per_1k: 30_000,
                vault_capability_id: Some("vcap_fixture_openai".into()),
                created_at: Utc::now(),
            },
            CognitiveModel {
                model_id: ModelId("mdl_fixture_ollama".into()),
                provider: ProviderClass::Ollama,
                capabilities: vec!["text-generation".into()],
                max_tokens: 4_096,
                input_cost_per_1k: 0,
                output_cost_per_1k: 0,
                vault_capability_id: None,
                created_at: Utc::now(),
            },
            CognitiveModel {
                model_id: ModelId("mdl_fixture_vllm".into()),
                provider: ProviderClass::Vllm,
                capabilities: vec!["text-generation".into()],
                max_tokens: 32_768,
                input_cost_per_1k: 0,
                output_cost_per_1k: 0,
                vault_capability_id: None,
                created_at: Utc::now(),
            },
            CognitiveModel {
                model_id: ModelId("mdl_fixture_other_brokered".into()),
                provider: ProviderClass::OtherVaultBrokered,
                capabilities: vec!["text-generation".into()],
                max_tokens: 100_000,
                input_cost_per_1k: 5_000,
                output_cost_per_1k: 25_000,
                vault_capability_id: Some("vcap_fixture_other".into()),
                created_at: Utc::now(),
            },
        ];
        for m in fixtures {
            models.insert(m.model_id.clone(), m);
        }
        drop(models);
        // Auto-set the first fixture as default.
        {
            let mut def = catalog.default_model.blocking_write();
            *def = Some(ModelId("mdl_fixture_anthropic".into()));
        }
        catalog
    }

    /// Register a model. Rejects duplicate `ModelId`. If the catalog was empty
    /// before this registration, the new model becomes the default.
    ///
    /// # Errors
    ///
    /// Returns `CognitiveError::NoMatchingCapability` if a model with the same
    /// `ModelId` is already registered.
    pub async fn register(&self, model: CognitiveModel) -> Result<(), CognitiveError> {
        let mid = model.model_id.clone();
        let was_empty = {
            let mut models = self.models.write().await;
            if models.contains_key(&mid) {
                return Err(CognitiveError::NoMatchingCapability(format!(
                    "model already registered: {}",
                    mid.0
                )));
            }
            let empty = models.is_empty();
            models.insert(mid.clone(), model);
            empty
        };
        if was_empty {
            *self.default_model.write().await = Some(mid);
        }
        Ok(())
    }

    /// Look up a model by id.
    ///
    /// # Errors
    ///
    /// Returns `CognitiveError::NoMatchingCapability` if the model is not found.
    pub async fn lookup(&self, model_id: &ModelId) -> Result<CognitiveModel, CognitiveError> {
        self.models
            .read()
            .await
            .get(model_id)
            .cloned()
            .ok_or_else(|| {
                CognitiveError::NoMatchingCapability(format!("model not found: {}", model_id.0))
            })
    }

    /// Return every registered model.
    pub async fn list(&self) -> Vec<CognitiveModel> {
        self.models.read().await.values().cloned().collect()
    }

    /// Return all models of the given provider class.
    pub async fn list_by_provider(&self, provider: ProviderClass) -> Vec<CognitiveModel> {
        self.models
            .read()
            .await
            .values()
            .filter(|m| m.provider == provider)
            .cloned()
            .collect()
    }

    /// Find the first model whose `ProviderClass` maps to `backend` per the
    /// S13.2 provider-to-backend canonical mapping.
    pub async fn find_for_backend(&self, backend: ModelBackendKind) -> Option<CognitiveModel> {
        self.models
            .read()
            .await
            .values()
            .find(|m| provider_to_backend(m.provider) == backend)
            .cloned()
    }

    /// Set the default model. Fails if `model_id` is not registered.
    ///
    /// # Errors
    ///
    /// Returns `CognitiveError::NoMatchingCapability` if the model is not found
    /// in the catalog.
    pub async fn set_default(&self, model_id: &ModelId) -> Result<(), CognitiveError> {
        {
            let models = self.models.read().await;
            if !models.contains_key(model_id) {
                return Err(CognitiveError::NoMatchingCapability(format!(
                    "cannot set default — model not found: {}",
                    model_id.0
                )));
            }
        }
        *self.default_model.write().await = Some(model_id.clone());
        Ok(())
    }

    /// Return the current default model, if any.
    pub async fn get_default(&self) -> Option<CognitiveModel> {
        let model_id = self.default_model.read().await.clone()?;
        self.models.read().await.get(&model_id).cloned()
    }

    /// Expose whether a provider requires a vault capability (INV-018 helper).
    #[must_use]
    pub const fn requires_vault_capability(provider: ProviderClass) -> bool {
        is_external_provider(provider)
    }
}

impl Default for CognitiveModelCatalog {
    fn default() -> Self {
        Self::new()
    }
}

// Allow the model_binding module to read the raw catalog without cloning
// every model on each lookup.
impl CognitiveModelCatalog {
    #[allow(dead_code)]
    pub(crate) async fn get_model_ref(&self, model_id: &ModelId) -> Option<CognitiveModel> {
        self.models.read().await.get(model_id).cloned()
    }
}
