//! T-086 `UnitFsmDriver` tests for the S15.1 unit FSM used by S15.2.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::unwrap_used,
    reason = "Integration-test failures should point at the failing contract"
)]

use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use aios_sgr::{
    is_legal_transition, DependencyEdge, DependencyKind, DesiredState, GpuBudget, GraphState,
    HealthCheckKind, HealthCheckSpec, InMemoryServiceGraph, ResourceBudget, RestartBudget,
    RestartPolicy, RollbackPointer, RollbackTrigger, ServiceGraph, SgrError, UnitDependency,
    UnitFsmDriver, UnitId, UnitKind, UnitManifest, UnitState, VerificationIntentRef, TRANSITIONS,
};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;
use tokio::sync::RwLock;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const AUTHORITY: &str = "pubcat_aiosroot";

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn fixed_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 25, 10, 0, 0)
        .single()
        .expect("valid datetime")
}

fn unit_id(name: &str) -> UnitId {
    UnitId::from_parts("aiosroot", name, None).expect("valid unit id")
}

fn unsigned_manifest(name: &str) -> UnitManifest {
    UnitManifest {
        schema_version: "aios.unit.v1alpha1".to_owned(),
        unit_id: unit_id(name),
        unit_kind: UnitKind::Service,
        display_name: format!("AIOS {name}"),
        description: "Test service unit.".to_owned(),
        issued_at: fixed_time(),
        publisher_id: "pub_01HXY9ROOTAIOS01KEY".to_owned(),
        publisher_root_id: AUTHORITY.to_owned(),
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
        desired_state: DesiredState::Running,
        provides: vec![format!("service.{name}")],
        adapter_id: Some("adapter:aiosroot:systemd:1.0.0".to_owned()),
    }
}

#[derive(Serialize)]
struct SignedUnitManifestBody<'a> {
    schema_version: &'a str,
    unit_id: &'a UnitId,
    unit_kind: &'a UnitKind,
    display_name: &'a str,
    description: &'a str,
    issued_at: &'a DateTime<Utc>,
    publisher_id: &'a str,
    publisher_root_id: &'a str,
    dependencies: &'a [UnitDependency],
    sandbox_profile_ref: &'a str,
    verification_intent: &'a [VerificationIntentRef],
    rollback_pointer: &'a RollbackPointer,
    resource_budget: &'a ResourceBudget,
    restart_policy: &'a RestartPolicy,
    restart_budget: &'a RestartBudget,
    health_check: &'a HealthCheckSpec,
    startup_deadline_seconds: u32,
    stop_deadline_seconds: u32,
    adapter_target: &'a serde_json::Value,
    labels: &'a Option<serde_json::Value>,
    correlation_id: &'a Option<String>,
    desired_state: &'a DesiredState,
    provides: &'a [String],
    adapter_id: &'a Option<String>,
}

impl<'a> From<&'a UnitManifest> for SignedUnitManifestBody<'a> {
    fn from(manifest: &'a UnitManifest) -> Self {
        Self {
            schema_version: &manifest.schema_version,
            unit_id: &manifest.unit_id,
            unit_kind: &manifest.unit_kind,
            display_name: &manifest.display_name,
            description: &manifest.description,
            issued_at: &manifest.issued_at,
            publisher_id: &manifest.publisher_id,
            publisher_root_id: &manifest.publisher_root_id,
            dependencies: &manifest.dependencies,
            sandbox_profile_ref: &manifest.sandbox_profile_ref,
            verification_intent: &manifest.verification_intent,
            rollback_pointer: &manifest.rollback_pointer,
            resource_budget: &manifest.resource_budget,
            restart_policy: &manifest.restart_policy,
            restart_budget: &manifest.restart_budget,
            health_check: &manifest.health_check,
            startup_deadline_seconds: manifest.startup_deadline_seconds,
            stop_deadline_seconds: manifest.stop_deadline_seconds,
            adapter_target: &manifest.adapter_target,
            labels: &manifest.labels,
            correlation_id: &manifest.correlation_id,
            desired_state: &manifest.desired_state,
            provides: &manifest.provides,
            adapter_id: &manifest.adapter_id,
        }
    }
}

fn canonical_manifest_digest(manifest: &UnitManifest) -> TestResult<blake3::Hash> {
    let body = SignedUnitManifestBody::from(manifest);
    Ok(blake3::hash(&serde_json::to_vec(&body)?))
}

