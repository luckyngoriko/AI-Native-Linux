//! T-091 cross-crate integration tests.

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

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;

use aios_action::{ActionEnvelope, ActionId};
use aios_capability_runtime::{
    ActionContext, ActionDispatchKind, ActionLifecycleState, CapabilityRuntime, EvidenceEmitter,
    ExecutionFailureReason, InMemoryCapabilityRuntime, InMemoryEvidenceSink, QueueClass,
    RuntimeContext, RuntimeError,
};
use aios_evidence::RecordType;
use aios_recovery::{BootPhase, EnterRecoveryRequest, InMemoryRecoveryBoundary, RecoveryBoundary};
use aios_sgr::{
    DefaultUnitActionFactory, DependencyKind, DesiredState, GpuBudget, GraphEvaluator,
    HealthCheckKind, HealthCheckSpec, InMemoryServiceGraph, InMemorySgrEvidenceLog, ResourceBudget,
    RestartBudget, RestartPolicy, RollbackPointer, RollbackTrigger, ServiceGraph, ServiceUnit,
    SgrCapabilityAdapter, SgrError, SgrEvidenceEmitter, SgrRecoveryHook, SgrSubjectRef,
    UnitActionFactory, UnitDependency, UnitFsmDriver, UnitId, UnitKind, UnitManifest, UnitState,
    VerificationIntentRef, AIOS_SGR_SUBJECT,
};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const UNIT_AUTHORITY: &str = "pubcat_aiosroot_t091";
const ADAPTER_ID: &str = "adapter:aiosroot:systemd:1.0.0";

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

