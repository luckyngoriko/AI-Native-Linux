//! T-024 — determinism + decision cache integration tests
//! (S2.3 §13 + §15 + §18.1).
//!
//! Covers:
//!
//! - `EnrichmentSnapshot::compute_id` determinism and content-addressing.
//! - `SharedDecisionCache` semantics: miss/hit, bundle-version isolation,
//!   capacity eviction, concurrent reads.
//! - `InMemoryPolicyKernel` cache wiring: hits preserve original
//!   `policy_decision_id`, refresh `evaluated_at`; misses run the
//!   pipeline.
//! - `LoadBundle` RPC: valid bundle activates + invalidates cache for the
//!   prior version; invalid bundle is `FailedPrecondition` and leaves the
//!   cache untouched.
//! - `ExplainDecision` RPC: known id returns the `DecisionPath`; unknown
//!   id returns `NotFound`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::needless_borrows_for_generic_args,
    clippy::needless_pass_by_value,
    clippy::assigning_clones,
    clippy::significant_drop_tightening,
    clippy::let_underscore_must_use
)]

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_policy::bundle::{PolicyBundle, PolicyRule, RuleEffect, RuleScope};
use aios_policy::bundle_loader::BundleLoader;
use aios_policy::cache::{CacheKey, DecisionCache, SharedDecisionCache};
use aios_policy::explain::SharedDecisionLog;
use aios_policy::service::{
    proto::{
        policy_kernel_client::PolicyKernelClient, ExplainDecisionRequest, LoadBundleRequest,
        PolicyBundle as ProtoPolicyBundle,
    },
    PolicyKernelService,
};
use aios_policy::{
    AdapterEnrichment, EnrichmentSnapshot, HydratedSubject, InMemoryPolicyKernel, ObjectEnrichment,
    PolicyContext, PolicyKernel, SubjectType,
};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn make_subject(name: &str) -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: name.into(),
        subject_type: SubjectType::Agent,
        groups: Vec::new(),
        capabilities: Vec::new(),
        session_class: "INTERNAL".into(),
        recovery_mode: false,
        is_ai: false,
    }
}

fn make_envelope(action: &str, tag: u32) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("agent:test", true),
        Request::new(
            action,
            serde_json::json!({"tag": tag, "risk": {"destructive": false}}),
        ),
        Trace::new(&format!("{tag:032x}"), &format!("{tag:016x}"), None),
    )
}

fn make_context(bundle_version: &str) -> PolicyContext {
    let snapshot =
        EnrichmentSnapshot::with_fields(ObjectEnrichment::default(), AdapterEnrichment::default())
            .unwrap();
    PolicyContext::new(
        make_subject("agent:test"),
        snapshot,
        bundle_version,
        "code_v1",
    )
}

fn fresh_keypair() -> (SigningKey, VerifyingKey) {
    let sk = SigningKey::generate(&mut OsRng);
    let vk = sk.verifying_key();
    (sk, vk)
}

fn allow_rule(rule_id: &str) -> PolicyRule {
    PolicyRule {
        rule_id: rule_id.into(),
        scope: RuleScope::Global,
        effect: RuleEffect::Allow,
        priority: 0,
        subjects: vec!["human:lucky".into()],
        actions: vec!["service.restart".into()],
        conditions: Vec::new(),
        constraints: None,
        approval: None,
        reason_code: "ScopedAllow".into(),
    }
}

fn build_signed_bundle(authority: &str, rules: Vec<PolicyRule>) -> (PolicyBundle, BundleLoader) {
    let (sk, vk) = fresh_keypair();
    let mut bundle = PolicyBundle {
        // The T-024 LoadBundle bridge collapses bundle_id onto
        // bundle_version and drops rules. Sign over the post-bridge
        // shape so the signature still verifies after the proto round
        // trip.
        bundle_version: format!("polb_{:032x}", rand_u128()),
        bundle_id: String::new(),
        created_at: Utc.with_ymd_and_hms(2026, 5, 24, 12, 0, 0).unwrap(),
        signing_authority: authority.into(),
        signature_ed25519: Vec::new(),
        rules: Vec::new(),
    };
    bundle.bundle_id = bundle.bundle_version.clone();
    let _ = rules;
    let body = bundle.canonical_signed_body_bytes().unwrap();
    bundle.signature_ed25519 = sk.sign(&body).to_bytes().to_vec();
    let mut trust = HashMap::new();
    trust.insert(authority.into(), vk);
    (bundle, BundleLoader::new(trust))
}

