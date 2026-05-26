//! gRPC `AppsService` surface (T-122, S12.x).
//!
//! This module hosts the tonic-generated server + client stubs, the wire ↔
//! Rust value-type conversions, and the [`AppsServer`] adapter that mounts
//! the five backing drivers behind the generated `apps_service_server::AppsService`
//! transport trait.
//!
//! ## Layout
//!
//! - [`proto`] — tonic-build output (`tonic::include_proto!`).
//! - [`conversions`] — Rust ↔ proto translations for every value type the
//!   service surface carries.
//! - [`server`] — [`AppsServer`] adapter + bootstrap helpers.

pub mod conversions;
pub mod server;

/// Tonic-generated server/client stubs + proto messages.
///
/// The `include_proto!` macro pulls the file emitted by `build.rs` (via
/// `tonic-build`).
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
    tonic::include_proto!("aios.apps.v1alpha1");
}

// Re-exports
pub use proto::apps_service_client::AppsServiceClient;
pub use proto::apps_service_server::{
    AppsService as AppsServiceGrpc, AppsServiceServer as AppsServiceGrpcServer,
};
pub use server::{build_router, serve, AppsServer};

/// Schema version string mirroring the proto3 package name.
pub const SCHEMA_VERSION: &str = "aios.apps.v1alpha1";
