use std::collections::HashMap;
use std::sync::Arc;

use crate::composition::ServiceComposition;
use crate::composition_engine::{default_aios_composition, CompositionEngine};
use crate::error::IntegrationError;

/// Orchestrator that consumes a typed `ServiceComposition` and drives
/// the boot sequence in topological order.
///
/// Pure type-level orchestration — no process spawning, no socket binding.
pub struct Orchestrator {
    composition: ServiceComposition,
    engine: Arc<CompositionEngine>,
}

/// A per-service health summary suitable for downstream tooling.
pub struct ServiceHealthSummary {
    /// Unique service identifier within the composition.
    pub service_id: String,
    /// Name of the AIOS crate implementing this service.
    pub crate_name: String,
    /// Scaffold-level status (no real health probes yet).
    pub status: ServiceScaffoldStatus,
    /// Position in the topological boot order (0-based).
    pub topological_index: usize,
}

/// Scaffold-level health status for a service.
///
/// No real process health probes exist yet — this gives downstream
/// tooling a shape to populate when real health checks land (M19+).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceScaffoldStatus {
    /// Service is present in the composition and the scaffold is ready.
    ScaffoldReady,
    /// Service is not present in the composition.
    NotInComposition,
    /// Service is present but its configuration is missing.
    ConfigMissing,
}

impl Orchestrator {
    /// Creates an `Orchestrator` from the canonical 17-crate default composition.
    ///
    /// The default composition is known-acyclic by construction;
    /// the fresh engine is wired for `validate_external_composition` use.
    ///
    /// # Errors
    ///
    /// This function is infallible — the `Result` return type is for
    /// caller ergonomics with `?` in bootstrap code.
    pub fn from_default_composition() -> Result<Self, IntegrationError> {
        let composition = default_aios_composition();
        let engine = Arc::new(CompositionEngine::new());
        Ok(Self {
            composition,
            engine,
        })
    }

    /// Returns the topological boot order (service IDs in dependency-respecting order).
    #[allow(clippy::unused_async)]
    pub async fn boot_order(&self) -> Vec<String> {
        self.composition.topological_order.clone()
    }

    /// Returns a health summary entry for every service in the composition.
    ///
    /// `topological_index` reflects the position in the topological order,
    /// not the service registration order.
    #[allow(clippy::unused_async)]
    pub async fn health_summary(&self) -> Vec<ServiceHealthSummary> {
        let order = &self.composition.topological_order;
        let pos_map: HashMap<&str, usize> = order
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();

        self.composition
            .services
            .iter()
            .map(|svc| ServiceHealthSummary {
                topological_index: *pos_map.get(svc.service_id.as_str()).unwrap_or(&0),
                service_id: svc.service_id.clone(),
                crate_name: svc.crate_name.clone(),
                status: ServiceScaffoldStatus::ScaffoldReady,
            })
            .collect()
    }

