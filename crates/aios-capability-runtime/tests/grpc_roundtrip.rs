//! T-033 — end-to-end gRPC roundtrip integration test for `CapabilityRuntime`.
//!
//! Spins up a tonic server backed by [`InMemoryCapabilityRuntime`] on a
//! random localhost port, builds a tonic client against that address, and
//! exercises the nine-RPC closed surface from S10.1 §5:
//!
//! - `ValidateAction` — happy path with a clean envelope (lifecycle =
//!   `CREATED`); empty bytes → `InvalidArgument`.
//! - `ExecuteAction` — happy path drives the full `submit_action` pipeline
//!   (lifecycle = `SUCCEEDED`); a `Decision::Deny` from the attached policy
//!   kernel surfaces as lifecycle `POLICY_DENIED` on the response.
//! - `GetActionStatus` — known id returns the persisted context; unknown id
//!   returns `Code::NotFound`.
//! - `ListAdapters` — empty when no registry attached; populated when a
//!   registry is wired.
//! - `GetAdapterCapabilities` — known id returns the manifest; unknown id
//!   returns `Code::NotFound`.
//! - `GetCapabilityRuntimeInfo` — schema list + runtime id + degraded flag.
//! - `EvaluatePolicyForAction`, `RequestApprovalForAction`, `VerifyAction`,
//!   `RollbackAction` — stubbed, return `Code::Unimplemented` per the T-033
//!   baseline.
//!
//! Also exercises:
//! - The full `RuntimeError → tonic::Status` mapping table at the wire
//!   boundary (constructed via `ActionContext` projection + the conversions
//!   module's mapping function).
//! - The `ActionLifecycleState`, `AdapterManifest`, `ActionContext`,
//!   `RegisteredAdapter` round-trips on the wire.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::significant_drop_tightening,
    clippy::items_after_statements,
    clippy::result_large_err,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_capability_runtime::adapter_manifest::{AdapterActionDeclaration, AdapterManifest};
use aios_capability_runtime::adapter_registry::{
    canonical_signed_manifest_bytes, encode_hex_signature, InMemoryAdapterRegistry,
};
use aios_capability_runtime::dispatch::{ActionDispatchKind, AdapterIOMode, AdapterStability};
use aios_capability_runtime::failure::RuntimeErrorCode;
use aios_capability_runtime::runtime::InMemoryCapabilityRuntime;
use aios_capability_runtime::service::conversions::{
    action_context_from_proto, action_context_to_proto, adapter_manifest_from_proto,
    adapter_manifest_to_proto, envelope_to_bytes, lifecycle_state_from_proto,
    registered_adapter_from_proto, registered_adapter_to_proto, runtime_error_code_from_proto,
    runtime_error_to_code, runtime_error_to_status,
};
use aios_capability_runtime::service::proto::{
    self, capability_runtime_client::CapabilityRuntimeClient,
    capability_runtime_server::CapabilityRuntimeServer,
};
use aios_capability_runtime::service::{CapabilityRuntimeService, SCHEMA_VERSION};
use aios_capability_runtime::ActionLifecycleState;
use aios_capability_runtime::QueueClass;
use aios_capability_runtime::RuntimeError;

use aios_policy::{
    ApprovalRequirement, ApprovalScope, Constraints, Decision, PolicyContext, PolicyDecision,
    PolicyError, PolicyKernel,
};

// ---------------------------------------------------------------------------
// Test harness helpers (mirror aios-policy T-023 / aios-evidence T-011 pattern).
// ---------------------------------------------------------------------------

async fn pick_port() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind random port");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);
    addr
}

async fn spawn_server(
    svc: CapabilityRuntimeService,
) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let addr = pick_port().await;
    let (tx, rx) = oneshot::channel::<()>();
    let server = tonic::transport::Server::builder().add_service(CapabilityRuntimeServer::new(svc));
    let handle = tokio::spawn(async move {
        server
            .serve_with_shutdown(addr, async move {
                let _ = rx.await;
            })
            .await
            .expect("server task");
    });
    // Yield once so the server has a chance to bind before the client
    // attempts to connect.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (addr, tx, handle)
}

