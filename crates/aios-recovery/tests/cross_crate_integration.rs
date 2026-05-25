//! T-081 cross-crate integration tests.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_capability_runtime::adapter_manifest::AdapterActionDeclaration;
use aios_capability_runtime::{
    canonical_signed_manifest_bytes, encode_hex_signature, ActionDispatchKind,
    ActionLifecycleState, AdapterIOMode, AdapterManifest, AdapterStability, CapabilityRuntime,
    ExecutionFailureReason, InMemoryAdapterRegistry, InMemoryCapabilityRuntime, RuntimeContext,
    RuntimeRecoveryHook,
};
use aios_fs::{AiosPath, NamespacePolicy, SubjectRef};
use aios_policy::{HydratedSubject, PolicyError, SubjectHydrator, SubjectType};
use aios_recovery::{
    BootPhase, EnterRecoveryRequest, InMemoryRecoveryBoundary, RecoveryBoundary,
    RecoveryPolicyHydratorEnhancer, RecoveryRuntimeAdapter,
};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const RECOVERY_ONLY_ACTION: &str = "recovery.firstboot.reset";
const NORMAL_ACTION: &str = "service.restart";

fn enter_request() -> EnterRecoveryRequest {
    EnterRecoveryRequest {
        reason: "OPERATOR_INITIATED".to_owned(),
        operator_grant: Some("grant_t081".to_owned()),
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

fn envelope(action: &str) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("_system:recovery:operator", false),
        Request::new(
            action,
            serde_json::json!({"path": "/aios/system/policy/active.bundle"}),
        ),
        Trace::new("00000000000000000000000000000081", "0000000000000081", None),
    )
}

fn runtime_context() -> RuntimeContext {
    RuntimeContext::from_subject(hydrated_subject(false), "polb_t081", "code_t081")
}

fn hydrated_subject(recovery_mode: bool) -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: "_system:recovery:operator".to_owned(),
        subject_type: SubjectType::Human,
        groups: vec!["operators".to_owned()],
        capabilities: vec![],
        session_class: "RECOVERY".to_owned(),
        recovery_mode,
        is_ai: false,
    }
}

fn unsigned_manifest(action_kind: &str) -> AdapterManifest {
    let now = Utc::now();
    AdapterManifest {
        adapter_id: format!("adapter:aios:t081-{}:0.1.0", action_kind.replace('.', "-")),
        adapter_version: "0.1.0".to_owned(),
        vendor: "aios".to_owned(),
        name: format!("t081-{}", action_kind.replace('.', "-")),
        declared_stability: AdapterStability::Stable,
        io_mode: AdapterIOMode::TypedParametersOnly,
        dispatch_kind: ActionDispatchKind::SubprocessFork,
        declared_actions: vec![AdapterActionDeclaration {
            action_kind: action_kind.to_owned(),
            target_schema: serde_json::json!({"type": "object"}),
            response_schema: serde_json::json!({"type": "object"}),
            rollback_strategy: "NONE".to_owned(),
            timeout_seconds: 30,
            template_string: None,
            template_substitution_variables: vec![],
        }],
        declared_invariants_supported: vec!["INV-012".to_owned()],
        default_adapter_timeout_seconds: 30,
        default_sandbox_profile_id: "host-service-control".to_owned(),
        adapter_signature: String::new(),
        signing_key_id: "t081-authority".to_owned(),
        manifest_created_at: now,
        manifest_expires_at: now + Duration::days(1),
    }
}

fn sign_manifest(manifest: &mut AdapterManifest, signing_key: &SigningKey) -> TestResult {
    let body = canonical_signed_manifest_bytes(manifest)?;
    let signature = signing_key.sign(&body);
    manifest.adapter_signature = encode_hex_signature(&signature.to_bytes());
    Ok(())
}

