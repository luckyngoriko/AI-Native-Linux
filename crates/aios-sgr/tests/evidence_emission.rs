//! T-090 SGR evidence emission tests.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::unwrap_used,
    reason = "Integration-test failures should point at the failing contract"
)]

use std::error::Error;
use std::sync::Arc;

use aios_evidence::RecordType;
use aios_sgr::{
    AdapterActionDeclaration, AdapterCapability, AdapterCapabilityClass, AdapterDeclaration,
    AdapterDispatchKind, AdapterFailureMode, AdapterIOMode, AdapterManifest,
    AdapterRollbackStrategy, AdapterStability, DependencyKind, DesiredState, GpuBudget,
    GraphEvaluator, GraphState, HealthCheckKind, HealthCheckSpec, InMemoryServiceGraph,
    InMemorySgrEvidenceLog, RegisteredAdapter, ResourceBudget, RestartBudget, RestartPolicy,
    RollbackPointer, RollbackTrigger, ServiceGraph, SgrAdapterRegistry, SgrEvidenceEmitter,
    SgrSubjectRef, UnitDependency, UnitFsmDriver, UnitId, UnitKind, UnitManifest, UnitState,
    VerificationIntentRef, AIOS_SGR_SUBJECT,
};
use aios_sgr::{
    AdapterRegisteredPayload, DependencyDeclaredPayload, GraphConvergedPayload, UnitFailedPayload,
    UnitRegisteredPayload, UnitStartedPayload, UnitStoppedPayload,
};
use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const UNIT_AUTHORITY: &str = "pubcat_aiosroot";
const ADAPTER_AUTHORITY: &str = "key_aiosroot_2026q2";

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
    let bytes = serde_json::to_vec(&body)?;
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
        declared_invariants_supported: vec!["INV-013".to_owned(), "INV-017".to_owned()],
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
        publisher_root_id: UNIT_AUTHORITY.to_owned(),
        manifest_signature: Vec::new(),
        signing_key_id: signing_key_id.to_owned(),
        manifest_created_at: fixed_time(),
        manifest_expires_at: Utc
            .with_ymd_and_hms(2026, 8, 25, 10, 0, 0)
            .single()
            .expect("valid datetime"),
    }))
}

fn evidence_fixture() -> (Arc<InMemorySgrEvidenceLog>, Arc<SgrEvidenceEmitter>) {
    let log = Arc::new(InMemorySgrEvidenceLog::new());
    let sink = Arc::clone(&log) as Arc<dyn aios_sgr::SgrEvidenceLog>;
    let emitter = Arc::new(SgrEvidenceEmitter::new(
        sink,
        signing_key(90),
        SgrSubjectRef(AIOS_SGR_SUBJECT.to_owned()),
    ));
    (log, emitter)
}

fn graph_with_evidence(
    sk: &SigningKey,
    emitter: Arc<SgrEvidenceEmitter>,
) -> Arc<InMemoryServiceGraph> {
    Arc::new(
        InMemoryServiceGraph::with_trusted_authority(UNIT_AUTHORITY, sk.verifying_key())
            .with_evidence_emitter(emitter),
    )
}

fn fsm_with_evidence(
    graph: Arc<InMemoryServiceGraph>,
    emitter: Arc<SgrEvidenceEmitter>,
) -> UnitFsmDriver {
    UnitFsmDriver::with_evidence_emitter(graph as Arc<dyn ServiceGraph>, emitter)
}

fn evaluator_with_evidence(
    graph: Arc<InMemoryServiceGraph>,
    emitter: Arc<SgrEvidenceEmitter>,
) -> GraphEvaluator {
    GraphEvaluator::with_evidence_emitter(graph as Arc<dyn ServiceGraph>, emitter)
}

fn registry_with_evidence(sk: &SigningKey, emitter: Arc<SgrEvidenceEmitter>) -> SgrAdapterRegistry {
    SgrAdapterRegistry::with_trusted_authority(ADAPTER_AUTHORITY.to_owned(), sk.verifying_key())
        .with_evidence_emitter(emitter)
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

async fn register_adapter(
    registry: &SgrAdapterRegistry,
    sk: &SigningKey,
    capability_id: &str,
) -> TestResult<RegisteredAdapter> {
    let mut cap = capability(capability_id, &["unit.start", "unit.stop"]);
    sign_capability(&mut cap, sk)?;
    Ok(registry
        .register_adapter(cap, adapter_declaration(ADAPTER_AUTHORITY))
        .await?)
}

fn decode_payload<T>(receipt: &aios_evidence::EvidenceReceipt) -> T
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(receipt.payload().clone()).expect("payload must decode")
}

fn payload_json_has_no_raw_signature_bytes(value: &serde_json::Value) -> bool {
    let text = serde_json::to_string(value).expect("payload serializes");
    !text.contains("publisher_signature")
        && !text.contains("manifest_signature_ed25519")
        && !text.contains('[')
        && !text.contains("signature_bytes")
}