fn clean_envelope() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("human:lucky", false),
        Request::new("service.status", serde_json::json!({"service": "nginx"})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn make_signed_manifest(
    adapter_id: &str,
    action_kind: &str,
) -> (AdapterManifest, VerifyingKey, String) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let signing_key_id = format!("key-{adapter_id}");
    let now = Utc::now();
    let mut manifest = AdapterManifest {
        adapter_id: adapter_id.to_owned(),
        adapter_version: "0.1.0".to_owned(),
        vendor: "test".to_owned(),
        name: "test-adapter".to_owned(),
        declared_stability: AdapterStability::Stable,
        io_mode: AdapterIOMode::TypedParametersOnly,
        dispatch_kind: ActionDispatchKind::SubprocessFork,
        declared_actions: vec![AdapterActionDeclaration {
            action_kind: action_kind.to_owned(),
            target_schema: serde_json::json!({"type": "object"}),
            response_schema: serde_json::json!({"type": "object"}),
            rollback_strategy: "NONE".to_owned(),
            timeout_seconds: 60,
            template_string: None,
            template_substitution_variables: Vec::new(),
        }],
        declared_invariants_supported: vec!["INV-013".to_owned()],
        default_adapter_timeout_seconds: 60,
        default_sandbox_profile_id: "sandbox.default".to_owned(),
        adapter_signature: String::new(), // filled below
        signing_key_id: signing_key_id.clone(),
        manifest_created_at: now,
        manifest_expires_at: now + ChronoDuration::days(30),
    };
    let body = canonical_signed_manifest_bytes(&manifest).expect("signed body");
    let sig = signing_key.sign(&body);
    let sig_bytes: [u8; 64] = sig.to_bytes();
    manifest.adapter_signature = encode_hex_signature(&sig_bytes);
    (manifest, verifying_key, signing_key_id)
}

async fn make_registry_with_one_adapter(
    adapter_id: &str,
    action_kind: &str,
) -> Arc<InMemoryAdapterRegistry> {
    let (manifest, vk, key_id) = make_signed_manifest(adapter_id, action_kind);
    let mut trust = HashMap::new();
    trust.insert(key_id, vk);
    let registry = Arc::new(InMemoryAdapterRegistry::new(trust));
    registry
        .register(manifest, Utc::now())
        .await
        .expect("register adapter");
    registry
}

// ===========================================================================
// 1. ValidateAction — happy path.
// ===========================================================================
#[tokio::test]
async fn validate_action_happy_path_returns_created_state() {
    let runtime = Arc::new(InMemoryCapabilityRuntime::new());
    let svc = CapabilityRuntimeService::new(runtime).with_runtime_id("test-runtime");
    let (addr, shutdown, handle) = spawn_server(svc).await;

    let mut client = CapabilityRuntimeClient::connect(format!("http://{addr}"))
        .await
        .expect("client connect");

    let env = clean_envelope();
    let req = proto::ValidateActionRequest {
        envelope_proto: envelope_to_bytes(&env).expect("encode envelope"),
    };
    let resp = client.validate_action(req).await.expect("ok").into_inner();
    let p_state = proto::ActionLifecycleState::try_from(resp.state).expect("known state");
    assert_eq!(
        lifecycle_state_from_proto(p_state),
        Some(ActionLifecycleState::Created)
    );
    assert!(resp.action_request_id.starts_with("actrq_"));
    assert_eq!(
        runtime_error_code_from_proto(
            proto::RuntimeErrorCode::try_from(resp.error).expect("known code"),
        ),
        RuntimeErrorCode::RuntimeOk
    );

    let _ = shutdown.send(());
    handle.await.expect("server join");
}

// ===========================================================================
// 2. ValidateAction — empty envelope_proto rejected with InvalidArgument.
// ===========================================================================
#[tokio::test]
async fn validate_action_empty_envelope_returns_invalid_argument() {
    let runtime = Arc::new(InMemoryCapabilityRuntime::new());
    let svc = CapabilityRuntimeService::new(runtime);
    let (addr, shutdown, handle) = spawn_server(svc).await;

    let mut client = CapabilityRuntimeClient::connect(format!("http://{addr}"))
        .await
        .expect("client connect");

    let req = proto::ValidateActionRequest {
        envelope_proto: Vec::new(),
    };
    let err = client.validate_action(req).await.expect_err("must reject");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    let _ = shutdown.send(());
    handle.await.expect("server join");
}

