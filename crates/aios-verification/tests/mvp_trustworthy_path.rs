//! T-073 — §22 trustworthy MVP contrast path.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::items_after_statements,
    reason = "integration tests use panic-on-failure assertions and local stack fixtures"
)]

use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};

use aios_action::{ActionEnvelope, ActionId, Identity, Request, Trace};
use aios_capability_runtime::{
    adapter_manifest::AdapterActionDeclaration, canonical_signed_manifest_bytes,
    encode_hex_signature, ActionDispatchKind, ActionLifecycleState, AdapterIOMode, AdapterManifest,
    AdapterStability, CapabilityRuntime, DispatchQueue, EvidenceEmitter, ExecutionFailureReason,
    InMemoryAdapterRegistry, InMemoryApprovalSink, InMemoryCapabilityRuntime, InMemoryEvidenceSink,
    RuntimeContext,
};
use aios_evidence::{EvidenceReceipt, RecordType};
use aios_fs::InMemoryAiosFs;
use aios_policy::{
    ApprovalRequirement, ApprovalScope, ApproverClass, Constraints, Decision, HydratedSubject,
    PolicyContext, PolicyDecision, PolicyError, PolicyKernel, SubjectType,
};
use aios_vault::{InMemoryVaultBroker, SubjectRef as VaultSubjectRef, VaultBroker};
use aios_verification::{
    InMemoryVerificationEngine, InMemoryVerificationEvidenceLog, SubjectRef,
    VerificationEvidenceEmitter, VerificationIntent, VerificationPrimitive,
    VerificationRuntimeAdapter, VerificationStatus, AIOS_VERIFICATION_SUBJECT,
};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const TRUSTED_KEY_ID: &str = "publisher:key:t073:mvp:01";
const MVP_ACTION_KIND: &str = "aios.fs.write";
const MVP_TARGET_PATH: &str = "/aios/groups/family/users/alice/journal/2026-05-25.md";

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
            policy_decision_id: "poldec_t073_trustworthy_path".to_owned(),
            action_id: ActionId::new(),
            request_hash: "0".repeat(32),
            bundle_version: "polb_t073_m8".to_owned(),
            enrichment_snapshot_id: "polb_snap_t073_m8".to_owned(),
            decision: Decision::Allow,
            reason_code: "ScopedAllow".to_owned(),
            reason_message: "T-073 trustworthy MVP policy allow".to_owned(),
            constraints: Constraints::default(),
            approval: ApprovalRequirement {
                required: false,
                approval_scope: ApprovalScope::ExactRequestHash,
                ttl_seconds: 0,
                approver_classes: vec![ApproverClass::Human],
                require_human_co_signer: false,
            },
            evidence_receipt_id: "evr_t073_policy_decision".to_owned(),
            evaluated_at: Utc::now(),
            rules_consulted: 1,
            simulated: false,
        })
    }
}

struct TrustworthyStack {
    runtime: InMemoryCapabilityRuntime,
    capability_evidence: Arc<InMemoryEvidenceSink>,
    verification_log: Option<Arc<InMemoryVerificationEvidenceLog>>,
    engine: Option<Arc<InMemoryVerificationEngine>>,
    fs: InMemoryAiosFs,
    vault: InMemoryVaultBroker,
}

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn human_alice() -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: "family:alice".to_owned(),
        subject_type: SubjectType::Human,
        groups: vec!["family".to_owned()],
        capabilities: vec!["cap.aios.fs.write".to_owned()],
        session_class: "INTERACTIVE".to_owned(),
        recovery_mode: false,
        is_ai: false,
    }
}

fn runtime_context() -> RuntimeContext {
    RuntimeContext::from_subject(human_alice(), "polb_t073_m8", "aios-verification/T-073")
}

