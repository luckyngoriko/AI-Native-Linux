//! T-089 integration tests for `aios.sgr.v1alpha1.SgrService`.

#![allow(
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::panic,
    clippy::result_large_err,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::unwrap_used,
    reason = "test code; panic-on-failure is the idiomatic contract signal"
)]

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use aios_sgr::service::conversions::{
    adapter_capability_from_proto, adapter_capability_to_proto, dependency_edge_from_proto,
    dependency_edge_to_proto, graph_state_from_proto, graph_state_to_proto,
    registered_adapter_from_proto, registered_adapter_to_proto, service_unit_from_proto,
    service_unit_to_proto, sgr_error_to_status, unit_manifest_from_proto, unit_manifest_to_proto,
    unit_state_from_proto, unit_state_to_proto,
};
use aios_sgr::service::proto::sgr_service_server::SgrService as _;
use aios_sgr::service::proto::{
    DeclareDependencyRequest, EvaluateGraphRequest, FindAdapterForUnitRequest,
    GetGraphStateRequest, GetUnitRequest, ListAdaptersRequest, ListDependenciesRequest,
    ListUnitsRequest, LookupAdapterRequest, MarkUnitFailedRequest, RegisterAdapterRequest,
    RegisterUnitRequest, RestartUnitRequest, StartUnitRequest, StopUnitRequest,
    TraverseGraphRequest,
};
use aios_sgr::service::{SgrServiceClient, SgrServiceGrpcServer, SgrServiceImpl, SCHEMA_VERSION};
use aios_sgr::{
    AdapterActionDeclaration, AdapterCapability, AdapterCapabilityClass, AdapterDeclaration,
    AdapterDispatchKind, AdapterFailureMode, AdapterIOMode, AdapterManifest,
    AdapterRegistrationState, AdapterRollbackStrategy, AdapterStability, DependencyEdge,
    DependencyKind, DesiredState, GpuBudget, GraphEvaluator, GraphState, HealthCheckKind,
    HealthCheckSpec, InMemoryServiceGraph, RegisteredAdapter, ResourceBudget, RestartBudget,
    RestartPolicy, RollbackPointer, RollbackTrigger, ServiceGraph, SgrAdapterRegistry, SgrError,
    UnitDependency, UnitFsmDriver, UnitId, UnitKind, UnitManifest, UnitState,
    VerificationIntentRef,
};
use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tonic::{Code, Request};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const UNIT_AUTHORITY: &str = "pubcat_aiosroot";
const ADAPTER_AUTHORITY: &str = "key_aiosroot_2026q2";

struct Harness {
    svc: SgrServiceImpl,
    unit_signing_key: SigningKey,
    adapter_signing_key: SigningKey,
}

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

fn harness() -> Harness {
    let unit_signing_key = signing_key(81);
    let adapter_signing_key = signing_key(82);
    let graph = Arc::new(InMemoryServiceGraph::with_trusted_authority(
        UNIT_AUTHORITY,
        unit_signing_key.verifying_key(),
    ));
    let graph_for_fsm: Arc<dyn ServiceGraph> = graph.clone();
    let graph_for_evaluator: Arc<dyn ServiceGraph> = graph.clone();
    let fsm = Arc::new(UnitFsmDriver::new(graph_for_fsm));
    let evaluator = Arc::new(GraphEvaluator::new(graph_for_evaluator));
    let registry = Arc::new(SgrAdapterRegistry::with_trusted_authority(
        ADAPTER_AUTHORITY.to_owned(),
        adapter_signing_key.verifying_key(),
    ));
    let svc = SgrServiceImpl::new(graph, fsm, evaluator, registry);

    Harness {
        svc,
        unit_signing_key,
        adapter_signing_key,
    }
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
        publisher_root_id: UNIT_AUTHORITY.to_owned(),
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
            args: serde_json::json!({ "path": "/healthz" }),
        },
        startup_deadline_seconds: 60,
        stop_deadline_seconds: 30,
        adapter_target: serde_json::json!({ "requires": ["unit.start"] }),
        labels: Some(serde_json::json!({ "layer": "L3", "criticality": "standard" })),
        correlation_id: Some(format!("corr_{name}")),
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

