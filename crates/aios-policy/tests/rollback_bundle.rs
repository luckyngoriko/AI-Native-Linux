//! T-025 — `RollbackBundle` RPC integration tests (S2.3 §12.5).
//!
//! Coverage:
//!
//! - Rollback restores the previously-active bundle from the kernel's
//!   bounded rollback stack.
//! - Rollback on an empty stack short-circuits with `failed_precondition`.
//! - Rollback invalidates cached decisions for the displaced bundle
//!   (§13.2 — bundle flip ⇒ cache invalidation).
//! - Rollback mints an evidence-receipt id `evr_rb_<ULID>` for the
//!   audit chain.
//! - Override boundary grants are cleared on rollback (§16.3 — overrides
//!   do not persist across bundle versions).

#![allow(
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::panic,
    clippy::redundant_clone,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::unwrap_used,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::net::SocketAddr;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use aios_policy::bundle::{PolicyBundle, PolicyRule, RuleEffect, RuleScope};
use aios_policy::cache::SharedDecisionCache;
use aios_policy::override_boundary::{OverrideBoundary, OverrideRequest, OverrideScope};
use aios_policy::service::proto::policy_kernel_client::PolicyKernelClient;
use aios_policy::service::proto::policy_kernel_server::PolicyKernelServer;
use aios_policy::service::proto::RollbackBundleRequest;
use aios_policy::service::PolicyKernelService;
use aios_policy::subject::SubjectType;
use aios_policy::{HydratedSubject, InMemoryPolicyKernel};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

async fn pick_port() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr
}

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
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (addr, tx, handle)
}

fn fresh_signed_bundle(bundle_version: &str, authority: &str) -> (PolicyBundle, SigningKey) {
    let sk = SigningKey::generate(&mut OsRng);
    let mut bundle = PolicyBundle {
        bundle_version: bundle_version.to_owned(),
        bundle_id: format!("test-{bundle_version}"),
        created_at: Utc.with_ymd_and_hms(2026, 5, 24, 0, 0, 0).unwrap(),
        signing_authority: authority.to_owned(),
        signature_ed25519: Vec::new(),
        rules: vec![PolicyRule {
            rule_id: "r1".to_owned(),
            scope: RuleScope::Global,
            effect: RuleEffect::Allow,
            priority: 0,
            subjects: vec!["human:lucky".to_owned()],
            actions: vec!["service.status".to_owned()],
            conditions: Vec::new(),
            constraints: None,
            approval: None,
            reason_code: "ScopedAllow".to_owned(),
        }],
    };
    let body = bundle.canonical_signed_body_bytes().unwrap();
    bundle.signature_ed25519 = sk.sign(&body).to_bytes().to_vec();
    (bundle, sk)
}

fn human_subject() -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: "human:lucky".to_owned(),
        subject_type: SubjectType::Human,
        groups: vec!["operators".to_owned()],
        capabilities: vec![],
        session_class: "INTERNAL".to_owned(),
        recovery_mode: false,
        is_ai: false,
    }
}

// ---------------------------------------------------------------------------
// 1. Direct kernel test — rollback restores previous bundle.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn kernel_rollback_restores_previous_bundle() {
    let kernel = InMemoryPolicyKernel::new();
    let (b1, _sk1) = fresh_signed_bundle("polb_v1", "auth-a");
    let (b2, _sk2) = fresh_signed_bundle("polb_v2", "auth-a");
    assert_eq!(kernel.rollback_stack_depth(), 0);
    // First LoadBundle: nothing displaced, stack stays empty.
    let prev = kernel.set_active_bundle(b1.clone());
    assert!(prev.is_none());
    assert_eq!(kernel.rollback_stack_depth(), 0);
    // Second LoadBundle: b1 displaced, pushed on stack.
    let prev = kernel.set_active_bundle(b2.clone());
    assert_eq!(prev.map(|b| b.bundle_version), Some("polb_v1".to_owned()));
    assert_eq!(kernel.rollback_stack_depth(), 1);
    // Rollback: restores b1 as active, displaces b2.
    let (restored, displaced_v) = kernel.rollback_active_bundle().unwrap();
    assert_eq!(restored.bundle_version, "polb_v1");
    assert_eq!(displaced_v, Some("polb_v2".to_owned()));
    assert_eq!(kernel.rollback_stack_depth(), 0);
    // Now the active bundle is b1 again.
    let snap = kernel.active_bundle_snapshot().unwrap();
    assert_eq!(snap.bundle_version, "polb_v1");
}

// ---------------------------------------------------------------------------
// 2. Rollback on empty stack returns None at the kernel level.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn kernel_rollback_on_empty_stack_returns_none() {
    let kernel = InMemoryPolicyKernel::new();
    assert!(kernel.rollback_active_bundle().is_none());
}

// ---------------------------------------------------------------------------
// 3. gRPC: RollbackBundle on empty stack ⇒ failed_precondition.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn grpc_rollback_on_empty_stack_returns_failed_precondition() {
    let kernel = Arc::new(InMemoryPolicyKernel::new());
    let svc = PolicyKernelService::new_in_memory(kernel);
    let (addr, shutdown, handle) = spawn_server(svc).await;
    let mut client = PolicyKernelClient::connect(format!("http://{addr}"))
        .await
        .unwrap();
    let err = client
        .rollback_bundle(RollbackBundleRequest::default())
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    let _ = shutdown.send(());
    let _ = handle.await;
}

