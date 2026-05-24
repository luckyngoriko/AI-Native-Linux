//! T-027 integration tests for the `ActionLifecyclePipeline` driver, the
//! §4.2 transition table, and the [`InMemoryCapabilityRuntime`] harness.
//!
//! Anchors:
//! - [`aios_capability_runtime::TRANSITIONS`] contains the canonical
//!   T2..T21 set from S10.1 §4.2 (the spec lists 21 rows; T1 is the
//!   constructor "(init) → CREATED" which has no `from` and is therefore
//!   not in `TRANSITIONS`).
//! - Empty / invalid envelopes short-circuit to `FAILED` with
//!   `EnvelopeValidationFailed`.
//! - Clean envelopes drive through to `SUCCEEDED`.
//! - [`aios_capability_runtime::InMemoryCapabilityRuntime`] persists the
//!   submitted [`aios_capability_runtime::ActionContext`] for later
//!   `get_action_status` lookups.
//! - Unknown action ids surface `RuntimeError::ActionNotFound`.
//! - [`aios_capability_runtime::apply_transition`] enforces the §4.2 table
//!   (illegal transitions return `RuntimeError::InvalidTransition`; the
//!   four strict terminals are non-leaving).
//! - Two concurrent `submit_action` calls on distinct envelopes do not
//!   collide.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use chrono::Utc;

use aios_action::{ActionEnvelope, ActionId, Identity, Request, Trace};
use aios_capability_runtime::{
    apply_transition, fresh_context, ActionLifecyclePipeline, ActionLifecycleState,
    CapabilityRuntime, ExecutionFailureReason, InMemoryCapabilityRuntime, RuntimeContext,
    RuntimeError, TRANSITIONS,
};

// ---------------------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------------------

