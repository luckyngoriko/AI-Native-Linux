//! [`CapabilityRuntime`] trait + [`RuntimeContext`] + [`InMemoryCapabilityRuntime`].
//!
//! This module defines the contract every Capability Runtime implementation
//! must satisfy. T-033 will add a gRPC server impl over this trait; T-029
//! will compose it with the dispatch queue; T-035 will compose the full
//! eight-step lifecycle.
//!
//! ## Layering
//!
//! ```text
//!   gRPC layer (T-033)
//!         |
//!         v
//!   CapabilityRuntime trait        <-- this module
//!         |
//!         v
//!   ActionLifecyclePipeline        <-- pipeline.rs
//!         |
//!         v
//!   apply_transition + TRANSITIONS <-- pipeline.rs (§4.2)
//! ```
//!
//! [`InMemoryCapabilityRuntime`] is the in-process harness used by tests today
//! and by T-028..T-035 as the substrate to attach the adapter registry, the
//! dispatch queue, the Policy Kernel handle, the evidence emitter, etc. The
//! T-027 baseline holds no external handles; every adapter / policy / evidence
//! reference is a `None` slot waiting for the relevant successor task.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;

use aios_action::{ActionEnvelope, ActionId};

use aios_policy::{Constraints, HydratedSubject, PolicyKernel, SubjectType};

use crate::adapter_registry::InMemoryAdapterRegistry;
use crate::context::ActionContext;
use crate::dispatch::ActionDispatchKind;
use crate::dispatch_queue::DispatchQueue;
use crate::error::RuntimeError;
use crate::pipeline::{fresh_context, ActionLifecyclePipeline};

// ---------------------------------------------------------------------------
// AdapterRegistry / AdapterHandle marker traits (T-028 wires real impls).
// ---------------------------------------------------------------------------

/// Stub marker trait for the adapter registry (S10.1 §10).
///
/// **T-028 lands the real `AdapterRegistry` struct** (manifest store +
/// signature verification + stability ladder enforcement). T-027 declares
/// the minimal trait surface so the runtime can hold an
/// `Option<Arc<dyn AdapterRegistry>>` slot without depending on T-028.
///
/// `lookup` is synchronous because the in-memory registry will be a
/// `HashMap` read; if a future production registry needs an async load
/// (e.g. read-through cache against AIOS-FS), the trait will be widened in
/// the task that adds it.
pub trait AdapterRegistry: Send + Sync + std::fmt::Debug {
    /// Resolve an action kind (e.g. `service.restart`) to a registered
    /// adapter. Returns `None` if no adapter declares the kind.
    fn lookup(&self, action_kind: &str) -> Option<Arc<dyn AdapterHandle>>;
}

/// Stub marker trait for an adapter dispatch handle (S10.1 §3.2 / §6.2).
///
/// **T-028 lands the real dispatch surface.** T-027 declares only the
/// dispatch-kind decision lookup so the pipeline's step 5 stub can be
/// trait-aware without depending on T-028's full implementation.
pub trait AdapterHandle: Send + Sync + std::fmt::Debug {
    /// The dispatch kind this adapter requires per §3.2's closed decision
    /// table. The runtime composes this with subject `is_ai` and risk flags
    /// to pick the actual [`ActionDispatchKind`].
    fn dispatch_kind(&self) -> ActionDispatchKind;
}

/// `NoOpAdapterRegistry` — empty registry; every `lookup` returns `None`.
///
/// Used in the T-027 baseline tests and as a placeholder when the runtime is
/// instantiated before T-028 lands the real registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpAdapterRegistry;

impl AdapterRegistry for NoOpAdapterRegistry {
    fn lookup(&self, _action_kind: &str) -> Option<Arc<dyn AdapterHandle>> {
        None
    }
}

/// `NoOpAdapterHandle` — a handle that nominally exists but reports
/// [`ActionDispatchKind::DryRun`]. Used in tests that need a handle without
/// touching the real adapter dispatch path.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpAdapterHandle;

impl AdapterHandle for NoOpAdapterHandle {
    fn dispatch_kind(&self) -> ActionDispatchKind {
        ActionDispatchKind::DryRun
    }
}

