//! T-071 integration tests — Verification Engine ⇄ Capability Runtime.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::items_after_statements,
    clippy::redundant_clone,
    reason = "panic-on-failure is the idiomatic test signal; this file exercises cross-crate integration fixtures"
)]

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};

use aios_action::{ActionEnvelope, ActionId, Identity, Request, Trace};
use aios_capability_runtime::{
    adapter_manifest::AdapterActionDeclaration, canonical_signed_manifest_bytes,
    encode_hex_signature, runtime::RuntimeVerificationEngine, ActionDispatchKind,
    ActionLifecycleState, AdapterIOMode, AdapterManifest, AdapterStability, CapabilityRuntime,
    EvidenceEmitter, ExecutionFailureReason, InMemoryAdapterRegistry, InMemoryCapabilityRuntime,
    InMemoryEvidenceSink, RuntimeContext,
};
use aios_evidence::RecordType;
use aios_verification::{
    InMemoryVerificationEngine, InMemoryVerificationEvidenceLog, MockLocalProbe, SubjectRef,
    VerificationEvidenceEmitter, VerificationIntent, VerificationPrimitive,
    VerificationRuntimeAdapter, VerificationStatus, AIOS_VERIFICATION_SUBJECT,
};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const TRUSTED_KEY_ID: &str = "publisher:key:t071:01";

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn runtime_context() -> RuntimeContext {
    RuntimeContext::new(
        "user:local:operator",
        "polb_t071_runtime_integration",
        "aios-verification/T-071-test",
    )
}

fn envelope_without_intent() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("user:local:operator", false),
        Request::new("service.restart", serde_json::json!({"service": "nginx"})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn envelope_with_intent(intent_source: impl Into<String>) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("user:local:operator", false),
        Request::new(
            "service.restart",
            serde_json::json!({
                "service": "nginx",
                "verification_intent": intent_source.into(),
            }),
        ),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn intent_json(expression: &str) -> Result<(VerificationIntent, String), serde_json::Error> {
    let intent = VerificationIntent::new(ActionId::new(), expression, 5);
    let json = serde_json::to_string(&intent)?;
    Ok((intent, json))
}

fn engine_with_file(path: &str, exists: bool) -> Arc<InMemoryVerificationEngine> {
    Arc::new(InMemoryVerificationEngine::new().with_local_probe(Arc::new(
        MockLocalProbe::default().with_file_exists(path, exists),
    )))
}

fn adapter_for(engine: Arc<InMemoryVerificationEngine>) -> Arc<VerificationRuntimeAdapter> {
    Arc::new(VerificationRuntimeAdapter::new(engine))
}

fn unsigned_manifest(adapter_id: &str) -> AdapterManifest {
    let now = Utc::now();
    AdapterManifest {
        adapter_id: adapter_id.to_owned(),
        adapter_version: "0.1.0".to_owned(),
        vendor: "aios".to_owned(),
        name: "systemd".to_owned(),
        declared_stability: AdapterStability::Stable,
        io_mode: AdapterIOMode::TypedParametersOnly,
        dispatch_kind: ActionDispatchKind::SubprocessFork,
        declared_actions: vec![AdapterActionDeclaration {
            action_kind: "service.restart".to_owned(),
            target_schema: serde_json::json!({"type": "object"}),
            response_schema: serde_json::json!({"type": "object"}),
            rollback_strategy: "IDEMPOTENT_REAPPLY".to_owned(),
            timeout_seconds: 30,
            template_string: None,
            template_substitution_variables: Vec::new(),
        }],
        declared_invariants_supported: vec!["INV-013".to_owned()],
        default_adapter_timeout_seconds: 60,
        default_sandbox_profile_id: "service-restart-default".to_owned(),
        adapter_signature: String::new(),
        signing_key_id: TRUSTED_KEY_ID.to_owned(),
        manifest_created_at: now,
        manifest_expires_at: now + Duration::days(365),
    }
}

fn sign_manifest(manifest: &mut AdapterManifest, sk: &SigningKey) {
    let body = canonical_signed_manifest_bytes(manifest).expect("canonical manifest body");
    let sig = sk.sign(&body);
    manifest.adapter_signature = encode_hex_signature(&sig.to_bytes());
}

async fn registry_with_systemd() -> Arc<InMemoryAdapterRegistry> {
    let sk = signing_key(71);
    let mut trusted = HashMap::new();
    trusted.insert(TRUSTED_KEY_ID.to_owned(), sk.verifying_key());
    let registry = InMemoryAdapterRegistry::new(trusted);
    let mut manifest = unsigned_manifest("adapter:aios:systemd:t071");
    sign_manifest(&mut manifest, &sk);
    registry
        .register(manifest, Utc::now())
        .await
        .expect("register systemd adapter");
    Arc::new(registry)
}

fn capability_evidence() -> (Arc<InMemoryEvidenceSink>, Arc<EvidenceEmitter>) {
    let sink = Arc::new(InMemoryEvidenceSink::new(signing_key(72)));
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
        signing_key(73),
        SubjectRef(AIOS_VERIFICATION_SUBJECT.to_owned()),
    ));
    (log, emitter)
}

