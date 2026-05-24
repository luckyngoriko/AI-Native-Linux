//! gRPC `AiosFs` service surface (T-043, S1.3 operational API).
//!
//! This module hosts the tonic-generated server + client stubs, the wire ↔
//! Rust value-type conversions, and the [`AiosFsService`] adapter that mounts
//! the in-tree [`crate::InMemoryAiosFs`] under the generated
//! `aios_fs_server::AiosFs` transport trait.

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
    tonic::include_proto!("aios.fs.v1alpha1");
}

// Re-exports that downstream crates should use.
pub use proto::aios_fs_client::AiosFsClient;
pub use proto::aios_fs_server::{AiosFs as AiosFsGrpc, AiosFsServer as AiosFsGrpcServer};
pub use server::{build_router, serve, AiosFsService, DEFAULT_CODE_VERSION, DEFAULT_FS_ID};

/// Schema version string for the AIOS-FS service adapter.
pub const SCHEMA_VERSION: &str = "aios.fs.v1alpha1+0.1.0-T045";
