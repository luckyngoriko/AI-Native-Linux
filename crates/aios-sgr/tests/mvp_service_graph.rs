//! T-093 M10 closure: end-to-end SGR scenarios over the M3-M9 stack.

#![allow(
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::panic,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::unwrap_used,
    reason = "closure integration fixtures fail loudly on contract drift"
)]

use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, TimeZone, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;

use aios_action::{ActionEnvelope, ActionId};
use aios_capability_runtime::adapter_manifest::AdapterActionDeclaration as RuntimeAdapterActionDeclaration;
use aios_capability_runtime::{
    canonical_signed_manifest_bytes, encode_hex_signature, ActionDispatchKind,
    AdapterIOMode as RuntimeAdapterIOMode, AdapterManifest as RuntimeAdapterManifest,
    AdapterStability as RuntimeAdapterStability, CapabilityRuntime, DispatchQueue, EvidenceEmitter,
    InMemoryAdapterRegistry, InMemoryApprovalSink, InMemoryCapabilityRuntime, InMemoryEvidenceSink,
    RuntimeRecoveryHook,
};
use aios_fs::InMemoryAiosFs;
use aios_policy::{
    ApprovalRequirement, ApprovalScope, ApproverClass, Constraints, Decision, PolicyContext,
    PolicyDecision, PolicyError, PolicyKernel,
};
use aios_recovery::{BootPhase, EnterRecoveryRequest, InMemoryRecoveryBoundary, RecoveryBoundary};
use aios_vault::InMemoryVaultBroker;
use aios_verification::{InMemoryVerificationEngine, VerificationRuntimeAdapter};

use aios_sgr::{
    AdapterActionDeclaration, AdapterCapability, AdapterCapabilityClass, AdapterDeclaration,
    AdapterDispatchKind, AdapterFailureMode, AdapterIOMode, AdapterManifest,
    AdapterRollbackStrategy, AdapterStability, DependencyKind, DesiredState, GpuBudget,
    GraphEvaluator, HealthCheckKind, HealthCheckSpec, InMemoryServiceGraph, InMemorySgrEvidenceLog,
    ResourceBudget, RestartBudget, RestartPolicy, RollbackPointer, RollbackTrigger, ServiceGraph,
    SgrAdapterRegistry, SgrCapabilityAdapter, SgrError, SgrEvidenceEmitter, SgrRecoveryHook,
    SgrSubjectRef, UnitDependency, UnitFsmDriver, UnitId, UnitKind, UnitManifest, UnitState,
    VerificationIntentRef, AIOS_SGR_SUBJECT,
};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const UNIT_AUTHORITY: &str = "pubcat_aiosroot_t093";
const SGR_ADAPTER_AUTHORITY: &str = "key_aiosroot_sgr_t093";
const RUNTIME_ADAPTER_AUTHORITY: &str = "publisher:aios-sgr-runtime-adapter:t093";
const ADAPTER_ID: &str = "adapter:aios:systemd:0.1.0";

struct Stack {
    graph: Arc<InMemoryServiceGraph>,
    fsm: Arc<UnitFsmDriver>,
    evaluator: Arc<GraphEvaluator>,
    sgr_registry: Arc<SgrAdapterRegistry>,
    capability_adapter: SgrCapabilityAdapter,
    recovery_hook: Arc<SgrRecoveryHook>,
    recovery_boundary: Arc<InMemoryRecoveryBoundary>,
    _runtime_evidence_sink: Arc<InMemoryEvidenceSink>,
    _sgr_evidence_log: Arc<InMemorySgrEvidenceLog>,
    _fs: Arc<InMemoryAiosFs>,
    _vault: Arc<InMemoryVaultBroker>,
    _verification_engine: Arc<InMemoryVerificationEngine>,
}

#[derive(Debug)]
struct AllowKernel;

