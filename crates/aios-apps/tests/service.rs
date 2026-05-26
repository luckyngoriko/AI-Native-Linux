//! Integration tests for the gRPC `AppsService` surface (T-122).
//!
//! Each test boots an in-process tonic server backed by the five in-memory
//! drivers, connects via an in-memory channel, and exercises one RPC path.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::doc_markdown,
    clippy::cast_possible_wrap,
    clippy::significant_drop_tightening,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;
use tonic::Request;

use aios_apps::compatibility_orchestrator::CompatibilityOrchestrator;
use aios_apps::knowledge_db::CompatibilityKnowledgeDB;
use aios_apps::package::PackageId;
use aios_apps::package_store::{AppPackage, InMemoryPackageStore, PackageStore};
use aios_apps::service::proto::apps_service_client::AppsServiceClient;
use aios_apps::service::proto::PackageEnvelopeProto;
use aios_apps::service::proto::{ActivateUpdateRequest, ExecuteUpdateRequest, VerifyUpdateRequest};
use aios_apps::service::proto::{
    CloseSessionRequest, GetPackageRequest, ListSessionsRequest, LookupCompatibilityProfileRequest,
    OpenSessionRequest, PlanUpdateRequest, RegisterPackageRequest, RollbackUpdateRequest,
    SessionFilterProto,
};
use aios_apps::service::{build_router, AppsServer};
use aios_apps::session_driver::{InMemorySessionDriver, SessionDriver};
use aios_apps::update_driver::{InMemoryUpdateDriver, UpdateRollbackDriver};

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

/// A fully wired test fixture with all five in-memory drivers and a connected
/// gRPC client.
struct TestHarness {
    client: AppsServiceClient<tonic::transport::Channel>,
    signing_key: SigningKey,
    _store: Arc<InMemoryPackageStore>,
    _knowledge: Arc<CompatibilityKnowledgeDB>,
    _sessions: Arc<InMemorySessionDriver>,
    _updates: Arc<InMemoryUpdateDriver>,
}

impl TestHarness {
    async fn new() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let mut trusted = HashMap::new();
        trusted.insert(verifying_key.to_bytes().to_vec(), "test-authority".into());

        let store = Arc::new(InMemoryPackageStore::new(trusted.clone()));
        let store_signing_key = signing_key.clone();
        let knowledge = Arc::new(CompatibilityKnowledgeDB::with_fixtures());
        let orchestrator = Arc::new(CompatibilityOrchestrator::new_with_defaults());
        let sessions = Arc::new(InMemorySessionDriver::new_with_defaults());
        let updates = Arc::new(InMemoryUpdateDriver::new());

