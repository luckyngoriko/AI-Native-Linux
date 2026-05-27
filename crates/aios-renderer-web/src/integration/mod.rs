//! Cross-crate integration shims for aios-renderer-web (T-149).
//!
//! [`renderer_parity`] proves the Web renderer compiles the same domain
//! payloads the CLI and KDE renderers already handle. [`apps_bridge`] connects
//! the Web renderer to the `aios-apps` gRPC `AppsService` surface, compiling
//! `ListPackages` / `GetPackage` responses into [`WebRenderTree`] shapes.

pub mod apps_bridge;
pub mod renderer_parity;

pub use apps_bridge::WebAppsBridge;
pub use renderer_parity::{
    apps_package_envelope_to_web_render_tree, assert_three_way_parity_for_apps_domain,
    ThreeWayParity, ThreeWayParityEntry, WebRenderTree, WebRenderTreeEntry,
};
