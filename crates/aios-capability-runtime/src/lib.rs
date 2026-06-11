//! `aios-capability-runtime` — core types for the AIOS Capability Runtime
//! (S10.1, schema `aios.runtime.v1alpha1`).
//!
//! This crate implements the **wire-format-agnostic core data model** for the
//! L3 Capability Runtime defined in
//! `002.AI-OS.NET--SPECREV.2/L3_AIOS_SGR_Service_Graph_Runtime/03_capability_runtime_grpc.md`.
//! It is the L3 sibling of `aios-policy` (L4) and consumes `aios-action` (S0.1).
//!
//! ## Scope of T-026 (M4 opener — types-only skeleton)
//!
//! - [`ActionLifecycleState`] — closed 14-state FSM per S10.1 §3.1.
//! - [`ActionDispatchKind`] — closed 4-variant dispatch enum per §3.2.
//! - [`AdapterIOMode`] — closed 2-variant adapter IO mode per §3.3.
//! - [`AdapterStability`] — closed 5-variant stability ladder per §3.4.
//! - [`QueueClass`] — closed 4-variant queue class per §3.5.
//! - [`ExecutionFailureReason`] — closed 12-variant execution failure enum per §3.6.
//! - [`RollbackOutcome`] — closed 4-variant rollback enum per §3.7.
//! - [`RuntimeErrorCode`] — closed 20-variant RPC error enum per §3.8.
//! - [`AdapterManifest`] — closed adapter manifest record per §10.1.
//! - [`ActionContext`] — internal per-action runtime context.
//! - [`RuntimeError`] — typed error taxonomy for the orchestration RPCs.
//!
//! Trait surface (`CapabilityRuntime`), adapter registry, dispatch queue,
//! policy / evidence integration, rollback FSM driver, approval orchestration,
//! and the gRPC service shell are **explicitly out of scope** for T-026 and
//! are queued for T-027..T-035.
//!
//! ## Constitutional invariants enforced here
//!
//! - **No `unsafe`, no `panic!`, no `unwrap`/`expect`, no `todo!`/`unimplemented!`** —
//!   workspace lints forbid them; every fallible path returns a typed `Result`.
//! - **`ActionLifecycleState::COUNT == 14`** — `EnumCount` provides the
//!   compile-time anchor; the round-trip tests assert the count.
//! - **Terminal states are terminal** — [`ActionLifecycleState::is_terminal`]
//!   returns `true` for the four spec-pinned strict terminals
//!   (`SUCCEEDED`, `ROLLED_BACK`, `ROLLBACK_FAILED`, `OVERRIDE_DENIED`) per
//!   the §4.2 forbidden-transition table.
//! - **Wire form is `SCREAMING_SNAKE_CASE`** for every closed enum, matching
//!   the proto IDL declared in §5.1 / §10.1.

#![forbid(unsafe_code)]

pub mod adapter_handle;
pub mod adapter_manifest;
pub mod adapter_registry;
/// T-034 — Approval orchestration types (S10.1 §6 ↔ S5.3).
pub mod approval;
/// T-034 — Approval binding sink (S10.1 ↔ S5.3 Approval Mechanics).
pub mod approval_sink;
pub mod context;
pub mod dispatch;
pub mod dispatch_queue;
pub mod dispatcher;
pub mod error;
pub mod evidence_emit;
pub mod evidence_payloads;
pub mod failure;
pub mod pipeline;
pub mod rollback;
pub mod rollback_strategy;
pub mod runtime;
/// T-033 — gRPC `CapabilityRuntime` service surface
/// (`aios.runtime.v1alpha1`, S10.1 §5).
pub mod service;
pub mod status;
/// OS-RESEARCH: Plan 9/Inferno-inspired per-capsule namespace model.
pub mod capsule_namespace;
/// OS-RESEARCH: seL4-inspired capability token model with formal invariants.
pub mod sel4_cap_model;
/// OS-RESEARCH: QNX/Plan 9-inspired transparent distributed IPC model.
pub mod transparent_ipc;
/// OS-RESEARCH: BeOS/QNX-inspired adaptive partition scheduler.
pub mod scheduler;

pub use adapter_handle::RealAdapterHandle;
pub use adapter_manifest::AdapterManifest;
pub use adapter_registry::{
    canonical_signed_manifest_bytes, encode_hex_signature, InMemoryAdapterRegistry,
    RegisteredAdapter,
};
pub use approval::{ApprovalBinding, ApprovalBindingState, ApprovalRequest};
pub use approval_sink::{ApprovalBindingSink, InMemoryApprovalSink};
pub use context::ActionContext;
pub use dispatch::{ActionDispatchKind, AdapterIOMode, AdapterStability, QueueClass};
pub use dispatch_queue::{
    DispatchQueue, TokenBucket, AGENT_PROPOSAL_CAP_DEN, AGENT_PROPOSAL_CAP_NUM,
    DEFAULT_BURST_CAPACITY, DEFAULT_REFILL_PER_SECOND, DEFAULT_TOTAL_CAPACITY,
};
pub use dispatcher::{ActionDispatcher, AI_INTERACTIVE_DOWNGRADE_MARKER};
pub use error::RuntimeError;
pub use evidence_emit::{
    EvidenceEmitter, EvidenceSink, InMemoryEvidenceSink, CAPABILITY_RUNTIME_SUBJECT,
};
pub use evidence_payloads::{
    ActionQueuedPayload, ActionReceivedPayload, AiInteractiveQueueDowngradePayload,
    ExecutionCompletedPayload, ExecutionStartedPayload, PolicyDecisionPayload,
    RollbackCompletedPayload, RoutingDecisionPayload, VerificationResultPayload,
};
pub use failure::{ExecutionFailureReason, RollbackOutcome, RuntimeErrorCode};
pub use pipeline::{
    apply_transition, compute_dispatch_kind, fresh_context, ActionLifecyclePipeline,
    DispatchKindInputs, PipelineState, TRANSITIONS,
};
pub use rollback::RollbackDriver;
pub use rollback_strategy::{RollbackFailureMode, RollbackStrategy};
pub use runtime::{
    AdapterHandle, AdapterRegistry, CapabilityRuntime, InMemoryCapabilityRuntime,
    NoOpAdapterHandle, NoOpAdapterRegistry, RuntimeCognitiveProvenance, RuntimeContext,
    RuntimeRecoveryHook, RuntimeSandboxComposer, SandboxProfileSummary,
};
pub use status::ActionLifecycleState;
// OS-RESEARCH re-exports
pub use capsule_namespace::{
    next_capsule_id, CapsuleId, CapsuleNamespace, MountFlag, NamespaceBinding,
    NamespacePath, NamespaceRegistry,
};
pub use sel4_cap_model::{CapRight, CapRights, CapToken, CapTokenId, CapTokenTree};
pub use transparent_ipc::{
    next_msg_id, CapsuleAddr, CapsuleMessage, MessageRouter, MsgId, MsgType, PendingRequest,
};
pub use scheduler::{
    AdaptivePartition, CapsulePriority, CapsuleSchedulingEntity, DecisionReason, PartitionScheduler,
    PriorityBand, SchedulingDecision,
};
