//! T-087 `GraphEvaluator` tests for S15.2 dependency evaluation.

#![allow(
    clippy::expect_used,
    clippy::many_single_char_names,
    clippy::panic,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::unwrap_used,
    reason = "Integration-test failures should point at the failing contract"
)]

use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use aios_sgr::{
    DependencyEdge, DependencyKind, DesiredState, GpuBudget, GraphEvaluator, GraphState,
    HealthCheckKind, HealthCheckSpec, ResourceBudget, RestartBudget, RestartPolicy,
    RollbackPointer, RollbackTrigger, ServiceGraph, ServiceUnit, SgrError, UnitId, UnitKind,
    UnitManifest, UnitState, VerificationIntentRef,
};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use tokio::sync::RwLock;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn fixed_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 25, 10, 0, 0)
        .single()
        .expect("valid datetime")
}

fn unit_id(name: &str) -> UnitId {
    UnitId::from_parts("aiosroot", name, None).expect("valid unit id")
}

fn manifest(name: &str, desired_state: DesiredState) -> UnitManifest {
    UnitManifest {
        schema_version: "aios.unit.v1alpha1".to_owned(),
        unit_id: unit_id(name),
        unit_kind: UnitKind::Service,
        display_name: format!("AIOS {name}"),
        description: "Test service unit.".to_owned(),
        issued_at: fixed_time(),
        publisher_id: "pub_01HXY9ROOTAIOS01KEY".to_owned(),
        publisher_root_id: "pubcat_aiosroot".to_owned(),
        publisher_signature: Vec::new(),
        canonical_hash: String::new(),
        dependencies: Vec::new(),
        sandbox_profile_ref: "prof_aios_runtime_floor_001".to_owned(),
        verification_intent: vec![VerificationIntentRef {
            type_: "service.active".to_owned(),
            args: serde_json::json!({ "service": name }),
        }],
        rollback_pointer: RollbackPointer {
            aiosfs_pointer_id: format!("ptr_{name}_release"),
            expected_current_version_id: "ver_01HXY8K2".to_owned(),
            trigger: RollbackTrigger::OnHealthFailure,
        },
        resource_budget: ResourceBudget {
            memory_bytes_max: 268_435_456,
            cpu_quota_cores: 1.0,
            disk_bytes_max: 1_073_741_824,
            file_descriptors_max: 1_024,
            process_count_max: 64,
            queue_depth_max: 256,
            gpu: Some(GpuBudget {
                requires_compute: false,
                vram_bytes_max: 0,
            }),
        },
        restart_policy: RestartPolicy::OnFailure,
        restart_budget: RestartBudget {
            max_attempts: 5,
            reset_window_seconds: 300,
            backoff_initial_seconds: 2,
            backoff_max_seconds: 60,
        },
        health_check: HealthCheckSpec {
            kind: HealthCheckKind::HttpOk,
            probe_interval_seconds: 10,
            probe_timeout_seconds: 3,
            startup_grace_seconds: 30,
            unhealthy_threshold: 3,
            healthy_threshold: 2,
            args: serde_json::json!({ "path": "/healthz", "port": 7421 }),
        },
        startup_deadline_seconds: 60,
        stop_deadline_seconds: 30,
        adapter_target: serde_json::json!({ "systemd_unit": format!("{name}.service") }),
        labels: Some(serde_json::json!({ "layer": "L3", "criticality": "standard" })),
        correlation_id: None,
        desired_state,
        provides: vec![format!("service.{name}")],
        adapter_id: Some("adapter:aiosroot:systemd:1.0.0".to_owned()),
    }
}

fn service_unit(name: &str, state: UnitState, desired_state: DesiredState) -> ServiceUnit {
    ServiceUnit {
        unit_id: unit_id(name),
        manifest: manifest(name, desired_state),
        state,
        last_transition_at: fixed_time(),
        evidence_chain: Vec::new(),
    }
}

#[derive(Debug, Default)]
struct TestServiceGraph {
    units: RwLock<HashMap<UnitId, ServiceUnit>>,
    dependencies: RwLock<HashMap<UnitId, Vec<DependencyEdge>>>,
}