// ---------------------------------------------------------------------------
// RuntimeContext.
// ---------------------------------------------------------------------------

/// Per-evaluation context the Capability Runtime needs alongside the
/// [`ActionEnvelope`].
///
/// Constructed by the caller (today: the test harness; T-033: the gRPC
/// server) and passed by reference into [`CapabilityRuntime::submit_action`].
/// The runtime does not own or cache the context.
///
/// T-030 holds:
/// - `subject` — typed [`aios_policy::HydratedSubject`] (replaces the T-027
///   opaque `subject_id` string). Pipeline step 2 (policy evaluate) reads
///   this directly when a kernel is attached; the legacy `.subject_id()`
///   accessor preserves the T-028 / T-029 dispatcher's plain-string lookup
///   for the queue's `subject_canonical_id`.
/// - `bundle_version` / `code_version` — versioning anchors for the
///   determinism contract (S10.1 §6.1 step 2; the runtime re-evaluates the
///   policy decision if the active bundle version drifts mid-flight).
/// - `adapter_registry` — `None` in the baseline; `Some(...)` once T-028
///   wires real adapters.
/// - `policy_constraints` — interior-mutable slot populated by
///   [`crate::ActionLifecyclePipeline::step_policy_evaluate_with_kernel`]
///   after an `ALLOW` / `REQUIRE_APPROVAL` decision (Option A from the T-030
///   brief). Downstream T-029 dispatcher logic and T-035 verification reads
///   the projected [`aios_policy::Constraints`] from here without needing a
///   new `ActionContext` field (the context has `deny_unknown_fields` —
///   adding a field is a versioned spec change). The slot is wrapped in an
///   `Arc<Mutex<...>>` because `submit_action` takes `&RuntimeContext`; the
///   policy step is the single writer and the dispatcher is a read-after-
///   write reader, so contention is structurally absent.
#[derive(Debug, Clone)]
pub struct RuntimeContext {
    /// Typed hydrated subject (S2.3 §7). T-030 replaces the T-027
    /// `subject_id: String` field; the canonical id is still accessible
    /// via [`Self::subject_id`].
    pub subject: HydratedSubject,
    /// The policy bundle version this submission is being evaluated against
    /// (mirrors `aios_policy::PolicyContext::bundle_version`).
    pub bundle_version: String,
    /// The Rust binary build identifier — distinct from `bundle_version` so
    /// two decisions that disagree under the same bundle can be traced to a
    /// code drift rather than a policy drift.
    pub code_version: String,
    /// Optional adapter registry handle. `None` in the T-027 baseline; T-028
    /// composes a real registry through here.
    pub adapter_registry: Option<Arc<dyn AdapterRegistry>>,
    /// T-030 — Option A "Constraints projection" slot. Populated by
    /// [`crate::ActionLifecyclePipeline::step_policy_evaluate_with_kernel`]
    /// on `Decision::Allow` / `Decision::RequireApproval`; left `None` on
    /// DENY or when no policy kernel is attached. Read by downstream steps
    /// that need to honour `sandbox_profile_id`, `max_runtime_seconds`,
    /// etc. The `Arc<Mutex<...>>` wrap is the minimal interior-mutability
    /// shape that lets the pipeline write through a `&RuntimeContext`.
    pub policy_constraints: Arc<Mutex<Option<Constraints>>>,
}

/// Build the constitutional-default [`HydratedSubject`] for the T-027
/// 3-arg `RuntimeContext::new(subject_id, bundle, code)` constructor.
///
/// The defaults are deliberately conservative: `SubjectType::Human` (the
/// most common interactive caller), no group / capability membership,
/// `session_class = "INTERNAL"` (the operator's default per S1.3), and
/// `recovery_mode = false`. `is_ai` is derived from `subject_type` per the
/// `HydratedSubject` invariant (`subject_type ∈ {Agent, Application}` ⇒
/// `is_ai = true`); the helper sets `subject_type = Human` so `is_ai`
/// here is `false` by construction.
///
/// Callers that need a non-default hydrated subject (AI agents, recovery
/// mode, group membership) should use [`RuntimeContext::with_hydrated_subject`]
/// — the 3-arg ctor preserves the T-027 / T-028 / T-029 test contract.
fn default_hydrated_subject(subject_id: String) -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: subject_id,
        subject_type: SubjectType::Human,
        groups: Vec::new(),
        capabilities: Vec::new(),
        session_class: "INTERNAL".to_string(),
        recovery_mode: false,
        is_ai: false,
    }
}

