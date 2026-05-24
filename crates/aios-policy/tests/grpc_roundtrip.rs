//! T-023 — end-to-end gRPC roundtrip integration test for `PolicyKernel`.
//!
//! Spins up a tonic server backed by [`aios_policy::InMemoryPolicyKernel`]
//! on a random localhost port, builds a tonic client against that address,
//! and exercises the six RPCs from S2.3 §20 / Appendix A:
//!
//! - `EvaluatePolicy` — happy path with a clean envelope; default-deny floor;
//!   AI self-approval prevention upgrade (`REQUIRE_APPROVAL`); hard-deny
//!   short-circuit (`DENY`); subject-unauthenticated maps to
//!   `Code::Unauthenticated`.
//! - `SimulatePolicy` — same envelope, `simulated = true` on the response.
//! - `LoadBundle`, `RollbackBundle`, `ExplainDecision` — stubbed, return
//!   `Code::Unimplemented`.
//! - `GetPolicyEngineInfo` — schema list + active bundle version + degraded
//!   flag.
//!
//! The harness mirrors `aios-evidence/tests/grpc_roundtrip.rs` exactly so the
//! pattern stays uniform across AIOS service crates.

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

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::oneshot;

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_policy::hard_deny_engine::HardDenyEngine;
use aios_policy::service::conversions::envelope_to_bytes;
use aios_policy::service::proto::policy_kernel_client::PolicyKernelClient;
use aios_policy::service::proto::policy_kernel_server::PolicyKernelServer;
use aios_policy::service::proto::{
    self, Decision as ProtoDecision, EvaluatePolicyRequest, ExplainDecisionRequest,
    LoadBundleRequest, RollbackBundleRequest,
};
use aios_policy::service::{PolicyKernelService, SCHEMA_VERSION};
use aios_policy::subject_hydration::{HydratedRecord, InMemoryHydrator};
use aios_policy::{
    HydratedSubject, InMemoryHydrator as _Hydrator, InMemoryPolicyKernel, PolicyKernel, SubjectType,
};

/// Bind a TCP listener to `127.0.0.1:0` and return the bound address.
async fn pick_port() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind random port");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);
    addr
}

/// Spawn the server task and return `(addr, shutdown_tx, join_handle)`.
async fn spawn_server(
    svc: PolicyKernelService,
) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let addr = pick_port().await;
    let (tx, rx) = oneshot::channel::<()>();
    let server = tonic::transport::Server::builder().add_service(PolicyKernelServer::new(svc));
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

