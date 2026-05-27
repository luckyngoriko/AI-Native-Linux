//! Integration tests for the gRPC `KdeRendererService` surface (T-134).
//!
//! Each test boots an in-process tonic server backed by the in-memory renderer,
//! wayland client, and KWin script loader, connects via a TCP listener, and
//! exercises one RPC path.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::doc_markdown,
    clippy::cast_possible_wrap,
    clippy::significant_drop_tightening,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;
use std::time::Duration;

use tonic::Request;

use aios_renderer_kde::renderer::InMemoryKdeRenderer;
use aios_renderer_kde::service::proto::kde_renderer_service_client::KdeRendererServiceClient;
use aios_renderer_kde::service::proto::{
    AllocateSurfaceRequestProto, ApplyVisualTokensRequest, CompositionZoneProto,
    EnterDegradedModeRequest, EnterRecoveryModeRequest, EvaluateWaylandSurfaceRequest,
    GetActiveTokensRequest, GetModeRequest, GetSurfaceRequest, KdeRendererModeKind,
    ListKwinScriptsRequest, ListSurfacesRequest, LoadKwinScriptRequest, NodeKindProto,
    ReleaseSurfaceRequest, SurfaceFilterProto, VisualTokenProto, WaylandProtocolProto,
    WaylandSurfaceRequestProto,
};
use aios_renderer_kde::service::{build_router, KdeRendererServer};
use aios_renderer_kde::wayland::WaylandClient;

// ── Test harness ─────────────────────────────────────────────────────────

struct TestHarness {
    client: KdeRendererServiceClient<tonic::transport::Channel>,
}

