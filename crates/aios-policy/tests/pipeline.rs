//! T-017 integration tests for the 12-step decision pipeline + precedence ladder.
//!
//! Anchors the constitutional shape of:
//! - [`aios_policy::RulePrecedence`] iteration order (S2.3 §5 — 7 fixed tiers).
//! - Step 1 (schema validation) short-circuits to `SchemaInvalid` `DENY`.
//! - Step 9 (default deny floor, S2.3 §11) fires when all stubs pass.
//! - [`aios_policy::InMemoryPolicyKernel::evaluate_policy`] returns a fully populated
//!   [`aios_policy::PolicyDecision`] (all 14 fields per S2.3 §4).
//! - `PolicyDecision.bundle_version` mirrors the input [`aios_policy::PolicyContext`].
//! - `PolicyDecision.evaluated_at` is within 5 s of test execution.
//! - Two evaluations on the same envelope mint distinct `policy_decision_id` ULIDs.
//! - [`aios_policy::PipelineState::ShortCircuit`] prevents later steps from running.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use chrono::{Duration, Utc};
use strum::EnumCount;

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_policy::{
    reason_code, Decision, DecisionPipeline, EnrichmentSnapshot, HydratedSubject,
    InMemoryPolicyKernel, PipelineState, PolicyContext, PolicyKernel, RulePrecedence, SubjectType,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn make_subject() -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: "human:lucky:01HX0000000000000000000000".to_owned(),
        subject_type: SubjectType::Human,
        groups: vec!["operators".to_owned()],
        capabilities: vec![],
        session_class: "INTERNAL".to_owned(),
        recovery_mode: false,
        is_ai: false,
    }
}

fn make_context() -> PolicyContext {
    PolicyContext::new(
        make_subject(),
        EnrichmentSnapshot {
            snapshot_id: "snap_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
            ..Default::default()
        },
        "polb_t017_test_bundle_v1",
        "code_t017_test",
    )
}

fn make_envelope() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("human:lucky", false),
        Request::new("service.restart", serde_json::json!({"service": "nginx"})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

// ---------------------------------------------------------------------------
// 1. RulePrecedence iteration is fixed at 7 tiers in spec order
// ---------------------------------------------------------------------------

#[test]
fn rule_precedence_iter_yields_exactly_seven_tiers_in_spec_order() {
    // S2.3 §5 fixes the tier order. The variant order in the enum declaration is
    // load-bearing; strum::IntoEnumIterator yields in declaration order.
    let tiers: Vec<RulePrecedence> = RulePrecedence::iter().collect();

    assert_eq!(
        tiers.len(),
        7,
        "S2.3 §5: exactly 7 precedence tiers, got {}",
        tiers.len()
    );

    assert_eq!(
        RulePrecedence::COUNT,
        7,
        "compile-time anchor: RulePrecedence::COUNT must equal 7"
    );

    assert_eq!(
        tiers,
        vec![
            RulePrecedence::InvalidSubject,
            RulePrecedence::HardDeny,
            RulePrecedence::EmergencyOverrideDenylist,
            RulePrecedence::ExplicitScopedDeny,
            RulePrecedence::ExplicitScopedAllow,
            RulePrecedence::AiSelfApprovalUpgrade,
            RulePrecedence::DefaultDeny,
        ],
        "S2.3 §5 tier order is constitutional — variant declaration order must not drift"
    );
}

// ---------------------------------------------------------------------------
// 2. Step 1 — schema validation short-circuit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn empty_envelope_action_short_circuits_at_step_1_with_schema_invalid() {
    // Build a malformed envelope: empty action name. Step 1 must short-circuit before
    // any later step (including the default-deny floor) is reached.
    let mut env = make_envelope();
    env.request.action = String::new();
    let ctx = make_context();
    let kernel = InMemoryPolicyKernel::new();

    let decision = kernel
        .evaluate_policy(&env, &ctx)
        .await
        .expect("evaluation must not raise PolicyError today");

    assert_eq!(decision.decision, Decision::Deny);
    assert_eq!(
        decision.reason_code,
        reason_code::SCHEMA_INVALID,
        "S2.3 §3 step 1: schema-invalid envelope must produce reason_code = SchemaInvalid"
    );
}

// ---------------------------------------------------------------------------
// 3. All-stubs-pass envelope reaches step 9 — default deny floor
// ---------------------------------------------------------------------------

#[tokio::test]
async fn well_formed_envelope_reaches_default_deny_floor() {
    // T-017 has steps 3..=8 stubbed as pass-through, step 1 + 2 real and trivially
    // passing for a well-formed envelope, and step 9 as the mandatory deny floor.
    // S2.3 §11: default deny is constitutional.
    let env = make_envelope();
    let ctx = make_context();
    let kernel = InMemoryPolicyKernel::new();

    let decision = kernel
        .evaluate_policy(&env, &ctx)
        .await
        .expect("evaluation must succeed");

    assert_eq!(decision.decision, Decision::Deny);
    assert_eq!(
        decision.reason_code,
        reason_code::DEFAULT_DENY,
        "S2.3 §11: well-formed envelope with no matching rule must hit DefaultDeny"
    );
}