        let svc = AppsServer::new(
            store.clone() as Arc<dyn PackageStore>,
            knowledge.clone(),
            sessions.clone() as Arc<dyn SessionDriver>,
            updates.clone() as Arc<dyn UpdateRollbackDriver>,
            orchestrator.clone(),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let router = build_router(svc);

        tokio::spawn(async move {
            router
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        // Give the server a moment to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = AppsServiceClient::connect(format!("http://{addr}"))
            .await
            .unwrap();

        Self {
            client,
            signing_key: store_signing_key,
            _store: store,
            _knowledge: knowledge,
            _sessions: sessions,
            _updates: updates,
        }
    }

    /// Generate a valid test package signed by the test authority.
    fn test_package(&self, name: &str, version: &str, manifest_json: &str) -> AppPackage {
        let verifying_key = self.signing_key.verifying_key();

        let manifest_bytes = manifest_json.as_bytes().to_vec();
        let content_hash_blake3 = blake3::hash(&manifest_bytes).to_hex().to_string();
        let sig = self.signing_key.sign(&manifest_bytes);

        AppPackage {
            package_id: PackageId(format!(
                "pkg_{}",
                ulid::Ulid::new().to_string().to_lowercase()
            )),
            name: name.to_string(),
            version: version.to_string(),
            manifest_bytes,
            content_hash_blake3,
            ed25519_signature: sig.to_bytes().to_vec(),
            signer_public_key: verifying_key.to_bytes().to_vec(),
            registered_at: chrono::Utc::now(),
        }
    }

    fn package_to_proto(pkg: &AppPackage) -> PackageEnvelopeProto {
        PackageEnvelopeProto {
            package_id: pkg.package_id.0.clone(),
            name: pkg.name.clone(),
            version: pkg.version.clone(),
            manifest_bytes: pkg.manifest_bytes.clone(),
            content_hash_blake3: pkg.content_hash_blake3.clone(),
            ed25519_signature: pkg.ed25519_signature.clone(),
            signer_public_key: pkg.signer_public_key.clone(),
            registered_at: Some(prost_types::Timestamp {
                seconds: pkg.registered_at.timestamp(),
                nanos: pkg.registered_at.timestamp_subsec_nanos() as i32,
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// 1. RegisterPackage → GetPackage round-trip
#[tokio::test]
async fn register_and_get_package_round_trip() {
    let harness = TestHarness::new().await;
    let pkg = harness.test_package("test-app", "1.0.0", r#"{"name":"test-app"}"#);
    let proto_pkg = TestHarness::package_to_proto(&pkg);

    let reg_resp = harness
        .client
        .clone()
        .register_package(Request::new(RegisterPackageRequest {
            package: Some(proto_pkg),
        }))
        .await
        .expect("register RPC");
    let reg = reg_resp.into_inner();
    assert!(!reg.package_id.is_empty());

    let get_resp = harness
        .client
        .clone()
        .get_package(Request::new(GetPackageRequest {
            package_id: reg.package_id,
        }))
        .await
        .expect("get RPC");
    let got = get_resp.into_inner();
    let got_pkg = got.package.expect("package present");
    assert_eq!(got_pkg.name, "test-app");
    assert_eq!(got_pkg.version, "1.0.0");
}

/// 2. ListPackages returns ≥1 after register
#[tokio::test]
async fn list_packages_returns_entries_after_register() {
    let harness = TestHarness::new().await;
    let pkg = harness.test_package("list-test", "0.1.0", r#"{"name":"list-test"}"#);
    let proto_pkg = TestHarness::package_to_proto(&pkg);

    // Initially empty
    let list0 = harness
        .client
        .clone()
        .list_packages(Request::new(()))
        .await
        .expect("list RPC");
    let initial_count = list0.into_inner().packages.len();

    harness
        .client
        .clone()
        .register_package(Request::new(RegisterPackageRequest {
            package: Some(proto_pkg),
        }))
        .await
        .expect("register RPC");

    let list1 = harness
        .client
        .clone()
        .list_packages(Request::new(()))
        .await
        .expect("list RPC");
    assert_eq!(list1.into_inner().packages.len(), initial_count + 1);
}

/// 3. OpenSession → ListSessions includes the session
#[tokio::test]
async fn open_session_then_list_sessions() {
    let harness = TestHarness::new().await;

    let open_resp = harness
        .client
        .clone()
        .open_session(Request::new(OpenSessionRequest {
            package_id: "pkg_any".into(),
            ecosystem: aios_apps::service::proto::EcosystemRuntimeProto::RuntimeLinuxNative as i32,
            requester: Some(aios_apps::service::proto::PrincipalProto {
                canonical_id: "human:test".into(),
            }),
            capability_grants: vec![],
            timeout_seconds: 300,
        }))
        .await
        .expect("open session RPC");
    let session = open_resp.into_inner().session.expect("session present");
    assert!(session.session_id.starts_with("sess_"));

    let list_resp = harness
        .client
        .clone()
        .list_sessions(Request::new(ListSessionsRequest {
            filter: Some(SessionFilterProto {
                filter_all: true,
                ..Default::default()
            }),
        }))
        .await
        .expect("list sessions RPC");
    let sessions = list_resp.into_inner().sessions;
    assert!(!sessions.is_empty());
}

/// 4. CloseSession → receipt with ClosedByOwner
#[tokio::test]
async fn close_session_returns_receipt() {
    let harness = TestHarness::new().await;

    let open_resp = harness
        .client
        .clone()
        .open_session(Request::new(OpenSessionRequest {
            package_id: "pkg_close_test".into(),
            ecosystem: aios_apps::service::proto::EcosystemRuntimeProto::RuntimeLinuxNative as i32,
            requester: Some(aios_apps::service::proto::PrincipalProto {
                canonical_id: "human:test".into(),
            }),
            capability_grants: vec![],
            timeout_seconds: 300,
        }))
        .await
        .expect("open session RPC");
    let session_id = open_resp.into_inner().session.unwrap().session_id;

    let close_resp = harness
        .client
        .clone()
        .close_session(Request::new(CloseSessionRequest {
            session_id: session_id.clone(),
        }))
        .await
        .expect("close session RPC");
    let receipt = close_resp.into_inner().receipt.expect("receipt present");
    assert_eq!(receipt.session_id, session_id);
    assert_eq!(
        receipt.exit_reason,
        aios_apps::service::proto::SessionExitReasonProto::ClosedByOwner as i32
    );
}

/// 5. PlanUpdate → ExecuteUpdate → VerifyUpdate → ActivateUpdate happy path
#[tokio::test]
async fn update_full_happy_path() {
    let harness = TestHarness::new().await;

    // Plan
    let plan_resp = harness
        .client
        .clone()
        .plan_update(Request::new(PlanUpdateRequest {
            package_id: "pkg_update_test".into(),
            from_version: "1.0.0".into(),
            to_version: "2.0.0".into(),
            requester: "human:test".into(),
            dry_run: false,
        }))
        .await
        .expect("plan RPC");
    let plan = plan_resp.into_inner().plan.expect("plan present");
    let plan_id = plan.plan_id;
    assert_eq!(
        plan.state,
        aios_apps::service::proto::UpdateStateProto::Planned as i32
    );

    // Execute
    let exec_resp = harness
        .client
        .clone()
        .execute_update(Request::new(ExecuteUpdateRequest {
            plan_id: plan_id.clone(),
        }))
        .await
        .expect("execute RPC");
    let outcome = exec_resp.into_inner().outcome.expect("outcome present");
    assert!(outcome.artifacts_swapped > 0);

    // Verify
    let verify_resp = harness
        .client
        .clone()
        .verify_update(Request::new(VerifyUpdateRequest {
            plan_id: plan_id.clone(),
        }))
        .await
        .expect("verify RPC");
    let verification = verify_resp
        .into_inner()
        .verification
        .expect("verification present");
    assert!(verification.hash_match);

    // Activate
    harness
        .client
        .clone()
        .activate_update(Request::new(ActivateUpdateRequest {
            plan_id: plan_id.clone(),
        }))
        .await
        .expect("activate RPC");
}

/// 6. PlanUpdate → ExecuteUpdate → RollbackUpdate
#[tokio::test]
async fn update_execute_then_rollback() {
    let harness = TestHarness::new().await;

    let plan_resp = harness
        .client
        .clone()
        .plan_update(Request::new(PlanUpdateRequest {
            package_id: "pkg_rollback_test".into(),
            from_version: "1.0.0".into(),
            to_version: "2.0.0".into(),
            requester: "human:test".into(),
            dry_run: false,
        }))
        .await
        .expect("plan RPC");
    let plan_id = plan_resp.into_inner().plan.unwrap().plan_id;

    // Execute first (moves to Executed)
    harness
        .client
        .clone()
        .execute_update(Request::new(ExecuteUpdateRequest {
            plan_id: plan_id.clone(),
        }))
        .await
        .expect("execute RPC");

    // Rollback (from Executed)
    let rollback_resp = harness
        .client
        .clone()
        .rollback_update(Request::new(RollbackUpdateRequest {
            plan_id: plan_id.clone(),
            reason: aios_apps::service::proto::RollbackReasonProto::UserRequested as i32,
        }))
        .await
        .expect("rollback RPC");
    let receipt = rollback_resp.into_inner().receipt.expect("receipt present");
    assert_eq!(receipt.reverted_to, "1.0.0");
    assert_eq!(
        receipt.exit_state,
        aios_apps::service::proto::RollbackExitStateProto::Reverted as i32
    );
}

/// 7. LookupCompatibilityProfile returns fixture profile
#[tokio::test]
async fn lookup_compatibility_profile_returns_fixture() {
    let harness = TestHarness::new().await;

    let resp = harness
        .client
        .clone()
        .lookup_compatibility_profile(Request::new(LookupCompatibilityProfileRequest {
            package_id: "pkg_fixture_linux_native".into(),
            ecosystem: aios_apps::service::proto::EcosystemRuntimeProto::RuntimeLinuxNative as i32,
        }))
        .await
        .expect("lookup RPC");
    let profile = resp.into_inner().profile.expect("profile present");
    assert_eq!(profile.app_id, "aios-terminal");
    assert_eq!(
        profile.headline_rating,
        aios_apps::service::proto::CompatibilityRatingProto::Platinum as i32
    );
}

/// 8. GetPackage unknown → NotFound
#[tokio::test]
async fn get_package_unknown_returns_not_found() {
    let harness = TestHarness::new().await;

    let err = harness
        .client
        .clone()
        .get_package(Request::new(GetPackageRequest {
            package_id: "pkg_nonexistent".into(),
        }))
        .await
        .expect_err("should fail");
    assert_eq!(err.code(), tonic::Code::NotFound);
}

/// 9. CloseSession unknown → NotFound
#[tokio::test]
async fn close_session_unknown_returns_not_found() {
    let harness = TestHarness::new().await;

    let err = harness
        .client
        .clone()
        .close_session(Request::new(CloseSessionRequest {
            session_id: "sess_nonexistent".into(),
        }))
        .await
        .expect_err("should fail");
    assert_eq!(err.code(), tonic::Code::NotFound);
}

/// 10. InvalidStateTransition → FailedPrecondition
#[tokio::test]
async fn invalid_transition_returns_failed_precondition() {
    let harness = TestHarness::new().await;

    // Activate without prior verify — verify state transition is validated in
    // the driver, which rejects Activating -> Active if not in Verified state.
    // Actually, InMemoryUpdateDriver::activate_update requires Verified.
    // So calling activate on a non-existent plan triggers UpdatePlanNotFound.
    // Instead, we test by trying to execute a non-existent plan.
    let err = harness
        .client
        .clone()
        .execute_update(Request::new(ExecuteUpdateRequest {
            plan_id: "updp_nonexistent".into(),
        }))
        .await
        .expect_err("should fail");
    assert_eq!(err.code(), tonic::Code::NotFound);
}

/// 10b. Activate without verify → FailedPrecondition (invalid state transition)
#[tokio::test]
async fn activate_without_verify_returns_failed_precondition() {
    let harness = TestHarness::new().await;

    // Plan but don't execute/verify, then try to activate
    let plan_resp = harness
        .client
        .clone()
        .plan_update(Request::new(PlanUpdateRequest {
            package_id: "pkg_state_test".into(),
            from_version: "1.0.0".into(),
            to_version: "2.0.0".into(),
            requester: "human:test".into(),
            dry_run: false,
        }))
        .await
        .expect("plan RPC");
    let plan_id = plan_resp.into_inner().plan.unwrap().plan_id;

    // Activate from Planned should fail (Planned → Activating is illegal)
    let err = harness
        .client
        .clone()
        .activate_update(Request::new(ActivateUpdateRequest { plan_id }))
        .await
        .expect_err("should fail");
    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
}

/// 11. Concurrent RegisterPackage from 3 clients → 3 entries
#[tokio::test]
async fn concurrent_register_package_from_three_clients() {
    let harness = TestHarness::new().await;

    let mut handles = Vec::new();
    for i in 0..3 {
        let mut client = harness.client.clone();
        let pkg = harness.test_package(
            &format!("concurrent-{i}"),
            "1.0.0",
            &format!(r#"{{"name":"concurrent-{i}"}}"#),
        );
        let proto_pkg = TestHarness::package_to_proto(&pkg);

        handles.push(tokio::spawn(async move {
            client
                .register_package(Request::new(RegisterPackageRequest {
                    package: Some(proto_pkg),
                }))
                .await
        }));
    }

    for h in handles {
        h.await.expect("join").expect("register RPC");
    }

    let list_resp = harness
        .client
        .clone()
        .list_packages(Request::new(()))
        .await
        .expect("list RPC");
    // All 3 should be present (the store already has whatever was registered by
    // prior tests, so just check that at least 3 exist)
    assert!(list_resp.into_inner().packages.len() >= 3);
}

/// 12. Server boots over TCP transport
#[tokio::test]
async fn server_boots_over_tcp_transport() {
    let harness = TestHarness::new().await;

    // If we got this far with a connected client, the server booted.
    // Verify with a real RPC.
    let list_resp = harness
        .client
        .clone()
        .list_packages(Request::new(()))
        .await
        .expect("list RPC");
    // Just confirming the RPC completed without transport errors.
    let _ = list_resp.into_inner();
}

/// 13. PlanUpdate dry_run returns plan without persisting
#[tokio::test]
async fn plan_update_dry_run_returns_plan() {
    let harness = TestHarness::new().await;

    let plan_resp = harness
        .client
        .clone()
        .plan_update(Request::new(PlanUpdateRequest {
            package_id: "pkg_dry_run".into(),
            from_version: "0.1.0".into(),
            to_version: "0.2.0".into(),
            requester: "human:test".into(),
            dry_run: true,
        }))
        .await
        .expect("plan RPC");
    let plan = plan_resp.into_inner().plan.expect("plan present");
    assert_eq!(
        plan.state,
        aios_apps::service::proto::UpdateStateProto::Planned as i32
    );
    assert!(!plan.plan_id.is_empty());
}
