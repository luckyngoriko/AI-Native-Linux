//! T-029 integration tests for [`DispatchQueue`] — per-class FIFO,
//! per-subject token-bucket rate limits, 50 % `AGENT_PROPOSAL` hard cap,
//! and pipeline-level enrolment short-circuits, per S10.1 §3.5 / §11.
//!
//! These tests deliberately exercise the **observable** queue contract:
//! the structural invariants the §3.5 / §11 tables pin in the spec.
//! They do not lock the queue's internal representation; the constants
//! and capacities are pulled from the public surface so a future tuning
//! pass does not break the tests.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;

use aios_action::{ActionEnvelope, ActionId, Identity, Request, Trace};
use aios_capability_runtime::{
    fresh_context, ActionContext, ActionDispatchKind, ActionLifecycleState, CapabilityRuntime,
    DispatchQueue, ExecutionFailureReason, InMemoryCapabilityRuntime, QueueClass, RuntimeContext,
    RuntimeError, TokenBucket,
};

// ---------------------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------------------

fn make_envelope(subject: &str, is_ai: bool, action: &str) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new(subject, is_ai),
        Request::new(action, serde_json::json!({})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn make_runtime_context(subject: &str) -> RuntimeContext {
    RuntimeContext::new(subject, "polb_t029_bundle_v1", "code_t029")
}

fn make_ctx(subject_seed: usize, class: QueueClass) -> ActionContext {
    let action_id = ActionId::new();
    let mut ctx = fresh_context(action_id, Utc::now());
    ctx.queue_class = class;
    let _ = subject_seed; // suppress unused; the seed lives in the subject id at call site
    ctx
}

// ---------------------------------------------------------------------------
// 1. new_with_defaults seeds every closed QueueClass per §11.1.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn new_with_defaults_seeds_every_queue_class() {
    let q = DispatchQueue::new_with_defaults();
    let depths = q.depth_per_class().await;
    assert_eq!(depths.len(), 4);
    assert_eq!(depths.get(&QueueClass::Interactive), Some(&0));
    assert_eq!(depths.get(&QueueClass::AgentProposal), Some(&0));
    assert_eq!(depths.get(&QueueClass::Background), Some(&0));
    assert_eq!(depths.get(&QueueClass::RecoveryPriority), Some(&0));
}

#[tokio::test]
async fn new_with_defaults_capacities_match_section_11_1_shares() {
    let q = DispatchQueue::new_with_defaults();
    // §11.1 default share table — translated to absolute slots against
    // DEFAULT_TOTAL_CAPACITY (1 000). The exact mapping is documented in
    // `new_with_defaults`; the test pins the contract.
    assert_eq!(q.capacity_of(QueueClass::Interactive), 300);
    assert_eq!(q.capacity_of(QueueClass::AgentProposal), 400);
    assert_eq!(q.capacity_of(QueueClass::Background), 250);
    assert_eq!(q.capacity_of(QueueClass::RecoveryPriority), 50);
}

// ---------------------------------------------------------------------------
// 2. Per-class capacity refused at N+1.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn enroll_respects_per_class_capacity_recovery_priority() {
    // RecoveryPriority has the smallest default capacity (50). Drive it to
    // saturation under a single subject — increase the bucket capacity for
    // the test so the rate limit does not interfere.
    let mut caps = HashMap::new();
    // Use small absolute capacities so the test runs fast.
    caps.insert(QueueClass::Interactive, 1_000);
    caps.insert(QueueClass::AgentProposal, 1_000);
    caps.insert(QueueClass::Background, 1_000);
    caps.insert(QueueClass::RecoveryPriority, 3);
    let q = DispatchQueue::new_with_capacities(caps);

    // Use distinct subject ids so the per-subject rate-limit does not gate
    // the test.
    for i in 0..3 {
        let ctx = make_ctx(i, QueueClass::RecoveryPriority);
        let subject = format!("human:lucky:{i}");
        q.enroll(ctx, &subject).await.expect("under capacity");
    }
    let ctx = make_ctx(99, QueueClass::RecoveryPriority);
    let err = q
        .enroll(ctx, "human:lucky:fresh")
        .await
        .expect_err("over capacity should fail");
    assert!(matches!(
        err,
        RuntimeError::QueueFull(QueueClass::RecoveryPriority)
    ));
}

