//! `ActionLifecyclePipeline` — eight-step orchestration driver for S10.1 §3 / §4.
//!
//! `submit_action(envelope, context) -> ActionContext` is the single hot-path
//! entry point of the Capability Runtime; this module implements that pipeline
//! as eight discrete steps that each either short-circuit the [`ActionContext`]
//! into a terminal state or pass it forward to the next step.
//!
//! Per S10.1 §3 the eight steps mirror the public RPC surface:
//!
//! 1. `step_validate` — `ValidateAction` (S10.1 §5.2 / §6.1 step 0).
//! 2. `step_policy_evaluate` — `EvaluatePolicyForAction` (S10.1 §5.2 / §6.1
//!    step 2). **T-030 stub.**
//! 3. `step_request_approval` — `RequestApprovalForAction` (S10.1 §5.2 / §6.1
//!    step 3). **T-034 stub.**
//! 4. `step_queue` — queue enrolment, §3.5 / §11. **T-029 stub.**
//! 5. `step_execute` — eight-step pre-dispatch + adapter dispatch, §6.1 /
//!    §6.2. **T-028/T-029 stub.**
//! 6. `step_verify` — `VerifyAction`, §7.1. **T-035 wires the real
//!    verification engine; T-027 ships a stub.**
//! 7. `step_rollback` — `RollbackAction`, §7.2 / §7.3 / §7.4. **T-032 stub.**
//! 8. `step_emit_evidence` — append evidence receipts per §6.4 / §13.
//!    **T-031 stub.**
//!
//! ## What is real vs stubbed in T-027
//!
//! T-027 lands the **driver skeleton, the §4.2 transition table, and the
//! `apply_transition` enforcement gate**. Step 1 (envelope schema validation)
//! is real and exercised by the tests. Steps 2..8 are deliberately stubbed and
//! pass-through their input; each stub names the task that lands the real
//! implementation so the placeholder cannot drift silently.
//!
//! ## Why a step enum
//!
//! [`PipelineState`] makes the short-circuit semantics explicit at the type
//! level: the pipeline driver loop terminates the moment a step returns
//! `ShortCircuit`, which is exactly the contract S10.1 §3 demands (no silent
//! fall-through, every step is authoritative when it fires). The test suite
//! verifies this by counting executed steps after an injected short-circuit
//! into `FAILED`.

use chrono::{DateTime, Utc};

use aios_action::ActionEnvelope;

use crate::context::ActionContext;
use crate::dispatch::{ActionDispatchKind, QueueClass};
use crate::error::RuntimeError;
use crate::failure::ExecutionFailureReason;
use crate::status::ActionLifecycleState;

// ---------------------------------------------------------------------------
// §4.2 transition table (exhaustive).
// ---------------------------------------------------------------------------