impl TestServiceGraph {
    async fn add_unit(&self, name: &str, state: UnitState, desired_state: DesiredState) -> UnitId {
        let unit = service_unit(name, state, desired_state);
        let id = unit.unit_id.clone();
        self.units.write().await.insert(id.clone(), unit);
        id
    }

    async fn depend(
        &self,
        unit: &UnitId,
        prerequisite: &UnitId,
        kind: DependencyKind,
    ) -> Result<DependencyEdge, SgrError> {
        self.declare_dependency(unit, prerequisite, kind).await
    }
}

#[async_trait]
impl ServiceGraph for TestServiceGraph {
    async fn register_unit(&self, manifest: UnitManifest) -> Result<ServiceUnit, SgrError> {
        let unit_id = manifest.unit_id.clone();
        let unit = ServiceUnit {
            unit_id: unit_id.clone(),
            manifest,
            state: UnitState::Queued,
            last_transition_at: fixed_time(),
            evidence_chain: Vec::new(),
        };
        self.units.write().await.insert(unit_id, unit.clone());
        Ok(unit)
    }

    async fn get_unit(&self, unit_id: &UnitId) -> Result<ServiceUnit, SgrError> {
        self.units
            .read()
            .await
            .get(unit_id)
            .cloned()
            .ok_or_else(|| SgrError::UnitNotFound(unit_id.clone()))
    }

    async fn list_units(&self) -> Result<Vec<ServiceUnit>, SgrError> {
        Ok(self.units.read().await.values().cloned().collect())
    }

    async fn declare_dependency(
        &self,
        from: &UnitId,
        to: &UnitId,
        kind: DependencyKind,
    ) -> Result<DependencyEdge, SgrError> {
        let units = self.units.read().await;
        if !units.contains_key(from) {
            return Err(SgrError::UnitNotFound(from.clone()));
        }
        if !units.contains_key(to) {
            return Err(SgrError::DependencyTargetNotRegistered(to.clone()));
        }
        drop(units);

        let edge = DependencyEdge {
            from_unit_id: from.clone(),
            to_unit_id: to.clone(),
            kind,
        };
        self.dependencies
            .write()
            .await
            .entry(from.clone())
            .or_default()
            .push(edge.clone());
        Ok(edge)
    }

    async fn list_dependencies(&self, unit_id: &UnitId) -> Result<Vec<DependencyEdge>, SgrError> {
        if !self.units.read().await.contains_key(unit_id) {
            return Err(SgrError::UnitNotFound(unit_id.clone()));
        }

        Ok(self
            .dependencies
            .read()
            .await
            .get(unit_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn graph_state(&self) -> Result<GraphState, SgrError> {
        Ok(GraphState::Resolving)
    }

    async fn set_unit_state(
        &self,
        unit_id: &UnitId,
        new_state: UnitState,
    ) -> Result<ServiceUnit, SgrError> {
        let mut units = self.units.write().await;
        let unit = units
            .get_mut(unit_id)
            .ok_or_else(|| SgrError::UnitNotFound(unit_id.clone()))?;
        unit.state = new_state;
        unit.last_transition_at = fixed_time();
        let updated = unit.clone();
        drop(units);
        Ok(updated)
    }
}

fn evaluator(graph: &Arc<TestServiceGraph>) -> GraphEvaluator {
    let graph: Arc<dyn ServiceGraph> = graph.clone();
    GraphEvaluator::new(graph)
}

fn id_strings(ids: &[UnitId]) -> Vec<String> {
    ids.iter().map(ToString::to_string).collect()
}

fn sorted_id_strings(ids: &[UnitId]) -> Vec<String> {
    let mut values = id_strings(ids);
    values.sort();
    values
}

#[tokio::test]
async fn topological_sort_empty_graph_returns_empty_vec() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());

    assert!(evaluator(&graph).topological_sort().await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn topological_sort_linear_chain_returns_dependency_order() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());
    let a = graph
        .add_unit("a", UnitState::Running, DesiredState::Running)
        .await;
    let b = graph
        .add_unit("b", UnitState::Running, DesiredState::Running)
        .await;
    let c = graph
        .add_unit("c", UnitState::Running, DesiredState::Running)
        .await;
    graph
        .depend(&b, &a, DependencyKind::RequiresRunning)
        .await?;
    graph.depend(&c, &b, DependencyKind::OrdersAfter).await?;

    assert_eq!(
        id_strings(&evaluator(&graph).topological_sort().await?),
        vec!["unit:aiosroot:a", "unit:aiosroot:b", "unit:aiosroot:c"]
    );
    Ok(())
}

#[tokio::test]
async fn topological_sort_diamond_places_root_first_and_leaf_last() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());
    let a = graph
        .add_unit("a", UnitState::Running, DesiredState::Running)
        .await;
    let b = graph
        .add_unit("b", UnitState::Running, DesiredState::Running)
        .await;
    let c = graph
        .add_unit("c", UnitState::Running, DesiredState::Running)
        .await;
    let d = graph
        .add_unit("d", UnitState::Running, DesiredState::Running)
        .await;
    graph
        .depend(&b, &a, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&c, &a, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&d, &b, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&d, &c, DependencyKind::RequiresRunning)
        .await?;

    let order = evaluator(&graph).topological_sort().await?;

    assert_eq!(order.first(), Some(&a));
    assert_eq!(order.last(), Some(&d));
    assert_eq!(
        sorted_id_strings(&order[1..3]),
        vec!["unit:aiosroot:b", "unit:aiosroot:c"]
    );
    Ok(())
}

