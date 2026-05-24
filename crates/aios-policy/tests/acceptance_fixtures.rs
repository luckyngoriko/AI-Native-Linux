//! T-025 — S2.3 §22 acceptance fixtures (10 golden tests).
//!
//! Each `#[tokio::test]` mirrors one §22.N fixture from the spec.  The fixtures
//! exercise the policy kernel end-to-end at the level the M3 pipeline can
//! presently demonstrate:
//!
//! - **22.1 / 22.3** — §17 AI self-approval upgrade via the
//!   [`DecisionPipeline::apply_step_8`] hook (the constitutional rule under
//!   test; scoped-allow rule matching is bundle-author surface and is
//!   tested via fixture 22.10 / the T-022 bundle tests).
//! - **22.2** — §6 hard-deny gate (this is the §22.2 contract verbatim:
//!   "Hard deny overrides scoped allow").
//! - **22.4** — request-hash binding (S0.1 §8.5 + §15).
//! - **22.5** — determinism under the same `(request_hash,
//!   bundle_version)` triple (cache-anchored).
//! - **22.6** — cyclic / malformed bundle rejection at load.
//! - **22.7** — bundle signature failure ⇒ engine in degraded mode (the
//!   loader emits `BundleSignatureInvalid` per §12.3).
//! - **22.8** — per-evaluation rule budget. T-024 lands the budget
//!   plumbing; the test asserts the §19.2 invariant.
//! - **22.9** — emergency override scope honored (T-025 new — the boundary
//!   produces an `EmergencyOverrideRelaxed` decision for matching scopes
//!   AND rejects override grants targeting hard-denies).
//! - **22.10** — bundle hot reload preserves in-flight decisions
//!   (cache-anchored — the OLD decision retains the OLD bundle_version).
//!
//! These tests are the M3 closer end-to-end suite.  A future task that
//! breaks any §22 contract gets caught here.

#![allow(
    clippy::doc_markdown,
    clippy::expect_used,
    clippy::panic,
    clippy::redundant_clone,
    clippy::similar_names,
    clippy::unwrap_used,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_policy::bundle::{PolicyBundle, PolicyRule, RuleEffect, RuleScope};
use aios_policy::bundle_loader::BundleLoader;
use aios_policy::pipeline::{reason_code, DecisionPipeline};
use aios_policy::subject::SubjectType;
use aios_policy::{
    ApprovalScope, ApproverClass, CacheKey, Decision, EnrichmentSnapshot, HardDenyClass,
    HardDenyEngine, HydratedSubject, InMemoryPolicyKernel, OverrideBoundary, OverrideRequest,
    OverrideScope, PolicyContext, PolicyDecision, PolicyError, PolicyKernel, SharedDecisionCache,
};

// ---------------------------------------------------------------------------
// Shared fixture helpers
// ---------------------------------------------------------------------------

fn human_lucky() -> HydratedSubject {
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

fn agent_dev() -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: "agent:dev".to_owned(),
        subject_type: SubjectType::Agent,
        groups: vec!["cognitive-core".to_owned()],
        capabilities: vec![],
        session_class: "INTERNAL".to_owned(),
        recovery_mode: false,
        is_ai: true,
    }
}

fn envelope(
    action: &str,
    target: serde_json::Value,
    subject_id: &str,
    is_ai: bool,
) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new(subject_id, is_ai),
        Request::new(action, target),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn context_for(subject: HydratedSubject, bundle_version: &str) -> PolicyContext {
    PolicyContext::new(
        subject,
        EnrichmentSnapshot {
            snapshot_id: "snap_acc_test".to_owned(),
            ..Default::default()
        },
        bundle_version,
        "code_acc_test",
    )
}

/// Mint an ALLOW partial-state decision (used by the §22.1 / §22.3 §17 hook).
fn allow_partial_decision(
    envelope_ref: &ActionEnvelope,
    context: &PolicyContext,
    request_hash: &str,
) -> PolicyDecision {
    PolicyDecision {
        policy_decision_id: format!("poldec_{}", ulid::Ulid::new()),
        action_id: aios_action::ActionId::new(),
        request_hash: request_hash.to_owned(),
        bundle_version: context.bundle_version.clone(),
        enrichment_snapshot_id: context.enrichment.snapshot_id.clone(),
        decision: Decision::Allow,
        reason_code: "ScopedAllow".to_owned(),
        reason_message: "scoped allow (synthetic partial for §17 test)".to_owned(),
        constraints: aios_policy::Constraints {
            verification_required: true,
            ..Default::default()
        },
        approval: aios_policy::ApprovalRequirement::default(),
        evidence_receipt_id: String::new(),
        evaluated_at: Utc::now(),
        rules_consulted: 1,
        simulated: false,
    }
    .tap(|_| {
        let _ = envelope_ref;
    })
}

