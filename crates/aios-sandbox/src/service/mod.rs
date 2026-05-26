//! gRPC `SandboxService` surface (T-110, S3.2).

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
    tonic::include_proto!("aios.sandbox.v1alpha1");
}

pub use proto::sandbox_service_client::SandboxServiceClient;
pub use proto::sandbox_service_server::{
    SandboxService as SandboxServiceGrpc, SandboxServiceServer as SandboxServiceGrpcServer,
};
pub use server::SandboxServiceImpl;

/// Schema version string for the AIOS `SandboxService` adapter.
pub const SCHEMA_VERSION: &str = "aios.sandbox.v1alpha1+T110";
