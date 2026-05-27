//! gRPC `KdeRendererService` server adapter + bootstrap helpers (T-134).
//!
//! [`KdeRendererServer`] mounts the KDE renderer, Wayland client, and `KWin`
//! script loader behind the tonic-generated `KdeRendererService` transport
//! trait. Each RPC method:
//!
//! 1. Converts the proto request into Rust domain types via [`super::conversions`].
//! 2. Calls the backing implementation.
//! 3. Converts the Rust response back into a proto message.
//! 4. Maps [`KdeRendererError`] → [`tonic::Status`] via [`kde_error_to_status`].

#![allow(clippy::result_large_err)]

use std::sync::Arc;

use tonic::{Request, Response, Status};

use crate::kwin_script::KwinScriptLoader;
use crate::renderer::KdeRenderer;
use crate::service::conversions::{
    allocate_request_from_proto, kde_error_to_status, recovery_receipt_to_proto,
    release_receipt_to_proto, surface_descriptor_to_proto, token_receipt_to_proto,
    visual_token_to_proto, wayland_grant_to_proto,
};
use crate::service::proto;
use crate::service::proto::kde_renderer_service_server::KdeRendererService;
use crate::types::KdeSurfaceId;
use crate::visual_token::VisualToken;
use crate::wayland::{evaluate_surface_request, WaylandClient};

// ── KdeRendererServer ────────────────────────────────────────────────────

/// Mounts the KDE renderer, Wayland client, and `KWin` script loader behind the
/// gRPC `KdeRendererService` trait.
#[derive(Clone)]
pub struct KdeRendererServer {
    renderer: Arc<dyn KdeRenderer>,
    #[allow(dead_code)]
    wayland: Arc<WaylandClient>,
    kwin_loader: Arc<KwinScriptLoader>,
}

impl std::fmt::Debug for KdeRendererServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KdeRendererServer").finish_non_exhaustive()
    }
}

impl KdeRendererServer {
    /// Construct a server mounting all three backing components.
    #[must_use]
    pub fn new(
        renderer: Arc<dyn KdeRenderer>,
        wayland: Arc<WaylandClient>,
        kwin_loader: Arc<KwinScriptLoader>,
    ) -> Self {
        Self {
            renderer,
            wayland,
            kwin_loader,
        }
    }
}

#[tonic::async_trait]
impl KdeRendererService for KdeRendererServer {
    // ── Surface lifecycle ────────────────────────────────────────────────

    async fn allocate_surface(
        &self,
        request: Request<proto::AllocateSurfaceRequestProto>,
    ) -> Result<Response<proto::KdeSurfaceDescriptorProto>, Status> {
        let r = request.into_inner();
        let req = allocate_request_from_proto(&r).map_err(|e| kde_error_to_status(&e))?;
        let desc = self
            .renderer
            .allocate_surface(req)
            .await
            .map_err(|e| kde_error_to_status(&e))?;
        Ok(Response::new(surface_descriptor_to_proto(&desc)))
    }

    async fn release_surface(
        &self,
        request: Request<proto::ReleaseSurfaceRequest>,
    ) -> Result<Response<proto::SurfaceReleaseReceiptProto>, Status> {
        let r = request.into_inner();
        let id = KdeSurfaceId(r.id);
        let receipt = self
            .renderer
            .release_surface(id)
            .await
            .map_err(|e| kde_error_to_status(&e))?;
        Ok(Response::new(release_receipt_to_proto(&receipt)))
    }

    async fn get_surface(
        &self,
        request: Request<proto::GetSurfaceRequest>,
    ) -> Result<Response<proto::KdeSurfaceDescriptorProto>, Status> {
        let r = request.into_inner();
        let id = KdeSurfaceId(r.id);
        let desc = self
            .renderer
            .get_surface(id)
            .await
            .map_err(|e| kde_error_to_status(&e))?;
        Ok(Response::new(surface_descriptor_to_proto(&desc)))
    }

    async fn list_surfaces(
        &self,
        request: Request<proto::ListSurfacesRequest>,
    ) -> Result<Response<proto::ListSurfacesResponse>, Status> {
        let r = request.into_inner();
        let filter = r
            .filter
            .as_ref()
            .map_or(Ok(crate::renderer::SurfaceFilter::All), |f| {
                crate::service::conversions::surface_filter_from_proto(f)
                    .map_err(|e| kde_error_to_status(&e))
            })?;
        let surfaces = self.renderer.list_surfaces(filter).await;
        let entries: Vec<proto::KdeSurfaceDescriptorProto> =
            surfaces.iter().map(surface_descriptor_to_proto).collect();
        Ok(Response::new(proto::ListSurfacesResponse {
            surfaces: entries,
        }))
    }

