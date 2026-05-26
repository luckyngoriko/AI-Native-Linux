use std::collections::HashMap;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::error::CognitiveError;
use crate::model::{CognitiveModel, ModelId};
use crate::model_catalog::CognitiveModelCatalog;

/// Runtime view of a `CognitiveModel` ready to invoke.
///
/// # INV-015
///
/// This struct tracks invocation statistics (calls, tokens, cost) but
/// **never** stores prompt or response bodies. Serializing to JSON must
/// not expose any prompt/response field — this is verified by test.
///
/// # INV-018
///
/// External-vault-brokered models (`Anthropic`, `Openai`, `OtherVaultBrokered`)
/// MUST carry a `vault_capability_id`. Binding one without it fails with
/// a `CognitiveError`. Local models (`Ollama`, `Vllm`) may omit it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelBinding {
    /// The bound model.
    pub model: CognitiveModel,
    /// Vault capability handle for external providers; `None` for local ones.
    /// INV-018 — this is a handle, not raw credential bytes.
    pub vault_capability_id: Option<String>,
    /// Last time this binding was used for an invocation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
    /// Total number of invocations recorded.
    pub total_calls: u64,
    /// Cumulative tokens consumed (input + output).
    pub total_tokens_used: u64,
    /// Cumulative cost in micro-units of the declared currency.
    pub total_cost_micros: u64,
}

impl ModelBinding {
    /// Create a new binding, enforcing INV-018.
    ///
    /// # Errors
    ///
    /// Returns `CognitiveError::Internal` if the model requires a vault
    /// capability but none is provided.
    pub fn new(
        model: CognitiveModel,
        vault_capability_id: Option<String>,
    ) -> Result<Self, CognitiveError> {
        if CognitiveModelCatalog::requires_vault_capability(model.provider)
            && vault_capability_id.is_none()
        {
            return Err(CognitiveError::Internal(format!(
                "vault credential required for external model: {}",
                model.model_id.0
            )));
        }
        Ok(Self {
            model,
            vault_capability_id,
            last_used_at: None,
            total_calls: 0,
            total_tokens_used: 0,
            total_cost_micros: 0,
        })
    }
}

/// Registry of live `ModelBinding` instances — one per `ModelId`.
///
/// Created from `CognitiveModelCatalog` entries via `bind()`.
/// INV-015: `record_invocation` updates stats only; no prompt or response
/// bodies are ever stored in any `ModelBinding` field.
pub struct ModelBindingRegistry {
    bindings: RwLock<HashMap<ModelId, ModelBinding>>,
}

impl ModelBindingRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bindings: RwLock::new(HashMap::new()),
        }
    }

    /// Bind a model into the runtime registry.
    ///
    /// INV-018 enforced here: external-vault-brokered models without a
    /// `vault_capability_id` are rejected.
    ///
    /// # Errors
    ///
    /// Returns `CognitiveError::Internal` if the model requires a vault
    /// capability but none is provided.
    pub async fn bind(
        &self,
        model: CognitiveModel,
        vault_capability_id: Option<String>,
    ) -> Result<ModelBinding, CognitiveError> {
        let binding = ModelBinding::new(model.clone(), vault_capability_id)?;
        self.bindings
            .write()
            .await
            .insert(model.model_id.clone(), binding.clone());
        Ok(binding)
    }

    /// Record an invocation, updating per-model stats.
    ///
    /// # INV-015
    ///
    /// Only token counts and cost are tracked. No prompt or response bodies
    /// are stored — the `ModelBinding` struct has no fields for them.
    pub async fn record_invocation(
        &self,
        model_id: &ModelId,
        tokens_in: u32,
        tokens_out: u32,
        cost_micros: u64,
    ) {
        let mut map = self.bindings.write().await;
        if let Some(binding) = map.get_mut(model_id) {
            binding.total_calls = binding.total_calls.saturating_add(1);
            binding.total_tokens_used = binding
                .total_tokens_used
                .saturating_add(u64::from(tokens_in) + u64::from(tokens_out));
            binding.total_cost_micros = binding.total_cost_micros.saturating_add(cost_micros);
            binding.last_used_at = Some(Utc::now());
        }
    }

    /// Get a binding by model id.
    pub async fn get(&self, model_id: &ModelId) -> Option<ModelBinding> {
        self.bindings.read().await.get(model_id).cloned()
    }

    /// List all registered bindings.
    pub async fn list(&self) -> Vec<ModelBinding> {
        self.bindings.read().await.values().cloned().collect()
    }
}

impl Default for ModelBindingRegistry {
    fn default() -> Self {
        Self::new()
    }
}