#[tokio::test]
async fn register_unit_emits_unit_registered() -> TestResult {
    let sk = signing_key(10);
    let (log, emitter) = evidence_fixture();
    let graph = graph_with_evidence(&sk, emitter);

    let unit = graph
        .register_unit(signed_manifest("registered", &sk)?)
        .await?;

    let receipts = log.receipts().await;
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].record_type(), RecordType::UnitRegistered);
    let payload: UnitRegisteredPayload = decode_payload(&receipts[0]);
    assert_eq!(payload.unit_id, unit.unit_id);
    assert_eq!(payload.kind, UnitKind::Service);
    assert_eq!(payload.name, "AIOS registered");
    assert!(payload.signing_authority.starts_with(UNIT_AUTHORITY));
    Ok(())
}

#[tokio::test]
async fn declare_dependency_emits_dependency_declared_payload() -> TestResult {
    let sk = signing_key(11);
    let (log, emitter) = evidence_fixture();
    let graph = graph_with_evidence(&sk, emitter);
    let runtime = register_named(&graph, &sk, "runtime").await?;
    let evidence = register_named(&graph, &sk, "evidence").await?;

    let edge = graph
        .declare_dependency(&runtime, &evidence, DependencyKind::RequiresRunning)
        .await?;

    let receipts = log.receipts().await;
    assert_eq!(
        receipts.last().expect("dependency receipt").record_type(),
        RecordType::GraphEvaluated
    );
    let payload: DependencyDeclaredPayload = decode_payload(receipts.last().expect("receipt"));
    assert_eq!(payload.from, edge.from_unit_id);
    assert_eq!(payload.to, edge.to_unit_id);
    assert_eq!(payload.kind, DependencyKind::RequiresRunning);
    Ok(())
}

#[tokio::test]
async fn start_unit_via_fsm_emits_unit_started() -> TestResult {
    let sk = signing_key(12);
    let (log, emitter) = evidence_fixture();
    let graph = graph_with_evidence(&sk, Arc::clone(&emitter));
    let unit_id = register_named(&graph, &sk, "start").await?;
    let fsm = fsm_with_evidence(Arc::clone(&graph), emitter);

    fsm.start(&unit_id).await?;

    let receipts = log.receipts().await;
    assert_eq!(
        receipts.last().expect("start receipt").record_type(),
        RecordType::UnitStarted
    );
    let payload: UnitStartedPayload = decode_payload(receipts.last().expect("receipt"));
    assert_eq!(payload.unit_id, unit_id);
    Ok(())
}

#[tokio::test]
async fn stop_unit_via_fsm_emits_unit_stopped() -> TestResult {
    let sk = signing_key(13);
    let (log, emitter) = evidence_fixture();
    let graph = graph_with_evidence(&sk, Arc::clone(&emitter));
    let unit_id = running_unit(&graph, &sk, "stop").await?;
    let fsm = fsm_with_evidence(Arc::clone(&graph), emitter);

    fsm.stop(&unit_id).await?;

    let receipts = log.receipts().await;
    assert_eq!(
        receipts.last().expect("stop receipt").record_type(),
        RecordType::UnitStopped
    );
    let payload: UnitStoppedPayload = decode_payload(receipts.last().expect("receipt"));
    assert_eq!(payload.unit_id, unit_id);
    assert!(!payload.requested_by_desired_state);
    Ok(())
}

#[tokio::test]
async fn mark_failed_via_fsm_emits_unit_failed() -> TestResult {
    let sk = signing_key(14);
    let (log, emitter) = evidence_fixture();
    let graph = graph_with_evidence(&sk, Arc::clone(&emitter));
    let unit_id = running_unit(&graph, &sk, "failed").await?;
    let fsm = fsm_with_evidence(Arc::clone(&graph), emitter);

    fsm.mark_failed(&unit_id, "adapter refused".to_owned())
        .await?;

    let receipts = log.receipts().await;
    assert_eq!(
        receipts.last().expect("failed receipt").record_type(),
        RecordType::UnitFailed
    );
    let payload: UnitFailedPayload = decode_payload(receipts.last().expect("receipt"));
    assert_eq!(payload.unit_id, unit_id);
    assert_eq!(payload.reason, "adapter refused");
    Ok(())
}

