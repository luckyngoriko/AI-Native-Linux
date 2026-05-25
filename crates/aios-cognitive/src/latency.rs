use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Latency tier per S1.2 §3 — decides how cognition happens before an action draft exists.
///
/// T0 and T1 must work without external AI. Boot, recovery, and basic administration
/// depend on these paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LatencyTier {
    /// T0 — Cached UI/state; no model use. Hard timeout 100 ms.
    T0CachedUiState = 1,
    /// T1 — Deterministic path; no model use. Hard timeout 500 ms.
    T1Deterministic = 2,
    /// T2 — Catalog retrieval; optional rerank only. Hard timeout 1 s.
    T2CatalogRetrieval = 3,
    /// T3 — Local cognitive; one local model call. Hard timeout 3 s.
    T3LocalCognitive = 4,
    /// T4 — Powerful reasoning; one or more external/broker-mediated calls. Hard timeout 10 s.
    T4PowerfulReasoning = 5,
}

/// Privacy class per S1.2 §5 — every routing request carries one.
///
/// The router enforces tier restrictions per class. No tier may bypass typed actions,
/// policy checks, verification, or evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PrivacyClass {
    /// No sensitive info; any tier allowed.
    Public = 1,
    /// Org/project context but no secrets; T4 with policy.
    Internal = 2,
    /// Identifiable user data; T4 with policy + approval.
    Sensitive = 3,
    /// References to secret material; T4 local only (NEVER external).
    SecretBearing = 4,
    /// Operator-marked classified context; T2 max.
    Classified = 5,
}