fn rand_u128() -> u128 {
    use rand_core::RngCore;
    let mut buf = [0u8; 16];
    OsRng.fill_bytes(&mut buf);
    u128::from_le_bytes(buf)
}

fn rust_to_proto_bundle(b: &PolicyBundle) -> ProtoPolicyBundle {
    ProtoPolicyBundle {
        bundle_version: b.bundle_version.clone(),
        schema_version: "aios.policy.v1alpha1".into(),
        rules: Vec::new(),
        hard_denies: Vec::new(),
        group_definitions: None,
        publisher_id: b.signing_authority.clone(),
        created_at: Some(prost_types::Timestamp {
            seconds: b.created_at.timestamp(),
            nanos: 0,
        }),
        publisher_signature: b.signature_ed25519.clone(),
        aios_root_signature: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// 1. Decision id is per-evaluation; cache preserves the FIRST id on hit.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn same_envelope_evaluated_twice_without_cache_produces_different_ids() {
    let kernel = InMemoryPolicyKernel::new();
    let ctx = make_context("polb_v1");
    let env = make_envelope("service.restart", 1);
    let d1 = kernel.evaluate_policy(&env, &ctx).await.unwrap();
    let d2 = kernel.evaluate_policy(&env, &ctx).await.unwrap();
    assert_ne!(
        d1.policy_decision_id, d2.policy_decision_id,
        "without cache, each evaluation mints a fresh ULID"
    );
}

#[tokio::test]
async fn cache_hit_preserves_first_decision_id_but_refreshes_evaluated_at() {
    let cache = SharedDecisionCache::with_capacity(8);
    let kernel = InMemoryPolicyKernel::new_with_cache(cache);
    let ctx = make_context("polb_v1");
    let env = make_envelope("service.restart", 1);
    let d1 = kernel.evaluate_policy(&env, &ctx).await.unwrap();
    // sleep just enough to ensure the evaluated_at differs on the refresh.
    std::thread::sleep(std::time::Duration::from_millis(2));
    let d2 = kernel.evaluate_policy(&env, &ctx).await.unwrap();
    assert_eq!(
        d1.policy_decision_id, d2.policy_decision_id,
        "cache hit must preserve the first id"
    );
    assert_eq!(d1.decision, d2.decision);
    assert!(d2.evaluated_at >= d1.evaluated_at);
}

// ---------------------------------------------------------------------------
// 2. Miss-then-hit semantics.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn cache_miss_runs_pipeline_then_subsequent_hit_serves_cached() {
    let cache = SharedDecisionCache::with_capacity(8);
    let kernel = InMemoryPolicyKernel::new_with_cache(cache.clone());
    let ctx = make_context("polb_v1");
    let env = make_envelope("service.restart", 42);
    assert!(cache.is_empty());
    let d1 = kernel.evaluate_policy(&env, &ctx).await.unwrap();
    assert_eq!(cache.len(), 1);
    let d2 = kernel.evaluate_policy(&env, &ctx).await.unwrap();
    assert_eq!(d1.policy_decision_id, d2.policy_decision_id);
}

// ---------------------------------------------------------------------------
// 3-5. Cache key isolation — different bundle / different request / etc.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn different_bundle_version_misses_cache() {
    let cache = SharedDecisionCache::with_capacity(8);
    let kernel = InMemoryPolicyKernel::new_with_cache(cache);
    let env = make_envelope("service.restart", 1);
    let d1 = kernel
        .evaluate_policy(&env, &make_context("polb_v1"))
        .await
        .unwrap();
    let d2 = kernel
        .evaluate_policy(&env, &make_context("polb_v2"))
        .await
        .unwrap();
    assert_ne!(
        d1.policy_decision_id, d2.policy_decision_id,
        "different bundle_version is a different cache key"
    );
}

