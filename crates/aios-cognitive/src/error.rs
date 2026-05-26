use thiserror::Error;

use crate::routing::AICrossOriginPosture;

/// Closed error taxonomy for the L5 Cognitive Core.
///
/// Every error path in the cognitive pipeline maps to one of these variants.
/// The taxonomy is closed; adding a variant is a versioned spec change.
#[derive(Debug, Error)]
pub enum CognitiveError {
    /// The intent could not be parsed from the input utterance.
    #[error("intent parse failed: {0}")]
    IntentParseFailed(String),

    /// The translator could not find a matching capability for the given intent.
    #[error("no matching capability for intent: {0}")]
    NoMatchingCapability(String),

    /// The translator refused to translate — action would be unsafe by construction.
    #[error("translation refused: {0}")]
    TranslationRefused(String),

    /// Ambiguous intent — multiple capabilities match; clarification needed.
    #[error("ambiguous intent: {0}")]
    AmbiguousIntent(String),

    /// The latency tier requested is incompatible with the privacy class.
    #[error("latency tier incompatible with privacy class: {0}")]
    LatencyPrivacyConflict(String),

    /// The model router could not select a backend — all candidates unhealthy or forbidden.
    #[error("no route available: {0}")]
    NoRouteAvailable(String),

    /// The circuit breaker is open for the requested backend.
    #[error("circuit breaker open for backend: {0}")]
    CircuitBreakerOpen(String),

    /// The model returned a response, but it failed structural validation.
    #[error("model response invalid: {0}")]
    ModelResponseInvalid(String),

    /// An internal error occurred (programmer error — should not reach the user).
    #[error("internal cognitive error: {0}")]
    Internal(String),

    /// External backend blocked by AI cross-origin posture.
    ///
    /// S8.1 §5.7 bypass-attempt guard: when `posture` is `AI_NO_EXTERNAL`
    /// and the provider class requires external vault-brokered access, the
    /// dispatch is refused. Real `AI_DIRECT_INTERNET_DENIED` evidence
    /// emission lands in T-102.
    #[error("external backend blocked by posture {posture:?}")]
    ExternalBackendBlocked {
        /// The posture that blocked the dispatch.
        posture: AICrossOriginPosture,
    },

    /// Vault capability id missing on a model that requires external access.
    ///
    /// INV-018: external providers (`Anthropic`, `Openai`,
    /// `OtherVaultBrokered`) must carry a `vault_capability_id` so the
    /// dispatch can route through the L4.2 vault broker.
    #[error("vault credential missing for model {0}")]
    VaultCredentialMissing(String),

    /// Evidence emission failed — the cognitive pipeline could not append
    /// a signed receipt to the evidence log.
    #[error("evidence emission failed: {0}")]
    EvidenceEmitFailed(String),
}