// ===========================================================================
// 3. ExecuteAction — happy path drives the full pipeline to SUCCEEDED.
// ===========================================================================
#[tokio::test]
async fn execute_action_happy_path_drives_pipeline_to_succeeded() {
    let runtime = Arc::new(InMemoryCapabilityRuntime::new());
    let svc = CapabilityRuntimeService::new(runtime);
    let (addr, shutdown, handle) = spawn_server(svc).await;

    let mut client = CapabilityRuntimeClient::connect(format!("http://{addr}"))
        .await
        .expect("client connect");

    let env = clean_envelope();
    let req = proto::ExecuteActionRequest {
        action_request_id: String::new(),
        envelope_proto: envelope_to_bytes(&env).expect("encode envelope"),
    };
    let resp = client.execute_action(req).await.expect("ok").into_inner();
    let p_state = proto::ActionLifecycleState::try_from(resp.state).expect("known state");
    assert_eq!(
        lifecycle_state_from_proto(p_state),
        Some(ActionLifecycleState::Succeeded)
    );
    assert!(resp.context.is_some(), "context populated");
    let ctx = resp.context.expect("context");
    assert!(!ctx.action_id.is_empty());

    let _ = shutdown.send(());
    handle.await.expect("server join");
}

// ===========================================================================
// 4. ExecuteAction — POLICY_DENIED short-circuits via attached PolicyKernel.
// ===========================================================================

/// Scripted kernel that always denies.
#[derive(Debug)]
struct DenyKernel;

#[async_trait]
impl PolicyKernel for DenyKernel {
    async fn evaluate_policy(
        &self,
        envelope: &ActionEnvelope,
        context: &PolicyContext,
    ) -> Result<PolicyDecision, PolicyError> {
        let _ = (envelope, context);
        Ok(PolicyDecision {
            policy_decision_id: format!("poldec_{}", ulid::Ulid::new()),
            action_id: aios_action::ActionId::new(),
            request_hash: "test_hash".to_owned(),
            bundle_version: context.bundle_version.clone(),
            enrichment_snapshot_id: "snap_test".to_owned(),
            decision: Decision::Deny,
            reason_code: "HardDeny".to_owned(),
            reason_message: "scripted deny".to_owned(),
            constraints: Constraints::default(),
            approval: ApprovalRequirement {
                required: false,
                approval_scope: ApprovalScope::ExactRequestHash,
                ttl_seconds: 0,
                approver_classes: Vec::new(),
                require_human_co_signer: false,
            },
            evidence_receipt_id: String::new(),
            evaluated_at: Utc::now(),
            rules_consulted: 1,
            simulated: false,
        })
    }
}

#[tokio::test]
async fn execute_action_with_deny_kernel_returns_policy_denied() {
    let kernel: Arc<dyn PolicyKernel> = Arc::new(DenyKernel);
    let runtime = Arc::new(InMemoryCapabilityRuntime::new().with_policy_kernel(kernel));
    let svc = CapabilityRuntimeService::new(runtime);
    let (addr, shutdown, handle) = spawn_server(svc).await;

    let mut client = CapabilityRuntimeClient::connect(format!("http://{addr}"))
        .await
        .expect("client connect");

    let env = clean_envelope();
    let req = proto::ExecuteActionRequest {
        action_request_id: String::new(),
        envelope_proto: envelope_to_bytes(&env).expect("encode envelope"),
    };
    let resp = client.execute_action(req).await.expect("ok").into_inner();
    let p_state = proto::ActionLifecycleState::try_from(resp.state).expect("known state");
    assert_eq!(
        lifecycle_state_from_proto(p_state),
        Some(ActionLifecycleState::PolicyDenied),
        "policy DENY must surface as POLICY_DENIED lifecycle"
    );

    let _ = shutdown.send(());
    handle.await.expect("server join");
}