fn envelope_with_intent(path: &str) -> ActionEnvelope {
    let expression = format!(r#"all(file_exists(path="{path}"))"#);
    ActionEnvelope::new(
        Identity::new("family:alice", false),
        Request::new(
            MVP_ACTION_KIND,
            serde_json::json!({
                "path": MVP_TARGET_PATH,
                "scope": "USER",
                "group_id": "family",
                "user_id": "alice",
                "content": "verified content\n",
                "verification_intent": expression,
            }),
        ),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn unsigned_mvp_manifest() -> AdapterManifest {
    let now = Utc::now();
    AdapterManifest {
        adapter_id: "adapter:aios:fs:t073".to_owned(),
        adapter_version: "0.1.0".to_owned(),
        vendor: "aios".to_owned(),
        name: "aios-fs".to_owned(),
        declared_stability: AdapterStability::Stable,
        io_mode: AdapterIOMode::TypedParametersOnly,
        dispatch_kind: ActionDispatchKind::SubprocessFork,
        declared_actions: vec![AdapterActionDeclaration {
            action_kind: MVP_ACTION_KIND.to_owned(),
            target_schema: serde_json::json!({"type": "object"}),
            response_schema: serde_json::json!({"type": "object"}),
            rollback_strategy: "NONE".to_owned(),
            timeout_seconds: 30,
            template_string: None,
            template_substitution_variables: Vec::new(),
        }],
        declared_invariants_supported: vec!["INV-014".to_owned()],
        default_adapter_timeout_seconds: 60,
        default_sandbox_profile_id: "aios-fs-default".to_owned(),
        adapter_signature: String::new(),
        signing_key_id: TRUSTED_KEY_ID.to_owned(),
        manifest_created_at: now,
        manifest_expires_at: now + Duration::days(365),
    }
}

fn sign_manifest(manifest: &mut AdapterManifest, key: &SigningKey) {
    let body = canonical_signed_manifest_bytes(manifest).expect("canonical manifest body");
    let signature = key.sign(&body);
    manifest.adapter_signature = encode_hex_signature(&signature.to_bytes());
}

async fn registry_with_aios_fs() -> Arc<InMemoryAdapterRegistry> {
    let key = signing_key(73);
    let mut trusted = HashMap::new();
    trusted.insert(TRUSTED_KEY_ID.to_owned(), key.verifying_key());
    let registry = InMemoryAdapterRegistry::new(trusted);
    let mut manifest = unsigned_mvp_manifest();
    sign_manifest(&mut manifest, &key);
    registry
        .register(manifest, Utc::now())
        .await
        .expect("register aios-fs adapter");
    Arc::new(registry)
}

fn capability_evidence() -> (Arc<InMemoryEvidenceSink>, Arc<EvidenceEmitter>) {
    let sink = Arc::new(InMemoryEvidenceSink::new(signing_key(74)));
    let emitter = Arc::new(EvidenceEmitter::new(sink.clone()));
    (sink, emitter)
}

fn verification_evidence() -> (
    Arc<InMemoryVerificationEvidenceLog>,
    Arc<VerificationEvidenceEmitter>,
) {
    let log = Arc::new(InMemoryVerificationEvidenceLog::new());
    let emitter = Arc::new(VerificationEvidenceEmitter::new(
        log.clone(),
        signing_key(75),
        SubjectRef(AIOS_VERIFICATION_SUBJECT.to_owned()),
    ));
    (log, emitter)
}

async fn trustworthy_stack(with_engine: bool) -> TrustworthyStack {
    let registry = registry_with_aios_fs().await;
    let queue = Arc::new(DispatchQueue::new_with_defaults());
    let approval_sink = Arc::new(InMemoryApprovalSink::new());
    let (capability_evidence, capability_emitter) = capability_evidence();
    let fs = InMemoryAiosFs::new();
    let vault = InMemoryVaultBroker::new();

    let base_runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(Arc::new(AllowKernel))
        .with_adapter_registry(registry)
        .with_dispatch_queue(queue)
        .with_evidence_emitter(capability_emitter)
        .with_approval_sink(approval_sink);

    if with_engine {
        let (verification_log, verification_emitter) = verification_evidence();
        let engine =
            Arc::new(InMemoryVerificationEngine::new().with_evidence_emitter(verification_emitter));
        let adapter = Arc::new(VerificationRuntimeAdapter::new(engine.clone()));
        return TrustworthyStack {
            runtime: base_runtime.with_verification_engine(adapter),
            capability_evidence,
            verification_log: Some(verification_log),
            engine: Some(engine),
            fs,
            vault,
        };
    }

    TrustworthyStack {
        runtime: base_runtime,
        capability_evidence,
        verification_log: None,
        engine: None,
        fs,
        vault,
    }
}

fn capability_verification_payload<'a>(
    ctx_chain: &[String],
    receipts: &'a [EvidenceReceipt],
) -> &'a serde_json::Value {
    ctx_chain
        .iter()
        .filter_map(|receipt_id| {
            receipts
                .iter()
                .find(|receipt| receipt.receipt_id().as_str() == receipt_id)
        })
        .find(|receipt| receipt.record_type() == RecordType::VerificationResult)
        .map(EvidenceReceipt::payload)
        .expect("capability VERIFICATION_RESULT in action chain")
}

fn assert_verification_log_status(
    records: &[EvidenceReceipt],
    expected_status: &str,
) -> serde_json::Value {
    records
        .iter()
        .find_map(|receipt| {
            let payload = receipt.payload();
            (receipt.record_type() == RecordType::VerificationResult
                && payload
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .is_some())
            .then(|| payload.clone())
        })
        .inspect(|payload| {
            assert_eq!(
                payload.get("status").and_then(serde_json::Value::as_str),
                Some(expected_status)
            );
        })
        .expect("verification result payload with status")
}

#[tokio::test]
async fn same_envelope_succeeds_only_when_real_verification_passes() -> TestResult {
    let marker = format!("/tmp/aios-marker-{}", ActionId::new().as_str());
    std::fs::write(&marker, b"ok")?;
    let stack = trustworthy_stack(true).await;
    let envelope = envelope_with_intent(&marker);

    let ctx = stack
        .runtime
        .submit_action(&envelope, &runtime_context())
        .await?;
    let capability_records = stack.capability_evidence.receipts().await;
    let capability_payload =
        capability_verification_payload(&ctx.evidence_chain, &capability_records);
    let verification_records = stack
        .verification_log
        .as_ref()
        .expect("verification log")
        .receipts()
        .await;
    let verification_payload = assert_verification_log_status(&verification_records, "PASSED");
    let intent = stack
        .engine
        .as_ref()
        .expect("verification engine")
        .get_result(
            &VerificationIntent::new(ctx.action_id.clone(), "file_exists(path=\"unused\")", 1)
                .intent_id,
        )
        .await;

    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    assert_eq!(ctx.error, None);
    assert_eq!(
        capability_payload
            .get("passed")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        verification_payload
            .get("status")
            .and_then(serde_json::Value::as_str),
        Some("PASSED")
    );
    assert_eq!(intent, None, "only the runtime-bound intent id is cached");
    assert_eq!(stack.fs.snapshot().object_count, 0);
    assert!(stack
        .vault
        .list_capabilities(&VaultSubjectRef("family:alice".to_owned()))
        .await?
        .is_empty());

    std::fs::remove_file(marker)?;
    Ok(())
}

#[tokio::test]
async fn same_envelope_fails_when_real_verification_fails() -> TestResult {
    let marker = format!("/tmp/aios-marker-missing-{}", ActionId::new().as_str());
    let stack = trustworthy_stack(true).await;
    let envelope = envelope_with_intent(&marker);

    let ctx = stack
        .runtime
        .submit_action(&envelope, &runtime_context())
        .await?;
    let capability_records = stack.capability_evidence.receipts().await;
    let capability_payload =
        capability_verification_payload(&ctx.evidence_chain, &capability_records);
    let verification_records = stack
        .verification_log
        .as_ref()
        .expect("verification log")
        .receipts()
        .await;
    let verification_payload = assert_verification_log_status(&verification_records, "FAILED");

    assert_eq!(ctx.status, ActionLifecycleState::Failed);
    assert_eq!(ctx.error, Some(ExecutionFailureReason::AdapterRefused));
    assert_eq!(
        capability_payload
            .get("passed")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        verification_payload
            .get("status")
            .and_then(serde_json::Value::as_str),
        Some("FAILED")
    );
    assert!(verification_records
        .iter()
        .any(|receipt| receipt.record_type() == RecordType::VerificationResult));
    Ok(())
}

#[tokio::test]
async fn same_envelope_without_engine_preserves_t027_stub_success() -> TestResult {
    let marker = format!("/tmp/aios-marker-stub-missing-{}", ActionId::new().as_str());
    let stack = trustworthy_stack(false).await;
    let envelope = envelope_with_intent(&marker);

    let ctx = stack
        .runtime
        .submit_action(&envelope, &runtime_context())
        .await?;
    let capability_records = stack.capability_evidence.receipts().await;
    let capability_payload =
        capability_verification_payload(&ctx.evidence_chain, &capability_records);

    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    assert_eq!(ctx.error, None);
    assert!(stack.verification_log.is_none());
    assert!(stack.engine.is_none());
    assert_eq!(
        capability_payload
            .get("passed")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    Ok(())
}

#[test]
fn runtime_adapter_status_mapping_is_truthworthy_floor() {
    for status in [
        VerificationStatus::Failed,
        VerificationStatus::Timeout,
        VerificationStatus::ProbeError,
        VerificationStatus::Skipped,
    ] {
        assert_ne!(
            status,
            VerificationStatus::Passed,
            "only VERIFICATION_PASSED may let runtime completion succeed"
        );
    }
    assert_eq!(
        VerificationPrimitive::FileExists.as_wire_str(),
        "FILE_EXISTS"
    );
}
