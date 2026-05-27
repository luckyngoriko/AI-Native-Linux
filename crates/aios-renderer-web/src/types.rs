//! Core Web renderer types: surface identity, renderer mode, surface
//! descriptor, route descriptor, and the Chrome shadow root marker (S7.5 §3
//! + §I2 + §I7).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::WebRendererError;
use crate::origin::ParsedOrigin;
use aios_renderer_kde::NodeKind;

/// Web renderer surface identity — Ulid-backed opaque string.
///
/// Each surface created by or registered with the Web renderer receives a
/// unique `WebSurfaceId`. The id is stable across re-renders.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WebSurfaceId(pub String);

impl WebSurfaceId {
    /// Generate a new unique surface identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(ulid::Ulid::new().to_string())
    }
}

impl Default for WebSurfaceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for WebSurfaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Web renderer operational mode (S7.5 §3).
///
/// Mirrors `KdeRendererMode` from S7.4 but with web-specific semantics: in
/// `Recovery` mode, the renderer serves a separate recovery shell SPA from
/// `/aios/recovery`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebRendererMode {
    /// Normal operation — full HTTPS + gRPC-Web + WebGPU capabilities.
    Normal,
    /// Recovery shell session — separate SPA, `AIOS_RECOVERY` theme, limited
    /// surface set.
    Recovery,
    /// Degraded operation. Carries the reason class for evidence emission
    /// (e.g. `cert_expired`, `webgpu_init_failed`, `chrome_shadow_integrity`).
    Degraded(String),
}

/// Web surface descriptor binding a surface identity to an origin, node kind,
/// claimant, mode, and creation timestamp (S7.5 §3.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSurfaceDescriptor {
    /// Unique surface identifier.
    pub id: WebSurfaceId,
    /// The parsed origin this surface is served under.
    pub origin: ParsedOrigin,
    /// The node kind of the surface (e.g. `SurfaceEmbed`, `Container`).
    pub node_kind: NodeKind,
    /// Canonical subject id of the client that claimed this surface.
    pub claimed_by: String,
    /// Current renderer operational mode.
    pub mode: WebRendererMode,
    /// When the surface was created (wall-clock).
    pub created_at: DateTime<Utc>,
}

impl WebSurfaceDescriptor {
    /// Create a new web surface descriptor.
    ///
    /// Defaults to `WebRendererMode::Normal`. INV I11 is enforced: if the
    /// origin is a recovery scheme, the mode must be `Recovery`.
    ///
    /// # Errors
    ///
    /// Returns `WebRendererError::Internal` if a recovery-scheme origin is
    /// presented with a non-Recovery mode (INV I11).
    pub fn new(
        origin: ParsedOrigin,
        node_kind: NodeKind,
        claimed_by: impl Into<String>,
    ) -> Result<Self, WebRendererError> {
        let mode = WebRendererMode::Normal;
        let claimed_by: String = claimed_by.into();
        Ok(Self {
            id: WebSurfaceId::new(),
            origin,
            node_kind,
            claimed_by,
            mode,
            created_at: Utc::now(),
        })
    }
}

/// Closed route descriptor for the future Next.js route table (S7.5 §3.3).
///
/// Each route entry defines a path, whether it requires authentication, and
/// whether it is served in recovery mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteDescriptor {
    /// The HTTP path this route handles (e.g. `/`, `/api/action`, `/recovery`).
    pub path: String,
    /// Whether this route requires an authenticated session.
    pub requires_auth: bool,
    /// Whether this route is served during recovery mode.
    pub served_in_recovery: bool,
}

/// Chrome shadow root mode (S7.5 §I7).
///
/// Only `Closed` is permitted — the shadow root must not be accessible from
/// JavaScript outside the AIOS chrome service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShadowRootMode {
    /// Closed shadow root — JavaScript outside the AIOS chrome context cannot
    /// access the shadow root via `Element.shadowRoot` (returns `null`).
    Closed,
}

/// Chrome shadow root marker enforcing constitutional invariants (S7.5 §I2 +
/// §I7).
///
/// The AIOS chrome overlay must render inside a closed shadow root with a
/// fixed z-index of 9999 to guarantee that no application or third-party
/// surface can paint above it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChromeShadowRootMarker {
    /// Fixed z-index — always 9999 (INV I2).
    pub z_index: u32,
    /// Shadow root mode — always `Closed` (INV I7).
    pub mode: ShadowRootMode,
    /// Integrity hash of the chrome shadow root bundle, verified at load time
    /// (INV I10).
    pub integrity_hash: String,
}