// ---------------------------------------------------------------------------
// 4. PolicyDecision has all 14 S2.3 §4 fields populated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn in_memory_kernel_returns_fully_populated_policy_decision() {
    let env = make_envelope();
    let ctx = make_context();
    let kernel = InMemoryPolicyKernel::new();

    let decision = kernel
        .evaluate_policy(&env, &ctx)
        .await
        .expect("evaluation must succeed");

    // Field 1
    assert!(
        decision.policy_decision_id.starts_with("poldec_"),
        "S2.3 §4 field 1: policy_decision_id prefix `poldec_`, got {}",
        decision.policy_decision_id
    );
    assert!(
        decision.policy_decision_id.len() > "poldec_".len(),
        "policy_decision_id must include a ULID body"
    );

    // Field 2 — action_id is a content-addressed ActionId derived from the request.
    assert!(
        !decision.action_id.as_str().is_empty(),
        "S2.3 §4 field 2: action_id must be non-empty"
    );

    // Field 3
    assert!(
        !decision.request_hash.is_empty(),
        "S2.3 §4 field 3: request_hash must be populated"
    );
    assert_eq!(
        decision.request_hash.len(),
        64,
        "request_hash is a 64-char lowercase hex BLAKE3 digest"
    );

    // Field 4
    assert_eq!(decision.bundle_version, ctx.bundle_version);

    // Field 5
    assert_eq!(decision.enrichment_snapshot_id, ctx.enrichment.snapshot_id);

    // Fields 6 + 7 + 8 — covered by other tests but assert non-emptiness here.
    assert!(matches!(decision.decision, Decision::Deny));
    assert!(!decision.reason_code.is_empty());
    assert!(!decision.reason_message.is_empty());

    // Field 9 + 10 — stub defaults are fine for T-017.
    let _: &aios_policy::Constraints = &decision.constraints;
    let _: &aios_policy::ApprovalRequirement = &decision.approval;

    // Field 11 — empty in T-017 (evidence emission hook is M5+).
    assert_eq!(decision.evidence_receipt_id, "");

    // Field 12 — evaluated_at populated (tested more thoroughly below).
    let _: chrono::DateTime<chrono::Utc> = decision.evaluated_at;

    // Field 13
    assert_eq!(
        decision.rules_consulted, 0,
        "T-017 stubs do not consult bundle rules; T-018+ will increment this"
    );

    // Field 14
    assert!(
        !decision.simulated,
        "evaluate_policy is the LIVE path; only SimulatePolicy sets simulated = true"
    );
}

// ---------------------------------------------------------------------------
// 5. bundle_version is mirrored from PolicyContext input
// ---------------------------------------------------------------------------

#[tokio::test]
async fn bundle_version_reflects_policy_context_input() {
    let env = make_envelope();
    let ctx = PolicyContext::new(
        make_subject(),
        EnrichmentSnapshot {
            snapshot_id: "snap_custom".to_owned(),
            ..Default::default()
        },
        "polb_alt_bundle_42",
        "code_alt",
    );
    let kernel = InMemoryPolicyKernel::new();

    let decision = kernel
        .evaluate_policy(&env, &ctx)
        .await
        .expect("evaluation must succeed");

    assert_eq!(decision.bundle_version, "polb_alt_bundle_42");
    assert_eq!(decision.enrichment_snapshot_id, "snap_custom");
}

// ---------------------------------------------------------------------------
// 6. evaluated_at is within 5s of test execution
// ---------------------------------------------------------------------------

#[tokio::test]
async fn evaluated_at_is_within_five_seconds_of_test_execution() {
    let before = Utc::now();
    let kernel = InMemoryPolicyKernel::new();
    let decision = kernel
        .evaluate_policy(&make_envelope(), &make_context())
        .await
        .expect("evaluation must succeed");
    let after = Utc::now();

    let window = Duration::seconds(5);
    assert!(
        decision.evaluated_at >= before - window,
        "evaluated_at {} is more than 5s before test start {}",
        decision.evaluated_at,
        before
    );
    assert!(
        decision.evaluated_at <= after + window,
        "evaluated_at {} is more than 5s after test end {}",
        decision.evaluated_at,
        after
    );
}