#[async_trait]
impl PolicyKernel for AllowKernel {
    async fn evaluate_policy(
        &self,
        _envelope: &ActionEnvelope,
        _context: &PolicyContext,
    ) -> Result<PolicyDecision, PolicyError> {
        Ok(PolicyDecision {
            policy_decision_id: "poldec_t093_sgr_allow".to_owned(),
            action_id: ActionId::new(),
            request_hash: "0".repeat(32),
            bundle_version: "polb_t093_sgr".to_owned(),
            enrichment_snapshot_id: "snap_t093_sgr".to_owned(),
            decision: Decision::Allow,
            reason_code: "T093Allow".to_owned(),
            reason_message: "T-093 SGR integration policy allow".to_owned(),
            constraints: Constraints::default(),
            approval: ApprovalRequirement {
                required: false,
                approval_scope: ApprovalScope::ExactRequestHash,
                ttl_seconds: 0,
                approver_classes: vec![ApproverClass::Human],
                require_human_co_signer: false,
            },
            evidence_receipt_id: "evr_t093_policy_allow".to_owned(),
            evaluated_at: Utc::now(),
            rules_consulted: 1,
            simulated: false,
        })
    }
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

fn unit_action(action: &str) -> RuntimeAdapterActionDeclaration {
    RuntimeAdapterActionDeclaration {
        action_kind: action.to_owned(),
        target_schema: serde_json::json!({ "type": "object" }),
        response_schema: serde_json::json!({ "type": "object" }),
        rollback_strategy: "IDEMPOTENT_REVERSE".to_owned(),
        timeout_seconds: 30,
        template_string: None,
        template_substitution_variables: Vec::new(),
    }
}

fn unsigned_runtime_adapter_manifest() -> RuntimeAdapterManifest {
    let now = Utc::now();
    RuntimeAdapterManifest {
        adapter_id: ADAPTER_ID.to_owned(),
        adapter_version: "0.1.0".to_owned(),
        vendor: "aios".to_owned(),
        name: "systemd".to_owned(),
        declared_stability: RuntimeAdapterStability::Stable,
        io_mode: RuntimeAdapterIOMode::TypedParametersOnly,
        dispatch_kind: ActionDispatchKind::SubprocessFork,
        declared_actions: vec![unit_action("unit.start"), unit_action("unit.stop")],
        declared_invariants_supported: vec!["INV-013".to_owned(), "INV-014".to_owned()],
        default_adapter_timeout_seconds: 60,
        default_sandbox_profile_id: "sgr-service-lifecycle".to_owned(),
        adapter_signature: String::new(),
        signing_key_id: RUNTIME_ADAPTER_AUTHORITY.to_owned(),
        manifest_created_at: now,
        manifest_expires_at: now + Duration::days(365),
    }
}

fn sign_runtime_adapter_manifest(manifest: &mut RuntimeAdapterManifest, sk: &SigningKey) {
    let body = canonical_signed_manifest_bytes(manifest).expect("runtime adapter signed body");
    let sig = sk.sign(&body);
    manifest.adapter_signature = encode_hex_signature(&sig.to_bytes());
}

async fn runtime_adapter_registry(sk: &SigningKey) -> TestResult<Arc<InMemoryAdapterRegistry>> {
    let mut trusted = HashMap::new();
    trusted.insert(RUNTIME_ADAPTER_AUTHORITY.to_owned(), sk.verifying_key());
    let registry = Arc::new(InMemoryAdapterRegistry::new(trusted));
    let mut manifest = unsigned_runtime_adapter_manifest();
    sign_runtime_adapter_manifest(&mut manifest, sk);
    registry.register(manifest, Utc::now()).await?;
    Ok(registry)
}

fn sgr_action(action: &str) -> AdapterActionDeclaration {
    AdapterActionDeclaration {
        action_kind: action.to_owned(),
        target_schema: serde_json::json!({ "type": "object" }),
        response_schema: serde_json::json!({ "type": "object" }),
        rollback_strategy: AdapterRollbackStrategy::IdempotentReverse,
        timeout_seconds: 30,
        template_string: None,
        template_substitution_variables: Vec::new(),
        per_action_capabilities: vec![AdapterCapabilityClass::ServiceLifecycle],
    }
}

fn sgr_adapter_declaration() -> AdapterDeclaration {
    AdapterDeclaration::Manifest(Box::new(AdapterManifest {
        adapter_id: ADAPTER_ID.to_owned(),
        vendor: "aios".to_owned(),
        name: "systemd".to_owned(),
        adapter_version: "0.1.0".to_owned(),
        spec_version: "v1alpha1".to_owned(),
        declared_actions: vec![sgr_action("unit.start"), sgr_action("unit.stop")],
        declared_capabilities: vec![AdapterCapabilityClass::ServiceLifecycle],
        declared_invariants_supported: vec!["INV-013".to_owned(), "INV-014".to_owned()],
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
        signing_key_id: SGR_ADAPTER_AUTHORITY.to_owned(),
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

fn sign_sgr_capability(capability: &mut AdapterCapability, sk: &SigningKey) -> TestResult {
    let body = SignedCapabilityBody::from(&*capability);
    let bytes = serde_json::to_vec(&body)?;
    capability.manifest_signature_ed25519 = sk.sign(&bytes).to_bytes().to_vec();
    Ok(())
}

async fn sgr_adapter_registry(sk: &SigningKey) -> TestResult<Arc<SgrAdapterRegistry>> {
    let registry = Arc::new(SgrAdapterRegistry::with_trusted_authority(
        SGR_ADAPTER_AUTHORITY.to_owned(),
        sk.verifying_key(),
    ));
    let mut capability = AdapterCapability {
        capability_id: "cap_sgr_service_lifecycle".to_owned(),
        provides: vec!["unit.start".to_owned(), "unit.stop".to_owned()],
        requires: vec!["SERVICE_LIFECYCLE".to_owned()],
        risk_template: "REQUIRE_APPROVAL".to_owned(),
        manifest_signature_ed25519: Vec::new(),
    };
    sign_sgr_capability(&mut capability, sk)?;
    registry
        .register_adapter(capability, sgr_adapter_declaration())
        .await?;
    Ok(registry)
}

async fn build_stack(unit_sk: &SigningKey) -> TestResult<Stack> {
    let (runtime_sink, runtime_emitter) = {
        let sink = Arc::new(InMemoryEvidenceSink::new(signing_key(71)));
        let emitter = Arc::new(EvidenceEmitter::new(sink.clone()));
        (sink, emitter)
    };
    let sgr_log = Arc::new(InMemorySgrEvidenceLog::new());
    let sgr_emitter = Arc::new(SgrEvidenceEmitter::new(
        sgr_log.clone(),
        signing_key(72),
        SgrSubjectRef(AIOS_SGR_SUBJECT.to_owned()),
    ));
    let graph = Arc::new(
        InMemoryServiceGraph::with_trusted_authority(UNIT_AUTHORITY, unit_sk.verifying_key())
            .with_evidence_emitter(sgr_emitter),
    );
    let graph_dyn: Arc<dyn ServiceGraph> = graph.clone();
    let fsm = Arc::new(UnitFsmDriver::new(graph_dyn.clone()));
    let evaluator = Arc::new(GraphEvaluator::new(graph_dyn.clone()));
    let runtime_registry = runtime_adapter_registry(&signing_key(73)).await?;
    let sgr_registry = sgr_adapter_registry(&signing_key(74)).await?;
    let fs = Arc::new(InMemoryAiosFs::new());
    let vault = Arc::new(InMemoryVaultBroker::new());
    let verification_engine = Arc::new(InMemoryVerificationEngine::new());
    let verification_adapter =
        Arc::new(VerificationRuntimeAdapter::new(verification_engine.clone()));
    let recovery_boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let recovery_hook = Arc::new(SgrRecoveryHook::new(
        graph_dyn.clone(),
        fsm.clone(),
        recovery_boundary.clone() as Arc<dyn RecoveryBoundary>,
    ));
    let runtime_hook: Arc<dyn RuntimeRecoveryHook> = recovery_hook.clone();
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(Arc::new(AllowKernel))
        .with_adapter_registry(runtime_registry)
        .with_dispatch_queue(Arc::new(DispatchQueue::new_with_defaults()))
        .with_evidence_emitter(runtime_emitter)
        .with_verification_engine(verification_adapter)
        .with_recovery_hook(runtime_hook)
        .with_approval_sink(Arc::new(InMemoryApprovalSink::new()));

    assert!(runtime.policy_kernel().is_some());
    assert!(runtime.adapter_registry().is_some());
    assert!(runtime.dispatch_queue().is_some());
    assert!(runtime.evidence_emitter().is_some());
    assert!(runtime.approval_sink().is_some());
    assert!(fs.snapshot().object_count == 0);

    let capability_adapter = SgrCapabilityAdapter::with_default_factory(
        graph_dyn,
        fsm.clone(),
        Arc::new(runtime) as Arc<dyn CapabilityRuntime>,
    );

    Ok(Stack {
        graph,
        fsm,
        evaluator,
        sgr_registry,
        capability_adapter,
        recovery_hook,
        recovery_boundary,
        _runtime_evidence_sink: runtime_sink,
        _sgr_evidence_log: sgr_log,
        _fs: fs,
        _vault: vault,
        _verification_engine: verification_engine,
    })
}

fn unsigned_manifest(
    name: &str,
    unit_kind: UnitKind,
    dependencies: Vec<UnitDependency>,
    recovery_mode_allowed: bool,
    adapter_id: Option<&str>,
    requires: &[&str],
) -> UnitManifest {
    UnitManifest {
        schema_version: "aios.unit.v1alpha1".to_owned(),
        unit_id: unit_id(name),
        unit_kind,
        display_name: format!("AIOS {name}"),
        description: "T-093 closure service unit.".to_owned(),
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
            args: serde_json::json!({ "path": "/healthz", "port": 7421 }),
        },
        startup_deadline_seconds: 60,
        stop_deadline_seconds: 30,
        adapter_target: serde_json::json!({
            "systemd_unit": format!("{name}.service"),
            "requires": requires,
        }),
        labels: Some(serde_json::json!({
            "layer": "L3",
            "criticality": "standard",
            "recovery_mode_allowed": recovery_mode_allowed,
        })),
        correlation_id: Some(format!("corr_t093_{name}")),
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
    unit_kind: UnitKind,
    dependencies: Vec<UnitDependency>,
    recovery_mode_allowed: bool,
    adapter_id: Option<&str>,
    requires: &[&str],
) -> TestResult<UnitManifest> {
    let mut manifest = unsigned_manifest(
        name,
        unit_kind,
        dependencies,
        recovery_mode_allowed,
        adapter_id,
        requires,
    );
    sign_manifest(&mut manifest, sk)?;
    Ok(manifest)
}

async fn register_unit(
    stack: &Stack,
    sk: &SigningKey,
    name: &str,
    dependencies: Vec<UnitDependency>,
) -> TestResult<UnitId> {
    let manifest = signed_manifest(
        name,
        sk,
        UnitKind::Service,
        dependencies,
        false,
        Some(ADAPTER_ID),
        &["unit.start"],
    )?;
    let id = manifest.unit_id.clone();
    stack.graph.register_unit(manifest).await?;
    Ok(id)
}

async fn register_running_unit(
    stack: &Stack,
    sk: &SigningKey,
    name: &str,
    unit_kind: UnitKind,
    recovery_mode_allowed: bool,
) -> TestResult<UnitId> {
    let manifest = signed_manifest(
        name,
        sk,
        unit_kind,
        Vec::new(),
        recovery_mode_allowed,
        Some(ADAPTER_ID),
        &["unit.start"],
    )?;
    let id = manifest.unit_id.clone();
    stack.graph.register_unit(manifest).await?;
    stack.fsm.start(&id).await?;
    Ok(id)
}

async fn enter_recovery(boundary: &InMemoryRecoveryBoundary) -> TestResult {
    boundary
        .enter_recovery(EnterRecoveryRequest {
            reason: "OPERATOR_INITIATED".to_owned(),
            operator_grant: Some("grant_t093".to_owned()),
            expected_phases: vec![BootPhase::Recovery],
            bundle: None,
        })
        .await?;
    Ok(())
}

async fn exit_recovery(boundary: &InMemoryRecoveryBoundary) -> TestResult {
    let token = boundary
        .current_exit_token()
        .await
        .ok_or("missing recovery exit token")?;
    boundary.exit_recovery(&token).await?;
    Ok(())
}

async fn propagate_failed_dependencies(stack: &Stack, dependent: &UnitId) -> TestResult {
    let edges = stack.graph.list_dependencies(dependent).await?;
    let mut blocked_by_failed = false;
    for edge in edges {
        if edge.kind.is_hard()
            && stack.graph.get_unit(&edge.to_unit_id).await?.state == UnitState::Failed
        {
            blocked_by_failed = true;
            break;
        }
    }
    if blocked_by_failed {
        stack
            .fsm
            .mark_failed(dependent, "hard dependency failed".to_owned())
            .await?;
    }
    Ok(())
}

#[tokio::test]
async fn multi_unit_boot_traverses_and_starts_three_units_in_topological_order() -> TestResult {
    let unit_sk = signing_key(80);
    let stack = build_stack(&unit_sk).await?;
    let base = register_unit(&stack, &unit_sk, "base", Vec::new()).await?;
    let sidecar = register_unit(
        &stack,
        &unit_sk,
        "sidecar",
        vec![UnitDependency {
            unit_id: base.clone(),
            kind: DependencyKind::RequiresRunning,
        }],
    )
    .await?;
    let app = register_unit(
        &stack,
        &unit_sk,
        "app",
        vec![
            UnitDependency {
                unit_id: base.clone(),
                kind: DependencyKind::RequiresRunning,
            },
            UnitDependency {
                unit_id: sidecar.clone(),
                kind: DependencyKind::RequiresRunning,
            },
        ],
    )
    .await?;

    let ordered = stack.evaluator.topological_sort().await?;
    assert_eq!(ordered, vec![base.clone(), sidecar.clone(), app.clone()]);

    for unit_id in &ordered {
        assert!(stack.evaluator.evaluate_readiness(unit_id).await?);
        let adapter = stack
            .sgr_registry
            .find_adapter_for_unit(&stack.graph.get_unit(unit_id).await?.manifest)
            .await?;
        assert!(
            adapter.is_some(),
            "SGR adapter registry must satisfy {unit_id}"
        );
        let ctx = stack
            .capability_adapter
            .start_unit_via_runtime(unit_id)
            .await?;
        assert_eq!(
            ctx.status,
            aios_capability_runtime::ActionLifecycleState::Succeeded
        );
    }

    for unit_id in [base, sidecar, app] {
        assert_eq!(
            stack.graph.get_unit(&unit_id).await?.state,
            UnitState::Running
        );
    }
    assert!(stack.evaluator.is_converged().await?);
    Ok(())
}

#[tokio::test]
async fn cycle_detection_returns_three_unit_scc() -> TestResult {
    let unit_sk = signing_key(81);
    let stack = build_stack(&unit_sk).await?;
    let alpha = register_unit(&stack, &unit_sk, "alpha", Vec::new()).await?;
    let beta = register_unit(&stack, &unit_sk, "beta", Vec::new()).await?;
    let gamma = register_unit(&stack, &unit_sk, "gamma", Vec::new()).await?;

    stack
        .graph
        .declare_dependency(&alpha, &beta, DependencyKind::RequiresRunning)
        .await?;
    stack
        .graph
        .declare_dependency(&beta, &gamma, DependencyKind::RequiresRunning)
        .await?;
    stack
        .graph
        .declare_dependency(&gamma, &alpha, DependencyKind::RequiresRunning)
        .await?;

    let cycles = stack.evaluator.detect_cycles().await?;
    assert_eq!(cycles, vec![vec![alpha, beta, gamma]]);
    assert!(matches!(
        stack.evaluator.topological_sort().await,
        Err(SgrError::DependencyCycleDetected(_))
    ));
    Ok(())
}

#[tokio::test]
async fn failed_unit_isolation_fails_dependents_only() -> TestResult {
    let unit_sk = signing_key(82);
    let stack = build_stack(&unit_sk).await?;
    let middle = register_unit(&stack, &unit_sk, "middle", Vec::new()).await?;
    let dependent = register_unit(
        &stack,
        &unit_sk,
        "dependent",
        vec![UnitDependency {
            unit_id: middle.clone(),
            kind: DependencyKind::RequiresRunning,
        }],
    )
    .await?;
    let independent = register_unit(&stack, &unit_sk, "independent", Vec::new()).await?;

    for unit_id in stack.evaluator.topological_sort().await? {
        stack
            .capability_adapter
            .start_unit_via_runtime(&unit_id)
            .await?;
    }
    stack
        .fsm
        .mark_failed(&middle, "adapter health failure".to_owned())
        .await?;
    propagate_failed_dependencies(&stack, &dependent).await?;

    assert_eq!(
        stack.graph.get_unit(&middle).await?.state,
        UnitState::Failed
    );
    assert_eq!(
        stack.graph.get_unit(&dependent).await?.state,
        UnitState::Failed
    );
    assert_eq!(
        stack.graph.get_unit(&independent).await?.state,
        UnitState::Running
    );
    assert_eq!(
        stack.evaluator.convergence_state().await?,
        aios_sgr::GraphState::Degraded
    );
    Ok(())
}

#[tokio::test]
async fn recovery_pause_stops_normal_units_and_resumes_after_exit() -> TestResult {
    let unit_sk = signing_key(83);
    let stack = build_stack(&unit_sk).await?;
    let normal_a =
        register_running_unit(&stack, &unit_sk, "normal_a", UnitKind::Service, false).await?;
    let normal_b =
        register_running_unit(&stack, &unit_sk, "normal_b", UnitKind::Service, false).await?;
    let recovery_task = register_running_unit(
        &stack,
        &unit_sk,
        "recovery_task",
        UnitKind::RecoveryTask,
        true,
    )
    .await?;

    enter_recovery(&stack.recovery_boundary).await?;
    let mut paused = stack.recovery_hook.pause_normal_units().await?;
    paused.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    assert_eq!(paused, vec![normal_a.clone(), normal_b.clone()]);
    assert_eq!(
        stack.graph.get_unit(&recovery_task).await?.state,
        UnitState::Running
    );

    exit_recovery(&stack.recovery_boundary).await?;
    let mut resumed = stack.recovery_hook.resume_normal_units().await?;
    resumed.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    assert_eq!(resumed, vec![normal_a.clone(), normal_b.clone()]);

    for unit_id in [normal_a, normal_b, recovery_task] {
        assert_eq!(
            stack.graph.get_unit(&unit_id).await?.state,
            UnitState::Running
        );
    }
    Ok(())
}

#[tokio::test]
async fn adapter_fail_closed_without_matching_capability_marks_unit_failed() -> TestResult {
    let unit_sk = signing_key(84);
    let stack = build_stack(&unit_sk).await?;
    let manifest = signed_manifest(
        "needs_gpu_adapter",
        &unit_sk,
        UnitKind::Service,
        Vec::new(),
        false,
        None,
        &["gpu.compute"],
    )?;
    assert!(stack
        .sgr_registry
        .find_adapter_for_unit(&manifest)
        .await?
        .is_none());
    let unit_id = manifest.unit_id.clone();
    stack.graph.register_unit(manifest).await?;

    let err = stack
        .capability_adapter
        .start_unit_via_runtime(&unit_id)
        .await
        .expect_err("missing adapter must fail closed");

    assert!(matches!(err, SgrError::AdapterCapabilityMismatch { .. }));
    assert_eq!(
        stack.graph.get_unit(&unit_id).await?.state,
        UnitState::Failed
    );
    Ok(())
}
