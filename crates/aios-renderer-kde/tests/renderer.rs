//! T-128 renderer integration tests — 16 tests covering the `KdeRenderer` trait
//! and `InMemoryKdeRenderer` implementation per S7.4 §4.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    reason = "test code; panic-on-failure is idiomatic"
)]

use aios_renderer_kde::{
    AllocateSurfaceRequest, CompositionZone, InMemoryKdeRenderer, KdeRenderer, KdeSurfaceId,
    NodeKind, RendererMode, SurfaceFilter, VisualToken, VisualTokenKind, ZoneLayer,
};

fn make_request(
    zone: CompositionZone,
    claimed_by: &str,
    node_kind: NodeKind,
) -> AllocateSurfaceRequest {
    AllocateSurfaceRequest {
        zone,
        claimed_by: claimed_by.to_string(),
        node_kind,
        requested_layer: None,
    }
}

// ── Mode + empty state ────────────────────────────────────────────────────────

#[tokio::test]
async fn new_renderer_is_in_normal_mode_with_no_surfaces() {
    let r = InMemoryKdeRenderer::new();
    assert_eq!(r.get_mode().await, RendererMode::Normal);
    assert!(r.list_surfaces(SurfaceFilter::All).await.is_empty());
}

// ── INV I4 — chrome zone guard ────────────────────────────────────────────────

#[tokio::test]
async fn allocate_surface_in_chrome_zone_for_aios_chrome_succeeds() {
    let r = InMemoryKdeRenderer::new();
    let req = make_request(
        CompositionZone::Chrome,
        "aios-chrome",
        NodeKind::SecurityIndicator,
    );
    let desc = r.allocate_surface(req).await.unwrap();
    assert_eq!(desc.zone, CompositionZone::Chrome);
    assert_eq!(desc.layer, ZoneLayer::Overlay);
    assert_eq!(desc.claimed_by, "aios-chrome");
}

#[tokio::test]
async fn allocate_surface_in_chrome_zone_for_other_claimant_returns_overlay_forbidden() {
    let r = InMemoryKdeRenderer::new();
    let req = make_request(
        CompositionZone::Chrome,
        "family:app:com.example",
        NodeKind::Text,
    );
    let result = r.allocate_surface(req).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(
            err,
            aios_renderer_kde::KdeRendererError::OverlayLayerForbidden { .. }
        ),
        "expected OverlayLayerForbidden, got {err:?}"
    );
}

#[tokio::test]
async fn allocate_surface_content_zone_returns_top_layer() {
    let r = InMemoryKdeRenderer::new();
    let req = make_request(CompositionZone::Content, "any-client", NodeKind::Text);
    let desc = r.allocate_surface(req).await.unwrap();
    assert_eq!(desc.zone, CompositionZone::Content);
    assert_eq!(desc.layer, ZoneLayer::Top);
}

// ── get_surface ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_surface_known_returns_descriptor() {
    let r = InMemoryKdeRenderer::new();
    let req = make_request(CompositionZone::Content, "test-client", NodeKind::Card);
    let allocated = r.allocate_surface(req).await.unwrap();
    let found = r.get_surface(allocated.id.clone()).await.unwrap();
    assert_eq!(found.id, allocated.id);
    assert_eq!(found.zone, CompositionZone::Content);
}

#[tokio::test]
async fn get_surface_unknown_returns_surface_not_found() {
    let r = InMemoryKdeRenderer::new();
    let fake_id = KdeSurfaceId::new();
    let result = r.get_surface(fake_id.clone()).await;
    assert!(result.is_err());
    assert!(
        matches!(
            result.unwrap_err(),
            aios_renderer_kde::KdeRendererError::SurfaceNotFound(ref id) if id == &fake_id
        ),
        "expected SurfaceNotFound"
    );
}

// ── release_surface ───────────────────────────────────────────────────────────