#[tokio::test]
async fn topological_sort_detects_direct_cycle() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());
    let a = graph
        .add_unit("a", UnitState::Running, DesiredState::Running)
        .await;
    let b = graph
        .add_unit("b", UnitState::Running, DesiredState::Running)
        .await;
    graph
        .depend(&a, &b, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&b, &a, DependencyKind::RequiresRunning)
        .await?;

    let err = evaluator(&graph)
        .topological_sort()
        .await
        .expect_err("cycle must reject topological sort");

    assert!(matches!(
        err,
        SgrError::DependencyCycleDetected(ref nodes)
            if sorted_id_strings(nodes) == vec!["unit:aiosroot:a", "unit:aiosroot:b"]
    ));
    Ok(())
}

#[tokio::test]
async fn detect_cycles_direct_two_cycle_returns_one_scc() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());
    let a = graph
        .add_unit("a", UnitState::Running, DesiredState::Running)
        .await;
    let b = graph
        .add_unit("b", UnitState::Running, DesiredState::Running)
        .await;
    graph
        .depend(&a, &b, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&b, &a, DependencyKind::RequiresRunning)
        .await?;

    let cycles = evaluator(&graph).detect_cycles().await?;

    assert_eq!(cycles.len(), 1);
    assert_eq!(
        sorted_id_strings(&cycles[0]),
        vec!["unit:aiosroot:a", "unit:aiosroot:b"]
    );
    Ok(())
}

#[tokio::test]
async fn detect_cycles_three_cycle_returns_one_scc_of_three() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());
    let a = graph
        .add_unit("a", UnitState::Running, DesiredState::Running)
        .await;
    let b = graph
        .add_unit("b", UnitState::Running, DesiredState::Running)
        .await;
    let c = graph
        .add_unit("c", UnitState::Running, DesiredState::Running)
        .await;
    graph
        .depend(&a, &b, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&b, &c, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&c, &a, DependencyKind::RequiresRunning)
        .await?;

    let cycles = evaluator(&graph).detect_cycles().await?;

    assert_eq!(cycles.len(), 1);
    assert_eq!(
        sorted_id_strings(&cycles[0]),
        vec!["unit:aiosroot:a", "unit:aiosroot:b", "unit:aiosroot:c"]
    );
    Ok(())
}

