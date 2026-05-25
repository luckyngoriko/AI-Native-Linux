use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::latency::{LatencyTier, PrivacyClass};

/// ULID-bodied newtype for cognitive intent identifiers.
///
/// Wire prefix: `cogi_<ULID>` (26 base32 chars).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IntentId(pub String);

impl IntentId {
    /// Mint a fresh `cogi_<ULID>` identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(format!("cogi_{}", ulid::Ulid::new()))
    }
}

impl Default for IntentId {
    fn default() -> Self {
        Self::new()
    }
}

/// Canonical subject reference — an opaque subject canonical id string per S5.1.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubjectRef(pub String);

/// A structured cognitive intent produced by the Intent Engine (S1.1 §9).
///
/// Carries the natural-language goal plus bounded context for downstream
/// translation into typed action envelopes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveIntent {
    /// Unique intent identifier (`cogi_<ULID>`).
    pub intent_id: IntentId,
    /// Canonical subject that authored the intent.
    pub subject: SubjectRef,
    /// Natural-language utterance as received from the renderer.
    pub natural_language: String,
    /// BLAKE3-256 hex digest of contextual material (prior intents, open plan steps).
    pub context_hash: String,
    /// When the intent was created.
    pub created_at: DateTime<Utc>,
    /// Latency tier assigned by S1.2 routing.
    pub latency_class: LatencyTier,
    /// Privacy class of the material in this intent.
    pub privacy_class: PrivacyClass,
}