fn register_unit_request(manifest: &UnitManifest) -> RegisterUnitRequest {
    RegisterUnitRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        manifest: Some(unit_manifest_to_proto(manifest)),
        action_id_proto: Vec::new(),
        action_id_format: 0,
    }
}

async fn register_named(svc: &SgrServiceImpl, sk: &SigningKey, name: &str) -> TestResult<UnitId> {
    let manifest = signed_manifest(name, sk)?;
    let id = manifest.unit_id.clone();
    svc.register_unit(Request::new(register_unit_request(&manifest)))
        .await?;
    Ok(id)
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

#[derive(Debug, Serialize)]
struct SignedCapabilityBody<'a> {
    capability_id: &'a str,
    provides: &'a [String],
    requires: &'a [String],
    risk_template: &'a str,
}

impl<'a> From<&'a AdapterCapability> for SignedCapabilityBody<'a> {
    fn from(capability: &'a AdapterCapability) -> Self {
        Self {
            capability_id: &capability.capability_id,
            provides: &capability.provides,
            requires: &capability.requires,
            risk_template: &capability.risk_template,
        }
    }
}

fn sign_capability(capability: &mut AdapterCapability, sk: &SigningKey) -> TestResult {
    let body = SignedCapabilityBody::from(&*capability);
    capability.manifest_signature_ed25519 =
        sk.sign(&serde_json::to_vec(&body)?).to_bytes().to_vec();
    Ok(())
}

fn adapter_declaration(signing_key_id: &str) -> AdapterDeclaration {
    AdapterDeclaration::Manifest(Box::new(AdapterManifest {
        adapter_id: "adapter:aiosroot:systemd:1.0.0".to_owned(),
        vendor: "aiosroot".to_owned(),
        name: "systemd".to_owned(),
        adapter_version: "1.0.0".to_owned(),
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
        source_package_id: "pkg:aiosroot:adapter-systemd:1.0.0".to_owned(),
        publisher_root_id: "pubcat_aiosroot".to_owned(),
        manifest_signature: Vec::new(),
        signing_key_id: signing_key_id.to_owned(),
        manifest_created_at: fixed_time(),
        manifest_expires_at: Utc
            .with_ymd_and_hms(2026, 8, 25, 10, 0, 0)
            .single()
            .expect("valid datetime"),
    }))
}

fn register_adapter_request(
    capability: &AdapterCapability,
    declaration: &AdapterDeclaration,
) -> TestResult<RegisterAdapterRequest> {
    Ok(RegisterAdapterRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        capability: Some(adapter_capability_to_proto(capability)),
        declaration_json: serde_json::to_vec(declaration)?,
        action_id_proto: Vec::new(),
        action_id_format: 0,
    })
}

async fn register_adapter(
    svc: &SgrServiceImpl,
    sk: &SigningKey,
    capability_id: &str,
    provides: &[&str],
) -> TestResult<RegisteredAdapter> {
    let mut cap = capability(capability_id, provides);
    sign_capability(&mut cap, sk)?;
    let response = svc
        .register_adapter(Request::new(register_adapter_request(
            &cap,
            &adapter_declaration(ADAPTER_AUTHORITY),
        )?))
        .await?
        .into_inner();
    Ok(registered_adapter_from_proto(response)?)
}

fn unit_request(unit_id: &UnitId) -> GetUnitRequest {
    GetUnitRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        unit_id: unit_id.to_string(),
    }
}

async fn pick_port() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind random port");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);
    addr
}