impl TestHarness {
    async fn new() -> Self {
        let renderer = Arc::new(InMemoryKdeRenderer::new());
        let wayland = Arc::new(
            WaylandClient::connect("wayland-0")
                .await
                .expect("wayland connect"),
        );
        let kwin_loader = Arc::new(aios_renderer_kde::kwin_script::KwinScriptLoader::default());

        let svc = KdeRendererServer::new(
            renderer as Arc<dyn aios_renderer_kde::renderer::KdeRenderer>,
            wayland,
            kwin_loader,
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

        let client = KdeRendererServiceClient::connect(format!("http://{addr}"))
            .await
            .unwrap();

        Self { client }
    }
}

// ── Test 1: server boots over in-memory transport ────────────────────────

#[tokio::test]
async fn server_boots() {
    let _harness = TestHarness::new().await;
}

// ── Test 2: AllocateSurface chrome zone for aios-chrome succeeds ──────────

#[tokio::test]
async fn allocate_surface_chrome_aios_chrome_succeeds() {
    let mut harness = TestHarness::new().await;
    let req = AllocateSurfaceRequestProto {
        zone: CompositionZoneProto::Chrome as i32,
        claimed_by: "aios-chrome".into(),
        node_kind: NodeKindProto::SecurityIndicator as i32,
        requested_layer: None,
    };
    let resp = harness
        .client
        .allocate_surface(Request::new(req))
        .await
        .unwrap();
    let desc = resp.into_inner();
    assert!(!desc.id.is_empty());
    assert_eq!(desc.claimed_by, "aios-chrome");
}

// ── Test 3: AllocateSurface chrome zone for non-aios-chrome → denied ─────

#[tokio::test]
async fn allocate_surface_chrome_non_aios_chrome_denied() {
    let mut harness = TestHarness::new().await;
    let req = AllocateSurfaceRequestProto {
        zone: CompositionZoneProto::Chrome as i32,
        claimed_by: "evil-client".into(),
        node_kind: NodeKindProto::SecurityIndicator as i32,
        requested_layer: None,
    };
    let status = harness
        .client
        .allocate_surface(Request::new(req))
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

// ── Test 4: GetSurface round-trip after Allocate ──────────────────────────

#[tokio::test]
async fn get_surface_roundtrip() {
    let mut harness = TestHarness::new().await;
    let req = AllocateSurfaceRequestProto {
        zone: CompositionZoneProto::Content as i32,
        claimed_by: "test-client".into(),
        node_kind: NodeKindProto::Container as i32,
        requested_layer: None,
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
    assert_eq!(get_resp.into_inner().id, allocated_id);
}

// ── Test 5: GetSurface unknown → not_found ────────────────────────────────

#[tokio::test]
async fn get_surface_unknown_not_found() {
    let mut harness = TestHarness::new().await;
    let status = harness
        .client
        .get_surface(Request::new(GetSurfaceRequest {
            id: "01JNONEXISTENT".into(),
        }))
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

// ── Test 6: ReleaseSurface known → receipt ────────────────────────────────

#[tokio::test]
async fn release_surface_known_receipt() {
    let mut harness = TestHarness::new().await;
    let req = AllocateSurfaceRequestProto {
        zone: CompositionZoneProto::Content as i32,
        claimed_by: "test-client".into(),
        node_kind: NodeKindProto::Container as i32,
        requested_layer: None,
    };
    let allocated = harness
        .client
        .allocate_surface(Request::new(req))
        .await
        .unwrap();
    let surface_id = allocated.into_inner().id;

    let receipt = harness
        .client
        .release_surface(Request::new(ReleaseSurfaceRequest {
            id: surface_id.clone(),
        }))
        .await
        .unwrap();
    let r = receipt.into_inner();
    assert_eq!(r.id, surface_id);
    assert!(r.released_at.is_some());
}

// ── Test 7: ReleaseSurface unknown → not_found ────────────────────────────

#[tokio::test]
async fn release_surface_unknown_not_found() {
    let mut harness = TestHarness::new().await;
    let status = harness
        .client
        .release_surface(Request::new(ReleaseSurfaceRequest {
            id: "01JNONEXISTENT".into(),
        }))
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

// ── Test 8: ListSurfaces after 3 Allocates returns 3 entries ──────────────

#[tokio::test]
async fn list_surfaces_after_3_allocates() {
    let mut harness = TestHarness::new().await;
    for i in 0..3 {
        let req = AllocateSurfaceRequestProto {
            zone: CompositionZoneProto::Content as i32,
            claimed_by: format!("client-{i}"),
            node_kind: NodeKindProto::Card as i32,
            requested_layer: None,
        };
        harness
            .client
            .allocate_surface(Request::new(req))
            .await
            .unwrap();
    }

    let resp = harness
        .client
        .list_surfaces(Request::new(ListSurfacesRequest {
            filter: Some(SurfaceFilterProto {
                filter: Some(
                    aios_renderer_kde::service::proto::surface_filter_proto::Filter::All(()),
                ),
            }),
        }))
        .await
        .unwrap();
    assert_eq!(resp.into_inner().surfaces.len(), 3);
}

// ── Test 9: EnterRecoveryMode → GetMode returns Recovery ──────────────────

#[tokio::test]
async fn enter_recovery_mode_get_mode() {
    let mut harness = TestHarness::new().await;
    harness
        .client
        .enter_recovery_mode(Request::new(EnterRecoveryModeRequest {}))
        .await
        .unwrap();

    let mode_resp = harness
        .client
        .get_mode(Request::new(GetModeRequest {}))
        .await
        .unwrap();
    let mode = mode_resp.into_inner().mode.unwrap();
    assert_eq!(mode.kind, KdeRendererModeKind::ModeRecovery as i32);
}

// ── Test 10: EnterDegradedMode with reason → GetMode returns Degraded ─────

#[tokio::test]
async fn enter_degraded_mode_get_mode() {
    let mut harness = TestHarness::new().await;
    harness
        .client
        .enter_degraded_mode(Request::new(EnterDegradedModeRequest {
            reason: "kwin_unreachable".into(),
        }))
        .await
        .unwrap();

    let mode_resp = harness
        .client
        .get_mode(Request::new(GetModeRequest {}))
        .await
        .unwrap();
    let mode = mode_resp.into_inner().mode.unwrap();
    assert_eq!(mode.kind, KdeRendererModeKind::ModeDegraded as i32);
    assert_eq!(mode.degraded_reason, "kwin_unreachable");
}

// ── Test 11: ApplyVisualTokens + GetActiveTokens round-trip ───────────────

#[tokio::test]
async fn apply_get_active_tokens_roundtrip() {
    let mut harness = TestHarness::new().await;
    let token = VisualTokenProto {
        id: "COLOR_ACTION_AI".into(),
        kind: aios_renderer_kde::service::proto::VisualTokenKindProto::Color as i32,
        canonical_value: "#FF5500".into(),
    };

    harness
        .client
        .apply_visual_tokens(Request::new(ApplyVisualTokensRequest {
            tokens: vec![token.clone()],
        }))
        .await
        .unwrap();

    let resp = harness
        .client
        .get_active_tokens(Request::new(GetActiveTokensRequest {}))
        .await
        .unwrap();
    let tokens = resp.into_inner().tokens;
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].id, "COLOR_ACTION_AI");
    assert_eq!(tokens[0].canonical_value, "#FF5500");
}

// ── Test 12: EvaluateWaylandSurface chrome aios-chrome → Overlay ──────────

#[tokio::test]
async fn evaluate_wayland_surface_chrome_aios_chrome_overlay() {
    let mut harness = TestHarness::new().await;
    let req = EvaluateWaylandSurfaceRequest {
        request: Some(WaylandSurfaceRequestProto {
            protocol: WaylandProtocolProto::WlrLayerShellV1 as i32,
            layer_namespace: "aios-chrome".into(),
            claimed_by: "aios-chrome".into(),
            zone: CompositionZoneProto::Chrome as i32,
            node_kind: NodeKindProto::SecurityIndicator as i32,
        }),
    };
    let resp = harness
        .client
        .evaluate_wayland_surface(Request::new(req))
        .await
        .unwrap();
    let grant = resp.into_inner();
    assert_eq!(
        grant.assigned_layer,
        aios_renderer_kde::service::proto::WaylandSurfaceLayerProto::WslOverlay as i32
    );
}

// ── Test 13: LoadKwinScript invalid path → permission_denied ──────────────

#[tokio::test]
async fn load_kwin_script_invalid_path_denied() {
    let mut harness = TestHarness::new().await;
    let script = aios_renderer_kde::service::proto::KwinScriptProto {
        id: "bad-script".into(),
        canonical_path: "/tmp/evil.js".into(),
        source: "console.log('hi')".into(),
        blake3_hash: "deadbeef".into(),
        signature: vec![0u8; 64],
        signer_key_fingerprint: "unknown".into(),
    };
    let status = harness
        .client
        .load_kwin_script(Request::new(LoadKwinScriptRequest {
            script: Some(script),
        }))
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

// ── Test 14: ListKwinScripts empty initially ──────────────────────────────

#[tokio::test]
async fn list_kwin_scripts_empty_initially() {
    let mut harness = TestHarness::new().await;
    let resp = harness
        .client
        .list_kwin_scripts(Request::new(ListKwinScriptsRequest {}))
        .await
        .unwrap();
    assert!(resp.into_inner().script_ids.is_empty());
}
