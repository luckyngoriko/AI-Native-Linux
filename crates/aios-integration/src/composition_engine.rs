#![allow(
    clippy::significant_drop_tightening,
    reason = "RwLock guard held briefly across composition insert; rewriting would obscure flow"
)]

use std::collections::{HashMap, HashSet, VecDeque};

use tokio::sync::RwLock;

use crate::composition::{ComposedService, ServiceComposition, ServiceDependency};
use crate::error::IntegrationError;
use crate::ids::ComposedSystemId;

/// Computes a topological order of service IDs using Kahn's algorithm.
///
/// Returns the ordered list of service IDs, or a `CompositionCycleDetected` error
/// with one cycle path if the dependency graph contains a directed cycle.
///
/// # Errors
///
/// Returns `CompositionCycleDetected` if the dependency graph contains a directed
/// cycle.
pub fn compute_topological_order(
    services: &[ComposedService],
    deps: &[ServiceDependency],
) -> Result<Vec<String>, IntegrationError> {
    let n = services.len();

    if n == 0 {
        return Ok(Vec::new());
    }

    // Build adjacency list and in-degree map keyed by service_id.
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::with_capacity(n);
    let mut in_degree: HashMap<&str, usize> = HashMap::with_capacity(n);

    for svc in services {
        adj.entry(svc.service_id.as_str()).or_default();
        in_degree.insert(svc.service_id.as_str(), 0);
    }

    // Only consider dependency edges where both endpoints are in the service set.
    let svc_set: HashSet<&str> = services.iter().map(|s| s.service_id.as_str()).collect();

    // Semantic: ServiceDependency { from, to } means "from depends on to"
    // (from is the depender, to is the dependee). For topological order,
    // dependees must come first. So in Kahn's algorithm we treat each edge
    // as `to → from` (to is a prerequisite for from), incrementing
    // in_degree[from] and storing the dependent `from` under adj[to].
    for dep in deps {
        if svc_set.contains(dep.from_service.as_str()) && svc_set.contains(dep.to_service.as_str())
        {
            adj.entry(dep.to_service.as_str())
                .or_default()
                .push(dep.from_service.as_str());
            *in_degree
                .get_mut(dep.from_service.as_str())
                .unwrap_or(&mut 0) += 1;
        }
    }

    // Kahn's algorithm.
    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut sorted: Vec<String> = Vec::with_capacity(n);

    while let Some(node) = queue.pop_front() {
        sorted.push(node.to_string());
        if let Some(neighbors) = adj.get(node) {
            for &next in neighbors {
                if let Some(deg) = in_degree.get_mut(next) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push_back(next);
                    }
                }
            }
        }
    }

    if sorted.len() < n {
        // A cycle exists — extract one cycle path from the remaining subgraph.
        // Every remaining node has in_degree > 0, so following predecessors
        // backwards must eventually revisit a node.
        let unsorted: HashSet<&str> = services
            .iter()
            .map(|s| s.service_id.as_str())
            .filter(|id| !sorted.iter().any(|s| s.as_str() == *id))
            .collect();

        let mut node = (*unsorted.iter().next().unwrap_or(&"<unknown>")).to_string();
        let mut path: Vec<String> = vec![node.clone()];
        let mut seen_pos: HashMap<String, usize> = HashMap::new();
        seen_pos.insert(node.clone(), 0);

        loop {
            // Walk backwards: find a predecessor whose target is the current node.
            let prev = deps
                .iter()
                .find(|d| d.to_service == node && unsorted.contains(d.from_service.as_str()));

            let Some(dep) = prev else {
                break;
            };

            let prev_id: String = dep.from_service.clone();
            if let Some(&pos) = seen_pos.get(&prev_id) {
                let mut cycle: Vec<String> = path[pos..].iter().rev().cloned().collect();
                // Close the cycle by appending the repeated node at the end.
                cycle.push(prev_id.clone());
                return Err(IntegrationError::CompositionCycleDetected { cycle });
            }
            seen_pos.insert(prev_id.clone(), path.len());
            path.push(prev_id.clone());
            node = prev_id;
        }

        // Fallback: should not be reachable, but return a descriptive error.
        let remaining_ids: Vec<String> = unsorted.iter().map(|s| (*s).to_string()).collect();
        return Err(IntegrationError::CompositionCycleDetected {
            cycle: remaining_ids,
        });
    }

    Ok(sorted)
}

