//! T-093 S15.1/S15.2/S15.3 acceptance fixtures.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::unwrap_used,
    reason = "acceptance fixtures are intentionally direct"
)]

use std::error::Error;
use std::sync::Arc;

use chrono::{DateTime, Duration, TimeZone, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;
use strum::EnumCount;

use aios_sgr::{
    adapter::AdapterRegistrationState as AdapterManifestRegistrationState, ABPromotionState,
    AdapterActionDeclaration, AdapterCapability, AdapterCapabilityClass, AdapterDeclaration,
    AdapterDispatchKind, AdapterFailureMode, AdapterIOMode, AdapterManifest,
    AdapterRollbackStrategy, AdapterStability, DependencyKind, DependencySolveResult, DesiredState,
    GpuBudget, GraphEvaluationResult, GraphEvaluator, HealthCheckKind, HealthCheckSpec,
    InMemoryServiceGraph, ResourceBudget, RestartBudget, RestartPolicy, RollbackPointer,
    RollbackTrigger, ServiceGraph, SgrAdapterRegistry, SgrError, TransitionKind, UnitDependency,
    UnitFsmDriver, UnitId, UnitKind, UnitManifest, UnitState, VerificationIntentRef,
};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const UNIT_AUTHORITY: &str = "pubcat_aiosroot_t093_accept";
const ADAPTER_AUTHORITY: &str = "key_aiosroot_t093_accept";

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

fn unsigned_unit_manifest(
    name: &str,
    desired_state: DesiredState,
    dependencies: Vec<UnitDependency>,
) -> UnitManifest {
    UnitManifest {
        schema_version: "aios.unit.v1alpha1".to_owned(),
        unit_id: unit_id(name),
        unit_kind: UnitKind::Service,
        display_name: format!("AIOS {name}"),
        description: "S15 acceptance fixture unit.".to_owned(),
        issued_at: fixed_time(),
        publisher_id: "pub_01HXY9ROOTAIOS01KEY".to_owned(),
        publisher_root_id: UNIT_AUTHORITY.to_owned(),
        publisher_signature: Vec::new(),
        canonical_hash: String::new(),
        dependencies,
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
            args: serde_json::json!({ "path": "/healthz" }),
        },
        startup_deadline_seconds: 60,
        stop_deadline_seconds: 30,
        adapter_target: serde_json::json!({ "requires": ["unit.start"] }),
        labels: Some(serde_json::json!({ "layer": "L3", "criticality": "standard" })),
        correlation_id: Some(format!("corr_accept_{name}")),
        desired_state,
        provides: vec![format!("service.{name}")],
        adapter_id: Some("adapter:aios:systemd:0.1.0".to_owned()),
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

fn sign_unit_manifest(manifest: &mut UnitManifest, sk: &SigningKey) -> TestResult {
    let bytes = serde_json::to_vec(&SignedUnitManifestBody::from(&*manifest))?;
    let digest = blake3::hash(&bytes);
    let digest_hex = digest.to_hex().to_string();
    digest_hex[..32].clone_into(&mut manifest.canonical_hash);
    manifest.publisher_signature = sk.sign(digest.as_bytes()).to_bytes().to_vec();
    Ok(())
}

fn signed_unit_manifest(
    name: &str,
    sk: &SigningKey,
    desired_state: DesiredState,
    dependencies: Vec<UnitDependency>,
) -> TestResult<UnitManifest> {
    let mut manifest = unsigned_unit_manifest(name, desired_state, dependencies);
    sign_unit_manifest(&mut manifest, sk)?;
    Ok(manifest)
}

fn graph(sk: &SigningKey) -> Arc<InMemoryServiceGraph> {
    Arc::new(InMemoryServiceGraph::with_trusted_authority(
        UNIT_AUTHORITY,
        sk.verifying_key(),
    ))
}

fn fsm(graph: Arc<InMemoryServiceGraph>) -> UnitFsmDriver {
    UnitFsmDriver::new(graph as Arc<dyn ServiceGraph>)
}

fn evaluator(graph: Arc<InMemoryServiceGraph>) -> GraphEvaluator {
    GraphEvaluator::new(graph as Arc<dyn ServiceGraph>)
}

async fn register_named(
    graph: &InMemoryServiceGraph,
    sk: &SigningKey,
    name: &str,
    desired_state: DesiredState,
    dependencies: Vec<UnitDependency>,
) -> TestResult<UnitId> {
    let manifest = signed_unit_manifest(name, sk, desired_state, dependencies)?;
    let id = manifest.unit_id.clone();
    graph.register_unit(manifest).await?;
    Ok(id)
}

fn adapter_declaration() -> AdapterDeclaration {
    AdapterDeclaration::Manifest(Box::new(AdapterManifest {
        adapter_id: "adapter:aios:systemd:0.1.0".to_owned(),
        vendor: "aios".to_owned(),
        name: "systemd".to_owned(),
        adapter_version: "0.1.0".to_owned(),
        spec_version: "v1alpha1".to_owned(),
        declared_actions: vec![AdapterActionDeclaration {
            action_kind: "unit.start".to_owned(),
            target_schema: serde_json::json!({ "type": "object" }),
            response_schema: serde_json::json!({ "type": "object" }),
            rollback_strategy: AdapterRollbackStrategy::IdempotentReverse,
            timeout_seconds: 30,
            template_string: None,
            template_substitution_variables: Vec::new(),
            per_action_capabilities: vec![AdapterCapabilityClass::ServiceLifecycle],
        }],
        declared_capabilities: vec![AdapterCapabilityClass::ServiceLifecycle],
        declared_invariants_supported: vec!["INV-013".to_owned()],
        io_mode: AdapterIOMode::TypedParametersOnly,
        preferred_dispatch_kind: AdapterDispatchKind::SubprocessFork,
        declared_stability: AdapterStability::Registered,
        sandbox_profile_minimum: serde_json::json!({ "network": "none" }),
        declared_failure_modes: vec![AdapterFailureMode::AdapterTimeout],
        default_adapter_timeout_seconds: 30,
        default_rollback_timeout_seconds: 30,
        network_outbound_hosts: Vec::new(),
        external_api_hosts: Vec::new(),
        declared_evidence_record_types: Vec::new(),
        source_package_id: "pkg:aios:adapter-systemd:0.1.0".to_owned(),
        publisher_root_id: "pubcat_aiosroot".to_owned(),
        manifest_signature: Vec::new(),
        signing_key_id: ADAPTER_AUTHORITY.to_owned(),
        manifest_created_at: fixed_time(),
        manifest_expires_at: fixed_time() + Duration::days(365),
    }))
}

#[derive(Serialize)]
struct SignedCapabilityBody<'a> {
    capability_id: &'a str,
    provides: &'a [String],
    requires: &'a [String],
    risk_template: &'a str,
}

fn sign_capability(capability: &mut AdapterCapability, sk: &SigningKey) -> TestResult {
    let bytes = serde_json::to_vec(&SignedCapabilityBody {
        capability_id: &capability.capability_id,
        provides: &capability.provides,
        requires: &capability.requires,
        risk_template: &capability.risk_template,
    })?;
    capability.manifest_signature_ed25519 = sk.sign(&bytes).to_bytes().to_vec();
    Ok(())
}

fn capability(capability_id: &str, provides: &[&str]) -> AdapterCapability {
    AdapterCapability {
        capability_id: capability_id.to_owned(),
        provides: provides.iter().map(ToString::to_string).collect(),
        requires: vec!["SERVICE_LIFECYCLE".to_owned()],
        risk_template: "REQUIRE_APPROVAL".to_owned(),
        manifest_signature_ed25519: Vec::new(),
    }
}

#[tokio::test]
async fn s151_closed_unit_manifest_vocabulary_counts_match_spec() {
    assert_eq!(UnitKind::COUNT, 10);
    assert_eq!(UnitState::COUNT, 11);
    assert_eq!(RestartPolicy::COUNT, 5);
    assert_eq!(HealthCheckKind::COUNT, 5);
}

#[tokio::test]
async fn s151_unit_id_and_signed_manifest_are_admitted_only_under_trusted_authority() -> TestResult
{
    let sk = signing_key(1);
    let graph = graph(&sk);
    assert!(UnitId::parse("unit:aiosroot:valid_name:blue").is_ok());
    assert!(UnitId::parse("bad:aiosroot:valid_name").is_err());

    let manifest = signed_unit_manifest("signed", &sk, DesiredState::Running, Vec::new())?;
    assert_eq!(manifest.canonical_hash.len(), 32);
    let unit = graph.register_unit(manifest).await?;
    assert_eq!(unit.state, UnitState::Queued);

    let mut forged = unsigned_unit_manifest("forged", DesiredState::Running, Vec::new());
    forged.publisher_root_id = "pubcat_unknown".to_owned();
    sign_unit_manifest(&mut forged, &sk)?;
    let err = graph
        .register_unit(forged)
        .await
        .expect_err("unknown authority must reject");
    assert!(matches!(err, SgrError::ManifestUnknownAuthority(_)));
    Ok(())
}

#[tokio::test]
async fn s151_dependency_cycle_is_rejected_by_graph_evaluator() -> TestResult {
    let sk = signing_key(2);
    let graph = graph(&sk);
    let first = register_named(&graph, &sk, "cycle_a", DesiredState::Running, Vec::new()).await?;
    let second = register_named(&graph, &sk, "cycle_b", DesiredState::Running, Vec::new()).await?;
    graph
        .declare_dependency(&first, &second, DependencyKind::RequiresHealthy)
        .await?;
    graph
        .declare_dependency(&second, &first, DependencyKind::RequiresHealthy)
        .await?;

    let err = evaluator(graph)
        .topological_sort()
        .await
        .expect_err("cycle must reject topo sort");
    assert!(
        matches!(err, SgrError::DependencyCycleDetected(nodes) if nodes == vec![first, second])
    );
    Ok(())
}

#[tokio::test]
async fn s152_closed_graph_transition_vocabulary_counts_match_spec() {
    assert_eq!(GraphEvaluationResult::COUNT, 5);
    assert_eq!(TransitionKind::COUNT, 10);
    assert_eq!(DependencySolveResult::COUNT, 4);
    assert_eq!(ABPromotionState::COUNT, 5);
}

#[tokio::test]
async fn s152_converged_short_circuit_matches_desired_stopped_state() -> TestResult {
    let sk = signing_key(3);
    let graph = graph(&sk);
    let unit = register_named(
        &graph,
        &sk,
        "already_stopped",
        DesiredState::Stopped,
        Vec::new(),
    )
    .await?;
    let fsm = fsm(graph.clone());
    fsm.start(&unit).await?;
    fsm.stop(&unit).await?;

    let evaluator = evaluator(graph);
    assert!(evaluator.is_converged().await?);
    assert_eq!(
        evaluator.convergence_state().await?,
        aios_sgr::GraphState::Converged
    );
    Ok(())
}

#[tokio::test]
async fn s152_single_start_transition_reaches_running() -> TestResult {
    let sk = signing_key(4);
    let graph = graph(&sk);
    let unit = register_named(&graph, &sk, "start_one", DesiredState::Running, Vec::new()).await?;

    let started = fsm(graph.clone()).start(&unit).await?;

    assert_eq!(started.state, UnitState::Running);
    assert!(evaluator(graph).is_converged().await?);
    Ok(())
}

#[tokio::test]
async fn s152_blocked_dependency_waits_until_prerequisite_runs() -> TestResult {
    let sk = signing_key(5);
    let graph = graph(&sk);
    let db = register_named(&graph, &sk, "db", DesiredState::Running, Vec::new()).await?;
    let app = register_named(
        &graph,
        &sk,
        "app_waits",
        DesiredState::Running,
        vec![UnitDependency {
            unit_id: db.clone(),
            kind: DependencyKind::RequiresRunning,
        }],
    )
    .await?;
    let evaluator = evaluator(graph.clone());
    assert!(!evaluator.evaluate_readiness(&app).await?);

    fsm(graph).start(&db).await?;
    assert!(evaluator.evaluate_readiness(&app).await?);
    Ok(())
}

#[tokio::test]
async fn s153_closed_adapter_vocabulary_counts_match_spec() {
    assert_eq!(AdapterManifestRegistrationState::COUNT, 6);
    assert_eq!(AdapterCapabilityClass::COUNT, 10);
    assert_eq!(AdapterIOMode::COUNT, 2);
    assert_eq!(AdapterDispatchKind::COUNT, 4);
    assert_eq!(AdapterFailureMode::COUNT, 10);
}

#[tokio::test]
async fn s153_valid_adapter_registration_becomes_active() -> TestResult {
    let sk = signing_key(6);
    let registry = SgrAdapterRegistry::with_trusted_authority(
        ADAPTER_AUTHORITY.to_owned(),
        sk.verifying_key(),
    );
    let mut cap = capability("cap_service_lifecycle", &["unit.start"]);
    sign_capability(&mut cap, &sk)?;

    let registered = registry
        .register_adapter(cap, adapter_declaration())
        .await?;

    assert_eq!(registered.state, aios_sgr::AdapterRegistrationState::Active);
    Ok(())
}

#[tokio::test]
async fn s153_forged_adapter_signature_rejects_fail_closed() -> TestResult {
    let sk = signing_key(7);
    let registry = SgrAdapterRegistry::with_trusted_authority(
        ADAPTER_AUTHORITY.to_owned(),
        sk.verifying_key(),
    );
    let mut cap = capability("cap_service_lifecycle", &["unit.start"]);
    sign_capability(&mut cap, &sk)?;
    cap.manifest_signature_ed25519[0] ^= 0x01;

    let err = registry
        .register_adapter(cap, adapter_declaration())
        .await
        .expect_err("bad adapter signature must reject");

    assert!(matches!(err, SgrError::ManifestSignatureInvalid));
    assert!(registry.list_adapters().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn s153_unit_requires_all_capabilities_or_no_adapter_is_selected() -> TestResult {
    let sk = signing_key(8);
    let registry = SgrAdapterRegistry::with_trusted_authority(
        ADAPTER_AUTHORITY.to_owned(),
        sk.verifying_key(),
    );
    let mut cap = capability("cap_partial", &["unit.start"]);
    sign_capability(&mut cap, &sk)?;
    registry
        .register_adapter(cap, adapter_declaration())
        .await?;

    let mut manifest = unsigned_unit_manifest("needs_two", DesiredState::Running, Vec::new());
    manifest.adapter_target = serde_json::json!({ "requires": ["unit.start", "unit.stop"] });

    assert!(registry.find_adapter_for_unit(&manifest).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn s153_suspended_adapter_is_not_selected_for_dispatch() -> TestResult {
    let sk = signing_key(9);
    let registry = SgrAdapterRegistry::with_trusted_authority(
        ADAPTER_AUTHORITY.to_owned(),
        sk.verifying_key(),
    );
    let mut cap = capability("cap_suspend", &["unit.start"]);
    sign_capability(&mut cap, &sk)?;
    registry
        .register_adapter(cap, adapter_declaration())
        .await?;
    registry
        .suspend_adapter("cap_suspend", "maintenance")
        .await?;
    let manifest = unsigned_unit_manifest("needs_start", DesiredState::Running, Vec::new());

    assert!(registry.find_adapter_for_unit(&manifest).await?.is_none());
    Ok(())
}
