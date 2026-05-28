//! gRPC `IntegrationService` surface (T-183, S11.4).
//!
//! Tonic-generated server/client stubs, wire ↔ Rust value-type conversions,
//! `IntegrationServer` adapter, and `build_router` bootstrap helper.

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
    tonic::include_proto!("aios.integration");
}

pub use proto::integration_service_client::IntegrationServiceClient;
pub use proto::integration_service_server::{
    IntegrationService as IntegrationServiceGrpc,
    IntegrationServiceServer as IntegrationServiceGrpcServer,
};

pub use conversions::integration_error_to_status;
pub use server::{build_router, IntegrationServer};

/// Schema version string mirroring the proto3 package name.
pub const SCHEMA_VERSION: &str = "aios.integration";
