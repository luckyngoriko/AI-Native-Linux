use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::intent::IntentId;

/// Provenance record documenting which model produced a translation (S1.1 §13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationProvenance {
    /// Version string of the translator that produced this result.
    pub translator_version: String,
    /// Identifier of the model used (e.g. `claude-sonnet-4-6`).
    pub model_used: String,
    /// Number of tokens consumed by the input prompt.
    pub tokens_in: u32,
    /// Number of tokens produced in the output.
    pub tokens_out: u32,
    /// Raw model response that was parsed into the structured translation.
    ///
    /// Carried for audit; never treated as executable.
    pub model_signed_response: Option<String>,
}

/// Result of translating a cognitive intent into typed action envelope(s) (S1.1 §12).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResult {
    /// The intent this translation was produced from.
    pub intent_id: IntentId,
    /// The typed action envelope proposed for execution.
    ///
    /// This is a full `aios_action::ActionEnvelope` — the canonical S0.1 shape
    /// consumed by the L3 Capability Runtime.
    pub produced_action: aios_action::ActionEnvelope,
    /// The routing decision that governed model selection for translation.
    pub routing_decision_id: Option<String>,
    /// S2.4 verification intent (may be empty for read-only actions).
    pub verification_intent: Option<String>,
    /// Full provenance record for audit.
    pub translation_provenance: TranslationProvenance,
    /// When the translation was produced.
    pub translated_at: DateTime<Utc>,
}
