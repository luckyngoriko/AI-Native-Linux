//! T-140 renderer tests — `WebRenderer` trait + `InMemoryWebRenderer` (20 tests).
//!
//! Covers: surface allocation with INV I4 group verification, recovery mode
//! admission (INV I11), degraded mode GPU-bearing blocking, route CRUD,
//! visual token round-trip, and concurrent allocation.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::no_effect_underscore_binding
)]

use std::collections::HashSet;
use std::sync::Arc;

use aios_renderer_web::{
    AllocateWebSurfaceRequest, InMemoryWebRenderer, OriginScheme, RouteDescriptor, WebRenderer,
    WebRendererError, WebRendererMode, WebSurfaceFilter, WebSurfaceId,
};

use aios_renderer_kde::{NodeKind, VisualToken, VisualTokenKind};

// ── Constructor / initial state ─────────────────────────────────────────────

#[tokio::test]
async fn new_renderer_normal_mode_localhost_exposure() {
    let r = InMemoryWebRenderer::new();
    let mode = r.get_mode().await;
    let exposure = r.current_exposure().await;
    assert!(matches!(mode, WebRendererMode::Normal));
    assert!(matches!(
        exposure,
        aios_renderer_web::ExposureLevel::Localhost
    ));
}

#[tokio::test]
async fn new_renderer_empty_surfaces_routes_tokens() {
    let r = InMemoryWebRenderer::new();
    let surfaces = r.list_surfaces(WebSurfaceFilter::All).await;
    let routes = r.list_routes().await;
    let tokens = r.get_active_tokens().await;
    assert!(surfaces.is_empty());
    assert!(routes.is_empty());
    assert!(tokens.is_empty());
}

// ── Surface allocation ──────────────────────────────────────────────────────

#[tokio::test]
async fn allocate_surface_aios_localhost_origin_succeeds() {
    let r = InMemoryWebRenderer::new();
    let origin = OriginScheme::parse("https://acme-app.aios.localhost:8443").unwrap();
    let req = AllocateWebSurfaceRequest {
        origin: origin.clone(),
        node_kind: NodeKind::Container,
        claimed_by: "family:app:com.example".into(),
        expected_group_id: None,
    };
    let desc = r.allocate_surface(req).await.unwrap();
    assert_eq!(
        desc.origin.full_origin,
        "https://acme-app.aios.localhost:8443"
    );
    assert_eq!(desc.node_kind, NodeKind::Container);
    assert_eq!(desc.claimed_by, "family:app:com.example");
    assert!(matches!(desc.mode, WebRendererMode::Normal));
}

#[tokio::test]
async fn allocate_surface_matching_group_id_succeeds() {
    let r = InMemoryWebRenderer::new();
    let origin = OriginScheme::parse("https://acme-app.aios.localhost:8443").unwrap();
    let req = AllocateWebSurfaceRequest {
        origin,
        node_kind: NodeKind::Card,
        claimed_by: "family:app:com.example".into(),
        expected_group_id: Some("acme-app".into()),
    };
    let result = r.allocate_surface(req).await;
    assert!(result.is_ok(), "matching group_id must succeed");
}

#[tokio::test]
async fn allocate_surface_mismatched_group_id_returns_origin_verification_failed() {
    let r = InMemoryWebRenderer::new();
    let origin = OriginScheme::parse("https://acme-app.aios.localhost:8443").unwrap();
    let req = AllocateWebSurfaceRequest {
        origin,
        node_kind: NodeKind::Container,
        claimed_by: "family:app:com.example".into(),
        expected_group_id: Some("other-group".into()),
    };
    let result = r.allocate_surface(req).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        WebRendererError::OriginVerificationFailed {
            expected_group_id,
            presented_origin,
        } => {
            assert_eq!(expected_group_id, "other-group");
            assert!(presented_origin.contains("acme-app"));
        }
        other => panic!("expected OriginVerificationFailed, got {other:?}"),
    }
}

// ── get_surface ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_surface_known_returns_descriptor() {
    let r = InMemoryWebRenderer::new();
    let origin = OriginScheme::parse("https://app-1.aios.localhost:8443").unwrap();
    let req = AllocateWebSurfaceRequest {
        origin,
        node_kind: NodeKind::Form,
        claimed_by: "app-1".into(),
        expected_group_id: None,
    };
    let allocated = r.allocate_surface(req).await.unwrap();
    let got = r.get_surface(allocated.id.clone()).await.unwrap();
    assert_eq!(got.id, allocated.id);
    assert_eq!(got.claimed_by, "app-1");
}