#[tokio::test]
async fn release_surface_known_returns_receipt() {
    let r = InMemoryKdeRenderer::new();
    let req = make_request(CompositionZone::Content, "test-client", NodeKind::Text);
    let allocated = r.allocate_surface(req).await.unwrap();
    let receipt = r.release_surface(allocated.id.clone()).await.unwrap();
    assert_eq!(receipt.id, allocated.id);
    assert_eq!(receipt.final_mode, RendererMode::Normal);
    // Surface is gone after release.
    assert!(r.get_surface(allocated.id).await.is_err());
}

#[tokio::test]
async fn release_surface_unknown_returns_surface_not_found() {
    let r = InMemoryKdeRenderer::new();
    let fake_id = KdeSurfaceId::new();
    let result = r.release_surface(fake_id.clone()).await;
    assert!(result.is_err());
    assert!(
        matches!(
            result.unwrap_err(),
            aios_renderer_kde::KdeRendererError::SurfaceNotFound(ref id) if id == &fake_id
        ),
        "expected SurfaceNotFound"
    );
}

// ── list_surfaces ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_surfaces_all_after_3_allocations_returns_3() {
    let r = InMemoryKdeRenderer::new();
    let _a = r
        .allocate_surface(make_request(CompositionZone::Content, "c1", NodeKind::Text))
        .await
        .unwrap();
    let _b = r
        .allocate_surface(make_request(
            CompositionZone::Background,
            "c2",
            NodeKind::Card,
        ))
        .await
        .unwrap();
    let _c = r
        .allocate_surface(make_request(CompositionZone::Content, "c3", NodeKind::List))
        .await
        .unwrap();
    let all = r.list_surfaces(SurfaceFilter::All).await;
    assert_eq!(all.len(), 3);
}

#[tokio::test]
async fn list_surfaces_by_zone_filters_correctly() {
    let r = InMemoryKdeRenderer::new();
    let _a = r
        .allocate_surface(make_request(CompositionZone::Content, "c1", NodeKind::Text))
        .await
        .unwrap();
    let _b = r
        .allocate_surface(make_request(
            CompositionZone::Background,
            "c2",
            NodeKind::Card,
        ))
        .await
        .unwrap();
    let content = r
        .list_surfaces(SurfaceFilter::ByZone(CompositionZone::Content))
        .await;
    assert_eq!(content.len(), 1);
    let bg = r
        .list_surfaces(SurfaceFilter::ByZone(CompositionZone::Background))
        .await;
    assert_eq!(bg.len(), 1);
}

#[tokio::test]
async fn list_surfaces_by_node_kind_filters_correctly() {
    let r = InMemoryKdeRenderer::new();
    let _a = r
        .allocate_surface(make_request(CompositionZone::Content, "c1", NodeKind::Text))
        .await
        .unwrap();
    let _b = r
        .allocate_surface(make_request(CompositionZone::Content, "c2", NodeKind::Card))
        .await
        .unwrap();
    let _c = r
        .allocate_surface(make_request(CompositionZone::Content, "c3", NodeKind::Text))
        .await
        .unwrap();
    let texts = r
        .list_surfaces(SurfaceFilter::ByNodeKind(NodeKind::Text))
        .await;
    assert_eq!(texts.len(), 2);
    let cards = r
        .list_surfaces(SurfaceFilter::ByNodeKind(NodeKind::Card))
        .await;
    assert_eq!(cards.len(), 1);
}

// ── recovery mode (INV I5) ────────────────────────────────────────────────────

#[tokio::test]
async fn enter_recovery_mode_sets_mode_recovery() {
    let r = InMemoryKdeRenderer::new();
    let receipt = r.enter_recovery_mode().await.unwrap();
    assert!(receipt.aios_surfaces_only);
    assert_eq!(receipt.display_separation, "separate-wayland-display");
    assert_eq!(r.get_mode().await, RendererMode::Recovery);
}

