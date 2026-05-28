//! gRPC `NetworkPolicyService` + `DnsVpnService` surfaces (T-160, S8.1 + S8.4).
//!
//! Tonic-generated server/client stubs, wire ↔ Rust value-type conversions,
//! `NetworkPolicyServer` + `DnsVpnServer` adapters, and `build_network_router` /
//! `build_dnsvpn_router` bootstrap helpers.

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
    tonic::include_proto!("aios.network");
}

pub use proto::dns_vpn_service_client::DnsVpnServiceClient;
pub use proto::dns_vpn_service_server::{
    DnsVpnService as DnsVpnServiceGrpc, DnsVpnServiceServer as DnsVpnServiceGrpcServer,
};
pub use proto::network_policy_service_client::NetworkPolicyServiceClient;
pub use proto::network_policy_service_server::{
    NetworkPolicyService as NetworkPolicyServiceGrpc,
    NetworkPolicyServiceServer as NetworkPolicyServiceGrpcServer,
};

pub use conversions::network_error_to_status;
pub use server::{build_dnsvpn_router, build_network_router, DnsVpnServer, NetworkPolicyServer};

/// Schema version string mirroring the proto3 package name.
pub const SCHEMA_VERSION: &str = "aios.network";
