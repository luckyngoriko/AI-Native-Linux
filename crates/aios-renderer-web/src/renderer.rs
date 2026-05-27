//! `WebRenderer` async trait and `InMemoryWebRenderer` implementation (S7.5 §4).
//!
//! The `WebRenderer` trait defines 14 RPCs covering surface allocation/release,
//! route registration, mode transitions (Normal ↔ Recovery ↔ Degraded),
//! exposure level, and visual token management per the S7.5 typed skeleton.
//!
//! `InMemoryWebRenderer` provides a single-node in-memory implementation backed
//! by `RwLock<WebRendererState>` for testing and development. Production
//! backends (gRPC-Web bridge, Next.js) compose against the trait.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::error::WebRendererError;
use crate::exposure::ExposureLevel;
use crate::origin::{OriginScheme, ParsedOrigin};
use crate::types::{RouteDescriptor, WebRendererMode, WebSurfaceDescriptor, WebSurfaceId};
use aios_renderer_kde::{NodeKind, VisualToken};

// ── Request / receipt types ─────────────────────────────────────────────────

/// Request to allocate a new web renderer surface.
///
/// Carries the origin, node kind, claimant identity, and an optional expected
/// group identifier used for INV I4 per-origin verification at allocation time.
#[derive(Debug, Clone)]
pub struct AllocateWebSurfaceRequest {
    /// The parsed origin this surface is served under.
    pub origin: ParsedOrigin,
    /// The node kind of the surface being allocated.
    pub node_kind: NodeKind,
    /// Canonical subject id of the client claiming this surface.
    pub claimed_by: String,
    /// Optional expected group identifier for INV I4 verification.
    pub expected_group_id: Option<String>,
}

/// Receipt emitted when a surface is released.
#[derive(Debug, Clone)]
pub struct WebSurfaceReleaseReceipt {
    /// The id of the released surface.
    pub id: WebSurfaceId,
    /// Wall-clock timestamp of release.
    pub released_at: DateTime<Utc>,
    /// Renderer mode at the time of release.
    pub final_mode: WebRendererMode,
}

/// Receipt emitted when the renderer enters recovery mode (INV I8).
#[derive(Debug, Clone)]
pub struct RecoveryEntryReceipt {
    /// Wall-clock timestamp of recovery mode entry.
    pub entered_at: DateTime<Utc>,
    /// Canonical recovery origin URL.
    pub recovery_origin: String,
    /// INV I8 — service worker is always disabled in recovery mode.
    pub service_worker_disabled: bool,
}

/// Receipt emitted after visual token application.
#[derive(Debug, Clone)]
pub struct TokenApplicationReceipt {
    /// Number of tokens applied.
    pub applied_count: usize,
    /// Wall-clock timestamp of application.
    pub timestamp: DateTime<Utc>,
}

/// Filter for listing web surfaces.
#[derive(Debug, Clone)]
pub enum WebSurfaceFilter {
    /// Return all surfaces.
    All,
    /// Filter by origin's full origin string.
    ByOrigin(String),
    /// Filter by claimant identity.
    ByClaimant(String),
    /// Filter by node kind.
    ByNodeKind(NodeKind),
    /// Filter by renderer mode.
    InModeOnly(WebRendererMode),
}

// ── In-memory state ─────────────────────────────────────────────────────────

/// Internal mutable state for `InMemoryWebRenderer`.
#[derive(Debug)]
pub struct WebRendererState {
    /// Allocated surfaces keyed by surface id.
    pub surfaces: HashMap<WebSurfaceId, WebSurfaceDescriptor>,
    /// Registered routes keyed by path.
    pub routes: HashMap<String, RouteDescriptor>,
    /// Current renderer operational mode.
    pub mode: WebRendererMode,
    /// Current exposure level.
    pub exposure: ExposureLevel,
    /// Active visual tokens.
    pub active_tokens: Vec<VisualToken>,
}

impl Default for WebRendererState {
    fn default() -> Self {
        Self {
            surfaces: HashMap::new(),
            routes: HashMap::new(),
            mode: WebRendererMode::Normal,
            exposure: ExposureLevel::Localhost,
            active_tokens: Vec::new(),
        }
    }
}

// ── WebRenderer trait ───────────────────────────────────────────────────────

