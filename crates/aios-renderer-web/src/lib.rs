//! L7 Web renderer for AIOS (S7.5).
//!
//! Typed core skeleton: data model + invariants only. HTTPS server,
//! gRPC-Web bridge, exposure FSM, and Next.js front-end land in later
//! tasks.

#![deny(unsafe_code)]

pub mod error;
pub mod exposure;
pub mod origin;
pub mod renderer;
pub mod types;

pub use error::WebRendererError;
pub use exposure::{ExposureLevel, ExposureLevelLabel};
pub use origin::{OriginScheme, OriginToken, ParsedOrigin};
pub use renderer::{
    AllocateWebSurfaceRequest, InMemoryWebRenderer, RecoveryEntryReceipt, TokenApplicationReceipt,
    WebRenderer, WebSurfaceFilter, WebSurfaceReleaseReceipt,
};
pub use types::{
    ChromeShadowRootMarker, RouteDescriptor, ShadowRootMode, WebRendererMode, WebSurfaceDescriptor,
    WebSurfaceId,
};

/// Re-exported from `aios-renderer-kde` so both L7 renderers share the
/// same closed `NodeKind` vocabulary (S7.2 §3 — 19 declared values).
pub use aios_renderer_kde::{NodeKind, NodeKindCompilationHint, VisualToken, VisualTokenKind};

/// Crate version marker used by closure-invariant tests in T-150.
pub const DEFAULT_CODE_VERSION: &str = "aios-renderer-web/0.0.1-T139";