fn sign_manifest(manifest: &mut UnitManifest, sk: &SigningKey) -> TestResult {
    let digest = canonical_manifest_digest(manifest)?;
    let digest_hex = digest.to_hex().to_string();
    digest_hex[..32].clone_into(&mut manifest.canonical_hash);
    manifest.publisher_signature = sk.sign(digest.as_bytes()).to_bytes().to_vec();
    Ok(())
}

fn signed_manifest(name: &str, sk: &SigningKey) -> TestResult<UnitManifest> {
    let mut manifest = unsigned_manifest(name);
    sign_manifest(&mut manifest, sk)?;
    Ok(manifest)
}

fn trusted_graph(sk: &SigningKey) -> InMemoryServiceGraph {
    InMemoryServiceGraph::with_trusted_authority(AUTHORITY, sk.verifying_key())
}

async fn register_named(
    graph: &InMemoryServiceGraph,
    sk: &SigningKey,
    name: &str,
) -> TestResult<UnitId> {
    let manifest = signed_manifest(name, sk)?;
    let id = manifest.unit_id.clone();
    graph.register_unit(manifest).await?;
    Ok(id)
}

async fn running_unit(
    graph: &InMemoryServiceGraph,
    sk: &SigningKey,
    name: &str,
) -> TestResult<UnitId> {
    let id = register_named(graph, sk, name).await?;
    graph.set_unit_state(&id, UnitState::Starting).await?;
    graph.set_unit_state(&id, UnitState::Running).await?;
    Ok(id)
}

fn assert_invalid_transition(err: &SgrError, from: UnitState, to: UnitState) {
    assert!(matches!(
        err,
        SgrError::InvalidStateTransition { from: actual_from, to: actual_to }
            if *actual_from == from && *actual_to == to
    ));
}

fn sample_unit(name: &str, state: UnitState) -> aios_sgr::ServiceUnit {
    aios_sgr::ServiceUnit {
        unit_id: unit_id(name),
        manifest: unsigned_manifest(name),
        state,
        last_transition_at: fixed_time(),
        evidence_chain: Vec::new(),
    }
}

#[derive(Debug)]
struct RecordingServiceGraph {
    unit: RwLock<aios_sgr::ServiceUnit>,
    transitions: RwLock<Vec<(UnitState, UnitState)>>,
}

impl RecordingServiceGraph {
    fn new(unit: aios_sgr::ServiceUnit) -> Self {
        Self {
            unit: RwLock::new(unit),
            transitions: RwLock::new(Vec::new()),
        }
    }

    async fn transitions(&self) -> Vec<(UnitState, UnitState)> {
        self.transitions.read().await.clone()
    }
}

#[async_trait]
impl ServiceGraph for RecordingServiceGraph {
    async fn register_unit(
        &self,
        manifest: UnitManifest,
    ) -> Result<aios_sgr::ServiceUnit, SgrError> {
        let mut unit = self.unit.write().await;
        unit.unit_id = manifest.unit_id.clone();
        unit.manifest = manifest;
        unit.state = UnitState::Queued;
        Ok(unit.clone())
    }

    async fn get_unit(&self, unit_id: &UnitId) -> Result<aios_sgr::ServiceUnit, SgrError> {
        let unit = self.unit.read().await;
        if &unit.unit_id == unit_id {
            Ok(unit.clone())
        } else {
            Err(SgrError::UnitNotFound(unit_id.clone()))
        }
    }

    async fn list_units(&self) -> Result<Vec<aios_sgr::ServiceUnit>, SgrError> {
        Ok(vec![self.unit.read().await.clone()])
    }

    async fn declare_dependency(
        &self,
        from: &UnitId,
        to: &UnitId,
        kind: DependencyKind,
    ) -> Result<DependencyEdge, SgrError> {
        Ok(DependencyEdge {
            from_unit_id: from.clone(),
            to_unit_id: to.clone(),
            kind,
        })
    }

    async fn list_dependencies(&self, unit_id: &UnitId) -> Result<Vec<DependencyEdge>, SgrError> {
        let unit = self.unit.read().await;
        if &unit.unit_id == unit_id {
            Ok(Vec::new())
        } else {
            Err(SgrError::UnitNotFound(unit_id.clone()))
        }
    }

