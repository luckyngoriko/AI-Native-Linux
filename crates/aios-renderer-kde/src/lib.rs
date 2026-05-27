//! L7 KDE Plasma renderer for AIOS (S7.4).
//!
//! Typed core skeleton: data model + invariants only. Wayland client,
//! Qt/QML compilation, and `KWin` scripting land in later tasks.

#![forbid(unsafe_code)]

pub mod compilation;
pub mod error;
pub mod node_kind;
pub mod renderer;
pub mod types;
pub mod visual_token;
pub mod zone;

pub use compilation::{CompilationContext, CompilationRule, NodeSurfaceKind};
pub use error::KdeRendererError;
pub use node_kind::{NodeKind, NodeKindCompilationHint};
pub use renderer::{
    AllocateSurfaceRequest, InMemoryKdeRenderer, KdeRenderer, RecoveryEntryReceipt, SurfaceFilter,
    SurfaceReleaseReceipt, TokenApplicationReceipt,
};
pub use types::{KdeSurfaceDescriptor, KdeSurfaceId, RecoveryShellMode, RendererMode};
pub use visual_token::{VisualToken, VisualTokenKind};
pub use zone::{CompositionZone, ZoneLayer};

/// Crate version marker used by closure-invariant tests in T-138.
pub const DEFAULT_CODE_VERSION: &str = "aios-renderer-kde/0.0.1-T127";
