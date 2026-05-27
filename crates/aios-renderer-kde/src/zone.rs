//! Composition zone and Wayland layer model (S7.1 + S7.4 §3.1–§3.2).
//!
//! `CompositionZone` is the four-zone stack from S7.1 §6. `ZoneLayer` is the
//! wlr-layer-shell layer abstraction used by the KDE renderer to bind each zone
//! to a Wayland layer.

use serde::{Deserialize, Serialize};

/// The four composition zones from S7.1 §6 (closed set).
///
/// Every surface belongs to exactly one zone. The zone determines z-ordering
/// and which Wayland protocol primitive the KDE renderer uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompositionZone {
    /// AIOS trust-bearing chrome — security indicator, approval prompts,
    /// evidence links, recovery shield. Always topmost (INV-020).
    Chrome,
    /// Application and AIOS content surfaces.
    Content,
    /// Wallpaper and desktop decoration.
    Background,
    /// Recovery shell surface (separate `KWin` session per I5).
    Recovery,
}

/// wlr-layer-shell layer abstraction (S7.1 §4.3 KDE column + S7.4 §3.1).
///
/// Maps S7.1 composition semantics onto Wayland layer-shell protocol values.
/// `Bottom` is defined for future widget/dock use; not currently assigned to
/// any composition zone in the S7.4 mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ZoneLayer {
    /// `wlr-layer-shell` background layer — below all xdg-shell windows.
    Background,
    /// `wlr-layer-shell` bottom layer — between background and top.
    Bottom,
    /// `wlr-layer-shell` top layer — above content, below overlay.
    Top,
    /// `wlr-layer-shell` overlay layer — always topmost, survives fullscreen
    /// (this is how INV-020 is enforced at the compositor per S7.4 §I2).
    Overlay,
}

impl CompositionZone {
    /// Map a composition zone to its canonical wlr-layer-shell layer.
    ///
    /// - `Chrome` → `Overlay` (trust-bearing, always topmost)
    /// - `Recovery` → `Overlay` (recovery shell is also topmost)
    /// - `Content` → `Top` (above background, below chrome)
    /// - `Background` → `Background` (below everything)
    #[must_use]
    pub const fn allowed_layer(self) -> ZoneLayer {
        match self {
            Self::Chrome | Self::Recovery => ZoneLayer::Overlay,
            Self::Content => ZoneLayer::Top,
            Self::Background => ZoneLayer::Background,
        }
    }
}
