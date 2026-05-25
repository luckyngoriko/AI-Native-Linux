//! Tests for action/runtime renderable implementations.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_action::{ActionEnvelope, ActionId, Identity, Request, Trace};
use aios_capability_runtime::{
    ActionContext, ActionDispatchKind, ActionLifecycleState, ExecutionFailureReason, QueueClass,
    RollbackOutcome,
};
use aios_renderer_cli::{OutputFormat, RenderContext, Renderable};
use chrono::{TimeZone, Utc};
use strum::IntoEnumIterator;

fn ctx(color: bool) -> RenderContext {
    RenderContext {
        color,
        width: Some(160),
        redact_secrets: true,
        verbose: false,
        locale: "en_US.UTF-8".to_owned(),
    }
}

fn action_id() -> ActionId {
    ActionId::parse("act_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("valid action id")
}

fn context_with_evidence(evidence_chain: Vec<String>) -> ActionContext {
    let now = Utc
        .with_ymd_and_hms(2026, 5, 25, 8, 30, 0)
        .single()
        .expect("valid timestamp");
    let mut context = ActionContext::new(
        action_id(),
        ActionDispatchKind::SubprocessFork,
        QueueClass::AgentProposal,
        now,
    );
    context.status = ActionLifecycleState::Succeeded;
    context.evidence_chain = evidence_chain;
    context
}

#[test]
fn action_context_text_includes_action_id_status_and_queue_class() {
    let context = context_with_evidence(Vec::new());

    let rendered = context
        .render(OutputFormat::Text, &ctx(false))
        .expect("render context text");

    assert!(rendered.contains("action_id: act_01HXY8K2JPQ7N3M4R5S6T7V8W9"));
    assert!(rendered.contains("status: Succeeded"));
    assert!(rendered.contains("queue_class: AgentProposal"));
}

#[test]
fn action_context_json_round_trips_through_serde_json() {
    let context = context_with_evidence(vec!["evr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned()]);

    let rendered = context
        .render(OutputFormat::Json, &ctx(false))
        .expect("render context json");
    let reparsed: ActionContext = serde_json::from_str(&rendered).expect("parse context json");

    assert_eq!(reparsed, context);
}

#[test]
fn action_context_tree_shows_hierarchy() {
    let context = context_with_evidence(vec!["evr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned()]);

    let rendered = context
        .render(OutputFormat::Tree, &ctx(false))
        .expect("render context tree");

    assert!(rendered.starts_with("Action act_01HXY8K2JPQ7N3M4R5S6T7V8W9"));
    assert!(rendered.contains("status: Succeeded"));
    assert!(rendered.contains("dispatch_kind: SubprocessFork"));
    assert!(rendered.contains("queue_class: AgentProposal"));
    assert!(rendered.contains("evidence_chain: count=1"));
    assert!(rendered.contains("evr_01HXY8K2JPQ7..."));
}

#[test]
fn action_context_table_produces_one_data_row() {
    let context = context_with_evidence(Vec::new());

    let rendered = context
        .render(OutputFormat::Table, &ctx(false))
        .expect("render context table");

    assert!(rendered.contains("action_id"));
    assert!(rendered.contains("status"));
    assert!(rendered.contains("adapter"));
    assert!(rendered.contains("duration_ms"));
    assert_eq!(
        rendered
            .matches("| act_01HXY8K2JPQ7N3M4R5S6T7V8W9 |")
            .count(),
        1
    );
}

#[test]
fn succeeded_state_renders_green_when_color_enabled() {
    let rendered = ActionLifecycleState::Succeeded
        .render(OutputFormat::Text, &ctx(true))
        .expect("render succeeded");

    assert!(rendered.contains("\u{1b}[32mSucceeded\u{1b}[0m"));
}

#[test]
fn succeeded_state_renders_plain_when_color_disabled() {
    let rendered = ActionLifecycleState::Succeeded
        .render(OutputFormat::Text, &ctx(false))
        .expect("render succeeded");

    assert_eq!(rendered, "status: Succeeded");
    assert!(!rendered.contains("\u{1b}["));
}

#[test]
fn failed_state_renders_red_or_plain_by_color_context() {
    let colored = ActionLifecycleState::Failed
        .render(OutputFormat::Text, &ctx(true))
        .expect("render failed colored");
    let plain = ActionLifecycleState::Failed
        .render(OutputFormat::Text, &ctx(false))
        .expect("render failed plain");

    assert!(colored.contains("\u{1b}[31mFailed\u{1b}[0m"));
    assert_eq!(plain, "status: Failed");
    assert!(!plain.contains("\u{1b}["));
}

#[test]
fn rollback_outcome_each_variant_renders() {
    for outcome in RollbackOutcome::iter() {
        let rendered = outcome
            .render(OutputFormat::Text, &ctx(false))
            .expect("render rollback outcome");

        assert!(rendered.contains(&format!("{outcome:?}")));
    }
}

#[test]
fn execution_failure_reason_each_variant_renders() {
    for reason in ExecutionFailureReason::iter() {
        let rendered = reason
            .render(OutputFormat::Text, &ctx(false))
            .expect("render failure reason");

        assert!(rendered.contains(&format!("{reason:?}")));
    }
}

#[test]
fn action_envelope_summary_shows_identity_and_target() {
    let envelope = action_envelope();

    let rendered = envelope
        .render(OutputFormat::Text, &ctx(false))
        .expect("render envelope");

    assert!(rendered.contains("subject_canonical_id: human:operator"));
    assert!(rendered.contains("target: {\"service\":\"nginx\"}"));
}

#[test]
fn evidence_chain_with_five_receipts_shows_count_and_first_three_truncated_ids() {
    let context = context_with_evidence(vec![
        "evr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        "evr_01HXY8K2JPQ7N3M4R5S6T7V8X0".to_owned(),
        "evr_01HXY8K2JPQ7N3M4R5S6T7V8X1".to_owned(),
        "evr_01HXY8K2JPQ7N3M4R5S6T7V8X2".to_owned(),
        "evr_01HXY8K2JPQ7N3M4R5S6T7V8X3".to_owned(),
    ]);

    let rendered = context
        .render(OutputFormat::Text, &ctx(false))
        .expect("render evidence chain");

    assert!(rendered.contains("evidence_chain: count=5"));
    assert!(rendered.contains("evr_01HXY8K2JPQ7..."));
    assert!(rendered.contains("evr_01HXY8K2JPQ7..., evr_01HXY8K2JPQ7..., evr_01HXY8K2JPQ7..."));
    assert!(!rendered.contains("V8X2"));
    assert!(!rendered.contains("V8X3"));
}

#[test]
fn evidence_chain_with_zero_receipts_renders_no_evidence_line() {
    let context = context_with_evidence(Vec::new());

    let rendered = context
        .render(OutputFormat::Text, &ctx(false))
        .expect("render empty evidence chain");

    assert!(rendered.contains("evidence_chain: (no evidence)"));
}

fn action_envelope() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("human:operator", false),
        Request::new("service.restart", serde_json::json!({"service": "nginx"})),
        Trace::new("11111111111111111111111111111111", "2222222222222222", None),
    )
}