async fn spawn_server(
    svc: SgrServiceImpl,
) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let addr = pick_port().await;
    let (tx, rx) = oneshot::channel::<()>();
    let server = tonic::transport::Server::builder().add_service(SgrServiceGrpcServer::new(svc));
    let handle = tokio::spawn(async move {
        server
            .serve_with_shutdown(addr, async move {
                let _ = rx.await;
            })
            .await
            .expect("server task");
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    (addr, tx, handle)
}

#[tokio::test]
async fn register_unit_valid_returns_service_unit_proto() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        ..
    } = harness();
    let manifest = signed_manifest("register_valid", &unit_signing_key)?;

    let unit = svc
        .register_unit(Request::new(register_unit_request(&manifest)))
        .await?
        .into_inner();

    assert_eq!(unit.unit_id, "unit:aiosroot:register_valid");
    assert_eq!(unit_state_from_proto(unit.state())?, UnitState::Queued);
    Ok(())
}

#[tokio::test]
async fn register_unit_bad_signature_maps_permission_denied() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        ..
    } = harness();
    let mut manifest = signed_manifest("bad_signature", &unit_signing_key)?;
    manifest.publisher_signature[0] ^= 0x01;

    let err = svc
        .register_unit(Request::new(register_unit_request(&manifest)))
        .await
        .expect_err("bad signature must reject");

    assert_eq!(err.code(), Code::PermissionDenied);
    Ok(())
}

#[tokio::test]
async fn get_unit_known_returns_service_unit_proto() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        ..
    } = harness();
    let id = register_named(&svc, &unit_signing_key, "known").await?;

    let unit = svc
        .get_unit(Request::new(unit_request(&id)))
        .await?
        .into_inner();

    assert_eq!(unit.unit_id, id.to_string());
    Ok(())
}

#[tokio::test]
async fn get_unit_unknown_maps_not_found() -> TestResult {
    let Harness { svc, .. } = harness();

    let err = svc
        .get_unit(Request::new(unit_request(&unit_id("missing"))))
        .await
        .expect_err("missing unit must reject");

    assert_eq!(err.code(), Code::NotFound);
    Ok(())
}

#[tokio::test]
async fn list_units_returns_all_registered_units() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        ..
    } = harness();
    register_named(&svc, &unit_signing_key, "alpha").await?;
    register_named(&svc, &unit_signing_key, "beta").await?;

    let mut ids = svc
        .list_units(Request::new(ListUnitsRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
        }))
        .await?
        .into_inner()
        .units
        .into_iter()
        .map(|unit| unit.unit_id)
        .collect::<Vec<_>>();
    ids.sort();

    assert_eq!(ids, vec!["unit:aiosroot:alpha", "unit:aiosroot:beta"]);
    Ok(())
}

#[tokio::test]
async fn declare_dependency_happy_path_returns_edge() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        ..
    } = harness();
    let source = register_named(&svc, &unit_signing_key, "source").await?;
    let target = register_named(&svc, &unit_signing_key, "target").await?;

    let edge = svc
        .declare_dependency(Request::new(DeclareDependencyRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            from_unit_id: source.to_string(),
            to_unit_id: target.to_string(),
            kind: i32::from(
                aios_sgr::service::proto::DependencyKindProto::DependencyRequiresHealthy,
            ),
            action_id_proto: Vec::new(),
            action_id_format: 0,
        }))
        .await?
        .into_inner();

    assert_eq!(edge.from_unit_id, source.to_string());
    assert_eq!(edge.to_unit_id, target.to_string());
    assert_eq!(
        dependency_edge_from_proto(&edge)?.kind,
        DependencyKind::RequiresHealthy
    );
    Ok(())
}

#[tokio::test]
async fn declare_dependency_to_unknown_target_maps_invalid_argument() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        ..
    } = harness();
    let source = register_named(&svc, &unit_signing_key, "source").await?;

    let err = svc
        .declare_dependency(Request::new(DeclareDependencyRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            from_unit_id: source.to_string(),
            to_unit_id: unit_id("missing").to_string(),
            kind: i32::from(
                aios_sgr::service::proto::DependencyKindProto::DependencyRequiresRunning,
            ),
            action_id_proto: Vec::new(),
            action_id_format: 0,
        }))
        .await
        .expect_err("unknown dependency target must reject");

    assert_eq!(err.code(), Code::InvalidArgument);
    Ok(())
}

