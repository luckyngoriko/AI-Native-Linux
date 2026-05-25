use thiserror::Error;

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
}
