//! gRPC `PolicyKernel` service surface (T-023, S2.3 §20 / Appendix A).
//!
//! This module hosts the tonic-generated server + client stubs, the wire ↔
//! Rust value-type conversions, and the [`PolicyKernelService`] adapter that
//! mounts the in-tree [`crate::PolicyKernel`] trait under the generated
//! `policy_kernel_server::PolicyKernel` transport trait.
//!
//! ## Layout
//!
//! - [`proto`] — verbatim tonic-build output (`tonic::include_proto!`).
//! - [`conversions`] — Rust ↔ proto translations for [`crate::Decision`],
//!   [`crate::PolicyDecision`], [`crate::Constraints`],
//!   [`crate::ApprovalRequirement`], [`crate::HydratedSubject`], and the
//!   typed [`crate::PolicyError`] → `tonic::Status` mapping.
//! - [`server`] — [`PolicyKernelService`] adapter + bootstrap helpers
//!   (`build_router`, `serve`).
//!
//! ## RPC surface (Appendix A + §20)
//!
//! | RPC                 | Spec ref       | T-023 status |
//! | ------------------- | -------------- | ------------ |
//! | `EvaluatePolicy`    | §20, §3, §4    | full         |
//! | `SimulatePolicy`    | §14, §20       | full (sets `simulated=true`) |
//! | `LoadBundle`        | §12, §20       | stub — `LoadBundleResponse` returned as-is; `T-024` |
//! | `RollbackBundle`    | §12, §20       | stub — `Unimplemented`; T-025 |
//! | `ExplainDecision`   | §20            | stub — `Unimplemented`; T-025 |
//! | `GetPolicyEngineInfo` | §20          | full (`engine_id` + schema list + degraded flag) |
//!
//! The two real RPCs (`EvaluatePolicy`, `SimulatePolicy`) are the §22 acceptance
//! anchor — the rest are deliberately stubbed pending T-024 (cache) and T-025
//! (M3 closer: override boundary + bundle activation + acceptance fixtures).
//!
//! ## `envelope_proto` placeholder
//!
//! `EvaluatePolicyRequest.envelope_proto` is declared as `bytes` per spec but
//! until `aios-action` ships its own proto IDL (open task #26 in the .ai
//! tracker), the in-tree wire format is the canonical JSON serde representation
//! of `aios_action::ActionEnvelope`. The conversion module is the single touch
//! point when the swap-in lands.

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
    tonic::include_proto!("aios.policy.v1alpha1");
}

// Re-exports that downstream crates should use.
pub use proto::policy_kernel_client::PolicyKernelClient;
pub use proto::policy_kernel_server::{
    PolicyKernel as PolicyKernelGrpc, PolicyKernelServer as PolicyKernelGrpcServer,
};
pub use server::{build_router, serve, PolicyKernelService, DEFAULT_ENGINE_ID};

/// Schema version string mirroring the proto3 package name.
///
/// Returned by `GetPolicyEngineInfo` and accepted as the `schema_version`
/// field on every `EvaluatePolicyRequest`. Mismatches are rejected at the
/// boundary with `tonic::Code::FailedPrecondition` per the §18.2
/// failure-mode table (degraded engine returns `DENY` on the typed path;
/// on the wire path the schema is checked before we even decode the
/// envelope).
pub const SCHEMA_VERSION: &str = "aios.policy.v1alpha1";