    /// Validates an external `ServiceComposition` without storing it.
    ///
    /// Returns the topological order on success.
    ///
    /// # Errors
    ///
    /// * `ComposedServiceMissing` — a dependency references a missing service.
    /// * `CompositionCycleDetected` — the dependency graph contains a directed cycle.
    pub async fn validate_external_composition(
        &self,
        composition: &ServiceComposition,
    ) -> Result<Vec<String>, IntegrationError> {
        self.engine.validate(composition).await
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    reason = "test code; unwrap-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use crate::composition::ComposedService;
    use crate::ids::ComposedSystemId;

    fn orchestrator() -> Orchestrator {
        Orchestrator::from_default_composition().unwrap()
    }

    #[test]
    fn from_default_composition_succeeds() {
        let orch = Orchestrator::from_default_composition();
        assert!(orch.is_ok());
    }

    #[tokio::test]
    async fn boot_order_has_17_entries() {
        let orch = orchestrator();
        let order = orch.boot_order().await;
        assert_eq!(order.len(), 17);
    }

    #[tokio::test]
    async fn boot_order_first_is_aios_action() {
        let orch = orchestrator();
        let order = orch.boot_order().await;
        assert_eq!(order[0], "aios-action");
    }

    #[tokio::test]
    async fn boot_order_last_is_aios_hardware() {
        let orch = orchestrator();
        let order = orch.boot_order().await;
        assert_eq!(order[16], "aios-hardware");
    }

    #[tokio::test]
    async fn boot_order_evidence_before_policy() {
        let orch = orchestrator();
        let order = orch.boot_order().await;
        let evidence_pos = order.iter().position(|s| s == "aios-evidence").unwrap();
        let policy_pos = order.iter().position(|s| s == "aios-policy").unwrap();
        assert!(evidence_pos < policy_pos);
    }

    #[tokio::test]
    async fn boot_order_policy_before_capability_runtime() {
        let orch = orchestrator();
        let order = orch.boot_order().await;
        let policy_pos = order.iter().position(|s| s == "aios-policy").unwrap();
        let cr_pos = order
            .iter()
            .position(|s| s == "aios-capability-runtime")
            .unwrap();
        assert!(policy_pos < cr_pos);
    }

    #[tokio::test]
    async fn boot_order_renderer_cli_before_renderer_kde() {
        let orch = orchestrator();
        let order = orch.boot_order().await;
        let cli_pos = order.iter().position(|s| s == "aios-renderer-cli").unwrap();
        let kde_pos = order.iter().position(|s| s == "aios-renderer-kde").unwrap();
        assert!(cli_pos < kde_pos);
    }

    #[tokio::test]
    async fn health_summary_returns_17_entries() {
        let orch = orchestrator();
        let summaries = orch.health_summary().await;
        assert_eq!(summaries.len(), 17);
    }

    #[tokio::test]
    async fn health_summary_all_status_scaffold_ready_by_default() {
        let orch = orchestrator();
        let summaries = orch.health_summary().await;
        for s in &summaries {
            assert_eq!(s.status, ServiceScaffoldStatus::ScaffoldReady);
        }
    }

    #[tokio::test]
    async fn health_summary_topological_index_matches_position() {
        let orch = orchestrator();
        let order = orch.boot_order().await;
        let summaries = orch.health_summary().await;

        for summary in &summaries {
            let pos = order
                .iter()
                .position(|id| id == &summary.service_id)
                .unwrap();
            assert_eq!(summary.topological_index, pos);
        }
    }

    #[tokio::test]
    async fn validate_external_composition_with_valid_succeeds() {
        let orch = orchestrator();
        let comp = ServiceComposition {
            composition_id: ComposedSystemId("ext-valid".into()),
            services: vec![
                ComposedService {
                    service_id: "a".into(),
                    crate_name: "a".into(),
                    binding_endpoint: "unix:/run/a.sock".into(),
                    depends_on: vec![],
                },
                ComposedService {
                    service_id: "b".into(),
                    crate_name: "b".into(),
                    binding_endpoint: "unix:/run/b.sock".into(),
                    depends_on: vec!["a".into()],
                },
            ],
            dependencies: vec![crate::composition::ServiceDependency {
                from_service: "b".into(),
                to_service: "a".into(),
                required: true,
            }],
            topological_order: vec![],
        };
        let order = orch.validate_external_composition(&comp).await.unwrap();
        assert_eq!(order, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn validate_external_composition_with_cycle_returns_composition_cycle_detected() {
        let orch = orchestrator();
        let comp = ServiceComposition {
            composition_id: ComposedSystemId("ext-cycle".into()),
            services: vec![
                ComposedService {
                    service_id: "x".into(),
                    crate_name: "x".into(),
                    binding_endpoint: "unix:/run/x.sock".into(),
                    depends_on: vec!["y".into()],
                },
                ComposedService {
                    service_id: "y".into(),
                    crate_name: "y".into(),
                    binding_endpoint: "unix:/run/y.sock".into(),
                    depends_on: vec!["x".into()],
                },
            ],
            dependencies: vec![
                crate::composition::ServiceDependency {
                    from_service: "x".into(),
                    to_service: "y".into(),
                    required: true,
                },
                crate::composition::ServiceDependency {
                    from_service: "y".into(),
                    to_service: "x".into(),
                    required: true,
                },
            ],
            topological_order: vec![],
        };
        let err = orch.validate_external_composition(&comp).await.unwrap_err();
        assert!(matches!(
            err,
            IntegrationError::CompositionCycleDetected { .. }
        ));
    }

    #[tokio::test]
    async fn validate_external_composition_with_missing_dep_returns_composed_service_missing() {
        let orch = orchestrator();
        let comp = ServiceComposition {
            composition_id: ComposedSystemId("ext-missing".into()),
            services: vec![ComposedService {
                service_id: "only".into(),
                crate_name: "only".into(),
                binding_endpoint: "unix:/run/only.sock".into(),
                depends_on: vec![],
            }],
            dependencies: vec![crate::composition::ServiceDependency {
                from_service: "only".into(),
                to_service: "missing".into(),
                required: true,
            }],
            topological_order: vec![],
        };
        let err = orch.validate_external_composition(&comp).await.unwrap_err();
        assert!(matches!(
            err,
            IntegrationError::ComposedServiceMissing { .. }
        ));
    }
}