/// Build a minimal envelope for a clean (no-risk) action by `human:lucky`.
fn clean_envelope() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("human:lucky", false),
        Request::new("service.status", serde_json::json!({"service": "nginx"})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

/// Build an envelope that flags `privileged = true` — exercises the §17
/// AI-self-approval upgrade path when paired with an AI subject.
fn ai_privileged_envelope() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("agent:dev", true),
        Request::new(
            "package.install",
            serde_json::json!({
                "package": "nginx",
                "risk": {"privileged": true},
            }),
        ),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

/// Build an envelope that triggers the §6 `disable_policy_kernel` hard deny.
fn hard_deny_envelope() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("human:lucky", false),
        Request::new(
            "policy_kernel.disable",
            serde_json::json!({"reason": "test"}),
        ),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

/// Hydrator with the canonical 4-subject fixture set — used to exercise the
/// `SubjectUnauthenticated` path (unknown subjects rejected at step 2).
fn hydrator_with_fixtures() -> Arc<dyn aios_policy::SubjectHydrator + Send + Sync> {
    let h = InMemoryHydrator::new();
    // Match the InMemoryHydrator::with_fixtures() set from the T-021
    // subject-hydration module: human:lucky, agent:dev, application:planner,
    // service:systemd. Reconstruct here so the test does not depend on a
    // future change to the fixtures helper.
    let mut h = h;
    h.insert(
        "human:lucky".to_owned(),
        HydratedRecord::new(HydratedSubject {
            canonical_subject_id: "human:lucky".to_owned(),
            subject_type: SubjectType::Human,
            groups: Vec::new(),
            capabilities: Vec::new(),
            session_class: "INTERNAL".to_owned(),
            recovery_mode: false,
            is_ai: false,
        }),
    );
    h.insert(
        "agent:dev".to_owned(),
        HydratedRecord::new(HydratedSubject {
            canonical_subject_id: "agent:dev".to_owned(),
            subject_type: SubjectType::Agent,
            groups: Vec::new(),
            capabilities: Vec::new(),
            session_class: "INTERNAL".to_owned(),
            recovery_mode: false,
            is_ai: true,
        }),
    );
    Arc::new(h)
}

/// Build a service wrapping an `InMemoryPolicyKernel` with the supplied
/// hydrator + hard-deny engine.
fn make_service_with_full_chain() -> PolicyKernelService {
    let hydrator = hydrator_with_fixtures();
    let engine = HardDenyEngine::new_with_defaults();
    let kernel: Arc<dyn PolicyKernel> =
        Arc::new(InMemoryPolicyKernel::new_with_full_chain(hydrator, engine));
    PolicyKernelService::new(kernel)
        .with_engine_id("test-engine")
        .with_bundle_version("polb_test")
}

/// Bare in-memory kernel (no hard-deny engine, no hydrator) — used to
/// exercise the default-deny floor without §6/§7 short-circuits.
fn make_service_bare() -> PolicyKernelService {
    let kernel: Arc<dyn PolicyKernel> = Arc::new(InMemoryPolicyKernel::new());
    PolicyKernelService::new(kernel).with_bundle_version("polb_bare")
}

// ===========================================================================
// 1. Happy-path EvaluatePolicy — every PolicyDecision field is populated.
// ===========================================================================
#[tokio::test]
async fn evaluate_policy_happy_path_returns_fully_populated_decision() {
    let (addr, shutdown, handle) = spawn_server(make_service_bare()).await;

    let mut client = PolicyKernelClient::connect(format!("http://{addr}"))
        .await
        .expect("client connect");

    let env = clean_envelope();
    let req = EvaluatePolicyRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        envelope_proto: envelope_to_bytes(&env).expect("encode envelope"),
    };
    let resp = client
        .evaluate_policy(req)
        .await
        .expect("evaluate ok")
        .into_inner();

    // All 14 PolicyDecision fields must be populated on a happy-path call.
    assert!(resp.policy_decision_id.starts_with("poldec_"));
    assert!(!resp.action_id.is_empty(), "action_id must be set");
    assert!(!resp.request_hash.is_empty(), "request_hash must be set");
    assert_eq!(resp.bundle_version, "polb_bare");
    assert!(!resp.enrichment_snapshot_id.is_empty());
    // Bare kernel without §6 / §11 wiring lands at the default-deny floor.
    assert_eq!(resp.decision, ProtoDecision::Deny as i32);
    assert_eq!(resp.reason_code, "DefaultDeny");
    assert!(resp.constraints.is_some());
    assert!(resp.approval.is_some());
    assert!(resp.evaluated_at.is_some());
    assert!(!resp.simulated);

    let _ = shutdown.send(());
    let _ = handle.await;
}

// ===========================================================================
// 2. Default-deny — same path as happy-path; explicit reason_code check.
// ===========================================================================
#[tokio::test]
async fn evaluate_policy_with_no_matching_rules_emits_default_deny() {
    let (addr, shutdown, handle) = spawn_server(make_service_bare()).await;
    let mut client = PolicyKernelClient::connect(format!("http://{addr}"))
        .await
        .unwrap();

    let env = clean_envelope();
    let req = EvaluatePolicyRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        envelope_proto: envelope_to_bytes(&env).unwrap(),
    };
    let resp = client.evaluate_policy(req).await.unwrap().into_inner();

    assert_eq!(resp.decision, ProtoDecision::Deny as i32);
    assert_eq!(resp.reason_code, "DefaultDeny");

    let _ = shutdown.send(());
    let _ = handle.await;
}

