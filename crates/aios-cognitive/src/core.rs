//! `CognitiveCore` async trait — the capability translator surface (S1.1).
//!
//! # INV-002 Enforcement
//!
//! Every `translate_intent` path MUST produce a typed [`aios_action::ActionEnvelope`].
//! Raw shell commands are never produced at this layer — the Capability Runtime is
//! the sole executor, and only after a Policy Kernel decision.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::CognitiveError;
use crate::intent::{CognitiveIntent, IntentId, SubjectRef};
use crate::latency::{LatencyTier, PrivacyClass};
use crate::model::CognitiveModel;
use crate::routing::AICrossOriginPosture;
use crate::translator::TranslationResult;

/// Context carried into every `translate_intent` call.
///
/// Captures the subject, available models, and routing-relevant state so the
/// translator can make a deterministic routing decision without side-channel reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationContext {
    /// Canonical subject that authored the intent.
    pub subject: SubjectRef,
    /// Models currently registered in the catalog.
    pub available_models: Vec<CognitiveModel>,
    /// Latency tier requested.
    pub latency_class: LatencyTier,
    /// Privacy class of the material.
    pub privacy_class: PrivacyClass,
    /// Cross-origin posture from network policy.
    pub ai_cross_origin_posture: AICrossOriginPosture,
    /// Whether the system is in recovery mode.
    pub recovery_mode: bool,
    /// Whether the subject's external-model budget is not exhausted.
    pub budget_ok: bool,
}

/// Describes what an intent kind can do — returned by `list_supported_intents`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentCapability {
    /// The intent kind this capability describes (e.g. `service.restart`).
    pub intent_kind: String,
    /// Human-readable description of what this capability enables.
    pub description: String,
    /// Minimum latency tier required to use this capability.
    pub requires_latency_tier: LatencyTier,
    /// The action type this intent produces (e.g. `service.restart`).
    pub produces_action_type: String,
    /// Estimated max tokens for translation of this intent kind.
    pub max_tokens_estimate: u32,
}

/// The cognitive core capability translator (S1.1).
///
/// # INV-002 Enforcement
///
/// Every `translate_intent` implementation MUST produce a typed
/// [`aios_action::ActionEnvelope`]. Raw shell commands are never produced at this
/// layer — the Capability Runtime is the sole executor, and only after a Policy
/// Kernel decision.
#[async_trait]
pub trait CognitiveCore: Send + Sync {
    /// Translate a cognitive intent into a typed action envelope.
    ///
    /// The returned [`TranslationResult`] carries a full `ActionEnvelope` ready for
    /// submission to the Capability Runtime. The routing decision embedded in the
    /// result is deterministic for a given `(intent, context)` pair.
    async fn translate_intent(
        &self,
        intent: &CognitiveIntent,
        context: &TranslationContext,
    ) -> Result<TranslationResult, CognitiveError>;

    /// List all intent kinds this cognitive core can translate.
    fn list_supported_intents(&self) -> Vec<IntentCapability>;

    /// Retrieve a previously produced translation by intent id.
    async fn get_translation(
        &self,
        intent_id: &IntentId,
    ) -> Result<TranslationResult, CognitiveError>;
}