// ---------------------------------------------------------------------------
// 4. gRPC: LoadBundle x2 → RollbackBundle restores v1 and invalidates v2 cache.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn grpc_rollback_after_two_loads_restores_v1_and_invalidates_cache() {
    let cache = SharedDecisionCache::with_capacity(64);
    let kernel = Arc::new(InMemoryPolicyKernel::new());
    let (b1, _sk1) = fresh_signed_bundle("polb_v1", "auth-a");
    let (b2, _sk2) = fresh_signed_bundle("polb_v2", "auth-a");
    // Push two bundles directly via the kernel's set_active_bundle (this is
    // the same path the LoadBundle RPC exercises; bypassing the RPC keeps
    // the test focused on rollback semantics without the LoadBundle proto-
    // bridge dance).
    kernel.set_active_bundle(b1.clone());
    kernel.set_active_bundle(b2.clone());
    assert_eq!(kernel.rollback_stack_depth(), 1);

    let svc = PolicyKernelService::new_in_memory(kernel.clone())
        .with_cache(cache.clone())
        .with_bundle_version("polb_v2");
    let (addr, shutdown, handle) = spawn_server(svc).await;
    let mut client = PolicyKernelClient::connect(format!("http://{addr}"))
        .await
        .unwrap();
    // Pre-populate cache with a v2 entry to confirm invalidation on rollback.
    use aios_policy::cache::CacheKey;
    use aios_policy::{Decision, PolicyDecision};
    let dummy = PolicyDecision {
        policy_decision_id: "poldec_dummy".to_owned(),
        action_id: aios_action::ActionId::new(),
        request_hash: "rh_test".to_owned(),
        bundle_version: "polb_v2".to_owned(),
        enrichment_snapshot_id: "snap".to_owned(),
        decision: Decision::Deny,
        reason_code: "DefaultDeny".to_owned(),
        reason_message: "test".to_owned(),
        constraints: aios_policy::Constraints::default(),
        approval: aios_policy::ApprovalRequirement::default(),
        evidence_receipt_id: String::new(),
        evaluated_at: Utc::now(),
        rules_consulted: 0,
        simulated: false,
    };
    cache.put(CacheKey::new("rh_test", "polb_v2"), dummy);
    assert_eq!(cache.len(), 1);
    // Rollback.
    let resp = client
        .rollback_bundle(RollbackBundleRequest::default())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.previous_bundle_version, "polb_v2");
    assert_eq!(resp.current_bundle_version, "polb_v1");
    assert!(resp.evidence_receipt_id.starts_with("evr_rb_"));
    // Cache entry for v2 invalidated.
    assert!(cache.get(&CacheKey::new("rh_test", "polb_v2")).is_none());
    let _ = shutdown.send(());
    let _ = handle.await;
    let _ = human_subject(); // silence unused warning
}

// ---------------------------------------------------------------------------
// 5. Rollback clears override boundary grants (§16.3 non-persistence).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn kernel_rollback_clears_override_grants() {
    let boundary = Arc::new(OverrideBoundary::new());
    let kernel = InMemoryPolicyKernel::new().with_override_boundary(boundary.clone());
    let (b1, _sk1) = fresh_signed_bundle("polb_v1", "auth-a");
    let (b2, _sk2) = fresh_signed_bundle("polb_v2", "auth-a");
    // Issue a grant after b1 is active.
    kernel.set_active_bundle(b1);
    let _ = boundary
        .request_override(OverrideRequest {
            granted_by_subject: human_subject(),
            scope: OverrideScope {
                rule_id: "deny_lan".to_owned(),
                action: "net.expose_lan".to_owned(),
                subjects: vec![],
            },
            reason: "test".to_owned(),
            ttl_seconds: 3600,
            attempted_hard_deny: None,
        })
        .unwrap();
    assert_eq!(boundary.len(), 1);
    // Bundle swap to b2 — clears grants.
    kernel.set_active_bundle(b2);
    assert_eq!(boundary.len(), 0);
    // Re-issue a grant on b2.
    let _ = boundary
        .request_override(OverrideRequest {
            granted_by_subject: human_subject(),
            scope: OverrideScope {
                rule_id: "deny_lan".to_owned(),
                action: "net.expose_lan".to_owned(),
                subjects: vec![],
            },
            reason: "test".to_owned(),
            ttl_seconds: 3600,
            attempted_hard_deny: None,
        })
        .unwrap();
    assert_eq!(boundary.len(), 1);
    // Rollback to b1 — clears the new grant too.
    let (restored, _) = kernel.rollback_active_bundle().unwrap();
    assert_eq!(restored.bundle_version, "polb_v1");
    assert_eq!(boundary.len(), 0);
}

// ---------------------------------------------------------------------------
// 6. Ring-buffer behaviour: stack capped at rollback_capacity.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn kernel_rollback_stack_is_ring_buffered_at_capacity() {
    let kernel = InMemoryPolicyKernel::new().with_rollback_capacity(2);
    assert_eq!(kernel.rollback_stack_capacity(), 2);
    let (b1, _) = fresh_signed_bundle("polb_v1", "auth-a");
    let (b2, _) = fresh_signed_bundle("polb_v2", "auth-a");
    let (b3, _) = fresh_signed_bundle("polb_v3", "auth-a");
    let (b4, _) = fresh_signed_bundle("polb_v4", "auth-a");
    kernel.set_active_bundle(b1);
    kernel.set_active_bundle(b2); // stack: [v1]
    kernel.set_active_bundle(b3); // stack: [v1, v2]
    kernel.set_active_bundle(b4); // stack: ring-buffered: [v2, v3]
    assert_eq!(kernel.rollback_stack_depth(), 2);
    // First rollback restores v3 (top of stack).
    let (r1, _) = kernel.rollback_active_bundle().unwrap();
    assert_eq!(r1.bundle_version, "polb_v3");
    // Second rollback restores v2.
    let (r2, _) = kernel.rollback_active_bundle().unwrap();
    assert_eq!(r2.bundle_version, "polb_v2");
    // Stack now empty.
    assert!(kernel.rollback_active_bundle().is_none());
}