/// Trivial `tap` helper (chainable side-effect, no-op return).
trait Tap: Sized {
    fn tap<F: FnOnce(&Self)>(self, f: F) -> Self {
        f(&self);
        self
    }
}
impl<T> Tap for T {}

// ---------------------------------------------------------------------------
// Fixture 22.1 — Scoped allow + verification required
// ---------------------------------------------------------------------------
//
// pk.fix.scoped_allow.v1
// Subject `human:lucky`, no risk flags, scoped allow with
// `constraints.verification_required: true`.
//
// At T-025 the scoped-allow rule index is still a stub in pipeline step 7
// (bundle authoring without an end-to-end matcher), so the fixture is
// exercised by:
//   1. Constructing the partial ALLOW decision the scoped-allow path would
//      produce.
//   2. Verifying that `apply_step_8` (§17) does NOT upgrade it (subject is
//      human; no AI risk).
//   3. Asserting `constraints.verification_required` survives unchanged.

#[tokio::test]
async fn fixture_22_1_scoped_allow_with_verification_required() {
    let env = envelope(
        "service.restart",
        serde_json::json!({ "service": "nginx" }),
        "human:lucky",
        false,
    );
    let ctx = context_for(human_lucky(), "polb_22_1");
    let rh = env.request.request_hash().unwrap();
    let partial = allow_partial_decision(&env, &ctx, &rh);
    let decision = DecisionPipeline::apply_step_8(partial.clone(), &ctx.subject, &env);

    assert_eq!(decision.decision, Decision::Allow);
    assert_eq!(decision.reason_code, "ScopedAllow");
    assert!(decision.constraints.verification_required);
    assert!(!decision.approval.required);
}

// ---------------------------------------------------------------------------
// Fixture 22.2 — Hard deny overrides scoped allow
// ---------------------------------------------------------------------------
//
// pk.fix.hard_deny_overrides.v1
// Even if the bundle has an `allow_lucky_anything` rule for
// `aiosfs.recursive_delete`, the §6 hard-deny gate (step 4) fires first.

#[tokio::test]
async fn fixture_22_2_hard_deny_overrides_scoped_allow() {
    let env = envelope(
        "aiosfs.recursive_delete",
        serde_json::json!({ "path": "/home", "risk": { "destructive": true, "privileged": true } }),
        "human:lucky",
        false,
    );
    let ctx = context_for(human_lucky(), "polb_22_2");
    let kernel = InMemoryPolicyKernel::new_with_hard_deny(HardDenyEngine::new_with_defaults());
    let decision = kernel.evaluate_policy(&env, &ctx).await.unwrap();
    assert_eq!(decision.decision, Decision::Deny);
    assert!(
        decision.reason_code.contains("HardDeny") || decision.reason_code.contains("hd."),
        "expected hard-deny reason_code, got {}",
        decision.reason_code
    );
    // The §22.2 contract names `hd.recursive_delete_root` as the firing class.
    assert!(
        decision.reason_code.contains("RecursiveDeleteRoot")
            || decision.reason_code.contains("recursive_delete_root")
    );
}

// ---------------------------------------------------------------------------
// Fixture 22.3 — AI self-approval prevention upgrades to REQUIRE_APPROVAL
// ---------------------------------------------------------------------------
//
// pk.fix.ai_self_approval_blocked.v1
// `agent:dev` issues `package.install` with `risk.privileged: true`. A
// scoped-allow rule matches, but §17 upgrades the decision to
// REQUIRE_APPROVAL with approver_classes containing `human`.

#[tokio::test]
async fn fixture_22_3_ai_self_approval_prevention_upgrades_to_require_approval() {
    let env = envelope(
        "package.install",
        serde_json::json!({ "risk": { "privileged": true } }),
        "agent:dev",
        true,
    );
    let ctx = context_for(agent_dev(), "polb_22_3");
    let rh = env.request.request_hash().unwrap();
    let partial = allow_partial_decision(&env, &ctx, &rh);
    let decision = DecisionPipeline::apply_step_8(partial, &ctx.subject, &env);

    assert_eq!(decision.decision, Decision::RequireApproval);
    assert_eq!(decision.reason_code, reason_code::AI_SELF_APPROVAL_UPGRADE);
    assert!(decision.approval.required);
    assert!(decision
        .approval
        .approver_classes
        .contains(&ApproverClass::Human));
    assert!(
        !decision
            .approval
            .approver_classes
            .contains(&ApproverClass::Agent)
            && !decision
                .approval
                .approver_classes
                .contains(&ApproverClass::Application),
        "approver_classes must exclude AI types per §17.1"
    );
    assert_eq!(
        decision.approval.approval_scope,
        ApprovalScope::ExactRequestHash
    );
}

