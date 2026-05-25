//! T-083 section 22 full-real MVP path.

#![allow(
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::panic,
    clippy::too_many_lines,
    reason = "integration test fixtures are intentionally explicit"
)]

use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use tokio::sync::Mutex;

use aios_action::{ActionEnvelope, ActionId, Identity, Request, Trace};
use aios_capability_runtime::adapter_manifest::AdapterActionDeclaration;
use aios_capability_runtime::runtime::RuntimeVerificationEngine;
use aios_capability_runtime::{
    canonical_signed_manifest_bytes, encode_hex_signature, ActionDispatchKind,
    ActionLifecycleState, AdapterIOMode, AdapterManifest, AdapterStability, ApprovalBindingSink,
    CapabilityRuntime, DispatchQueue, EvidenceEmitter, EvidenceSink, InMemoryAdapterRegistry,
    InMemoryApprovalSink, InMemoryCapabilityRuntime, RuntimeContext, RuntimeRecoveryHook,
};
use aios_evidence::{EvidenceError, EvidenceReceipt, ReceiptBuilder, ReceiptChain, RecordType};
use aios_fs::{
    materialize_view, AiosFs, AiosPath, ChunkId, ChunkRef, ConsistencyClass, FsContext,
    InMemoryAiosFs, NamespaceClass, ObjectWriteRequest, Predicate, Query, QueryField,
    QueryNamespace, QueryOperator, QueryValue, SubjectRef as FsSubjectRef,
};
use aios_policy::{
    ApprovalRequirement, Constraints, Decision, HydratedSubject, PolicyContext, PolicyDecision,
    PolicyError, PolicyKernel, SubjectType,
};
use aios_recovery::first_boot::FIRST_BOOT_PROVISIONING_PHASES;
use aios_recovery::{
    BootPhase, CandidateState, EnterRecoveryRequest, FirstBootDriver, FirstBootStatus,
    InMemoryRecoveryBoundary, KernelManifest, KernelPipelineDriver, RecoveryBoundary,
    RecoveryEvidenceEmitter, RecoveryEvidenceLog, RecoveryMode, RecoveryRuntimeAdapter,
    RecoverySubjectRef, AIOS_RECOVERY_SUBJECT,
};
use aios_renderer_cli::{OutputFormat, RenderContext, Renderable};
use aios_vault::{InMemoryVaultBroker, SubjectRef as VaultSubjectRef, VaultBroker};
use aios_verification::{InMemoryVerificationEngine, VerificationRuntimeAdapter};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const ACTION_KIND: &str = "aios.fs.write";
const ADAPTER_AUTHORITY: &str = "t083-adapter-authority";
const KERNEL_AUTHORITY: &str = "aios-kernel-root";

#[derive(Debug)]
struct SharedEvidenceLog {
    chain: Mutex<ReceiptChain>,
    runtime_signing_key: SigningKey,
}

impl SharedEvidenceLog {
    fn new(runtime_signing_key: SigningKey) -> Self {
        Self {
            chain: Mutex::new(ReceiptChain::new()),
            runtime_signing_key,
        }
    }

    async fn receipts(&self) -> Vec<EvidenceReceipt> {
        self.chain.lock().await.receipts().to_vec()
    }

    async fn verify_integrity(&self) -> Result<(), EvidenceError> {
        self.chain.lock().await.verify_integrity()
    }
}

#[async_trait]
impl EvidenceSink for SharedEvidenceLog {
    async fn append_signed(
        &self,
        builder: ReceiptBuilder,
    ) -> Result<EvidenceReceipt, EvidenceError> {
        let mut guard = self.chain.lock().await;
        let previous = guard.receipts().last().cloned();
        let receipt = builder.seal_signed(previous.as_ref(), &self.runtime_signing_key)?;
        guard.append(receipt.clone())?;
        drop(guard);
        Ok(receipt)
    }
}

#[async_trait]
impl RecoveryEvidenceLog for SharedEvidenceLog {
    async fn append_signed(
        &self,
        builder: ReceiptBuilder,
        signing_key: &SigningKey,
        expected_previous_receipt_id: Option<&str>,
    ) -> Result<EvidenceReceipt, EvidenceError> {
        let mut guard = self.chain.lock().await;
        let previous = guard.receipts().last().cloned();
        if let Some(expected) = expected_previous_receipt_id {
            let actual = previous
                .as_ref()
                .map(|receipt| receipt.receipt_id().as_str());
            if actual != Some(expected) {
                return Err(EvidenceError::ChainBroken {
                    index: guard.len(),
                    actual: actual.unwrap_or("<genesis>").to_owned(),
                    expected: expected.to_owned(),
                });
            }
        }
        let receipt = builder.seal_signed(previous.as_ref(), signing_key)?;
        guard.append(receipt.clone())?;
        drop(guard);
        Ok(receipt)
    }
}

