//! MINIX-inspired Watchdog Timer for AIOS recovery.
//!
//! Actively monitors component health: if a component doesn't report health
//! (via `ping`) within a timeout window, it gets automatically flagged as
//! Degraded or Failed.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the watchdog vocabulary"
)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Watchdog policy (declarative)
// ---------------------------------------------------------------------------

/// Per-component watchdog timeout configuration.
///
/// Loaded at boot time alongside [`crate::SelfHealingPolicy`]; defines which
/// components are actively watched and how long the timer window is before the
/// component is flagged as Degraded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct WatchdogPolicy {
    /// Global toggle — when `false`, the watchdog never auto-flags components.
    pub enabled: bool,
    /// Default timeout in seconds applied to components not listed in
    /// [`Self::component_timeouts`].
    pub default_timeout_secs: u64,
    /// Per-component timeout overrides (component id → timeout in seconds).
    #[serde(default)]
    pub component_timeouts: HashMap<String, u64>,
}

impl Default for WatchdogPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            default_timeout_secs: 30,
            component_timeouts: HashMap::new(),
        }
    }
}

impl WatchdogPolicy {
    /// Return the effective timeout [`Duration`] for the given component id.
    #[must_use]
    pub fn timeout_for(&self, component_id: &str) -> Duration {
        let secs = self
            .component_timeouts
            .get(component_id)
            .copied()
            .unwrap_or(self.default_timeout_secs);
        Duration::from_secs(secs)
    }
}

// ---------------------------------------------------------------------------
// Watchdog timer (runtime)
// ---------------------------------------------------------------------------

/// Active watchdog timer that tracks per-component health deadlines.
///
/// Each registered component has a deadline (`Instant`).  Calling [`ping`]
/// resets the deadline to `now + timeout`.  [`check_deadlines`] returns the
/// ids of every component whose deadline has passed *and* whose policy is
/// enabled.
///
/// Internally uses [`tokio::sync::RwLock`] for safe concurrent access.
#[derive(Debug)]
pub struct WatchdogTimer {
    policy: RwLock<WatchdogPolicy>,
    deadlines: RwLock<HashMap<String, Instant>>,
}

impl Default for WatchdogTimer {
    fn default() -> Self {
        Self {
            policy: RwLock::new(WatchdogPolicy::default()),
            deadlines: RwLock::new(HashMap::new()),
        }
    }
}

impl WatchdogTimer {
    /// Create a new timer with the given declarative policy.
    #[must_use]
    pub fn new(policy: WatchdogPolicy) -> Self {
        Self {
            policy: RwLock::new(policy),
            deadlines: RwLock::new(HashMap::new()),
        }
    }

    /// Replace the current watchdog policy at runtime.
    pub async fn set_policy(&self, policy: WatchdogPolicy) {
        let mut guard = self.policy.write().await;
        *guard = policy;
    }

    /// Return a snapshot of the current watchdog policy.
    #[must_use]
    pub async fn policy(&self) -> WatchdogPolicy {
        self.policy.read().await.clone()
    }

    /// Register a component for watchdog monitoring.
    ///
    /// Sets an initial deadline of `now + timeout` where `timeout` is taken
    /// from the currently loaded policy.  If the component was already
    /// registered the deadline is refreshed (semantically equivalent to a
    /// [`ping`]).
    pub async fn register(&self, component_id: &str) {
        let timeout = self.policy.read().await.timeout_for(component_id);
        let mut deadlines = self.deadlines.write().await;
        deadlines.insert(component_id.to_owned(), Instant::now() + timeout);
    }

    /// Signal a liveness heartbeat from a component — reset its deadline.
    ///
    /// If the component was never registered this is a no-op (we don't
    /// implicitly register unknown components; the caller must call
    /// [`register`] first).
    pub async fn ping(&self, component_id: &str) {
        let timeout = self.policy.read().await.timeout_for(component_id);
        let mut deadlines = self.deadlines.write().await;
        deadlines.insert(component_id.to_owned(), Instant::now() + timeout);
    }

    /// Check all registered components and return the ids of those whose
    /// deadline has passed.
    ///
    /// Returns an empty `Vec` when the policy is disabled.
    #[must_use]
    pub async fn check_deadlines(&self) -> Vec<String> {
        let enabled = self.policy.read().await.enabled;
        if !enabled {
            return Vec::new();
        }
        let now = Instant::now();
        let deadlines = self.deadlines.read().await;
        deadlines
            .iter()
            .filter(|(_, &deadline)| now >= deadline)
            .map(|(id, _)| id.clone())
            .collect()
    }
}