#[tokio::test]
async fn graph_state_changing_to_converged_emits_graph_converged() -> TestResult {
    let sk = signing_key(15);
    let (log, emitter) = evidence_fixture();
    let graph = graph_with_evidence(&sk, Arc::clone(&emitter));
    let unit_id = running_unit(&graph, &sk, "converged").await?;
    assert_eq!(graph.get_unit(&unit_id).await?.state, UnitState::Running);
    let evaluator = evaluator_with_evidence(Arc::clone(&graph), emitter);

    let state = evaluator.convergence_state().await?;

    assert_eq!(state, GraphState::Converged);
    let receipts = log.receipts().await;
    assert_eq!(
        receipts.last().expect("graph receipt").record_type(),
        RecordType::GraphConverged
    );
    let payload: GraphConvergedPayload = decode_payload(receipts.last().expect("receipt"));
    assert_eq!(payload.graph_state, GraphState::Converged);
    assert_eq!(payload.unit_count, 1);
    Ok(())
}

#[tokio::test]
async fn register_adapter_emits_adapter_registered() -> TestResult {
    let sk = signing_key(16);
    let (log, emitter) = evidence_fixture();
    let registry = registry_with_evidence(&sk, emitter);

    let registered = register_adapter(&registry, &sk, "cap_service_lifecycle").await?;

    let receipts = log.receipts().await;
    assert_eq!(receipts[0].record_type(), RecordType::AdapterRegistered);
    let payload: AdapterRegisteredPayload = decode_payload(&receipts[0]);
    assert_eq!(payload.capability_id, registered.capability.capability_id);
    assert!(payload.signing_authority.starts_with(ADAPTER_AUTHORITY));
    Ok(())
}

#[test]
fn all_typed_payloads_round_trip_through_serde_json() -> TestResult {
    let unit_payload = UnitRegisteredPayload {
        unit_id: unit_id("payload"),
        kind: UnitKind::Service,
        name: "AIOS payload".to_owned(),
        signing_authority: "pubcat_aiosroot:sig:01020304".to_owned(),
        registered_at: fixed_time(),
    };
    assert_eq!(
        serde_json::from_value::<UnitRegisteredPayload>(serde_json::to_value(&unit_payload)?)?,
        unit_payload
    );

    let started = UnitStartedPayload {
        unit_id: unit_id("payload"),
        started_at: fixed_time(),
    };
    assert_eq!(
        serde_json::from_value::<UnitStartedPayload>(serde_json::to_value(&started)?)?,
        started
    );

    let stopped = UnitStoppedPayload {
        unit_id: unit_id("payload"),
        stopped_at: fixed_time(),
        requested_by_desired_state: true,
    };
    assert_eq!(
        serde_json::from_value::<UnitStoppedPayload>(serde_json::to_value(&stopped)?)?,
        stopped
    );

    let failed = UnitFailedPayload {
        unit_id: unit_id("payload"),
        reason: "adapter refused".to_owned(),
        failed_at: fixed_time(),
    };
    assert_eq!(
        serde_json::from_value::<UnitFailedPayload>(serde_json::to_value(&failed)?)?,
        failed
    );

    let dependency = DependencyDeclaredPayload {
        from: unit_id("from"),
        to: unit_id("to"),
        kind: DependencyKind::OrdersAfter,
        declared_at: fixed_time(),
    };
    assert_eq!(
        serde_json::from_value::<DependencyDeclaredPayload>(serde_json::to_value(&dependency)?)?,
        dependency
    );

    let graph = GraphConvergedPayload {
        graph_state: GraphState::Converged,
        unit_count: 3,
        converged_at: fixed_time(),
    };
    assert_eq!(
        serde_json::from_value::<GraphConvergedPayload>(serde_json::to_value(&graph)?)?,
        graph
    );

    let adapter = AdapterRegisteredPayload {
        capability_id: "cap_service_lifecycle".to_owned(),
        registered_at: fixed_time(),
        signing_authority: "key_aiosroot_2026q2:sig:01020304".to_owned(),
    };
    assert_eq!(
        serde_json::from_value::<AdapterRegisteredPayload>(serde_json::to_value(&adapter)?)?,
        adapter
    );
    Ok(())
}

#[tokio::test]
async fn blake3_chain_is_coherent_across_three_emissions() -> TestResult {
    let sk = signing_key(17);
    let (log, emitter) = evidence_fixture();
    let unit = {
        let graph = graph_with_evidence(&sk, Arc::clone(&emitter));
        graph.register_unit(signed_manifest("chain", &sk)?).await?
    };

    let first = log.receipts().await[0].receipt_id().as_str().to_owned();
    emitter.emit_unit_started(&unit, Some(&first)).await?;
    let second = log.receipts().await[1].receipt_id().as_str().to_owned();
    emitter.emit_unit_stopped(&unit, Some(&second)).await?;

    let receipts = log.receipts().await;
    assert_eq!(receipts.len(), 3);
    assert_eq!(
        receipts[1].previous_receipt_hash(),
        Some(receipts[0].link_hash()?.as_str())
    );
    assert_eq!(
        receipts[2].previous_receipt_hash(),
        Some(receipts[1].link_hash()?.as_str())
    );
    log.verify_integrity().await?;
    Ok(())
}

