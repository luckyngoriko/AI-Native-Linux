//! gRPC `KdeRendererService` surface (T-134, S7.4 §10).
//!
//! This module hosts the tonic-generated server + client stubs, the wire ↔
//! Rust value-type conversions, and the [`KdeRendererServer`] adapter that
//! mounts the KDE renderer, Wayland client, and `KWin` script loader behind
//! the generated transport trait.

pub mod conversions;
pub mod server;

/// Tonic-generated server/client stubs + proto messages.
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
    tonic::include_proto!("aios.renderer.kde");
}

pub use proto::kde_renderer_service_client::KdeRendererServiceClient;
pub use proto::kde_renderer_service_server::{
    KdeRendererService as KdeRendererServiceGrpc,
    KdeRendererServiceServer as KdeRendererServiceGrpcServer,
};
pub use server::{build_router, KdeRendererServer};

pub use conversions::kde_error_to_status;

/// Schema version string mirroring the proto3 package name.
pub const SCHEMA_VERSION: &str = "aios.renderer.kde";
