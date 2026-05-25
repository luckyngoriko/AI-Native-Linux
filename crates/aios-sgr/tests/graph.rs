//! T-085 `ServiceGraph` contract tests for S15.1.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "Integration-test failures should point at the failing contract"
)]

use std::error::Error;
use std::sync::Arc;

use aios_sgr::{
    DependencyKind, DesiredState, GpuBudget, GraphState, HealthCheckKind, HealthCheckSpec,
    InMemoryServiceGraph, ResourceBudget, RestartBudget, RestartPolicy, RollbackPointer,
    RollbackTrigger, ServiceGraph, SgrError, UnitDependency, UnitId, UnitKind, UnitManifest,
    UnitState, VerificationIntentRef,
};
use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;

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

#[tokio::test]
async fn new_graph_starts_empty() -> TestResult {
    let graph = InMemoryServiceGraph::new();

    assert_eq!(graph.graph_state().await?, GraphState::Empty);
    assert!(graph.list_units().await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn register_unit_with_valid_signature_starts_queued() -> TestResult {
    let sk = signing_key(11);
    let graph = trusted_graph(&sk);

    let unit = graph
        .register_unit(signed_manifest("capability_runtime", &sk)?)
        .await?;

    assert_eq!(unit.unit_id, unit_id("capability_runtime"));
    assert_eq!(unit.state, UnitState::Queued);
    assert_eq!(unit.evidence_chain, Vec::<String>::new());
    Ok(())
}

#[tokio::test]
async fn register_unit_with_bad_signature_rejects() -> TestResult {
    let sk = signing_key(12);
    let graph = trusted_graph(&sk);
    let mut manifest = signed_manifest("bad_signature", &sk)?;
    manifest.publisher_signature[0] ^= 0xff;

    let err = graph
        .register_unit(manifest)
        .await
        .expect_err("bad signature must reject");

    assert!(matches!(err, SgrError::ManifestSignatureInvalid));
    assert!(graph.list_units().await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn register_unit_from_unknown_authority_rejects() -> TestResult {
    let sk = signing_key(13);
    let graph = trusted_graph(&sk);
    let mut manifest = unsigned_manifest("unknown_authority");
    manifest.publisher_root_id = "pubcat_unknown".to_owned();
    sign_manifest(&mut manifest, &sk)?;

    let err = graph
        .register_unit(manifest)
        .await
        .expect_err("unknown authority must reject");

    assert!(
        matches!(err, SgrError::ManifestUnknownAuthority(ref name) if name == "pubcat_unknown")
    );
    Ok(())
}

#[tokio::test]
async fn register_duplicate_unit_id_rejects() -> TestResult {
    let sk = signing_key(14);
    let graph = trusted_graph(&sk);
    graph
        .register_unit(signed_manifest("duplicate", &sk)?)
        .await?;

    let err = graph
        .register_unit(signed_manifest("duplicate", &sk)?)
        .await
        .expect_err("duplicate unit id must reject");

    assert!(matches!(err, SgrError::UnitAlreadyRegistered(ref id) if id == &unit_id("duplicate")));
    assert_eq!(graph.list_units().await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn get_unit_returns_registered_unit() -> TestResult {
    let sk = signing_key(15);
    let graph = trusted_graph(&sk);
    let id = register_named(&graph, &sk, "lookup").await?;

    let unit = graph.get_unit(&id).await?;

    assert_eq!(unit.unit_id, id);
    Ok(())
}

#[tokio::test]
async fn get_unit_unknown_rejects() -> TestResult {
    let graph = InMemoryServiceGraph::new();
    let missing = unit_id("missing");

    let err = graph
        .get_unit(&missing)
        .await
        .expect_err("unknown unit must reject");

    assert!(matches!(err, SgrError::UnitNotFound(ref id) if id == &missing));
    Ok(())
}

#[tokio::test]
async fn list_units_returns_all_registered_units() -> TestResult {
    let sk = signing_key(16);
    let graph = trusted_graph(&sk);
    register_named(&graph, &sk, "alpha").await?;
    register_named(&graph, &sk, "beta").await?;

    let mut ids = graph
        .list_units()
        .await?
        .into_iter()
        .map(|unit| unit.unit_id.to_string())
        .collect::<Vec<_>>();
    ids.sort();

    assert_eq!(ids, vec!["unit:aiosroot:alpha", "unit:aiosroot:beta"]);
    Ok(())
}

#[tokio::test]
async fn declare_dependency_happy_path_returns_edge() -> TestResult {
    let sk = signing_key(17);
    let graph = trusted_graph(&sk);
    let source = register_named(&graph, &sk, "source").await?;
    let target = register_named(&graph, &sk, "target").await?;

    let edge = graph
        .declare_dependency(&source, &target, DependencyKind::RequiresHealthy)
        .await?;

    assert_eq!(edge.from_unit_id, source);
    assert_eq!(edge.to_unit_id, target);
    assert_eq!(edge.kind, DependencyKind::RequiresHealthy);
    Ok(())
}

#[tokio::test]
async fn declare_dependency_to_unknown_target_rejects() -> TestResult {
    let sk = signing_key(18);
    let graph = trusted_graph(&sk);
    let source = register_named(&graph, &sk, "source").await?;
    let missing = unit_id("missing_target");

    let err = graph
        .declare_dependency(&source, &missing, DependencyKind::RequiresRunning)
        .await
        .expect_err("unknown target must reject");

    assert!(matches!(err, SgrError::DependencyTargetNotRegistered(ref id) if id == &missing));
    Ok(())
}

#[tokio::test]
async fn list_dependencies_returns_edges_for_unit() -> TestResult {
    let sk = signing_key(19);
    let graph = trusted_graph(&sk);
    let source = register_named(&graph, &sk, "source").await?;
    let first = register_named(&graph, &sk, "first").await?;
    let second = register_named(&graph, &sk, "second").await?;
    graph
        .declare_dependency(&source, &first, DependencyKind::RequiresHealthy)
        .await?;
    graph
        .declare_dependency(&source, &second, DependencyKind::OrdersAfter)
        .await?;

    let mut targets = graph
        .list_dependencies(&source)
        .await?
        .into_iter()
        .map(|edge| edge.to_unit_id.to_string())
        .collect::<Vec<_>>();
    targets.sort();

    assert_eq!(targets, vec!["unit:aiosroot:first", "unit:aiosroot:second"]);
    Ok(())
}

#[tokio::test]
async fn set_unit_state_accepts_valid_transition() -> TestResult {
    let sk = signing_key(20);
    let graph = trusted_graph(&sk);
    let id = register_named(&graph, &sk, "stateful").await?;

    let unit = graph.set_unit_state(&id, UnitState::Starting).await?;

    assert_eq!(unit.state, UnitState::Starting);
    Ok(())
}

#[tokio::test]
async fn set_unit_state_rejects_invalid_transition() -> TestResult {
    let sk = signing_key(21);
    let graph = trusted_graph(&sk);
    let id = register_named(&graph, &sk, "invalid_state").await?;

    let err = graph
        .set_unit_state(&id, UnitState::Running)
        .await
        .expect_err("QUEUED -> RUNNING must reject");

    assert!(matches!(
        err,
        SgrError::InvalidStateTransition {
            from: UnitState::Queued,
            to: UnitState::Running,
        }
    ));
    Ok(())
}

#[tokio::test]
async fn graph_state_tracks_empty_converging_and_converged() -> TestResult {
    let sk = signing_key(22);
    let graph = trusted_graph(&sk);
    assert_eq!(graph.graph_state().await?, GraphState::Empty);
    let id = register_named(&graph, &sk, "graph_state").await?;
    assert_eq!(graph.graph_state().await?, GraphState::Converging);

    graph.set_unit_state(&id, UnitState::Starting).await?;
    graph.set_unit_state(&id, UnitState::Running).await?;

    assert_eq!(graph.graph_state().await?, GraphState::Converged);
    Ok(())
}

#[tokio::test]
async fn graph_state_degraded_when_noncritical_unit_failed() -> TestResult {
    let sk = signing_key(23);
    let graph = trusted_graph(&sk);
    let failed = register_named(&graph, &sk, "noncritical_failed").await?;
    let running = register_named(&graph, &sk, "running_peer").await?;
    graph.set_unit_state(&failed, UnitState::Starting).await?;
    graph.set_unit_state(&failed, UnitState::Failed).await?;
    graph.set_unit_state(&running, UnitState::Starting).await?;
    graph.set_unit_state(&running, UnitState::Running).await?;

    assert_eq!(graph.graph_state().await?, GraphState::Degraded);
    Ok(())
}

#[tokio::test]
async fn graph_state_failed_when_critical_unit_failed() -> TestResult {
    let sk = signing_key(24);
    let graph = trusted_graph(&sk);
    let mut manifest = signed_manifest("critical_failed", &sk)?;
    manifest.labels = Some(serde_json::json!({ "criticality": "critical" }));
    sign_manifest(&mut manifest, &sk)?;
    let id = manifest.unit_id.clone();
    graph.register_unit(manifest).await?;
    graph.set_unit_state(&id, UnitState::Starting).await?;
    graph.set_unit_state(&id, UnitState::Failed).await?;

    assert_eq!(graph.graph_state().await?, GraphState::Failed);
    Ok(())
}

#[tokio::test]
async fn arc_dyn_service_graph_end_to_end() -> TestResult {
    let sk = signing_key(25);
    let graph: Arc<dyn ServiceGraph> = Arc::new(trusted_graph(&sk));
    let source_manifest = signed_manifest("arc_source", &sk)?;
    let source = source_manifest.unit_id.clone();
    let target_manifest = signed_manifest("arc_target", &sk)?;
    let target = target_manifest.unit_id.clone();

    graph.register_unit(source_manifest).await?;
    graph.register_unit(target_manifest).await?;
    graph
        .declare_dependency(&source, &target, DependencyKind::RequiresRunning)
        .await?;
    graph.set_unit_state(&source, UnitState::Starting).await?;

    assert_eq!(graph.get_unit(&source).await?.state, UnitState::Starting);
    assert_eq!(graph.list_dependencies(&source).await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn concurrent_registration_and_reads_are_consistent() -> TestResult {
    let sk = signing_key(26);
    let graph = Arc::new(trusted_graph(&sk));
    let manifests = (0..8)
        .map(|idx| signed_manifest(&format!("worker_{idx}"), &sk))
        .collect::<TestResult<Vec<_>>>()?;

    let mut handles = Vec::new();
    for manifest in manifests {
        let graph = Arc::clone(&graph);
        handles.push(tokio::spawn(
            async move { graph.register_unit(manifest).await },
        ));
    }
    for handle in handles {
        handle.await.expect("join")?;
    }

    let mut read_handles = Vec::new();
    for idx in 0..8 {
        let graph = Arc::clone(&graph);
        let id = unit_id(&format!("worker_{idx}"));
        read_handles.push(tokio::spawn(async move { graph.get_unit(&id).await }));
    }
    for handle in read_handles {
        assert_eq!(handle.await.expect("join")?.state, UnitState::Queued);
    }
    assert_eq!(graph.list_units().await?.len(), 8);
    Ok(())
}