#[tokio::test]
async fn in_recovery_mode_allocate_non_aios_surface_kind_is_rejected() {
    let r = InMemoryKdeRenderer::new();
    r.enter_recovery_mode().await.unwrap();

    let req = make_request(CompositionZone::Content, "test", NodeKind::Text);
    let result = r.allocate_surface(req).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(&err, aios_renderer_kde::KdeRendererError::Internal(msg) if msg.contains("AIOS_SURFACE")),
        "expected Internal containing AIOS_SURFACE, got {err:?}"
    );
}

#[tokio::test]
async fn in_recovery_mode_allocate_aios_surface_kind_succeeds() {
    let r = InMemoryKdeRenderer::new();
    r.enter_recovery_mode().await.unwrap();

    let req = make_request(
        CompositionZone::Recovery,
        "aios-chrome",
        NodeKind::SecurityIndicator,
    );
    let desc = r.allocate_surface(req).await.unwrap();
    assert_eq!(desc.mode, RendererMode::Recovery);
}

// ── degraded mode (INV I7) ────────────────────────────────────────────────────

#[tokio::test]
async fn enter_degraded_mode_blocks_gpu_bearing_node_kinds() {
    let r = InMemoryKdeRenderer::new();
    r.enter_degraded_mode("kwin_unreachable".into())
        .await
        .unwrap();
    assert!(matches!(r.get_mode().await, RendererMode::Degraded(_)));

    // Text (non-GPU) should succeed.
    let text_req = make_request(CompositionZone::Content, "test", NodeKind::Text);
    assert!(r.allocate_surface(text_req).await.is_ok());

    // Visualization (GPU-bearing) should be rejected.
    let gpu_req = make_request(CompositionZone::Content, "test", NodeKind::Visualization);
    let result = r.allocate_surface(gpu_req).await;
    assert!(result.is_err());
    assert!(
        matches!(
            result.unwrap_err(),
            aios_renderer_kde::KdeRendererError::Degraded(_)
        ),
        "expected Degraded error for GPU-bearing kind in degraded mode"
    );
}

// ── visual tokens ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn apply_visual_tokens_round_trip() {
    let r = InMemoryKdeRenderer::new();
    let tokens = vec![
        VisualToken {
            id: "COLOR_BG".into(),
            kind: VisualTokenKind::Color,
            canonical_value: "#FFFFFF".into(),
        },
        VisualToken {
            id: "FONT_UI".into(),
            kind: VisualTokenKind::Font,
            canonical_value: "Inter".into(),
        },
        VisualToken {
            id: "SPACING_MD".into(),
            kind: VisualTokenKind::Spacing,
            canonical_value: "8".into(),
        },
    ];
    let receipt = r.apply_visual_tokens(tokens.clone()).await.unwrap();
    assert_eq!(receipt.applied_count, 3);
    let active = r.get_active_tokens().await;
    assert_eq!(active.len(), 3);
    assert_eq!(active, tokens);
}

// ── concurrent allocation ─────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn concurrent_allocate_3_tasks_produces_3_distinct_ids() {
    use std::sync::Arc;

    let r = Arc::new(InMemoryKdeRenderer::new());
    let r1 = Arc::clone(&r);
    let r2 = Arc::clone(&r);
    let r3 = Arc::clone(&r);

    let (d1, d2, d3) = tokio::join!(
        r1.allocate_surface(make_request(CompositionZone::Content, "t1", NodeKind::Text)),
        r2.allocate_surface(make_request(CompositionZone::Content, "t2", NodeKind::Card)),
        r3.allocate_surface(make_request(CompositionZone::Content, "t3", NodeKind::List)),
    );

    let ids = vec![d1.unwrap().id, d2.unwrap().id, d3.unwrap().id];
    // All 3 ids must be distinct.
    let mut seen = std::collections::HashSet::new();
    for id in &ids {
        assert!(
            seen.insert(id.0.clone()),
            "duplicate KdeSurfaceId: {}",
            id.0
        );
    }
    assert_eq!(seen.len(), 3);
    assert_eq!(r.list_surfaces(SurfaceFilter::All).await.len(), 3);
}