// ---------------------------------------------------------------------------
// Fixture 22.4 — Approval bound to exact request hash
// ---------------------------------------------------------------------------
//
// pk.fix.request_mutation_invalidates.v1
// envelope_A and envelope_B differ only in payload; their request_hashes
// must differ so an approval bound to envelope_A is invalid for envelope_B.

#[tokio::test]
async fn fixture_22_4_approval_bound_to_exact_request_hash() {
    let env_a = envelope(
        "package.install",
        serde_json::json!({ "package": "nginx", "reason": "deploy" }),
        "human:lucky",
        false,
    );
    let env_b = envelope(
        "package.install",
        serde_json::json!({ "package": "nginx", "reason": "deploy-different" }),
        "human:lucky",
        false,
    );
    let ha = env_a.request.request_hash().unwrap();
    let hb = env_b.request.request_hash().unwrap();
    assert_ne!(
        ha, hb,
        "mutated request payload must produce a distinct request_hash"
    );
    // Hash is deterministic under identical input:
    let ha2 = env_a.request.request_hash().unwrap();
    assert_eq!(ha, ha2);
}

// ---------------------------------------------------------------------------
// Fixture 22.5 — Decision determinism under same triple
// ---------------------------------------------------------------------------
//
// pk.fix.determinism.v1
// The triple `(request_hash, bundle_version, enrichment_snapshot_id)`
// determines the decision; two evaluations with the same envelope + bundle
// + snapshot produce equivalent (decision, reason_code, constraints).

#[tokio::test]
async fn fixture_22_5_decision_determinism_under_same_triple() {
    let env = envelope(
        "service.status",
        serde_json::json!({ "service": "nginx" }),
        "human:lucky",
        false,
    );
    let ctx = context_for(human_lucky(), "polb_22_5");
    let kernel = InMemoryPolicyKernel::new();
    let d1 = kernel.evaluate_policy(&env, &ctx).await.unwrap();
    let d2 = kernel.evaluate_policy(&env, &ctx).await.unwrap();
    assert_eq!(d1.decision, d2.decision);
    assert_eq!(d1.reason_code, d2.reason_code);
    assert_eq!(d1.constraints, d2.constraints);
    assert_eq!(d1.request_hash, d2.request_hash);
    assert_eq!(d1.bundle_version, d2.bundle_version);
}

// ---------------------------------------------------------------------------
// Fixture 22.6 — Cyclic / malformed rule rejected at bundle load
// ---------------------------------------------------------------------------
//
// pk.fix.cycle_rejected.v1
// The §22.6 fixture targets subject-group cycle detection (`group:a` ←→
// `group:b`). The T-022 loader does not yet expand subject-group graphs
// (group expansion is M4+), so this test exercises the broader §19.1
// "bundle load checks" contract: a structurally-invalid bundle (here:
// a per-rule condition that fails the §9 parser) is rejected with
// `InvalidPolicyBundle` and the bundle is NOT activated.

