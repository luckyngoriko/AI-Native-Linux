//! Cross-crate integration shims for aios-renderer-kde (T-137).
//!
//! [`cli_parity`] proves the KDE renderer compiles the same domain payloads
//! the CLI renderer already renders. [`apps_bridge`] connects the KDE
//! renderer to the `aios-apps` gRPC `AppsService` surface, compiling
//! `ListPackages` / `GetPackage` responses into [`KdeNodeTree`] shapes.

pub mod apps_bridge;
pub mod cli_parity;

pub use apps_bridge::AppsBridge;
pub use cli_parity::{
    apps_package_envelope_to_kde_node_tree, assert_parity_for_apps_domain, DomainTypeParity,
    DomainTypeParityEntry, KdeNodeTree, KdeNodeTreeEntry,
};