#[tokio::test]
async fn list_dependencies_returns_edges() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        ..
    } = harness();
    let source = register_named(&svc, &unit_signing_key, "source").await?;
    let target = register_named(&svc, &unit_signing_key, "target").await?;
    svc.declare_dependency(Request::new(DeclareDependencyRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        from_unit_id: source.to_string(),
        to_unit_id: target.to_string(),
        kind: i32::from(aios_sgr::service::proto::DependencyKindProto::DependencyOrdersAfter),
        action_id_proto: Vec::new(),
        action_id_format: 0,
    }))
    .await?;

    let edges = svc
        .list_dependencies(Request::new(ListDependenciesRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            unit_id: source.to_string(),
        }))
        .await?
        .into_inner()
        .edges;

    assert_eq!(edges.len(), 1);
    assert_eq!(
        dependency_edge_from_proto(&edges[0])?.kind,
        DependencyKind::OrdersAfter
    );
    Ok(())
}

#[tokio::test]
async fn traverse_graph_topological_returns_ordered_unit_ids() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        ..
    } = harness();
    let root = register_named(&svc, &unit_signing_key, "root").await?;
    let leaf = register_named(&svc, &unit_signing_key, "leaf").await?;
    svc.declare_dependency(Request::new(DeclareDependencyRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        from_unit_id: leaf.to_string(),
        to_unit_id: root.to_string(),
        kind: i32::from(aios_sgr::service::proto::DependencyKindProto::DependencyRequiresRunning),
        action_id_proto: Vec::new(),
        action_id_format: 0,
    }))
    .await?;

    let response = svc
        .traverse_graph(Request::new(TraverseGraphRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
        }))
        .await?
        .into_inner();

    assert_eq!(
        response.ordered_unit_ids,
        vec![root.to_string(), leaf.to_string()]
    );
    Ok(())
}

#[tokio::test]
async fn traverse_graph_with_cycle_maps_failed_precondition() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        ..
    } = harness();
    let a = register_named(&svc, &unit_signing_key, "cycle_a").await?;
    let b = register_named(&svc, &unit_signing_key, "cycle_b").await?;
    for (from, to) in [(&a, &b), (&b, &a)] {
        svc.declare_dependency(Request::new(DeclareDependencyRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            from_unit_id: from.to_string(),
            to_unit_id: to.to_string(),
            kind: i32::from(
                aios_sgr::service::proto::DependencyKindProto::DependencyRequiresRunning,
            ),
            action_id_proto: Vec::new(),
            action_id_format: 0,
        }))
        .await?;
    }

    let err = svc
        .traverse_graph(Request::new(TraverseGraphRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
        }))
        .await
        .expect_err("cycle must reject graph traversal");

    assert_eq!(err.code(), Code::FailedPrecondition);
    Ok(())
}

#[tokio::test]
async fn evaluate_graph_returns_convergence_state() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        ..
    } = harness();
    register_named(&svc, &unit_signing_key, "eval").await?;

    let response = svc
        .evaluate_graph(Request::new(EvaluateGraphRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
        }))
        .await?
        .into_inner();

    assert_eq!(
        graph_state_from_proto(response.convergence_state())?,
        GraphState::Converging
    );
    assert!(!response.converged);
    Ok(())
}

#[tokio::test]
async fn start_unit_drives_fsm_to_running() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        ..
    } = harness();
    let id = register_named(&svc, &unit_signing_key, "startable").await?;

    let unit = svc
        .start_unit(Request::new(StartUnitRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            unit_id: id.to_string(),
            action_id_proto: Vec::new(),
            action_id_format: 0,
        }))
        .await?
        .into_inner();

    assert_eq!(unit_state_from_proto(unit.state())?, UnitState::Running);
    Ok(())
}

#[tokio::test]
async fn stop_unit_drives_fsm_to_stopped() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        ..
    } = harness();
    let id = register_named(&svc, &unit_signing_key, "stoppable").await?;
    svc.start_unit(Request::new(StartUnitRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        unit_id: id.to_string(),
        action_id_proto: Vec::new(),
        action_id_format: 0,
    }))
    .await?;

    let unit = svc
        .stop_unit(Request::new(StopUnitRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            unit_id: id.to_string(),
            action_id_proto: Vec::new(),
            action_id_format: 0,
        }))
        .await?
        .into_inner();

    assert_eq!(unit_state_from_proto(unit.state())?, UnitState::Stopped);
    Ok(())
}