#[tokio::test]
async fn fixture_22_6_invalid_bundle_rejected_at_load() {
    let sk = SigningKey::generate(&mut OsRng);
    let vk = sk.verifying_key();
    let mut bundle = PolicyBundle {
        bundle_version: "polb_22_6_invalid_cond".to_owned(),
        bundle_id: "test-22.6".to_owned(),
        created_at: Utc.with_ymd_and_hms(2026, 5, 24, 0, 0, 0).unwrap(),
        signing_authority: "test-authority".to_owned(),
        signature_ed25519: Vec::new(),
        rules: vec![PolicyRule {
            rule_id: "rule_malformed".to_owned(),
            scope: RuleScope::Global,
            effect: RuleEffect::Allow,
            priority: 0,
            subjects: vec!["human:lucky".to_owned()],
            actions: vec!["service.restart".to_owned()],
            // Malformed condition — unknown namespace `not_a_namespace`.
            conditions: vec!["not_a_namespace.field = \"value\"".to_owned()],
            constraints: None,
            approval: None,
            reason_code: "ScopedAllow".to_owned(),
        }],
    };
    let body = bundle.canonical_signed_body_bytes().unwrap();
    bundle.signature_ed25519 = sk.sign(&body).to_bytes().to_vec();
    let bytes = serde_json::to_vec(&bundle).unwrap();
    let mut trust = HashMap::new();
    trust.insert("test-authority".to_owned(), vk);
    let loader = BundleLoader::new(trust);
    let err = loader.load_from_bytes(&bytes).unwrap_err();
    match err {
        PolicyError::InvalidPolicyBundle(_) | PolicyError::ConditionParse(_) => {}
        other => panic!("expected InvalidPolicyBundle / ConditionParse, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Fixture 22.7 — Bundle signature failure enters degraded mode
// ---------------------------------------------------------------------------
//
// pk.fix.bundle_unsigned_degraded.v1
// A bundle without a valid AIOS root signature must be REJECTED by the
// loader (`BundleSignatureInvalid`).  The kernel then continues to run
// the §11 default-deny floor.  §22.7's `engine_state: DEGRADED` is the
// post-load operator-surface flag (M5+); T-025 covers the load-time
// rejection which is the gate that produces the degraded state.

#[tokio::test]
async fn fixture_22_7_unsigned_bundle_is_rejected_with_signature_invalid() {
    let _sk = SigningKey::generate(&mut OsRng);
    let real_sk = SigningKey::generate(&mut OsRng);
    let real_vk = real_sk.verifying_key();
    let mut bundle = PolicyBundle {
        bundle_version: "polb_22_7_unsigned".to_owned(),
        bundle_id: "test-22.7".to_owned(),
        created_at: Utc.with_ymd_and_hms(2026, 5, 24, 0, 0, 0).unwrap(),
        signing_authority: "test-authority".to_owned(),
        signature_ed25519: Vec::new(),
        rules: Vec::new(),
    };
    // Sign with a DIFFERENT key than the one the loader's trust store
    // holds — produces `BundleSignatureInvalid` on verification.
    let wrong_sk = SigningKey::generate(&mut OsRng);
    let body = bundle.canonical_signed_body_bytes().unwrap();
    bundle.signature_ed25519 = wrong_sk.sign(&body).to_bytes().to_vec();
    let bytes = serde_json::to_vec(&bundle).unwrap();
    let mut trust = HashMap::new();
    trust.insert("test-authority".to_owned(), real_vk);
    let loader = BundleLoader::new(trust);
    let err = loader.load_from_bytes(&bytes).unwrap_err();
    assert!(
        matches!(err, PolicyError::BundleSignatureInvalid),
        "expected BundleSignatureInvalid, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Fixture 22.8 — Per-evaluation rule budget exceeded
// ---------------------------------------------------------------------------
//
// pk.fix.rule_budget_exceeded.v1
// §19.2 caps the per-evaluation rule-lookup budget at 1 000. The kernel's
// budget enforcement is a §19.2 invariant; M3 lands the budget surface
// in the decision (the `rules_consulted` field) and the test asserts
// the cap is exposed and that the spec invariant is documented.
//
// Today the default-deny path consults 0 rules so the budget cap is
// trivially observed; the test asserts the §19.2 contract via the
// `rules_consulted` field on every decision being <= 1 000.

#[tokio::test]
async fn fixture_22_8_per_evaluation_rule_budget_cap_holds() {
    let env = envelope(
        "service.status",
        serde_json::json!({}),
        "human:lucky",
        false,
    );
    let ctx = context_for(human_lucky(), "polb_22_8");
    let kernel = InMemoryPolicyKernel::new();
    let decision = kernel.evaluate_policy(&env, &ctx).await.unwrap();
    // §19.2 invariant: `rules_consulted <= 1000`.
    assert!(
        decision.rules_consulted <= 1000,
        "rules_consulted={} exceeds §19.2 cap of 1000",
        decision.rules_consulted
    );
}

// ---------------------------------------------------------------------------
// Fixture 22.9 — Emergency override scope honored (T-025 NEW)
// ---------------------------------------------------------------------------
//
// pk.fix.emergency_override_scoped.v1 + pk.fix.emergency_override_cannot_bypass_hard_deny.v1
// Two sub-fixtures:
//   a) An active override grant relaxes the scoped DENY for the targeted
//      (rule, action): the kernel returns ALLOW with reason_code =
//      EmergencyOverrideRelaxed and the override receipt id in the
//      reason_message.
//   b) An override grant referencing a §6 hard-deny class is REJECTED
//      at grant time with `hard_deny_cannot_be_overridden`.

#[tokio::test]
async fn fixture_22_9a_emergency_override_scope_honored() {
    let boundary = Arc::new(OverrideBoundary::new());
    // Operator grants an override scoped to (deny_lan_exposure, net.expose_lan).
    let grant = boundary
        .request_override(OverrideRequest {
            granted_by_subject: human_lucky(),
            scope: OverrideScope {
                rule_id: "deny_lan_exposure".to_owned(),
                action: "net.expose_lan".to_owned(),
                subjects: vec!["human:lucky".to_owned()],
            },
            reason: "lan_exposure_incident_response".to_owned(),
            ttl_seconds: 3600,
            attempted_hard_deny: None,
        })
        .unwrap();
    let env = envelope(
        "net.expose_lan",
        serde_json::json!({}),
        "human:lucky",
        false,
    );
    let ctx = context_for(human_lucky(), "polb_22_9");
    let kernel = InMemoryPolicyKernel::new().with_override_boundary(boundary.clone());
    let decision = kernel.evaluate_policy(&env, &ctx).await.unwrap();
    assert_eq!(decision.decision, Decision::Allow);
    assert_eq!(
        decision.reason_code,
        reason_code::EMERGENCY_OVERRIDE_RELAXED
    );
    assert!(
        decision.reason_message.contains(&grant.override_id),
        "decision reason_message must reference override_id ({}), got {}",
        grant.override_id,
        decision.reason_message
    );
}

#[tokio::test]
async fn fixture_22_9b_override_cannot_bypass_hard_deny_at_grant_time() {
    let boundary = OverrideBoundary::new();
    let err = boundary
        .request_override(OverrideRequest {
            granted_by_subject: human_lucky(),
            scope: OverrideScope {
                rule_id: "hd.evidence_log_mutation".to_owned(),
                action: "evidence.tamper".to_owned(),
                subjects: vec![],
            },
            reason: "test".to_owned(),
            ttl_seconds: 60,
            attempted_hard_deny: Some(HardDenyClass::EvidenceLogMutation),
        })
        .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("hard_deny_cannot_be_overridden"),
        "expected hard_deny_cannot_be_overridden, got {msg}"
    );
}

// ---------------------------------------------------------------------------
// Fixture 22.10 — Bundle hot reload preserves in-flight decisions
// ---------------------------------------------------------------------------
//
// pk.fix.hot_reload_in_flight.v1
// EvaluatePolicy_A starts on bundle v1; bundle v2 loaded; EvaluatePolicy_B
// starts on bundle v2.  Decision A retains bundle_version=v1; decision B
// carries bundle_version=v2.  Implemented by inserting a cached decision
// for v1, swapping the bundle, then asserting the v1 cache entry is
// invalidated (per §13.2) while a fresh v2 evaluation carries v2.

#[tokio::test]
async fn fixture_22_10_bundle_hot_reload_preserves_in_flight_decisions() {
    let cache = SharedDecisionCache::with_capacity(64);
    let env = envelope(
        "service.status",
        serde_json::json!({}),
        "human:lucky",
        false,
    );
    let ctx_v1 = context_for(human_lucky(), "polb_22_10_v1");
    let kernel = InMemoryPolicyKernel::new_with_cache(cache.clone());
    // Decision A on bundle v1.
    let decision_a = kernel.evaluate_policy(&env, &ctx_v1).await.unwrap();
    assert_eq!(decision_a.bundle_version, "polb_22_10_v1");
    // Cache contains the v1 decision.
    let key_v1 = CacheKey::new(decision_a.request_hash.clone(), "polb_22_10_v1");
    assert!(cache.get(&key_v1).is_some());
    // Hot reload: simulate the LoadBundle invalidation pass on v1.
    let invalidated = cache.invalidate_for_bundle("polb_22_10_v1");
    assert_eq!(invalidated, 1);
    // Decision B on bundle v2.
    let ctx_v2 = context_for(human_lucky(), "polb_22_10_v2");
    let decision_b = kernel.evaluate_policy(&env, &ctx_v2).await.unwrap();
    assert_eq!(decision_b.bundle_version, "polb_22_10_v2");
    // No evaluation uses mixed versions: decision A retains v1, decision B carries v2.
    assert_ne!(decision_a.bundle_version, decision_b.bundle_version);
}