/// `TRANSITIONS` — the exhaustive list of allowed `(from, to)` FSM transitions
/// per S10.1 §4.2.
///
/// T1 (`(init) → CREATED`) has no `from` state and is the constructor for
/// [`ActionContext`]; it is therefore **not** present in this list. All
/// twenty remaining transitions (T2..T21) are listed verbatim in spec order.
/// Adding a row is a versioned spec change.
///
/// [`apply_transition`] is the sole writer through this table; any
/// `(from, to)` pair not present here returns
/// [`RuntimeError::InvalidTransition`] and the runtime emits a
/// `LIFECYCLE_ILLEGAL_TRANSITION` evidence record (T-031 wiring).
pub const TRANSITIONS: &[(ActionLifecycleState, ActionLifecycleState)] = &[
    // T2 — CREATED → POLICY_PENDING: pre-validation succeeded.
    (
        ActionLifecycleState::Created,
        ActionLifecycleState::PolicyPending,
    ),
    // T3 — CREATED → FAILED: pre-validation failed.
    (ActionLifecycleState::Created, ActionLifecycleState::Failed),
    // T4 — POLICY_PENDING → APPROVED: policy = ALLOW.
    (
        ActionLifecycleState::PolicyPending,
        ActionLifecycleState::Approved,
    ),
    // T5 — POLICY_PENDING → APPROVAL_PENDING: policy = REQUIRE_APPROVAL.
    (
        ActionLifecycleState::PolicyPending,
        ActionLifecycleState::ApprovalPending,
    ),
    // T6 — POLICY_PENDING → POLICY_DENIED: policy = DENY (no override path).
    (
        ActionLifecycleState::PolicyPending,
        ActionLifecycleState::PolicyDenied,
    ),
    // T7 — POLICY_PENDING → OVERRIDE_PENDING: policy = DENY (scoped) + operator override authored.
    (
        ActionLifecycleState::PolicyPending,
        ActionLifecycleState::OverridePending,
    ),
    // T8 — APPROVAL_PENDING → APPROVED: ApprovalBinding GRANTED.
    (
        ActionLifecycleState::ApprovalPending,
        ActionLifecycleState::Approved,
    ),
    // T9 — APPROVAL_PENDING → FAILED: approval terminated non-GRANTED.
    (
        ActionLifecycleState::ApprovalPending,
        ActionLifecycleState::Failed,
    ),
    // T10 — OVERRIDE_PENDING → APPROVED: OverrideBinding OS_ACTIVE.
    (
        ActionLifecycleState::OverridePending,
        ActionLifecycleState::Approved,
    ),
    // T11 — OVERRIDE_PENDING → OVERRIDE_DENIED: override terminated non-OS_ACTIVE (terminal).
    (
        ActionLifecycleState::OverridePending,
        ActionLifecycleState::OverrideDenied,
    ),
    // T12 — APPROVED → QUEUED: queue enrolment.
    (ActionLifecycleState::Approved, ActionLifecycleState::Queued),
    // T13 — QUEUED → EXECUTING: 8-step pre-dispatch succeeded.
    (
        ActionLifecycleState::Queued,
        ActionLifecycleState::Executing,
    ),
    // T14 — QUEUED → FAILED: 8-step pre-dispatch failed.
    (ActionLifecycleState::Queued, ActionLifecycleState::Failed),
    // T15 — EXECUTING → VERIFYING: adapter ADAPTER_OK.
    (
        ActionLifecycleState::Executing,
        ActionLifecycleState::Verifying,
    ),
    // T16 — EXECUTING → FAILED: adapter failed/panicked/timeout.
    (
        ActionLifecycleState::Executing,
        ActionLifecycleState::Failed,
    ),
    // T17 — VERIFYING → SUCCEEDED: VERIFICATION_PASSED all intents.
    (
        ActionLifecycleState::Verifying,
        ActionLifecycleState::Succeeded,
    ),
    // T18 — VERIFYING → FAILED: verification failed + rollback_strategy = NONE.
    (
        ActionLifecycleState::Verifying,
        ActionLifecycleState::Failed,
    ),
    // T19 — FAILED → ROLLED_BACK: rollback strategy != NONE; rollback SUCCEEDED.
    (
        ActionLifecycleState::Failed,
        ActionLifecycleState::RolledBack,
    ),
    // T20 — FAILED → ROLLBACK_FAILED: rollback strategy != NONE; rollback FAILED.
    (
        ActionLifecycleState::Failed,
        ActionLifecycleState::RollbackFailed,
    ),
    // T21 — POLICY_DENIED → OVERRIDE_PENDING: operator-authored override.
    (
        ActionLifecycleState::PolicyDenied,
        ActionLifecycleState::OverridePending,
    ),
];

// ---------------------------------------------------------------------------
// PipelineState — step result.
// ---------------------------------------------------------------------------

/// Outcome of a single pipeline step.
///
/// `Continue(ctx)` carries the mutated context forward to the next step.
/// `ShortCircuit(ctx)` halts the pipeline immediately and returns the context
/// to the caller; the context is **already** in its terminal lifecycle state
/// (the step that short-circuits owns the §4.2 transition).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineState {
    /// Step did not produce a terminal outcome; the pipeline continues.
    Continue(ActionContext),
    /// Step produced a terminal outcome (success or failure); the pipeline
    /// halts here and the caller receives the context as-is.
    ShortCircuit(ActionContext),
}