#[tokio::test]
async fn restart_unit_drives_fsm_to_running() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        ..
    } = harness();
    let id = register_named(&svc, &unit_signing_key, "restartable").await?;

    let unit = svc
        .restart_unit(Request::new(RestartUnitRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            unit_id: id.to_string(),
            action_id_proto: Vec::new(),
            action_id_format: 0,
        }))
        .await?
        .into_inner();

    assert_eq!(unit_state_from_proto(unit.state())?, UnitState::Running);
    Ok(())
}

#[tokio::test]
async fn mark_unit_failed_drives_fsm_to_failed() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        ..
    } = harness();
    let id = register_named(&svc, &unit_signing_key, "failing").await?;

    let unit = svc
        .mark_unit_failed(Request::new(MarkUnitFailedRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            unit_id: id.to_string(),
            reason: "test failure".to_owned(),
            action_id_proto: Vec::new(),
            action_id_format: 0,
        }))
        .await?
        .into_inner();

    assert_eq!(unit_state_from_proto(unit.state())?, UnitState::Failed);
    Ok(())
}

#[tokio::test]
async fn register_adapter_valid_returns_registered_adapter_proto() -> TestResult {
    let Harness {
        svc,
        adapter_signing_key,
        ..
    } = harness();

    let registered = register_adapter(
        &svc,
        &adapter_signing_key,
        "cap_service_lifecycle",
        &["unit.start"],
    )
    .await?;

    assert_eq!(registered.state, AdapterRegistrationState::Active);
    assert_eq!(registered.capability.capability_id, "cap_service_lifecycle");
    Ok(())
}

#[tokio::test]
async fn lookup_adapter_returns_registered_adapter() -> TestResult {
    let Harness {
        svc,
        adapter_signing_key,
        ..
    } = harness();
    register_adapter(&svc, &adapter_signing_key, "cap_lookup", &["unit.start"]).await?;

    let found = svc
        .lookup_adapter(Request::new(LookupAdapterRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            capability_id: "cap_lookup".to_owned(),
        }))
        .await?
        .into_inner();

    assert_eq!(
        found.capability.expect("capability").capability_id,
        "cap_lookup"
    );
    Ok(())
}

#[tokio::test]
async fn list_adapters_returns_all_registered_adapters() -> TestResult {
    let Harness {
        svc,
        adapter_signing_key,
        ..
    } = harness();
    register_adapter(&svc, &adapter_signing_key, "cap_a", &["unit.start"]).await?;
    register_adapter(&svc, &adapter_signing_key, "cap_b", &["unit.stop"]).await?;

    let adapters = svc
        .list_adapters(Request::new(ListAdaptersRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
        }))
        .await?
        .into_inner()
        .adapters;

    assert_eq!(adapters.len(), 2);
    Ok(())
}

#[tokio::test]
async fn find_adapter_for_unit_matching_returns_some() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        adapter_signing_key,
    } = harness();
    register_adapter(
        &svc,
        &adapter_signing_key,
        "cap_service_lifecycle",
        &["unit.start", "unit.stop"],
    )
    .await?;
    let manifest = signed_manifest("needs_adapter", &unit_signing_key)?;

    let response = svc
        .find_adapter_for_unit(Request::new(FindAdapterForUnitRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            manifest: Some(unit_manifest_to_proto(&manifest)),
        }))
        .await?
        .into_inner();

    assert_eq!(
        response
            .adapter
            .expect("matching adapter")
            .capability
            .expect("capability")
            .capability_id,
        "cap_service_lifecycle"
    );
    Ok(())
}