/// Web renderer async trait — 14 RPCs covering the full S7.5 surface lifecycle.
///
/// Production backends (gRPC-Web bridge, Next.js) implement this trait.
/// `InMemoryWebRenderer` provides a single-node in-memory implementation for
/// testing and development.
#[async_trait::async_trait]
pub trait WebRenderer: Send + Sync {
    /// Allocate a new web renderer surface.
    async fn allocate_surface(
        &self,
        req: AllocateWebSurfaceRequest,
    ) -> Result<WebSurfaceDescriptor, WebRendererError>;

    /// Release a previously allocated surface.
    async fn release_surface(
        &self,
        id: WebSurfaceId,
    ) -> Result<WebSurfaceReleaseReceipt, WebRendererError>;

    /// Look up a surface by id.
    async fn get_surface(&self, id: WebSurfaceId)
        -> Result<WebSurfaceDescriptor, WebRendererError>;

    /// List surfaces matching the given filter.
    async fn list_surfaces(&self, filter: WebSurfaceFilter) -> Vec<WebSurfaceDescriptor>;

    /// Register a route descriptor.
    async fn register_route(&self, route: RouteDescriptor) -> Result<(), WebRendererError>;

    /// Unregister a route by path.
    async fn unregister_route(&self, path: &str) -> Result<(), WebRendererError>;

    /// List all registered routes.
    async fn list_routes(&self) -> Vec<RouteDescriptor>;

    /// Enter recovery mode — serves the recovery shell SPA (INV I8).
    async fn enter_recovery_mode(&self) -> Result<RecoveryEntryReceipt, WebRendererError>;

    /// Exit recovery mode — return to normal operation.
    async fn exit_recovery_mode(&self) -> Result<(), WebRendererError>;

    /// Enter degraded mode with a reason class.
    async fn enter_degraded_mode(&self, reason: String) -> Result<(), WebRendererError>;

    /// Return the current renderer mode.
    async fn get_mode(&self) -> WebRendererMode;

    /// Return the current exposure level.
    async fn current_exposure(&self) -> ExposureLevel;

    /// Apply a batch of visual tokens (replaces existing tokens).
    async fn apply_visual_tokens(
        &self,
        tokens: Vec<VisualToken>,
    ) -> Result<TokenApplicationReceipt, WebRendererError>;

    /// Return the currently active visual tokens.
    async fn get_active_tokens(&self) -> Vec<VisualToken>;
}

// ── InMemoryWebRenderer ─────────────────────────────────────────────────────

/// Single-node in-memory `WebRenderer` backed by `RwLock<WebRendererState>`.
///
/// All operations are synchronous under the hood — the async trait interface is
/// retained for compatibility with production backends (gRPC-Web bridge,
/// Next.js) that perform I/O.
pub struct InMemoryWebRenderer {
    state: RwLock<WebRendererState>,
}

impl InMemoryWebRenderer {
    /// Create a new `InMemoryWebRenderer` with default state:
    /// `Normal` mode, `Localhost` exposure, empty surfaces/routes/tokens.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: RwLock::new(WebRendererState::default()),
        }
    }
}

impl Default for InMemoryWebRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebRenderer for InMemoryWebRenderer {
    async fn allocate_surface(
        &self,
        req: AllocateWebSurfaceRequest,
    ) -> Result<WebSurfaceDescriptor, WebRendererError> {
        // INV I4 per-origin group verification
        if let Some(ref group_id) = req.expected_group_id {
            req.origin.verify_against_group(group_id)?;
        }

        let state = self.state.read().await;

        // Recovery mode admission (INV I11): only recovery.localhost scheme
        if matches!(state.mode, WebRendererMode::Recovery)
            && !matches!(req.origin.scheme, OriginScheme::Recovery)
        {
            return Err(WebRendererError::Internal(
                "recovery mode: recovery origin only".into(),
            ));
        }

        // Degraded mode: GPU-bearing kinds blocked
        if matches!(state.mode, WebRendererMode::Degraded(_))
            && req.node_kind.compilation_hint().is_gpu_bearing
        {
            return Err(WebRendererError::WebgpuAdapterUnavailable(
                "gpu kind in degraded mode".into(),
            ));
        }

        drop(state);

        let mut desc =
            WebSurfaceDescriptor::new(req.origin.clone(), req.node_kind, req.claimed_by.clone())?;

        {
            let mut state = self.state.write().await;
            desc.mode = state.mode.clone();
            state.surfaces.insert(desc.id.clone(), desc.clone());
        }
        Ok(desc)
    }

