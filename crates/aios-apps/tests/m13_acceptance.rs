//! T-126 — M13 acceptance E2E test.
//!
//! 4-phase end-to-end fixture exercising the full S12.x / S6.5 stack:
//! install → update → rollback → close, with evidence chain verification
//! and gRPC smoke calls.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::doc_markdown,
    clippy::cast_possible_wrap,
    clippy::significant_drop_tightening,
    clippy::future_not_send,
    clippy::too_many_lines,
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
use aios_apps::service::proto::{
    ActivateUpdateRequest, CloseSessionRequest, EcosystemRuntimeProto, ExecuteUpdateRequest,
    GetPackageRequest, ListSessionsRequest, LookupCompatibilityProfileRequest,
    OpenSessionRequest as GrpcOpenSession, PlanUpdateRequest, PrincipalProto,
    RegisterPackageRequest, RollbackUpdateRequest, SessionFilterProto, VerifyUpdateRequest,
};
use aios_apps::service::{build_router, AppsServer};
use aios_apps::session_driver::{
    InMemorySessionDriver, OpenSessionRequest, Principal, SessionDriver,
};
use aios_apps::update_driver::{
    InMemoryUpdateDriver, RollbackReason, UpdatePlanRequest, UpdateRollbackDriver,
};
use aios_apps::{EcosystemRuntime, InMemoryAppsEvidenceEmitter, SandboxBridge};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn signing_key() -> SigningKey {
    SigningKey::generate(&mut OsRng)
}

fn make_package(name: &str, version: &str, key: &SigningKey) -> AppPackage {
    let manifest = serde_json::json!({
        "name": name,
        "version": version,
        "kind": "application",
        "runtime_class": "flatpak",
    });
    let manifest_bytes = serde_json::to_vec(&manifest).expect("ser");
    let content_hash = blake3::hash(&manifest_bytes).to_hex().to_string();
    let sig = key.sign(&manifest_bytes);
    let verifying_key = key.verifying_key();
    AppPackage {
        package_id: PackageId(format!(
            "pkg_{}",
            ulid::Ulid::new().to_string().to_lowercase()
        )),
        name: name.to_string(),
        version: version.to_string(),
        manifest_bytes,
        content_hash_blake3: content_hash,
        ed25519_signature: sig.to_bytes().to_vec(),
        signer_public_key: verifying_key.to_bytes().to_vec(),
        registered_at: chrono::Utc::now(),
    }
}

fn emitter() -> Arc<InMemoryAppsEvidenceEmitter> {
    Arc::new(InMemoryAppsEvidenceEmitter::new("service:aios-apps"))
}