#[tokio::test]
async fn get_surface_unknown_returns_surface_not_found() {
    let r = InMemoryWebRenderer::new();
    let unknown_id = WebSurfaceId::new();
    let result = r.get_surface(unknown_id.clone()).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        WebRendererError::SurfaceNotFound(id) => assert_eq!(id.0, unknown_id.0),
        other => panic!("expected SurfaceNotFound, got {other:?}"),
    }
}

// ── release_surface ─────────────────────────────────────────────────────────

#[tokio::test]
async fn release_surface_known_returns_receipt() {
    let r = InMemoryWebRenderer::new();
    let origin = OriginScheme::parse("https://app-2.aios.localhost:8443").unwrap();
    let req = AllocateWebSurfaceRequest {
        origin,
        node_kind: NodeKind::Text,
        claimed_by: "app-2".into(),
        expected_group_id: None,
    };
    let allocated = r.allocate_surface(req).await.unwrap();
    let receipt = r.release_surface(allocated.id.clone()).await.unwrap();
    assert_eq!(receipt.id, allocated.id);
    assert!(matches!(receipt.final_mode, WebRendererMode::Normal));
    // Surface gone after release
    let get_result = r.get_surface(allocated.id).await;
    assert!(get_result.is_err());
}

#[tokio::test]
async fn release_surface_unknown_returns_surface_not_found() {
    let r = InMemoryWebRenderer::new();
    let unknown_id = WebSurfaceId::new();
    let result = r.release_surface(unknown_id.clone()).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        WebRendererError::SurfaceNotFound(id) => assert_eq!(id.0, unknown_id.0),
        other => panic!("expected SurfaceNotFound, got {other:?}"),
    }
}

// ── list_surfaces filters ───────────────────────────────────────────────────

#[tokio::test]
async fn list_surfaces_all_after_3_allocations() {
    let r = InMemoryWebRenderer::new();
    for i in 0..3 {
        let origin = OriginScheme::parse(&format!("https://app-{i}.aios.localhost:8443")).unwrap();
        let req = AllocateWebSurfaceRequest {
            origin,
            node_kind: NodeKind::Container,
            claimed_by: format!("app-{i}"),
            expected_group_id: None,
        };
        r.allocate_surface(req).await.unwrap();
    }
    let all = r.list_surfaces(WebSurfaceFilter::All).await;
    assert_eq!(all.len(), 3);
}

#[tokio::test]
async fn list_surfaces_by_origin_filters_correctly() {
    let r = InMemoryWebRenderer::new();
    let origin_a = OriginScheme::parse("https://app-a.aios.localhost:8443").unwrap();
    let origin_b = OriginScheme::parse("https://app-b.aios.localhost:8443").unwrap();

    r.allocate_surface(AllocateWebSurfaceRequest {
        origin: origin_a.clone(),
        node_kind: NodeKind::Container,
        claimed_by: "a".into(),
        expected_group_id: None,
    })
    .await
    .unwrap();
    r.allocate_surface(AllocateWebSurfaceRequest {
        origin: origin_b,
        node_kind: NodeKind::Container,
        claimed_by: "b".into(),
        expected_group_id: None,
    })
    .await
    .unwrap();

    let filtered = r
        .list_surfaces(WebSurfaceFilter::ByOrigin(
            "https://app-a.aios.localhost:8443".into(),
        ))
        .await;
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].claimed_by, "a");
}

#[tokio::test]
async fn list_surfaces_by_node_kind_filters_correctly() {
    let r = InMemoryWebRenderer::new();
    let origin = OriginScheme::parse("https://app-k.aios.localhost:8443").unwrap();

    r.allocate_surface(AllocateWebSurfaceRequest {
        origin: origin.clone(),
        node_kind: NodeKind::Container,
        claimed_by: "k1".into(),
        expected_group_id: None,
    })
    .await
    .unwrap();
    r.allocate_surface(AllocateWebSurfaceRequest {
        origin,
        node_kind: NodeKind::Card,
        claimed_by: "k2".into(),
        expected_group_id: None,
    })
    .await
    .unwrap();

    let containers = r
        .list_surfaces(WebSurfaceFilter::ByNodeKind(NodeKind::Container))
        .await;
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].node_kind, NodeKind::Container);
}

// ── Route CRUD ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn register_then_list_routes_returns_1() {
    let r = InMemoryWebRenderer::new();
    let route = RouteDescriptor {
        path: "/api/action".into(),
        requires_auth: true,
        served_in_recovery: false,
    };
    r.register_route(route).await.unwrap();
    let routes = r.list_routes().await;
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].path, "/api/action");
}

