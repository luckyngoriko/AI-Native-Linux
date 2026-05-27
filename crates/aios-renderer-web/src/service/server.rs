//! gRPC `WebRendererService` server adapter + bootstrap helpers (T-147).
//!
//! [`WebRendererServer`] mounts the Web renderer, exposure FSM, origin verifier,
//! chrome integrity monitor, and gRPC-Web bridge behind the tonic-generated
//! `WebRendererService` transport trait. Each RPC method:
//!
//! 1. Converts the proto request into Rust domain types via [`super::conversions`].
//! 2. Calls the backing implementation.
//! 3. Converts the Rust response back into a proto message.
//! 4. Maps [`WebRendererError`] → [`tonic::Status`] via [`web_error_to_status`].

#![allow(clippy::result_large_err)]

use std::sync::Arc;

use chrono::TimeZone;
use tonic::{Request, Response, Status};

use crate::chrome_integrity::{ChromeIntegrityMonitor, ChromeTreeFragment};
use crate::exposure_fsm::ExposureFsm;
use crate::grpc_web_bridge::GrpcWebBridge;
use crate::origin_verifier::{IframeOriginBinding, OriginVerifier};
use crate::renderer::WebRenderer;
use crate::service::conversions::{
    allocate_request_from_proto, exposure_level_to_proto, recovery_receipt_to_proto,
    release_receipt_to_proto, route_descriptor_from_proto, route_descriptor_to_proto,
    surface_descriptor_to_proto, surface_filter_from_proto, token_receipt_to_proto,
    visual_token_from_proto, visual_token_to_proto, web_error_to_status,
};
use crate::service::proto;
use crate::service::proto::web_renderer_service_server::WebRendererService;
use crate::types::WebSurfaceId;
use aios_renderer_kde::VisualToken;

// ── WebRendererServer ────────────────────────────────────────────────────

/// Mounts the Web renderer, exposure FSM, origin verifier, chrome integrity
/// monitor, and gRPC-Web bridge behind the gRPC `WebRendererService` trait.
#[derive(Clone)]
pub struct WebRendererServer {
    renderer: Arc<dyn WebRenderer>,
    exposure_fsm: Arc<ExposureFsm>,
    origin_verifier: Arc<OriginVerifier>,
    chrome_integrity: Arc<ChromeIntegrityMonitor>,
    #[allow(dead_code)]
    bridge: Arc<GrpcWebBridge>,
}

impl std::fmt::Debug for WebRendererServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebRendererServer").finish_non_exhaustive()
    }
}

impl WebRendererServer {
    /// Construct a server mounting all five backing components.
    #[must_use]
    pub fn new(
        renderer: Arc<dyn WebRenderer>,
        exposure_fsm: Arc<ExposureFsm>,
        origin_verifier: Arc<OriginVerifier>,
        chrome_integrity: Arc<ChromeIntegrityMonitor>,
        bridge: Arc<GrpcWebBridge>,
    ) -> Self {
        Self {
            renderer,
            exposure_fsm,
            origin_verifier,
            chrome_integrity,
            bridge,
        }
    }
}

#[tonic::async_trait]
impl WebRendererService for WebRendererServer {
    // ── Surface lifecycle ────────────────────────────────────────────────

    async fn allocate_surface(
        &self,
        request: Request<proto::AllocateWebSurfaceRequestProto>,
    ) -> Result<Response<proto::WebSurfaceDescriptorProto>, Status> {
        let r = request.into_inner();
        let req = allocate_request_from_proto(&r).map_err(|e| web_error_to_status(&e))?;
        let desc = self
            .renderer
            .allocate_surface(req)
            .await
            .map_err(|e| web_error_to_status(&e))?;
        Ok(Response::new(surface_descriptor_to_proto(&desc)))
    }

    async fn release_surface(
        &self,
        request: Request<proto::ReleaseSurfaceRequest>,
    ) -> Result<Response<proto::WebSurfaceReleaseReceiptProto>, Status> {
        let r = request.into_inner();
        let id = WebSurfaceId(r.id);
        let receipt = self
            .renderer
            .release_surface(id)
            .await
            .map_err(|e| web_error_to_status(&e))?;
        Ok(Response::new(release_receipt_to_proto(&receipt)))
    }

    async fn get_surface(
        &self,
        request: Request<proto::GetSurfaceRequest>,
    ) -> Result<Response<proto::WebSurfaceDescriptorProto>, Status> {
        let r = request.into_inner();
        let id = WebSurfaceId(r.id);
        let desc = self
            .renderer
            .get_surface(id)
            .await
            .map_err(|e| web_error_to_status(&e))?;
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
            .map_or(Ok(crate::renderer::WebSurfaceFilter::All), |f| {
                surface_filter_from_proto(f).map_err(|e| web_error_to_status(&e))
            })?;
        let surfaces = self.renderer.list_surfaces(filter).await;
        let entries: Vec<proto::WebSurfaceDescriptorProto> =
            surfaces.iter().map(surface_descriptor_to_proto).collect();
        Ok(Response::new(proto::ListSurfacesResponse {
            surfaces: entries,
        }))
    }

    // ── Route management ─────────────────────────────────────────────────