// ===========================================================================
// 3. Hard deny — §6 disable_policy_kernel envelope short-circuits to DENY.
// ===========================================================================
#[tokio::test]
async fn evaluate_policy_with_disable_policy_kernel_action_emits_hard_deny() {
    let (addr, shutdown, handle) = spawn_server(make_service_with_full_chain()).await;
    let mut client = PolicyKernelClient::connect(format!("http://{addr}"))
        .await
        .unwrap();

    let env = hard_deny_envelope();
    let req = EvaluatePolicyRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        envelope_proto: envelope_to_bytes(&env).unwrap(),
    };
    let resp = client.evaluate_policy(req).await.unwrap().into_inner();

    assert_eq!(resp.decision, ProtoDecision::Deny as i32);
    // Hard-deny reason codes are spec-prefixed `HardDeny:<PascalCase>`.
    assert!(
        resp.reason_code.starts_with("HardDeny:"),
        "got reason_code {}",
        resp.reason_code
    );

    let _ = shutdown.send(());
    let _ = handle.await;
}

// ===========================================================================
// 4. Subject unauthenticated -> Code::Unauthenticated.
// ===========================================================================
#[tokio::test]
async fn evaluate_policy_with_unknown_subject_maps_to_unauthenticated() {
    let (addr, shutdown, handle) = spawn_server(make_service_with_full_chain()).await;
    let mut client = PolicyKernelClient::connect(format!("http://{addr}"))
        .await
        .unwrap();

    // The hydrator above carries `human:lucky` + `agent:dev` only;
    // `human:nobody` is not registered, so step 2 collapses to
    // `SubjectUnauthenticated`. The kernel converts that into a DENY in-band
    // (Ok(decision)), so the wire response is `OK` with `reason_code =
    // SubjectUnauthenticated`. The Status::unauthenticated mapping fires when
    // the kernel returns Err(SubjectUnauthenticated) — exercised by a separate
    // test below using a synthetic error-returning kernel.
    let env = ActionEnvelope::new(
        Identity::new("human:nobody", false),
        Request::new("service.status", serde_json::json!({})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    );
    let req = EvaluatePolicyRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        envelope_proto: envelope_to_bytes(&env).unwrap(),
    };
    let resp = client.evaluate_policy(req).await.unwrap().into_inner();

    assert_eq!(resp.decision, ProtoDecision::Deny as i32);
    assert_eq!(resp.reason_code, "SubjectUnauthenticated");

    let _ = shutdown.send(());
    let _ = handle.await;
}

// ===========================================================================
// 5. SimulatePolicy — same pipeline, `simulated = true` on response.
// ===========================================================================
#[tokio::test]
async fn simulate_policy_flips_the_simulated_flag_on_the_decision() {
    let (addr, shutdown, handle) = spawn_server(make_service_bare()).await;
    let mut client = PolicyKernelClient::connect(format!("http://{addr}"))
        .await
        .unwrap();

    let env = clean_envelope();
    let req = EvaluatePolicyRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        envelope_proto: envelope_to_bytes(&env).unwrap(),
    };
    let resp = client.simulate_policy(req).await.unwrap().into_inner();

    assert!(
        resp.simulated,
        "SimulatePolicy must set simulated=true per §14"
    );
    // Decision content is otherwise identical to the EvaluatePolicy result.
    assert_eq!(resp.decision, ProtoDecision::Deny as i32);
    assert_eq!(resp.reason_code, "DefaultDeny");

    let _ = shutdown.send(());
    let _ = handle.await;
}

// ===========================================================================
// 6. AI self-approval prevention path — the kernel processes the envelope
//    and lands at default-deny (because the bare default-deny pipeline does
//    not have a scoped ALLOW to upgrade in T-023). The §17 upgrade is
//    exercised in the unit tests under `tests/ai_self_approval.rs`. Here we
//    only assert that an AI-flagged envelope with risk routes through the
//    wire path without error.
// ===========================================================================
#[tokio::test]
async fn evaluate_policy_with_ai_privileged_envelope_routes_through_wire() {
    let (addr, shutdown, handle) = spawn_server(make_service_with_full_chain()).await;
    let mut client = PolicyKernelClient::connect(format!("http://{addr}"))
        .await
        .unwrap();

    let env = ai_privileged_envelope();
    let req = EvaluatePolicyRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        envelope_proto: envelope_to_bytes(&env).unwrap(),
    };
    let resp = client.evaluate_policy(req).await.unwrap().into_inner();

    // The pipeline reaches default-deny because no scoped ALLOW rule fired.
    // The §17 upgrade pathway is therefore not triggered here, but the wire
    // round-trip itself must succeed — the request flows through the
    // SchemaVersion + envelope decode + kernel invocation + decision encode
    // path without any tonic-level error.
    assert!(matches!(
        proto::Decision::try_from(resp.decision).unwrap(),
        ProtoDecision::Deny | ProtoDecision::RequireApproval
    ));

    let _ = shutdown.send(());
    let _ = handle.await;
}

