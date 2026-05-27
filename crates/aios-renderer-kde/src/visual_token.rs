//! Visual token vocabulary (S7.3 closed reference).
//!
//! `VisualTokenKind` is the closed taxonomy of semantic token families that
//! every renderer reads. Concrete values (hex codes, typeface names, icon
//! glyphs) are stage-3 artifacts and are not defined here.

use serde::{Deserialize, Serialize};

/// Closed visual token family taxonomy (S7.3 §4).
///
/// These are the semantic categories that themes populate with concrete values.
/// The KDE renderer compiles each family into Qt primitives per the S7.4 §5
/// token compilation table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VisualTokenKind {
    /// Color tokens — `QPalette` entries and QML property overrides.
    Color,
    /// Font/typography tokens — `QFont` instances registered in `QQuickStyle`.
    Font,
    /// Spacing tokens — pixel values for `Layout.margins` / `Layout.spacing`.
    Spacing,
    /// Motion tokens — `QPropertyAnimation` duration and easing curves.
    Motion,
    /// Icon tokens — loaded via `KIconLoader` from the AIOS theme bundle path.
    Icon,
    /// Shape tokens — border radius, corner style, clipping masks.
    Shape,
    /// Elevation tokens — shadow depth, z-height, surface layering.
    Elevation,
}

/// A semantic visual token binding a token family, an identifier, and a
/// canonical value string (S7.3 §2 + §4).
///
/// Concrete value interpretation depends on `kind` and the active theme.
/// For example, a `Color` token's `canonical_value` is a hex string in the
/// theme; a `Font` token's is a family name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualToken {
    /// Unique token identifier within the active theme (e.g. `COLOR_ACTION_AI`).
    pub id: String,
    /// The token family this token belongs to.
    pub kind: VisualTokenKind,
    /// Theme-supplied canonical value (hex code, family name, pixel count, etc.).
    pub canonical_value: String,
}