    async fn release_surface(
        &self,
        id: WebSurfaceId,
    ) -> Result<WebSurfaceReleaseReceipt, WebRendererError> {
        let mut state = self.state.write().await;
        let _desc = state
            .surfaces
            .remove(&id)
            .ok_or_else(|| WebRendererError::SurfaceNotFound(id.clone()))?;
        Ok(WebSurfaceReleaseReceipt {
            id,
            released_at: Utc::now(),
            final_mode: state.mode.clone(),
        })
    }

    async fn get_surface(
        &self,
        id: WebSurfaceId,
    ) -> Result<WebSurfaceDescriptor, WebRendererError> {
        let state = self.state.read().await;
        state
            .surfaces
            .get(&id)
            .cloned()
            .ok_or(WebRendererError::SurfaceNotFound(id))
    }

    async fn list_surfaces(&self, filter: WebSurfaceFilter) -> Vec<WebSurfaceDescriptor> {
        let state = self.state.read().await;
        match filter {
            WebSurfaceFilter::All => state.surfaces.values().cloned().collect(),
            WebSurfaceFilter::ByOrigin(origin) => state
                .surfaces
                .values()
                .filter(|s| s.origin.full_origin == origin)
                .cloned()
                .collect(),
            WebSurfaceFilter::ByClaimant(claimant) => state
                .surfaces
                .values()
                .filter(|s| s.claimed_by == claimant)
                .cloned()
                .collect(),
            WebSurfaceFilter::ByNodeKind(kind) => state
                .surfaces
                .values()
                .filter(|s| s.node_kind == kind)
                .cloned()
                .collect(),
            WebSurfaceFilter::InModeOnly(mode) => state
                .surfaces
                .values()
                .filter(|s| s.mode == mode)
                .cloned()
                .collect(),
        }
    }

    async fn register_route(&self, route: RouteDescriptor) -> Result<(), WebRendererError> {
        self.state
            .write()
            .await
            .routes
            .insert(route.path.clone(), route);
        Ok(())
    }

    async fn unregister_route(&self, path: &str) -> Result<(), WebRendererError> {
        let mut state = self.state.write().await;
        state
            .routes
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| WebRendererError::Internal(format!("route not found: {path}")))
    }

    async fn list_routes(&self) -> Vec<RouteDescriptor> {
        let state = self.state.read().await;
        state.routes.values().cloned().collect()
    }

    async fn enter_recovery_mode(&self) -> Result<RecoveryEntryReceipt, WebRendererError> {
        let mut state = self.state.write().await;
        state.mode = WebRendererMode::Recovery;
        drop(state);
        Ok(RecoveryEntryReceipt {
            entered_at: Utc::now(),
            recovery_origin: "https://recovery.localhost:8443".into(),
            service_worker_disabled: true,
        })
    }

    async fn exit_recovery_mode(&self) -> Result<(), WebRendererError> {
        let mut state = self.state.write().await;
        state.mode = WebRendererMode::Normal;
        drop(state);
        Ok(())
    }

    async fn enter_degraded_mode(&self, reason: String) -> Result<(), WebRendererError> {
        let mut state = self.state.write().await;
        state.mode = WebRendererMode::Degraded(reason);
        drop(state);
        Ok(())
    }

    async fn get_mode(&self) -> WebRendererMode {
        let state = self.state.read().await;
        state.mode.clone()
    }

    async fn current_exposure(&self) -> ExposureLevel {
        let state = self.state.read().await;
        state.exposure.clone()
    }

    async fn apply_visual_tokens(
        &self,
        tokens: Vec<VisualToken>,
    ) -> Result<TokenApplicationReceipt, WebRendererError> {
        let mut state = self.state.write().await;
        let count = tokens.len();
        state.active_tokens = tokens;
        drop(state);
        Ok(TokenApplicationReceipt {
            applied_count: count,
            timestamp: Utc::now(),
        })
    }

    async fn get_active_tokens(&self) -> Vec<VisualToken> {
        let state = self.state.read().await;
        state.active_tokens.clone()
    }
}
