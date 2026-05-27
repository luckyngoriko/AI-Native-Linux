//! Core renderer types: surface identity, renderer mode, recovery shell mode,
//! and the KDE surface descriptor (S7.4 §3 + §7).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::KdeRendererError;
use crate::zone::{CompositionZone, ZoneLayer};

/// KDE renderer surface identity — Ulid-backed opaque string.
///
/// Each surface created by or registered with the KDE renderer receives a
/// unique `KdeSurfaceId`. The id is stable across re-renders.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KdeSurfaceId(pub String);

impl KdeSurfaceId {
    /// Generate a new unique surface identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(ulid::Ulid::new().to_string())
    }
}

impl Default for KdeSurfaceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for KdeSurfaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Renderer operational mode (S7.4 §I7 + Appendix A `KdeRendererMode`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RendererMode {
    /// Normal operation — Wayland + wgpu + full theme capabilities.
    Normal,
    /// Degraded text-only fallback (per §I7). Carries the reason class for
    /// evidence emission (`kwin_unreachable`, `wgpu_init_failed`, etc.).
    Degraded(String),
    /// Recovery shell session — separate `KWin` session, `AIOS_RECOVERY` theme.
    Recovery,
}

/// Recovery shell session marker (INV I5).
///
/// In a recovery session, the renderer runs under a separate `KWin` process and
/// user; `APP_SURFACE` / `STREAM_SURFACE` / `SURFACE_EMBED` are rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RecoveryShellMode {
    /// Normal (non-recovery) rendering session.
    #[default]
    NotRecovery,
    /// Recovery shell is active — separate `KWin` session, `AIOS_RECOVERY` theme.
    RecoveryActive,
}

/// KDE surface descriptor binding a surface identity to a composition zone,
/// layer, renderer mode, and claimant (S7.4 §3.2 + §7.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdeSurfaceDescriptor {
    /// Unique surface identifier.
    pub id: KdeSurfaceId,
    /// Composition zone this surface occupies.
    pub zone: CompositionZone,
    /// wlr-layer-shell layer derived from the zone.
    pub layer: ZoneLayer,
    /// Current renderer operational mode.
    pub mode: RendererMode,
    /// When the surface was created (wall-clock).
    pub created_at: DateTime<Utc>,
    /// Canonical subject id of the client that claimed this surface.
    pub claimed_by: String,
}

impl KdeSurfaceDescriptor {
    /// Create a new surface descriptor.
    ///
    /// The layer is derived from the zone via `CompositionZone::allowed_layer()`.
    /// INV I4 is enforced here: if the zone is `Chrome` and `claimed_by` is not
    /// `"aios-chrome"`, returns `KdeRendererError::OverlayLayerForbidden`.
    ///
    /// # Errors
    ///
    /// Returns `KdeRendererError::OverlayLayerForbidden` when a non-chrome
    /// client attempts to claim the chrome (`Overlay`) layer.
    pub fn new(
        zone: CompositionZone,
        claimed_by: impl Into<String>,
    ) -> Result<Self, KdeRendererError> {
        let claimed_by: String = claimed_by.into();
        if zone == CompositionZone::Chrome && claimed_by != "aios-chrome" {
            return Err(KdeRendererError::OverlayLayerForbidden {
                client_id: claimed_by,
            });
        }
        Ok(Self {
            id: KdeSurfaceId::new(),
            layer: zone.allowed_layer(),
            zone,
            mode: RendererMode::Normal,
            created_at: Utc::now(),
            claimed_by,
        })
    }
}
