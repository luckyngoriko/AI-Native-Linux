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
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;

use aios_action::{ActionEnvelope, ActionId};

use crate::context::ActionContext;
use crate::dispatch::ActionDispatchKind;
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
/// T-027 holds:
/// - `subject_id` — opaque canonical subject id (the rev.2 envelope-level
///   form; T-030 will replace this with `aios_policy::HydratedSubject` when
///   the Policy Kernel integration lands).
/// - `bundle_version` / `code_version` — versioning anchors for the
///   determinism contract (S10.1 §6.1 step 2; the runtime re-evaluates the
///   policy decision if the active bundle version drifts mid-flight).
/// - `adapter_registry` — `None` in the baseline; `Some(...)` once T-028
///   wires real adapters.
#[derive(Debug, Clone)]
pub struct RuntimeContext {
    /// Opaque canonical subject id. Replaced by a typed `HydratedSubject`
    /// when T-030 wires the Policy Kernel.
    pub subject_id: String,
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
}

impl RuntimeContext {
    /// Construct a fresh context with no adapter registry attached.
    ///
    /// This is the T-027 baseline used by the integration tests; T-028
    /// adds a `with_adapter_registry` chainable ctor.
    #[must_use]
    pub fn new(
        subject_id: impl Into<String>,
        bundle_version: impl Into<String>,
        code_version: impl Into<String>,
    ) -> Self {
        Self {
            subject_id: subject_id.into(),
            bundle_version: bundle_version.into(),
            code_version: code_version.into(),
            adapter_registry: None,
        }
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
#[derive(Debug, Default, Clone)]
pub struct InMemoryCapabilityRuntime {
    /// The stateless eight-step driver. Pipeline state lives entirely on
    /// the per-action [`ActionContext`]; this field is held by value
    /// (zero-sized) so cloning the runtime is `O(1)`.
    pipeline: ActionLifecyclePipeline,
    /// Per-action context registry. `Arc<RwLock<...>>` so clones of the
    /// runtime share one canonical store across `tokio` worker tasks
    /// (matching the M3 [`InMemoryPolicyKernel`] composition discipline).
    contexts: Arc<RwLock<HashMap<ActionId, ActionContext>>>,
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
        Self::default()
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
        _context: &RuntimeContext,
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
        let final_ctx = self.pipeline.run(envelope, ctx, now)?;

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