impl RuntimeContext {
    /// Construct a fresh context with no adapter registry attached.
    ///
    /// **Backwards-compat 3-arg constructor** preserved across T-027..T-030.
    /// The opaque `subject_id` string is wrapped in a constitutional-default
    /// [`HydratedSubject`] (see [`default_hydrated_subject`]); callers that
    /// need a non-default subject (AI agent, recovery mode, group/capability
    /// membership) chain [`Self::with_hydrated_subject`].
    #[must_use]
    pub fn new(
        subject_id: impl Into<String>,
        bundle_version: impl Into<String>,
        code_version: impl Into<String>,
    ) -> Self {
        Self {
            subject: default_hydrated_subject(subject_id.into()),
            bundle_version: bundle_version.into(),
            code_version: code_version.into(),
            adapter_registry: None,
            policy_constraints: Arc::new(Mutex::new(None)),
        }
    }

    /// Construct a context from a fully-typed [`HydratedSubject`] (T-030).
    ///
    /// Use this when the caller already has the L4-hydrated subject record
    /// (production wiring, AI-agent submissions, recovery-mode operators).
    /// The simple 3-arg [`Self::new`] is shorthand that wraps a string id
    /// in the constitutional-default human subject.
    #[must_use]
    pub fn from_subject(
        subject: HydratedSubject,
        bundle_version: impl Into<String>,
        code_version: impl Into<String>,
    ) -> Self {
        Self {
            subject,
            bundle_version: bundle_version.into(),
            code_version: code_version.into(),
            adapter_registry: None,
            policy_constraints: Arc::new(Mutex::new(None)),
        }
    }

    /// Replace the hydrated subject — returns `self` for chaining.
    ///
    /// Mirrors the T-027 `with_adapter_registry` pattern: lets the 3-arg
    /// `new()` baseline keep working while production code paths swap in a
    /// typed [`HydratedSubject`] without touching the rest of the context.
    #[must_use]
    pub fn with_hydrated_subject(mut self, subject: HydratedSubject) -> Self {
        self.subject = subject;
        self
    }

    /// Canonical subject id accessor.
    ///
    /// Returns `&self.subject.canonical_subject_id`. Preserves the T-028 /
    /// T-029 dispatcher's plain-string lookup contract (queue enrolment is
    /// keyed by `subject_canonical_id`); the T-027 `subject_id: String`
    /// field is gone but every prior call-site can `.subject_id()` to get
    /// the identical `&str` view.
    #[must_use]
    pub fn subject_id(&self) -> &str {
        &self.subject.canonical_subject_id
    }

    /// Snapshot the projected policy constraints, if any.
    ///
    /// Returns `Some(...)` after a `Decision::Allow` /
    /// `Decision::RequireApproval` evaluation with a kernel attached; `None`
    /// before the policy step has run, on `Decision::Deny`, or when no
    /// kernel is configured. Cheap clone (constraints are 12 `Option<…>`
    /// fields).
    #[must_use]
    pub fn policy_constraints_snapshot(&self) -> Option<Constraints> {
        // Lock-poison recovery mirrors the override-boundary pattern from
        // M3: a poisoned mutex is a code defect (no panicking writers exist
        // in this crate) but reading the inner value is still meaningful.
        let guard = match self.policy_constraints.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.clone()
    }

    /// Internal writer for the projected policy constraints — used by
    /// [`crate::ActionLifecyclePipeline::step_policy_evaluate_with_kernel`].
    pub(crate) fn install_policy_constraints(&self, constraints: Option<Constraints>) {
        let mut guard = match self.policy_constraints.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard = constraints;
    }