#[tokio::test]
async fn inv_018_payloads_do_not_include_raw_signature_bytes() -> TestResult {
    let sk = signing_key(18);
    let (log, emitter) = evidence_fixture();
    let graph = graph_with_evidence(&sk, Arc::clone(&emitter));
    let unit_id = register_named(&graph, &sk, "inv018").await?;
    let fsm = fsm_with_evidence(Arc::clone(&graph), emitter);
    fsm.start(&unit_id).await?;

    let receipts = log.receipts().await;
    assert!(receipts
        .iter()
        .all(|receipt| payload_json_has_no_raw_signature_bytes(receipt.payload())));
    Ok(())
}

#[tokio::test]
async fn no_evidence_emitter_preserves_backward_compatibility() -> TestResult {
    let sk = signing_key(19);
    let graph = Arc::new(InMemoryServiceGraph::with_trusted_authority(
        UNIT_AUTHORITY,
        sk.verifying_key(),
    ));
    let unit_id = register_named(&graph, &sk, "no_emitter").await?;
    let fsm = UnitFsmDriver::new(Arc::clone(&graph) as Arc<dyn ServiceGraph>);

    fsm.start(&unit_id).await?;

    assert_eq!(graph.get_unit(&unit_id).await?.state, UnitState::Running);
    assert!(graph.get_unit(&unit_id).await?.evidence_chain.is_empty());
    Ok(())
}

#[tokio::test]
async fn every_receipt_has_valid_ed25519_signature() -> TestResult {
    let sk = signing_key(20);
    let (log, emitter) = evidence_fixture();
    let graph = graph_with_evidence(&sk, Arc::clone(&emitter));
    let unit_id = register_named(&graph, &sk, "signed").await?;
    let fsm = fsm_with_evidence(Arc::clone(&graph), Arc::clone(&emitter));
    fsm.start(&unit_id).await?;
    fsm.stop(&unit_id).await?;

    let verifying_key = emitter.verifying_key();
    for receipt in log.receipts().await {
        assert!(receipt.signature().is_some());
        receipt.verify_signature(&verifying_key)?;
    }
    log.verify_integrity_signed(&verifying_key).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn concurrent_emission_from_three_tasks_keeps_coherent_chain() -> TestResult {
    let sk = signing_key(21);
    let (log, emitter) = evidence_fixture();
    let unit = {
        let graph = graph_with_evidence(&sk, Arc::clone(&emitter));
        graph
            .register_unit(signed_manifest("concurrent", &sk)?)
            .await?
    };

    let mut handles = Vec::new();
    for idx in 0..3 {
        let emitter = Arc::clone(&emitter);
        let unit = unit.clone();
        handles.push(tokio::spawn(async move {
            match idx {
                0 => emitter.emit_unit_started(&unit, None).await,
                1 => emitter.emit_unit_stopped(&unit, None).await,
                _ => {
                    emitter
                        .emit_unit_failed(&unit, "concurrent failure", None)
                        .await
                }
            }
        }));
    }

    for handle in handles {
        handle.await??;
    }

    let receipts = log.receipts().await;
    assert_eq!(receipts.len(), 4);
    for pair in receipts.windows(2) {
        assert_eq!(
            pair[1].previous_receipt_hash(),
            Some(pair[0].link_hash()?.as_str())
        );
    }
    log.verify_integrity().await?;
    Ok(())
}

#[tokio::test]
async fn end_to_end_register_three_units_declare_two_dependencies_start_all_orders_receipts(
) -> TestResult {
    let sk = signing_key(22);
    let (log, emitter) = evidence_fixture();
    let graph = graph_with_evidence(&sk, Arc::clone(&emitter));
    let fsm = fsm_with_evidence(Arc::clone(&graph), emitter);

    let evidence = register_named(&graph, &sk, "evidence").await?;
    let policy = register_named(&graph, &sk, "policy").await?;
    let runtime = register_named(&graph, &sk, "runtime").await?;
    graph
        .declare_dependency(&policy, &evidence, DependencyKind::RequiresRunning)
        .await?;
    graph
        .declare_dependency(&runtime, &policy, DependencyKind::RequiresRunning)
        .await?;
    fsm.start(&evidence).await?;
    fsm.start(&policy).await?;
    fsm.start(&runtime).await?;

    let record_types = log
        .receipts()
        .await
        .iter()
        .map(aios_evidence::EvidenceReceipt::record_type)
        .collect::<Vec<_>>();
    assert_eq!(
        record_types,
        vec![
            RecordType::UnitRegistered,
            RecordType::UnitRegistered,
            RecordType::UnitRegistered,
            RecordType::GraphEvaluated,
            RecordType::GraphEvaluated,
            RecordType::UnitStarted,
            RecordType::UnitStarted,
            RecordType::UnitStarted,
        ]
    );
    Ok(())
}