/// Registry that validates and stores `ServiceComposition` declarations.
///
/// Validates that every dependency target exists in the composition and
/// that the dependency graph has no cycles (via topological sort).
pub struct CompositionEngine {
    compositions: RwLock<HashMap<ComposedSystemId, ServiceComposition>>,
}

impl CompositionEngine {
    /// Creates an empty composition engine.
    #[must_use]
    pub fn new() -> Self {
        Self {
            compositions: RwLock::new(HashMap::new()),
        }
    }

    /// Registers a composition after validation.
    ///
    /// # Errors
    ///
    /// * `ComposedServiceMissing` — a dependency references a service not in the composition.
    /// * `CompositionCycleDetected` — the dependency graph contains a directed cycle.
    #[allow(clippy::unused_async)]
    pub async fn register(
        &self,
        mut composition: ServiceComposition,
    ) -> Result<(), IntegrationError> {
        // Validate dependency targets exist.
        let svc_ids: HashSet<&str> = composition
            .services
            .iter()
            .map(|s| s.service_id.as_str())
            .collect();

        for dep in &composition.dependencies {
            if !svc_ids.contains(dep.from_service.as_str()) {
                return Err(IntegrationError::ComposedServiceMissing {
                    service_id: dep.from_service.clone(),
                    required_by: dep.to_service.clone(),
                });
            }
            if !svc_ids.contains(dep.to_service.as_str()) {
                return Err(IntegrationError::ComposedServiceMissing {
                    service_id: dep.to_service.clone(),
                    required_by: dep.from_service.clone(),
                });
            }
        }

        // Compute topological order (also detects cycles).
        let order = compute_topological_order(&composition.services, &composition.dependencies)?;

        composition.topological_order = order;

        let mut guard = self.compositions.write().await;
        guard.insert(composition.composition_id.clone(), composition);

        Ok(())
    }

    /// Retrieves a composition by ID.
    #[allow(clippy::unused_async)]
    pub async fn get(&self, id: &ComposedSystemId) -> Option<ServiceComposition> {
        let guard = self.compositions.read().await;
        guard.get(id).cloned()
    }

    /// Lists all registered compositions.
    #[allow(clippy::unused_async)]
    pub async fn list(&self) -> Vec<ServiceComposition> {
        let guard = self.compositions.read().await;
        guard.values().cloned().collect()
    }

    /// Validates a composition without storing it.
    ///
    /// Returns the topological order of the service IDs on success.
    ///
    /// # Errors
    ///
    /// * `ComposedServiceMissing` — a dependency references a missing service.
    /// * `CompositionCycleDetected` — the dependency graph contains a directed cycle.
    #[allow(clippy::unused_async)]
    pub async fn validate(
        &self,
        composition: &ServiceComposition,
    ) -> Result<Vec<String>, IntegrationError> {
        let svc_ids: HashSet<&str> = composition
            .services
            .iter()
            .map(|s| s.service_id.as_str())
            .collect();

        for dep in &composition.dependencies {
            if !svc_ids.contains(dep.from_service.as_str()) {
                return Err(IntegrationError::ComposedServiceMissing {
                    service_id: dep.from_service.clone(),
                    required_by: dep.to_service.clone(),
                });
            }
            if !svc_ids.contains(dep.to_service.as_str()) {
                return Err(IntegrationError::ComposedServiceMissing {
                    service_id: dep.to_service.clone(),
                    required_by: dep.from_service.clone(),
                });
            }
        }

        compute_topological_order(&composition.services, &composition.dependencies)
    }
}