    /// Attach an adapter registry handle. Returns `self` for chaining.
    ///
    /// **T-028 entry point.** Today no caller in this crate consumes the
    /// registry; the method is shipped so T-028 lands as a pure addition.
    #[must_use]
    pub fn with_adapter_registry(mut self, registry: Arc<dyn AdapterRegistry>) -> Self {
        self.adapter_registry = Some(registry);
        self
    }
}

// ---------------------------------------------------------------------------
// CapabilityRuntime trait.
// ---------------------------------------------------------------------------

/// The Capability Runtime contract — S10.1 §3 / §5.
///
/// Every implementation must drive the §3 eight-step pipeline through the
/// §4.2 transition table. T-027 ships the [`InMemoryCapabilityRuntime`]
/// harness; T-033 adds the gRPC server impl.
///
/// The trait is `Send + Sync` so impls can be shared across `tokio` tasks
/// (the production server holds one runtime behind `Arc<dyn CapabilityRuntime>`).
///
/// ## Single-writer contract
///
/// Per S10.1 §4.4 a single L3 instance owns the lifecycle of any one action.
/// The `submit_action` call is the single point of entry; concurrent
/// submissions for distinct action ids are safe (the in-memory harness uses
/// per-key writes inside an `RwLock<HashMap<..>>`), but concurrent
/// submissions for the **same** action id are a discipline violation.
#[async_trait]
pub trait CapabilityRuntime: Send + Sync {
    /// Submit a fresh action envelope to the runtime. Mints a fresh
    /// [`ActionId`], drives the envelope through the eight-step pipeline,
    /// and persists the final [`ActionContext`] for later
    /// [`Self::get_action_status`] lookups.
    ///
    /// Per §3 the runtime never silently falls through; every envelope
    /// produces a terminal lifecycle state (one of
    /// [`crate::ActionLifecycleState::is_terminal`] or
    /// [`crate::ActionLifecycleState::Failed`] /
    /// [`crate::ActionLifecycleState::PolicyDenied`]).
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] when a pipeline pre-condition fails (e.g.
    /// an illegal transition attempted by a step author — a code defect,
    /// not a runtime input failure). Input failures (validation, policy
    /// deny, etc.) are reflected in the returned `ActionContext.status`,
    /// not as an `Err`.
    async fn submit_action(
        &self,
        envelope: &ActionEnvelope,
        context: &RuntimeContext,
    ) -> Result<ActionContext, RuntimeError>;

    /// Read the current [`ActionContext`] for a known action id.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::ActionNotFound`] when the runtime has no
    /// record of the requested id.
    async fn get_action_status(&self, action_id: &ActionId) -> Result<ActionContext, RuntimeError>;
}

// ---------------------------------------------------------------------------
// InMemoryCapabilityRuntime.
// ---------------------------------------------------------------------------