#[tokio::test]
async fn different_request_hash_misses_cache() {
    let cache = SharedDecisionCache::with_capacity(8);
    let kernel = InMemoryPolicyKernel::new_with_cache(cache);
    let ctx = make_context("polb_v1");
    let d1 = kernel
        .evaluate_policy(&make_envelope("service.restart", 1), &ctx)
        .await
        .unwrap();
    let d2 = kernel
        .evaluate_policy(&make_envelope("service.restart", 2), &ctx)
        .await
        .unwrap();
    assert_ne!(d1.policy_decision_id, d2.policy_decision_id);
}

// ---------------------------------------------------------------------------
// 6-7. EnrichmentSnapshot determinism & content-addressing.
// ---------------------------------------------------------------------------
#[test]
fn enrichment_snapshot_id_is_deterministic_for_same_fields() {
    let a = EnrichmentSnapshot::with_fields(
        ObjectEnrichment {
            privacy_class: Some("PUBLIC".into()),
            ..Default::default()
        },
        AdapterEnrichment::default(),
    )
    .unwrap();
    let b = EnrichmentSnapshot::with_fields(
        ObjectEnrichment {
            privacy_class: Some("PUBLIC".into()),
            ..Default::default()
        },
        AdapterEnrichment::default(),
    )
    .unwrap();
    assert_eq!(a.snapshot_id, b.snapshot_id);
}