    async fn register_route(
        &self,
        request: Request<proto::RegisterRouteRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let route_proto = r
            .route
            .ok_or_else(|| Status::invalid_argument("route field is required"))?;
        let route = route_descriptor_from_proto(&route_proto);
        self.renderer
            .register_route(route)
            .await
            .map_err(|e| web_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn unregister_route(
        &self,
        request: Request<proto::UnregisterRouteRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.renderer
            .unregister_route(&r.path)
            .await
            .map_err(|e| web_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn list_routes(
        &self,
        _request: Request<proto::ListRoutesRequest>,
    ) -> Result<Response<proto::ListRoutesResponse>, Status> {
        let routes = self.renderer.list_routes().await;
        let entries: Vec<proto::RouteDescriptorProto> =
            routes.iter().map(route_descriptor_to_proto).collect();
        Ok(Response::new(proto::ListRoutesResponse { routes: entries }))
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
            .map_err(|e| web_error_to_status(&e))?;
        Ok(Response::new(recovery_receipt_to_proto(&receipt)))
    }

    async fn exit_recovery_mode(
        &self,
        _request: Request<proto::ExitRecoveryModeRequest>,
    ) -> Result<Response<()>, Status> {
        self.renderer
            .exit_recovery_mode()
            .await
            .map_err(|e| web_error_to_status(&e))?;
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
            .map_err(|e| web_error_to_status(&e))?;
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

    // ── Exposure FSM ─────────────────────────────────────────────────────

    async fn current_exposure(
        &self,
        _request: Request<proto::CurrentExposureRequest>,
    ) -> Result<Response<proto::CurrentExposureResponse>, Status> {
        let level = self.renderer.current_exposure().await;
        Ok(Response::new(proto::CurrentExposureResponse {
            level: Some(exposure_level_to_proto(&level)),
        }))
    }

    async fn request_lan_escalation(
        &self,
        request: Request<proto::RequestLanEscalationRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.exposure_fsm
            .request_lan_escalation(&r.approver_canonical_id)
            .await
            .map_err(|e| web_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn apply_policy_decision(
        &self,
        request: Request<proto::ApplyPolicyDecisionRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.exposure_fsm
            .apply_policy_decision(&r.decision_id)
            .await
            .map_err(|e| web_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn activate_lan_exposure(
        &self,
        _request: Request<proto::ActivateLanExposureRequest>,
    ) -> Result<Response<()>, Status> {
        self.exposure_fsm
            .activate_lan_exposure()
            .await
            .map_err(|e| web_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn record_heartbeat(
        &self,
        _request: Request<proto::RecordHeartbeatRequest>,
    ) -> Result<Response<()>, Status> {
        self.exposure_fsm
            .record_heartbeat()
            .await
            .map_err(|e| web_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn revoke_exposure(
        &self,
        request: Request<proto::RevokeExposureRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.exposure_fsm
            .revoke(&r.reason)
            .await
            .map_err(|e| web_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn escalate_to_public(
        &self,
        request: Request<proto::EscalateToPublicRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.exposure_fsm
            .escalate_to_public(&r.authorized_by, &r.decision_id)
            .await
            .map_err(|e| web_error_to_status(&e))?;
        Ok(Response::new(()))
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
            .map(|t| visual_token_from_proto(t).map_err(|e| web_error_to_status(&e)))
            .collect();
        let tokens = tokens?;
        let receipt = self
            .renderer
            .apply_visual_tokens(tokens)
            .await
            .map_err(|e| web_error_to_status(&e))?;
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

    // ── Iframe origin binding (INV I4) ───────────────────────────────────

    async fn register_iframe_binding(
        &self,
        request: Request<proto::RegisterIframeBindingRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let binding = IframeOriginBinding {
            iframe_origin: r.iframe_origin,
            surface_id: WebSurfaceId(r.surface_id),
            bound_group_id: r.bound_group_id,
            scope_binding_evidence_id: r.scope_binding_evidence_id,
        };
        self.origin_verifier
            .register_binding(binding)
            .await
            .map_err(|e| web_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn verify_composition(
        &self,
        request: Request<proto::VerifyCompositionRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.origin_verifier
            .verify_composition(&WebSurfaceId(r.surface_id), &r.presented_origin)
            .await
            .map_err(|e| web_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    // ── Chrome shadow-root integrity (INV I10) ───────────────────────────

    async fn admit_chrome_fragment(
        &self,
        request: Request<proto::AdmitChromeFragmentRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let signed_at = r.signed_at.map_or_else(chrono::Utc::now, |ts| {
            chrono::Utc
                .timestamp_opt(ts.seconds, u32::try_from(ts.nanos).unwrap_or(0))
                .single()
                .unwrap_or_else(chrono::Utc::now)
        });
        let fragment = ChromeTreeFragment {
            root_hash: r.root_hash,
            signature: r.signature,
            signed_at,
        };
        self.chrome_integrity
            .admit_signed_fragment(fragment)
            .await
            .map_err(|e| web_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn check_chrome_integrity(
        &self,
        request: Request<proto::CheckChromeIntegrityRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.chrome_integrity
            .check_observed_hash(&r.observed_hash)
            .await
            .map_err(|e| web_error_to_status(&e))?;
        Ok(Response::new(()))
    }
}

// ── Bootstrap helpers ────────────────────────────────────────────────────

/// Build a `tonic::transport::server::Router` with `WebRendererService` mounted.
#[must_use]
pub fn build_router(svc: WebRendererServer) -> tonic::transport::server::Router {
    tonic::transport::server::Server::builder()
        .add_service(proto::web_renderer_service_server::WebRendererServiceServer::new(svc))
}