impl Default for CompositionEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the canonical 17-crate AIOS default composition.
///
/// Encodes the AIOS stack from `aios-action` (L0 foundation) through
/// `aios-hardware` (L8), with dependency edges that mirror actual
/// cross-crate use.
///
/// # Panics
///
/// Panics if the canonical 17-crate composition is somehow cyclic — this is
/// a programmer-error invariant (the table is constant and acyclic by
/// construction). If the table ever changes and introduces a cycle, the
/// `expect()` below will catch it at first build / test run.
#[must_use]
#[allow(
    clippy::too_many_lines,
    clippy::expect_used,
    reason = "canonical 17-crate composition table; one arm per service is the clearest form, and the topological-order call cannot fail by construction"
)]
pub fn default_aios_composition() -> ServiceComposition {
    let services = vec![
        ComposedService {
            service_id: "aios-action".into(),
            crate_name: "aios-action".into(),
            binding_endpoint: "unix:/run/aios/aios-action.sock".into(),
            depends_on: vec![],
        },
        ComposedService {
            service_id: "aios-evidence".into(),
            crate_name: "aios-evidence".into(),
            binding_endpoint: "unix:/run/aios/aios-evidence.sock".into(),
            depends_on: vec!["aios-action".into()],
        },
        ComposedService {
            service_id: "aios-policy".into(),
            crate_name: "aios-policy".into(),
            binding_endpoint: "unix:/run/aios/aios-policy.sock".into(),
            depends_on: vec!["aios-action".into(), "aios-evidence".into()],
        },
        ComposedService {
            service_id: "aios-capability-runtime".into(),
            crate_name: "aios-capability-runtime".into(),
            binding_endpoint: "unix:/run/aios/aios-capability-runtime.sock".into(),
            depends_on: vec![
                "aios-action".into(),
                "aios-evidence".into(),
                "aios-policy".into(),
            ],
        },
        ComposedService {
            service_id: "aios-fs".into(),
            crate_name: "aios-fs".into(),
            binding_endpoint: "unix:/run/aios/aios-fs.sock".into(),
            depends_on: vec![
                "aios-action".into(),
                "aios-evidence".into(),
                "aios-policy".into(),
            ],
        },
        ComposedService {
            service_id: "aios-vault".into(),
            crate_name: "aios-vault".into(),
            binding_endpoint: "unix:/run/aios/aios-vault.sock".into(),
            depends_on: vec![
                "aios-action".into(),
                "aios-evidence".into(),
                "aios-policy".into(),
            ],
        },
        ComposedService {
            service_id: "aios-verification".into(),
            crate_name: "aios-verification".into(),
            binding_endpoint: "unix:/run/aios/aios-verification.sock".into(),
            depends_on: vec![
                "aios-action".into(),
                "aios-evidence".into(),
                "aios-policy".into(),
                "aios-capability-runtime".into(),
            ],
        },
        ComposedService {
            service_id: "aios-recovery".into(),
            crate_name: "aios-recovery".into(),
            binding_endpoint: "unix:/run/aios/aios-recovery.sock".into(),
            depends_on: vec![
                "aios-action".into(),
                "aios-evidence".into(),
                "aios-policy".into(),
                "aios-capability-runtime".into(),
                "aios-fs".into(),
            ],
        },
        ComposedService {
            service_id: "aios-renderer-cli".into(),
            crate_name: "aios-renderer-cli".into(),
            binding_endpoint: "unix:/run/aios/aios-renderer-cli.sock".into(),
            depends_on: vec![
                "aios-action".into(),
                "aios-evidence".into(),
                "aios-policy".into(),
                "aios-capability-runtime".into(),
                "aios-fs".into(),
                "aios-vault".into(),
                "aios-verification".into(),
                "aios-recovery".into(),
            ],
        },
        ComposedService {
            service_id: "aios-sgr".into(),
            crate_name: "aios-sgr".into(),
            binding_endpoint: "unix:/run/aios/aios-sgr.sock".into(),
            depends_on: vec![
                "aios-action".into(),
                "aios-evidence".into(),
                "aios-capability-runtime".into(),
                "aios-recovery".into(),
            ],
        },
        ComposedService {
            service_id: "aios-cognitive".into(),
            crate_name: "aios-cognitive".into(),
            binding_endpoint: "unix:/run/aios/aios-cognitive.sock".into(),
            depends_on: vec![
                "aios-action".into(),
                "aios-evidence".into(),
                "aios-policy".into(),
                "aios-capability-runtime".into(),
                "aios-vault".into(),
                "aios-sgr".into(),
            ],
        },
        ComposedService {
            service_id: "aios-sandbox".into(),
            crate_name: "aios-sandbox".into(),
            binding_endpoint: "unix:/run/aios/aios-sandbox.sock".into(),
            depends_on: vec![
                "aios-action".into(),
                "aios-evidence".into(),
                "aios-policy".into(),
                "aios-capability-runtime".into(),
                "aios-vault".into(),
                "aios-cognitive".into(),
            ],
        },
        ComposedService {
            service_id: "aios-apps".into(),
            crate_name: "aios-apps".into(),
            binding_endpoint: "unix:/run/aios/aios-apps.sock".into(),
            depends_on: vec![
                "aios-action".into(),
                "aios-evidence".into(),
                "aios-policy".into(),
                "aios-capability-runtime".into(),
                "aios-sgr".into(),
                "aios-sandbox".into(),
            ],
        },
        ComposedService {
            service_id: "aios-renderer-kde".into(),
            crate_name: "aios-renderer-kde".into(),
            binding_endpoint: "unix:/run/aios/aios-renderer-kde.sock".into(),
            depends_on: vec![
                "aios-action".into(),
                "aios-evidence".into(),
                "aios-policy".into(),
                "aios-apps".into(),
                "aios-renderer-cli".into(),
            ],
        },
        ComposedService {
            service_id: "aios-renderer-web".into(),
            crate_name: "aios-renderer-web".into(),
            binding_endpoint: "unix:/run/aios/aios-renderer-web.sock".into(),
            depends_on: vec![
                "aios-action".into(),
                "aios-evidence".into(),
                "aios-policy".into(),
                "aios-apps".into(),
                "aios-renderer-cli".into(),
            ],
        },
        ComposedService {
            service_id: "aios-network".into(),
            crate_name: "aios-network".into(),
            binding_endpoint: "unix:/run/aios/aios-network.sock".into(),
            depends_on: vec![
                "aios-action".into(),
                "aios-evidence".into(),
                "aios-policy".into(),
                "aios-capability-runtime".into(),
                "aios-apps".into(),
                "aios-sandbox".into(),
                "aios-renderer-web".into(),
            ],
        },
        ComposedService {
            service_id: "aios-hardware".into(),
            crate_name: "aios-hardware".into(),
            binding_endpoint: "unix:/run/aios/aios-hardware.sock".into(),
            depends_on: vec![
                "aios-action".into(),
                "aios-evidence".into(),
                "aios-policy".into(),
                "aios-capability-runtime".into(),
                "aios-sandbox".into(),
                "aios-recovery".into(),
                "aios-network".into(),
            ],
        },
    ];

    // Build dependency edges from each service's depends_on list.
    let mut dependencies = Vec::new();
    for svc in &services {
        for target in &svc.depends_on {
            dependencies.push(ServiceDependency {
                from_service: svc.service_id.clone(),
                to_service: target.clone(),
                required: true,
            });
        }
    }

    let topological_order = compute_topological_order(&services, &dependencies)
        .expect("default AIOS composition must be acyclic");

    ServiceComposition {
        composition_id: ComposedSystemId("aios-default".into()),
        services,
        dependencies,
        topological_order,
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

    fn svc(id: &str) -> ComposedService {
        ComposedService {
            service_id: id.into(),
            crate_name: id.into(),
            binding_endpoint: format!("unix:/run/aios/{id}.sock"),
            depends_on: vec![],
        }
    }

    fn dep(from: &str, to: &str) -> ServiceDependency {
        ServiceDependency {
            from_service: from.into(),
            to_service: to.into(),
            required: true,
        }
    }

    // -- compute_topological_order -------------------------------------------------

    #[test]
    fn compute_topological_order_empty_returns_empty() {
        let result = compute_topological_order(&[], &[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn compute_topological_order_single_no_deps() {
        let services = vec![svc("a")];
        let result = compute_topological_order(&services, &[]).unwrap();
        assert_eq!(result, vec!["a"]);
    }

    #[test]
    fn compute_topological_order_linear_chain() {
        let services = vec![svc("a"), svc("b"), svc("c")];
        let deps = vec![dep("b", "a"), dep("c", "b")];
        let result = compute_topological_order(&services, &deps).unwrap();
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn compute_topological_order_diamond() {
        let services = vec![svc("a"), svc("b"), svc("c"), svc("d")];
        let deps = vec![dep("b", "a"), dep("c", "a"), dep("d", "b"), dep("d", "c")];
        let order = compute_topological_order(&services, &deps).unwrap();
        // a must be first; d must be last; b,c in between.
        assert_eq!(order[0], "a");
        assert_eq!(order[3], "d");
        assert!(order.contains(&"b".to_string()));
        assert!(order.contains(&"c".to_string()));
    }

    #[test]
    fn compute_topological_order_cycle_detected() {
        let services = vec![svc("a"), svc("b"), svc("c")];
        let deps = vec![dep("b", "a"), dep("a", "c"), dep("c", "b")];
        let err = compute_topological_order(&services, &deps).unwrap_err();
        assert!(matches!(
            err,
            IntegrationError::CompositionCycleDetected { .. }
        ));
    }

    #[test]
    fn compute_topological_order_self_cycle_detected() {
        let services = vec![svc("a")];
        let deps = vec![dep("a", "a")];
        let err = compute_topological_order(&services, &deps).unwrap_err();
        assert!(matches!(
            err,
            IntegrationError::CompositionCycleDetected { .. }
        ));
    }

    #[test]
    fn compute_topological_order_extraneous_deps_ignored() {
        // Dependencies referencing services not in the service list are ignored.
        let services = vec![svc("a"), svc("b")];
        let deps = vec![dep("b", "a"), dep("b", "missing")];
        let result = compute_topological_order(&services, &deps).unwrap();
        assert_eq!(result, vec!["a", "b"]);
    }

    // -- CompositionEngine register / get / list / validate -----------------------

    #[tokio::test]
    async fn register_valid_succeeds() {
        let engine = CompositionEngine::new();
        let comp = ServiceComposition {
            composition_id: ComposedSystemId("test-1".to_string()),
            services: vec![svc("a"), svc("b"), svc("c")],
            dependencies: vec![dep("b", "a"), dep("c", "b")],
            topological_order: vec![],
        };
        engine.register(comp).await.unwrap();
        let list = engine.list().await;
        assert_eq!(list.len(), 1);
        assert!(!list[0].topological_order.is_empty());
    }

    #[tokio::test]
    async fn register_missing_dep_target_returns_error() {
        let engine = CompositionEngine::new();
        let comp = ServiceComposition {
            composition_id: ComposedSystemId("test-missing".to_string()),
            services: vec![svc("a"), svc("b")],
            dependencies: vec![dep("b", "missing-svc")],
            topological_order: vec![],
        };
        let err = engine.register(comp).await.unwrap_err();
        assert!(matches!(
            err,
            IntegrationError::ComposedServiceMissing { .. }
        ));
    }

    #[tokio::test]
    async fn register_missing_from_service_returns_error() {
        let engine = CompositionEngine::new();
        let comp = ServiceComposition {
            composition_id: ComposedSystemId("test-missing-from".to_string()),
            services: vec![svc("a"), svc("b")],
            dependencies: vec![dep("missing-svc", "a")],
            topological_order: vec![],
        };
        let err = engine.register(comp).await.unwrap_err();
        assert!(matches!(
            err,
            IntegrationError::ComposedServiceMissing { .. }
        ));
    }

    #[tokio::test]
    async fn register_cycle_returns_composition_cycle_detected() {
        let engine = CompositionEngine::new();
        let comp = ServiceComposition {
            composition_id: ComposedSystemId("test-cycle".to_string()),
            services: vec![svc("a"), svc("b"), svc("c")],
            dependencies: vec![dep("b", "a"), dep("a", "c"), dep("c", "b")],
            topological_order: vec![],
        };
        let err = engine.register(comp).await.unwrap_err();
        assert!(matches!(
            err,
            IntegrationError::CompositionCycleDetected { .. }
        ));
    }

    #[tokio::test]
    async fn get_after_register_returns_some_with_topological_order() {
        let engine = CompositionEngine::new();
        let id = ComposedSystemId("test-get".to_string());
        let comp = ServiceComposition {
            composition_id: id.clone(),
            services: vec![svc("a"), svc("b")],
            dependencies: vec![dep("b", "a")],
            topological_order: vec![],
        };
        engine.register(comp).await.unwrap();
        let got = engine.get(&id).await;
        assert!(got.is_some());
        let got = got.unwrap();
        assert_eq!(got.topological_order, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let engine = CompositionEngine::new();
        let id = ComposedSystemId("no-such".to_string());
        let got = engine.get(&id).await;
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn list_after_three_registrations_returns_three() {
        let engine = CompositionEngine::new();
        for i in 0..3 {
            let comp = ServiceComposition {
                composition_id: ComposedSystemId(format!("list-{i}")),
                services: vec![svc("a"), svc("b")],
                dependencies: vec![dep("b", "a")],
                topological_order: vec![],
            };
            engine.register(comp).await.unwrap();
        }
        let list = engine.list().await;
        assert_eq!(list.len(), 3);
    }

    #[tokio::test]
    async fn validate_does_not_store() {
        let engine = CompositionEngine::new();
        let comp = ServiceComposition {
            composition_id: ComposedSystemId("validate-only".to_string()),
            services: vec![svc("a"), svc("b"), svc("c")],
            dependencies: vec![dep("b", "a"), dep("c", "b")],
            topological_order: vec![],
        };
        let order = engine.validate(&comp).await.unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
        // Composition was not stored.
        let list = engine.list().await;
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn validate_cycle_returns_error_but_does_not_store() {
        let engine = CompositionEngine::new();
        let comp = ServiceComposition {
            composition_id: ComposedSystemId("validate-cycle".to_string()),
            services: vec![svc("a"), svc("b")],
            dependencies: vec![dep("a", "b"), dep("b", "a")],
            topological_order: vec![],
        };
        let err = engine.validate(&comp).await.unwrap_err();
        assert!(matches!(
            err,
            IntegrationError::CompositionCycleDetected { .. }
        ));
        let list = engine.list().await;
        assert!(list.is_empty());
    }

    // -- default_aios_composition --------------------------------------------------

    #[test]
    fn default_aios_composition_has_17_services() {
        let comp = default_aios_composition();
        assert_eq!(comp.services.len(), 17);
    }

    #[test]
    fn default_aios_composition_has_no_cycles() {
        let comp = default_aios_composition();
        // Re-run topological sort to confirm acyclicity.
        let order = compute_topological_order(&comp.services, &comp.dependencies).unwrap();
        assert_eq!(order.len(), 17);
    }

    #[test]
    fn default_aios_composition_first_is_aios_action() {
        let comp = default_aios_composition();
        assert_eq!(comp.topological_order[0], "aios-action");
    }

    #[test]
    fn default_aios_composition_action_before_evidence() {
        let comp = default_aios_composition();
        let action_pos = comp
            .topological_order
            .iter()
            .position(|s| s == "aios-action")
            .unwrap();
        let evidence_pos = comp
            .topological_order
            .iter()
            .position(|s| s == "aios-evidence")
            .unwrap();
        assert!(action_pos < evidence_pos);
    }

    #[test]
    fn default_aios_composition_evidence_before_policy() {
        let comp = default_aios_composition();
        let evidence_pos = comp
            .topological_order
            .iter()
            .position(|s| s == "aios-evidence")
            .unwrap();
        let policy_pos = comp
            .topological_order
            .iter()
            .position(|s| s == "aios-policy")
            .unwrap();
        assert!(evidence_pos < policy_pos);
    }

    #[test]
    fn default_aios_composition_renderer_cli_before_renderer_kde() {
        let comp = default_aios_composition();
        let cli_pos = comp
            .topological_order
            .iter()
            .position(|s| s == "aios-renderer-cli")
            .unwrap();
        let kde_pos = comp
            .topological_order
            .iter()
            .position(|s| s == "aios-renderer-kde")
            .unwrap();
        assert!(cli_pos < kde_pos);
    }

    #[test]
    fn default_aios_composition_last_is_aios_hardware() {
        let comp = default_aios_composition();
        assert_eq!(comp.topological_order[16], "aios-hardware");
    }

    #[test]
    fn default_aios_composition_sandbox_before_apps() {
        let comp = default_aios_composition();
        let sandbox_pos = comp
            .topological_order
            .iter()
            .position(|s| s == "aios-sandbox")
            .unwrap();
        let apps_pos = comp
            .topological_order
            .iter()
            .position(|s| s == "aios-apps")
            .unwrap();
        assert!(sandbox_pos < apps_pos);
    }

    // -- Concurrent registration ---------------------------------------------------

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_register_three_distinct_no_panic() {
        use std::sync::Arc;
        let engine = Arc::new(CompositionEngine::new());
        let mut handles = Vec::new();
        for i in 0..3 {
            let e = Arc::clone(&engine);
            handles.push(tokio::spawn(async move {
                let comp = ServiceComposition {
                    composition_id: ComposedSystemId(format!("concurrent-{i}")),
                    services: vec![svc("a"), svc("b")],
                    dependencies: vec![dep("b", "a")],
                    topological_order: vec![],
                };
                e.register(comp).await
            }));
        }
        for h in handles {
            h.await.unwrap().unwrap();
        }
        let list = engine.list().await;
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn default_aios_composition_topological_order_length_equals_services() {
        let comp = default_aios_composition();
        assert_eq!(comp.topological_order.len(), comp.services.len());
    }

    #[test]
    fn default_aios_composition_all_services_in_topological_order() {
        let comp = default_aios_composition();
        let mut order_sorted = comp.topological_order.clone();
        order_sorted.sort();
        let mut svc_ids: Vec<String> = comp.services.iter().map(|s| s.service_id.clone()).collect();
        svc_ids.sort();
        assert_eq!(order_sorted, svc_ids);
    }

    #[test]
    fn default_aios_composition_every_dependency_respected_in_order() {
        let comp = default_aios_composition();
        let order = &comp.topological_order;
        let pos: HashMap<&str, usize> = order
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        for dep in &comp.dependencies {
            let from_pos = pos[dep.from_service.as_str()];
            let to_pos = pos[dep.to_service.as_str()];
            assert!(to_pos < from_pos, "{dep:?} violates topological order");
        }
    }

    #[test]
    fn default_aios_composition_aios_hardware_last() {
        let comp = default_aios_composition();
        let hardware_pos = comp
            .topological_order
            .iter()
            .position(|s| s == "aios-hardware")
            .unwrap();
        assert_eq!(hardware_pos, comp.topological_order.len() - 1);
    }

    #[tokio::test]
    async fn validate_detects_missing_to_service() {
        let engine = CompositionEngine::new();
        let comp = ServiceComposition {
            composition_id: ComposedSystemId("val-missing".to_string()),
            services: vec![svc("a")],
            dependencies: vec![dep("a", "missing")],
            topological_order: vec![],
        };
        let err = engine.validate(&comp).await.unwrap_err();
        assert!(matches!(
            err,
            IntegrationError::ComposedServiceMissing { .. }
        ));
    }
}