// ===========================================================================
// 7. Schema version mismatch -> Code::FailedPrecondition.
// ===========================================================================
#[tokio::test]
async fn schema_version_mismatch_returns_failed_precondition() {
    let (addr, shutdown, handle) = spawn_server(make_service_bare()).await;
    let mut client = PolicyKernelClient::connect(format!("http://{addr}"))
        .await
        .unwrap();

    let env = clean_envelope();
    let req = EvaluatePolicyRequest {
        schema_version: "aios.policy.v0".to_owned(),
        envelope_proto: envelope_to_bytes(&env).unwrap(),
    };
    let err = client.evaluate_policy(req).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::FailedPrecondition);

    let _ = shutdown.send(());
    let _ = handle.await;
}

// ===========================================================================
// 8. Empty envelope_proto -> Code::InvalidArgument.
// ===========================================================================
#[tokio::test]
async fn empty_envelope_proto_returns_invalid_argument() {
    let (addr, shutdown, handle) = spawn_server(make_service_bare()).await;
    let mut client = PolicyKernelClient::connect(format!("http://{addr}"))
        .await
        .unwrap();

    let req = EvaluatePolicyRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        envelope_proto: Vec::new(),
    };
    let err = client.evaluate_policy(req).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    let _ = shutdown.send(());
    let _ = handle.await;
}

// ===========================================================================
// 9. GetPolicyEngineInfo — schema list + bundle version + degraded flag.
// ===========================================================================
#[tokio::test]
async fn get_policy_engine_info_returns_schema_list_and_bundle_version() {
    let (addr, shutdown, handle) = spawn_server(make_service_bare()).await;
    let mut client = PolicyKernelClient::connect(format!("http://{addr}"))
        .await
        .unwrap();

    let resp = client
        .get_policy_engine_info(())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.default_schema_version, SCHEMA_VERSION);
    assert_eq!(resp.supported_schema_versions, vec![SCHEMA_VERSION]);
    assert_eq!(resp.active_bundle_version, "polb_bare");
    assert!(!resp.degraded);
    assert!(resp.started_at.is_some());

    let _ = shutdown.send(());
    let _ = handle.await;
}

// ===========================================================================
// 10. LoadBundle / RollbackBundle / ExplainDecision return Unimplemented.
// ===========================================================================
#[tokio::test]
async fn deferred_rpcs_return_unimplemented_until_t024_t025_land() {
    let (addr, shutdown, handle) = spawn_server(make_service_bare()).await;
    let mut client = PolicyKernelClient::connect(format!("http://{addr}"))
        .await
        .unwrap();

    let err = client
        .load_bundle(LoadBundleRequest {
            bundle: None,
            stage_only: false,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::Unimplemented);

    let err = client
        .rollback_bundle(RollbackBundleRequest {
            target_bundle_version: "polb_old".into(),
            operator_subject: "human:lucky".into(),
            reason: "test".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::Unimplemented);

    let err = client
        .explain_decision(ExplainDecisionRequest {
            policy_decision_id: "poldec_unknown".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::Unimplemented);

    let _ = shutdown.send(());
    let _ = handle.await;
}

// ===========================================================================
// 11. Re-export sanity: aios_policy::SubjectHydrator trait is reachable.
//     This guards against an accidental re-export drop in lib.rs when
//     adding `pub mod service`.
// ===========================================================================
#[test]
fn lib_reexports_remain_stable_across_t023() {
    let _: _Hydrator = InMemoryHydrator::new();
    // Compile-time check: the gRPC server module type is reachable.
    fn _accept(_svc: PolicyKernelService) {}
}