/// In-process [`CapabilityRuntime`] impl backed by an in-memory action
/// context store and the [`ActionLifecyclePipeline`].
///
/// T-027 ships this with all per-step T-028..T-035 hooks deliberately
/// stubbed in [`ActionLifecyclePipeline`]. Successor tasks attach the
/// adapter registry (T-028), dispatch queue (T-029), policy kernel (T-030),
/// evidence emitter (T-031), rollback driver (T-032), gRPC adapter (T-033),
/// and approval orchestrator (T-034) through the same composition pattern
/// the M3 [`InMemoryPolicyKernel`] uses: add a field, expose a `with_*`
/// chainable ctor, and route the pipeline driver through the engine-aware
/// overload — never break the bare `new()` baseline that the T-027 tests
/// pin.
///
/// [`InMemoryPolicyKernel`]: ../../../aios_policy/struct.InMemoryPolicyKernel.html
#[derive(Clone)]
pub struct InMemoryCapabilityRuntime {
    /// The stateless eight-step driver. Pipeline state lives entirely on
    /// the per-action [`ActionContext`]; this field is held by value
    /// (zero-sized) so cloning the runtime is `O(1)`.
    pipeline: ActionLifecyclePipeline,
    /// Per-action context registry. `Arc<RwLock<...>>` so clones of the
    /// runtime share one canonical store across `tokio` worker tasks
    /// (matching the M3 [`InMemoryPolicyKernel`] composition discipline).
    contexts: Arc<RwLock<HashMap<ActionId, ActionContext>>>,
    /// Optional adapter registry handle. `None` keeps the T-027 success
    /// path intact (`step_execute` is a structural pass-through); `Some(...)`
    /// engages the T-028 lookup path: `step_execute` consults the registry
    /// and fails closed with `ExecutionFailureReason::DependencyUnready`
    /// when the envelope's `request.action` does not map to a registered
    /// adapter.
    ///
    /// **Rationale for `DependencyUnready` as the surrogate reason:**
    /// `ExecutionFailureReason` is closed (T-026) and does not declare an
    /// `AdapterUnknown` variant — adding one is a versioned spec change
    /// that T-028 is explicitly forbidden from making (§3.6 is owned by
    /// T-026). `DependencyUnready` is the closest spec-pinned variant:
    /// "a declared adapter dependency … was not in a ready state at
    /// dispatch". An adapter that is not registered is, by definition,
    /// not in a ready state. A future spec extension may introduce a
    /// dedicated `ADAPTER_NOT_REGISTERED` reason (the wire form
    /// `RuntimeErrorCode::UnknownAdapter` already exists at the RPC
    /// surface); T-029 / T-035 will reconcile the two.
    adapter_registry: Option<Arc<InMemoryAdapterRegistry>>,
    /// Optional dispatch queue handle. **T-029 entry point.** When attached,
    /// `step_queue` (§3.5 / §11) enrols the action through the queue and
    /// fails closed with `QUEUED → FAILED` (T14) +
    /// [`crate::ExecutionFailureReason::ResourceBudgetExceeded`] on
    /// [`crate::RuntimeError::QueueFull`] or
    /// [`crate::RuntimeError::RateLimited`]. When `None`, the T-027
    /// structural-pass-through behaviour is preserved so the §22 golden
    /// path tests and the T-028 baseline keep driving.
    dispatch_queue: Option<Arc<DispatchQueue>>,
    /// Optional Policy Kernel handle. **T-030 entry point.** When attached,
    /// pipeline step 2 (`step_policy_evaluate_with_kernel`, §3 / §4.2)
    /// calls [`PolicyKernel::evaluate_policy`] and maps the returned
    /// [`aios_policy::Decision`] onto the §4.2 transition table:
    ///
    /// - `Decision::Allow` → T4 (`POLICY_PENDING → APPROVED`).
    /// - `Decision::RequireApproval` → T5 (`POLICY_PENDING → APPROVAL_PENDING`).
    /// - `Decision::Deny` (no override path) → T6 (`POLICY_PENDING → POLICY_DENIED`)
    ///   with `error = PolicyDenied` (mapped onto
    ///   [`crate::ExecutionFailureReason::AdapterRefused`] today — the
    ///   §3.6 enum is closed at T-026 and no dedicated `POLICY_DENIED`
    ///   reason exists; T-031 will revisit the policy-denied evidence
    ///   shape once `aios-evidence` linkage lands).
    /// - `Decision::Deny` + active operator override → T7
    ///   (`POLICY_PENDING → OVERRIDE_PENDING`).
    ///
    /// When `None`, step 2 falls back to the T-027 stub (unconditional T4)
    /// so the T-027 / T-028 / T-029 test baselines stay green.
    policy_kernel: Option<Arc<dyn PolicyKernel>>,
    /// T-030 — defense-in-depth tripwire counter for the §17 AI
    /// self-approval prevention boundary.
    ///
    /// `aios_policy` already enforces §17 (a `Decision::Allow` for an AI
    /// subject without an explicit `approver_classes` override is
    /// converted into `Decision::Deny` inside the kernel). The runtime
    /// double-checks: if a kernel returns `Decision::Allow` for an AI
    /// subject and the bound `ApprovalRequirement.approver_classes` does
    /// not require a human, the runtime increments this counter (no
    /// behaviour change — the decision still drives the action through
    /// T4). Tests can `policy_double_check_warnings()` to assert the
    /// tripwire fires.
    policy_double_check_warnings: Arc<AtomicU64>,
}

