//! Wayland surface model and INV I4 wlr-layer-shell enforcement (S7.4 §3.1).
//!
//! Closed set of 7 permitted Wayland protocols, the surface request/grant
//! types, `evaluate_surface_request()` with zone→layer mapping and INV I4
//! enforcement, and an in-memory `WaylandClient` grant tracker.
//!
//! No actual Wayland socket I/O — pure type-level model ready to bind against
//! `wayland-client` in a later milestone.

use std::collections::HashMap;

use tokio::sync::RwLock;

use crate::error::KdeRendererError;
use crate::node_kind::NodeKind;
use crate::types::KdeSurfaceId;
use crate::zone::CompositionZone;

/// Closed set of Wayland protocols the renderer is permitted to speak (S7.4 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WaylandProtocol {
    /// Core compositor surface management.
    WlCompositor,
    /// xdg-shell desktop-style surface protocol.
    XdgShell,
    /// wlr-layer-shell v1 — reserved for chrome and recovery zones.
    WlrLayerShellV1,
    /// wp-viewporter for crop-and-scale.
    WpViewporter,
    /// Linux dmabuf protocol for zero-copy GPU buffer sharing.
    ZwpLinuxDmabufV1,
    /// xdg-decoration for server-side window decorations.
    XdgDecorationV1,
    /// wlr-idle-inhibit v1.
    IdleInhibitV1,
}

impl WaylandProtocol {
    /// Number of declared `WaylandProtocol` variants (S7.4 §3.1 closed set).
    pub const LEN: usize = 7;

    /// All 7 declared variants in declaration order.
    pub const ALL: &'static [Self] = &[
        Self::WlCompositor,
        Self::XdgShell,
        Self::WlrLayerShellV1,
        Self::WpViewporter,
        Self::ZwpLinuxDmabufV1,
        Self::XdgDecorationV1,
        Self::IdleInhibitV1,
    ];
}

/// Surface request received from the cognitive core or policy kernel (S7.4 §3.1).
///
/// Carries the requested protocol, a layer namespace, the claimed client
/// identity, the composition zone, and the node kind.
#[derive(Debug, Clone)]
pub struct WaylandSurfaceRequest {
    /// Requested Wayland protocol.
    pub protocol: WaylandProtocol,
    /// Layer namespace (e.g. "aios-shell", "aios-chrome").
    pub layer_namespace: String,
    /// Canonical subject ID of the requesting client.
    pub claimed_by: String,
    /// Composition zone the surface targets.
    pub zone: CompositionZone,
    /// Kind of UI node being surfaced.
    pub node_kind: NodeKind,
}

/// wlr-layer-shell layer assignment (S7.4 §3.1).
///
/// Mirror of the four wlr-layer-shell layers. Distinct from `ZoneLayer` to
/// keep the Wayland binding surface separate from the composition model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WaylandSurfaceLayer {
    /// Below all xdg-shell windows.
    Background,
    /// Between background and top.
    Bottom,
    /// Above content, below overlay.
    Top,
    /// Always topmost, survives fullscreen.
    Overlay,
}

/// Interactivity level granted for a surface (S7.4 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaylandInteractivity {
    /// Surface cannot receive input focus.
    None,
    /// Surface can receive input focus on demand.
    OnDemand,
    /// Surface holds exclusive input grab (recovery only).
    Exclusive,
}

/// Surface grant returned by `evaluate_surface_request` (S7.4 §3.1).
///
/// Contains the assigned wlr-layer-shell layer, interactivity level, and
/// exclusive zone width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaylandSurfaceGrant {
    /// Assigned wlr-layer-shell layer.
    pub assigned_layer: WaylandSurfaceLayer,
    /// Interactivity level for this surface.
    pub interactivity: WaylandInteractivity,
    /// Exclusive zone width in pixels (always 0 for non-recovery).
    pub exclusive_zone: u32,
}