#[test]
fn adapter_constructs_from_in_memory_engine() {
    let engine = Arc::new(InMemoryVerificationEngine::new());
    let _adapter = VerificationRuntimeAdapter::new(engine);
}

#[tokio::test]
async fn adapter_parses_valid_intent_json_to_success() -> TestResult {
    let engine = engine_with_file("/tmp/aios-ok", true);
    let adapter = VerificationRuntimeAdapter::new(engine);
    let (_intent, json) = intent_json(r#"file.exists(object_or_path="/tmp/aios-ok")"#)?;
    let action_id = ActionId::new();

    let passed = adapter.verify(&json, action_id.as_str()).await?;

    assert!(passed);
    Ok(())
}

#[tokio::test]
async fn adapter_maps_failed_status_to_false() -> TestResult {
    let engine = engine_with_file("/tmp/aios-missing", false);
    let adapter = VerificationRuntimeAdapter::new(engine);
    let (_intent, json) = intent_json(r#"file.exists(object_or_path="/tmp/aios-missing")"#)?;
    let action_id = ActionId::new();

    let passed = adapter.verify(&json, action_id.as_str()).await?;

    assert!(!passed);
    Ok(())
}

#[tokio::test]
async fn adapter_handles_invalid_intent_json() {
    let engine = Arc::new(InMemoryVerificationEngine::new());
    let adapter = VerificationRuntimeAdapter::new(engine);
    let action_id = ActionId::new();

    let error = adapter.verify("{not-json", action_id.as_str()).await;

    assert!(error.is_err());
}

#[tokio::test]
async fn submit_action_with_verification_intent_passes_to_succeeded() -> TestResult {
    let engine = engine_with_file("/tmp/aios-ok", true);
    let adapter = adapter_for(engine);
    let (_intent, json) = intent_json(r#"file.exists(object_or_path="/tmp/aios-ok")"#)?;
    let runtime = InMemoryCapabilityRuntime::new().with_verification_engine(adapter);

    let ctx = runtime
        .submit_action(&envelope_with_intent(json), &runtime_context())
        .await?;

    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    assert_eq!(ctx.error, None);
    Ok(())
}

#[tokio::test]
async fn submit_action_with_verification_intent_failure_blocks_success() -> TestResult {
    let engine = engine_with_file("/tmp/aios-missing", false);
    let adapter = adapter_for(engine);
    let (_intent, json) = intent_json(r#"file.exists(object_or_path="/tmp/aios-missing")"#)?;
    let runtime = InMemoryCapabilityRuntime::new().with_verification_engine(adapter);

    let ctx = runtime
        .submit_action(&envelope_with_intent(json), &runtime_context())
        .await?;

    assert_eq!(ctx.status, ActionLifecycleState::Failed);
    assert_eq!(ctx.error, Some(ExecutionFailureReason::AdapterRefused));
    Ok(())
}

#[tokio::test]
async fn submit_action_without_verification_intent_preserves_noop_success() -> TestResult {
    let engine = engine_with_file("/tmp/aios-missing", false);
    let adapter = adapter_for(engine);
    let runtime = InMemoryCapabilityRuntime::new().with_verification_engine(adapter);

    let ctx = runtime
        .submit_action(&envelope_without_intent(), &runtime_context())
        .await?;

    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    assert_eq!(ctx.error, None);
    Ok(())
}

#[tokio::test]
async fn end_to_end_marker_file_present_succeeds() -> TestResult {
    let path = format!("/tmp/aios-marker-{}", ActionId::new().as_str());
    std::fs::write(&path, b"ok")?;
    let engine = Arc::new(InMemoryVerificationEngine::new());
    let adapter = adapter_for(engine);
    let runtime = InMemoryCapabilityRuntime::new().with_verification_engine(adapter);
    let expression = format!(r#"all(file_exists(path="{path}"))"#);

    let ctx = runtime
        .submit_action(&envelope_with_intent(expression), &runtime_context())
        .await?;

    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    Ok(())
}

#[tokio::test]
async fn end_to_end_marker_file_absent_fails() -> TestResult {
    let path = format!("/tmp/aios-marker-missing-{}", ActionId::new().as_str());
    let engine = Arc::new(InMemoryVerificationEngine::new());
    let adapter = adapter_for(engine);
    let runtime = InMemoryCapabilityRuntime::new().with_verification_engine(adapter);
    let expression = format!(r#"all(file_exists(path="{path}"))"#);

    let ctx = runtime
        .submit_action(&envelope_with_intent(expression), &runtime_context())
        .await?;

    assert_eq!(ctx.status, ActionLifecycleState::Failed);
    assert_eq!(ctx.error, Some(ExecutionFailureReason::AdapterRefused));
    Ok(())
}

#[tokio::test]
async fn concurrent_submit_action_with_verification_stays_coherent() -> TestResult {
    let engine = engine_with_file("/tmp/aios-ok", true);
    let adapter = adapter_for(engine);
    let runtime = Arc::new(InMemoryCapabilityRuntime::new().with_verification_engine(adapter));
    let context = runtime_context();
    let mut handles = Vec::new();

    for _ in 0..8 {
        let runtime = runtime.clone();
        let context = context.clone();
        let (_intent, json) = intent_json(r#"file.exists(object_or_path="/tmp/aios-ok")"#)?;
        let envelope = envelope_with_intent(json);
        handles.push(tokio::spawn(async move {
            runtime.submit_action(&envelope, &context).await
        }));
    }

    for handle in handles {
        let ctx = handle.await??;
        assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
        assert_eq!(ctx.error, None);
    }
    assert_eq!(runtime.len().await, 8);
    Ok(())
}

#[tokio::test]
async fn adapter_uses_runtime_action_id_for_result_chain() -> TestResult {
    let engine = engine_with_file("/tmp/aios-ok", true);
    let adapter = VerificationRuntimeAdapter::new(engine.clone());
    let (intent, json) = intent_json(r#"file.exists(object_or_path="/tmp/aios-ok")"#)?;
    let runtime_action_id = ActionId::new();

    let passed = adapter.verify(&json, runtime_action_id.as_str()).await?;
    let result = engine
        .get_result(&intent.intent_id)
        .await
        .expect("completed result");

    assert!(passed);
    assert_eq!(result.action_id, runtime_action_id);
    Ok(())
}

#[tokio::test]
async fn section_22_trustworthy_pass_records_real_verification_evidence() -> TestResult {
    let (verification_log, verification_emitter) = verification_evidence();
    let engine = Arc::new(
        InMemoryVerificationEngine::new()
            .with_local_probe(Arc::new(
                MockLocalProbe::default().with_file_exists("/tmp/aios-ok", true),
            ))
            .with_evidence_emitter(verification_emitter),
    );
    let adapter = adapter_for(engine.clone());
    let (intent, json) = intent_json(r#"file.exists(object_or_path="/tmp/aios-ok")"#)?;
    let registry = registry_with_systemd().await;
    let (cap_sink, cap_emitter) = capability_evidence();
    let runtime = InMemoryCapabilityRuntime::new()
        .with_adapter_registry(registry)
        .with_evidence_emitter(cap_emitter)
        .with_verification_engine(adapter);

    let ctx = runtime
        .submit_action(&envelope_with_intent(json), &runtime_context())
        .await?;
    let result = engine
        .get_result(&intent.intent_id)
        .await
        .expect("completed verification result");
    let cap_records = cap_sink.receipts().await;
    let verification_records = verification_log.receipts().await;

    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    assert_eq!(result.status, VerificationStatus::Passed);
    assert!(result.evidence_receipt_id.is_some());
    assert!(cap_records
        .iter()
        .any(|receipt| receipt.record_type() == RecordType::VerificationResult));
    assert!(verification_records
        .iter()
        .any(|receipt| receipt.record_type() == RecordType::VerificationResult));
    Ok(())
}

#[tokio::test]
async fn section_22_trustworthy_contrast_failed_verification_blocks_success() -> TestResult {
    let (verification_log, verification_emitter) = verification_evidence();
    let engine = Arc::new(
        InMemoryVerificationEngine::new()
            .with_local_probe(Arc::new(
                MockLocalProbe::default().with_file_exists("/tmp/aios-missing", false),
            ))
            .with_evidence_emitter(verification_emitter),
    );
    let adapter = adapter_for(engine.clone());
    let (intent, json) = intent_json(r#"file.exists(object_or_path="/tmp/aios-missing")"#)?;
    let registry = registry_with_systemd().await;
    let runtime = InMemoryCapabilityRuntime::new()
        .with_adapter_registry(registry)
        .with_verification_engine(adapter);

    let ctx = runtime
        .submit_action(&envelope_with_intent(json), &runtime_context())
        .await?;
    let result = engine
        .get_result(&intent.intent_id)
        .await
        .expect("completed verification result");

    assert_eq!(ctx.status, ActionLifecycleState::Failed);
    assert_eq!(ctx.error, Some(ExecutionFailureReason::AdapterRefused));
    assert_eq!(result.status, VerificationStatus::Failed);
    assert_eq!(verification_log.len().await, 2);
    assert_eq!(
        result.per_primitive[0].primitive_kind,
        VerificationPrimitive::FileExists
    );
    Ok(())
}