fn make_envelope() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("human:lucky", false),
        Request::new("service.restart", serde_json::json!({"service": "nginx"})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn make_envelope_with_empty_action() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("human:lucky", false),
        Request::new("", serde_json::json!({})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn make_envelope_with_empty_subject() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("", false),
        Request::new("service.restart", serde_json::json!({})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn make_runtime_context() -> RuntimeContext {
    RuntimeContext::new(
        "human:lucky:01HX0000000000000000000000",
        "polb_t027_test_bundle_v1",
        "code_t027_test",
    )
}

// ---------------------------------------------------------------------------
// 1. §4.2 transition table — anchor on the 20 spec rows (T2..T21).
// ---------------------------------------------------------------------------

#[test]
fn transitions_table_contains_exactly_the_spec_listed_rows() {
    // T1 is the (init) → CREATED constructor and is not in TRANSITIONS by
    // design. Spec §4.2 lists T2..T21 = 20 rows.
    assert_eq!(
        TRANSITIONS.len(),
        20,
        "S10.1 §4.2 lists 20 explicit (from, to) transitions; TRANSITIONS has {}",
        TRANSITIONS.len()
    );

    // Spot-check the four constitutional anchors the brief calls out:
    //
    // T19 — FAILED → ROLLED_BACK (rollback succeeded).
    assert!(
        TRANSITIONS.contains(&(
            ActionLifecycleState::Failed,
            ActionLifecycleState::RolledBack
        )),
        "T19 missing"
    );
    // T20 — FAILED → ROLLBACK_FAILED (rollback failed; terminal forensic).
    assert!(
        TRANSITIONS.contains(&(
            ActionLifecycleState::Failed,
            ActionLifecycleState::RollbackFailed
        )),
        "T20 missing"
    );
    // T21 — POLICY_DENIED → OVERRIDE_PENDING (operator-authored override).
    assert!(
        TRANSITIONS.contains(&(
            ActionLifecycleState::PolicyDenied,
            ActionLifecycleState::OverridePending
        )),
        "T21 missing"
    );
    // T17 — VERIFYING → SUCCEEDED (the happy-path landing).
    assert!(
        TRANSITIONS.contains(&(
            ActionLifecycleState::Verifying,
            ActionLifecycleState::Succeeded
        )),
        "T17 missing"
    );
}

// ---------------------------------------------------------------------------
// 2. Pipeline rejects invalid envelopes — short-circuit to FAILED.
// ---------------------------------------------------------------------------

#[test]
fn pipeline_short_circuits_to_failed_on_empty_action_kind() {
    let pipeline = ActionLifecyclePipeline::new();
    let now = Utc::now();
    let ctx = fresh_context(ActionId::new(), now);
    let envelope = make_envelope_with_empty_action();

    let final_ctx = pipeline
        .run(&envelope, ctx, now)
        .expect("pipeline must terminate cleanly even on invalid input");

    assert_eq!(final_ctx.status, ActionLifecycleState::Failed);
    assert_eq!(
        final_ctx.error,
        Some(ExecutionFailureReason::EnvelopeValidationFailed)
    );
}

#[test]
fn pipeline_short_circuits_to_failed_on_empty_subject_id() {
    let pipeline = ActionLifecyclePipeline::new();
    let now = Utc::now();
    let ctx = fresh_context(ActionId::new(), now);
    let envelope = make_envelope_with_empty_subject();

    let final_ctx = pipeline
        .run(&envelope, ctx, now)
        .expect("pipeline must terminate cleanly even on invalid input");

    assert_eq!(final_ctx.status, ActionLifecycleState::Failed);
    assert_eq!(
        final_ctx.error,
        Some(ExecutionFailureReason::EnvelopeValidationFailed)
    );
}

// ---------------------------------------------------------------------------
// 3. Pipeline drives a clean envelope to SUCCEEDED.
// ---------------------------------------------------------------------------

#[test]
fn pipeline_drives_clean_envelope_to_succeeded() {
    let pipeline = ActionLifecyclePipeline::new();
    let now = Utc::now();
    let ctx = fresh_context(ActionId::new(), now);
    let envelope = make_envelope();

    let final_ctx = pipeline.run(&envelope, ctx, now).expect("happy path");

    assert_eq!(final_ctx.status, ActionLifecycleState::Succeeded);
    assert_eq!(final_ctx.error, None);
    assert!(
        final_ctx.status.is_terminal(),
        "SUCCEEDED is a §4.2 strict terminal"
    );
}

// ---------------------------------------------------------------------------
// 4. InMemoryCapabilityRuntime.submit_action returns a populated context.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn in_memory_runtime_submit_action_returns_populated_context() {
    let runtime = InMemoryCapabilityRuntime::new();
    let envelope = make_envelope();
    let ctx = make_runtime_context();

    let result = runtime
        .submit_action(&envelope, &ctx)
        .await
        .expect("happy path");

    assert_eq!(result.status, ActionLifecycleState::Succeeded);
    assert!(result.action_id.as_str().starts_with("act_"));
    assert_eq!(runtime.len().await, 1, "context must be persisted");
}

// ---------------------------------------------------------------------------
// 5. get_action_status returns the same context as submit_action.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_action_status_returns_persisted_context() {
    let runtime = InMemoryCapabilityRuntime::new();
    let envelope = make_envelope();
    let ctx = make_runtime_context();

    let submitted = runtime
        .submit_action(&envelope, &ctx)
        .await
        .expect("happy path");

    let read_back = runtime
        .get_action_status(&submitted.action_id)
        .await
        .expect("status must be readable for a freshly submitted action");

    assert_eq!(submitted, read_back);
}

// ---------------------------------------------------------------------------
// 6. get_action_status on an unknown id returns ActionNotFound.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_action_status_returns_action_not_found_for_unknown_id() {
    let runtime = InMemoryCapabilityRuntime::new();
    let stranger = ActionId::new();

    let err = runtime
        .get_action_status(&stranger)
        .await
        .expect_err("an unknown id must not resolve");

    assert!(matches!(err, RuntimeError::ActionNotFound(_)));
}

// ---------------------------------------------------------------------------
// 7. apply_transition rejects a transition not in the §4.2 table.
// ---------------------------------------------------------------------------

#[test]
fn apply_transition_rejects_created_to_succeeded_jump() {
    let now = Utc::now();
    let mut ctx = fresh_context(ActionId::new(), now);
    // CREATED → SUCCEEDED is forbidden — must traverse POLICY_PENDING,
    // APPROVED, QUEUED, EXECUTING, VERIFYING per §4.2.
    let err = apply_transition(&mut ctx, ActionLifecycleState::Succeeded, now)
        .expect_err("CREATED → SUCCEEDED must be rejected");

    match err {
        RuntimeError::InvalidTransition { from, to } => {
            assert_eq!(from, ActionLifecycleState::Created);
            assert_eq!(to, ActionLifecycleState::Succeeded);
        }
        other => panic!("expected InvalidTransition, got {other:?}"),
    }
    // The context status must not have advanced.
    assert_eq!(ctx.status, ActionLifecycleState::Created);
}