// ===========================================================================
// 5. GetActionStatus — known id returns context; unknown id is NotFound.
// ===========================================================================
#[tokio::test]
async fn get_action_status_happy_path_and_not_found() {
    let runtime = Arc::new(InMemoryCapabilityRuntime::new());
    let svc = CapabilityRuntimeService::new(runtime);
    let (addr, shutdown, handle) = spawn_server(svc).await;

    let mut client = CapabilityRuntimeClient::connect(format!("http://{addr}"))
        .await
        .expect("client connect");

    // Submit one action so the runtime has at least one persisted context.
    let env = clean_envelope();
    let exec = client
        .execute_action(proto::ExecuteActionRequest {
            action_request_id: String::new(),
            envelope_proto: envelope_to_bytes(&env).expect("encode envelope"),
        })
        .await
        .expect("ok")
        .into_inner();
    let action_id = exec.context.expect("ctx").action_id;

    // Happy path: known id.
    let resp = client
        .get_action_status(proto::GetActionStatusRequest {
            action_request_id: action_id.clone(),
        })
        .await
        .expect("ok")
        .into_inner();
    assert!(resp.context.is_some());
    assert_eq!(resp.context.expect("ctx").action_id, action_id);

    // Unknown id (well-formed but absent).
    let unknown = aios_action::ActionId::new().to_string();
    let err = client
        .get_action_status(proto::GetActionStatusRequest {
            action_request_id: unknown,
        })
        .await
        .expect_err("must be not found");
    assert_eq!(err.code(), tonic::Code::NotFound);

    let _ = shutdown.send(());
    handle.await.expect("server join");
}

// ===========================================================================
// 6. ListAdapters — empty without registry; populated with one when wired.
// ===========================================================================
#[tokio::test]
async fn list_adapters_reflects_registry_contents() {
    let runtime = Arc::new(InMemoryCapabilityRuntime::new());

    // Without a registry: empty list.
    let svc = CapabilityRuntimeService::new(runtime.clone());
    let (addr, shutdown, handle) = spawn_server(svc).await;
    let mut client = CapabilityRuntimeClient::connect(format!("http://{addr}"))
        .await
        .expect("client connect");
    let resp = client
        .list_adapters(proto::ListAdaptersRequest {
            action_kind_filter: String::new(),
            stability_filter: 0,
            include_retired: false,
        })
        .await
        .expect("ok")
        .into_inner();
    assert!(resp.entries.is_empty());
    let _ = shutdown.send(());
    handle.await.expect("server join");

    // With a registry: one entry.
    let registry = make_registry_with_one_adapter("adapter:test:t1:0.1.0", "service.status").await;
    let svc = CapabilityRuntimeService::new(runtime).with_adapter_registry(registry);
    let (addr, shutdown, handle) = spawn_server(svc).await;
    let mut client = CapabilityRuntimeClient::connect(format!("http://{addr}"))
        .await
        .expect("client connect");
    let resp = client
        .list_adapters(proto::ListAdaptersRequest {
            action_kind_filter: String::new(),
            stability_filter: 0,
            include_retired: false,
        })
        .await
        .expect("ok")
        .into_inner();
    assert_eq!(resp.entries.len(), 1);
    let entry = &resp.entries[0];
    assert_eq!(
        entry
            .manifest
            .as_ref()
            .map_or("", |m| m.adapter_id.as_str()),
        "adapter:test:t1:0.1.0"
    );
    let _ = shutdown.send(());
    handle.await.expect("server join");
}

// ===========================================================================
// 7. GetAdapterCapabilities — known id round-trip + unknown id → NotFound.
// ===========================================================================
#[tokio::test]
async fn get_adapter_capabilities_returns_manifest_or_not_found() {
    let runtime = Arc::new(InMemoryCapabilityRuntime::new());
    let registry = make_registry_with_one_adapter("adapter:test:t2:0.1.0", "service.restart").await;
    let svc = CapabilityRuntimeService::new(runtime).with_adapter_registry(registry);
    let (addr, shutdown, handle) = spawn_server(svc).await;
    let mut client = CapabilityRuntimeClient::connect(format!("http://{addr}"))
        .await
        .expect("client connect");

    // Known id.
    let resp = client
        .get_adapter_capabilities(proto::GetAdapterCapabilitiesRequest {
            adapter_id: "adapter:test:t2:0.1.0".to_owned(),
        })
        .await
        .expect("ok")
        .into_inner();
    assert!(resp.manifest.is_some());
    let m = resp.manifest.expect("manifest");
    assert_eq!(m.adapter_id, "adapter:test:t2:0.1.0");

    // Unknown id.
    let err = client
        .get_adapter_capabilities(proto::GetAdapterCapabilitiesRequest {
            adapter_id: "adapter:nope:nope:0.0.0".to_owned(),
        })
        .await
        .expect_err("must be not found");
    assert_eq!(err.code(), tonic::Code::NotFound);

    let _ = shutdown.send(());
    handle.await.expect("server join");
}

