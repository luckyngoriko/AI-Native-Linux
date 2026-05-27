//! Integration tests for the gRPC `WebRendererService` surface (T-147).
//!
//! Each test boots an in-process tonic server backed by the in-memory
//! renderer, exposure FSM, origin verifier, chrome integrity monitor,
//! and gRPC-Web bridge, connects via a TCP listener, and exercises
//! one RPC path.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::no_effect_underscore_binding,
    clippy::doc_markdown,
    clippy::significant_drop_tightening,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;
use std::time::Duration;

use tonic::Request;

use aios_renderer_web::chrome_integrity::ChromeIntegrityMonitor;
use aios_renderer_web::exposure_fsm::ExposureFsm;
use aios_renderer_web::grpc_web_bridge::GrpcWebBridge;
use aios_renderer_web::origin_verifier::OriginVerifier;
use aios_renderer_web::renderer::{InMemoryWebRenderer, WebRenderer};
use aios_renderer_web::service::proto::web_renderer_service_client::WebRendererServiceClient;
use aios_renderer_web::service::proto::{
    AllocateWebSurfaceRequestProto, ApplyVisualTokensRequest, GetActiveTokensRequest,
    GetModeRequest, GetSurfaceRequest, ListRoutesRequest, ListSurfacesRequest, ParsedOriginProto,
    RegisterRouteRequest, ReleaseSurfaceRequest, RouteDescriptorProto, VisualTokenProto,
    WebSurfaceFilterProto,
};
use aios_renderer_web::service::{build_router, WebRendererServer};

// ── Test harness ─────────────────────────────────────────────────────────

struct TestHarness {
    client: WebRendererServiceClient<tonic::transport::Channel>,
}

