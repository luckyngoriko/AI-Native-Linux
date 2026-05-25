use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

// ---------------------------------------------------------------------------
// S14.1 §6 — CircuitState (closed, 3 values)
// ---------------------------------------------------------------------------

/// Circuit breaker state per S14.1 §6 — consumed by S13.2 model router (§9.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "PascalCase")]
pub enum CircuitState {
    /// Calls go through normally.
    Closed,
    /// All calls rejected without dispatch.
    Open,
    /// Exactly one probe call admitted.
    HalfOpen,
}

// ---------------------------------------------------------------------------
// S13.2 §9.3 — CircuitBreakerConfig
// ---------------------------------------------------------------------------

/// Per-backend circuit breaker configuration per S13.2 §9.3.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Error rate threshold that triggers open (default 0.05 = 5 %).
    pub error_rate_threshold: f64,
    /// Rolling window duration in seconds (default 300 = 5 min).
    pub window_seconds: u32,
    /// Initial cool-down in seconds after opening (default 30).
    pub initial_cooldown_seconds: u32,
    /// Maximum cool-down in seconds (600 per S14.1 §6.2).
    pub max_cooldown_seconds: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            error_rate_threshold: 0.05,
            window_seconds: 300,
            initial_cooldown_seconds: 30,
            max_cooldown_seconds: 600,
        }
    }
}

// ---------------------------------------------------------------------------
// S13.2 §9 — CircuitBreakerStats (per-backend rolling window)
// ---------------------------------------------------------------------------

/// Per-backend circuit breaker statistics and current state (S13.2 §9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStats {
    /// Current circuit state.
    pub state: CircuitState,
    /// Success count in the current rolling window.
    pub success_count: u64,
    /// Failure count in the current rolling window.
    pub failure_count: u64,
    /// Current error rate = `failure_count` / max(1, `success_count` + `failure_count`).
    pub error_rate: f64,
    /// Remaining cool-down seconds (0 when closed).
    pub cooldown_seconds: u32,
    /// When the state last changed.
    pub last_state_change_at: DateTime<Utc>,
    /// When the next half-open probe is eligible (None when closed).
    pub next_probe_at: Option<DateTime<Utc>>,
}