// ===========================================================================
// 8. GetCapabilityRuntimeInfo — schema list + runtime id.
// ===========================================================================
#[tokio::test]
async fn get_capability_runtime_info_returns_schema_and_runtime_id() {
    let runtime = Arc::new(InMemoryCapabilityRuntime::new());
    let svc = CapabilityRuntimeService::new(runtime).with_runtime_id("test-runtime-info");
    let (addr, shutdown, handle) = spawn_server(svc).await;
    let mut client = CapabilityRuntimeClient::connect(format!("http://{addr}"))
        .await
        .expect("client connect");

    let resp = client
        .get_capability_runtime_info(())
        .await
        .expect("ok")
        .into_inner();
    assert_eq!(resp.capability_runtime_id, "test-runtime-info");
    assert!(resp
        .supported_schema_versions
        .contains(&SCHEMA_VERSION.to_owned()));
    assert!(!resp.degraded);

    let _ = shutdown.send(());
    handle.await.expect("server join");
}

// ===========================================================================
// 9. Stubbed RPCs return Unimplemented (T-033 baseline).
// ===========================================================================
#[tokio::test]
async fn stubbed_rpcs_return_unimplemented() {
    let runtime = Arc::new(InMemoryCapabilityRuntime::new());
    let svc = CapabilityRuntimeService::new(runtime);
    let (addr, shutdown, handle) = spawn_server(svc).await;
    let mut client = CapabilityRuntimeClient::connect(format!("http://{addr}"))
        .await
        .expect("client connect");

    let err = client
        .evaluate_policy_for_action(proto::EvaluatePolicyForActionRequest {
            action_request_id: String::new(),
            envelope_proto: Vec::new(),
        })
        .await
        .expect_err("stubbed");
    assert_eq!(err.code(), tonic::Code::Unimplemented);

    let err = client
        .request_approval_for_action(proto::RequestApprovalForActionRequest {
            action_request_id: String::new(),
        })
        .await
        .expect_err("stubbed");
    assert_eq!(err.code(), tonic::Code::Unimplemented);

    let err = client
        .verify_action(proto::VerifyActionRequest {
            action_request_id: String::new(),
        })
        .await
        .expect_err("stubbed");
    assert_eq!(err.code(), tonic::Code::Unimplemented);

    let err = client
        .rollback_action(proto::RollbackActionRequest {
            action_request_id: String::new(),
        })
        .await
        .expect_err("stubbed");
    assert_eq!(err.code(), tonic::Code::Unimplemented);

    let _ = shutdown.send(());
    handle.await.expect("server join");
}

// ===========================================================================
// 10. ActionContext, AdapterManifest, RegisteredAdapter wire round-trips.
// ===========================================================================
#[test]
fn action_context_wire_round_trip() {
    let now = Utc::now();
    let original = aios_capability_runtime::ActionContext::new(
        aios_action::ActionId::new(),
        ActionDispatchKind::DryRun,
        QueueClass::AgentProposal,
        now,
    );
    let p = action_context_to_proto(&original);
    let back = action_context_from_proto(&p).expect("round trip");
    assert_eq!(original.action_id, back.action_id);
    assert_eq!(original.status, back.status);
    assert_eq!(original.dispatch_kind, back.dispatch_kind);
    assert_eq!(original.queue_class, back.queue_class);
    assert_eq!(original.error, back.error);
    assert_eq!(original.rollback_outcome, back.rollback_outcome);
}

