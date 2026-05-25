//! T-061 integration coverage for renderer-side gRPC client wiring.

#![allow(
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::panic,
    clippy::result_large_err,
    clippy::too_many_lines,
    clippy::unwrap_used,
    reason = "integration tests use panic-on-failure assertions"
)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use aios_action::{ActionEnvelope, ActionId, Identity, Request, Trace};
use aios_capability_runtime::ActionContext;
use aios_fs::{ChunkId, ChunkRef, ObjectId, ObjectWriteRequest, SubjectRef as FsSubjectRef};
use aios_policy::{
    ApprovalRequirement, Constraints, Decision, PolicyContext, PolicyDecision, PolicyError,
    PolicyKernel,
};
use aios_renderer_cli::{AiosClient, AiosEndpoints, InProcessBackend, RenderError, ShutdownHandle};
use chrono::Utc;
use tokio::net::TcpListener;

fn clean_envelope() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("human:lucky", false),
        Request::new("service.status", serde_json::json!({"service": "nginx"})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn chunk_ref(bytes: &[u8]) -> ChunkRef {
    ChunkRef(ChunkId::from_hash_bytes(bytes))
}

fn write_request(name: &str) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: None,
        parent_version_ids: Vec::new(),
        chunks: vec![chunk_ref(name.as_bytes())],
        metadata_delta: serde_json::json!({
            "name": name,
            "mime": "text/plain"
        }),
        action_id: None,
        subject: FsSubjectRef("family:alice".to_owned()),
    }
}

#[derive(Debug)]
struct ScriptedPolicyKernel {
    decision: Decision,
    reason_code: &'static str,
}

impl ScriptedPolicyKernel {
    const fn new(decision: Decision, reason_code: &'static str) -> Self {
        Self {
            decision,
            reason_code,
        }
    }
}

impl PolicyKernel for ScriptedPolicyKernel {
    fn evaluate_policy<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 self,
        _envelope: &'life1 ActionEnvelope,
        context: &'life2 PolicyContext,
    ) -> Pin<Box<dyn Future<Output = Result<PolicyDecision, PolicyError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: 'async_trait,
    {
        let decision = self.decision;
        let reason_code = self.reason_code.to_owned();
        let bundle_version = context.bundle_version.clone();
        let enrichment_snapshot_id = context.enrichment.snapshot_id.clone();

        Box::pin(async move {
            Ok(PolicyDecision {
                policy_decision_id: "poldec_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
                action_id: ActionId::new(),
                request_hash: "0".repeat(64),
                bundle_version,
                enrichment_snapshot_id,
                decision,
                reason_code,
                reason_message: "scripted renderer-cli integration decision".to_owned(),
                constraints: Constraints::default(),
                approval: ApprovalRequirement::default(),
                evidence_receipt_id: String::new(),
                evaluated_at: Utc::now(),
                rules_consulted: 1,
                simulated: false,
            })
        })
    }
}

async fn unused_endpoint() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind random port");
    let addr = listener.local_addr().expect("local addr");
    drop(listener);
    format!("http://{addr}")
}

async fn scripted_client(
    decision: Decision,
    reason_code: &'static str,
) -> (AiosClient, ShutdownHandle) {
    let kernel: Arc<dyn PolicyKernel> = Arc::new(ScriptedPolicyKernel::new(decision, reason_code));
    InProcessBackend::spawn_and_connect_with_policy(kernel)
        .await
        .expect("spawn scripted in-process backend")
}

#[test]
fn localhost_default_returns_distinct_ports_per_service() {
    let endpoints = AiosEndpoints::localhost_default();
    let evidence = endpoints.evidence.expect("evidence endpoint");
    let mut values = vec![
        endpoints.policy,
        endpoints.runtime,
        endpoints.fs,
        endpoints.vault,
        evidence,
    ];
    values.sort();
    values.dedup();

    assert_eq!(values.len(), 5);
    assert!(values
        .iter()
        .all(|value| value.starts_with("http://[::1]:")));
}

#[tokio::test]
async fn spawn_and_connect_starts_four_backend_servers() {
    let (client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn and connect");

    assert_eq!(shutdown.service_count(), 4);
    assert!(!client.has_evidence_client());

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn submit_action_routes_to_runtime_and_returns_action_context() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn and connect");

    let context: ActionContext = client
        .submit_action(clean_envelope())
        .await
        .expect("submit action");

    assert!(!context.action_id.as_str().is_empty());

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn write_then_read_object_routes_through_fs_client() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn and connect");

    let written = client
        .write_object(write_request("created-through-aios-client"))
        .await
        .expect("write object");
    let object = client
        .read_object(written.object_id.as_ref())
        .await
        .expect("read object");

    assert_eq!(object.object_id, written.object_id);
    assert_eq!(object.metadata.name, "created-through-aios-client");

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn list_capabilities_routes_through_vault_client() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn and connect");

    let capabilities = client
        .list_capabilities("family:alice")
        .await
        .expect("list capabilities");

    assert_eq!(capabilities.len(), 1);
    assert_eq!(capabilities[0].issued_to.0, "family:alice");

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn policy_allow_path_routes_through_policy_grpc() {
    let (mut client, shutdown) = scripted_client(Decision::Allow, "ScopedAllow").await;

    let decision = client
        .evaluate_policy(clean_envelope())
        .await
        .expect("evaluate policy");

    assert_eq!(decision.decision, Decision::Allow);
    assert_eq!(decision.reason_code, "ScopedAllow");

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn policy_deny_path_routes_through_policy_grpc() {
    let (mut client, shutdown) = scripted_client(Decision::Deny, "ScriptedDeny").await;

    let decision = client
        .evaluate_policy(clean_envelope())
        .await
        .expect("evaluate policy");

    assert_eq!(decision.decision, Decision::Deny);
    assert_eq!(decision.reason_code, "ScriptedDeny");

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn connection_failure_maps_to_client_connect_failed() {
    let endpoint = unused_endpoint().await;
    let endpoints = AiosEndpoints {
        policy: endpoint.clone(),
        runtime: endpoint.clone(),
        fs: endpoint.clone(),
        vault: endpoint,
        evidence: None,
    };

    let err = AiosClient::connect(&endpoints)
        .await
        .expect_err("connect must fail");

    match err {
        RenderError::ClientConnectFailed { service, reason } => {
            assert_eq!(service, "policy");
            assert!(!reason.is_empty());
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn rpc_not_found_maps_to_client_call_failed() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn and connect");
    let missing = ObjectId::new().to_string();

    let err = client
        .read_object(&missing)
        .await
        .expect_err("missing object");

    match err {
        RenderError::ClientCallFailed {
            service,
            rpc,
            status,
        } => {
            assert_eq!(service, "fs");
            assert_eq!(rpc, "ReadObject");
            assert!(status.contains("NotFound"), "{status}");
        }
        other => panic!("unexpected error: {other:?}"),
    }

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn shutdown_handle_stops_all_four_servers() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn and connect");
    assert_eq!(shutdown.service_count(), 4);

    shutdown.shutdown().await.expect("shutdown");

    let err = client
        .submit_action(clean_envelope())
        .await
        .expect_err("runtime server is down");
    assert!(matches!(
        err,
        RenderError::ClientCallFailed { service, .. } if service == "runtime"
    ));
}
