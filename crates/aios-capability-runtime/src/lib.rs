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

pub mod adapter_manifest;
pub mod context;
pub mod dispatch;
pub mod error;
pub mod failure;
pub mod pipeline;
pub mod runtime;
pub mod status;

pub use adapter_manifest::AdapterManifest;
pub use context::ActionContext;
pub use dispatch::{ActionDispatchKind, AdapterIOMode, AdapterStability, QueueClass};
pub use error::RuntimeError;
pub use failure::{ExecutionFailureReason, RollbackOutcome, RuntimeErrorCode};
pub use pipeline::{
    apply_transition, fresh_context, ActionLifecyclePipeline, PipelineState, TRANSITIONS,
};
pub use runtime::{
    AdapterHandle, AdapterRegistry, CapabilityRuntime, InMemoryCapabilityRuntime,
    NoOpAdapterHandle, NoOpAdapterRegistry, RuntimeContext,
};
pub use status::ActionLifecycleState;
