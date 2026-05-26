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

use crate::breaker_registry::CircuitBreakerRegistry;
use crate::circuit::{CircuitBreakerConfig, CircuitBreakerStats, CircuitState};
use crate::core::{CognitiveCore, IntentCapability, TranslationContext};
use crate::error::CognitiveError;
use crate::intent::{CognitiveIntent, IntentId};
use crate::latency::{LatencyTier, PrivacyClass};
use crate::model::CognitiveModel;
use crate::router::ModelRouter;
use crate::router_state::RouterState;
use crate::routing::{
    BackendHealthEntry, ModelBackendKind, ProviderClass, RoutingDecision, RoutingInputs,
};
use crate::translator::{TranslationProvenance, TranslationResult};

/// In-memory [`CognitiveCore`] backed by a translation cache and model catalog.
pub struct InMemoryCognitiveCore {
    translation_cache: RwLock<HashMap<IntentId, TranslationResult>>,
    #[allow(dead_code)]
    model_catalog: Arc<Vec<CognitiveModel>>,
    /// Optional model router for T-097+ S13.2 routing decisions.
    router: Option<Arc<ModelRouter>>,
    /// Optional router state for health tracking and routing-id minting.
    router_state: Option<Arc<RouterState>>,
    /// Optional circuit breaker registry for S14.1 breaker consultation.
    breaker_registry: Option<Arc<CircuitBreakerRegistry>>,
}

impl InMemoryCognitiveCore {
    /// Create an empty cognitive core with no models in the catalog.
    #[must_use]
    pub fn new() -> Self {
        Self {
            translation_cache: RwLock::new(HashMap::new()),
            model_catalog: Arc::new(Vec::new()),
            router: None,
            router_state: None,
            breaker_registry: None,
        }
    }

    /// Create a cognitive core with a pre-populated model catalog.
    #[must_use]
    pub fn with_models(models: Vec<CognitiveModel>) -> Self {
        Self {
            translation_cache: RwLock::new(HashMap::new()),
            model_catalog: Arc::new(models),
            router: None,
            router_state: None,
            breaker_registry: None,
        }
    }

    /// Attach a model router and router state for S13.2 routing decisions.
    ///
    /// When configured, `translate_intent` builds `RoutingInputs` from the
    /// translation context and calls `router.route()` instead of the T-095
    /// deterministic stub. Without a router, the T-095 stub is preserved for
    /// backward compatibility.
    #[must_use]
    pub fn with_router(mut self, router: Arc<ModelRouter>, state: Arc<RouterState>) -> Self {
        self.router = Some(router);
        self.router_state = Some(state);
        self
    }

    /// Attach a circuit breaker registry for S14.1 breaker consultation.
    ///
    /// When configured, `translate_intent` consults the breaker registry before
    /// routing decisions. If the circuit for the chosen backend is open, the
    /// call is rejected with `CognitiveError::CircuitBreakerOpen`.
    #[must_use]
    pub fn with_breakers(mut self, registry: Arc<CircuitBreakerRegistry>) -> Self {
        self.breaker_registry = Some(registry);
        self
    }
}

impl Default for InMemoryCognitiveCore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[allow(clippy::too_many_lines)]
impl CognitiveCore for InMemoryCognitiveCore {
    async fn translate_intent(
        &self,
        intent: &CognitiveIntent,
        context: &TranslationContext,
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

        // ── T-097 router path ──
        let (chosen_backend, provider_class, degraded, reason, routing_decision_id) =
            if let (Some(router), Some(state)) = (&self.router, &self.router_state) {
                let health_map = state.get_health().await;
                let health_snapshot: Vec<BackendHealthEntry> = health_map
                    .iter()
                    .map(|(kind, hstate)| BackendHealthEntry {
                        backend_kind: *kind,
                        provider_class: ProviderClass::Ollama,
                        state: *hstate,
                        config: CircuitBreakerConfig::default(),
                        stats: CircuitBreakerStats {
                            state: CircuitState::Closed,
                            success_count: 0,
                            failure_count: 0,
                            error_rate: 0.0,
                            cooldown_seconds: 0,
                            last_state_change_at: Utc::now(),
                            next_probe_at: None,
                        },
                    })
                    .collect();

                let inputs = RoutingInputs {
                    latency_class: context.latency_class,
                    privacy_class: context.privacy_class,
                    ai_cross_origin_posture: context.ai_cross_origin_posture,
                    backend_health_snapshot: health_snapshot,
                    recovery_mode: context.recovery_mode,
                    budget_ok: context.budget_ok,
                };

                let decision = router.route(&inputs)?;
                // ── S14.1 circuit breaker consultation ──
                if let Some(ref reg) = self.breaker_registry {
                    reg.try_admit(decision.chosen_backend).await?;
                }
                let rid = decision.routing_id.clone();
                (
                    decision.chosen_backend,
                    decision.provider_class,
                    decision.degraded,
                    decision.reason,
                    Some(rid),
                )
            } else {
                // ── T-095 stub (backward compat; no router configured) ──
                let (be, pc, dg, rsn) = match intent.privacy_class {
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
                // ── S14.1 circuit breaker consultation (stub path) ──
                if let Some(ref reg) = self.breaker_registry {
                    reg.try_admit(be).await?;
                }
                (be, pc, dg, rsn, None)
            };

        let routing_decision_id =
            routing_decision_id.unwrap_or_else(|| format!("rtdg_{}", ulid::Ulid::new()));

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
                translator_version: "0.1.0-T098".into(),
                model_used: format!("{chosen_backend:?}").to_lowercase(),
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