    async fn graph_state(&self) -> Result<GraphState, SgrError> {
        let unit_state = self.unit.read().await.state;
        let state = match unit_state {
            UnitState::Draft | UnitState::Queued | UnitState::Starting | UnitState::Stopping => {
                GraphState::Converging
            }
            UnitState::Failed => GraphState::Failed,
            UnitState::Degraded | UnitState::Unhealthy | UnitState::Retired => GraphState::Degraded,
            UnitState::Running | UnitState::Healthy | UnitState::Stopped => GraphState::Converged,
        };
        Ok(state)
    }

    async fn set_unit_state(
        &self,
        unit_id: &UnitId,
        new_state: UnitState,
    ) -> Result<aios_sgr::ServiceUnit, SgrError> {
        let mut unit = self.unit.write().await;
        if &unit.unit_id != unit_id {
            return Err(SgrError::UnitNotFound(unit_id.clone()));
        }
        let from = unit.state;
        if !is_legal_transition(from, new_state) {
            return Err(SgrError::InvalidStateTransition {
                from,
                to: new_state,
            });
        }
        unit.state = new_state;
        unit.last_transition_at = Utc::now();
        let updated = unit.clone();
        drop(unit);

        self.transitions.write().await.push((from, new_state));
        Ok(updated)
    }
}

#[test]
fn transition_table_contains_canonical_s15_1_entries() {
    assert_eq!(TRANSITIONS.len(), 25);
    for edge in [
        (UnitState::Draft, UnitState::Queued),
        (UnitState::Draft, UnitState::Retired),
        (UnitState::Queued, UnitState::Starting),
        (UnitState::Starting, UnitState::Running),
        (UnitState::Running, UnitState::Healthy),
        (UnitState::Running, UnitState::Stopped),
        (UnitState::Running, UnitState::Failed),
        (UnitState::Healthy, UnitState::Stopping),
        (UnitState::Degraded, UnitState::Unhealthy),
        (UnitState::Unhealthy, UnitState::Starting),
        (UnitState::Stopping, UnitState::Stopped),
        (UnitState::Stopped, UnitState::Queued),
        (UnitState::Failed, UnitState::Starting),
        (UnitState::Failed, UnitState::Retired),
    ] {
        assert!(TRANSITIONS.contains(&edge), "missing edge {edge:?}");
    }
}

#[test]
fn is_legal_transition_accepts_draft_to_queued() {
    assert!(is_legal_transition(UnitState::Draft, UnitState::Queued));
}

#[test]
fn is_legal_transition_rejects_queued_to_running() {
    assert!(!is_legal_transition(UnitState::Queued, UnitState::Running));
}

#[test]
fn is_legal_transition_rejects_failed_to_running() {
    assert!(!is_legal_transition(UnitState::Failed, UnitState::Running));
}

#[test]
fn is_legal_transition_rejects_failed_to_stopped_per_spec() {
    assert!(!is_legal_transition(UnitState::Failed, UnitState::Stopped));
}

#[tokio::test]
async fn start_queued_unit_drives_to_running() -> TestResult {
    let sk = signing_key(30);
    let graph = Arc::new(trusted_graph(&sk));
    let id = register_named(&graph, &sk, "start_queued").await?;
    let driver = UnitFsmDriver::new(Arc::clone(&graph) as Arc<dyn ServiceGraph>);

    let unit = driver.start(&id).await?;

    assert_eq!(unit.state, UnitState::Running);
    assert_eq!(graph.get_unit(&id).await?.state, UnitState::Running);
    Ok(())
}

