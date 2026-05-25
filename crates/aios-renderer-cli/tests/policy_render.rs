//! Tests for policy renderable implementations.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_action::ActionId;
use aios_policy::{
    ApprovalRequirement, ApprovalScope, ApproverClass, Constraints, Decision, EvidenceGrade,
    NetworkPolicy, PolicyDecision, SandboxProfileId, SessionClass, VaultCapabilityId,
};
use aios_renderer_cli::{OutputFormat, RenderContext, Renderable};
use chrono::{TimeZone, Utc};

fn ctx(color: bool) -> RenderContext {
    RenderContext {
        color,
        width: Some(220),
        redact_secrets: true,
        verbose: false,
        locale: "en_US.UTF-8".to_owned(),
    }
}

const fn formats() -> [OutputFormat; 4] {
    [
        OutputFormat::Text,
        OutputFormat::Json,
        OutputFormat::Tree,
        OutputFormat::Table,
    ]
}

fn policy_decision(decision: Decision) -> PolicyDecision {
    PolicyDecision {
        policy_decision_id: "poldec_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        action_id: ActionId::parse("act_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("valid action id"),
        request_hash: "0123456789abcdef0123456789abcdef".to_owned(),
        bundle_version: "bundle-2026.05.25.r1".to_owned(),
        enrichment_snapshot_id: "snap_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        decision,
        reason_code: format!("Fixture{decision:?}"),
        reason_message: format!("Fixture policy rendered {decision:?}"),
        constraints: constraints(),
        approval: approval_requirement(decision == Decision::RequireApproval),
        evidence_receipt_id: "evr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        evaluated_at: Utc
            .with_ymd_and_hms(2026, 5, 25, 9, 0, 0)
            .single()
            .expect("valid timestamp"),
        rules_consulted: 3,
        simulated: false,
    }
}

fn constraints() -> Constraints {
    Constraints {
        sandbox_profile_id: Some(SandboxProfileId("host-service-control".to_owned())),
        max_runtime_seconds: Some(45),
        verification_required: true,
        dry_run_only: false,
        require_evidence_grade: Some(EvidenceGrade::E3),
        require_human_co_signer: true,
        network_policy: Some(NetworkPolicy::LanAllowed),
        max_concurrent_per_subject: Some(2),
        min_subject_session_class: Some(SessionClass::Internal),
        vault_capability_required: Some(VaultCapabilityId(
            "cap_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        )),
        ttl_seconds: 300,
        expires_at: None,
    }
}

fn approval_requirement(required: bool) -> ApprovalRequirement {
    ApprovalRequirement {
        required,
        approval_scope: ApprovalScope::ExactRequestHash,
        ttl_seconds: 120,
        approver_classes: vec![ApproverClass::Human, ApproverClass::Operator],
        require_human_co_signer: true,
    }
}

#[test]
fn policy_decision_allow_renders_in_all_formats() {
    let decision = policy_decision(Decision::Allow);

    for format in formats() {
        let rendered = decision
            .render(format, &ctx(false))
            .expect("render allow decision");
        assert!(rendered.contains("FixtureAllow"), "{format:?}: {rendered}");
        assert!(rendered.contains("Fixture policy rendered Allow"));
    }
}

#[test]
fn policy_decision_deny_renders_in_all_formats() {
    let decision = policy_decision(Decision::Deny);

    for format in formats() {
        let rendered = decision
            .render(format, &ctx(false))
            .expect("render deny decision");
        assert!(rendered.contains("FixtureDeny"), "{format:?}: {rendered}");
        assert!(rendered.contains("Fixture policy rendered Deny"));
    }
}

#[test]
fn policy_decision_require_approval_renders_in_all_formats() {
    let decision = policy_decision(Decision::RequireApproval);

    for format in formats() {
        let rendered = decision
            .render(format, &ctx(false))
            .expect("render require approval decision");
        assert!(
            rendered.contains("FixtureRequireApproval"),
            "{format:?}: {rendered}"
        );
        assert!(rendered.contains("Fixture policy rendered RequireApproval"));
    }
}

#[test]
fn decision_enum_uses_required_colors_when_color_enabled() {
    let allow = Decision::Allow
        .render(OutputFormat::Text, &ctx(true))
        .expect("render allow");
    let deny = Decision::Deny
        .render(OutputFormat::Text, &ctx(true))
        .expect("render deny");
    let approval = Decision::RequireApproval
        .render(OutputFormat::Text, &ctx(true))
        .expect("render require approval");

    assert!(allow.contains("\u{1b}[32mAllow\u{1b}[0m"));
    assert!(deny.contains("\u{1b}[31mDeny\u{1b}[0m"));
    assert!(approval.contains("\u{1b}[33mRequireApproval\u{1b}[0m"));
}

#[test]
fn constraints_render_core_execution_limits() {
    let constraints = constraints();

    for format in formats() {
        let rendered = constraints
            .render(format, &ctx(false))
            .expect("render constraints");
        assert!(rendered.contains("ttl_seconds"), "{format:?}: {rendered}");
        assert!(rendered.contains("host-service-control"));
        assert!(rendered.contains("cap_01HXY8K2JPQ7N3M4R5S6T7V8W9"));
    }
}

#[test]
fn approval_requirement_renders_scope_and_approver_classes() {
    let approval = approval_requirement(true);

    for format in formats() {
        let rendered = approval
            .render(format, &ctx(false))
            .expect("render approval requirement");
        assert!(rendered.contains("ttl_seconds"), "{format:?}: {rendered}");
        assert!(rendered.contains("required"));
    }

    let text = approval
        .render(OutputFormat::Text, &ctx(false))
        .expect("render approval text");
    let json = approval
        .render(OutputFormat::Json, &ctx(false))
        .expect("render approval json");

    assert!(text.contains("ExactRequestHash"));
    assert!(text.contains("Human, Operator"));
    assert!(json.contains("\"approval_scope\":\"exact_request_hash\""));
    assert!(json.contains("\"operator\""));
}