#[test]
fn adapter_manifest_wire_round_trip() {
    let (m, _vk, _key_id) = make_signed_manifest("adapter:rt:test:0.1.0", "service.status");
    let p = adapter_manifest_to_proto(&m);
    let back = adapter_manifest_from_proto(&p).expect("round trip");
    assert_eq!(m.adapter_id, back.adapter_id);
    assert_eq!(m.io_mode, back.io_mode);
    assert_eq!(m.dispatch_kind, back.dispatch_kind);
    assert_eq!(m.declared_actions.len(), back.declared_actions.len());
    assert_eq!(m.adapter_signature, back.adapter_signature);
}

#[test]
fn registered_adapter_wire_round_trip() {
    let (m, _vk, _key_id) = make_signed_manifest("adapter:rt:reg:0.1.0", "service.status");
    let reg = aios_capability_runtime::adapter_registry::RegisteredAdapter {
        manifest: m,
        registered_at: Utc::now(),
    };
    let p = registered_adapter_to_proto(&reg);
    let back = registered_adapter_from_proto(&p).expect("round trip");
    assert_eq!(reg.manifest.adapter_id, back.manifest.adapter_id);
}

// ===========================================================================
// 11. RuntimeError → tonic::Status mapping table (every variant).
// ===========================================================================
#[test]
fn runtime_error_status_mapping_table_full_coverage() {
    use aios_capability_runtime::dispatch::QueueClass;

    let cases: Vec<(RuntimeError, tonic::Code, RuntimeErrorCode)> = vec![
        (
            RuntimeError::ActionNotFound(aios_action::ActionId::new()),
            tonic::Code::NotFound,
            RuntimeErrorCode::RuntimeInternal,
        ),
        (
            RuntimeError::InvalidTransition {
                from: ActionLifecycleState::Succeeded,
                to: ActionLifecycleState::Executing,
            },
            tonic::Code::FailedPrecondition,
            RuntimeErrorCode::LifecycleIllegalTransition,
        ),
        (
            RuntimeError::AdapterUnknown("x".into()),
            tonic::Code::NotFound,
            RuntimeErrorCode::UnknownAdapter,
        ),
        (
            RuntimeError::AdapterSignatureInvalid,
            tonic::Code::PermissionDenied,
            RuntimeErrorCode::ManifestSignatureInvalid,
        ),
        (
            RuntimeError::AdapterUnknownAuthority("k".into()),
            tonic::Code::PermissionDenied,
            RuntimeErrorCode::ManifestSignatureInvalid,
        ),
        (
            RuntimeError::AdapterAlreadyRegistered("x".into()),
            tonic::Code::AlreadyExists,
            RuntimeErrorCode::RuntimeInternal,
        ),
        (
            RuntimeError::ManifestInvalid("bad".into()),
            tonic::Code::InvalidArgument,
            RuntimeErrorCode::ManifestSignatureInvalid,
        ),
        (
            RuntimeError::QueueFull(QueueClass::Interactive),
            tonic::Code::ResourceExhausted,
            RuntimeErrorCode::QueueBackpressureRejected,
        ),
        (
            RuntimeError::RateLimited("agent:dev".into()),
            tonic::Code::ResourceExhausted,
            RuntimeErrorCode::QueueBackpressureRejected,
        ),
        (
            RuntimeError::PolicyEvalFailed("kernel down".into()),
            tonic::Code::FailedPrecondition,
            RuntimeErrorCode::PolicyDecisionUnavailable,
        ),
        (
            RuntimeError::EvidenceEmitFailed("stall".into()),
            tonic::Code::Internal,
            RuntimeErrorCode::EvidenceLogUnavailable,
        ),
        (
            RuntimeError::Internal("boom".into()),
            tonic::Code::Internal,
            RuntimeErrorCode::RuntimeInternal,
        ),
    ];
    for (err, expected_code, expected_runtime_code) in cases {
        let s = runtime_error_to_status(&err);
        assert_eq!(
            s.code(),
            expected_code,
            "RuntimeError={err:?} → expected gRPC code {expected_code:?}"
        );
        assert_eq!(
            runtime_error_to_code(&err),
            expected_runtime_code,
            "RuntimeError={err:?} → expected RuntimeErrorCode {expected_runtime_code:?}"
        );
    }
}