/// Evaluate a surface request against S7.4 §3.1 invariants.
///
/// # INV I4 enforcement
///
/// Chrome zone is reserved exclusively for the `aios-chrome` client. Any
/// surface request targeting `CompositionZone::Chrome` with a `claimed_by`
/// other than `"aios-chrome"` is refused.
///
/// # Zone → layer mapping
///
/// | Zone       | Layer      | Interactivity | Exclusive Zone |
/// |------------|------------|---------------|----------------|
/// | Chrome     | Overlay    | `OnDemand`    | 0              |
/// | Content    | Top        | `OnDemand`    | 0              |
/// | Background | Background | None          | 0              |
/// | Recovery   | Overlay    | Exclusive     | 0              |
///
/// # wlr-layer-shell protocol guard
///
/// `WlrLayerShellV1` is only permitted on chrome and recovery zones.
/// Attempting it on content or background returns an internal error.
///
/// # Errors
///
/// - `OverlayLayerForbidden` — INV I4 violation (non-chrome client on chrome zone).
/// - `Internal` — wlr-layer-shell used on a non-chrome, non-recovery zone.
pub fn evaluate_surface_request(
    req: &WaylandSurfaceRequest,
) -> Result<WaylandSurfaceGrant, KdeRendererError> {
    // INV I4: chrome zone is reserved for aios-chrome.
    if req.zone == CompositionZone::Chrome && req.claimed_by != "aios-chrome" {
        return Err(KdeRendererError::OverlayLayerForbidden {
            client_id: req.claimed_by.clone(),
        });
    }

    // wlr-layer-shell is only valid on chrome or recovery zones.
    if req.protocol == WaylandProtocol::WlrLayerShellV1
        && req.zone != CompositionZone::Chrome
        && req.zone != CompositionZone::Recovery
    {
        return Err(KdeRendererError::Internal(
            "wlr-layer-shell requires chrome or recovery zone".to_owned(),
        ));
    }

    let (assigned_layer, interactivity) = match req.zone {
        CompositionZone::Chrome => (WaylandSurfaceLayer::Overlay, WaylandInteractivity::OnDemand),
        CompositionZone::Content => (WaylandSurfaceLayer::Top, WaylandInteractivity::OnDemand),
        CompositionZone::Background => {
            (WaylandSurfaceLayer::Background, WaylandInteractivity::None)
        }
        CompositionZone::Recovery => (
            WaylandSurfaceLayer::Overlay,
            WaylandInteractivity::Exclusive,
        ),
    };

    Ok(WaylandSurfaceGrant {
        assigned_layer,
        interactivity,
        exclusive_zone: 0,
    })
}

/// In-memory Wayland client surface grant tracker (S7.4 §3.1).
///
/// Stores the display name and a map of granted surfaces. No actual socket
/// I/O — `connect()` only validates the display name; `request_surface()`
/// delegates to `evaluate_surface_request()`.
#[derive(Debug)]
pub struct WaylandClient {
    /// Wayland display socket name (e.g. "wayland-0").
    /// Referenced by tests; the field is set during connect and will be used
    /// for actual socket I/O when `wayland-client` is wired in a later milestone.
    #[allow(dead_code)]
    display_name: String,
    /// Map of surface id → (request, grant) for all active grants.
    granted_surfaces: RwLock<HashMap<KdeSurfaceId, (WaylandSurfaceRequest, WaylandSurfaceGrant)>>,
}

impl WaylandClient {
    /// Create a new in-memory Wayland client with the given display name.
    ///
    /// No actual socket connection is established. An empty display name is
    /// rejected as an invariant guard.
    ///
    /// # Errors
    ///
    /// Returns `WaylandConnectError` if `display_name` is empty.
    #[allow(clippy::unused_async)]
    pub async fn connect(display_name: impl Into<String>) -> Result<Self, KdeRendererError> {
        let name: String = display_name.into();
        if name.is_empty() {
            return Err(KdeRendererError::WaylandConnectError(
                "empty display name".to_owned(),
            ));
        }
        Ok(Self {
            display_name: name,
            granted_surfaces: RwLock::new(HashMap::new()),
        })
    }

    /// Request a surface grant for the given surface id and request.
    ///
    /// Delegates to `evaluate_surface_request()` for INV I4 enforcement and
    /// zone→layer mapping. On success, stores the (`request`, `grant`) tuple
    /// keyed by `id` and returns the grant.
    ///
    /// # Errors
    ///
    /// Returns the same errors as `evaluate_surface_request()`.
    pub async fn request_surface(
        &self,
        id: KdeSurfaceId,
        req: WaylandSurfaceRequest,
    ) -> Result<WaylandSurfaceGrant, KdeRendererError> {
        let grant = evaluate_surface_request(&req)?;
        self.granted_surfaces
            .write()
            .await
            .insert(id, (req, grant.clone()));
        Ok(grant)
    }

    /// Revoke a previously granted surface.
    ///
    /// # Errors
    ///
    /// Returns `SurfaceNotFound` if the surface id is unknown.
    pub async fn revoke_surface(&self, id: &KdeSurfaceId) -> Result<(), KdeRendererError> {
        let mut guard = self.granted_surfaces.write().await;
        if guard.remove(id).is_some() {
            Ok(())
        } else {
            Err(KdeRendererError::SurfaceNotFound(id.clone()))
        }
    }

    /// List all active surface grants.
    ///
    /// Returns a vector of `(KdeSurfaceId, WaylandSurfaceGrant)` pairs sorted
    /// for deterministic output.
    pub async fn list_grants(&self) -> Vec<(KdeSurfaceId, WaylandSurfaceGrant)> {
        let mut grants: Vec<(KdeSurfaceId, WaylandSurfaceGrant)> = {
            let guard = self.granted_surfaces.read().await;
            guard
                .iter()
                .map(|(id, (_req, grant))| (id.clone(), grant.clone()))
                .collect()
        };
        grants.sort_by(|(a, _), (b, _)| a.0.cmp(&b.0));
        grants
    }
}