async fn runtime_with_registered_action(
    action_kind: &str,
    boundary: Arc<InMemoryRecoveryBoundary>,
    attach_hook: bool,
) -> Result<InMemoryCapabilityRuntime, Box<dyn std::error::Error + Send + Sync>> {
    let signing_key = SigningKey::from_bytes(&[81_u8; 32]);
    let mut manifest = unsigned_manifest(action_kind);
    sign_manifest(&mut manifest, &signing_key)?;

    let mut trusted = HashMap::new();
    trusted.insert("t081-authority".to_owned(), signing_key.verifying_key());
    let registry = Arc::new(InMemoryAdapterRegistry::new(trusted));
    registry.register(manifest, Utc::now()).await?;

    let runtime = InMemoryCapabilityRuntime::new().with_adapter_registry(registry);
    if attach_hook {
        let hook: Arc<dyn RuntimeRecoveryHook> =
            Arc::new(RecoveryRuntimeAdapter::new(Arc::clone(&boundary)));
        Ok(runtime.with_recovery_hook(hook))
    } else {
        Ok(runtime)
    }
}

#[derive(Debug, Clone)]
struct FixedHydrator {
    subject: HydratedSubject,
}

#[async_trait]
impl SubjectHydrator for FixedHydrator {
    async fn hydrate(&self, _provisional: &str) -> Result<HydratedSubject, PolicyError> {
        Ok(self.subject.clone())
    }
}

fn base_hydrator(recovery_mode: bool) -> Arc<dyn SubjectHydrator> {
    Arc::new(FixedHydrator {
        subject: hydrated_subject(recovery_mode),
    })
}

async fn hydrated_namespace_mutation(
    boundary: Arc<InMemoryRecoveryBoundary>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let hydrator = RecoveryPolicyHydratorEnhancer::new(base_hydrator(false), Arc::clone(&boundary));
    let subject = hydrator.hydrate("_system:recovery:operator").await?;
    let path = AiosPath::new("/aios/system/policy/active.bundle");
    NamespacePolicy::can_mutate(
        &path,
        &SubjectRef(subject.canonical_subject_id),
        subject.recovery_mode,
        subject.is_ai,
    )?;
    Ok(())
}

#[tokio::test]
async fn runtime_recovery_hook_trait_object_reports_normal_mode() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let hook: Arc<dyn RuntimeRecoveryHook> =
        Arc::new(RecoveryRuntimeAdapter::new(Arc::clone(&boundary)));

    assert!(!hook.current_recovery_mode().await);
}

#[tokio::test]
async fn runtime_recovery_hook_trait_object_reports_recovery_mode() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    enter_recovery(&boundary).await?;
    let hook: Arc<dyn RuntimeRecoveryHook> =
        Arc::new(RecoveryRuntimeAdapter::new(Arc::clone(&boundary)));

    assert!(hook.current_recovery_mode().await);
    Ok(())
}

#[tokio::test]
async fn recovery_only_adapter_action_fails_in_normal_mode() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let runtime =
        runtime_with_registered_action(RECOVERY_ONLY_ACTION, Arc::clone(&boundary), true).await?;

    let ctx = runtime
        .submit_action(&envelope(RECOVERY_ONLY_ACTION), &runtime_context())
        .await?;

    assert_eq!(ctx.status, ActionLifecycleState::Failed);
    assert_eq!(ctx.error, Some(ExecutionFailureReason::AdapterRefused));
    Ok(())
}

#[tokio::test]
async fn recovery_only_adapter_action_succeeds_in_recovery_mode() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    enter_recovery(&boundary).await?;
    let runtime =
        runtime_with_registered_action(RECOVERY_ONLY_ACTION, Arc::clone(&boundary), true).await?;

    let ctx = runtime
        .submit_action(&envelope(RECOVERY_ONLY_ACTION), &runtime_context())
        .await?;

    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    assert_eq!(ctx.error, None);
    Ok(())
}

#[tokio::test]
async fn normal_adapter_action_succeeds_in_normal_mode_with_hook() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let runtime =
        runtime_with_registered_action(NORMAL_ACTION, Arc::clone(&boundary), true).await?;

    let ctx = runtime
        .submit_action(&envelope(NORMAL_ACTION), &runtime_context())
        .await?;

    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    Ok(())
}

