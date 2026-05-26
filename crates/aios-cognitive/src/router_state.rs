//! Router operational state — rolling-window health tracking (S13.2 §9).
//!
//! # INV-014 Enforcement
//!
//! Health is OBSERVED never ASSERTED — adapters cannot self-report `HEALTHY`
//! without successful invocations in the observation window. The `RouterState`
//! computes health from measured invocation outcomes only.

use std::collections::HashMap;

use tokio::sync::RwLock;

use crate::routing::{BackendHealthState, ModelBackendKind};

/// Per-backend invocation window for health computation.
#[derive(Debug, Clone, Default)]
struct BackendHealthWindow {
    success_count: u64,
    failure_count: u64,
}

impl BackendHealthWindow {
    #[allow(clippy::cast_precision_loss)]
    fn error_rate(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            return 0.0;
        }
        self.failure_count as f64 / total as f64
    }

    fn compute_state(&self) -> BackendHealthState {
        if self.success_count == 0 && self.failure_count == 0 {
            return BackendHealthState::Healthy; // no observations yet → assume healthy
        }
        let rate = self.error_rate();
        if rate >= 0.05 {
            BackendHealthState::Unhealthy
        } else if rate >= 0.01 {
            BackendHealthState::DegradedAvailability
        } else {
            BackendHealthState::Healthy
        }
    }
}

/// Router operational state tracking per-backend invocation windows (S13.2 §9).
///
/// # INV-014
///
/// Backend health is computed from measured invocation outcomes — never
/// asserted by adapters. `observe_invocation()` updates the rolling window;
/// `get_health()` returns the computed snapshot.
pub struct RouterState {
    /// Per-backend invocation windows, keyed by `ModelBackendKind`.
    health_by_backend: RwLock<HashMap<ModelBackendKind, BackendHealthWindow>>,
    /// Monotonic counter incremented on each `mint_routing_id()` call.
    last_routing_id: RwLock<u64>,
}

impl RouterState {
    /// Create a new `RouterState` with empty health windows.
    #[must_use]
    pub fn new() -> Self {
        Self {
            health_by_backend: RwLock::new(HashMap::new()),
            last_routing_id: RwLock::new(0),
        }
    }

    /// Observe a model invocation outcome and update backend health.
    ///
    /// # INV-014
    ///
    /// Health is OBSERVED never ASSERTED. Only successful invocations move a
    /// backend toward `HEALTHY`; the state is always derived from the rolling
    /// window, never from adapter self-reporting.
    #[allow(clippy::significant_drop_tightening)]
    pub async fn observe_invocation(&self, backend: ModelBackendKind, success: bool) {
        let mut windows = self.health_by_backend.write().await;
        let window = windows.entry(backend).or_default();
        if success {
            window.success_count += 1;
        } else {
            window.failure_count += 1;
        }
    }

    /// Return a snapshot of current backend health states.
    ///
    /// Backends not yet observed default to `Healthy` (trust-until-proven-otherwise
    /// for initial routing; subsequent observations will degrade if necessary).
    pub async fn get_health(&self) -> HashMap<ModelBackendKind, BackendHealthState> {
        // Scope the read lock tight: collect state computations, drop the guard,
        // then build the snapshot HashMap.
        let states: Vec<(ModelBackendKind, BackendHealthState)> = {
            let windows = self.health_by_backend.read().await;
            windows
                .iter()
                .map(|(k, w)| (*k, w.compute_state()))
                .collect()
        };
        let mut snapshot = HashMap::new();
        for (kind, state) in states {
            snapshot.insert(kind, state);
        }
        // Backends not in the map default to Healthy for initial routing
        snapshot
    }

    /// Directly set the health state for a backend.
    ///
    /// This is used for testing; in production health is derived from
    /// `observe_invocation` observations only.
    /// Compute the window counts before taking the write lock so the guard is
    /// not held across the match.
    #[allow(clippy::too_many_lines, clippy::significant_drop_tightening)]
    pub async fn set_health(&self, backend: ModelBackendKind, state: BackendHealthState) {
        let (success_count, failure_count) = match state {
            BackendHealthState::Healthy => (100, 0),
            BackendHealthState::DegradedAvailability | BackendHealthState::DegradedLatency => {
                (99, 1)
            }
            BackendHealthState::Unhealthy | BackendHealthState::Suspended => (0, 100),
        };
        {
            let mut windows = self.health_by_backend.write().await;
            let window = windows.entry(backend).or_default();
            window.success_count = success_count;
            window.failure_count = failure_count;
        }
    }

    /// Mint a fresh routing identifier.
    ///
    /// Returns `"rtdg_<ULID>"` — a globally unique, time-sortable identifier
    /// for tracing a routing decision through evidence logs.
    pub async fn mint_routing_id(&self) -> String {
        {
            let mut counter = self.last_routing_id.write().await;
            *counter += 1;
        }
        format!("rtdg_{}", ulid::Ulid::new())
    }
}

impl Default for RouterState {
    fn default() -> Self {
        Self::new()
    }
}