// ---------------------------------------------------------------------------
// 7. Two consecutive evaluations mint distinct policy_decision_id ULIDs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn two_consecutive_evaluations_mint_distinct_policy_decision_ids() {
    let kernel = InMemoryPolicyKernel::new();
    let env = make_envelope();
    let ctx = make_context();

    let d1 = kernel
        .evaluate_policy(&env, &ctx)
        .await
        .expect("first evaluation");
    let d2 = kernel
        .evaluate_policy(&env, &ctx)
        .await
        .expect("second evaluation");

    assert_ne!(
        d1.policy_decision_id, d2.policy_decision_id,
        "S2.3 §4 field 1: ULID minting must produce distinct ids per evaluation"
    );

    // The deterministic content fields (request_hash) MUST coincide because the
    // envelope payload is the same — this is the S2.3 §13 determinism anchor on
    // content-addressed components. `action_id` will become deterministic once the
    // envelope carries the Capability-Runtime-minted id (T-002 / T-006); today it is
    // a fresh ULID per evaluation, so we do NOT assert equality on it.
    assert_eq!(d1.request_hash, d2.request_hash);
}

// ---------------------------------------------------------------------------
// 8. PipelineState short-circuit prevents later steps from running
// ---------------------------------------------------------------------------

#[test]
fn pipeline_short_circuit_at_step_1_prevents_later_steps_from_running() {
    // Drive the pipeline manually and verify that when step 1 short-circuits, the
    // later steps are not executed. We do this by:
    //  a) building a schema-invalid envelope,
    //  b) calling step_1 directly and asserting we get ShortCircuit,
    //  c) verifying the full driver returns the SAME decision and never reaches the
    //     default-deny floor (i.e. reason_code remains SchemaInvalid, not DefaultDeny).
    let pipeline = DecisionPipeline::new();
    let mut env = make_envelope();
    env.request.action = String::new(); // makes step 1 short-circuit
    let ctx = make_context();
    let req_hash = env.request.request_hash().unwrap_or_default();

    // (b)
    let state = pipeline.step_1_validate_schema(&env, &ctx, &req_hash);
    let step_1_decision = match state {
        PipelineState::ShortCircuit(d) => *d,
        PipelineState::Continue => panic!("step 1 must short-circuit on empty action"),
    };
    assert_eq!(step_1_decision.reason_code, reason_code::SCHEMA_INVALID);

    // (c)
    let full_decision = pipeline.evaluate(&env, &ctx);
    assert_eq!(
        full_decision.reason_code,
        reason_code::SCHEMA_INVALID,
        "full pipeline must terminate at step 1; never reach DefaultDeny"
    );
    assert_ne!(
        full_decision.reason_code,
        reason_code::DEFAULT_DENY,
        "step 9 must not fire when step 1 has already short-circuited"
    );
}

// ---------------------------------------------------------------------------
// 9. Bonus — step 9 unconditionally short-circuits when reached
// ---------------------------------------------------------------------------

#[test]
fn step_9_always_short_circuits_with_default_deny_when_reached() {
    // S2.3 §11: default deny is the mandatory floor. Reaching step 9 must always
    // yield ShortCircuit, never Continue, because the spec forbids silent fall-through.
    let pipeline = DecisionPipeline::new();
    let env = make_envelope();
    let ctx = make_context();
    let req_hash = env
        .request
        .request_hash()
        .expect("request_hash must succeed");

    let state = pipeline.step_9_apply_default_deny(&env, &ctx, &req_hash);
    match state {
        PipelineState::ShortCircuit(d) => {
            assert_eq!(d.decision, Decision::Deny);
            assert_eq!(d.reason_code, reason_code::DEFAULT_DENY);
        }
        PipelineState::Continue => panic!(
            "S2.3 §11 mandates default deny as a hard floor; step 9 must always short-circuit"
        ),
    }
}

// ---------------------------------------------------------------------------
// 10. Bonus — stub steps (3..=8) return Continue (compile-time const test)
// ---------------------------------------------------------------------------

#[test]
fn stub_steps_return_continue() {
    // T-017 contract: steps 3..=8 are stubs and must pass through. T-018..T-025 will
    // replace each with the real impl; this test pins the current stub contract so a
    // future commit that accidentally short-circuits one of these steps without
    // landing the real engine is caught here.
    assert_eq!(
        DecisionPipeline::step_3_enrich_resources(),
        PipelineState::Continue
    );
    assert_eq!(
        DecisionPipeline::step_4_evaluate_hard_denies(),
        PipelineState::Continue
    );
    assert_eq!(
        DecisionPipeline::step_5_emergency_override_denylist(),
        PipelineState::Continue
    );
    assert_eq!(
        DecisionPipeline::step_6_evaluate_scoped_denies(),
        PipelineState::Continue
    );
    assert_eq!(
        DecisionPipeline::step_7_evaluate_scoped_allows(),
        PipelineState::Continue
    );
    assert_eq!(
        DecisionPipeline::step_8_ai_self_approval_prevention(),
        PipelineState::Continue
    );
}