#[tokio::test]
async fn detect_cycles_linear_graph_returns_empty_vec() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());
    let a = graph
        .add_unit("a", UnitState::Running, DesiredState::Running)
        .await;
    let b = graph
        .add_unit("b", UnitState::Running, DesiredState::Running)
        .await;
    let c = graph
        .add_unit("c", UnitState::Running, DesiredState::Running)
        .await;
    graph
        .depend(&b, &a, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&c, &b, DependencyKind::RequiresRunning)
        .await?;

    assert!(evaluator(&graph).detect_cycles().await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn detect_cycles_two_separate_cycles_returns_two_sccs() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());
    let a = graph
        .add_unit("a", UnitState::Running, DesiredState::Running)
        .await;
    let b = graph
        .add_unit("b", UnitState::Running, DesiredState::Running)
        .await;
    let c = graph
        .add_unit("c", UnitState::Running, DesiredState::Running)
        .await;
    let d = graph
        .add_unit("d", UnitState::Running, DesiredState::Running)
        .await;
    let e = graph
        .add_unit("e", UnitState::Running, DesiredState::Running)
        .await;
    graph
        .depend(&a, &b, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&b, &a, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&c, &d, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&d, &e, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&e, &c, DependencyKind::RequiresRunning)
        .await?;

    let mut cycles = evaluator(&graph)
        .detect_cycles()
        .await?
        .into_iter()
        .map(|cycle| sorted_id_strings(&cycle))
        .collect::<Vec<_>>();
    cycles.sort();

    assert_eq!(
        cycles,
        vec![
            vec!["unit:aiosroot:a", "unit:aiosroot:b"],
            vec!["unit:aiosroot:c", "unit:aiosroot:d", "unit:aiosroot:e"],
        ]
    );
    Ok(())
}

#[tokio::test]
async fn detect_cycles_self_loop_returns_single_node_cycle() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());
    let a = graph
        .add_unit("a", UnitState::Running, DesiredState::Running)
        .await;
    graph
        .depend(&a, &a, DependencyKind::RequiresRunning)
        .await?;

    let cycles = evaluator(&graph).detect_cycles().await?;

    assert_eq!(cycles.len(), 1);
    assert_eq!(id_strings(&cycles[0]), vec!["unit:aiosroot:a"]);
    Ok(())
}

#[tokio::test]
async fn identify_parallel_batches_diamond_groups_concurrent_units() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());
    let a = graph
        .add_unit("a", UnitState::Running, DesiredState::Running)
        .await;
    let b = graph
        .add_unit("b", UnitState::Running, DesiredState::Running)
        .await;
    let c = graph
        .add_unit("c", UnitState::Running, DesiredState::Running)
        .await;
    let d = graph
        .add_unit("d", UnitState::Running, DesiredState::Running)
        .await;
    graph
        .depend(&b, &a, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&c, &a, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&d, &b, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&d, &c, DependencyKind::RequiresRunning)
        .await?;

    let batches = evaluator(&graph).identify_parallel_batches().await?;

    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0], vec![a]);
    assert_eq!(
        sorted_id_strings(&batches[1]),
        vec!["unit:aiosroot:b", "unit:aiosroot:c"]
    );
    assert_eq!(batches[2], vec![d]);
    Ok(())
}

#[tokio::test]
async fn identify_parallel_batches_linear_chain_returns_singleton_batches() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());
    let a = graph
        .add_unit("a", UnitState::Running, DesiredState::Running)
        .await;
    let b = graph
        .add_unit("b", UnitState::Running, DesiredState::Running)
        .await;
    let c = graph
        .add_unit("c", UnitState::Running, DesiredState::Running)
        .await;
    let d = graph
        .add_unit("d", UnitState::Running, DesiredState::Running)
        .await;
    graph
        .depend(&b, &a, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&c, &b, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&d, &c, DependencyKind::RequiresRunning)
        .await?;

    let batches = evaluator(&graph).identify_parallel_batches().await?;

    assert_eq!(batches, vec![vec![a], vec![b], vec![c], vec![d]]);
    Ok(())
}

#[tokio::test]
async fn identify_parallel_batches_empty_graph_returns_empty_vec() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());

    assert!(evaluator(&graph)
        .identify_parallel_batches()
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn evaluate_readiness_with_all_requires_deps_running_returns_true() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());
    let app = graph
        .add_unit("app", UnitState::Queued, DesiredState::Running)
        .await;
    let db = graph
        .add_unit("db", UnitState::Running, DesiredState::Running)
        .await;
    let cache = graph
        .add_unit("cache", UnitState::Running, DesiredState::Running)
        .await;
    graph
        .depend(&app, &db, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&app, &cache, DependencyKind::RequiresHealthy)
        .await?;

    assert!(evaluator(&graph).evaluate_readiness(&app).await?);
    Ok(())
}