impl TestHarness {
    async fn new() -> Self {
        let renderer = Arc::new(InMemoryWebRenderer::new());
        let exposure_fsm = Arc::new(ExposureFsm::new());
        let origin_verifier = Arc::new(OriginVerifier::new());
        let integrity = Arc::new(ChromeIntegrityMonitor::new(
            ed25519_dalek::SigningKey::generate(&mut rand_core::OsRng).verifying_key(),
        ));
        let bridge = Arc::new(GrpcWebBridge::new(
            aios_renderer_web::grpc_web_bridge::default_localhost_config(),
        ));

        let svc = WebRendererServer::new(
            renderer as Arc<dyn WebRenderer>,
            exposure_fsm,
            origin_verifier,
            integrity,
            bridge,
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let router = build_router(svc);

        tokio::spawn(async move {
            router
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = WebRendererServiceClient::connect(format!("http://{addr}"))
            .await
            .unwrap();

        Self { client }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn aios_localhost_origin() -> ParsedOriginProto {
    ParsedOriginProto {
        full_origin: "https://acme-app.aios.localhost:8443".into(),
        host: "acme-app.aios.localhost".into(),
        port: 8443,
        origin_scheme: "aios_localhost".into(),
        origin_token: Some("acme-app".into()),
    }
}

// ── Test 1: server boots ─────────────────────────────────────────────────

#[tokio::test]
async fn server_boots() {
    let _harness = TestHarness::new().await;
}

// ── Test 2: allocate_surface chrome aios-localhost succeeds ───────────────

#[tokio::test]
async fn allocate_surface_aios_localhost_succeeds() {
    let mut harness = TestHarness::new().await;
    let req = AllocateWebSurfaceRequestProto {
        origin: Some(aios_localhost_origin()),
        node_kind: aios_renderer_web::service::proto::NodeKindProto::Container as i32,
        claimed_by: "family:app:com.example".into(),
        expected_group_id: None,
    };
    let resp = harness
        .client
        .allocate_surface(Request::new(req))
        .await
        .unwrap();
    let desc = resp.into_inner();
    assert!(!desc.id.is_empty());
    assert_eq!(desc.claimed_by, "family:app:com.example");
}

// ── Test 3: mismatched group_id returns PermissionDenied ──────────────────

#[tokio::test]
async fn allocate_surface_mismatched_group_id_returns_permission_denied() {
    let mut harness = TestHarness::new().await;
    let req = AllocateWebSurfaceRequestProto {
        origin: Some(aios_localhost_origin()),
        node_kind: aios_renderer_web::service::proto::NodeKindProto::Container as i32,
        claimed_by: "family:app:com.example".into(),
        expected_group_id: Some("other-group".into()),
    };
    let status = harness
        .client
        .allocate_surface(Request::new(req))
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

// ── Test 4: get_surface round-trip after allocate ─────────────────────────

#[tokio::test]
async fn get_surface_roundtrip() {
    let mut harness = TestHarness::new().await;
    let req = AllocateWebSurfaceRequestProto {
        origin: Some(aios_localhost_origin()),
        node_kind: aios_renderer_web::service::proto::NodeKindProto::Card as i32,
        claimed_by: "test-client".into(),
        expected_group_id: None,
    };
    let allocated = harness
        .client
        .allocate_surface(Request::new(req))
        .await
        .unwrap();
    let allocated_id = allocated.into_inner().id;

    let get_resp = harness
        .client
        .get_surface(Request::new(GetSurfaceRequest {
            id: allocated_id.clone(),
        }))
        .await
        .unwrap();
    let desc = get_resp.into_inner();
    assert_eq!(desc.id, allocated_id);
    assert_eq!(desc.claimed_by, "test-client");
}

// ── Test 5: release_surface then get returns NotFound ─────────────────────

#[tokio::test]
async fn release_surface_then_get_returns_not_found() {
    let mut harness = TestHarness::new().await;
    let req = AllocateWebSurfaceRequestProto {
        origin: Some(aios_localhost_origin()),
        node_kind: aios_renderer_web::service::proto::NodeKindProto::Container as i32,
        claimed_by: "test-client".into(),
        expected_group_id: None,
    };
    let allocated = harness
        .client
        .allocate_surface(Request::new(req))
        .await
        .unwrap();
    let allocated_id = allocated.into_inner().id;

    // Release
    let release_resp = harness
        .client
        .release_surface(Request::new(ReleaseSurfaceRequest {
            id: allocated_id.clone(),
        }))
        .await
        .unwrap();
    let receipt = release_resp.into_inner();
    assert_eq!(receipt.id, allocated_id);

    // Now Get should return NotFound
    let status = harness
        .client
        .get_surface(Request::new(GetSurfaceRequest { id: allocated_id }))
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

// ── Test 6: list_surfaces All returns allocated ───────────────────────────

#[tokio::test]
async fn list_surfaces_all_returns_allocated() {
    let mut harness = TestHarness::new().await;
    let req = AllocateWebSurfaceRequestProto {
        origin: Some(aios_localhost_origin()),
        node_kind: aios_renderer_web::service::proto::NodeKindProto::Container as i32,
        claimed_by: "test-client".into(),
        expected_group_id: None,
    };
    let allocated_id = harness
        .client
        .allocate_surface(Request::new(req))
        .await
        .unwrap()
        .into_inner()
        .id;

    let resp = harness
        .client
        .list_surfaces(Request::new(ListSurfacesRequest {
            filter: Some(WebSurfaceFilterProto {
                filter: Some(
                    aios_renderer_web::service::proto::web_surface_filter_proto::Filter::All(()),
                ),
            }),
        }))
        .await
        .unwrap();
    let surfaces = resp.into_inner().surfaces;
    assert_eq!(surfaces.len(), 1);
    assert_eq!(surfaces[0].id, allocated_id);
}

// ── Test 7: list_surfaces by_origin filters correctly ─────────────────────

#[tokio::test]
async fn list_surfaces_by_origin_filters_correctly() {
    let mut harness = TestHarness::new().await;
    // Allocate one surface
    let req = AllocateWebSurfaceRequestProto {
        origin: Some(aios_localhost_origin()),
        node_kind: aios_renderer_web::service::proto::NodeKindProto::Container as i32,
        claimed_by: "test-client".into(),
        expected_group_id: None,
    };
    harness
        .client
        .allocate_surface(Request::new(req))
        .await
        .unwrap();

    let resp = harness
        .client
        .list_surfaces(Request::new(ListSurfacesRequest {
            filter: Some(WebSurfaceFilterProto {
                filter: Some(
                    aios_renderer_web::service::proto::web_surface_filter_proto::Filter::ByOrigin(
                        "https://acme-app.aios.localhost:8443".into(),
                    ),
                ),
            }),
        }))
        .await
        .unwrap();
    assert_eq!(resp.into_inner().surfaces.len(), 1);
}

// ── Test 8: register and list routes round-trip ───────────────────────────

#[tokio::test]
async fn register_and_list_routes_roundtrip() {
    let mut harness = TestHarness::new().await;
    let route = RouteDescriptorProto {
        path: "/dashboard".into(),
        requires_auth: true,
        served_in_recovery: false,
    };
    harness
        .client
        .register_route(Request::new(RegisterRouteRequest { route: Some(route) }))
        .await
        .unwrap();

    let resp = harness
        .client
        .list_routes(Request::new(ListRoutesRequest {}))
        .await
        .unwrap();
    let routes = resp.into_inner().routes;
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].path, "/dashboard");
    assert!(routes[0].requires_auth);
    assert!(!routes[0].served_in_recovery);
}

// ── Test 9: unregister route then list empty ──────────────────────────────

#[tokio::test]
async fn unregister_route_then_list_empty() {
    let mut harness = TestHarness::new().await;
    let route = RouteDescriptorProto {
        path: "/dashboard".into(),
        requires_auth: false,
        served_in_recovery: false,
    };
    harness
        .client
        .register_route(Request::new(RegisterRouteRequest { route: Some(route) }))
        .await
        .unwrap();

    harness
        .client
        .unregister_route(Request::new(
            aios_renderer_web::service::proto::UnregisterRouteRequest {
                path: "/dashboard".into(),
            },
        ))
        .await
        .unwrap();

    let resp = harness
        .client
        .list_routes(Request::new(ListRoutesRequest {}))
        .await
        .unwrap();
    assert!(resp.into_inner().routes.is_empty());
}

// ── Test 10: degraded mode blocks GPU-bearing kind ────────────────────────

#[tokio::test]
async fn enter_degraded_mode_blocks_gpu_bearing_kind() {
    let mut harness = TestHarness::new().await;

    // Enter degraded mode
    harness
        .client
        .enter_degraded_mode(Request::new(
            aios_renderer_web::service::proto::EnterDegradedModeRequest {
                reason: "gpu fault".into(),
            },
        ))
        .await
        .unwrap();

    // GPU-bearing kind (Visualization has is_gpu_bearing=true)
    let req = AllocateWebSurfaceRequestProto {
        origin: Some(aios_localhost_origin()),
        node_kind: aios_renderer_web::service::proto::NodeKindProto::Visualization as i32,
        claimed_by: "test-client".into(),
        expected_group_id: None,
    };
    let status = harness
        .client
        .allocate_surface(Request::new(req))
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::ResourceExhausted);
}

// ── Test 11: apply and get active tokens round-trip ───────────────────────

#[tokio::test]
async fn apply_and_get_active_tokens_roundtrip() {
    let mut harness = TestHarness::new().await;
    let tokens = vec![VisualTokenProto {
        id: "dk.color.bg".into(),
        kind: aios_renderer_web::service::proto::VisualTokenKindProto::Color as i32,
        canonical_value: "#1a1b26".into(),
    }];
    let receipt = harness
        .client
        .apply_visual_tokens(Request::new(ApplyVisualTokensRequest {
            tokens: tokens.clone(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(receipt.applied_count, 1);

    let resp = harness
        .client
        .get_active_tokens(Request::new(GetActiveTokensRequest {}))
        .await
        .unwrap();
    let active = resp.into_inner().tokens;
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, "dk.color.bg");
    assert_eq!(active[0].canonical_value, "#1a1b26");
}

// ── Test 12: request LAN escalation RPC succeeds (FSM transition) ────────

#[tokio::test]
async fn request_lan_escalation_rpc_succeeds() {
    let mut harness = TestHarness::new().await;

    harness
        .client
        .request_lan_escalation(Request::new(
            aios_renderer_web::service::proto::RequestLanEscalationRequest {
                approver_canonical_id: "operator:root".into(),
            },
        ))
        .await
        .unwrap();
}

// ── Test 13: full LAN escalation chain RPCs all succeed ────────────────────

#[tokio::test]
async fn full_lan_escalation_chain_rpcs_all_succeed() {
    let mut harness = TestHarness::new().await;

    // 1. Request LAN
    harness
        .client
        .request_lan_escalation(Request::new(
            aios_renderer_web::service::proto::RequestLanEscalationRequest {
                approver_canonical_id: "operator:root".into(),
            },
        ))
        .await
        .unwrap();

    // 2. Apply policy decision
    harness
        .client
        .apply_policy_decision(Request::new(
            aios_renderer_web::service::proto::ApplyPolicyDecisionRequest {
                decision_id: "dec-001".into(),
            },
        ))
        .await
        .unwrap();

    // 3. Activate LAN
    harness
        .client
        .activate_lan_exposure(Request::new(
            aios_renderer_web::service::proto::ActivateLanExposureRequest {},
        ))
        .await
        .unwrap();
}

// ── Test 14: record_heartbeat succeeds while LanActive ────────────────────

#[tokio::test]
async fn record_heartbeat_succeeds_while_lan_active() {
    let mut harness = TestHarness::new().await;

    // Drive to LanActive
    harness
        .client
        .request_lan_escalation(Request::new(
            aios_renderer_web::service::proto::RequestLanEscalationRequest {
                approver_canonical_id: "operator:root".into(),
            },
        ))
        .await
        .unwrap();
    harness
        .client
        .apply_policy_decision(Request::new(
            aios_renderer_web::service::proto::ApplyPolicyDecisionRequest {
                decision_id: "dec-001".into(),
            },
        ))
        .await
        .unwrap();
    harness
        .client
        .activate_lan_exposure(Request::new(
            aios_renderer_web::service::proto::ActivateLanExposureRequest {},
        ))
        .await
        .unwrap();

    // Heartbeat should succeed
    harness
        .client
        .record_heartbeat(Request::new(
            aios_renderer_web::service::proto::RecordHeartbeatRequest {},
        ))
        .await
        .unwrap();
}

// ── Test 15: revoke exposure from LanPending succeeds ────────────────────

#[tokio::test]
async fn revoke_exposure_from_lan_pending_succeeds() {
    let mut harness = TestHarness::new().await;

    harness
        .client
        .request_lan_escalation(Request::new(
            aios_renderer_web::service::proto::RequestLanEscalationRequest {
                approver_canonical_id: "operator:root".into(),
            },
        ))
        .await
        .unwrap();

    harness
        .client
        .revoke_exposure(Request::new(
            aios_renderer_web::service::proto::RevokeExposureRequest {
                reason: "operator override".into(),
            },
        ))
        .await
        .unwrap();
}

// ── Test 16: get_mode reflects current mode ───────────────────────────────

#[tokio::test]
async fn get_mode_reflects_current_mode() {
    let mut harness = TestHarness::new().await;

    // Default is Normal
    let resp = harness
        .client
        .get_mode(Request::new(GetModeRequest {}))
        .await
        .unwrap();
    let mode = resp.into_inner().mode.unwrap();
    assert_eq!(
        mode.kind,
        aios_renderer_web::service::proto::WebRendererModeKind::ModeNormal as i32
    );

    // Enter recovery
    harness
        .client
        .enter_recovery_mode(Request::new(
            aios_renderer_web::service::proto::EnterRecoveryModeRequest {},
        ))
        .await
        .unwrap();

    let resp = harness
        .client
        .get_mode(Request::new(GetModeRequest {}))
        .await
        .unwrap();
    let mode = resp.into_inner().mode.unwrap();
    assert_eq!(
        mode.kind,
        aios_renderer_web::service::proto::WebRendererModeKind::ModeRecovery as i32
    );
}
