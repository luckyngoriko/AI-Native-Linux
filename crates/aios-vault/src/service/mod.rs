//! gRPC `VaultBroker` service surface (T-052, S5.2/S5.1/S5.4).
//!
//! This module hosts the tonic-generated server + client stubs, Rust ↔ proto
//! conversions, and the [`VaultBrokerService`] adapter over the in-memory
//! vault, override, identity, audit, and lifecycle components.

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
    tonic::include_proto!("aios.vault.v1alpha1");
}

pub use proto::vault_broker_client::VaultBrokerClient;
pub use proto::vault_broker_server::{
    VaultBroker as VaultBrokerGrpc, VaultBrokerServer as VaultBrokerGrpcServer,
};
pub use server::{build_router, serve, VaultBrokerService, DEFAULT_CODE_VERSION, DEFAULT_VAULT_ID};

/// Schema version string for the AIOS `VaultBroker` service adapter.
pub const SCHEMA_VERSION: &str = "aios.vault.v1alpha1+T052";