#[derive(Debug)]
struct ScopedAllowPolicyKernel;

#[async_trait]
impl PolicyKernel for ScopedAllowPolicyKernel {
    async fn evaluate_policy(
        &self,
        envelope: &ActionEnvelope,
        context: &PolicyContext,
    ) -> Result<PolicyDecision, PolicyError> {
        Ok(PolicyDecision {
            policy_decision_id: "poldec_t083_full_real_allow".to_owned(),
            action_id: ActionId::new(),
            request_hash: envelope.request.request_hash().unwrap_or_default(),
            bundle_version: context.bundle_version.clone(),
            enrichment_snapshot_id: "enrich_t083_full_real".to_owned(),
            decision: Decision::Allow,
            reason_code: "ScopedAllow".to_owned(),
            reason_message: "T-083 full-real path allows the journal write fixture".to_owned(),
            constraints: Constraints {
                verification_required: true,
                ..Constraints::default()
            },
            approval: ApprovalRequirement::default(),
            evidence_receipt_id: String::new(),
            evaluated_at: Utc::now(),
            rules_consulted: 1,
            simulated: false,
        })
    }
}

fn unsigned_manifest(action_kind: &str) -> AdapterManifest {
    let now = Utc::now();
    AdapterManifest {
        adapter_id: format!("adapter:aios:t083-{}:0.1.0", action_kind.replace('.', "-")),
        adapter_version: "0.1.0".to_owned(),
        vendor: "aios".to_owned(),
        name: format!("t083-{}", action_kind.replace('.', "-")),
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
        declared_invariants_supported: vec!["INV-014".to_owned()],
        default_adapter_timeout_seconds: 30,
        default_sandbox_profile_id: "host-service-control".to_owned(),
        adapter_signature: String::new(),
        signing_key_id: ADAPTER_AUTHORITY.to_owned(),
        manifest_created_at: now,
        manifest_expires_at: now + Duration::days(1),
    }
}

fn sign_adapter_manifest(manifest: &mut AdapterManifest, signing_key: &SigningKey) -> TestResult {
    let body = canonical_signed_manifest_bytes(manifest)?;
    let signature = signing_key.sign(&body);
    manifest.adapter_signature = encode_hex_signature(&signature.to_bytes());
    Ok(())
}

fn kernel_manifest(version: &str, requires_recovery_install: bool) -> KernelManifest {
    KernelManifest {
        version: version.to_owned(),
        min_aios_version: "0.1.0".to_owned(),
        requires_recovery_install,
        verification_intent: Some("dedicated kernel candidate gate witness".to_owned()),
        tags: vec!["KSPP_STRICT".to_owned()],
    }
}

fn sign_kernel_manifest(
    manifest: &KernelManifest,
    signing_key: &SigningKey,
) -> TestResult<Vec<u8>> {
    Ok(signing_key
        .sign(&serde_json::to_vec(manifest)?)
        .to_bytes()
        .to_vec())
}

fn runtime_context() -> RuntimeContext {
    RuntimeContext::from_subject(
        HydratedSubject {
            canonical_subject_id: "family:alice".to_owned(),
            subject_type: SubjectType::Human,
            groups: vec!["family".to_owned()],
            capabilities: Vec::new(),
            session_class: "INTERNAL".to_owned(),
            recovery_mode: false,
            is_ai: false,
        },
        "polb_t083_full_real",
        "0.1.0-T083",
    )
}

fn action_envelope(aios_path: &AiosPath, object_id: &str, version_id: &str) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("family:alice", false),
        Request::new(
            ACTION_KIND,
            serde_json::json!({
                "path": aios_path.as_str(),
                "object_id": object_id,
                "version_id": version_id,
                "content": "I went hiking today.",
                "verification_intent": "subject_session_flag_state(subject_canonical_id=\"family:alice\", session_id=\"sess_t083\", flag=\"is_recovery_mode\", expected_state=false, observed_state=false)"
            }),
        ),
        Trace::new("00000000000000000000000000000083", "0000000000000083", None),
    )
}

async fn register_adapter() -> TestResult<Arc<InMemoryAdapterRegistry>> {
    let signing_key = SigningKey::from_bytes(&[83_u8; 32]);
    let mut manifest = unsigned_manifest(ACTION_KIND);
    sign_adapter_manifest(&mut manifest, &signing_key)?;

    let mut trusted = HashMap::new();
    trusted.insert(ADAPTER_AUTHORITY.to_owned(), signing_key.verifying_key());
    let registry = Arc::new(InMemoryAdapterRegistry::new(trusted));
    registry.register(manifest, Utc::now()).await?;
    Ok(registry)
}

