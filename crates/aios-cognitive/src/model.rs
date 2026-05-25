use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::routing::ProviderClass;

/// ULID-bodied newtype for cognitive model identifiers.
///
/// Wire prefix: `mdl_<ULID>` (26 base32 chars).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelId(pub String);

impl ModelId {
    /// Mint a fresh `mdl_<ULID>` identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(format!("mdl_{}", ulid::Ulid::new()))
    }
}

impl Default for ModelId {
    fn default() -> Self {
        Self::new()
    }
}

/// A cognitive model binding — represents a concrete model (local or external)
/// that the L5 Cognitive Core can invoke through the Model Router (S13.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveModel {
    /// Unique model identifier (`mdl_<ULID>`).
    pub model_id: ModelId,
    /// The provider class this model belongs to.
    pub provider: ProviderClass,
    /// Declared capabilities — what this model can do (e.g. `text-generation`, `code-completion`).
    pub capabilities: Vec<String>,
    /// Maximum context window in tokens.
    pub max_tokens: u32,
    /// Cost per 1 000 input tokens in micro-units of the declared currency.
    pub input_cost_per_1k: u64,
    /// Cost per 1 000 output tokens in micro-units of the declared currency.
    pub output_cost_per_1k: u64,
    /// Vault capability id for external providers (`vcap_<ULID>`).
    ///
    /// `None` for local backends that do not require a vault capability.
    pub vault_capability_id: Option<String>,
    /// When this model binding was created.
    pub created_at: DateTime<Utc>,
}