#[tokio::test]
async fn evaluate_readiness_with_one_requires_dep_not_running_returns_false() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());
    let app = graph
        .add_unit("app", UnitState::Queued, DesiredState::Running)
        .await;
    let db = graph
        .add_unit("db", UnitState::Running, DesiredState::Running)
        .await;
    let cache = graph
        .add_unit("cache", UnitState::Queued, DesiredState::Running)
        .await;
    graph
        .depend(&app, &db, DependencyKind::RequiresRunning)
        .await?;
    graph
        .depend(&app, &cache, DependencyKind::RequiresRunning)
        .await?;

    assert!(!evaluator(&graph).evaluate_readiness(&app).await?);
    Ok(())
}

#[tokio::test]
async fn evaluate_readiness_with_no_dependencies_returns_true() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());
    let unit = graph
        .add_unit("standalone", UnitState::Queued, DesiredState::Running)
        .await;

    assert!(evaluator(&graph).evaluate_readiness(&unit).await?);
    Ok(())
}

#[tokio::test]
async fn is_converged_empty_graph_returns_true() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());

    assert!(evaluator(&graph).is_converged().await?);
    Ok(())
}

#[tokio::test]
async fn is_converged_when_desired_running_but_stopped_returns_false() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());
    graph
        .add_unit("app", UnitState::Stopped, DesiredState::Running)
        .await;

    assert!(!evaluator(&graph).is_converged().await?);
    Ok(())
}

#[tokio::test]
async fn convergence_state_covers_empty_converging_converged_degraded_failed() -> TestResult {
    let empty = Arc::new(TestServiceGraph::default());
    assert_eq!(
        evaluator(&empty).convergence_state().await?,
        GraphState::Empty
    );

    let converging = Arc::new(TestServiceGraph::default());
    converging
        .add_unit("queued", UnitState::Queued, DesiredState::Running)
        .await;
    assert_eq!(
        evaluator(&converging).convergence_state().await?,
        GraphState::Converging
    );

    let converged = Arc::new(TestServiceGraph::default());
    converged
        .add_unit("running", UnitState::Running, DesiredState::Running)
        .await;
    assert_eq!(
        evaluator(&converged).convergence_state().await?,
        GraphState::Converged
    );

    let degraded = Arc::new(TestServiceGraph::default());
    degraded
        .add_unit("running", UnitState::Running, DesiredState::Running)
        .await;
    degraded
        .add_unit("degraded", UnitState::Degraded, DesiredState::Running)
        .await;
    assert_eq!(
        evaluator(&degraded).convergence_state().await?,
        GraphState::Degraded
    );

    let failed = Arc::new(TestServiceGraph::default());
    failed
        .add_unit("failed", UnitState::Failed, DesiredState::Running)
        .await;
    assert_eq!(
        evaluator(&failed).convergence_state().await?,
        GraphState::Failed
    );
    Ok(())
}

#[tokio::test]
async fn topological_sort_hundred_unit_linear_graph_completes_under_100ms() -> TestResult {
    let graph = Arc::new(TestServiceGraph::default());
    let mut previous = graph
        .add_unit("node_000", UnitState::Running, DesiredState::Running)
        .await;
    for idx in 1..100 {
        let current = graph
            .add_unit(
                &format!("node_{idx:03}"),
                UnitState::Running,
                DesiredState::Running,
            )
            .await;
        graph
            .depend(&current, &previous, DependencyKind::RequiresRunning)
            .await?;
        previous = current;
    }

    let started = Instant::now();
    let order = evaluator(&graph).topological_sort().await?;
    let elapsed = started.elapsed();

    assert_eq!(order.len(), 100);
    assert!(
        elapsed < Duration::from_millis(100),
        "topological_sort took {elapsed:?}"
    );
    Ok(())
}
