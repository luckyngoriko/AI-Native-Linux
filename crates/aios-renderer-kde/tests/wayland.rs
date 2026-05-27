//! T-131 wayland tests — 16 integration tests covering Wayland surface model,
//! INV I4 wlr-layer-shell enforcement, and `WaylandClient` grant tracking (S7.4 §3.1).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::no_effect_underscore_binding
)]

use std::sync::Arc;

use aios_renderer_kde::{
    evaluate_surface_request, CompositionZone, KdeRendererError, KdeSurfaceId, NodeKind,
    WaylandClient, WaylandInteractivity, WaylandProtocol, WaylandSurfaceLayer,
    WaylandSurfaceRequest,
};
use tokio::sync::Barrier;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn chrome_request(claimed_by: &str) -> WaylandSurfaceRequest {
    WaylandSurfaceRequest {
        protocol: WaylandProtocol::XdgShell,
        layer_namespace: "aios-chrome".into(),
        claimed_by: claimed_by.into(),
        zone: CompositionZone::Chrome,
        node_kind: NodeKind::SecurityIndicator,
    }
}

fn content_request() -> WaylandSurfaceRequest {
    WaylandSurfaceRequest {
        protocol: WaylandProtocol::XdgShell,
        layer_namespace: "app".into(),
        claimed_by: "any-client".into(),
        zone: CompositionZone::Content,
        node_kind: NodeKind::Card,
    }
}

fn background_request() -> WaylandSurfaceRequest {
    WaylandSurfaceRequest {
        protocol: WaylandProtocol::XdgShell,
        layer_namespace: "desktop".into(),
        claimed_by: "aios-desktop".into(),
        zone: CompositionZone::Background,
        node_kind: NodeKind::Container,
    }
}

fn recovery_request() -> WaylandSurfaceRequest {
    WaylandSurfaceRequest {
        protocol: WaylandProtocol::WlrLayerShellV1,
        layer_namespace: "aios-recovery".into(),
        claimed_by: "aios-recovery".into(),
        zone: CompositionZone::Recovery,
        node_kind: NodeKind::Container,
    }
}

fn layer_shell_request(zone: CompositionZone, claimed_by: &str) -> WaylandSurfaceRequest {
    WaylandSurfaceRequest {
        protocol: WaylandProtocol::WlrLayerShellV1,
        layer_namespace: "test".into(),
        claimed_by: claimed_by.into(),
        zone,
        node_kind: NodeKind::Container,
    }
}

// ── evaluate_surface_request tests ──────────────────────────────────────────

#[test]
fn evaluate_chrome_zone_for_aios_chrome_returns_overlay() {
    let req = chrome_request("aios-chrome");
    let grant = evaluate_surface_request(&req).expect("aios-chrome on chrome zone must succeed");
    assert_eq!(grant.assigned_layer, WaylandSurfaceLayer::Overlay);
    assert_eq!(grant.interactivity, WaylandInteractivity::OnDemand);
    assert_eq!(grant.exclusive_zone, 0);
}

#[test]
fn evaluate_chrome_zone_for_non_aios_chrome_returns_overlay_forbidden() {
    let req = chrome_request("family:app:com.example");
    let err = evaluate_surface_request(&req).unwrap_err();
    match err {
        KdeRendererError::OverlayLayerForbidden { client_id } => {
            assert_eq!(client_id, "family:app:com.example");
        }
        other => panic!("expected OverlayLayerForbidden, got {other:?}"),
    }
}

#[test]
fn evaluate_content_zone_returns_top_layer() {
    let req = content_request();
    let grant = evaluate_surface_request(&req).expect("content zone must succeed");
    assert_eq!(grant.assigned_layer, WaylandSurfaceLayer::Top);
    assert_eq!(grant.interactivity, WaylandInteractivity::OnDemand);
    assert_eq!(grant.exclusive_zone, 0);
}

#[test]
fn evaluate_background_zone_returns_background_layer_and_none_interactivity() {
    let req = background_request();
    let grant = evaluate_surface_request(&req).expect("background zone must succeed");
    assert_eq!(grant.assigned_layer, WaylandSurfaceLayer::Background);
    assert_eq!(grant.interactivity, WaylandInteractivity::None);
    assert_eq!(grant.exclusive_zone, 0);
}

#[test]
fn evaluate_recovery_zone_returns_overlay_and_exclusive_interactivity() {
    let req = recovery_request();
    let grant = evaluate_surface_request(&req).expect("recovery zone must succeed");
    assert_eq!(grant.assigned_layer, WaylandSurfaceLayer::Overlay);
    assert_eq!(grant.interactivity, WaylandInteractivity::Exclusive);
    assert_eq!(grant.exclusive_zone, 0);
}

#[test]
fn evaluate_layer_shell_protocol_on_content_zone_returns_internal_error() {
    let req = layer_shell_request(CompositionZone::Content, "any-client");
    let err = evaluate_surface_request(&req).unwrap_err();
    match err {
        KdeRendererError::Internal(msg) => {
            assert!(msg.contains("wlr-layer-shell"), "unexpected message: {msg}");
        }
        other => panic!("expected Internal error, got {other:?}"),
    }
}

