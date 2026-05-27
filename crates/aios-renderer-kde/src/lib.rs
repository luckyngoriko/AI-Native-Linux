//! L7 KDE Plasma renderer for AIOS (S7.4).
//!
//! Typed core skeleton: data model + invariants only. Wayland client,
//! Qt/QML compilation, and `KWin` scripting land in later tasks.

#![forbid(unsafe_code)]

pub mod compilation;
pub mod error;
pub mod kwin_script;
pub mod node_kind;
pub mod recovery_shell;
pub mod renderer;
pub mod service;
pub mod token_compile;
pub mod types;
pub mod visual_token;
pub mod wayland;
pub mod zone;

pub use compilation::{CompilationContext, CompilationRule, NodeSurfaceKind};
pub use error::KdeRendererError;
pub use kwin_script::{KwinScript, KwinScriptLoader, KwinScriptRecord, DEFAULT_ALLOWED_ROOT};
pub use node_kind::{NodeKind, NodeKindCompilationHint};
pub use recovery_shell::{
    escalate_to_degraded, ConstitutionalIconBundle, DegradedTrigger, IconEntry, RecoverySession,
    RecoveryShellGuard, SessionIsolationMarker,
};
pub use renderer::{
    AllocateSurfaceRequest, InMemoryKdeRenderer, KdeRenderer, RecoveryEntryReceipt, SurfaceFilter,
    SurfaceReleaseReceipt, TokenApplicationReceipt,
};
pub use token_compile::{
    compile_token, compile_token_with_ctx, IconLookupCtx, QEasingCurve, QFontWeight, QPaletteRole,
    QtRecipe, ShapeKind,
};
pub use types::{KdeSurfaceDescriptor, KdeSurfaceId, RecoveryShellMode, RendererMode};
pub use visual_token::{VisualToken, VisualTokenKind};
pub use wayland::{
    evaluate_surface_request, WaylandClient, WaylandInteractivity, WaylandProtocol,
    WaylandSurfaceGrant, WaylandSurfaceLayer, WaylandSurfaceRequest,
};
pub use zone::{CompositionZone, ZoneLayer};

/// Crate version marker used by closure-invariant tests in T-138.
pub const DEFAULT_CODE_VERSION: &str = "aios-renderer-kde/0.0.1-T127";
