//! gRPC `HardwareManagerService` + `GpuResourceService` + `FirmwareTrustService`
//! surfaces (T-172, S8.3 + S8.2 + S8.5).
//!
//! Tonic-generated server/client stubs, wire ↔ Rust value-type conversions,
//! server adapters, and `build_*_router` bootstrap helpers.

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
    tonic::include_proto!("aios.hardware");
}

pub use proto::firmware_trust_service_client::FirmwareTrustServiceClient;
pub use proto::firmware_trust_service_server::{
    FirmwareTrustService as FirmwareTrustServiceGrpc,
    FirmwareTrustServiceServer as FirmwareTrustServiceGrpcServer,
};
pub use proto::gpu_resource_service_client::GpuResourceServiceClient;
pub use proto::gpu_resource_service_server::{
    GpuResourceService as GpuResourceServiceGrpc,
    GpuResourceServiceServer as GpuResourceServiceGrpcServer,
};
pub use proto::hardware_manager_service_client::HardwareManagerServiceClient;
pub use proto::hardware_manager_service_server::{
    HardwareManagerService as HardwareManagerServiceGrpc,
    HardwareManagerServiceServer as HardwareManagerServiceGrpcServer,
};

pub use conversions::hardware_error_to_status;
pub use server::{
    build_firmware_router, build_gpu_router, build_hardware_router, FirmwareTrustServer,
    GpuResourceServer, HardwareManagerServer,
};

/// Schema version string mirroring the proto3 package name.
pub const SCHEMA_VERSION: &str = "aios.hardware";