impl Default for InMemoryCapabilityRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for InMemoryCapabilityRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryCapabilityRuntime")
            .field("pipeline", &self.pipeline)
            .field("adapter_registry", &self.adapter_registry.is_some())
            .field("dispatch_queue", &self.dispatch_queue.is_some())
            .field("policy_kernel", &self.policy_kernel.is_some())
            .field(
                "policy_double_check_warnings",
                &self.policy_double_check_warnings.load(Ordering::Acquire),
            )
            .finish_non_exhaustive()
    }
}

impl InMemoryCapabilityRuntime {
    /// Construct a fresh in-memory runtime with no adapters / policies /
    /// evidence emitters attached.
    ///
    /// This is the T-027 baseline used by the integration tests; T-028+
    /// will add `with_adapter_registry` / `with_policy_kernel` / etc.
    /// chainable ctors. The `pipeline` field is `Default` (a zero-sized
    /// stateless driver).
    #[must_use]
    pub fn new() -> Self {
        Self {
            pipeline: ActionLifecyclePipeline,
            contexts: Arc::new(RwLock::new(HashMap::new())),
            adapter_registry: None,
            dispatch_queue: None,
            policy_kernel: None,
            policy_double_check_warnings: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Attach an [`InMemoryAdapterRegistry`] to the runtime. Returns `self`
    /// for chaining.
    ///
    /// **T-028 entry point.** With a registry attached, `step_execute`
    /// (S10.1 §6.1 step 5) consults the registry for an adapter that
    /// declares `envelope.request.action` and fails closed with
    /// `QUEUED → FAILED` (T14) + [`ExecutionFailureReason::DependencyUnready`]
    /// when no live adapter is registered for the kind. Without a registry,
    /// the T-027 pass-through behaviour is preserved (no lookup, no
    /// fail-closed) so existing tests and the M4 §22 golden path remain
    /// drivable end-to-end against the stub steps.
    #[must_use]
    pub fn with_adapter_registry(mut self, registry: Arc<InMemoryAdapterRegistry>) -> Self {
        self.adapter_registry = Some(registry);
        self
    }

    /// Borrow the attached registry, if any. Used by tests and by T-029's
    /// dispatcher composition.
    #[must_use]
    pub const fn adapter_registry(&self) -> Option<&Arc<InMemoryAdapterRegistry>> {
        self.adapter_registry.as_ref()
    }

    /// Attach a [`DispatchQueue`] to the runtime. Returns `self` for
    /// chaining.
    ///
    /// **T-029 entry point.** With a queue attached, `step_queue`
    /// (S10.1 §3.5 / §11) enrols the action via
    /// [`DispatchQueue::enroll`] keyed by the envelope's
    /// `identity.subject_canonical_id`. On a
    /// [`crate::RuntimeError::QueueFull`] or
    /// [`crate::RuntimeError::RateLimited`] failure the pipeline records
    /// `error = ExecutionFailureReason::ResourceBudgetExceeded` and drives
    /// `APPROVED → ... → QUEUED → FAILED` (T12 + T14) — the action does
    /// not progress past queue admission. Without a queue, the T-027
    /// structural-pass-through behaviour is preserved.
    #[must_use]
    pub fn with_dispatch_queue(mut self, queue: Arc<DispatchQueue>) -> Self {
        self.dispatch_queue = Some(queue);
        self
    }

    /// Borrow the attached dispatch queue, if any. Used by tests to inspect
    /// per-class depths after a submission.
    #[must_use]
    pub const fn dispatch_queue(&self) -> Option<&Arc<DispatchQueue>> {
        self.dispatch_queue.as_ref()
    }

    /// Attach a [`PolicyKernel`] to the runtime. Returns `self` for
    /// chaining.
    ///
    /// **T-030 entry point.** With a kernel attached, pipeline step 2
    /// (`step_policy_evaluate_with_kernel`) consults
    /// [`PolicyKernel::evaluate_policy`] and maps the returned
    /// [`aios_policy::Decision`] onto the S10.1 §4.2 transition table
    /// (T4 / T5 / T6 / T7 — see the field-level docs on
    /// [`Self::policy_kernel`]). When no kernel is attached, the T-027
    /// stub behaviour (unconditional `POLICY_PENDING → APPROVED`) is
    /// preserved so the T-027 / T-028 / T-029 baselines stay drivable.
    #[must_use]
    pub fn with_policy_kernel(mut self, kernel: Arc<dyn PolicyKernel>) -> Self {
        self.policy_kernel = Some(kernel);
        self
    }

    /// Borrow the attached policy kernel, if any.
    #[must_use]
    pub const fn policy_kernel(&self) -> Option<&Arc<dyn PolicyKernel>> {
        self.policy_kernel.as_ref()
    }

    /// Snapshot the count of `Decision::Allow` results the §17 tripwire
    /// has flagged for an AI subject without a human approver class.
    ///
    /// The counter is informational only — the decision still drives the
    /// action through T4 (the source of truth for §17 enforcement is the
    /// `aios_policy` kernel, which converts the offending Allow into a
    /// Deny before returning). The tripwire is defense-in-depth: a
    /// non-zero count indicates the kernel returned an Allow that the
    /// runtime would have wanted to escalate, which warrants forensic
    /// review.
    #[must_use]
    pub fn policy_double_check_warnings(&self) -> u64 {
        self.policy_double_check_warnings.load(Ordering::Acquire)
    }

    /// Snapshot of the number of action contexts currently held. Useful
    /// for tests that assert no leaks across submissions.
    pub async fn len(&self) -> usize {
        self.contexts.read().await.len()
    }

    /// `true` iff the runtime has no recorded actions.
    pub async fn is_empty(&self) -> bool {
        self.contexts.read().await.is_empty()
    }
}

#[async_trait]
impl CapabilityRuntime for InMemoryCapabilityRuntime {
    async fn submit_action(
        &self,
        envelope: &ActionEnvelope,
        context: &RuntimeContext,
    ) -> Result<ActionContext, RuntimeError> {
        // T1 — (init) → CREATED: the envelope is accepted by the runtime.
        // `ActionContext::new` seeds the status as `CREATED`; the §4.2 table
        // does not list T1 (no `from` state) so no `apply_transition` call
        // is needed here.
        let now = Utc::now();
        let action_id = ActionId::new();
        let ctx = fresh_context(action_id.clone(), now);

        // Drive the eight-step pipeline. The pipeline is the single owner of
        // §4.2 transitions; this method only reads the result and persists
        // it. A successful run returns the final terminal `ActionContext`;
        // an `Err` indicates a step requested an illegal transition (code
        // defect — not a normal envelope-input failure).
        //
        // The registry handle is passed through so T-028's `step_execute`
        // can FAIL_CLOSE on an unknown adapter. When no registry is
        // attached (T-027 contract), the pipeline preserves its
        // structural-pass-through behaviour. T-030 additionally threads the
        // optional Policy Kernel and the `RuntimeContext` itself so step 2
        // can drive the §4.2 T4 / T5 / T6 / T7 mapping.
        let registry_ref = self.adapter_registry.as_deref();
        let queue_ref = self.dispatch_queue.as_deref();
        let kernel_ref = self.policy_kernel.as_deref();
        let final_ctx = self
            .pipeline
            .run_with_full_engines(
                envelope,
                ctx,
                now,
                registry_ref,
                queue_ref,
                kernel_ref,
                Some(context),
                Some(&self.policy_double_check_warnings),
            )
            .await?;

        // Persist for subsequent `get_action_status` reads.
        self.contexts
            .write()
            .await
            .insert(action_id, final_ctx.clone());

        Ok(final_ctx)
    }

    async fn get_action_status(&self, action_id: &ActionId) -> Result<ActionContext, RuntimeError> {
        self.contexts
            .read()
            .await
            .get(action_id)
            .cloned()
            .ok_or_else(|| RuntimeError::ActionNotFound(action_id.clone()))
    }
}