fn principal(id: &str) -> Principal {
    Principal {
        canonical_id: id.into(),
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

// =========================================================================
// Phase 1–4 E2E: install → update → rollback → close
// =========================================================================

#[tokio::test]
async fn m13_acceptance_full_e2e_install_update_rollback_close() {
    // --- Bootstrap ---
    let key = signing_key();
    let mut trusted = HashMap::new();
    trusted.insert(
        key.verifying_key().to_bytes().to_vec(),
        "m13-authority".into(),
    );

    let em = emitter();
    let _knowledge = CompatibilityKnowledgeDB::with_fixtures();
    let orchestrator = CompatibilityOrchestrator::new_with_defaults();

    // --- Phase 1: Install ---

    let store = InMemoryPackageStore::new(trusted).with_emitter(em.clone());
    let pkg = make_package("firefox", "120.0", &key);
    let pkg_id = store.register_package(pkg).await.expect("register");
    // Receipt 0: PACKAGE_REGISTERED

    let sessions = InMemorySessionDriver::new(orchestrator.clone()).with_emitter(em.clone());
    let desc = sessions
        .open_session(OpenSessionRequest {
            package_id: pkg_id.clone(),
            ecosystem: EcosystemRuntime::RuntimeLinuxNative,
            requester: principal("human:tester"),
            capability_grants: vec![],
            timeout: Duration::from_secs(300),
        })
        .await
        .expect("open session");
    // Receipt 1: SESSION_OPENED

    // Sandbox allocation
    let composer = Arc::new(aios_sandbox::InMemorySandboxComposer::new());
    let sandbox = SandboxBridge::new(composer);
    let _profile_id = sandbox
        .allocate_for_session(&pkg_id, EcosystemRuntime::RuntimeLinuxNative, &[])
        .await
        .expect("sandbox alloc");

    // --- Phase 2: Update ---

    let updates = InMemoryUpdateDriver::new().with_emitter(em.clone());
    let plan = updates
        .plan_update(UpdatePlanRequest {
            package_id: pkg_id.clone(),
            from_version: "120.0".into(),
            to_version: "121.0".into(),
            requester: principal("human:tester"),
            dry_run: false,
        })
        .await
        .expect("plan");

    let outcome = updates
        .execute_update(plan.id.clone())
        .await
        .expect("execute");
    assert_eq!(outcome.artifacts_swapped, 128);
    // Receipt 2: PACKAGE_UPDATE_EXECUTED

    let verification = updates
        .verify_update(plan.id.clone())
        .await
        .expect("verify");
    assert!(verification.hash_match);
    // Receipt 3: PACKAGE_UPDATE_VERIFIED

    updates
        .activate_update(plan.id.clone())
        .await
        .expect("activate");
    // Receipt 4: PACKAGE_UPDATE_ACTIVATED

    let active_plan = updates.get_update(plan.id.clone()).await.expect("get");
    assert_eq!(active_plan.state.to_string(), "Active");

    // --- Phase 3: Rollback ---

    let rollback = updates
        .rollback_update(plan.id.clone(), RollbackReason::RegressionDetected)
        .await
        .expect("rollback");
    assert_eq!(rollback.exit_state.to_string(), "Reverted");
    // Receipt 5: PACKAGE_UPDATE_ROLLED_BACK

    let rolled_back = updates.get_update(plan.id.clone()).await.expect("get");
    assert_eq!(rolled_back.state.to_string(), "RolledBack");

    // --- Phase 4: Close ---

    let termination = sessions
        .close_session(desc.session_id.clone())
        .await
        .expect("close");
    assert_eq!(termination.exit_reason.to_string(), "ClosedByOwner");
    // Receipt 6: SESSION_CLOSED

    // =========================================================================
    // Evidence chain assertions — 7 records
    // =========================================================================

    assert_eq!(
        em.receipt_count().await,
        7,
        "expected 7 evidence records: register + open + execute + verify + activate + rollback + close"
    );

    // Chain integrity (BLAKE3 hash linkage)
    em.verify_chain().await.expect("evidence chain integrity");

    // Sequence continuity
    for i in 0..7 {
        let payload = em
            .get_payload(i)
            .await
            .unwrap_or_else(|| panic!("receipt {i} present"));
        assert!(!payload.is_null(), "receipt {i} must have non-null payload");
    }

    // Record-level type assertions
    let p0 = em.get_payload(0).await.unwrap();
    assert!(
        p0["package_id"].as_str().is_some(),
        "PACKAGE_REGISTERED must have package_id"
    );

    let p1 = em.get_payload(1).await.unwrap();
    assert!(
        p1["session_id"].as_str().is_some(),
        "SESSION_OPENED must have session_id"
    );

    let p6 = em.get_payload(6).await.unwrap();
    assert_eq!(p6["exit_reason"], "CLOSED_BY_OWNER");
}

// =========================================================================
// gRPC smoke — all 12 RPCs reachable via AppsServer
// =========================================================================

struct GrpcHarness {
    client: AppsServiceClient<tonic::transport::Channel>,
    signing_key: SigningKey,
    _store: Arc<InMemoryPackageStore>,
    _knowledge: Arc<CompatibilityKnowledgeDB>,
    _sessions: Arc<InMemorySessionDriver>,
    _updates: Arc<InMemoryUpdateDriver>,
}

impl GrpcHarness {
    async fn new() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let mut trusted = HashMap::new();
        trusted.insert(verifying_key.to_bytes().to_vec(), "m13-grpc".into());

        let store = Arc::new(InMemoryPackageStore::new(trusted.clone()));
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

        tokio::time::sleep(Duration::from_millis(50)).await;

        let channel = tonic::transport::Endpoint::from_shared(format!("http://{addr}"))
            .unwrap()
            .connect()
            .await
            .unwrap();
        let client = AppsServiceClient::new(channel);

        Self {
            client,
            signing_key,
            _store: store,
            _knowledge: knowledge,
            _sessions: sessions,
            _updates: updates,
        }
    }
}

#[tokio::test]
async fn m13_acceptance_grpc_all_12_rpcs_smoke() {
    let h = GrpcHarness::new().await;

    // 1. register_package
    let pkg = make_package("test-app", "1.0", &h.signing_key);
    let proto_pkg = package_to_proto(&pkg);
    let reg_resp = h
        .client
        .clone()
        .register_package(Request::new(RegisterPackageRequest {
            package: Some(proto_pkg),
        }))
        .await
        .expect("register_package RPC");
    let registered_id = reg_resp.into_inner().package_id;
    assert!(!registered_id.is_empty());

    // 2. get_package
    let get_resp = h
        .client
        .clone()
        .get_package(Request::new(GetPackageRequest {
            package_id: registered_id.clone(),
        }))
        .await
        .expect("get_package RPC");
    let got_pkg = get_resp.into_inner().package.expect("package present");
    assert_eq!(got_pkg.name, "test-app");

    // 3. list_packages
    let list_resp = h
        .client
        .clone()
        .list_packages(Request::new(()))
        .await
        .expect("list_packages RPC");
    assert!(!list_resp.into_inner().packages.is_empty());

    // 4. open_session
    let open_req = GrpcOpenSession {
        package_id: registered_id.clone(),
        ecosystem: EcosystemRuntimeProto::RuntimeLinuxNative as i32,
        requester: Some(PrincipalProto {
            canonical_id: "human:smoke".into(),
        }),
        capability_grants: vec![],
        timeout_seconds: 300,
    };
    let open_resp = h
        .client
        .clone()
        .open_session(Request::new(open_req))
        .await
        .expect("open_session RPC");
    let session = open_resp.into_inner().session.expect("session present");
    assert!(!session.session_id.is_empty());
    let opened_session_id = session.session_id;

    // 5. close_session
    let close_resp = h
        .client
        .clone()
        .close_session(Request::new(CloseSessionRequest {
            session_id: opened_session_id.clone(),
        }))
        .await
        .expect("close_session RPC");
    let receipt = close_resp.into_inner().receipt.expect("receipt present");
    assert_eq!(receipt.session_id, opened_session_id);

    // 6. list_sessions
    let ls_resp = h
        .client
        .clone()
        .list_sessions(Request::new(ListSessionsRequest {
            filter: Some(SessionFilterProto {
                filter_all: true,
                ..Default::default()
            }),
        }))
        .await
        .expect("list_sessions RPC");
    assert!(!ls_resp.into_inner().sessions.is_empty());

    // 7. plan_update
    let plan_resp = h
        .client
        .clone()
        .plan_update(Request::new(PlanUpdateRequest {
            package_id: registered_id.clone(),
            from_version: "1.0".into(),
            to_version: "2.0".into(),
            requester: "human:smoke".into(),
            dry_run: false,
        }))
        .await
        .expect("plan_update RPC");
    let plan = plan_resp.into_inner().plan.expect("plan present");
    let plan_id = plan.plan_id;
    assert!(!plan_id.is_empty());

    // 8. execute_update
    let exec_resp = h
        .client
        .clone()
        .execute_update(Request::new(ExecuteUpdateRequest {
            plan_id: plan_id.clone(),
        }))
        .await
        .expect("execute_update RPC");
    let outcome = exec_resp.into_inner().outcome.expect("outcome present");
    assert!(outcome.artifacts_swapped > 0);

    // 9. verify_update
    let verify_resp = h
        .client
        .clone()
        .verify_update(Request::new(VerifyUpdateRequest {
            plan_id: plan_id.clone(),
        }))
        .await
        .expect("verify_update RPC");
    let verification = verify_resp
        .into_inner()
        .verification
        .expect("verification present");
    assert!(verification.hash_match);

    // 10. activate_update
    let _activate = h
        .client
        .clone()
        .activate_update(Request::new(ActivateUpdateRequest {
            plan_id: plan_id.clone(),
        }))
        .await
        .expect("activate_update RPC");

    // 11. rollback_update
    let rb_resp = h
        .client
        .clone()
        .rollback_update(Request::new(RollbackUpdateRequest {
            plan_id: plan_id.clone(),
            reason: aios_apps::service::proto::RollbackReasonProto::RegressionDetected as i32,
        }))
        .await
        .expect("rollback_update RPC");
    let rb_receipt = rb_resp.into_inner().receipt.expect("receipt present");
    assert!(!rb_receipt.plan_id.is_empty());

    // 12. lookup_compatibility_profile
    let lkp_resp = h
        .client
        .clone()
        .lookup_compatibility_profile(Request::new(LookupCompatibilityProfileRequest {
            package_id: "pkg_fixture_linux_native".into(),
            ecosystem: EcosystemRuntimeProto::RuntimeLinuxNative as i32,
        }))
        .await
        .expect("lookup_compatibility_profile RPC");
    // The response may be empty but the RPC must not be Unimplemented.
    let _ = lkp_resp;
}

// =========================================================================
// Evidence chain hash continuity (direct BLAKE3 prev_hash check)
// =========================================================================

#[tokio::test]
async fn m13_acceptance_evidence_chain_hash_continuity() {
    let key = signing_key();
    let mut trusted = HashMap::new();
    trusted.insert(key.verifying_key().to_bytes().to_vec(), "m13-chain".into());

    let em = emitter();
    let store = InMemoryPackageStore::new(trusted).with_emitter(em.clone());
    let orchestrator = CompatibilityOrchestrator::new_with_defaults();
    let pkg = make_package("chain-test", "1.0", &key);
    let pkg_id = store.register_package(pkg).await.expect("register");

    let sessions = InMemorySessionDriver::new(orchestrator).with_emitter(em.clone());
    let desc = sessions
        .open_session(OpenSessionRequest {
            package_id: pkg_id.clone(),
            ecosystem: EcosystemRuntime::RuntimeLinuxNative,
            requester: principal("human:chain"),
            capability_grants: vec![],
            timeout: Duration::from_secs(300),
        })
        .await
        .expect("open");

    let updates = InMemoryUpdateDriver::new().with_emitter(em.clone());
    let plan = updates
        .plan_update(UpdatePlanRequest {
            package_id: pkg_id,
            from_version: "1.0".into(),
            to_version: "2.0".into(),
            requester: principal("human:chain"),
            dry_run: false,
        })
        .await
        .expect("plan");
    updates.execute_update(plan.id.clone()).await.expect("exec");
    updates
        .verify_update(plan.id.clone())
        .await
        .expect("verify");
    updates
        .activate_update(plan.id.clone())
        .await
        .expect("activate");
    updates
        .rollback_update(plan.id.clone(), RollbackReason::RegressionDetected)
        .await
        .expect("rollback");
    sessions
        .close_session(desc.session_id)
        .await
        .expect("close");

    // 7 records total
    assert_eq!(em.receipt_count().await, 7);

    // Chain hash integrity — verify the BLAKE3 chain linkage
    em.verify_chain().await.expect("chain hash continuity");
}

// =========================================================================
// INV-015: No secret material in any payload
// =========================================================================

#[tokio::test]
async fn m13_acceptance_inv015_no_secrets_in_payloads() {
    let key = signing_key();
    let mut trusted = HashMap::new();
    trusted.insert(key.verifying_key().to_bytes().to_vec(), "m13-inv015".into());

    let em = emitter();
    let store = InMemoryPackageStore::new(trusted).with_emitter(em.clone());
    let orchestrator = CompatibilityOrchestrator::new_with_defaults();
    let pkg = make_package("inv015-app", "1.0", &key);
    let pkg_id = store.register_package(pkg).await.expect("register");

    let sessions = InMemorySessionDriver::new(orchestrator).with_emitter(em.clone());
    let desc = sessions
        .open_session(OpenSessionRequest {
            package_id: pkg_id.clone(),
            ecosystem: EcosystemRuntime::RuntimeLinuxNative,
            requester: principal("human:inv015"),
            capability_grants: vec![],
            timeout: Duration::from_secs(300),
        })
        .await
        .expect("open");
    sessions
        .close_session(desc.session_id)
        .await
        .expect("close");

    // Scan all payloads for secret markers
    for i in 0..em.receipt_count().await {
        let payload = em.get_payload(i).await.expect("payload");
        let payload_str = serde_json::to_string(&payload).expect("ser");
        assert!(!payload_str.contains("private_key"));
        assert!(!payload_str.contains("secret"));
        assert!(!payload_str.contains("password"));
        assert!(!payload_str.contains("token"));
        assert!(!payload_str.contains("signing_key"));
        assert!(!payload_str.contains("signature"));
    }
}