#[test]
fn evaluate_layer_shell_protocol_on_chrome_zone_succeeds() {
    let req = layer_shell_request(CompositionZone::Chrome, "aios-chrome");
    let grant = evaluate_surface_request(&req).expect("wlr-layer-shell on chrome must succeed");
    assert_eq!(grant.assigned_layer, WaylandSurfaceLayer::Overlay);
}

#[test]
fn evaluate_layer_shell_protocol_on_recovery_zone_succeeds() {
    let req = layer_shell_request(CompositionZone::Recovery, "aios-recovery");
    let grant = evaluate_surface_request(&req).expect("wlr-layer-shell on recovery must succeed");
    assert_eq!(grant.assigned_layer, WaylandSurfaceLayer::Overlay);
}

// ── WaylandClient async tests ───────────────────────────────────────────────

#[tokio::test]
async fn wayland_client_connect_empty_name_fails() {
    let result = WaylandClient::connect("").await;
    assert!(result.is_err());
    match result.unwrap_err() {
        KdeRendererError::WaylandConnectError(msg) => {
            assert!(msg.contains("empty"), "unexpected message: {msg}");
        }
        other => panic!("expected WaylandConnectError, got {other:?}"),
    }
}

#[tokio::test]
async fn wayland_client_request_surface_grants_chrome_overlay() {
    let client = WaylandClient::connect("wayland-0").await.expect("connect");
    let id = KdeSurfaceId::new();
    let grant = client
        .request_surface(id, chrome_request("aios-chrome"))
        .await
        .expect("aios-chrome on chrome zone must succeed");
    assert_eq!(grant.assigned_layer, WaylandSurfaceLayer::Overlay);
    assert_eq!(grant.interactivity, WaylandInteractivity::OnDemand);
}

#[tokio::test]
async fn wayland_client_request_surface_non_chrome_overlay_forbidden() {
    let client = WaylandClient::connect("wayland-0").await.expect("connect");
    let id = KdeSurfaceId::new();
    let err = client
        .request_surface(id, chrome_request("intruder"))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        KdeRendererError::OverlayLayerForbidden { .. }
    ));
}

#[tokio::test]
async fn wayland_client_revoke_known_surface_succeeds() {
    let client = WaylandClient::connect("wayland-0").await.expect("connect");
    let id = KdeSurfaceId::new();
    client
        .request_surface(id.clone(), content_request())
        .await
        .expect("request");
    client.revoke_surface(&id).await.expect("revoke");
}

#[tokio::test]
async fn wayland_client_revoke_unknown_surface_returns_surface_not_found() {
    let client = WaylandClient::connect("wayland-0").await.expect("connect");
    let unknown = KdeSurfaceId::new();
    let err = client.revoke_surface(&unknown).await.unwrap_err();
    assert!(matches!(err, KdeRendererError::SurfaceNotFound(_)));
}

#[tokio::test]
async fn wayland_client_list_grants_returns_all_after_3_requests() {
    let client = WaylandClient::connect("wayland-0").await.expect("connect");
    let ids: Vec<KdeSurfaceId> = (0..3).map(|_| KdeSurfaceId::new()).collect();

    client
        .request_surface(ids[0].clone(), content_request())
        .await
        .expect("req 1");
    client
        .request_surface(ids[1].clone(), background_request())
        .await
        .expect("req 2");
    client
        .request_surface(ids[2].clone(), recovery_request())
        .await
        .expect("req 3");

    let grants = client.list_grants().await;
    assert_eq!(grants.len(), 3, "3 grants expected");
    // Sorted by KdeSurfaceId string.
    for i in 1..grants.len() {
        assert!(
            grants[i - 1].0 .0 <= grants[i].0 .0,
            "grants must be sorted"
        );
    }
}

#[tokio::test]
async fn concurrent_request_3_distinct_surfaces_no_panic() {
    let client = Arc::new(WaylandClient::connect("wayland-0").await.expect("connect"));
    let barrier = Arc::new(Barrier::new(3));

    let mut handles = vec![];
    for _ in 0..3 {
        let c = Arc::clone(&client);
        let b = Arc::clone(&barrier);
        handles.push(tokio::spawn(async move {
            b.wait().await;
            let id = KdeSurfaceId::new();
            c.request_surface(id, content_request()).await
        }));
    }

    for h in handles {
        let result = h.await.expect("task join");
        assert!(result.is_ok(), "concurrent request must succeed");
    }
}

// ── WaylandProtocol set invariant ───────────────────────────────────────────

#[test]
fn wayland_protocol_set_has_exactly_7_variants() {
    assert_eq!(
        WaylandProtocol::LEN,
        7,
        "S7.4 §3.1 declares exactly 7 WaylandProtocol variants"
    );
    assert_eq!(WaylandProtocol::ALL.len(), 7);
    let mut seen = std::collections::HashSet::new();
    for proto in WaylandProtocol::ALL {
        assert!(
            seen.insert(*proto),
            "duplicate WaylandProtocol variant: {proto:?}"
        );
    }
}