fn enter_request() -> EnterRecoveryRequest {
    EnterRecoveryRequest {
        reason: "OPERATOR_INITIATED".to_owned(),
        operator_grant: Some("grant_t083_kernel_refresh".to_owned()),
        expected_phases: vec![BootPhase::Recovery],
        bundle: None,
    }
}

#[tokio::test]
async fn section_22_full_real_walk_l1_boot_through_l7_render() -> TestResult {
    let shared_log = Arc::new(SharedEvidenceLog::new(SigningKey::from_bytes(&[84_u8; 32])));
    let recovery_log: Arc<dyn RecoveryEvidenceLog> = shared_log.clone();
    let recovery_emitter = Arc::new(RecoveryEvidenceEmitter::new(
        recovery_log,
        SigningKey::from_bytes(&[85_u8; 32]),
        RecoverySubjectRef(AIOS_RECOVERY_SUBJECT.to_owned()),
    ));

    let boundary = Arc::new(InMemoryRecoveryBoundary::with_evidence_emitter(
        recovery_emitter.clone(),
    ));
    let initial_state = boundary.current_state().await;
    assert_eq!(initial_state.mode, RecoveryMode::Normal);
    assert!(!boundary.is_recovery_active().await);

    let first_boot_boundary: Arc<dyn RecoveryBoundary> = boundary.clone();
    let first_boot =
        FirstBootDriver::with_evidence_emitter(first_boot_boundary, recovery_emitter.clone());
    let first_boot_context = first_boot.run_provisioning().await?;
    assert_eq!(first_boot_context.status, FirstBootStatus::Completed);
    assert_eq!(
        first_boot_context.performed_phases,
        FIRST_BOOT_PROVISIONING_PHASES
    );
    assert_eq!(first_boot.stage_records().await.len(), 14);

    let kernel_signing_key = SigningKey::from_bytes(&[86_u8; 32]);
    let kernel_boundary: Arc<dyn RecoveryBoundary> = boundary.clone();
    let kernel_pipeline =
        KernelPipelineDriver::with_evidence_emitter(kernel_boundary, recovery_emitter.clone())
            .with_trusted_authority(
                KERNEL_AUTHORITY.to_owned(),
                kernel_signing_key.verifying_key(),
            );
    let manifest = kernel_manifest("0.1.0-t083-full-real", false);
    let registered = kernel_pipeline
        .register_candidate(
            manifest.clone(),
            sign_kernel_manifest(&manifest, &kernel_signing_key)?,
        )
        .await?;
    assert_eq!(registered.state, CandidateState::Built);
    let verified = kernel_pipeline
        .verify_candidate(&registered.candidate_id)
        .await?;
    assert_eq!(verified.state, CandidateState::GatePassed);
    let active = kernel_pipeline
        .activate_candidate(&registered.candidate_id)
        .await?;
    assert_eq!(active.state, CandidateState::APromoted);

    let fs = InMemoryAiosFs::new();
    let aios_path = AiosPath::new("/aios/groups/family/users/alice/home/journal/2026-05-11.md");
    assert_eq!(aios_path.namespace_class(), Some(NamespaceClass::UserHome));

    let action_id = ActionId::new();
    let fs_context = FsContext {
        subject: FsSubjectRef("family:alice".to_owned()),
        action_id: Some(action_id.clone()),
        expected_snapshot_id: None,
        consistency_class: ConsistencyClass::Linearizable,
    };
    let chunk_bytes = b"I went hiking today.\n";
    let write_result = fs
        .write_object(
            ObjectWriteRequest {
                object_id: None,
                parent_version_ids: Vec::new(),
                chunks: vec![ChunkRef(ChunkId::from_hash_bytes(chunk_bytes))],
                metadata_delta: serde_json::json!({
                    "kind": "FILE",
                    "privacy_class": "SENSITIVE",
                    "name": "2026-05-11.md",
                    "mime": "text/markdown",
                    "policy_tags": ["personal", "journal"],
                    "scope": {
                        "kind": "USER",
                        "group_id": "family",
                        "user_id": "alice"
                    }
                }),
                action_id: Some(action_id),
                subject: FsSubjectRef("family:alice".to_owned()),
            },
            &fs_context,
        )
        .await?;
    let read_result = fs
        .read_object(
            &write_result.object_id,
            Some(&write_result.snapshot_id_after),
        )
        .await?;
    assert_eq!(read_result.version.version_id, write_result.version_id);
    assert_eq!(read_result.chunks.len(), 1);

    let view = materialize_view(
        &Query::And(vec![Predicate {
            namespace: QueryNamespace::Object,
            field: QueryField::ObjectMetadataName,
            op: QueryOperator::Eq,
            rhs: QueryValue::String("2026-05-11.md".to_owned()),
        }]),
        &fs,
        Some(&write_result.snapshot_id_after),
    )
    .await?;
    assert_eq!(view.matched.len(), 1);
    assert_eq!(view.matched[0].object_id, write_result.object_id);

    let vault = InMemoryVaultBroker::new();
    assert!(
        vault
            .list_capabilities(&VaultSubjectRef("family:alice".to_owned()))
            .await?
            .is_empty(),
        "vault broker is constructed as a real M6 backend even when this path does not request a capability"
    );

    let registry = register_adapter().await?;
    let runtime_sink: Arc<dyn EvidenceSink> = shared_log.clone();
    let runtime_evidence = Arc::new(EvidenceEmitter::new(runtime_sink));
    let verification_engine = Arc::new(InMemoryVerificationEngine::new());
    let runtime_verification: Arc<dyn RuntimeVerificationEngine> =
        Arc::new(VerificationRuntimeAdapter::new(verification_engine));
    let recovery_hook: Arc<dyn RuntimeRecoveryHook> =
        Arc::new(RecoveryRuntimeAdapter::new(boundary.clone()));
    let approval_sink: Arc<dyn ApprovalBindingSink> = Arc::new(InMemoryApprovalSink::new());
    let runtime = InMemoryCapabilityRuntime::new()
        .with_adapter_registry(registry)
        .with_dispatch_queue(Arc::new(DispatchQueue::new_with_defaults()))
        .with_policy_kernel(Arc::new(ScopedAllowPolicyKernel))
        .with_evidence_emitter(runtime_evidence)
        .with_verification_engine(runtime_verification)
        .with_recovery_hook(recovery_hook)
        .with_approval_sink(approval_sink);

    let envelope = action_envelope(
        &aios_path,
        write_result.object_id.as_str(),
        write_result.version_id.as_str(),
    );
    let ctx = runtime.submit_action(&envelope, &runtime_context()).await?;
    assert_eq!(
        ctx.status,
        ActionLifecycleState::Succeeded,
        "action context: {ctx:#?}"
    );
    assert_eq!(ctx.error, None);
    assert_eq!(ctx.dispatch_kind, ActionDispatchKind::SubprocessFork);
    assert!(ctx.evidence_chain.len() >= 7);
    let persisted_ctx = runtime.get_action_status(&ctx.action_id).await?;
    assert_eq!(persisted_ctx.status, ActionLifecycleState::Succeeded);

    let receipts = shared_log.receipts().await;
    shared_log.verify_integrity().await?;
    let record_types = receipts
        .iter()
        .map(EvidenceReceipt::record_type)
        .collect::<Vec<_>>();
    for required in [
        RecordType::FirstBootStarted,
        RecordType::FirstBootStageCompleted,
        RecordType::FirstBootComplete,
        RecordType::KernelPipelineStarted,
        RecordType::KernelGateResult,
        RecordType::KernelPromotedToA,
        RecordType::ActionReceived,
        RecordType::PolicyDecision,
        RecordType::ActionDispatched,
        RecordType::RoutingDecision,
        RecordType::ExecutionStarted,
        RecordType::ExecutionCompleted,
        RecordType::VerificationResult,
    ] {
        assert!(
            record_types.contains(&required),
            "missing full-real evidence record {required:?}"
        );
    }

    boundary.enter_recovery(enter_request()).await?;
    let recovery_state = boundary.current_state().await;
    assert_eq!(recovery_state.mode, RecoveryMode::Recovery);
    let token = boundary
        .current_exit_token()
        .await
        .ok_or("missing recovery exit token")?;
    boundary.exit_recovery(&token).await?;
    let final_state = boundary.current_state().await;
    assert_eq!(final_state.mode, RecoveryMode::Normal);

    let record_types = shared_log
        .receipts()
        .await
        .iter()
        .map(EvidenceReceipt::record_type)
        .collect::<Vec<_>>();
    assert!(record_types.contains(&RecordType::RecoveryBootEntered));
    assert!(record_types.contains(&RecordType::RecoveryBootExited));

    let rendered_text = ctx.render(OutputFormat::Text, &RenderContext::new_pipe_defaults())?;
    assert!(rendered_text.contains("Succeeded"));
    assert!(rendered_text.contains(ctx.action_id.as_str()));
    let rendered_json = ctx.render(OutputFormat::Json, &RenderContext::new_pipe_defaults())?;
    assert!(rendered_json.contains("\"status\""));
    assert!(rendered_json.contains("SUCCEEDED"));

    Ok(())
}
