//! T-021 integration tests — AI self-approval prevention (S2.3 §17).
//!
//! Pins the constitutional invariant from §17: an AI subject (`is_ai = true`)
//! cannot self-approve any action with at least one risk flag set. The
//! pipeline upgrades a scoped `ALLOW` to `REQUIRE_APPROVAL` with
//! `reason_code = "AiSelfApprovalUpgrade"` and `approval.approver_classes =
//! [Human]`.
//!
//! The pure §17 evaluator [`evaluate_ai_self_approval_prevention`] is exercised
//! directly, and the full end-to-end pipeline path is exercised via
//! [`DecisionPipeline::apply_step_8`] applied to a synthetic scoped-ALLOW
//! partial state (the §3 step 7 stub means no scoped ALLOW lands on the
//! end-to-end path until T-022; T-021 anchors the §17 contract independent of
//! the rule-index implementation).

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use chrono::Utc;

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_policy::{
    evaluate_ai_self_approval_prevention, ApprovalRequirement, ApprovalScope, ApproverClass,
    Constraints, Decision, DecisionPipeline, HydratedSubject, PolicyDecision, SubjectType,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn human_subject() -> HydratedSubject {
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

fn agent_subject() -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: "agent:dev:01HX1111111111111111111111".to_owned(),
        subject_type: SubjectType::Agent,
        groups: vec!["cognitive-core".to_owned()],
        capabilities: vec![],
        session_class: "INTERNAL".to_owned(),
        recovery_mode: false,
        is_ai: true,
    }
}

