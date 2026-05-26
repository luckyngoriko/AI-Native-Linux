//! `aios-cognitive` — L5 Cognitive Core typed skeleton (S1.1, S1.2, S13.1, S13.2, S14.1).
//!
//! Types-only crate; no implementation logic, no gRPC, no evidence emission.

#![forbid(unsafe_code)]

/// S14.1 circuit breaker types.
pub mod circuit;
/// `CognitiveCore` async trait + `TranslationContext` + `IntentCapability` (S1.1).
pub mod core;
/// `CognitiveError` taxonomy.
pub mod error;
/// `InMemoryCognitiveCore` — test/prototype implementation of `CognitiveCore`.
pub mod in_memory_core;
/// `CognitiveIntent` + `IntentId` + `SubjectRef`.
pub mod intent;
/// `LatencyTier` + `PrivacyClass`.
pub mod latency;
/// Latency tiering classifier per S1.2 (stub heuristic; replaced M12+).
pub mod latency_classifier;
/// `CognitiveModel` + `ModelId`.
pub mod model;
/// Model router types (S13.2).
pub mod routing;
/// `TranslationResult` + `TranslationProvenance`.
pub mod translator;

// Re-exports — flattened public surface
pub use circuit::{CircuitBreakerConfig, CircuitBreakerStats, CircuitState};
pub use core::{CognitiveCore, IntentCapability, TranslationContext};
pub use error::CognitiveError;
pub use in_memory_core::InMemoryCognitiveCore;
pub use intent::{CognitiveIntent, IntentId, SubjectRef};
pub use latency::{LatencyTier, PrivacyClass};
pub use latency_classifier::LatencyClassifier;
pub use model::{CognitiveModel, ModelId};
pub use routing::{
    AICrossOriginPosture, BackendHealthEntry, BackendHealthState, ModelBackendKind, ProviderClass,
    RoutingDecision, RoutingInputs,
};
pub use translator::{TranslationProvenance, TranslationResult};

/// Crate version marker — bump on every semantic change.
pub const DEFAULT_CODE_VERSION: &str = "aios-cognitive/0.0.1-T094";