// ---------------------------------------------------------------------------
// 3. 50 % AGENT_PROPOSAL hard cap (§11.1).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn enroll_respects_50pct_agent_proposal_hard_cap() {
    // Tiny capacities so we reach the cap fast. Total capacity = 10; hard
    // cap on AGENT_PROPOSAL = 5 (50 %).
    let mut caps = HashMap::new();
    caps.insert(QueueClass::Interactive, 3);
    caps.insert(QueueClass::AgentProposal, 100); // operator-configured ceiling > cap
    caps.insert(QueueClass::Background, 3);
    caps.insert(QueueClass::RecoveryPriority, 4);
    let q = DispatchQueue::new_with_capacities(caps); // total = 110, cap = 55

    // Stuff in 55 AGENT_PROPOSAL entries with distinct subjects to bypass
    // rate limit.
    for i in 0..55 {
        let ctx = make_ctx(i, QueueClass::AgentProposal);
        let subject = format!("ai:agent:{i}");
        q.enroll(ctx, &subject).await.expect("under hard cap");
    }
    // 56th must fail the hard cap (regardless of operator-configured
    // AgentProposal capacity being 100).
    let ctx = make_ctx(999, QueueClass::AgentProposal);
    let err = q
        .enroll(ctx, "ai:agent:fresh")
        .await
        .expect_err("hard cap should fire");
    assert!(matches!(
        err,
        RuntimeError::QueueFull(QueueClass::AgentProposal)
    ));
}

// ---------------------------------------------------------------------------
// 4. Per-subject rate limit drains the burst.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn enroll_respects_per_subject_rate_limit() {
    let q = DispatchQueue::new_with_defaults();
    let subject = "ai:agent:rate_limited";
    // Default burst = 15 (AI agent §11.2). The first 15 enrolments succeed;
    // the 16th in the same instant fails with RateLimited.
    for i in 0..15_usize {
        let ctx = make_ctx(i, QueueClass::Interactive);
        q.enroll(ctx, subject).await.expect("within burst");
    }
    let ctx = make_ctx(99, QueueClass::Interactive);
    let err = q
        .enroll(ctx, subject)
        .await
        .expect_err("burst exhausted should fail");
    match err {
        RuntimeError::RateLimited(s) => assert_eq!(s, subject),
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 5. depth_per_class reports correct counts after enrolment.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn depth_per_class_reports_correct_counts() {
    let q = DispatchQueue::new_with_defaults();
    // Three INTERACTIVE under three subjects (avoid rate limit).
    for i in 0..3 {
        let ctx = make_ctx(i, QueueClass::Interactive);
        q.enroll(ctx, &format!("human:{i}")).await.expect("ok");
    }
    // One AGENT_PROPOSAL.
    let ctx = make_ctx(99, QueueClass::AgentProposal);
    q.enroll(ctx, "ai:agent:alpha").await.expect("ok");

    let depths = q.depth_per_class().await;
    assert_eq!(depths.get(&QueueClass::Interactive), Some(&3));
    assert_eq!(depths.get(&QueueClass::AgentProposal), Some(&1));
    assert_eq!(depths.get(&QueueClass::Background), Some(&0));
    assert_eq!(depths.get(&QueueClass::RecoveryPriority), Some(&0));
    assert_eq!(q.total_len().await, 4);
}

// ---------------------------------------------------------------------------
// 6. dequeue returns FIFO.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dequeue_returns_fifo() {
    let q = DispatchQueue::new_with_defaults();
    let ctx_a = make_ctx(1, QueueClass::Background);
    let ctx_b = make_ctx(2, QueueClass::Background);
    let id_a = ctx_a.action_id.clone();
    let id_b = ctx_b.action_id.clone();

    q.enroll(ctx_a, "service:alpha").await.expect("ok");
    q.enroll(ctx_b, "service:beta").await.expect("ok");

    let first = q.dequeue(QueueClass::Background).await.expect("present");
    let second = q.dequeue(QueueClass::Background).await.expect("present");
    assert_eq!(first.action_id, id_a);
    assert_eq!(second.action_id, id_b);
}

// ---------------------------------------------------------------------------
// 7. dequeue on empty returns None.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dequeue_on_empty_returns_none() {
    let q = DispatchQueue::new_with_defaults();
    assert!(q.dequeue(QueueClass::Interactive).await.is_none());
    assert!(q.dequeue(QueueClass::AgentProposal).await.is_none());
    assert!(q.dequeue(QueueClass::Background).await.is_none());
    assert!(q.dequeue(QueueClass::RecoveryPriority).await.is_none());
}