fn envelope_with_risk(privileged: bool) -> ActionEnvelope {
    let target = serde_json::json!({
        "service": "nginx",
        "risk": {
            "destructive": false,
            "privileged": privileged,
            "network_exposure": false,
            "secret_access": false,
            "recovery_path_affected": false,
        }
    });
    ActionEnvelope::new(
        Identity::new("agent:dev", true),
        Request::new("package.install", target),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn envelope_no_risk() -> ActionEnvelope {
    let target = serde_json::json!({
        "service": "nginx",
        "risk": {
            "destructive": false,
            "privileged": false,
            "network_exposure": false,
            "secret_access": false,
            "recovery_path_affected": false,
        }
    });
    ActionEnvelope::new(
        Identity::new("agent:dev", true),
        Request::new("service.status", target),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

/// Mint a synthetic scoped-ALLOW partial decision so the step-8 wiring can
/// be exercised independently of the T-022 rule index.
fn synthetic_scoped_allow() -> PolicyDecision {
    PolicyDecision {
        policy_decision_id: "poldec_TESTSCOPEDALLOW".to_owned(),
        action_id: aios_action::ActionId::new(),
        request_hash: "abcd".to_owned(),
        bundle_version: "polb_t021_v1".to_owned(),
        enrichment_snapshot_id: "snap_t021".to_owned(),
        decision: Decision::Allow,
        reason_code: "ScopedAllow".to_owned(),
        reason_message: "synthetic scoped-allow partial state for §17 test".to_owned(),
        constraints: Constraints::default(),
        approval: ApprovalRequirement::default(),
        evidence_receipt_id: String::new(),
        evaluated_at: Utc::now(),
        rules_consulted: 1,
        simulated: false,
    }
}

/// Mint a synthetic scoped-DENY partial decision to verify §17 never
/// downgrades a DENY.
fn synthetic_scoped_deny() -> PolicyDecision {
    PolicyDecision {
        policy_decision_id: "poldec_TESTSCOPEDDENY".to_owned(),
        action_id: aios_action::ActionId::new(),
        request_hash: "abcd".to_owned(),
        bundle_version: "polb_t021_v1".to_owned(),
        enrichment_snapshot_id: "snap_t021".to_owned(),
        decision: Decision::Deny,
        reason_code: "ScopedDeny".to_owned(),
        reason_message: "synthetic scoped-deny partial state for §17 test".to_owned(),
        constraints: Constraints::default(),
        approval: ApprovalRequirement::default(),
        evidence_receipt_id: String::new(),
        evaluated_at: Utc::now(),
        rules_consulted: 1,
        simulated: false,
    }
}

// ---------------------------------------------------------------------------
// 1. AI + ALLOW + risk → REQUIRE_APPROVAL with AiSelfApprovalUpgrade
// ---------------------------------------------------------------------------

#[test]
fn ai_subject_with_privileged_action_upgrades_allow_to_require_approval() {
    let env = envelope_with_risk(true);
    let upgraded = DecisionPipeline::apply_step_8(synthetic_scoped_allow(), &agent_subject(), &env);

    assert_eq!(upgraded.decision, Decision::RequireApproval);
    assert_eq!(upgraded.reason_code, "AiSelfApprovalUpgrade");
    assert!(upgraded.approval.required, "approval must be required");
}

// ---------------------------------------------------------------------------
// 2. Human + ALLOW → unchanged (§17 only fires for AI)
// ---------------------------------------------------------------------------

#[test]
fn human_subject_with_privileged_action_remains_allow() {
    let env = envelope_with_risk(true);
    let original = synthetic_scoped_allow();
    let result = DecisionPipeline::apply_step_8(original.clone(), &human_subject(), &env);
    assert_eq!(result.decision, Decision::Allow);
    assert_eq!(result.reason_code, original.reason_code);
}

// ---------------------------------------------------------------------------
// 3. AI + DENY → unchanged (§17 never downgrades)
// ---------------------------------------------------------------------------

#[test]
fn ai_subject_with_deny_decision_remains_deny() {
    let env = envelope_with_risk(true);
    let original = synthetic_scoped_deny();
    let original_reason = original.reason_code.clone();
    let result = DecisionPipeline::apply_step_8(original, &agent_subject(), &env);
    assert_eq!(result.decision, Decision::Deny);
    assert_eq!(
        result.reason_code, original_reason,
        "§17.2: §17 never downgrades a DENY"
    );
}

// ---------------------------------------------------------------------------
// 4. §17.3 carve-out — AI + ALLOW + all-flags-false → unchanged ALLOW
// ---------------------------------------------------------------------------

#[test]
fn ai_subject_with_no_risk_flags_keeps_allow_per_section_17_3() {
    // §17.3 carve-out: self-management low-risk actions may self-approve.
    let env = envelope_no_risk();
    let result = DecisionPipeline::apply_step_8(synthetic_scoped_allow(), &agent_subject(), &env);
    assert_eq!(
        result.decision,
        Decision::Allow,
        "§17.3 carve-out: all-risk-false AI action stays ALLOW"
    );
    assert_eq!(result.reason_code, "ScopedAllow");
}

// ---------------------------------------------------------------------------
// 5. Upgraded approval has approver_classes = [Human]
// ---------------------------------------------------------------------------

#[test]
fn upgraded_decision_carries_human_approver_class() {
    let env = envelope_with_risk(true);
    let upgraded = DecisionPipeline::apply_step_8(synthetic_scoped_allow(), &agent_subject(), &env);

    assert_eq!(
        upgraded.approval.approver_classes,
        vec![ApproverClass::Human],
        "§17.1: approver_classes must be [Human]"
    );
    assert_eq!(
        upgraded.approval.approval_scope,
        ApprovalScope::ExactRequestHash,
        "§15: only ExactRequestHash binding in rev.2"
    );
    assert!(
        upgraded.approval.required,
        "approval.required must be true after §17 upgrade"
    );
}

// ---------------------------------------------------------------------------
// 6. Pure evaluator — direct invariant check
// ---------------------------------------------------------------------------

#[test]
fn pure_evaluator_returns_some_only_for_ai_allow_with_risk() {
    // ALLOW + AI + risk → Some
    let env = envelope_with_risk(true);
    assert!(
        evaluate_ai_self_approval_prevention(Decision::Allow, &agent_subject(), &env).is_some()
    );

    // ALLOW + AI + no risk → None (§17.3 carve-out)
    let env_safe = envelope_no_risk();
    assert!(
        evaluate_ai_self_approval_prevention(Decision::Allow, &agent_subject(), &env_safe)
            .is_none()
    );

    // ALLOW + human + risk → None
    assert!(
        evaluate_ai_self_approval_prevention(Decision::Allow, &human_subject(), &env).is_none()
    );

    // DENY + AI + risk → None (§17.2)
    assert!(evaluate_ai_self_approval_prevention(Decision::Deny, &agent_subject(), &env).is_none());

    // REQUIRE_APPROVAL + AI + risk → None (§17 only upgrades ALLOW)
    assert!(evaluate_ai_self_approval_prevention(
        Decision::RequireApproval,
        &agent_subject(),
        &env
    )
    .is_none());
}

// ---------------------------------------------------------------------------
// 7. Decision-path log records the upgrade
// ---------------------------------------------------------------------------

#[test]
fn upgrade_preserves_original_identity_fields_for_audit() {
    // Per §13 determinism + §15 explain-decision: an upgraded decision must
    // be traceable back to its scoped-ALLOW partial state. The original
    // policy_decision_id, request_hash, bundle_version, and
    // enrichment_snapshot_id are preserved across the §17 upgrade.
    let env = envelope_with_risk(true);
    let original = synthetic_scoped_allow();
    let upgraded = DecisionPipeline::apply_step_8(original.clone(), &agent_subject(), &env);

    assert_eq!(upgraded.policy_decision_id, original.policy_decision_id);
    assert_eq!(upgraded.request_hash, original.request_hash);
    assert_eq!(upgraded.bundle_version, original.bundle_version);
    assert_eq!(
        upgraded.enrichment_snapshot_id,
        original.enrichment_snapshot_id
    );
    // §17 consults one constitutional rule; rules_consulted bumps by 1.
    assert_eq!(upgraded.rules_consulted, original.rules_consulted + 1);
    // The reason code changes — that's the audit signal.
    assert_ne!(upgraded.reason_code, original.reason_code);
    assert_eq!(upgraded.reason_code, "AiSelfApprovalUpgrade");
}

// ---------------------------------------------------------------------------
// 8. Application subject (also is_ai) triggers the same upgrade
// ---------------------------------------------------------------------------

#[test]
fn application_subject_also_triggers_section_17_upgrade() {
    let app = HydratedSubject {
        canonical_subject_id: "application:planner:01HX2222222222222222222222".to_owned(),
        subject_type: SubjectType::Application,
        groups: vec![],
        capabilities: vec![],
        session_class: "INTERNAL".to_owned(),
        recovery_mode: false,
        is_ai: true,
    };
    let env = envelope_with_risk(true);
    let upgraded = DecisionPipeline::apply_step_8(synthetic_scoped_allow(), &app, &env);
    assert_eq!(upgraded.decision, Decision::RequireApproval);
    assert_eq!(upgraded.reason_code, "AiSelfApprovalUpgrade");
}
