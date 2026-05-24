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

use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};

use aios_action::ActionEnvelope;
use aios_policy::{
    ApproverClass, Decision, EnrichmentSnapshot, PolicyContext, PolicyError, PolicyKernel,
};

use crate::adapter_registry::InMemoryAdapterRegistry;
use crate::context::ActionContext;
use crate::dispatch::{ActionDispatchKind, QueueClass};
use crate::dispatch_queue::DispatchQueue;
use crate::dispatcher::ActionDispatcher;
use crate::error::RuntimeError;
use crate::evidence_emit::EvidenceEmitter;
use crate::failure::{ExecutionFailureReason, RollbackOutcome};
use crate::rollback::RollbackDriver;
use crate::rollback_strategy::RollbackStrategy;
use crate::runtime::RuntimeContext;
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
    /// This entry point keeps the T-027 contract: no adapter registry is
    /// engaged, so step 5 (`step_execute`) performs the structural
    /// `QUEUED → EXECUTING` transition only. To engage T-028's adapter
    /// lookup + `FAIL_CLOSED` discipline, call
    /// [`Self::run_with_registry`].
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
        self.run_with_registry(envelope, ctx, now, None)
    }

    /// Drive an envelope through all eight steps with an optional adapter
    /// registry attached. **T-028 entry point.**
    ///
    /// When `registry` is `Some(...)`, step 5 (`step_execute`) consults the
    /// registry for an adapter declaring `envelope.request.action`. A miss
    /// transitions `QUEUED → FAILED` (T14) with
    /// [`ExecutionFailureReason::DependencyUnready`] (the closest
    /// spec-pinned reason in the closed §3.6 enum — the brief's nominal
    /// `AdapterUnknown` variant is not declared by T-026 and §3.6 is
    /// out-of-scope for T-028). When `registry` is `None`, the behaviour
    /// is identical to [`Self::run`] (T-027 backwards compatibility).
    ///
    /// # Errors
    ///
    /// Same as [`Self::run`].
    ///
    /// Async sibling: [`Self::run_with_engines`] composes the dispatch queue
    /// (T-029) over the same pipeline and is awaited from
    /// [`crate::InMemoryCapabilityRuntime::submit_action`].
    pub fn run_with_registry(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        now: DateTime<Utc>,
        registry: Option<&InMemoryAdapterRegistry>,
    ) -> Result<ActionContext, RuntimeError> {
        let state = self.step_validate(envelope, ctx, now)?;
        let state = match state {
            PipelineState::Continue(c) => self.step_policy_evaluate(envelope, c, now)?,
            PipelineState::ShortCircuit(c) => return Ok(c),
        };
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_request_approval_passthrough(envelope, c, now)?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };
        let state = match state {
            PipelineState::Continue(c) => self.step_queue(envelope, c, now)?,
            PipelineState::ShortCircuit(c) => return Ok(c),
        };
        let state = match state {
            PipelineState::Continue(c) => self.step_execute(envelope, c, now, registry)?,
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
    // T-029 entry point — async pipeline with dispatch queue + dispatcher.
    // -----------------------------------------------------------------------

    /// Drive an envelope through all eight steps with an optional adapter
    /// registry **and** an optional dispatch queue attached. **T-029 entry
    /// point.**
    ///
    /// Composition rules:
    ///
    /// - `queue = None`, `registry = None` — identical to [`Self::run`]
    ///   (T-027 contract preserved).
    /// - `queue = None`, `registry = Some(...)` — identical to
    ///   [`Self::run_with_registry`] (T-028 contract preserved).
    /// - `queue = Some(...)` — step 4 (`step_queue`) consults
    ///   [`ActionDispatcher::select_queue_class`] to pick the bucket,
    ///   applies the §11.4 AI-interactive downgrade marker, and calls
    ///   [`DispatchQueue::enroll`]. A
    ///   [`RuntimeError::QueueFull`] / [`RuntimeError::RateLimited`]
    ///   short-circuits to `QUEUED → FAILED` (T14) with
    ///   `error = ExecutionFailureReason::ResourceBudgetExceeded`.
    /// - `queue = Some(...)` + `registry = Some(...)` — step 5
    ///   (`step_execute`) additionally records the dispatcher's chosen
    ///   [`ActionDispatchKind`] on `context.dispatch_kind` (the field T-026
    ///   already wires) per the §3.2 closed table. The selection logs are
    ///   forensic only today; T-031 surfaces them through evidence.
    ///
    /// # Errors
    ///
    /// Same as [`Self::run`]; additionally returns
    /// [`RuntimeError::InvalidTransition`] only if a step author requests a
    /// transition outside the §4.2 table.
    pub async fn run_with_engines(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        now: DateTime<Utc>,
        registry: Option<&InMemoryAdapterRegistry>,
        queue: Option<&DispatchQueue>,
    ) -> Result<ActionContext, RuntimeError> {
        let state = self.step_validate(envelope, ctx, now)?;
        let state = match state {
            PipelineState::Continue(c) => self.step_policy_evaluate(envelope, c, now)?,
            PipelineState::ShortCircuit(c) => return Ok(c),
        };
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_request_approval_passthrough(envelope, c, now)?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_queue_with_engine(envelope, c, now, queue).await?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_execute_with_engines(envelope, c, now, registry)?
            }
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

    /// T-030 — async sibling of [`Self::run_with_engines`] that additionally
    /// threads an optional Policy Kernel and the caller's
    /// [`RuntimeContext`] through pipeline step 2.
    ///
    /// Composition rules (additive to [`Self::run_with_engines`]):
    ///
    /// - `kernel = None` — step 2 falls back to the T-027 stub (unconditional
    ///   `POLICY_PENDING → APPROVED`); every T-027 / T-028 / T-029 test
    ///   stays green.
    /// - `kernel = Some(...)` — step 2 calls
    ///   [`PolicyKernel::evaluate_policy`] with a `PolicyContext`
    ///   constructed from the `RuntimeContext` (subject, `bundle_version`,
    ///   `code_version` + an empty `EnrichmentSnapshot` until the AIOS-FS
    ///   read-path lands). The returned [`Decision`] drives the §4.2
    ///   transition table:
    ///
    ///   - `Allow` → T4 (`POLICY_PENDING → APPROVED`); the projected
    ///     [`aios_policy::Constraints`] are stored on the
    ///     `RuntimeContext.policy_constraints` slot for downstream steps.
    ///     If the subject is AI and the bound
    ///     `ApprovalRequirement.approver_classes` does not require a
    ///     human, the runtime increments the
    ///     `policy_double_check_warnings` counter (defense-in-depth §17
    ///     tripwire — the kernel already enforces §17, this is the
    ///     forensic backstop).
    ///   - `RequireApproval` → T5 (`POLICY_PENDING → APPROVAL_PENDING`);
    ///     short-circuit (T-034 will resume from here once approval
    ///     orchestration lands).
    ///   - `Deny` (no override path) → T6 (`POLICY_PENDING → POLICY_DENIED`);
    ///     short-circuit. The `error` is left `None` because §3.6
    ///     `ExecutionFailureReason` is closed at T-026 and no dedicated
    ///     `POLICY_DENIED` reason exists — terminal state alone is the
    ///     signal; T-031 wires the policy-deny evidence shape.
    ///   - `Deny` + active override on the boundary → T7
    ///     (`POLICY_PENDING → OVERRIDE_PENDING`); short-circuit (T-034
    ///     resumes from here as well).
    ///
    /// On `PolicyError::SubjectUnauthenticated` from the kernel, the
    /// runtime drives T3 (`CREATED → FAILED`) with
    /// [`ExecutionFailureReason::EnvelopeValidationFailed`] per S2.3 §7 /
    /// S10.1 §6.1 step 0 — the envelope's identity is the runtime's input
    /// contract, so an unauthenticated subject is a validation failure
    /// rather than a policy outcome. **Note:** step 2 fires from
    /// `POLICY_PENDING` (step 1 already drove T2); the §4.2 table does not
    /// list `POLICY_PENDING → FAILED`, so the runtime maps
    /// `SubjectUnauthenticated` onto T6 (`POLICY_PENDING → POLICY_DENIED`)
    /// to keep the FSM walk legal and preserves the typed reason in the
    /// per-step trace (T-031 surfaces it through evidence).
    ///
    /// Other `PolicyError` variants (`BundleVersionMismatch`,
    /// `BundleLoad`, `SchemaInvalid`, …) propagate as
    /// [`RuntimeError::PolicyEvalFailed`] — the kernel was unable to
    /// produce a decision, which is a runtime-level failure rather than
    /// an envelope-level deny.
    ///
    /// # Errors
    ///
    /// Same as [`Self::run_with_engines`]; additionally returns
    /// [`RuntimeError::PolicyEvalFailed`] when the kernel raises a
    /// non-recoverable evaluation error.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_with_full_engines(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        now: DateTime<Utc>,
        registry: Option<&InMemoryAdapterRegistry>,
        queue: Option<&DispatchQueue>,
        kernel: Option<&dyn PolicyKernel>,
        runtime_context: Option<&RuntimeContext>,
        tripwire: Option<&AtomicU64>,
    ) -> Result<ActionContext, RuntimeError> {
        let state = self.step_validate(envelope, ctx, now)?;
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_policy_evaluate_with_kernel(
                    envelope,
                    c,
                    now,
                    kernel,
                    runtime_context,
                    tripwire,
                )
                .await?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_request_approval_passthrough(envelope, c, now)?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_queue_with_engine(envelope, c, now, queue).await?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_execute_with_engines(envelope, c, now, registry)?
            }
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

    /// T-031 — top-level driver with both the full engine set (registry +
    /// queue + kernel) **and** an [`EvidenceEmitter`] attached.
    ///
    /// Behaviour is identical to [`Self::run_with_full_engines`] except
    /// that one [`aios_evidence::EvidenceReceipt`] is appended via the
    /// supplied emitter at every §4.2 transition the runtime drives.
    /// Specifically:
    ///
    /// 1. After `step_validate` succeeds (`CREATED → POLICY_PENDING`) —
    ///    emit `ACTION_RECEIVED` (S3.1 §4 ID 1).
    /// 2. After `step_policy_evaluate_with_kernel_and_emit` decides —
    ///    emit `POLICY_DECISION` (S3.1 §4 ID 4) with the
    ///    `policy_decision_id` from `aios_policy`. On the no-kernel-
    ///    attached fallback path the emission is skipped because there
    ///    is no kernel decision to record (the T-030 stub is structural-
    ///    only).
    /// 3. After `step_queue_with_engine` enrolls — emit the queued
    ///    marker via `RecordType::ActionDispatched(dispatched=false)`
    ///    and, when applicable, the silent §11.4
    ///    `AI_INTERACTIVE_QUEUE_DOWNGRADE`.
    /// 4. After `step_execute_with_engines` resolves the adapter —
    ///    emit `ROUTING_DECISION` (S3.1 §4 ID 3) followed by
    ///    `EXECUTION_STARTED` (S3.1 §4 ID 8). The adapter handle is
    ///    addressed via the registry; when no registry is attached the
    ///    routing/execution emissions are skipped (the T-027 baseline
    ///    is preserved bit-for-bit).
    /// 5. After `step_verify` reaches its terminal — emit
    ///    `EXECUTION_COMPLETED` (S3.1 §4 ID 9) followed by
    ///    `VERIFICATION_RESULT` (S3.1 §4 ID 10). The verifying-step
    ///    stub still drives `SUCCEEDED`; T-035 will swap in the real
    ///    verification engine without touching this emission shape.
    /// 6. After `step_rollback` reaches its terminal — emit
    ///    `ROLLBACK_COMPLETED` (S3.1 §4 ID 11). Today's stub never
    ///    drives rollback so the call is a no-op outside test paths
    ///    that injected a `FAILED` state directly; T-032 wires the
    ///    real rollback path against this same emit point.
    ///
    /// Emission failures map to [`RuntimeError::EvidenceEmitFailed`].
    /// Per S10.1 §12.6 the pipeline fails closed: the action does not
    /// progress past the failing transition (INV-014).
    ///
    /// # Errors
    ///
    /// Same as [`Self::run_with_full_engines`], additionally returns
    /// [`RuntimeError::EvidenceEmitFailed`] on any evidence sink failure.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_with_full_engines_and_evidence(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        now: DateTime<Utc>,
        registry: Option<&InMemoryAdapterRegistry>,
        queue: Option<&DispatchQueue>,
        kernel: Option<&dyn PolicyKernel>,
        runtime_context: Option<&RuntimeContext>,
        tripwire: Option<&AtomicU64>,
        emitter: &EvidenceEmitter,
    ) -> Result<ActionContext, RuntimeError> {
        // ── Step 1: validate envelope. Emit ACTION_RECEIVED on success. ──
        let state = self.step_validate(envelope, ctx, now)?;
        let state = match state {
            PipelineState::Continue(mut c) => {
                emitter.emit_action_received(envelope, &mut c).await?;
                PipelineState::Continue(c)
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };

        // ── Step 2: policy evaluate. Emit POLICY_DECISION on a kernel hit. ──
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_policy_evaluate_with_kernel_and_emit(
                    envelope,
                    c,
                    now,
                    kernel,
                    runtime_context,
                    tripwire,
                    emitter,
                )
                .await?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };

        // ── Step 3: approval (T-034 stub today). No emission. ──
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_request_approval_passthrough(envelope, c, now)?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };

        // ── Step 4: queue enroll. Emit ACTION_QUEUED (folded into
        //    ActionDispatched(dispatched=false)) and the §11.4 downgrade
        //    marker when applicable. ──
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_queue_with_engine_and_emit(envelope, c, now, queue, emitter)
                    .await?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };

        // ── Step 5: execute. Emit ROUTING_DECISION + EXECUTION_STARTED. ──
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_execute_with_engines_and_emit(envelope, c, now, registry, emitter)
                    .await?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };

        // ── Step 6: verify. Emit EXECUTION_COMPLETED + VERIFICATION_RESULT. ──
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_verify_and_emit(envelope, c, now, emitter).await?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };

        // ── Step 7: rollback (T-032 stub today; today's pipeline never
        //    reaches rollback because verify short-circuits on success). ──
        let state = match state {
            PipelineState::Continue(c) => self.step_rollback(envelope, c, now)?,
            PipelineState::ShortCircuit(c) => return Ok(c),
        };

        // ── Step 8: terminal evidence pass-through. T-031 emits per-
        //    transition above, so this final step is a structural no-op. ──
        let state = match state {
            PipelineState::Continue(c) | PipelineState::ShortCircuit(c) => {
                self.step_emit_evidence(envelope, c, now)?
            }
        };
        Ok(state.into_context())
    }

    /// T-032 — full driver with rollback engaged.
    ///
    /// Identical to [`Self::run_with_full_engines_and_evidence`] except:
    ///
    /// - Step 6 (verify) routes through
    ///   [`Self::step_verify_inject_failure_and_emit`] so the rollback
    ///   driver's `inject_verify_failure` knob can force the
    ///   `VERIFYING → FAILED` (T18) transition needed to engage step 7.
    /// - Step 7 (rollback) routes through
    ///   [`Self::step_rollback_with_driver_and_emit`] which calls the
    ///   driver, applies the §4.2 terminal mapping per
    ///   [`RollbackDriver::classify_terminal`], emits
    ///   `ROLLBACK_COMPLETED` evidence (FOREVER retention for `Failed`
    ///   per §7.4), and increments the supplied `operator_alerts`
    ///   counter on `ROLLBACK_FAILED`.
    ///
    /// When `rollback_driver` is `None`, the pipeline behaves identically
    /// to [`Self::run_with_full_engines_and_evidence`] (T-031 baseline
    /// preserved bit-for-bit).
    ///
    /// # Errors
    ///
    /// Same as [`Self::run_with_full_engines_and_evidence`]; additionally
    /// returns [`RuntimeError::ManifestInvalid`] when a manifest's
    /// `rollback_strategy` string fails to parse.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_with_full_engines_and_evidence_and_rollback(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        now: DateTime<Utc>,
        registry: Option<&InMemoryAdapterRegistry>,
        queue: Option<&DispatchQueue>,
        kernel: Option<&dyn PolicyKernel>,
        runtime_context: Option<&RuntimeContext>,
        tripwire: Option<&AtomicU64>,
        emitter: &EvidenceEmitter,
        rollback_driver: Option<&RollbackDriver>,
        operator_alerts: Option<&AtomicU64>,
    ) -> Result<ActionContext, RuntimeError> {
        // ── Step 1: validate envelope. Emit ACTION_RECEIVED on success. ──
        let state = self.step_validate(envelope, ctx, now)?;
        let state = match state {
            PipelineState::Continue(mut c) => {
                emitter.emit_action_received(envelope, &mut c).await?;
                PipelineState::Continue(c)
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };

        // ── Step 2: policy evaluate. ──
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_policy_evaluate_with_kernel_and_emit(
                    envelope,
                    c,
                    now,
                    kernel,
                    runtime_context,
                    tripwire,
                    emitter,
                )
                .await?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };

        // ── Step 3: approval (T-034 stub today). ──
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_request_approval_passthrough(envelope, c, now)?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };

        // ── Step 4: queue enroll. ──
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_queue_with_engine_and_emit(envelope, c, now, queue, emitter)
                    .await?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };

        // ── Step 5: execute. ──
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_execute_with_engines_and_emit(envelope, c, now, registry, emitter)
                    .await?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };

        // ── Step 6: verify (with verify-failure injection seam). ──
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_verify_inject_failure_and_emit(envelope, c, now, emitter, rollback_driver)
                    .await?
            }
            PipelineState::ShortCircuit(c) => return Ok(c),
        };

        // ── Step 7: rollback (T-032 — real driver engaged). ──
        let state = match state {
            PipelineState::Continue(c) => {
                self.step_rollback_with_driver_and_emit(
                    envelope,
                    c,
                    now,
                    registry,
                    rollback_driver,
                    operator_alerts,
                    emitter,
                )
                .await?
            }
            PipelineState::ShortCircuit(c) => {
                // The verify step short-circuited (today: SUCCEEDED on the
                // no-injection happy path). Rollback has no work; preserve
                // the terminal context.
                return Ok(c);
            }
        };

        // ── Step 8: terminal evidence pass-through. ──
        let state = match state {
            PipelineState::Continue(c) | PipelineState::ShortCircuit(c) => {
                self.step_emit_evidence(envelope, c, now)?
            }
        };
        Ok(state.into_context())
    }

    /// T-031 — emitter-aware sibling of
    /// [`Self::step_policy_evaluate_with_kernel`]. Emits
    /// `POLICY_DECISION` after the kernel returns and before the
    /// transition fires (so the receipt records both the decision id and
    /// the lifecycle state the runtime moves into).
    ///
    /// On the no-kernel-attached fallback (T-027 stub: unconditional T4)
    /// no `POLICY_DECISION` evidence is emitted because there is no
    /// authentic decision to record. T-034 will re-emit at approval-
    /// resume time once the orchestrator lands.
    ///
    /// # Errors
    ///
    /// Same as [`Self::step_policy_evaluate_with_kernel`]; additionally
    /// returns [`RuntimeError::EvidenceEmitFailed`] on emitter failure.
    #[allow(clippy::too_many_arguments)]
    pub async fn step_policy_evaluate_with_kernel_and_emit(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        now: DateTime<Utc>,
        kernel: Option<&dyn PolicyKernel>,
        runtime_context: Option<&RuntimeContext>,
        tripwire: Option<&AtomicU64>,
        emitter: &EvidenceEmitter,
    ) -> Result<PipelineState, RuntimeError> {
        let (Some(kernel), Some(rctx)) = (kernel, runtime_context) else {
            // No kernel attached → fall back to T-027 stub. No POLICY_DECISION
            // evidence (no authentic decision to record).
            return self.step_policy_evaluate(envelope, ctx, now);
        };

        let policy_context = PolicyContext::new(
            rctx.subject.clone(),
            EnrichmentSnapshot::default(),
            rctx.bundle_version.clone(),
            rctx.code_version.clone(),
        );

        let evaluation = kernel.evaluate_policy(envelope, &policy_context).await;
        let mut ctx = ctx;
        match evaluation {
            Ok(decision) => {
                // Apply the §4.2 transition first so the emitted receipt
                // records the lifecycle state the runtime moved into.
                let state = match decision.decision {
                    Decision::Allow => {
                        apply_transition(&mut ctx, ActionLifecycleState::Approved, now)?;
                        rctx.install_policy_constraints(Some(decision.constraints.clone()));
                        rctx.install_policy_approval(Some(decision.approval.clone()));
                        if rctx.subject.is_ai
                            && !decision.approval.approver_classes.iter().any(|c| {
                                matches!(c, ApproverClass::Human | ApproverClass::Operator)
                            })
                        {
                            if let Some(counter) = tripwire {
                                counter.fetch_add(1, Ordering::AcqRel);
                            }
                        }
                        PipelineState::Continue(ctx)
                    }
                    Decision::RequireApproval => {
                        apply_transition(&mut ctx, ActionLifecycleState::ApprovalPending, now)?;
                        rctx.install_policy_constraints(Some(decision.constraints.clone()));
                        rctx.install_policy_approval(Some(decision.approval.clone()));
                        PipelineState::ShortCircuit(ctx)
                    }
                    Decision::Deny => {
                        apply_transition(&mut ctx, ActionLifecycleState::PolicyDenied, now)?;
                        rctx.install_policy_constraints(None);
                        rctx.install_policy_approval(None);
                        PipelineState::ShortCircuit(ctx)
                    }
                    Decision::Unspecified => {
                        return Err(RuntimeError::PolicyEvalFailed(
                            "kernel returned Decision::Unspecified (S2.3 §4 invariant violated)"
                                .to_string(),
                        ));
                    }
                };

                // Emit POLICY_DECISION. The receipt links the decision id
                // and the lifecycle state we just moved into.
                let mut ctx_with_emit = state.into_context();
                emitter
                    .emit_policy_decision(envelope, &mut ctx_with_emit, &decision)
                    .await?;
                // Reconstruct the PipelineState. Allow → Continue; the
                // other two are short-circuit terminals.
                match decision.decision {
                    Decision::Allow => Ok(PipelineState::Continue(ctx_with_emit)),
                    _ => Ok(PipelineState::ShortCircuit(ctx_with_emit)),
                }
            }
            Err(PolicyError::SubjectUnauthenticated) => {
                apply_transition(&mut ctx, ActionLifecycleState::PolicyDenied, now)?;
                ctx.error = Some(ExecutionFailureReason::EnvelopeValidationFailed);
                rctx.install_policy_constraints(None);
                rctx.install_policy_approval(None);
                // No POLICY_DECISION evidence — the kernel did not produce
                // a decision. The terminal state alone is the audit
                // signal; T-031 elects not to synthesise a phantom
                // decision id.
                Ok(PipelineState::ShortCircuit(ctx))
            }
            Err(other) => Err(RuntimeError::PolicyEvalFailed(other.to_string())),
        }
    }

    /// T-031 — emitter-aware sibling of
    /// [`Self::step_queue_with_engine`]. Emits the queue-enrolment
    /// marker after the §4.2 T12 transition and the §11.4 downgrade
    /// marker when the AI-interactive condition fires.
    ///
    /// # Errors
    ///
    /// Same as [`Self::step_queue_with_engine`]; additionally returns
    /// [`RuntimeError::EvidenceEmitFailed`] on emitter failure.
    pub async fn step_queue_with_engine_and_emit(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        now: DateTime<Utc>,
        queue: Option<&DispatchQueue>,
        emitter: &EvidenceEmitter,
    ) -> Result<PipelineState, RuntimeError> {
        // §11.4 downgrade: check BEFORE running the queue step because
        // `step_queue_with_engine` folds the downgrade into the selection
        // (so by the time it returns, ctx.queue_class is already
        // AGENT_PROPOSAL, losing the "was downgraded" signal). The
        // dispatcher's pure `apply_ai_interactive_downgrade` helper
        // computes the marker against the pre-enrolment context.
        let downgrade_marker =
            ActionDispatcher::apply_ai_interactive_downgrade(&ctx, envelope.identity.is_ai);

        let state = self
            .step_queue_with_engine(envelope, ctx, now, queue)
            .await?;
        match state {
            PipelineState::Continue(mut c) => {
                let queue_class = c.queue_class;
                emitter
                    .emit_action_queued(envelope, &mut c, queue_class)
                    .await?;
                if downgrade_marker.is_some() {
                    emitter
                        .emit_ai_interactive_queue_downgrade(envelope, &mut c)
                        .await?;
                }
                Ok(PipelineState::Continue(c))
            }
            PipelineState::ShortCircuit(c) => Ok(PipelineState::ShortCircuit(c)),
        }
    }

    /// T-031 — emitter-aware sibling of
    /// [`Self::step_execute_with_engines`]. Emits `ROUTING_DECISION`
    /// (S3.1 §4 ID 3) after the adapter is resolved and
    /// `EXECUTION_STARTED` (S3.1 §4 ID 8) after the §4.2 T13 transition
    /// fires.
    ///
    /// When no registry is attached the routing/execution emissions are
    /// skipped — the T-027 baseline is preserved bit-for-bit.
    ///
    /// # Errors
    ///
    /// Same as [`Self::step_execute_with_engines`]; additionally returns
    /// [`RuntimeError::EvidenceEmitFailed`] on emitter failure.
    pub async fn step_execute_with_engines_and_emit(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        now: DateTime<Utc>,
        registry: Option<&InMemoryAdapterRegistry>,
        emitter: &EvidenceEmitter,
    ) -> Result<PipelineState, RuntimeError> {
        // Reuse the existing engine-aware step which sets `ctx.dispatch_kind`
        // when a registry is attached and drives the §4.2 T13 / T14
        // transition.
        let routed_adapter_id = registry.and_then(|reg| {
            use crate::runtime::AdapterRegistry;
            reg.lookup(&envelope.request.action)
                .map(|_| envelope.request.action.clone())
        });
        let state = self.step_execute_with_engines(envelope, ctx, now, registry)?;
        match state {
            PipelineState::Continue(mut c) => {
                // Adapter resolved: emit ROUTING_DECISION then
                // EXECUTION_STARTED.
                if let Some(adapter_kind) = routed_adapter_id {
                    let dispatch_kind = c.dispatch_kind;
                    emitter
                        .emit_routing_decision(envelope, &mut c, &adapter_kind, dispatch_kind)
                        .await?;
                }
                emitter.emit_execution_started(envelope, &mut c).await?;
                Ok(PipelineState::Continue(c))
            }
            PipelineState::ShortCircuit(c) => Ok(PipelineState::ShortCircuit(c)),
        }
    }

    /// T-031 — emitter-aware sibling of [`Self::step_verify`]. Emits
    /// `EXECUTION_COMPLETED` (S3.1 §4 ID 9) then `VERIFICATION_RESULT`
    /// (S3.1 §4 ID 10). Today's stub always drives `SUCCEEDED`; T-035
    /// will swap in the real verification engine without changing the
    /// emission shape.
    ///
    /// # Errors
    ///
    /// Same as [`Self::step_verify`]; additionally returns
    /// [`RuntimeError::EvidenceEmitFailed`] on emitter failure.
    pub async fn step_verify_and_emit(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        now: DateTime<Utc>,
        emitter: &EvidenceEmitter,
    ) -> Result<PipelineState, RuntimeError> {
        // First emit EXECUTION_COMPLETED for the EXECUTING → VERIFYING
        // transition. We do this before driving step_verify because that
        // step performs two consecutive transitions (T15 then T17) and
        // we need to fence the EXECUTION_COMPLETED before the
        // VERIFICATION_RESULT.
        let mut pre = ctx;
        emitter
            .emit_execution_completed(envelope, &mut pre, "ADAPTER_OK")
            .await?;
        let state = self.step_verify(envelope, pre, now)?;
        match state {
            PipelineState::Continue(mut c) | PipelineState::ShortCircuit(mut c) => {
                let passed = c.status == ActionLifecycleState::Succeeded;
                emitter
                    .emit_verification_result(envelope, &mut c, passed)
                    .await?;
                // Re-wrap as ShortCircuit because step_verify always ends
                // in a terminal in T-031 scope (today's stub drives
                // SUCCEEDED unconditionally).
                Ok(PipelineState::ShortCircuit(c))
            }
        }
    }

    /// T-030 — sibling of [`Self::step_policy_evaluate`] that consults the
    /// Policy Kernel.
    ///
    /// See [`Self::run_with_full_engines`] for the full Decision →
    /// §4.2-transition mapping; this method is the per-step body that
    /// `run_with_full_engines` drives.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::InvalidTransition`] when the context is not
    /// in [`ActionLifecycleState::PolicyPending`].
    /// Returns [`RuntimeError::PolicyEvalFailed`] on a non-recoverable
    /// `PolicyError` from the kernel (excluding `SubjectUnauthenticated`,
    /// which short-circuits to T6 `POLICY_PENDING → POLICY_DENIED`).
    pub async fn step_policy_evaluate_with_kernel(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        now: DateTime<Utc>,
        kernel: Option<&dyn PolicyKernel>,
        runtime_context: Option<&RuntimeContext>,
        tripwire: Option<&AtomicU64>,
    ) -> Result<PipelineState, RuntimeError> {
        // No kernel attached → fall back to T-027 stub (unconditional T4).
        // Preserves T-027 / T-028 / T-029 baselines verbatim.
        let (Some(kernel), Some(rctx)) = (kernel, runtime_context) else {
            return self.step_policy_evaluate(envelope, ctx, now);
        };

        // Build the `PolicyContext` from the `RuntimeContext`. The
        // enrichment snapshot is empty for T-030 (AIOS-FS read-path is
        // M4+ scope); the snapshot id is computed by
        // `EnrichmentSnapshot::default()` and is a stable sentinel value
        // the audit tooling recognises as "no enrichment".
        let policy_context = PolicyContext::new(
            rctx.subject.clone(),
            EnrichmentSnapshot::default(),
            rctx.bundle_version.clone(),
            rctx.code_version.clone(),
        );

        let evaluation = kernel.evaluate_policy(envelope, &policy_context).await;
        let mut ctx = ctx;
        match evaluation {
            Ok(decision) => {
                match decision.decision {
                    Decision::Allow => {
                        // T4 — POLICY_PENDING → APPROVED.
                        apply_transition(&mut ctx, ActionLifecycleState::Approved, now)?;
                        // Constraints projection (Option A): hand the bound
                        // §10 constraints to the RuntimeContext for downstream
                        // dispatcher / verify steps.
                        rctx.install_policy_constraints(Some(decision.constraints));
                        rctx.install_policy_approval(Some(decision.approval.clone()));
                        // §17 defense-in-depth tripwire: AI subject + ALLOW
                        // without a human approver class.
                        if rctx.subject.is_ai
                            && !decision.approval.approver_classes.iter().any(|c| {
                                matches!(c, ApproverClass::Human | ApproverClass::Operator)
                            })
                        {
                            if let Some(counter) = tripwire {
                                counter.fetch_add(1, Ordering::AcqRel);
                            }
                        }
                        Ok(PipelineState::Continue(ctx))
                    }
                    Decision::RequireApproval => {
                        // T5 — POLICY_PENDING → APPROVAL_PENDING. Short-circuit;
                        // T-034 owns the approval orchestration that resumes
                        // from here.
                        apply_transition(&mut ctx, ActionLifecycleState::ApprovalPending, now)?;
                        rctx.install_policy_constraints(Some(decision.constraints));
                        rctx.install_policy_approval(Some(decision.approval));
                        Ok(PipelineState::ShortCircuit(ctx))
                    }
                    Decision::Deny => {
                        // T6 — POLICY_PENDING → POLICY_DENIED (terminal).
                        // T-031 will revisit the override-on-deny path (T7)
                        // when the override boundary handle is threaded
                        // through the runtime alongside the kernel; today
                        // the kernel's pipeline step 5 already consults its
                        // own boundary handle, so any successful override
                        // relaxation has already converted the Deny into an
                        // Allow inside the kernel — reaching `Decision::Deny`
                        // here means no override path was authored.
                        apply_transition(&mut ctx, ActionLifecycleState::PolicyDenied, now)?;
                        rctx.install_policy_constraints(None);
                        rctx.install_policy_approval(None);
                        Ok(PipelineState::ShortCircuit(ctx))
                    }
                    Decision::Unspecified => {
                        // Spec invariant: `Decision::Unspecified` is reserved
                        // for proto3 wire compatibility and never produced by
                        // a real evaluation (S2.3 §4). If we see it, the
                        // kernel impl is buggy — fail closed via
                        // RuntimeError::PolicyEvalFailed.
                        Err(RuntimeError::PolicyEvalFailed(
                            "kernel returned Decision::Unspecified (S2.3 §4 invariant violated)"
                                .to_string(),
                        ))
                    }
                }
            }
            Err(PolicyError::SubjectUnauthenticated) => {
                // §7 — unauthenticated subject. The kernel does not produce
                // a decision; the runtime maps this onto T6
                // (`POLICY_PENDING → POLICY_DENIED`) with
                // `error = EnvelopeValidationFailed` (the §3.6 enum has no
                // dedicated SubjectUnauthenticated row; the envelope's
                // identity is the runtime's input contract, so
                // EnvelopeValidationFailed is the spec-pinned closest
                // reason).
                apply_transition(&mut ctx, ActionLifecycleState::PolicyDenied, now)?;
                ctx.error = Some(ExecutionFailureReason::EnvelopeValidationFailed);
                rctx.install_policy_constraints(None);
                rctx.install_policy_approval(None);
                Ok(PipelineState::ShortCircuit(ctx))
            }
            Err(other) => Err(RuntimeError::PolicyEvalFailed(other.to_string())),
        }
    }

    /// T-029 — async sibling of [`Self::step_queue`] that consults the
    /// dispatch queue.
    ///
    /// Behaviour:
    /// - `queue = None` — identical to [`Self::step_queue`] (T-027 stub).
    /// - `queue = Some(...)` — selects the [`QueueClass`] via
    ///   [`ActionDispatcher::select_queue_class`] (`recovery_mode` is
    ///   `false` today; T-030 will surface the host's recovery flag), applies the
    ///   §11.4 AI-interactive downgrade if applicable, updates
    ///   `ctx.queue_class`, then calls [`DispatchQueue::enroll`].
    ///
    ///   - On success → `APPROVED → QUEUED` (T12) and continue.
    ///   - On [`RuntimeError::QueueFull`] / [`RuntimeError::RateLimited`] →
    ///     `APPROVED → QUEUED → FAILED` (T12 + T14) with
    ///     `error = ExecutionFailureReason::ResourceBudgetExceeded`, then
    ///     short-circuit.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::InvalidTransition`] if the context is not in
    /// [`ActionLifecycleState::Approved`].
    pub async fn step_queue_with_engine(
        &self,
        envelope: &ActionEnvelope,
        mut ctx: ActionContext,
        now: DateTime<Utc>,
        queue: Option<&DispatchQueue>,
    ) -> Result<PipelineState, RuntimeError> {
        let Some(queue) = queue else {
            return self.step_queue(envelope, ctx, now);
        };

        // §3.5 / §11.4 selection. `recovery_mode = false` for T-029; T-030
        // will surface the host's recovery flag through `RuntimeContext`.
        let is_ai = envelope.identity.is_ai;
        let selected = ActionDispatcher::select_queue_class(envelope, false);
        ctx.queue_class = selected;

        // §11.4 silent downgrade marker — folded into selection above, but
        // the call is preserved for the forensic log (T-031 emits the
        // evidence record on the marker presence).
        let _downgrade_marker = ActionDispatcher::apply_ai_interactive_downgrade(&ctx, is_ai);

        // §4.2 T12 — APPROVED → QUEUED.
        apply_transition(&mut ctx, ActionLifecycleState::Queued, now)?;

        // Admission gate. The subject id is the envelope's
        // `subject_canonical_id`; T-030 will replace it with the hydrated
        // typed subject.
        let subject_id = envelope.identity.subject_canonical_id.clone();
        match queue.enroll(ctx.clone(), &subject_id).await {
            Ok(stored) => Ok(PipelineState::Continue(stored)),
            Err(RuntimeError::QueueFull(_) | RuntimeError::RateLimited(_)) => {
                // §4.2 T14 — QUEUED → FAILED with ResourceBudgetExceeded.
                apply_transition(&mut ctx, ActionLifecycleState::Failed, now)?;
                ctx.error = Some(ExecutionFailureReason::ResourceBudgetExceeded);
                Ok(PipelineState::ShortCircuit(ctx))
            }
            Err(other) => Err(other),
        }
    }

    /// T-029 — sibling of [`Self::step_execute`] that additionally records
    /// the dispatcher's chosen [`ActionDispatchKind`] (per the §3.2 closed
    /// table) onto `ctx.dispatch_kind` when a registry is attached.
    ///
    /// Lookup behaviour mirrors [`Self::step_execute`]; the only addition
    /// is the dispatch-kind selection. When no manifest can be retrieved
    /// (no registry attached, or unknown `action_kind`), the
    /// `dispatch_kind` on the context is left at its prior value — the
    /// T-026 [`fresh_context`] seed ([`ActionDispatchKind::SubprocessFork`]).
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::InvalidTransition`] only if the context is
    /// not in [`ActionLifecycleState::Queued`].
    pub fn step_execute_with_engines(
        &self,
        envelope: &ActionEnvelope,
        mut ctx: ActionContext,
        now: DateTime<Utc>,
        registry: Option<&InMemoryAdapterRegistry>,
    ) -> Result<PipelineState, RuntimeError> {
        if let Some(registry) = registry {
            use crate::runtime::AdapterRegistry;
            let Some(handle) = registry.lookup(&envelope.request.action) else {
                apply_transition(&mut ctx, ActionLifecycleState::Failed, now)?;
                ctx.error = Some(ExecutionFailureReason::DependencyUnready);
                return Ok(PipelineState::ShortCircuit(ctx));
            };
            // Down-cast to RealAdapterHandle to pull the manifest for the
            // §3.2 dispatch-kind decision. The cast is safe because the
            // InMemoryAdapterRegistry only ever returns RealAdapterHandle
            // instances; the trait object is what `AdapterRegistry::lookup`
            // returns for forward compatibility with future handle kinds.
            // We perform the decision via a minimal sketch using the
            // handle's dispatch_kind() (manifest's preferred kind) plus
            // §3.2 modifiers.
            let manifest_kind = handle.dispatch_kind();
            let is_ai = envelope.identity.is_ai;
            let is_simulate = matches!(envelope.request.dry_run, aios_action::DryRunMode::Simulate);
            // Synthesise a minimal AdapterManifest-shaped view by reading
            // the handle's preferred dispatch kind + assuming Stable
            // stability when the registry returned it (Retired adapters
            // are filtered upstream by `AdapterRegistry::lookup`).
            // `risk_privileged` is `false` for T-029 (the typed risk
            // surface is not yet on the envelope).
            let chosen = compute_dispatch_kind(
                manifest_kind,
                DispatchKindInputs {
                    is_simulate,
                    is_ai,
                    risk_privileged: false,
                    manifest_stable: true,
                },
            );
            ctx.dispatch_kind = chosen;
        }
        apply_transition(&mut ctx, ActionLifecycleState::Executing, now)?;
        Ok(PipelineState::Continue(ctx))
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

    /// Step 3 — issue an `ApprovalRequest` to the Approval Mechanics service
    /// and park the action at [`ActionLifecycleState::ApprovalPending`].
    ///
    /// **T-034 — approval orchestration (S10.1 §6 + S5.3 + S2.3 §11.2).**
    ///
    /// This step is invoked **after** [`Self::step_policy_evaluate_with_kernel_and_emit`]
    /// has driven T5 (`POLICY_PENDING → APPROVAL_PENDING`). It does not
    /// drive a §4.2 transition itself — the policy step has already
    /// short-circuited the lifecycle into `APPROVAL_PENDING`. Step 3's job
    /// is to:
    ///
    /// 1. Mint a fresh `actrq_<ULID>` runtime request handle.
    /// 2. Compose the [`crate::ApprovalRequest`] from the policy decision's
    ///    [`aios_policy::ApprovalRequirement`].
    /// 3. Submit the request to the [`crate::ApprovalBindingSink`].
    /// 4. Emit `APPROVAL_REQUESTED` evidence (when an emitter is wired).
    ///
    /// The action is then returned in `ShortCircuit`: `submit_action`
    /// terminates here and the caller resumes via `ExecuteAction` once the
    /// operator has granted the binding through the Approval Mechanics
    /// service.
    ///
    /// When `sink` is `None`, this step is a no-op: the action stays
    /// `ApprovalPending` and the caller observes the short-circuit per the
    /// T-027 baseline (this preserves backwards compatibility for callers
    /// that wire a kernel without yet wiring an approval sink).
    ///
    /// Synchronous structural pass-through used by the legacy pipeline
    /// drivers (`run_with_registry`, `run_with_engines`, `run_with_full_engines`,
    /// `run_with_full_engines_and_evidence`,
    /// `run_with_full_engines_and_evidence_and_rollback`).
    ///
    /// These drivers do not engage approval orchestration (no
    /// [`crate::ApprovalBindingSink`] is threaded); when the policy step
    /// has not short-circuited, the action's lifecycle is `APPROVED` and
    /// there is nothing for step 3 to do. The async
    /// [`Self::step_request_approval`] supersedes this pass-through on
    /// the T-034 entry points that thread a sink.
    ///
    /// # Errors
    ///
    /// Currently infallible; the `Result` is kept for signature parity
    /// with sibling pipeline steps.
    #[allow(
        clippy::unused_self,
        clippy::unnecessary_wraps,
        clippy::missing_const_for_fn,
        reason = "structural pass-through; signature kept stable across the trait surface so the legacy drivers can chain through step 3 without async colour"
    )]
    pub fn step_request_approval_passthrough(
        &self,
        _envelope: &ActionEnvelope,
        ctx: ActionContext,
        _now: DateTime<Utc>,
    ) -> Result<PipelineState, RuntimeError> {
        Ok(PipelineState::Continue(ctx))
    }

    /// T-034 — submit an [`crate::ApprovalRequest`] to the configured
    /// [`crate::ApprovalBindingSink`] and emit `APPROVAL_REQUESTED`
    /// evidence. Parks the action at
    /// [`ActionLifecycleState::ApprovalPending`].
    ///
    /// # Errors
    ///
    /// - [`RuntimeError::EvidenceEmitFailed`] — emitter rejected the
    ///   `APPROVAL_REQUESTED` receipt.
    /// - Sink-specific errors propagate from
    ///   [`crate::ApprovalBindingSink::submit_request`].
    #[allow(clippy::too_many_arguments)]
    pub async fn step_request_approval(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        _now: DateTime<Utc>,
        sink: Option<&dyn crate::ApprovalBindingSink>,
        requirement: Option<&aios_policy::ApprovalRequirement>,
        emitter: Option<&EvidenceEmitter>,
    ) -> Result<PipelineState, RuntimeError> {
        // Only meaningful at APPROVAL_PENDING; for any other state pass
        // through unchanged so the eight-step driver loop can still call
        // this step as a structural slot when the policy step has not
        // short-circuited.
        if ctx.status != ActionLifecycleState::ApprovalPending {
            return Ok(PipelineState::Continue(ctx));
        }

        // No sink wired → keep the T-027 baseline (action parks at
        // ApprovalPending and the caller resumes externally). Tests
        // exercise this branch explicitly.
        let Some(sink) = sink else {
            return Ok(PipelineState::ShortCircuit(ctx));
        };

        // Build the request. The requirement is sourced from the policy
        // decision; when absent (caller wired the sink without a kernel)
        // fall back to a default that demands a Human approver — the
        // safest fail-closed posture.
        let default_requirement = aios_policy::ApprovalRequirement {
            required: true,
            approval_scope: aios_policy::ApprovalScope::ExactRequestHash,
            ttl_seconds: 300,
            approver_classes: vec![aios_policy::ApproverClass::Human],
            require_human_co_signer: false,
        };
        let req = requirement.unwrap_or(&default_requirement).clone();

        let request_id = aios_action::ActionRuntimeRequestId::new().to_string();
        let canonical_hash = aios_action::canonical::jcs_canonicalize(&envelope.request)
            .ok()
            .map(|s| aios_action::canonical::blake3_truncated(s.as_bytes()))
            .unwrap_or_default();

        let approval_request = crate::ApprovalRequest {
            request_id: request_id.clone(),
            action_id: ctx.action_id.clone(),
            requirement: req.clone(),
            proposing_subject_id: envelope.identity.subject_canonical_id.clone(),
            proposing_subject_is_ai: envelope.identity.is_ai,
            bound_action_canonical_hash: canonical_hash,
            requested_at: ctx.last_updated_at,
        };

        sink.submit_request(approval_request).await?;

        let mut ctx = ctx;
        if let Some(emitter) = emitter {
            emitter
                .emit_approval_requested(
                    envelope,
                    &mut ctx,
                    &request_id,
                    &envelope.identity.subject_canonical_id,
                    envelope.identity.is_ai,
                    req.ttl_seconds,
                    req.require_human_co_signer,
                )
                .await?;
        }

        Ok(PipelineState::ShortCircuit(ctx))
    }

    /// Consume an approval binding atomically and drive
    /// `APPROVAL_PENDING → APPROVED` (T8).
    ///
    /// **T-034 — `ExecuteAction` resume path (S10.1 §6.1 step 3 + S5.3 §13.1).**
    ///
    /// Fails closed per the closed-vocabulary discipline:
    /// - Unknown binding → [`RuntimeError::ApprovalBindingInvalid`].
    /// - State `Pending` / `Denied` → [`RuntimeError::ApprovalBindingInvalid`].
    /// - State `Consumed` (anti-replay) → [`RuntimeError::ApprovalBindingConsumed`].
    /// - State `Expired` (TTL elapsed) → [`RuntimeError::ApprovalBindingExpired`].
    /// - `granted_by_class` not in the policy's `required_approver_classes`
    ///   filter → [`RuntimeError::ApprovalApproverClassMismatch`].
    /// - AI self-approval (`envelope.identity.is_ai` AND
    ///   `binding.granted_by == envelope.identity.subject_canonical_id`)
    ///   → [`RuntimeError::ApprovalApproverClassMismatch`] (defense-in-depth
    ///   per S2.3 §17 and S5.3 §14.3 — already enforced at the policy and
    ///   identity layers; this is the constitutional backstop).
    ///
    /// On success, drives T8 (`APPROVAL_PENDING → APPROVED`) and emits
    /// `APPROVAL_GRANTED` evidence (when an emitter is wired).
    ///
    /// # Errors
    ///
    /// See the variants enumerated above.
    #[allow(clippy::too_many_arguments)]
    pub async fn step_consume_binding(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        now: DateTime<Utc>,
        sink: &dyn crate::ApprovalBindingSink,
        binding_id: &str,
        requirement: Option<&aios_policy::ApprovalRequirement>,
        emitter: Option<&EvidenceEmitter>,
    ) -> Result<PipelineState, RuntimeError> {
        // Pre-flight: the action must be in APPROVAL_PENDING to consume a
        // binding. The caller (gRPC ExecuteAction handler) is responsible
        // for routing only ApprovalPending actions through this gate.
        if ctx.status != ActionLifecycleState::ApprovalPending {
            return Err(RuntimeError::InvalidTransition {
                from: ctx.status,
                to: ActionLifecycleState::Approved,
            });
        }

        // Atomic consume — fails closed on every non-Granted state.
        let binding = sink.consume_binding(binding_id).await?;

        // §13.2 action-revision invariant — recompute the canonical hash
        // of the envelope's `request` and compare against the binding's
        // frozen hash. Divergence voids the binding.
        let current_hash = aios_action::canonical::jcs_canonicalize(&envelope.request)
            .ok()
            .map(|s| aios_action::canonical::blake3_truncated(s.as_bytes()))
            .unwrap_or_default();
        if !binding.bound_action_canonical_hash.is_empty()
            && current_hash != binding.bound_action_canonical_hash
        {
            if let Some(emitter) = emitter {
                let mut ctx_e = ctx.clone();
                emitter
                    .emit_approval_denied(
                        envelope,
                        &mut ctx_e,
                        Some(binding.request_id.clone()),
                        Some(binding.binding_id.clone()),
                        "ACTION_REVISED",
                        "binding voided: action canonical hash changed between grant and execute",
                    )
                    .await?;
            }
            return Err(RuntimeError::ApprovalBindingInvalid(format!(
                "binding {binding_id} bound to a different canonical hash"
            )));
        }

        // AI self-approval defense-in-depth (S2.3 §17 / S5.3 §14.3).
        // The policy kernel already enforces this, and the identity
        // service refuses to sign an AI grant; the runtime double-checks
        // to make INV-002 self-evident at the L3 consume gate.
        if envelope.identity.is_ai && binding.granted_by == envelope.identity.subject_canonical_id {
            if let Some(emitter) = emitter {
                let mut ctx_e = ctx.clone();
                emitter
                    .emit_approval_denied(
                        envelope,
                        &mut ctx_e,
                        Some(binding.request_id.clone()),
                        Some(binding.binding_id.clone()),
                        "AI_SELF_APPROVAL_BLOCKED",
                        "AI subject cannot approve its own action (INV-002 defense-in-depth)",
                    )
                    .await?;
            }
            return Err(RuntimeError::ApprovalApproverClassMismatch);
        }

        // Approver class filter — match the binding's `granted_by_class`
        // against the requirement's `approver_classes`. When no
        // requirement is supplied, the default policy is "any approver
        // class except AI" (defense-in-depth — the AI case is already
        // filtered above).
        let class_matches = match requirement {
            Some(req) if !req.approver_classes.is_empty() => {
                req.approver_classes.contains(&binding.granted_by_class)
            }
            _ => !matches!(binding.granted_by_class, aios_policy::ApproverClass::Agent),
        };
        if !class_matches {
            if let Some(emitter) = emitter {
                let mut ctx_e = ctx.clone();
                emitter
                    .emit_approval_denied(
                        envelope,
                        &mut ctx_e,
                        Some(binding.request_id.clone()),
                        Some(binding.binding_id.clone()),
                        "APPROVER_CLASS_MISMATCH",
                        "binding granted_by_class is not in the policy's required approver classes",
                    )
                    .await?;
            }
            return Err(RuntimeError::ApprovalApproverClassMismatch);
        }

        // T8 — APPROVAL_PENDING → APPROVED.
        let mut ctx = ctx;
        apply_transition(&mut ctx, ActionLifecycleState::Approved, now)?;

        if let Some(emitter) = emitter {
            emitter
                .emit_approval_granted(
                    envelope,
                    &mut ctx,
                    &binding.request_id,
                    &binding.binding_id,
                    &binding.granted_by,
                    &binding.bound_action_canonical_hash,
                )
                .await?;
        }

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
    /// **T-028 wires the adapter registry lookup + `FAIL_CLOSED` on unknown
    /// adapter; T-029 wires the dispatcher.**
    ///
    /// Behaviour depends on whether a registry is attached:
    ///
    /// - `registry = None` (T-027 contract) — structural pass-through;
    ///   `QUEUED → EXECUTING` (T13) and `Continue`. Used by the existing
    ///   integration tests and by the M4 §22 golden path before T-029
    ///   wires the dispatcher.
    /// - `registry = Some(...)` (T-028 contract) — look up an adapter
    ///   declaring `envelope.request.action`. **Hit:** structural
    ///   `QUEUED → EXECUTING` (T13) and `Continue` (the dispatcher itself
    ///   is queued for T-029; T-028 only proves the registry is consulted).
    ///   **Miss:** transition `QUEUED → FAILED` (T14) with
    ///   [`ExecutionFailureReason::DependencyUnready`] and short-circuit.
    ///
    /// The §6.1 eight pre-dispatch steps (canonical-hash re-validate,
    /// policy re-evaluate, binding re-check, sandbox compose, etc.) are
    /// the joint responsibility of T-028 (this lookup step), T-029
    /// (dispatcher), and T-030 (policy re-evaluate). T-028 lands the
    /// outer-most gate.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::InvalidTransition`] if the context is not in
    /// [`ActionLifecycleState::Queued`].
    pub fn step_execute(
        &self,
        envelope: &ActionEnvelope,
        mut ctx: ActionContext,
        now: DateTime<Utc>,
        registry: Option<&InMemoryAdapterRegistry>,
    ) -> Result<PipelineState, RuntimeError> {
        if let Some(registry) = registry {
            // Synchronous trait lookup via `AdapterRegistry::lookup`. This
            // path does not engage the dispatcher (T-029) — it only proves
            // the registry was consulted and that an unknown adapter
            // FAIL_CLOSEs before any dispatch attempt.
            //
            // `lookup` returns `None` for: (a) no adapter declares the kind;
            // (b) the declaring adapter is `AdapterStability::Retired`; or
            // (c) the registry's read lock was momentarily unavailable —
            // all three are observationally equivalent to "adapter not
            // ready" at this step's granularity.
            use crate::runtime::AdapterRegistry;
            if registry.lookup(&envelope.request.action).is_none() {
                apply_transition(&mut ctx, ActionLifecycleState::Failed, now)?;
                ctx.error = Some(ExecutionFailureReason::DependencyUnready);
                return Ok(PipelineState::ShortCircuit(ctx));
            }
        }
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
        reason = "stub kept for backwards-compat with the no-rollback-driver baseline; T-032 wires the real driver via `step_rollback_with_driver`"
    )]
    pub fn step_rollback(
        &self,
        _envelope: &ActionEnvelope,
        ctx: ActionContext,
        _now: DateTime<Utc>,
    ) -> Result<PipelineState, RuntimeError> {
        Ok(PipelineState::Continue(ctx))
    }

    /// T-032 — sibling of [`Self::step_rollback`] that engages the
    /// [`RollbackDriver`] when the action is in
    /// [`ActionLifecycleState::Failed`] and the adapter's
    /// `rollback_strategy != NONE`.
    ///
    /// Behaviour:
    ///
    /// - If `ctx.status != FAILED`, this is a no-op pass-through (the
    ///   action either succeeded or short-circuited earlier; there is
    ///   nothing to roll back).
    /// - If `driver` is `None`, this is a no-op pass-through (T-027
    ///   backward-compat baseline).
    /// - If `registry` is `None` or no adapter declares
    ///   `envelope.request.action`, no rollback can be attempted (the
    ///   strategy is unknown); the action stays `FAILED`.
    /// - Otherwise: read the strategy from the manifest, call
    ///   [`RollbackDriver::run_rollback`], record the outcome on
    ///   `ctx.rollback_outcome`, and apply the §4.2 transition per
    ///   [`RollbackDriver::classify_terminal`]:
    ///   - `Succeeded` → T19 (`FAILED → ROLLED_BACK`)
    ///   - `Failed` → T20 (`FAILED → ROLLBACK_FAILED`); increments the
    ///     supplied `operator_alerts` counter (the §7.4 alert).
    ///   - `NotAttempted` / `NotApplicable` → no transition; `FAILED`
    ///     stays.
    ///
    /// The §7.4 FOREVER-retention evidence emission is the caller's
    /// responsibility (see
    /// [`Self::step_rollback_with_driver_and_emit`]).
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::InvalidTransition`] only if a step author
    /// requests a §4.2 transition that is not in the table — which can
    /// only happen via a code defect; the strategy / outcome mapping is
    /// exhaustive by construction.
    /// Returns [`RuntimeError::ManifestInvalid`] when the manifest's
    /// `rollback_strategy` string fails [`RollbackStrategy::parse_manifest_value`].
    pub async fn step_rollback_with_driver(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        now: DateTime<Utc>,
        registry: Option<&InMemoryAdapterRegistry>,
        driver: Option<&RollbackDriver>,
        operator_alerts: Option<&AtomicU64>,
    ) -> Result<(PipelineState, Option<RollbackOutcome>), RuntimeError> {
        // Driver not configured → preserve the T-027 stub behaviour.
        let Some(driver) = driver else {
            return Ok((PipelineState::Continue(ctx), None));
        };
        // Not in FAILED → nothing to roll back.
        if ctx.status != ActionLifecycleState::Failed {
            return Ok((PipelineState::Continue(ctx), None));
        }
        // No registry / no adapter → cannot resolve a strategy; stay
        // FAILED (the caller's evidence path records the gap).
        let Some(registry) = registry else {
            return Ok((PipelineState::ShortCircuit(ctx), None));
        };
        let Some(registered) = registry.lookup_for_target(&envelope.request.action).await else {
            return Ok((PipelineState::ShortCircuit(ctx), None));
        };
        // Resolve the per-action strategy from the manifest's
        // `declared_actions`.
        let strategy = registered
            .manifest
            .declared_actions
            .iter()
            .find(|d| d.action_kind == envelope.request.action)
            .map(|d| RollbackStrategy::parse_manifest_value(&d.rollback_strategy))
            .transpose()?
            .unwrap_or(RollbackStrategy::Unspecified);
        let adapter = crate::adapter_handle::RealAdapterHandle::new(std::sync::Arc::new(
            registered.manifest.clone(),
        ));
        // Call the driver. Pure async; does not mutate ctx.
        let outcome = driver
            .run_rollback(envelope, &ctx, strategy, &adapter)
            .await;
        // Record the outcome on the context.
        let mut ctx = ctx;
        ctx.rollback_outcome = Some(outcome);
        // Apply the §4.2 terminal mapping.
        let terminal = RollbackDriver::classify_terminal(&outcome);
        match outcome {
            RollbackOutcome::Succeeded => {
                apply_transition(&mut ctx, terminal, now)?; // T19
                Ok((PipelineState::ShortCircuit(ctx), Some(outcome)))
            }
            RollbackOutcome::Failed => {
                apply_transition(&mut ctx, terminal, now)?; // T20
                                                            // §7.4 operator alert.
                if let Some(counter) = operator_alerts {
                    counter.fetch_add(1, Ordering::AcqRel);
                }
                Ok((PipelineState::ShortCircuit(ctx), Some(outcome)))
            }
            RollbackOutcome::NotAttempted | RollbackOutcome::NotApplicable => {
                // No FSM transition; FAILED stays. The outcome is still
                // recorded on the context for forensic emission.
                Ok((PipelineState::ShortCircuit(ctx), Some(outcome)))
            }
        }
    }

    /// T-032 — emitter-aware sibling of
    /// [`Self::step_rollback_with_driver`]. After the rollback FSM has
    /// classified the terminal, emits a single `ROLLBACK_COMPLETED`
    /// receipt (S3.1 §4 ID 11) via
    /// [`EvidenceEmitter::emit_rollback_completed`]. The emitter pins
    /// `Failed` outcomes at `FOREVER` retention per §7.4.
    ///
    /// # Errors
    ///
    /// Same as [`Self::step_rollback_with_driver`]; additionally returns
    /// [`RuntimeError::EvidenceEmitFailed`] on emitter failure.
    #[allow(clippy::too_many_arguments)]
    pub async fn step_rollback_with_driver_and_emit(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        now: DateTime<Utc>,
        registry: Option<&InMemoryAdapterRegistry>,
        driver: Option<&RollbackDriver>,
        operator_alerts: Option<&AtomicU64>,
        emitter: &EvidenceEmitter,
    ) -> Result<PipelineState, RuntimeError> {
        let (state, outcome) = self
            .step_rollback_with_driver(envelope, ctx, now, registry, driver, operator_alerts)
            .await?;
        // Emit ROLLBACK_COMPLETED only when an outcome was produced (the
        // driver actually attempted or declined to attempt rollback). When
        // `outcome` is `None`, the driver was not configured or the action
        // was not in FAILED — there is no rollback event to record.
        let Some(outcome) = outcome else {
            return Ok(state);
        };
        // Carry the pre-rollback `error` (the triggering failure reason)
        // for the receipt payload.
        match state {
            PipelineState::ShortCircuit(mut c) | PipelineState::Continue(mut c) => {
                let triggering = c.error;
                emitter
                    .emit_rollback_completed(envelope, &mut c, outcome, triggering)
                    .await?;
                Ok(PipelineState::ShortCircuit(c))
            }
        }
    }

    /// T-032 — emitter-aware sibling of [`Self::step_verify_and_emit`]
    /// that honours the [`RollbackDriver`]'s
    /// [`RollbackDriver::inject_verify_failure`] knob.
    ///
    /// When the driver's knob is `true`, the verify step drives
    /// `EXECUTING → VERIFYING → FAILED` (T15 + T18) with
    /// [`ExecutionFailureReason::AdapterRefused`] as the surrogate
    /// reason (the §3.6 enum has no dedicated
    /// `VERIFICATION_FAILED` row — `AdapterRefused` is the closest
    /// spec-pinned variant for "the action ran but the verification
    /// engine refused it"; T-035 will revisit once the real
    /// verification engine lands). Otherwise the step behaves identically
    /// to [`Self::step_verify_and_emit`].
    ///
    /// # Errors
    ///
    /// Same as [`Self::step_verify_and_emit`].
    pub async fn step_verify_inject_failure_and_emit(
        &self,
        envelope: &ActionEnvelope,
        ctx: ActionContext,
        now: DateTime<Utc>,
        emitter: &EvidenceEmitter,
        driver: Option<&RollbackDriver>,
    ) -> Result<PipelineState, RuntimeError> {
        let inject = driver.is_some_and(RollbackDriver::inject_verify_failure);
        if !inject {
            return self.step_verify_and_emit(envelope, ctx, now, emitter).await;
        }
        // EXECUTION_COMPLETED first (T-031 ordering preserved).
        let mut pre = ctx;
        emitter
            .emit_execution_completed(envelope, &mut pre, "ADAPTER_OK")
            .await?;
        // T15 — EXECUTING → VERIFYING.
        apply_transition(&mut pre, ActionLifecycleState::Verifying, now)?;
        // T18 — VERIFYING → FAILED.
        apply_transition(&mut pre, ActionLifecycleState::Failed, now)?;
        pre.error = Some(ExecutionFailureReason::AdapterRefused);
        // VERIFICATION_RESULT with passed=false.
        emitter
            .emit_verification_result(envelope, &mut pre, false)
            .await?;
        // Continue so step_rollback can engage (we are now in FAILED).
        Ok(PipelineState::Continue(pre))
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

/// Bundle of the boolean modifiers that feed the §3.2 dispatch-kind rule.
///
/// Authored as a single struct so [`compute_dispatch_kind`] can take one
/// typed argument rather than four positional `bool`s (constitutional
/// clippy rule: more than three bools in a signature is a reviewer trap).
#[allow(
    clippy::struct_excessive_bools,
    reason = "the §3.2 closed decision table has four independent boolean inputs; modelling each as a two-variant enum would inflate the surface without adding type safety"
)]
#[derive(Debug, Clone, Copy)]
pub struct DispatchKindInputs {
    /// `request.dry_run == SIMULATE` (§3.2 rule 1, topmost).
    pub is_simulate: bool,
    /// Subject's `is_ai` flag (§3.2 rule 2 — AI forces `ISOLATED_SANDBOX`).
    pub is_ai: bool,
    /// `request.risk.privileged` (§3.2 rule 3 — privileged forces
    /// `ISOLATED_SANDBOX`).
    pub risk_privileged: bool,
    /// `manifest.declared_stability == STABLE` (§3.2 rule 5 — only STABLE
    /// adapters are eligible for `IN_PROCESS_RPC`).
    pub manifest_stable: bool,
}

/// T-029 dispatch-kind decision (§3.2 closed table) — pure function so
/// `step_execute_with_engines` can decide without re-binding the manifest.
///
/// The decision is identical to
/// [`crate::dispatcher::ActionDispatcher::select_dispatch_kind`] but takes
/// the manifest's preferred kind + a [`DispatchKindInputs`] modifier
/// bundle directly so the adapter-registry trait surface (which returns
/// [`crate::runtime::AdapterHandle`] objects, not full manifests) can
/// drive it without down-casting.
#[must_use]
pub fn compute_dispatch_kind(
    manifest_kind: ActionDispatchKind,
    inputs: DispatchKindInputs,
) -> ActionDispatchKind {
    if inputs.is_simulate {
        return ActionDispatchKind::DryRun;
    }
    if inputs.is_ai || inputs.risk_privileged {
        return ActionDispatchKind::IsolatedSandbox;
    }
    if manifest_kind == ActionDispatchKind::InProcessRpc && inputs.manifest_stable {
        ActionDispatchKind::InProcessRpc
    } else {
        // SUBPROCESS_FORK terminus (§3.2 rule 4 + rule 6 fallback).
        ActionDispatchKind::SubprocessFork
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