impl PipelineState {
    /// Borrow the context regardless of variant.
    #[must_use]
    pub const fn context(&self) -> &ActionContext {
        match self {
            Self::Continue(ctx) | Self::ShortCircuit(ctx) => ctx,
        }
    }

    /// Consume `self` and return the context regardless of variant.
    #[must_use]
    pub fn into_context(self) -> ActionContext {
        match self {
            Self::Continue(ctx) | Self::ShortCircuit(ctx) => ctx,
        }
    }
}

// ---------------------------------------------------------------------------
// apply_transition — the §4.2 enforcement gate.
// ---------------------------------------------------------------------------

/// Drive `ctx` through one §4.2 transition.
///
/// Verifies the `(ctx.status, to)` pair is present in [`TRANSITIONS`]; on
/// match, mutates `ctx.status` to `to` and stamps `ctx.last_updated_at = now`.
/// On miss, returns [`RuntimeError::InvalidTransition`] with both endpoints
/// preserved for forensic logging (the gRPC adapter — T-033 — maps this to
/// `RuntimeErrorCode::LifecycleIllegalTransition`).
///
/// This is the **single writer** through which every lifecycle state change
/// must flow. The pipeline steps below call this helper; direct mutation of
/// `ctx.status` is a discipline violation.
///
/// # Errors
///
/// Returns [`RuntimeError::InvalidTransition`] when `(ctx.status, to)` is not
/// in [`TRANSITIONS`] — including any attempt to transition out of a strict
/// terminal state ([`ActionLifecycleState::is_terminal`]).
pub fn apply_transition(
    ctx: &mut ActionContext,
    to: ActionLifecycleState,
    now: DateTime<Utc>,
) -> Result<(), RuntimeError> {
    let from = ctx.status;
    if TRANSITIONS.iter().any(|&(f, t)| f == from && t == to) {
        ctx.status = to;
        ctx.last_updated_at = now;
        Ok(())
    } else {
        Err(RuntimeError::InvalidTransition { from, to })
    }
}

// ---------------------------------------------------------------------------
// ActionLifecyclePipeline — the eight-step driver.
// ---------------------------------------------------------------------------

/// The eight-step orchestration driver for S10.1 §3.
///
/// Stateless: every step takes `&mut ActionContext` (or [`PipelineState`])
/// plus the immutable `(envelope, now)` inputs. Composition into a real
/// runtime is the [`crate::runtime::InMemoryCapabilityRuntime`]'s job.
#[derive(Debug, Default, Clone, Copy)]
pub struct ActionLifecyclePipeline;

impl ActionLifecyclePipeline {
    /// Construct a fresh stateless driver. Cheap; equivalent to `Default`.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Drive an envelope through all eight steps. Returns the final
    /// [`ActionContext`] regardless of whether the pipeline short-circuited.
    ///
    /// # Errors
    ///
    /// Propagates [`RuntimeError::InvalidTransition`] when any step attempts a
    /// transition not listed in [`TRANSITIONS`]. Step authors are expected to
    /// only request §4.2 transitions; an `InvalidTransition` here is a code
    /// defect, not a runtime input failure.
    pub fn run(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        now: DateTime<Utc>,
    ) -> Result<ActionContext, RuntimeError> {
        let state = self.step_validate(envelope, ctx, now)?;
        let state = match state {
            PipelineState::Continue(c) => self.step_policy_evaluate(envelope, c, now)?,
            PipelineState::ShortCircuit(c) => return Ok(c),
        };
        let state = match state {
            PipelineState::Continue(c) => self.step_request_approval(envelope, c, now)?,
            PipelineState::ShortCircuit(c) => return Ok(c),
        };
        let state = match state {
            PipelineState::Continue(c) => self.step_queue(envelope, c, now)?,
            PipelineState::ShortCircuit(c) => return Ok(c),
        };
        let state = match state {
            PipelineState::Continue(c) => self.step_execute(envelope, c, now)?,
            PipelineState::ShortCircuit(c) => return Ok(c),
        };
        let state = match state {
            PipelineState::Continue(c) => self.step_verify(envelope, c, now)?,
            PipelineState::ShortCircuit(c) => return Ok(c),
        };
        let state = match state {
            PipelineState::Continue(c) => self.step_rollback(envelope, c, now)?,
            PipelineState::ShortCircuit(c) => return Ok(c),
        };
        let state = match state {
            PipelineState::Continue(c) | PipelineState::ShortCircuit(c) => {
                self.step_emit_evidence(envelope, c, now)?
            }
        };
        Ok(state.into_context())
    }