#[tokio::test]
async fn unregister_known_route_ok() {
    let r = InMemoryWebRenderer::new();
    let route = RouteDescriptor {
        path: "/dashboard".into(),
        requires_auth: true,
        served_in_recovery: true,
    };
    r.register_route(route).await.unwrap();
    r.unregister_route("/dashboard").await.unwrap();
    let routes = r.list_routes().await;
    assert!(routes.is_empty());
}

#[tokio::test]
async fn unregister_unknown_route_internal_error() {
    let r = InMemoryWebRenderer::new();
    let result = r.unregister_route("/nonexistent").await;
    assert!(result.is_err());
    match result.unwrap_err() {
        WebRendererError::Internal(msg) => {
            assert!(msg.contains("/nonexistent"));
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

// ── Recovery mode ───────────────────────────────────────────────────────────

#[tokio::test]
async fn enter_recovery_mode_returns_receipt_with_recovery_origin_and_service_worker_disabled() {
    let r = InMemoryWebRenderer::new();
    let receipt = r.enter_recovery_mode().await.unwrap();
    assert_eq!(receipt.recovery_origin, "https://recovery.localhost:8443");
    assert!(receipt.service_worker_disabled);
    let mode = r.get_mode().await;
    assert!(matches!(mode, WebRendererMode::Recovery));
}

#[tokio::test]
async fn in_recovery_non_recovery_origin_rejected() {
    let r = InMemoryWebRenderer::new();
    r.enter_recovery_mode().await.unwrap();

    let origin = OriginScheme::parse("https://acme-app.aios.localhost:8443").unwrap();
    let req = AllocateWebSurfaceRequest {
        origin,
        node_kind: NodeKind::Container,
        claimed_by: "app".into(),
        expected_group_id: None,
    };
    let result = r.allocate_surface(req).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        WebRendererError::Internal(msg) => {
            assert!(msg.contains("recovery mode"));
        }
        other => panic!("expected Internal recovery rejection, got {other:?}"),
    }
}

// ── Degraded mode ───────────────────────────────────────────────────────────

#[tokio::test]
async fn enter_degraded_blocks_gpu_bearing_kinds_with_webgpu_unavailable() {
    let r = InMemoryWebRenderer::new();
    r.enter_degraded_mode("webgpu_init_failed".into())
        .await
        .unwrap();

    let origin = OriginScheme::parse("https://viz.aios.localhost:8443").unwrap();
    let req = AllocateWebSurfaceRequest {
        origin,
        node_kind: NodeKind::Visualization,
        claimed_by: "viz".into(),
        expected_group_id: None,
    };
    let result = r.allocate_surface(req).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        WebRendererError::WebgpuAdapterUnavailable(msg) => {
            assert!(msg.contains("degraded"));
        }
        other => panic!("expected WebgpuAdapterUnavailable, got {other:?}"),
    }
}

// ── Visual tokens ───────────────────────────────────────────────────────────

#[tokio::test]
async fn apply_visual_tokens_round_trip() {
    let r = InMemoryWebRenderer::new();
    let tokens = vec![
        VisualToken {
            id: "COLOR_ACTION_AI".into(),
            kind: VisualTokenKind::Color,
            canonical_value: "#00FF00".into(),
        },
        VisualToken {
            id: "FONT_UI".into(),
            kind: VisualTokenKind::Font,
            canonical_value: "Inter".into(),
        },
    ];
    let receipt = r.apply_visual_tokens(tokens.clone()).await.unwrap();
    assert_eq!(receipt.applied_count, 2);
    let active = r.get_active_tokens().await;
    assert_eq!(active.len(), 2);
    assert_eq!(active[0].id, "COLOR_ACTION_AI");
    assert_eq!(active[1].canonical_value, "Inter");
}

// ── Concurrent allocation ───────────────────────────────────────────────────

#[tokio::test]
async fn concurrent_allocate_3_distinct_ids() {
    let renderer = Arc::new(InMemoryWebRenderer::new());
    let mut handles = vec![];
    for i in 0..3 {
        let r = Arc::clone(&renderer);
        handles.push(tokio::spawn(async move {
            let origin =
                OriginScheme::parse(&format!("https://concurrent-{i}.aios.localhost:8443"))
                    .unwrap();
            let req = AllocateWebSurfaceRequest {
                origin,
                node_kind: NodeKind::Container,
                claimed_by: format!("concurrent-{i}"),
                expected_group_id: None,
            };
            r.allocate_surface(req).await.unwrap()
        }));
    }
    let mut ids = HashSet::new();
    for handle in handles {
        let desc = handle.await.unwrap();
        ids.insert(desc.id);
    }
    assert_eq!(
        ids.len(),
        3,
        "concurrent allocations must produce 3 distinct ids"
    );
}