#[tokio::test]
async fn find_adapter_for_unit_no_match_returns_none() -> TestResult {
    let Harness {
        svc,
        unit_signing_key,
        ..
    } = harness();
    let manifest = signed_manifest("no_adapter", &unit_signing_key)?;

    let response = svc
        .find_adapter_for_unit(Request::new(FindAdapterForUnitRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            manifest: Some(unit_manifest_to_proto(&manifest)),
        }))
        .await?
        .into_inner();

    assert!(response.adapter.is_none());
    Ok(())
}

#[test]
fn conversions_roundtrip_core_wire_types() -> TestResult {
    let sk = signing_key(83);
    let manifest = signed_manifest("convert", &sk)?;
    assert_eq!(
        unit_manifest_from_proto(unit_manifest_to_proto(&manifest))?,
        manifest
    );

    let unit = aios_sgr::ServiceUnit {
        unit_id: unit_id("convert"),
        manifest: signed_manifest("convert", &sk)?,
        state: UnitState::Running,
        last_transition_at: fixed_time(),
        evidence_chain: vec!["evr_test".to_owned()],
    };
    assert_eq!(service_unit_from_proto(service_unit_to_proto(&unit))?, unit);

    let edge = DependencyEdge {
        from_unit_id: unit_id("convert"),
        to_unit_id: unit_id("target"),
        kind: DependencyKind::OrdersAfter,
    };
    assert_eq!(
        dependency_edge_from_proto(&dependency_edge_to_proto(&edge))?,
        edge
    );
    assert_eq!(
        graph_state_from_proto(graph_state_to_proto(GraphState::Converged))?,
        GraphState::Converged
    );
    assert_eq!(
        unit_state_from_proto(unit_state_to_proto(UnitState::Healthy))?,
        UnitState::Healthy
    );
    Ok(())
}

#[test]
fn conversions_roundtrip_adapter_wire_types() -> TestResult {
    let sk = signing_key(84);
    let mut cap = capability("cap_roundtrip", &["unit.start"]);
    sign_capability(&mut cap, &sk)?;
    assert_eq!(
        adapter_capability_from_proto(adapter_capability_to_proto(&cap))?,
        cap
    );

    let registered = RegisteredAdapter {
        capability: cap,
        declaration: adapter_declaration(ADAPTER_AUTHORITY),
        registered_at: fixed_time(),
        state: AdapterRegistrationState::Active,
    };
    assert_eq!(
        registered_adapter_from_proto(registered_adapter_to_proto(&registered)?)?,
        registered
    );
    Ok(())
}

#[test]
fn sgr_error_status_mapping_matches_t089_contract() {
    assert_eq!(
        sgr_error_to_status(&SgrError::UnitNotFound(unit_id("missing"))).code(),
        Code::NotFound
    );
    assert_eq!(
        sgr_error_to_status(&SgrError::DependencyCycleDetected(vec![unit_id("a")])).code(),
        Code::FailedPrecondition
    );
    assert_eq!(
        sgr_error_to_status(&SgrError::DependencyTargetNotRegistered(unit_id("b"))).code(),
        Code::InvalidArgument
    );
    assert_eq!(
        sgr_error_to_status(&SgrError::ManifestSignatureInvalid).code(),
        Code::PermissionDenied
    );
}

#[tokio::test]
async fn tonic_in_process_smoke_test_lists_units() -> TestResult {
    let Harness { svc, .. } = harness();
    let (addr, shutdown, handle) = spawn_server(svc).await;

    let response = {
        let mut client = SgrServiceClient::connect(format!("http://{addr}")).await?;
        client
            .list_units(ListUnitsRequest {
                schema_version: SCHEMA_VERSION.to_owned(),
            })
            .await?
            .into_inner()
    };

    assert!(response.units.is_empty());
    let _sent = shutdown.send(());
    handle.await?;
    Ok(())
}

#[tokio::test]
async fn get_graph_state_returns_current_state() -> TestResult {
    let Harness { svc, .. } = harness();

    let response = svc
        .get_graph_state(Request::new(GetGraphStateRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
        }))
        .await?
        .into_inner();

    assert_eq!(graph_state_from_proto(response.state())?, GraphState::Empty);
    Ok(())
}