    // -----------------------------------------------------------------------
    // Step 1 — ValidateAction (REAL in T-027).
    // -----------------------------------------------------------------------

    /// Step 1 — schema-validate the envelope (S10.1 §5.2 / §6.1 step 0).
    ///
    /// Today's implementation enforces the minimal envelope shape:
    /// - `request.action` is non-empty (a registered `action_kind` would
    ///   otherwise be unresolvable).
    /// - `identity.subject_canonical_id` is non-empty (subject hydration in
    ///   step 2 — T-030 — cannot proceed against an empty id).
    ///
    /// On miss, transitions `CREATED → FAILED` (T3) with
    /// `error = EnvelopeValidationFailed` and short-circuits. On hit,
    /// transitions `CREATED → POLICY_PENDING` (T2) and continues.
    ///
    /// T-033's gRPC `ValidateAction` RPC will call this step in isolation;
    /// today the driver always calls it as part of `submit_action`.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::InvalidTransition`] only if the context is not
    /// in [`ActionLifecycleState::Created`] (a precondition violation).
    pub fn step_validate(
        &self,
        envelope: &ActionEnvelope,
        mut ctx: ActionContext,
        now: DateTime<Utc>,
    ) -> Result<PipelineState, RuntimeError> {
        if envelope.request.action.trim().is_empty()
            || envelope.identity.subject_canonical_id.trim().is_empty()
        {
            apply_transition(&mut ctx, ActionLifecycleState::Failed, now)?;
            ctx.error = Some(ExecutionFailureReason::EnvelopeValidationFailed);
            return Ok(PipelineState::ShortCircuit(ctx));
        }
        apply_transition(&mut ctx, ActionLifecycleState::PolicyPending, now)?;
        Ok(PipelineState::Continue(ctx))
    }

    // -----------------------------------------------------------------------
    // Step 2 — EvaluatePolicyForAction (STUB; T-030 wires aios-policy).
    // -----------------------------------------------------------------------

    /// Step 2 — call the Policy Kernel against the active bundle.
    ///
    /// **T-030 wires `aios_policy::PolicyKernel`.** Today the stub transitions
    /// `POLICY_PENDING → APPROVED` (T4 — "policy = ALLOW") unconditionally so
    /// the rest of the pipeline can be driven end-to-end. Step authors that
    /// land T-030 must:
    /// - call the kernel,
    /// - map `Decision::Allow` → T4,
    /// - map `Decision::RequireApproval` → T5 (continue to step 3),
    /// - map `Decision::Deny` (no override) → T6 + `ShortCircuit`,
    /// - map `Decision::Deny` (override authored) → T7 + `ShortCircuit`.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::InvalidTransition`] if the context is not in
    /// [`ActionLifecycleState::PolicyPending`].
    pub fn step_policy_evaluate(
        &self,
        _envelope: &ActionEnvelope,
        mut ctx: ActionContext,
        now: DateTime<Utc>,
    ) -> Result<PipelineState, RuntimeError> {
        // T-030 stub: assume ALLOW.
        apply_transition(&mut ctx, ActionLifecycleState::Approved, now)?;
        Ok(PipelineState::Continue(ctx))
    }

    // -----------------------------------------------------------------------
    // Step 3 — RequestApprovalForAction (STUB; T-034 owns approval orchestration).
    // -----------------------------------------------------------------------

