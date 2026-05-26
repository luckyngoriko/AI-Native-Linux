//! Circuit breaker registry — per-backend breaker management (S14.1 §9.4).
//!
//! One `CircuitBreaker` per `ModelBackendKind` variant, keyed for consultation
//! by the model router before dispatch decisions.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::breaker::{AdmissionTicket, CircuitBreaker};
use crate::circuit::{CircuitBreakerConfig, CircuitState};
use crate::error::CognitiveError;
use crate::routing::ModelBackendKind;

/// Registry of circuit breakers keyed by `ModelBackendKind`.
///
/// # Lifecycle
///
/// - Created via `new_with_defaults()` — one breaker per backend variant with
///   default config.
/// - `get()` returns a shared reference for state inspection or admission.
/// - `observe_and_update()` records invocation outcomes and advances state.
pub struct CircuitBreakerRegistry {
    breakers: RwLock<HashMap<ModelBackendKind, Arc<CircuitBreaker>>>,
}

impl CircuitBreakerRegistry {
    /// Create a registry with one `CircuitBreaker` per `ModelBackendKind` variant
    /// using default `CircuitBreakerConfig`.
    #[must_use]
    pub fn new_with_defaults() -> Self {
        let mut breakers = HashMap::new();
        let all_kinds = [
            ModelBackendKind::LocalCpu,
            ModelBackendKind::LocalGpu,
            ModelBackendKind::LocalDistributed,
            ModelBackendKind::ExternalVaultBrokered,
            ModelBackendKind::FallbackRuleBased,
            ModelBackendKind::Cached,
            ModelBackendKind::DegradedNull,
            ModelBackendKind::Forbidden,
        ];
        for kind in all_kinds {
            breakers.insert(
                kind,
                Arc::new(CircuitBreaker::new(kind, CircuitBreakerConfig::default())),
            );
        }
        Self {
            breakers: RwLock::new(breakers),
        }
    }

    /// Return the `CircuitBreaker` for the given backend kind.
    ///
    /// Returns `None` if no breaker is registered for the kind (should not happen
    /// with `new_with_defaults`).
    pub async fn get(&self, backend: ModelBackendKind) -> Option<Arc<CircuitBreaker>> {
        self.breakers.read().await.get(&backend).cloned()
    }

    /// Observe an invocation outcome and advance the breaker state for the given backend.
    ///
    /// Returns the resulting circuit state after the observation is recorded.
    /// If no breaker is registered for the backend, no-op and returns `None`.
    pub async fn observe_and_update(
        &self,
        backend: ModelBackendKind,
        succeeded: bool,
        latency_ms: u64,
    ) -> Option<CircuitState> {
        let breaker = self.get(backend).await?;
        Some(breaker.record_outcome(succeeded, latency_ms).await)
    }

    /// Try to admit a call through the breaker for the given backend.
    ///
    /// Returns an `AdmissionTicket` on success, or `CognitiveError::CircuitBreakerOpen`
    /// if the circuit is open or the half-open probe limit is reached.
    ///
    /// # Errors
    ///
    /// Returns `CircuitBreakerOpen` when the circuit rejects the call.
    pub async fn try_admit(
        &self,
        backend: ModelBackendKind,
    ) -> Result<AdmissionTicket, CognitiveError> {
        match self.get(backend).await {
            Some(breaker) => breaker.try_admit().await,
            None => Err(CognitiveError::CircuitBreakerOpen(format!(
                "no breaker registered for backend: {backend:?}"
            ))),
        }
    }

    /// Return a snapshot of all breaker states keyed by backend kind.
    pub async fn all_states(&self) -> HashMap<ModelBackendKind, CircuitState> {
        let entries: Vec<_> = {
            let breakers = self.breakers.read().await;
            breakers.iter().map(|(k, b)| (*k, Arc::clone(b))).collect()
        };
        let mut states = HashMap::new();
        for (kind, breaker) in entries {
            states.insert(kind, breaker.current_state().await);
        }
        states
    }
}

impl Default for CircuitBreakerRegistry {
    fn default() -> Self {
        Self::new_with_defaults()
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        clippy::panic,
        clippy::unwrap_used,
        reason = "panic-on-failure is the idiomatic test signal"
    )]

    use super::*;

    #[tokio::test]
    async fn registry_has_all_eight_backends() {
        let registry = CircuitBreakerRegistry::new_with_defaults();
        let all_kinds = [
            ModelBackendKind::LocalCpu,
            ModelBackendKind::LocalGpu,
            ModelBackendKind::LocalDistributed,
            ModelBackendKind::ExternalVaultBrokered,
            ModelBackendKind::FallbackRuleBased,
            ModelBackendKind::Cached,
            ModelBackendKind::DegradedNull,
            ModelBackendKind::Forbidden,
        ];
        for kind in all_kinds {
            assert!(
                registry.get(kind).await.is_some(),
                "missing breaker for {kind:?}"
            );
        }
    }

    #[tokio::test]
    async fn all_breakers_start_closed() {
        let registry = CircuitBreakerRegistry::new_with_defaults();
        let states = registry.all_states().await;
        for (kind, state) in states {
            assert_eq!(state, CircuitState::Closed, "{kind:?} not closed");
        }
    }

    #[tokio::test]
    async fn observe_and_update_changes_state() {
        let registry = CircuitBreakerRegistry::new_with_defaults();
        // Record enough failures to open the LocalGpu breaker.
        for _ in 0..10 {
            registry
                .observe_and_update(ModelBackendKind::LocalGpu, false, 500)
                .await;
        }
        let breaker = registry.get(ModelBackendKind::LocalGpu).await.unwrap();
        assert_eq!(breaker.current_state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn try_admit_respects_circuit_state() {
        let registry = CircuitBreakerRegistry::new_with_defaults();
        // Initially closed — admission works.
        let result = registry.try_admit(ModelBackendKind::LocalCpu).await;
        assert!(result.is_ok());

        // Force open.
        for _ in 0..10 {
            registry
                .observe_and_update(ModelBackendKind::LocalCpu, false, 500)
                .await;
        }
        let result = registry.try_admit(ModelBackendKind::LocalCpu).await;
        assert!(result.is_err());
    }
}
