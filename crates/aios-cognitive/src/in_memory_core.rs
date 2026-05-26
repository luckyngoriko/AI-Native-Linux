//! In-memory implementation of [`CognitiveCore`] for testing and prototyping.
//!
//! Uses `RwLock<HashMap<...>>` for the translation cache and an `Arc<Vec<...>>`
//! for the model catalog — the same pattern as T-085 `InMemoryServiceGraph`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;

use aios_action::{ActionEnvelope, Identity, Request, Trace};

use crate::core::{CognitiveCore, IntentCapability, TranslationContext};
use crate::error::CognitiveError;
use crate::intent::{CognitiveIntent, IntentId};
use crate::latency::{LatencyTier, PrivacyClass};
use crate::model::CognitiveModel;
use crate::routing::{ModelBackendKind, ProviderClass, RoutingDecision};
use crate::translator::{TranslationProvenance, TranslationResult};

/// In-memory [`CognitiveCore`] backed by a translation cache and model catalog.
pub struct InMemoryCognitiveCore {
    translation_cache: RwLock<HashMap<IntentId, TranslationResult>>,
    #[allow(dead_code)]
    model_catalog: Arc<Vec<CognitiveModel>>,
}

impl InMemoryCognitiveCore {
    /// Create an empty cognitive core with no models in the catalog.
    #[must_use]
    pub fn new() -> Self {
        Self {
            translation_cache: RwLock::new(HashMap::new()),
            model_catalog: Arc::new(Vec::new()),
        }
    }

    /// Create a cognitive core with a pre-populated model catalog.
    #[must_use]
    pub fn with_models(models: Vec<CognitiveModel>) -> Self {
        Self {
            translation_cache: RwLock::new(HashMap::new()),
            model_catalog: Arc::new(models),
        }
    }
}

impl Default for InMemoryCognitiveCore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CognitiveCore for InMemoryCognitiveCore {
    async fn translate_intent(
        &self,
        intent: &CognitiveIntent,
        _context: &TranslationContext,
    ) -> Result<TranslationResult, CognitiveError> {
        // INV-002: Always produce a typed ActionEnvelope, never a raw shell command.
        let envelope = ActionEnvelope::new(
            Identity::new(intent.subject.0.clone(), true),
            Request::new(
                "cognitive.translate",
                serde_json::json!({
                    "intent_id": intent.intent_id.0,
                    "natural_language": intent.natural_language,
                }),
            ),
            Trace::new("00000000000000000000000000000000", "0000000000000000", None),
        );

        // Deterministic stub routing: pick backend based on privacy class.
        let (chosen_backend, provider_class, degraded, reason) = match intent.privacy_class {
            PrivacyClass::Public | PrivacyClass::Internal => (
                ModelBackendKind::LocalCpu,
                ProviderClass::Anthropic,
                false,
                None,
            ),
            PrivacyClass::Sensitive => (
                ModelBackendKind::LocalGpu,
                ProviderClass::Openai,
                false,
                None,
            ),
            PrivacyClass::SecretBearing | PrivacyClass::Classified => (
                ModelBackendKind::FallbackRuleBased,
                ProviderClass::Ollama,
                true,
                Some("privacy-restricted: local-only deterministic fallback".into()),
            ),
        };

        let routing_decision_id = format!("rtdg_{}", ulid::Ulid::new());

        let _ = RoutingDecision {
            routing_id: routing_decision_id.clone(),
            chosen_backend,
            provider_class,
            backend_id: "stub-backend".into(),
            matched_rule: 1,
            degraded,
            reason,
            decided_at: Utc::now(),
        };

        let result = TranslationResult {
            intent_id: intent.intent_id.clone(),
            produced_action: envelope,
            routing_decision_id: Some(routing_decision_id),
            verification_intent: None,
            translation_provenance: TranslationProvenance {
                translator_version: "0.1.0-T095".into(),
                model_used: "stub".into(),
                tokens_in: 0,
                tokens_out: 0,
                model_signed_response: None,
            },
            translated_at: Utc::now(),
        };

        self.translation_cache
            .write()
            .await
            .insert(intent.intent_id.clone(), result.clone());

        Ok(result)
    }

    fn list_supported_intents(&self) -> Vec<IntentCapability> {
        vec![
            IntentCapability {
                intent_kind: "service.restart".into(),
                description: "Restart a system service".into(),
                requires_latency_tier: LatencyTier::T1Deterministic,
                produces_action_type: "service.restart".into(),
                max_tokens_estimate: 512,
            },
            IntentCapability {
                intent_kind: "cognitive.translate".into(),
                description: "Translate a natural-language intent into a typed action".into(),
                requires_latency_tier: LatencyTier::T3LocalCognitive,
                produces_action_type: "cognitive.translate".into(),
                max_tokens_estimate: 2048,
            },
        ]
    }

    async fn get_translation(
        &self,
        intent_id: &IntentId,
    ) -> Result<TranslationResult, CognitiveError> {
        self.translation_cache
            .read()
            .await
            .get(intent_id)
            .cloned()
            .ok_or_else(|| {
                CognitiveError::NoMatchingCapability(format!("intent not found: {}", intent_id.0))
            })
    }
}