// ---------------------------------------------------------------------------
// 8. Runtime + queue: AI envelope enrols into AGENT_PROPOSAL.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn runtime_with_queue_enrols_ai_envelope_into_agent_proposal() {
    let queue = Arc::new(DispatchQueue::new_with_defaults());
    let runtime = InMemoryCapabilityRuntime::new().with_dispatch_queue(Arc::clone(&queue));
    let env = make_envelope("ai:agent:alpha", true, "service.restart");
    let ctx = runtime
        .submit_action(&env, &make_runtime_context("ai:agent:alpha"))
        .await
        .expect("happy path");
    // The action drives all the way to SUCCEEDED through the stubbed
    // verify path; what matters here is that the queue selected
    // AGENT_PROPOSAL when the envelope was AI-origin.
    assert_eq!(ctx.queue_class, QueueClass::AgentProposal);
    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    // The queue retains the enrolment (T-029 does not drain on dispatch;
    // T-035 wires the dispatcher's pop side).
    let depths = queue.depth_per_class().await;
    assert_eq!(depths.get(&QueueClass::AgentProposal), Some(&1));
}

// ---------------------------------------------------------------------------
// 9. Pipeline integration — QueueFull → FAILED + ResourceBudgetExceeded.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pipeline_queue_full_short_circuits_to_failed() {
    // Build a queue with capacity 1 on INTERACTIVE. Pre-load it.
    let mut caps = HashMap::new();
    caps.insert(QueueClass::Interactive, 1);
    caps.insert(QueueClass::AgentProposal, 100);
    caps.insert(QueueClass::Background, 100);
    caps.insert(QueueClass::RecoveryPriority, 100);
    let queue = Arc::new(DispatchQueue::new_with_capacities(caps));
    // Pre-load the queue under a distinct subject.
    let ctx0 = make_ctx(0, QueueClass::Interactive);
    queue
        .enroll(ctx0, "human:lucky:preload")
        .await
        .expect("preload");

    let runtime = InMemoryCapabilityRuntime::new().with_dispatch_queue(Arc::clone(&queue));
    let env = make_envelope("human:lucky", false, "service.restart");
    let ctx = runtime
        .submit_action(&env, &make_runtime_context("human:lucky"))
        .await
        .expect("envelope is structurally valid");
    assert_eq!(ctx.status, ActionLifecycleState::Failed);
    assert_eq!(
        ctx.error,
        Some(ExecutionFailureReason::ResourceBudgetExceeded)
    );
}

// ---------------------------------------------------------------------------
// 10. Pipeline integration — RateLimited → FAILED + ResourceBudgetExceeded.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pipeline_rate_limited_short_circuits_to_failed() {
    let queue = Arc::new(DispatchQueue::new_with_defaults());
    // Pre-drain the bucket for `human:lucky` by enrolling 15 dummies under
    // that subject directly (all into INTERACTIVE, sized 300).
    for i in 0..15_usize {
        let ctx = make_ctx(i, QueueClass::Interactive);
        queue
            .enroll(ctx, "human:lucky")
            .await
            .expect("preload burst");
    }
    let runtime = InMemoryCapabilityRuntime::new().with_dispatch_queue(Arc::clone(&queue));
    let env = make_envelope("human:lucky", false, "service.restart");
    let ctx = runtime
        .submit_action(&env, &make_runtime_context("human:lucky"))
        .await
        .expect("envelope structurally valid");
    assert_eq!(ctx.status, ActionLifecycleState::Failed);
    assert_eq!(
        ctx.error,
        Some(ExecutionFailureReason::ResourceBudgetExceeded)
    );
}

// ---------------------------------------------------------------------------
// 11. Runtime baseline: queue=None keeps T-027 behaviour.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn runtime_without_queue_preserves_t027_behaviour() {
    let runtime = InMemoryCapabilityRuntime::new();
    let env = make_envelope("human:lucky", false, "service.restart");
    let ctx = runtime
        .submit_action(&env, &make_runtime_context("human:lucky"))
        .await
        .expect("happy path");
    // With no queue, queue_class stays at the T-027 fresh_context seed.
    assert_eq!(ctx.queue_class, QueueClass::Interactive);
    assert_eq!(ctx.dispatch_kind, ActionDispatchKind::SubprocessFork);
    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
}

// ---------------------------------------------------------------------------
// 12. TokenBucket determinism — explicit `consume_at` for repeatable tests.
// ---------------------------------------------------------------------------

#[test]
fn token_bucket_consume_at_is_deterministic() {
    let mut b = TokenBucket::default_ai_agent();
    let t = std::time::Instant::now();
    // Drain the burst.
    let burst = 15_usize;
    for _ in 0..burst {
        assert!(b.consume_at(1.0, t));
    }
    // Same instant: no refill, must reject.
    assert!(!b.consume_at(1.0, t));
}