// ---------------------------------------------------------------------------
// 8. apply_transition accepts a §4.2-listed transition.
// ---------------------------------------------------------------------------

#[test]
fn apply_transition_accepts_created_to_policy_pending() {
    let now = Utc::now();
    let mut ctx = fresh_context(ActionId::new(), now);
    apply_transition(&mut ctx, ActionLifecycleState::PolicyPending, now).expect("T2 is in §4.2");
    assert_eq!(ctx.status, ActionLifecycleState::PolicyPending);
    assert_eq!(ctx.last_updated_at, now);
}

// ---------------------------------------------------------------------------
// 9. SUCCEEDED is terminal — no outbound transition is accepted.
// ---------------------------------------------------------------------------

#[test]
fn succeeded_is_strict_terminal_and_rejects_every_outbound_transition() {
    use ActionLifecycleState::{
        ApprovalPending, Approved, Created, Executing, Failed, OverrideDenied, OverridePending,
        PolicyDenied, PolicyPending, Queued, RollbackFailed, RolledBack, Verifying,
    };

    let now = Utc::now();
    let mut ctx = fresh_context(ActionId::new(), now);
    // Drive the FSM through the canonical happy path one transition at a time.
    apply_transition(&mut ctx, ActionLifecycleState::PolicyPending, now).unwrap();
    apply_transition(&mut ctx, ActionLifecycleState::Approved, now).unwrap();
    apply_transition(&mut ctx, ActionLifecycleState::Queued, now).unwrap();
    apply_transition(&mut ctx, ActionLifecycleState::Executing, now).unwrap();
    apply_transition(&mut ctx, ActionLifecycleState::Verifying, now).unwrap();
    apply_transition(&mut ctx, ActionLifecycleState::Succeeded, now).unwrap();
    assert!(ctx.status.is_terminal());

    // Every other state must be rejected as a destination from SUCCEEDED.
    for dest in [
        Created,
        PolicyPending,
        ApprovalPending,
        OverridePending,
        Approved,
        PolicyDenied,
        OverrideDenied,
        Queued,
        Executing,
        Verifying,
        Failed,
        RolledBack,
        RollbackFailed,
    ] {
        let mut probe = ctx.clone();
        let err = apply_transition(&mut probe, dest, now)
            .expect_err("SUCCEEDED is a §4.2 strict terminal");
        assert!(matches!(err, RuntimeError::InvalidTransition { .. }));
        assert_eq!(probe.status, ActionLifecycleState::Succeeded);
    }
}

// ---------------------------------------------------------------------------
// 10. Concurrent submissions of distinct envelopes don't collide.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_submissions_of_distinct_envelopes_do_not_collide() {
    let runtime = InMemoryCapabilityRuntime::new();
    let ctx_a = make_runtime_context();
    let ctx_b = make_runtime_context();
    let envelope_a = make_envelope();
    let envelope_b = make_envelope();

    let runtime_a = runtime.clone();
    let runtime_b = runtime.clone();

    let (res_a, res_b) = tokio::join!(
        async move { runtime_a.submit_action(&envelope_a, &ctx_a).await },
        async move { runtime_b.submit_action(&envelope_b, &ctx_b).await },
    );

    let ctx_a = res_a.expect("submission A");
    let ctx_b = res_b.expect("submission B");

    assert_eq!(ctx_a.status, ActionLifecycleState::Succeeded);
    assert_eq!(ctx_b.status, ActionLifecycleState::Succeeded);
    assert_ne!(
        ctx_a.action_id, ctx_b.action_id,
        "two submissions must mint distinct action ids"
    );
    assert_eq!(runtime.len().await, 2);
}

// ---------------------------------------------------------------------------
// 11. All four strict terminals reject every outbound transition.
// ---------------------------------------------------------------------------

#[test]
fn all_strict_terminals_reject_every_outbound_transition() {
    for terminal in [
        ActionLifecycleState::Succeeded,
        ActionLifecycleState::RolledBack,
        ActionLifecycleState::RollbackFailed,
        ActionLifecycleState::OverrideDenied,
    ] {
        // No `(terminal, _)` row may appear in TRANSITIONS.
        let outbound: Vec<_> = TRANSITIONS
            .iter()
            .filter(|(from, _)| *from == terminal)
            .collect();
        assert!(
            outbound.is_empty(),
            "{terminal:?} is a §4.2 strict terminal but TRANSITIONS lists outbound rows: {outbound:?}"
        );
    }
}
