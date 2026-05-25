//! gRPC `RecoveryService` service surface (T-079, S9.1/S9.2/S9.3).

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
    tonic::include_proto!("aios.recovery.v1alpha1");
}

pub use proto::recovery_service_client::RecoveryServiceClient;
pub use proto::recovery_service_server::{
    RecoveryService as RecoveryServiceGrpc, RecoveryServiceServer as RecoveryServiceGrpcServer,
};
pub use server::{build_router, serve, RecoveryServiceImpl};

/// Schema version string for the AIOS `RecoveryService` adapter.
pub const SCHEMA_VERSION: &str = "aios.recovery.v1alpha1+T079";
