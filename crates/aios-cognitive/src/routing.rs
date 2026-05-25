use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

use crate::latency::{LatencyTier, PrivacyClass};

// ---------------------------------------------------------------------------
// S13.2 §4 — ModelBackendKind (closed, 8 values)
// ---------------------------------------------------------------------------

/// Closed enum of backend kinds per S13.2 §4.
///
/// New backend identities require a versioned spec change. Adapters cannot
/// synthesise a new `ModelBackendKind` value through capability negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelBackendKind {
    /// CPU-only inference (e.g. llama.cpp on host CPU).
    LocalCpu = 1,
    /// Single-host GPU inference (e.g. vLLM / Ollama with GPU).
    LocalGpu = 2,
    /// Multi-host LAN inference cluster.
    LocalDistributed = 3,
    /// External provider through L4.2 vault broker.
    ExternalVaultBrokered = 4,
    /// Deterministic non-LLM fallback (regex / templates).
    FallbackRuleBased = 5,
    /// Pre-existing T0 result returned by router cache.
    Cached = 6,
    /// No backend available; deliberate null.
    DegradedNull = 7,
    /// Constitutional refuse (privacy / posture).
    Forbidden = 8,
}

// ---------------------------------------------------------------------------
// S13.2 §5 — ProviderClass (closed, 5 values)
// ---------------------------------------------------------------------------

/// Closed enum of provider classes per S13.2 §5.
///
/// The `OtherVaultBrokered` slot is for providers whose adapter is `AIOS_VERIFIED`
/// and which use the L4.2 broker pattern. The discriminator is the adapter
/// `package_id`, not a freeform string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProviderClass {
    /// Anthropic provider.
    Anthropic = 1,
    /// `OpenAI` provider.
    Openai = 2,
    /// Local Ollama runtime.
    Ollama = 3,
    /// Local / LAN vLLM cluster.
    Vllm = 4,
    /// Any other provider mediated by L4.2 broker.
    OtherVaultBrokered = 5,
}

// ---------------------------------------------------------------------------
// S8.1 §4.9 — AICrossOriginPosture (consumed by S13.2, closed, 3 values)
// ---------------------------------------------------------------------------

/// AI cross-origin posture per S8.1 §4.9 — consumed by the model router as a
/// routing input that gates external backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AICrossOriginPosture {
    /// External providers allowed through the vault broker.
    AiVaultBrokeredOnly = 1,
    /// No external provider connectivity permitted.
    AiNoExternal = 2,
    /// Loopback-only; no LAN peers.
    AiLoopbackOnly = 3,
}

// ---------------------------------------------------------------------------
// S13.2 §9.1 — BackendHealthState (closed, 5 values)
// ---------------------------------------------------------------------------

/// Closed enum of backend health states per S13.2 §9.1.
///
/// Health is observed from measured invocations, never self-reported by adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BackendHealthState {
    /// `error_rate` < 1 %; p95 within declared budget × 1.5.
    Healthy = 1,
    /// p95 between declared × 1.5 and × 3; `error_rate` < 1 %.
    DegradedLatency = 2,
    /// 1 % ≤ `error_rate` < 5 %.
    DegradedAvailability = 3,
    /// `error_rate` ≥ 5 % (closed → open transition trigger).
    Unhealthy = 4,
    /// Operator-suspended (manual takedown per S11.1).
    Suspended = 5,
}

// ---------------------------------------------------------------------------
// RoutingInputs — the tuple that feeds the deterministic precedence table (§7)
// ---------------------------------------------------------------------------

/// Inputs to the model router's deterministic precedence table (S13.2 §7).
///
/// Given identical inputs and `code_version`, the router must return the same
/// `ModelBackendKind` decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingInputs {
    /// Latency class requested by S1.2.
    pub latency_class: LatencyTier,
    /// Privacy class of the material.
    pub privacy_class: PrivacyClass,
    /// Cross-origin posture from network policy.
    pub ai_cross_origin_posture: AICrossOriginPosture,
    /// Snapshot of relevant backend health states.
    pub backend_health_snapshot: Vec<BackendHealthEntry>,
    /// Whether the system is in recovery mode.
    pub recovery_mode: bool,
    /// Whether the subject's external-model budget is not exhausted.
    pub budget_ok: bool,
}

/// A single entry in the backend health snapshot carried in routing inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendHealthEntry {
    /// Which backend kind this entry describes.
    pub backend_kind: ModelBackendKind,
    /// The provider class of this backend.
    pub provider_class: ProviderClass,
    /// Current health state as observed by the router.
    pub state: BackendHealthState,
    /// Circuit breaker configuration for this backend.
    pub config: crate::circuit::CircuitBreakerConfig,
    /// Circuit breaker rolling-window statistics.
    pub stats: crate::circuit::CircuitBreakerStats,
}

// ---------------------------------------------------------------------------
// RoutingDecision — the deterministic output of the router (§7)
// ---------------------------------------------------------------------------

/// The result of a model router decision (S13.2 §7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// The routing decision id (`rtdg_<ULID>`).
    pub routing_id: String,
    /// The chosen backend kind.
    pub chosen_backend: ModelBackendKind,
    /// The provider class of the chosen backend.
    pub provider_class: ProviderClass,
    /// Adapter `package_id` of the chosen backend.
    pub backend_id: String,
    /// Which precedence rule matched (1–13).
    pub matched_rule: u32,
    /// `true` when the result is a degraded choice (rules 11 / 12).
    pub degraded: bool,
    /// Reason code when degraded or forbidden.
    pub reason: Option<String>,
    /// When the decision was made.
    pub decided_at: DateTime<Utc>,
}