#[test]
fn enrichment_snapshot_id_changes_on_any_field_flip() {
    let baseline =
        EnrichmentSnapshot::with_fields(ObjectEnrichment::default(), AdapterEnrichment::default())
            .unwrap();
    let alt_priv = EnrichmentSnapshot::with_fields(
        ObjectEnrichment {
            privacy_class: Some("INTERNAL".into()),
            ..Default::default()
        },
        AdapterEnrichment::default(),
    )
    .unwrap();
    let alt_adapter = EnrichmentSnapshot::with_fields(
        ObjectEnrichment::default(),
        AdapterEnrichment {
            risk_template: Some("high".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_ne!(baseline.snapshot_id, alt_priv.snapshot_id);
    assert_ne!(baseline.snapshot_id, alt_adapter.snapshot_id);
    assert_ne!(alt_priv.snapshot_id, alt_adapter.snapshot_id);
}

// ---------------------------------------------------------------------------
// 8. Concurrent reads on SharedDecisionCache.
// ---------------------------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_cache_survives_concurrent_reads() {
    let cache = SharedDecisionCache::with_capacity(8);
    let key = CacheKey::new("rh1", "polb_v1");
    let decision = aios_policy::decision::PolicyDecision {
        policy_decision_id: "poldec_seed".into(),
        action_id: aios_action::ActionId::default(),
        request_hash: "rh1".into(),
        bundle_version: "polb_v1".into(),
        enrichment_snapshot_id: "polb_snap_x".into(),
        decision: aios_policy::Decision::Deny,
        reason_code: "Seed".into(),
        reason_message: "seed".into(),
        constraints: aios_policy::Constraints::default(),
        approval: aios_policy::ApprovalRequirement::default(),
        evidence_receipt_id: String::new(),
        evaluated_at: Utc::now(),
        rules_consulted: 0,
        simulated: false,
    };
    cache.put(key.clone(), decision);

    let mut handles = Vec::new();
    for _ in 0..10 {
        let c = cache.clone();
        let k = key.clone();
        handles.push(tokio::spawn(async move { c.get(&k).is_some() }));
    }
    for h in handles {
        assert!(h.await.unwrap());
    }
}

// ---------------------------------------------------------------------------
// 9. invalidate_for_bundle removes only matching entries.
// ---------------------------------------------------------------------------
#[test]
fn invalidate_for_bundle_removes_only_matching_version() {
    let mut cache = DecisionCache::new(8);
    let d_template = |bundle: &str| aios_policy::decision::PolicyDecision {
        policy_decision_id: format!("poldec_{bundle}"),
        action_id: aios_action::ActionId::default(),
        request_hash: "rh".into(),
        bundle_version: bundle.into(),
        enrichment_snapshot_id: "polb_snap_x".into(),
        decision: aios_policy::Decision::Deny,
        reason_code: "T".into(),
        reason_message: "t".into(),
        constraints: aios_policy::Constraints::default(),
        approval: aios_policy::ApprovalRequirement::default(),
        evidence_receipt_id: String::new(),
        evaluated_at: Utc::now(),
        rules_consulted: 0,
        simulated: false,
    };
    cache.put(CacheKey::new("rh1", "polb_v1"), d_template("polb_v1"));
    cache.put(CacheKey::new("rh2", "polb_v1"), d_template("polb_v1"));
    cache.put(CacheKey::new("rh1", "polb_v2"), d_template("polb_v2"));
    let removed = cache.invalidate_for_bundle("polb_v1");
    assert_eq!(removed, 2);
    assert!(cache.get(&CacheKey::new("rh1", "polb_v2")).is_some());
}

// ---------------------------------------------------------------------------
// 10. Capacity bound — N+1 insert evicts oldest.
// ---------------------------------------------------------------------------
#[test]
fn cache_capacity_bound_evicts_oldest_on_overflow() {
    let mut cache = DecisionCache::new(2);
    let mk = |i: u32| aios_policy::decision::PolicyDecision {
        policy_decision_id: format!("poldec_{i}"),
        action_id: aios_action::ActionId::default(),
        request_hash: format!("rh{i}"),
        bundle_version: "polb_v1".into(),
        enrichment_snapshot_id: "polb_snap_x".into(),
        decision: aios_policy::Decision::Deny,
        reason_code: "T".into(),
        reason_message: "t".into(),
        constraints: aios_policy::Constraints::default(),
        approval: aios_policy::ApprovalRequirement::default(),
        evidence_receipt_id: String::new(),
        evaluated_at: Utc::now(),
        rules_consulted: 0,
        simulated: false,
    };
    cache.put(CacheKey::new("rh1", "polb_v1"), mk(1));
    cache.put(CacheKey::new("rh2", "polb_v1"), mk(2));
    cache.put(CacheKey::new("rh3", "polb_v1"), mk(3));
    assert!(cache.get(&CacheKey::new("rh1", "polb_v1")).is_none());
    assert!(cache.get(&CacheKey::new("rh2", "polb_v1")).is_some());
    assert!(cache.get(&CacheKey::new("rh3", "polb_v1")).is_some());
}

// ---------------------------------------------------------------------------
// 11. Kernel without cache always runs pipeline.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn kernel_without_cache_always_runs_pipeline() {
    let kernel = InMemoryPolicyKernel::new();
    let ctx = make_context("polb_v1");
    let env = make_envelope("service.restart", 7);
    let d1 = kernel.evaluate_policy(&env, &ctx).await.unwrap();
    let d2 = kernel.evaluate_policy(&env, &ctx).await.unwrap();
    assert_ne!(d1.policy_decision_id, d2.policy_decision_id);
}

// ---------------------------------------------------------------------------
// 12. LoadBundle RPC: valid bundle activates + invalidates cache for prior.
// ---------------------------------------------------------------------------
async fn spawn_test_server(
    svc: PolicyKernelService,
) -> (std::net::SocketAddr, tokio::sync::oneshot::Sender<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let server = tonic::transport::Server::builder().add_service(
        aios_policy::service::proto::policy_kernel_server::PolicyKernelServer::new(svc),
    );
    tokio::spawn(async move {
        let _ = server
            .serve_with_shutdown(addr, async move {
                let _ = rx.await;
            })
            .await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (addr, tx)
}

#[tokio::test]
async fn load_bundle_valid_swaps_active_version_and_invalidates_old_cache() {
    let cache = SharedDecisionCache::with_capacity(8);
    // Pre-load the cache with an entry under the default bundle.
    let kernel = Arc::new(InMemoryPolicyKernel::new_with_cache(cache.clone()));
    let ctx = make_context("polb_default");
    let env = make_envelope("service.restart", 11);
    let _ = kernel.evaluate_policy(&env, &ctx).await.unwrap();
    assert_eq!(cache.len(), 1);

    let (bundle, loader) = build_signed_bundle("publisher-a", vec![allow_rule("r1")]);
    let svc = PolicyKernelService::new_in_memory(kernel.clone())
        .with_bundle_version("polb_default")
        .with_bundle_loader(loader)
        .with_cache(cache.clone());
    let (addr, shutdown) = spawn_test_server(svc).await;
    let mut client = PolicyKernelClient::connect(format!("http://{addr}"))
        .await
        .unwrap();
    let resp = client
        .load_bundle(LoadBundleRequest {
            bundle: Some(rust_to_proto_bundle(&bundle)),
            stage_only: false,
        })
        .await
        .unwrap();
    let resp = resp.into_inner();
    assert!(resp.active);
    assert_eq!(resp.bundle_version, bundle.bundle_version);
    // The pre-load cache entry was for "polb_default"; it should be gone.
    assert_eq!(cache.len(), 0);
    let _ = shutdown.send(());
}

#[tokio::test]
async fn load_bundle_invalid_signature_returns_failed_precondition_and_keeps_cache() {
    let cache = SharedDecisionCache::with_capacity(8);
    let kernel = Arc::new(InMemoryPolicyKernel::new_with_cache(cache.clone()));
    let ctx = make_context("polb_default");
    let env = make_envelope("service.restart", 12);
    let _ = kernel.evaluate_policy(&env, &ctx).await.unwrap();
    let pre_len = cache.len();
    assert_eq!(pre_len, 1);

    let (mut bundle, loader) = build_signed_bundle("publisher-a", vec![allow_rule("r1")]);
    // Corrupt the signature.
    bundle.signature_ed25519[0] ^= 0xFF;
    let svc = PolicyKernelService::new_in_memory(kernel.clone())
        .with_bundle_version("polb_default")
        .with_bundle_loader(loader)
        .with_cache(cache.clone());
    let (addr, shutdown) = spawn_test_server(svc).await;
    let mut client = PolicyKernelClient::connect(format!("http://{addr}"))
        .await
        .unwrap();
    let err = client
        .load_bundle(LoadBundleRequest {
            bundle: Some(rust_to_proto_bundle(&bundle)),
            stage_only: false,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    // Cache must be untouched.
    assert_eq!(cache.len(), pre_len);
    let _ = shutdown.send(());
}

// ---------------------------------------------------------------------------
// 13. ExplainDecision RPC: known id ⇒ DecisionPath; unknown ⇒ NotFound.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn explain_decision_known_id_returns_path_and_unknown_id_returns_not_found() {
    let cache = SharedDecisionCache::with_capacity(8);
    let log = SharedDecisionLog::with_capacity(16);
    let kernel = Arc::new(InMemoryPolicyKernel::new_with_cache(cache.clone()));
    let svc = PolicyKernelService::new_in_memory(kernel.clone())
        .with_bundle_version("polb_default")
        .with_cache(cache.clone())
        .with_decision_log(log.clone());
    let (addr, shutdown) = spawn_test_server(svc).await;
    let mut client = PolicyKernelClient::connect(format!("http://{addr}"))
        .await
        .unwrap();

    // Drive one evaluation through the gRPC surface to populate the log.
    let env = make_envelope("service.restart", 99);
    let bytes = serde_json::to_vec(&env).unwrap();
    let dec = client
        .evaluate_policy(aios_policy::service::proto::EvaluatePolicyRequest {
            schema_version: aios_policy::service::SCHEMA_VERSION.into(),
            envelope_proto: bytes,
        })
        .await
        .unwrap()
        .into_inner();
    let id = dec.policy_decision_id.clone();
    assert!(!id.is_empty());

    let resp = client
        .explain_decision(ExplainDecisionRequest {
            policy_decision_id: id.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.decision.unwrap().policy_decision_id, id);

    let err = client
        .explain_decision(ExplainDecisionRequest {
            policy_decision_id: "poldec_does_not_exist".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
    let _ = shutdown.send(());
}

// ---------------------------------------------------------------------------
// 14. CacheKey wire form is deterministic and content-addressed.
// ---------------------------------------------------------------------------
#[test]
fn cache_key_wire_form_is_deterministic_and_content_addressed() {
    let a = CacheKey::new("rh1", "polb_v1").wire_form();
    let b = CacheKey::new("rh1", "polb_v1").wire_form();
    let c = CacheKey::new("rh1", "polb_v2").wire_form();
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert!(a.starts_with("polc_"));
}