fn unsigned_manifest(
    name: &str,
    recovery_mode_allowed: bool,
    adapter_id: Option<&str>,
) -> UnitManifest {
    UnitManifest {
        schema_version: "aios.unit.v1alpha1".to_owned(),
        unit_id: unit_id(name),
        unit_kind: UnitKind::Service,
        display_name: format!("AIOS {name}"),
        description: "T-091 integration unit.".to_owned(),
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
        labels: Some(serde_json::json!({
            "layer": "L3",
            "criticality": "standard",
            "recovery_mode_allowed": recovery_mode_allowed,
        })),
        correlation_id: None,
        desired_state: DesiredState::Running,
        provides: vec![format!("service.{name}")],
        adapter_id: adapter_id.map(str::to_owned),
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

fn sign_manifest(manifest: &mut UnitManifest, sk: &SigningKey) -> TestResult {
    let body = SignedUnitManifestBody::from(&*manifest);
    let bytes = serde_json::to_vec(&body)?;
    let digest = blake3::hash(&bytes);
    let digest_hex = digest.to_hex().to_string();
    digest_hex[..32].clone_into(&mut manifest.canonical_hash);
    manifest.publisher_signature = sk.sign(digest.as_bytes()).to_bytes().to_vec();
    Ok(())
}

fn signed_manifest(
    name: &str,
    sk: &SigningKey,
    recovery_mode_allowed: bool,
    adapter_id: Option<&str>,
) -> TestResult<UnitManifest> {
    let mut manifest = unsigned_manifest(name, recovery_mode_allowed, adapter_id);
    sign_manifest(&mut manifest, sk)?;
    Ok(manifest)
}

fn trusted_graph(sk: &SigningKey) -> Arc<InMemoryServiceGraph> {
    Arc::new(InMemoryServiceGraph::with_trusted_authority(
        UNIT_AUTHORITY,
        sk.verifying_key(),
    ))
}

fn evidence_fixture() -> (Arc<InMemorySgrEvidenceLog>, Arc<SgrEvidenceEmitter>) {
    let log = Arc::new(InMemorySgrEvidenceLog::new());
    let emitter = Arc::new(SgrEvidenceEmitter::new(
        log.clone(),
        signing_key(91),
        SgrSubjectRef(AIOS_SGR_SUBJECT.to_owned()),
    ));
    (log, emitter)
}

fn trusted_graph_with_evidence(
    sk: &SigningKey,
    emitter: Arc<SgrEvidenceEmitter>,
) -> Arc<InMemoryServiceGraph> {
    Arc::new(
        InMemoryServiceGraph::with_trusted_authority(UNIT_AUTHORITY, sk.verifying_key())
            .with_evidence_emitter(emitter),
    )
}

fn fsm_for(graph: Arc<InMemoryServiceGraph>) -> Arc<UnitFsmDriver> {
    Arc::new(UnitFsmDriver::new(graph as Arc<dyn ServiceGraph>))
}

fn fsm_with_evidence(
    graph: Arc<InMemoryServiceGraph>,
    emitter: Arc<SgrEvidenceEmitter>,
) -> Arc<UnitFsmDriver> {
    Arc::new(UnitFsmDriver::with_evidence_emitter(
        graph as Arc<dyn ServiceGraph>,
        emitter,
    ))
}

fn adapter_for(
    graph: Arc<InMemoryServiceGraph>,
    fsm: Arc<UnitFsmDriver>,
    runtime: Arc<dyn CapabilityRuntime>,
) -> SgrCapabilityAdapter {
    SgrCapabilityAdapter::new(
        graph as Arc<dyn ServiceGraph>,
        fsm,
        runtime,
        Arc::new(DefaultUnitActionFactory),
    )
}

async fn register_named(
    graph: &InMemoryServiceGraph,
    sk: &SigningKey,
    name: &str,
) -> TestResult<UnitId> {
    let manifest = signed_manifest(name, sk, false, Some(ADAPTER_ID))?;
    let id = manifest.unit_id.clone();
    graph.register_unit(manifest).await?;
    Ok(id)
}

async fn running_unit(
    graph: &InMemoryServiceGraph,
    sk: &SigningKey,
    name: &str,
    recovery_mode_allowed: bool,
) -> TestResult<UnitId> {
    let manifest = signed_manifest(name, sk, recovery_mode_allowed, Some(ADAPTER_ID))?;
    let id = manifest.unit_id.clone();
    graph.register_unit(manifest).await?;
    graph.set_unit_state(&id, UnitState::Starting).await?;
    graph.set_unit_state(&id, UnitState::Running).await?;
    Ok(id)
}

fn enter_request() -> EnterRecoveryRequest {
    EnterRecoveryRequest {
        reason: "OPERATOR_INITIATED".to_owned(),
        operator_grant: Some("grant_t091".to_owned()),
        expected_phases: vec![BootPhase::Recovery],
        bundle: None,
    }
}

async fn enter_recovery(boundary: &InMemoryRecoveryBoundary) -> TestResult {
    boundary.enter_recovery(enter_request()).await?;
    Ok(())
}

async fn exit_recovery(boundary: &InMemoryRecoveryBoundary) -> TestResult {
    let token = boundary
        .current_exit_token()
        .await
        .ok_or("missing active recovery exit token")?;
    boundary.exit_recovery(&token).await?;
    Ok(())
}

fn runtime_context() -> RuntimeContext {
    RuntimeContext::new("_system:sgr", "polb_t091", "code_t091")
}

#[derive(Debug)]
struct FixedRuntime {
    status: ActionLifecycleState,
    error: Option<ExecutionFailureReason>,
}

impl FixedRuntime {
    const fn succeeded() -> Self {
        Self {
            status: ActionLifecycleState::Succeeded,
            error: None,
        }
    }

    const fn failed() -> Self {
        Self {
            status: ActionLifecycleState::Failed,
            error: Some(ExecutionFailureReason::AdapterRefused),
        }
    }

    fn context_for(&self, action_id: ActionId) -> ActionContext {
        let now = Utc::now();
        let mut ctx = ActionContext::new(
            action_id,
            ActionDispatchKind::DryRun,
            QueueClass::Interactive,
            now,
        );
        ctx.status = self.status;
        ctx.error = self.error;
        ctx
    }
}

#[async_trait]
impl CapabilityRuntime for FixedRuntime {
    async fn submit_action(
        &self,
        _envelope: &ActionEnvelope,
        _context: &RuntimeContext,
    ) -> Result<ActionContext, RuntimeError> {
        Ok(self.context_for(ActionId::new()))
    }

    async fn get_action_status(&self, action_id: &ActionId) -> Result<ActionContext, RuntimeError> {
        Ok(self.context_for(action_id.clone()))
    }
}

#[derive(Debug)]
struct FailingFactory;

impl UnitActionFactory for FailingFactory {
    fn build_action(&self, _unit: &ServiceUnit) -> Result<ActionEnvelope, SgrError> {
        Err(SgrError::Internal("factory refused".to_owned()))
    }
}

#[tokio::test]
async fn start_unit_via_runtime_happy_path_succeeds_and_unit_runs() -> TestResult {
    let sk = signing_key(1);
    let graph = trusted_graph(&sk);
    let unit_id = register_named(&graph, &sk, "start_happy").await?;
    let fsm = fsm_for(graph.clone());
    let runtime: Arc<dyn CapabilityRuntime> = Arc::new(FixedRuntime::succeeded());
    let adapter = adapter_for(graph.clone(), fsm, runtime);

    let ctx = adapter.start_unit_via_runtime(&unit_id).await?;

    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    assert_eq!(graph.get_unit(&unit_id).await?.state, UnitState::Running);
    Ok(())
}

#[tokio::test]
async fn start_unit_via_runtime_runtime_denial_marks_unit_failed() -> TestResult {
    let sk = signing_key(2);
    let graph = trusted_graph(&sk);
    let unit_id = register_named(&graph, &sk, "start_denied").await?;
    let fsm = fsm_for(graph.clone());
    let runtime: Arc<dyn CapabilityRuntime> = Arc::new(FixedRuntime::failed());
    let adapter = adapter_for(graph.clone(), fsm, runtime);

    let ctx = adapter.start_unit_via_runtime(&unit_id).await?;

    assert_eq!(ctx.status, ActionLifecycleState::Failed);
    assert_eq!(ctx.error, Some(ExecutionFailureReason::AdapterRefused));
    assert_eq!(graph.get_unit(&unit_id).await?.state, UnitState::Failed);
    Ok(())
}

#[tokio::test]
async fn default_unit_action_factory_builds_valid_envelope_from_manifest() -> TestResult {
    let sk = signing_key(3);
    let manifest = signed_manifest("factory_valid", &sk, false, Some(ADAPTER_ID))?;
    let unit = ServiceUnit {
        unit_id: manifest.unit_id.clone(),
        manifest,
        state: UnitState::Queued,
        last_transition_at: fixed_time(),
        evidence_chain: Vec::new(),
    };

    let envelope = DefaultUnitActionFactory.build_action(&unit)?;

    assert_eq!(envelope.identity.subject_canonical_id, "_system:sgr");
    assert_eq!(envelope.request.action, "unit.start");
    assert_eq!(envelope.request.target["unit_id"], unit.unit_id.as_str());
    assert_eq!(envelope.request.target["adapter_id"], ADAPTER_ID);
    assert_eq!(
        envelope.request.target["adapter_target"]["systemd_unit"],
        "factory_valid.service"
    );
    Ok(())
}

#[tokio::test]
async fn default_unit_action_factory_without_adapter_id_returns_capability_mismatch() -> TestResult
{
    let sk = signing_key(4);
    let manifest = signed_manifest("factory_missing_adapter", &sk, false, None)?;
    let unit = ServiceUnit {
        unit_id: manifest.unit_id.clone(),
        manifest,
        state: UnitState::Queued,
        last_transition_at: fixed_time(),
        evidence_chain: Vec::new(),
    };

    let err = DefaultUnitActionFactory
        .build_action(&unit)
        .expect_err("missing adapter_id must fail");

    assert!(matches!(err, SgrError::AdapterCapabilityMismatch { .. }));
    Ok(())
}

#[tokio::test]
async fn recovery_hook_trait_reports_boundary_state() -> TestResult {
    let sk = signing_key(5);
    let graph = trusted_graph(&sk);
    let fsm = fsm_for(graph.clone());
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let hook = SgrRecoveryHook::new(
        graph as Arc<dyn ServiceGraph>,
        fsm,
        boundary.clone() as Arc<dyn RecoveryBoundary>,
    );

    assert!(!hook.current_recovery_mode().await);
    enter_recovery(&boundary).await?;
    assert!(hook.current_recovery_mode().await);
    Ok(())
}

#[tokio::test]
async fn entering_recovery_pause_normal_units_stops_running_units() -> TestResult {
    let sk = signing_key(6);
    let graph = trusted_graph(&sk);
    let normal = running_unit(&graph, &sk, "normal_pause", false).await?;
    let fsm = fsm_for(graph.clone());
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let hook = SgrRecoveryHook::new(
        graph.clone() as Arc<dyn ServiceGraph>,
        fsm,
        boundary.clone() as Arc<dyn RecoveryBoundary>,
    );

    enter_recovery(&boundary).await?;
    hook.pause_normal_units().await?;

    assert_eq!(graph.get_unit(&normal).await?.state, UnitState::Stopped);
    Ok(())
}

#[tokio::test]
async fn exiting_recovery_resume_normal_units_restarts_paused_units() -> TestResult {
    let sk = signing_key(7);
    let graph = trusted_graph(&sk);
    let normal = running_unit(&graph, &sk, "normal_resume", false).await?;
    let fsm = fsm_for(graph.clone());
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let hook = SgrRecoveryHook::new(
        graph.clone() as Arc<dyn ServiceGraph>,
        fsm,
        boundary.clone() as Arc<dyn RecoveryBoundary>,
    );

    enter_recovery(&boundary).await?;
    hook.pause_normal_units().await?;
    exit_recovery(&boundary).await?;
    hook.resume_normal_units().await?;

    assert_eq!(graph.get_unit(&normal).await?.state, UnitState::Running);
    Ok(())
}

#[tokio::test]
async fn recovery_mode_allowed_units_stay_running_through_recovery_cycle() -> TestResult {
    let sk = signing_key(8);
    let graph = trusted_graph(&sk);
    let allowed = running_unit(&graph, &sk, "recovery_allowed", true).await?;
    let normal = running_unit(&graph, &sk, "recovery_normal", false).await?;
    let fsm = fsm_for(graph.clone());
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let hook = SgrRecoveryHook::new(
        graph.clone() as Arc<dyn ServiceGraph>,
        fsm,
        boundary.clone() as Arc<dyn RecoveryBoundary>,
    );

    enter_recovery(&boundary).await?;
    hook.pause_normal_units().await?;
    assert_eq!(graph.get_unit(&allowed).await?.state, UnitState::Running);
    assert_eq!(graph.get_unit(&normal).await?.state, UnitState::Stopped);
    exit_recovery(&boundary).await?;
    hook.resume_normal_units().await?;

    assert_eq!(graph.get_unit(&allowed).await?.state, UnitState::Running);
    assert_eq!(graph.get_unit(&normal).await?.state, UnitState::Running);
    Ok(())
}

#[tokio::test]
async fn backward_compat_capability_runtime_plain_submit_still_succeeds() -> TestResult {
    let runtime = InMemoryCapabilityRuntime::new();
    let sk = signing_key(9);
    let manifest = signed_manifest("runtime_compat", &sk, false, Some(ADAPTER_ID))?;
    let unit = ServiceUnit {
        unit_id: manifest.unit_id.clone(),
        manifest,
        state: UnitState::Queued,
        last_transition_at: fixed_time(),
        evidence_chain: Vec::new(),
    };
    let envelope = DefaultUnitActionFactory.build_action(&unit)?;

    let ctx = runtime.submit_action(&envelope, &runtime_context()).await?;

    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    Ok(())
}

#[tokio::test]
async fn backward_compat_recovery_boundary_enter_exit_still_roundtrips() -> TestResult {
    let boundary = InMemoryRecoveryBoundary::new();

    enter_recovery(&boundary).await?;
    assert!(boundary.is_recovery_active().await);
    exit_recovery(&boundary).await?;

    assert!(!boundary.is_recovery_active().await);
    Ok(())
}

#[tokio::test]
async fn end_to_end_start_via_runtime_records_sgr_and_runtime_evidence() -> TestResult {
    let sk = signing_key(10);
    let (sgr_log, sgr_emitter) = evidence_fixture();
    let graph = trusted_graph_with_evidence(&sk, sgr_emitter.clone());
    let dependency = register_named(&graph, &sk, "runtime_dep").await?;
    graph
        .set_unit_state(&dependency, UnitState::Starting)
        .await?;
    graph
        .set_unit_state(&dependency, UnitState::Running)
        .await?;
    let app = register_named(&graph, &sk, "runtime_app").await?;
    graph
        .declare_dependency(&app, &dependency, DependencyKind::RequiresRunning)
        .await?;
    let evaluator = GraphEvaluator::new(graph.clone() as Arc<dyn ServiceGraph>);
    assert!(evaluator.evaluate_readiness(&app).await?);

    let runtime_sink = Arc::new(InMemoryEvidenceSink::new(signing_key(11)));
    let runtime_emitter = Arc::new(EvidenceEmitter::new(runtime_sink.clone()));
    let runtime: Arc<dyn CapabilityRuntime> =
        Arc::new(InMemoryCapabilityRuntime::new().with_evidence_emitter(runtime_emitter));
    let fsm = fsm_with_evidence(graph.clone(), sgr_emitter);
    let adapter = adapter_for(graph.clone(), fsm, runtime);

    let ctx = adapter.start_unit_via_runtime(&app).await?;

    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    assert_eq!(graph.get_unit(&app).await?.state, UnitState::Running);
    assert!(!ctx.evidence_chain.is_empty());

    let sgr_types = sgr_log
        .receipts()
        .await
        .iter()
        .map(aios_evidence::EvidenceReceipt::record_type)
        .collect::<Vec<_>>();
    assert!(sgr_types.contains(&RecordType::UnitRegistered));
    assert!(sgr_types.contains(&RecordType::GraphEvaluated));
    assert!(sgr_types.contains(&RecordType::UnitStarted));

    let runtime_receipts = runtime_sink.receipts().await;
    let runtime_types = runtime_receipts
        .iter()
        .map(aios_evidence::EvidenceReceipt::record_type)
        .collect::<Vec<_>>();
    assert!(runtime_types.contains(&RecordType::ActionReceived));
    assert!(runtime_types.contains(&RecordType::ExecutionStarted));
    assert!(runtime_types.contains(&RecordType::ExecutionCompleted));
    assert!(runtime_types.contains(&RecordType::VerificationResult));
    assert!(runtime_receipts
        .iter()
        .all(|receipt| receipt.receipt_id().as_str().starts_with("evr_")));
    Ok(())
}

#[tokio::test]
async fn concurrent_start_unit_via_runtime_from_three_tasks_completes() -> TestResult {
    let sk = signing_key(12);
    let graph = trusted_graph(&sk);
    let ids = [
        register_named(&graph, &sk, "worker_0").await?,
        register_named(&graph, &sk, "worker_1").await?,
        register_named(&graph, &sk, "worker_2").await?,
    ];
    let fsm = fsm_for(graph.clone());
    let runtime: Arc<dyn CapabilityRuntime> = Arc::new(FixedRuntime::succeeded());
    let adapter = Arc::new(adapter_for(graph.clone(), fsm, runtime));

    let ids_for_tasks = ids.clone();
    let handles = ids_for_tasks
        .into_iter()
        .map(|unit_id| {
            let adapter = Arc::clone(&adapter);
            tokio::spawn(async move { adapter.start_unit_via_runtime(&unit_id).await })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        let ctx = handle.await??;
        assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    }
    for unit_id in ids {
        assert_eq!(graph.get_unit(&unit_id).await?.state, UnitState::Running);
    }
    Ok(())
}

#[tokio::test]
async fn action_factory_error_marks_unit_failed_with_reason() -> TestResult {
    let sk = signing_key(13);
    let graph = trusted_graph(&sk);
    let unit_id = register_named(&graph, &sk, "factory_error").await?;
    let fsm = fsm_for(graph.clone());
    let runtime: Arc<dyn CapabilityRuntime> = Arc::new(FixedRuntime::succeeded());
    let adapter = SgrCapabilityAdapter::new(
        graph.clone() as Arc<dyn ServiceGraph>,
        fsm,
        runtime,
        Arc::new(FailingFactory),
    );

    let err = adapter
        .start_unit_via_runtime(&unit_id)
        .await
        .expect_err("factory error must surface");

    assert!(matches!(err, SgrError::Internal(_)));
    assert_eq!(graph.get_unit(&unit_id).await?.state, UnitState::Failed);
    Ok(())
}

#[tokio::test]
async fn unit_with_no_adapter_id_start_returns_capability_mismatch() -> TestResult {
    let sk = signing_key(14);
    let graph = trusted_graph(&sk);
    let manifest = signed_manifest("missing_adapter", &sk, false, None)?;
    let unit_id = manifest.unit_id.clone();
    graph.register_unit(manifest).await?;
    let fsm = fsm_for(graph.clone());
    let runtime: Arc<dyn CapabilityRuntime> = Arc::new(FixedRuntime::succeeded());
    let adapter = adapter_for(graph.clone(), fsm, runtime);

    let err = adapter
        .start_unit_via_runtime(&unit_id)
        .await
        .expect_err("missing adapter_id must fail");

    assert!(matches!(err, SgrError::AdapterCapabilityMismatch { .. }));
    assert_eq!(graph.get_unit(&unit_id).await?.state, UnitState::Failed);
    Ok(())
}

#[tokio::test]
async fn stop_unit_via_runtime_happy_path_transitions_to_stopped() -> TestResult {
    let sk = signing_key(15);
    let graph = trusted_graph(&sk);
    let unit_id = running_unit(&graph, &sk, "stop_happy", false).await?;
    let fsm = fsm_for(graph.clone());
    let runtime: Arc<dyn CapabilityRuntime> = Arc::new(FixedRuntime::succeeded());
    let adapter = adapter_for(graph.clone(), fsm, runtime);

    let ctx = adapter.stop_unit_via_runtime(&unit_id).await?;

    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    assert_eq!(graph.get_unit(&unit_id).await?.state, UnitState::Stopped);
    Ok(())
}