#[tokio::test]
async fn normal_adapter_action_succeeds_in_recovery_mode_with_hook() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    enter_recovery(&boundary).await?;
    let runtime =
        runtime_with_registered_action(NORMAL_ACTION, Arc::clone(&boundary), true).await?;

    let ctx = runtime
        .submit_action(&envelope(NORMAL_ACTION), &runtime_context())
        .await?;

    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    Ok(())
}

#[tokio::test]
async fn recovery_only_action_keeps_backward_compat_without_hook() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let runtime =
        runtime_with_registered_action(RECOVERY_ONLY_ACTION, Arc::clone(&boundary), false).await?;

    let ctx = runtime
        .submit_action(&envelope(RECOVERY_ONLY_ACTION), &runtime_context())
        .await?;

    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    Ok(())
}

#[tokio::test]
async fn exiting_recovery_makes_recovery_only_runtime_action_fail_again() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    enter_recovery(&boundary).await?;
    let runtime =
        runtime_with_registered_action(RECOVERY_ONLY_ACTION, Arc::clone(&boundary), true).await?;

    let recovered = runtime
        .submit_action(&envelope(RECOVERY_ONLY_ACTION), &runtime_context())
        .await?;
    assert_eq!(recovered.status, ActionLifecycleState::Succeeded);

    exit_recovery(&boundary).await?;
    let normal = runtime
        .submit_action(&envelope(RECOVERY_ONLY_ACTION), &runtime_context())
        .await?;

    assert_eq!(normal.status, ActionLifecycleState::Failed);
    assert_eq!(normal.error, Some(ExecutionFailureReason::AdapterRefused));
    Ok(())
}

#[tokio::test]
async fn policy_hydrator_enhancer_overrides_base_false_to_true_in_recovery() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    enter_recovery(&boundary).await?;
    let hydrator = RecoveryPolicyHydratorEnhancer::new(base_hydrator(false), Arc::clone(&boundary));

    let subject = hydrator.hydrate("_system:recovery:operator").await?;

    assert!(subject.recovery_mode);
    Ok(())
}

#[tokio::test]
async fn policy_hydrator_enhancer_sets_false_in_normal_mode() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let hydrator = RecoveryPolicyHydratorEnhancer::new(base_hydrator(false), Arc::clone(&boundary));

    let subject = hydrator.hydrate("_system:recovery:operator").await?;

    assert!(!subject.recovery_mode);
    Ok(())
}

#[tokio::test]
async fn policy_hydrator_enhancer_is_dyn_subject_hydrator_compatible() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    enter_recovery(&boundary).await?;
    let hydrator: Arc<dyn SubjectHydrator> = Arc::new(RecoveryPolicyHydratorEnhancer::new(
        base_hydrator(false),
        Arc::clone(&boundary),
    ));

    let subject = hydrator.hydrate("_system:recovery:operator").await?;

    assert!(subject.recovery_mode);
    Ok(())
}

#[tokio::test]
async fn live_policy_recovery_state_allows_recovery_only_fs_mutation() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    enter_recovery(&boundary).await?;

    hydrated_namespace_mutation(boundary).await
}

#[tokio::test]
async fn live_policy_recovery_state_denies_recovery_only_fs_mutation_in_normal_mode() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());

    let err = hydrated_namespace_mutation(boundary)
        .await
        .expect_err("normal mode must deny recovery-only namespace mutation");

    assert!(err.to_string().contains("recovery mode required"));
}

#[tokio::test]
async fn live_policy_recovery_state_denies_after_recovery_exit() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    enter_recovery(&boundary).await?;
    hydrated_namespace_mutation(Arc::clone(&boundary)).await?;

    exit_recovery(&boundary).await?;
    let err = hydrated_namespace_mutation(boundary)
        .await
        .expect_err("normal mode after exit must deny recovery-only namespace mutation");

    assert!(err.to_string().contains("recovery mode required"));
    Ok(())
}