    /// Step 3 — issue an `ApprovalRequest` and wait for a binding.
    ///
    /// **T-034 owns approval orchestration (S5.3).** Today's stub is a no-op
    /// because step 2's T-030 stub never produces `APPROVAL_PENDING`. Step
    /// authors that land T-034 must drive `APPROVAL_PENDING → APPROVED` (T8)
    /// or `APPROVAL_PENDING → FAILED` (T9) here.
    ///
    /// # Errors
    ///
    /// Currently infallible; the signature carries `Result` for forward
    /// compatibility with T-034's transition driver.
    #[allow(
        clippy::unused_self,
        clippy::unnecessary_wraps,
        clippy::missing_const_for_fn,
        reason = "stub for T-034; signature stable across the trait surface and must accept `mut ctx` once approval orchestration lands"
    )]
    pub fn step_request_approval(
        &self,
        _envelope: &ActionEnvelope,
        ctx: ActionContext,
        _now: DateTime<Utc>,
    ) -> Result<PipelineState, RuntimeError> {
        Ok(PipelineState::Continue(ctx))
    }

    // -----------------------------------------------------------------------
    // Step 4 — Queue enrolment (STUB; T-029 owns DispatchQueue).
    // -----------------------------------------------------------------------

    /// Step 4 — enrol the action in its [`QueueClass`] bucket (§3.5 / §11).
    ///
    /// **T-029 wires the real dispatch queue + backpressure.** Today the stub
    /// performs the structural transition `APPROVED → QUEUED` (T12) and
    /// continues. The actual queue (and the backpressure shed-load path that
    /// would short-circuit to `FAILED` here) lands in T-029.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::InvalidTransition`] if the context is not in
    /// [`ActionLifecycleState::Approved`].
    pub fn step_queue(
        &self,
        _envelope: &ActionEnvelope,
        mut ctx: ActionContext,
        now: DateTime<Utc>,
    ) -> Result<PipelineState, RuntimeError> {
        apply_transition(&mut ctx, ActionLifecycleState::Queued, now)?;
        Ok(PipelineState::Continue(ctx))
    }

    // -----------------------------------------------------------------------
    // Step 5 — ExecuteAction (STUB; T-028 + T-029 wire the eight pre-dispatch steps).
    // -----------------------------------------------------------------------

    /// Step 5 — run §6.1's eight pre-dispatch steps + dispatch the adapter.
    ///
    /// **T-028 wires the adapter registry; T-029 wires the dispatcher.**
    /// Today the stub performs `QUEUED → EXECUTING` (T13) and immediately
    /// continues; the real pre-dispatch sequence (hash re-validate, policy
    /// re-evaluate, binding re-check, sandbox compose, queue re-check,
    /// binding mark `CONSUMED`, adapter dispatch) is queued for those tasks.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::InvalidTransition`] if the context is not in
    /// [`ActionLifecycleState::Queued`].
    pub fn step_execute(
        &self,
        _envelope: &ActionEnvelope,
        mut ctx: ActionContext,
        now: DateTime<Utc>,
    ) -> Result<PipelineState, RuntimeError> {
        apply_transition(&mut ctx, ActionLifecycleState::Executing, now)?;
        Ok(PipelineState::Continue(ctx))
    }

    // -----------------------------------------------------------------------
    // Step 6 — VerifyAction (STUB; T-035 wires the verification engine).
    // -----------------------------------------------------------------------

    /// Step 6 — call the verification engine against the envelope's
    /// `verification_intent` (S2.4 / S10.1 §7.1).
    ///
    /// **T-035 wires the real verification engine.** Today the stub performs
    /// `EXECUTING → VERIFYING → SUCCEEDED` (T15 + T17) — two consecutive
    /// transitions through [`apply_transition`] so the §4.2 table is still
    /// the single writer. Real impl will fan out per intent and drive T18
    /// (`VERIFYING → FAILED`) on any failure.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::InvalidTransition`] if the context is not in
    /// [`ActionLifecycleState::Executing`].
    pub fn step_verify(
        &self,
        _envelope: &ActionEnvelope,
        mut ctx: ActionContext,
        now: DateTime<Utc>,
    ) -> Result<PipelineState, RuntimeError> {
        apply_transition(&mut ctx, ActionLifecycleState::Verifying, now)?;
        apply_transition(&mut ctx, ActionLifecycleState::Succeeded, now)?;
        // SUCCEEDED is a strict terminal. The remaining steps (rollback,
        // evidence) treat this branch as a short-circuit so they cannot
        // illegally attempt outbound transitions from a terminal state.
        Ok(PipelineState::ShortCircuit(ctx))
    }

    // -----------------------------------------------------------------------
    // Step 7 — RollbackAction (STUB; T-032 owns the rollback FSM).
    // -----------------------------------------------------------------------

    /// Step 7 — drive the rollback FSM if execution or verification failed
    /// and `rollback_strategy != NONE` (S10.1 §7.2 / §7.3 / §7.4).
    ///
    /// **T-032 wires the rollback FSM + the `ROLLBACK_FAILED` forensic
    /// semantics.** Today the stub is a no-op because the pipeline reaches
    /// step 7 only when no prior step short-circuited (verify already drives
    /// SUCCEEDED + short-circuits in the success path). Step authors that
    /// land T-032 must drive `FAILED → ROLLED_BACK` (T19) or
    /// `FAILED → ROLLBACK_FAILED` (T20) here.
    ///
    /// # Errors
    ///
    /// Currently infallible; signature carries `Result` for forward
    /// compatibility with T-032.
    #[allow(
        clippy::unused_self,
        clippy::unnecessary_wraps,
        clippy::missing_const_for_fn,
        reason = "stub for T-032; signature stable across the trait surface and must accept `mut ctx` once the rollback FSM driver lands"
    )]
    pub fn step_rollback(
        &self,
        _envelope: &ActionEnvelope,
        ctx: ActionContext,
        _now: DateTime<Utc>,
    ) -> Result<PipelineState, RuntimeError> {
        Ok(PipelineState::Continue(ctx))
    }

    // -----------------------------------------------------------------------
    // Step 8 — Emit evidence (STUB; T-031 wires aios-evidence).
    // -----------------------------------------------------------------------

    /// Step 8 — append the per-step evidence records to the L0 evidence log
    /// (S10.1 §6.4 / §13; S3.1).
    ///
    /// **T-031 wires `aios_evidence::EvidenceLog`.** Today the stub is a
    /// no-op; the evidence chain on the context remains empty.
    ///
    /// # Errors
    ///
    /// Currently infallible; signature carries `Result` for forward
    /// compatibility with T-031 (the evidence log can fail closed on
    /// `EvidenceLogUnavailable` per S10.1 §3.8).
    #[allow(
        clippy::unused_self,
        clippy::unnecessary_wraps,
        clippy::missing_const_for_fn,
        reason = "stub for T-031; signature stable across the trait surface and must accept `mut ctx` once evidence emission appends receipts"
    )]
    pub fn step_emit_evidence(
        &self,
        _envelope: &ActionEnvelope,
        ctx: ActionContext,
        _now: DateTime<Utc>,
    ) -> Result<PipelineState, RuntimeError> {
        Ok(PipelineState::Continue(ctx))
    }
}

/// Convenience constructor: an `ActionContext` seeded for a fresh envelope.
///
/// Sets the default dispatch kind to [`ActionDispatchKind::SubprocessFork`]
/// and the default queue class to [`QueueClass::Interactive`] (the
/// p95 < 200 ms human-initiated bucket); the real values are decided at
/// T-029 / [`ActionLifecyclePipeline::step_queue`] once the adapter manifest
/// is resolved against the envelope and the subject's `is_ai` flag has been
/// honoured (AI subjects downgrade to [`QueueClass::AgentProposal`] per §11.4).
#[must_use]
pub const fn fresh_context(action_id: aios_action::ActionId, now: DateTime<Utc>) -> ActionContext {
    ActionContext::new(
        action_id,
        ActionDispatchKind::SubprocessFork,
        QueueClass::Interactive,
        now,
    )
}
