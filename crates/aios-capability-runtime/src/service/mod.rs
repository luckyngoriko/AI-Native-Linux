//! gRPC `CapabilityRuntime` service surface (T-033, S10.1 §5).
//!
//! This module hosts the tonic-generated server + client stubs, the wire ↔
//! Rust value-type conversions, and the [`CapabilityRuntimeService`] adapter
//! that mounts the in-tree [`crate::InMemoryCapabilityRuntime`] under the
//! generated `capability_runtime_server::CapabilityRuntime` transport trait.
//!
//! ## Layout
//!
//! - [`proto`] — verbatim tonic-build output (`tonic::include_proto!`).
//! - [`conversions`] — Rust ↔ proto translations for every value type the
//!   service surface carries: [`crate::ActionLifecycleState`],
//!   [`crate::ActionDispatchKind`], [`crate::AdapterIOMode`],
//!   [`crate::AdapterStability`], [`crate::QueueClass`],
//!   [`crate::ExecutionFailureReason`], [`crate::RollbackOutcome`],
//!   [`crate::RuntimeErrorCode`], [`crate::AdapterManifest`],
//!   [`crate::RegisteredAdapter`], [`crate::ActionContext`], and the typed
//!   [`crate::RuntimeError`] → `tonic::Status` mapping.
//! - [`server`] — [`CapabilityRuntimeService`] adapter + bootstrap helpers
//!   (`build_router`, `serve`).
//!
//! ## RPC surface (S10.1 §5.1)
//!
//! | RPC                          | Spec ref       | T-033 status |
//! | ---------------------------- | -------------- | ------------ |
//! | `ValidateAction`             | §5.2, §6.1 step 0 | full (envelope schema check) |
//! | `EvaluatePolicyForAction`    | §5.2, §6.1 step 2 | stub — `Unimplemented`; folded into `ExecuteAction` today |
//! | `RequestApprovalForAction`   | §5.2, §6.1     | stub — `Unimplemented`; T-034 |
//! | `ExecuteAction`              | §5.2, §6       | full (drives `submit_action` end-to-end) |
//! | `VerifyAction`               | §5.2, §7.1     | stub — `Unimplemented`; folded into `ExecuteAction` today |
//! | `RollbackAction`             | §5.2, §7.2     | stub — `Unimplemented`; folded into `ExecuteAction` today (T-032) |
//! | `GetActionStatus`            | §5.2           | full |
//! | `ListAdapters`               | §5.2, §10      | full |
//! | `GetAdapterCapabilities`     | §5.2           | full |
//! | `GetCapabilityRuntimeInfo`   | §5.2           | full |
//!
//! The four real RPCs (`ValidateAction`, `ExecuteAction`, `GetActionStatus`,
//! `ListAdapters`) exercise the full T-027..T-032 stack end-to-end; the five
//! stubbed RPCs are deliberately `Unimplemented` until T-034 splits the
//! validate / approve / execute / verify / rollback transitions into
//! independent RPC entry points.
//!
//! ## `envelope_proto` placeholder
//!
//! `ValidateActionRequest.envelope_proto` / `ExecuteActionRequest.envelope_proto`
//! are declared as `bytes` per spec but until `aios-action` ships its own
//! proto IDL (open task #26 in the .ai tracker), the in-tree wire format is
//! the canonical JSON serde representation of `aios_action::ActionEnvelope`.
//! The conversion module is the single touch point when the swap-in lands.

pub mod conversions;
pub mod server;

/// Tonic-generated server/client stubs + proto messages.
///
/// The `include_proto!` macro pulls the file emitted by `build.rs` (via
/// `tonic-build`). Downstream code should depend on the public re-exports
/// below rather than reaching into `proto::*` directly.
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    missing_docs,
    unused_qualifications,
    clippy::default_trait_access,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::empty_line_after_doc_comments,
    clippy::large_enum_variant,
    clippy::missing_const_for_fn,
    clippy::missing_errors_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::needless_borrow,
    clippy::option_option,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    clippy::unused_async,
    clippy::use_self,
    clippy::wildcard_imports
)]
pub mod proto {
    tonic::include_proto!("aios.runtime.v1alpha1");
}

// Re-exports that downstream crates should use.
pub use proto::capability_runtime_client::CapabilityRuntimeClient;
pub use proto::capability_runtime_server::{
    CapabilityRuntime as CapabilityRuntimeGrpc,
    CapabilityRuntimeServer as CapabilityRuntimeGrpcServer,
};
pub use server::{build_router, serve, CapabilityRuntimeService, DEFAULT_RUNTIME_ID};

/// Schema version string mirroring the proto3 package name.
///
/// Returned by `GetCapabilityRuntimeInfo` as the sole entry of the
/// `supported_schema_versions` list.
pub const SCHEMA_VERSION: &str = "aios.runtime.v1alpha1";

/// Default Rust crate version reported by `GetCapabilityRuntimeInfo`.
pub const DEFAULT_RUNTIME_VERSION: &str = "aios-capability-runtime/0.1.0-T035";