    // ── Mode management ──────────────────────────────────────────────────

    async fn enter_recovery_mode(
        &self,
        _request: Request<proto::EnterRecoveryModeRequest>,
    ) -> Result<Response<proto::RecoveryEntryReceiptProto>, Status> {
        let receipt = self
            .renderer
            .enter_recovery_mode()
            .await
            .map_err(|e| kde_error_to_status(&e))?;
        Ok(Response::new(recovery_receipt_to_proto(&receipt)))
    }

    async fn exit_recovery_mode(
        &self,
        _request: Request<proto::ExitRecoveryModeRequest>,
    ) -> Result<Response<()>, Status> {
        self.renderer
            .exit_recovery_mode()
            .await
            .map_err(|e| kde_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn enter_degraded_mode(
        &self,
        request: Request<proto::EnterDegradedModeRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.renderer
            .enter_degraded_mode(r.reason)
            .await
            .map_err(|e| kde_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn get_mode(
        &self,
        _request: Request<proto::GetModeRequest>,
    ) -> Result<Response<proto::GetModeResponse>, Status> {
        let mode = self.renderer.get_mode().await;
        let mode_proto = crate::service::conversions::renderer_mode_to_proto(&mode);
        Ok(Response::new(proto::GetModeResponse {
            mode: Some(mode_proto),
        }))
    }

    // ── Visual tokens ────────────────────────────────────────────────────

    async fn apply_visual_tokens(
        &self,
        request: Request<proto::ApplyVisualTokensRequest>,
    ) -> Result<Response<proto::TokenApplicationReceiptProto>, Status> {
        let r = request.into_inner();
        let tokens: Result<Vec<VisualToken>, Status> = r
            .tokens
            .iter()
            .map(|t| {
                crate::service::conversions::visual_token_from_proto(t)
                    .map_err(|e| kde_error_to_status(&e))
            })
            .collect();
        let tokens = tokens?;
        let receipt = self
            .renderer
            .apply_visual_tokens(tokens)
            .await
            .map_err(|e| kde_error_to_status(&e))?;
        Ok(Response::new(token_receipt_to_proto(&receipt)))
    }

    async fn get_active_tokens(
        &self,
        _request: Request<proto::GetActiveTokensRequest>,
    ) -> Result<Response<proto::GetActiveTokensResponse>, Status> {
        let tokens = self.renderer.get_active_tokens().await;
        let entries: Vec<proto::VisualTokenProto> =
            tokens.iter().map(visual_token_to_proto).collect();
        Ok(Response::new(proto::GetActiveTokensResponse {
            tokens: entries,
        }))
    }

    // ── Wayland evaluation ───────────────────────────────────────────────

    async fn evaluate_wayland_surface(
        &self,
        request: Request<proto::EvaluateWaylandSurfaceRequest>,
    ) -> Result<Response<proto::WaylandSurfaceGrantProto>, Status> {
        let r = request.into_inner();
        let req = r
            .request
            .ok_or_else(|| Status::invalid_argument("request field is required"))?;
        let surface_request = crate::service::conversions::wayland_request_from_proto(&req)
            .map_err(|e| kde_error_to_status(&e))?;
        let grant =
            evaluate_surface_request(&surface_request).map_err(|e| kde_error_to_status(&e))?;
        Ok(Response::new(wayland_grant_to_proto(&grant)))
    }

    // ── KWin script management ───────────────────────────────────────────

    async fn load_kwin_script(
        &self,
        request: Request<proto::LoadKwinScriptRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let script_proto = r
            .script
            .ok_or_else(|| Status::invalid_argument("script field is required"))?;
        let script = crate::service::conversions::kwin_script_from_proto(&script_proto);
        self.kwin_loader
            .load_script(script)
            .await
            .map_err(|e| kde_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn unload_kwin_script(
        &self,
        request: Request<proto::UnloadKwinScriptRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.kwin_loader
            .unload_script(&r.script_id)
            .await
            .map_err(|e| kde_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn list_kwin_scripts(
        &self,
        _request: Request<proto::ListKwinScriptsRequest>,
    ) -> Result<Response<proto::ListKwinScriptsResponse>, Status> {
        let script_ids = self.kwin_loader.list_loaded().await;
        Ok(Response::new(proto::ListKwinScriptsResponse { script_ids }))
    }
}

// ── Bootstrap helpers ────────────────────────────────────────────────────

/// Build a `tonic::transport::server::Router` with `KdeRendererService` mounted.
#[must_use]
pub fn build_router(svc: KdeRendererServer) -> tonic::transport::server::Router {
    tonic::transport::server::Server::builder()
        .add_service(proto::kde_renderer_service_server::KdeRendererServiceServer::new(svc))
}