#[tokio::test]
async fn start_draft_unit_records_intermediate_edges() -> TestResult {
    let id = unit_id("start_draft");
    let graph = Arc::new(RecordingServiceGraph::new(sample_unit(
        "start_draft",
        UnitState::Draft,
    )));
    let driver = UnitFsmDriver::new(Arc::clone(&graph) as Arc<dyn ServiceGraph>);

    let unit = driver.start(&id).await?;

    assert_eq!(unit.state, UnitState::Running);
    assert_eq!(
        graph.transitions().await,
        vec![
            (UnitState::Draft, UnitState::Queued),
            (UnitState::Queued, UnitState::Starting),
            (UnitState::Starting, UnitState::Running),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn start_running_unit_is_idempotent() -> TestResult {
    let id = unit_id("start_running");
    let graph = Arc::new(RecordingServiceGraph::new(sample_unit(
        "start_running",
        UnitState::Running,
    )));
    let driver = UnitFsmDriver::new(Arc::clone(&graph) as Arc<dyn ServiceGraph>);

    let unit = driver.start(&id).await?;

    assert_eq!(unit.state, UnitState::Running);
    assert!(graph.transitions().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn stop_running_unit_drives_to_stopped() -> TestResult {
    let sk = signing_key(31);
    let graph = Arc::new(trusted_graph(&sk));
    let id = running_unit(&graph, &sk, "stop_running").await?;
    let driver = UnitFsmDriver::new(Arc::clone(&graph) as Arc<dyn ServiceGraph>);

    let unit = driver.stop(&id).await?;

    assert_eq!(unit.state, UnitState::Stopped);
    assert_eq!(graph.get_unit(&id).await?.state, UnitState::Stopped);
    Ok(())
}

#[tokio::test]
async fn stop_stopped_unit_is_idempotent() -> TestResult {
    let id = unit_id("stop_stopped");
    let graph = Arc::new(RecordingServiceGraph::new(sample_unit(
        "stop_stopped",
        UnitState::Stopped,
    )));
    let driver = UnitFsmDriver::new(Arc::clone(&graph) as Arc<dyn ServiceGraph>);

    let unit = driver.stop(&id).await?;

    assert_eq!(unit.state, UnitState::Stopped);
    assert!(graph.transitions().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn restart_running_unit_stops_then_starts() -> TestResult {
    let id = unit_id("restart_running");
    let graph = Arc::new(RecordingServiceGraph::new(sample_unit(
        "restart_running",
        UnitState::Running,
    )));
    let driver = UnitFsmDriver::new(Arc::clone(&graph) as Arc<dyn ServiceGraph>);

    let unit = driver.restart(&id).await?;

    assert_eq!(unit.state, UnitState::Running);
    assert_eq!(
        graph.transitions().await,
        vec![
            (UnitState::Running, UnitState::Stopped),
            (UnitState::Stopped, UnitState::Queued),
            (UnitState::Queued, UnitState::Starting),
            (UnitState::Starting, UnitState::Running),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn restart_failed_unit_uses_spec_retry_path() -> TestResult {
    let id = unit_id("restart_failed");
    let graph = Arc::new(RecordingServiceGraph::new(sample_unit(
        "restart_failed",
        UnitState::Failed,
    )));
    let driver = UnitFsmDriver::new(Arc::clone(&graph) as Arc<dyn ServiceGraph>);

    let unit = driver.restart(&id).await?;

    assert_eq!(unit.state, UnitState::Running);
    assert_eq!(
        graph.transitions().await,
        vec![
            (UnitState::Failed, UnitState::Starting),
            (UnitState::Starting, UnitState::Running),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn mark_failed_running_to_failed() -> TestResult {
    let sk = signing_key(32);
    let graph = Arc::new(trusted_graph(&sk));
    let id = running_unit(&graph, &sk, "mark_failed_running").await?;
    let driver = UnitFsmDriver::new(Arc::clone(&graph) as Arc<dyn ServiceGraph>);

    let unit = driver
        .mark_failed(&id, "adapter refused".to_owned())
        .await?;

    assert_eq!(unit.state, UnitState::Failed);
    assert_eq!(graph.get_unit(&id).await?.state, UnitState::Failed);
    Ok(())
}

#[tokio::test]
async fn mark_failed_stopped_rejects_per_spec() -> TestResult {
    let sk = signing_key(33);
    let graph = Arc::new(trusted_graph(&sk));
    let id = running_unit(&graph, &sk, "mark_failed_stopped").await?;
    graph.set_unit_state(&id, UnitState::Stopped).await?;
    let driver = UnitFsmDriver::new(Arc::clone(&graph) as Arc<dyn ServiceGraph>);

    let err = driver
        .mark_failed(&id, "stopped units are terminal".to_owned())
        .await
        .expect_err("STOPPED -> FAILED is not in S15.1 §3.2.1");

    assert_invalid_transition(&err, UnitState::Stopped, UnitState::Failed);
    Ok(())
}

#[tokio::test]
async fn transition_queued_to_running_direct_rejects() -> TestResult {
    let sk = signing_key(34);
    let graph = Arc::new(trusted_graph(&sk));
    let id = register_named(&graph, &sk, "direct_start").await?;
    let driver = UnitFsmDriver::new(Arc::clone(&graph) as Arc<dyn ServiceGraph>);

    let err = driver
        .transition(&id, UnitState::Running)
        .await
        .expect_err("QUEUED -> RUNNING must go through STARTING");

    assert_invalid_transition(&err, UnitState::Queued, UnitState::Running);
    Ok(())
}

#[tokio::test]
async fn transition_failed_to_running_direct_rejects() -> TestResult {
    let sk = signing_key(35);
    let graph = Arc::new(trusted_graph(&sk));
    let id = register_named(&graph, &sk, "failed_direct_running").await?;
    graph.set_unit_state(&id, UnitState::Starting).await?;
    graph.set_unit_state(&id, UnitState::Failed).await?;
    let driver = UnitFsmDriver::new(Arc::clone(&graph) as Arc<dyn ServiceGraph>);

    let err = driver
        .transition(&id, UnitState::Running)
        .await
        .expect_err("FAILED -> RUNNING must go through STARTING");

    assert_invalid_transition(&err, UnitState::Failed, UnitState::Running);
    Ok(())
}

#[tokio::test]
async fn transition_failed_to_starting_is_legal_recovery_edge() -> TestResult {
    let sk = signing_key(36);
    let graph = Arc::new(trusted_graph(&sk));
    let id = register_named(&graph, &sk, "failed_to_starting").await?;
    graph.set_unit_state(&id, UnitState::Starting).await?;
    graph.set_unit_state(&id, UnitState::Failed).await?;
    let driver = UnitFsmDriver::new(Arc::clone(&graph) as Arc<dyn ServiceGraph>);

    let unit = driver.transition(&id, UnitState::Starting).await?;

    assert_eq!(unit.state, UnitState::Starting);
    Ok(())
}

#[tokio::test]
async fn concurrent_start_same_unit_converges_to_running() -> TestResult {
    let sk = signing_key(37);
    let graph = Arc::new(trusted_graph(&sk));
    let id = register_named(&graph, &sk, "concurrent_start").await?;
    let driver = Arc::new(UnitFsmDriver::new(
        Arc::clone(&graph) as Arc<dyn ServiceGraph>
    ));

    let mut handles = Vec::new();
    for _ in 0..5 {
        let driver = Arc::clone(&driver);
        let id = id.clone();
        handles.push(tokio::spawn(async move { driver.start(&id).await }));
    }

    for handle in handles {
        assert_eq!(handle.await.expect("join")?.state, UnitState::Running);
    }
    assert_eq!(graph.get_unit(&id).await?.state, UnitState::Running);
    Ok(())
}

#[tokio::test]
async fn concurrent_start_records_one_success_per_edge() -> TestResult {
    let id = unit_id("concurrent_recorded_start");
    let graph = Arc::new(RecordingServiceGraph::new(sample_unit(
        "concurrent_recorded_start",
        UnitState::Queued,
    )));
    let driver = Arc::new(UnitFsmDriver::new(
        Arc::clone(&graph) as Arc<dyn ServiceGraph>
    ));

    let mut handles = Vec::new();
    for _ in 0..5 {
        let driver = Arc::clone(&driver);
        let id = id.clone();
        handles.push(tokio::spawn(async move { driver.start(&id).await }));
    }
    for handle in handles {
        assert_eq!(handle.await.expect("join")?.state, UnitState::Running);
    }

    let counts = graph
        .transitions()
        .await
        .into_iter()
        .fold(HashMap::new(), |mut acc, edge| {
            *acc.entry(edge).or_insert(0usize) += 1;
            acc
        });
    assert_eq!(
        counts.get(&(UnitState::Queued, UnitState::Starting)),
        Some(&1)
    );
    assert_eq!(
        counts.get(&(UnitState::Starting, UnitState::Running)),
        Some(&1)
    );
    assert_eq!(counts.len(), 2);
    Ok(())
}

#[tokio::test]
async fn concurrent_start_and_stop_no_deadlock_final_running_or_stopped() -> TestResult {
    let sk = signing_key(38);
    let graph = Arc::new(trusted_graph(&sk));
    let id = running_unit(&graph, &sk, "concurrent_start_stop").await?;
    let driver = Arc::new(UnitFsmDriver::new(
        Arc::clone(&graph) as Arc<dyn ServiceGraph>
    ));

    let start_driver = Arc::clone(&driver);
    let start_id = id.clone();
    let start = tokio::spawn(async move { start_driver.start(&start_id).await });
    let stop_driver = Arc::clone(&driver);
    let stop_id = id.clone();
    let stop = tokio::spawn(async move { stop_driver.stop(&stop_id).await });

    start.await.expect("join")?;
    stop.await.expect("join")?;

    assert!(matches!(
        graph.get_unit(&id).await?.state,
        UnitState::Running | UnitState::Stopped
    ));
    Ok(())
}
