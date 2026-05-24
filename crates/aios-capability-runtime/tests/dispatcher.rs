//! T-029 integration tests for [`ActionDispatcher`] — §3.2 dispatch-kind
//! decision, §3.5 queue-class selection, and §11.4 AI-interactive
//! downgrade marker.
//!
//! Each test pins one row of the spec's closed decision tables; the
//! coverage is intentionally exhaustive over the table dimensions
//! (`is_ai × is_simulate × risk_privileged × manifest_kind × stability`).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::Utc;

use aios_action::ActionId;
use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_capability_runtime::adapter_manifest::{AdapterActionDeclaration, AdapterManifest};
use aios_capability_runtime::{
    fresh_context, ActionDispatchKind, ActionDispatcher, ActionLifecycleState, AdapterIOMode,
    AdapterStability, CapabilityRuntime, DispatchQueue, InMemoryCapabilityRuntime, QueueClass,
    RuntimeContext, AI_INTERACTIVE_DOWNGRADE_MARKER,
};

// ---------------------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------------------

fn make_envelope(subject: &str, is_ai: bool) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new(subject, is_ai),
        Request::new("service.restart", serde_json::json!({})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn make_manifest(kind: ActionDispatchKind, stability: AdapterStability) -> AdapterManifest {
    AdapterManifest {
        adapter_id: "adapter:test:t029:0.0.1".to_owned(),
        adapter_version: "0.0.1".to_owned(),
        vendor: "test".to_owned(),
        name: "t029".to_owned(),
        declared_stability: stability,
        io_mode: AdapterIOMode::TypedParametersOnly,
        dispatch_kind: kind,
        declared_actions: vec![AdapterActionDeclaration {
            action_kind: "service.restart".to_owned(),
            target_schema: serde_json::json!({}),
            response_schema: serde_json::json!({}),
            rollback_strategy: "NONE".to_owned(),
            timeout_seconds: 30,
            template_string: None,
            template_substitution_variables: vec![],
        }],
        declared_invariants_supported: vec![],
        default_adapter_timeout_seconds: 30,
        default_sandbox_profile_id: "sbx_default".to_owned(),
        adapter_signature: "0".repeat(128),
        signing_key_id: "key_test".to_owned(),
        manifest_created_at: Utc::now(),
        manifest_expires_at: Utc::now() + chrono::Duration::days(365),
    }
}

// ---------------------------------------------------------------------------
// 1. select_queue_class — AI subject → AGENT_PROPOSAL.
// ---------------------------------------------------------------------------

#[test]
fn select_queue_class_for_ai_subject_is_agent_proposal() {
    let env = make_envelope("ai:agent:alpha", true);
    assert_eq!(
        ActionDispatcher::select_queue_class(&env, false),
        QueueClass::AgentProposal
    );
}

// ---------------------------------------------------------------------------
// 2. select_queue_class — human subject → INTERACTIVE.
// ---------------------------------------------------------------------------

#[test]
fn select_queue_class_for_human_subject_is_interactive() {
    let env = make_envelope("human:lucky", false);
    assert_eq!(
        ActionDispatcher::select_queue_class(&env, false),
        QueueClass::Interactive
    );
}

// ---------------------------------------------------------------------------
// 3. select_queue_class — recovery_mode forces RECOVERY_PRIORITY.
// ---------------------------------------------------------------------------

#[test]
fn select_queue_class_recovery_mode_forces_recovery_priority() {
    // §3.5 row 4 — "Any action while host.recovery_mode = true; preempts
    // all other classes". Both human and AI envelopes route here.
    let env_human = make_envelope("human:lucky", false);
    let env_ai = make_envelope("ai:agent:alpha", true);
    assert_eq!(
        ActionDispatcher::select_queue_class(&env_human, true),
        QueueClass::RecoveryPriority
    );
    assert_eq!(
        ActionDispatcher::select_queue_class(&env_ai, true),
        QueueClass::RecoveryPriority
    );
}

// ---------------------------------------------------------------------------
// 4. select_dispatch_kind — §3.2 truth table cases.
// ---------------------------------------------------------------------------

#[test]
fn select_dispatch_kind_dry_run_simulate_wins() {
    // §3.2 rule 1 — `request.dry_run == SIMULATE` → DRY_RUN regardless.
    let m = make_manifest(ActionDispatchKind::SubprocessFork, AdapterStability::Stable);
    let kind = ActionDispatcher::select_dispatch_kind(
        &m, /* is_ai */ true, /* is_simulate */ true, /* risk_privileged */ true,
    );
    assert_eq!(kind, ActionDispatchKind::DryRun);
}

#[test]
fn select_dispatch_kind_ai_forces_isolated_sandbox() {
    // §3.2 rule 2 — AI-origin → ISOLATED_SANDBOX even if manifest says
    // IN_PROCESS_RPC.
    let m = make_manifest(ActionDispatchKind::InProcessRpc, AdapterStability::Stable);
    let kind = ActionDispatcher::select_dispatch_kind(&m, true, false, false);
    assert_eq!(kind, ActionDispatchKind::IsolatedSandbox);
}

#[test]
fn select_dispatch_kind_privileged_forces_isolated_sandbox() {
    // §3.2 rule 3 — risk.privileged → ISOLATED_SANDBOX.
    let m = make_manifest(ActionDispatchKind::InProcessRpc, AdapterStability::Stable);
    let kind = ActionDispatcher::select_dispatch_kind(&m, false, false, true);
    assert_eq!(kind, ActionDispatchKind::IsolatedSandbox);
}

#[test]
fn select_dispatch_kind_manifest_subprocess_fork_honoured() {
    // §3.2 rule 4 — manifest SUBPROCESS_FORK → SUBPROCESS_FORK (no
    // upgrade).
    let m = make_manifest(ActionDispatchKind::SubprocessFork, AdapterStability::Stable);
    let kind = ActionDispatcher::select_dispatch_kind(&m, false, false, false);
    assert_eq!(kind, ActionDispatchKind::SubprocessFork);
}

#[test]
fn select_dispatch_kind_manifest_in_process_rpc_requires_stable() {
    // §3.2 rule 5 — IN_PROCESS_RPC only when manifest STABLE.
    let m_stable = make_manifest(ActionDispatchKind::InProcessRpc, AdapterStability::Stable);
    let kind_stable = ActionDispatcher::select_dispatch_kind(&m_stable, false, false, false);
    assert_eq!(kind_stable, ActionDispatchKind::InProcessRpc);

    // §3.2 line 140 — EXPERIMENTAL adapters never run in-process; fall
    // back to SUBPROCESS_FORK (rule 6 terminus).
    let m_exp = make_manifest(
        ActionDispatchKind::InProcessRpc,
        AdapterStability::Experimental,
    );
    let kind_exp = ActionDispatcher::select_dispatch_kind(&m_exp, false, false, false);
    assert_eq!(kind_exp, ActionDispatchKind::SubprocessFork);
}

// ---------------------------------------------------------------------------
// 5. apply_ai_interactive_downgrade — AI + INTERACTIVE → marker.
// ---------------------------------------------------------------------------

#[test]
fn ai_interactive_downgrade_marker_for_ai_on_interactive() {
    let mut ctx = fresh_context(ActionId::new(), Utc::now());
    ctx.queue_class = QueueClass::Interactive;
    let marker = ActionDispatcher::apply_ai_interactive_downgrade(&ctx, /* is_ai */ true);
    assert_eq!(marker, Some(AI_INTERACTIVE_DOWNGRADE_MARKER));
}

// ---------------------------------------------------------------------------
// 6. apply_ai_interactive_downgrade — human + INTERACTIVE → None.
// ---------------------------------------------------------------------------

#[test]
fn ai_interactive_downgrade_no_marker_for_human() {
    let mut ctx = fresh_context(ActionId::new(), Utc::now());
    ctx.queue_class = QueueClass::Interactive;
    let marker = ActionDispatcher::apply_ai_interactive_downgrade(&ctx, /* is_ai */ false);
    assert_eq!(marker, None);
}

// ---------------------------------------------------------------------------
// 7. apply_ai_interactive_downgrade — AI + BACKGROUND → None.
// ---------------------------------------------------------------------------

#[test]
fn ai_interactive_downgrade_no_marker_for_ai_on_background() {
    let mut ctx = fresh_context(ActionId::new(), Utc::now());
    ctx.queue_class = QueueClass::Background;
    let marker = ActionDispatcher::apply_ai_interactive_downgrade(&ctx, /* is_ai */ true);
    assert_eq!(marker, None);
}

// ---------------------------------------------------------------------------
// 8. End-to-end through the runtime: AI envelope drives full
//    select_queue_class → enroll → succeed path with the dispatch queue
//    attached.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispatcher_e2e_ai_envelope_through_runtime() {
    let queue = Arc::new(DispatchQueue::new_with_defaults());
    let runtime = InMemoryCapabilityRuntime::new().with_dispatch_queue(Arc::clone(&queue));
    let env = make_envelope("ai:agent:alpha", true);
    let rctx = RuntimeContext::new("ai:agent:alpha", "polb_t029_e2e_v1", "code_t029_e2e");
    let final_ctx = runtime
        .submit_action(&env, &rctx)
        .await
        .expect("happy path");
    // Dispatcher selected AGENT_PROPOSAL (§11.4 routes AI envelopes onto
    // the fairness-bounded class), the action enrolled successfully, and
    // the stubbed verify path drove it through to SUCCEEDED.
    assert_eq!(final_ctx.queue_class, QueueClass::AgentProposal);
    assert_eq!(final_ctx.status, ActionLifecycleState::Succeeded);
    // The dispatch_kind is left at the T-026 fresh_context seed because
    // no adapter registry is attached in this E2E — the §3.2 dispatch
    // selection requires a manifest. T-035 closes the loop end-to-end.
    assert_eq!(final_ctx.dispatch_kind, ActionDispatchKind::SubprocessFork);
}
