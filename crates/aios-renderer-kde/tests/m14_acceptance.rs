//! T-138 — M14 acceptance E2E fixtures (S7.4).
//!
//! End-to-end acceptance tests exercising the full KDE renderer stack across
//! six phases: chrome surface allocation, KWin script load, icon bundle
//! verification, recovery shell, degraded mode, and Apps bridge parity.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::items_after_statements,
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    clippy::doc_markdown,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;

use aios_apps::compatibility_orchestrator::CompatibilityOrchestrator;
use aios_apps::knowledge_db::CompatibilityKnowledgeDB;
use aios_apps::package_store::{InMemoryPackageStore, PackageStore};
use aios_apps::service::proto::RegisterPackageRequest;
use aios_apps::service::{build_router, AppsServer};
use aios_apps::session_driver::InMemorySessionDriver;
use aios_apps::update_driver::InMemoryUpdateDriver;

use aios_renderer_kde::{
    escalate_to_degraded, evaluate_surface_request, AllocateSurfaceRequest, AppsBridge,
    ConstitutionalIconBundle, DegradedTrigger, InMemoryKdeEvidenceEmitter, InMemoryKdeRenderer,
    KdeRenderer, KdeRendererError, KdeSurfaceId, KwinScript, KwinScriptLoader, NodeKind,
    RendererMode, WaylandClient, WaylandInteractivity, WaylandProtocol, WaylandSurfaceLayer,
    WaylandSurfaceRequest,
};

// ===========================================================================
// E2E Test 1 — Full 6-phase KDE renderer stack
// ===========================================================================

#[tokio::test]
async fn e2e_full_kde_renderer_stack_six_phases() {
    // ── Bootstrap stack ──────────────────────────────────────────────────
    let emitter = Arc::new(InMemoryKdeEvidenceEmitter::new("service:aios-renderer-kde"));
    let renderer = InMemoryKdeRenderer::new().with_emitter(emitter.clone());

    // Phase 1 ─ Chrome surface allocation (INV I4) ────────────────────────
    let chrome_req = AllocateSurfaceRequest {
        zone: aios_renderer_kde::CompositionZone::Chrome,
        claimed_by: "aios-chrome".into(),
        node_kind: NodeKind::SecurityIndicator,
        requested_layer: None,
    };
    let chrome_desc = renderer
        .allocate_surface(chrome_req)
        .await
        .expect("aios-chrome on Chrome zone must succeed");
    assert_eq!(chrome_desc.zone, aios_renderer_kde::CompositionZone::Chrome);
    assert_eq!(chrome_desc.claimed_by, "aios-chrome");

    // Attempt non-chrome claimant on Chrome zone → must be rejected.
    let intruder_req = AllocateSurfaceRequest {
        zone: aios_renderer_kde::CompositionZone::Chrome,
        claimed_by: "intruder".into(),
        node_kind: NodeKind::Text,
        requested_layer: None,
    };
    let intruder_result = renderer.allocate_surface(intruder_req).await;
    assert!(
        intruder_result.is_err(),
        "intruder on Chrome zone must be rejected"
    );
    match intruder_result.unwrap_err() {
        KdeRendererError::OverlayLayerForbidden { client_id } => {
            assert_eq!(client_id, "intruder");
        }
        other => panic!("expected OverlayLayerForbidden, got {other:?}"),
    }

    // Phase 2 ─ KWin script load (INV I8) ─────────────────────────────────
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key: VerifyingKey = signing_key.verifying_key();
    let source = "// AIOS fullscreen block script v1";
    let hash = blake3::hash(source.as_bytes());
    let sig = signing_key.sign(hash.as_bytes());

    let mut loader = KwinScriptLoader::new("/aios/system/renderers/kde/kwin-scripts")
        .with_emitter(emitter.clone());
    loader.register_authority("aios-kde-signer", verifying_key);

    let valid_script = KwinScript {
        id: "aios-fullscreen-block".into(),
        canonical_path: "/aios/system/renderers/kde/kwin-scripts/aios-fullscreen-block.js".into(),
        source: source.into(),
        blake3_hash: hash.to_hex().to_string(),
        signature: sig.to_bytes().to_vec(),
        signer_key_fingerprint: "aios-kde-signer".into(),
    };
    loader
        .load_script(valid_script)
        .await
        .expect("valid signed script must load");

    // Reject script with bad signature.
    let bad_sig_script = KwinScript {
        id: "bad-sig".into(),
        canonical_path: "/aios/system/renderers/kde/kwin-scripts/bad.js".into(),
        source: "// bad".into(),
        blake3_hash: blake3::hash(b"// bad").to_hex().to_string(),
        signature: vec![0u8; 64],
        signer_key_fingerprint: "aios-kde-signer".into(),
    };
    let bad_result = loader.load_script(bad_sig_script).await;
    assert!(bad_result.is_err(), "bad signature must be rejected");

    // Phase 3 ─ Constitutional icon bundle (INV I6) ───────────────────────
    use std::collections::{BTreeMap, HashMap};

    let icon_signing_key = SigningKey::generate(&mut OsRng);
    let icon_verifying_key: VerifyingKey = icon_signing_key.verifying_key();
    let icon_fp = "aios-icon-authority";

    let icon_entry = aios_renderer_kde::IconEntry {
        token_id: "ICON_ACTION_SAVE".into(),
        relative_path: "actions/save.svg".into(),
        blake3_hash: "b".repeat(64),
    };

    let mut icon_message = Vec::new();
    icon_message.extend_from_slice(icon_entry.token_id.as_bytes());
    icon_message.extend_from_slice(icon_entry.relative_path.as_bytes());
    icon_message.extend_from_slice(icon_entry.blake3_hash.as_bytes());
    let icon_sig = icon_signing_key.sign(&icon_message);

    let mut icon_manifest = BTreeMap::new();
    icon_manifest.insert(icon_entry.token_id.clone(), icon_entry);

    let mut icon_trusted = HashMap::new();
    icon_trusted.insert(icon_fp.to_string(), icon_verifying_key);

    let valid_bundle = ConstitutionalIconBundle {
        theme_id: "aios-recovery".into(),
        root_path: "/aios/system/renderers/kde/icons/aios-recovery".into(),
        manifest: icon_manifest.clone(),
        bundle_signature: icon_sig.to_bytes().to_vec(),
        signer_fingerprint: icon_fp.into(),
        trusted_authorities: icon_trusted.clone(),
        emitter: Some(emitter.clone()),
    };
    assert!(valid_bundle.verify().is_ok(), "valid bundle must verify");

    // Reject bundle with bad authority.
    let bad_bundle = ConstitutionalIconBundle {
        theme_id: "aios-recovery".into(),
        root_path: "/aios/system/renderers/kde/icons/aios-recovery".into(),
        manifest: icon_manifest,
        bundle_signature: vec![0u8; 64],
        signer_fingerprint: "unknown-authority".into(),
        trusted_authorities: HashMap::new(),
        emitter: None,
    };
    assert!(
        bad_bundle.verify().is_err(),
        "bad authority bundle must reject"
    );

    // Phase 4 ─ Recovery shell (INV I5) ───────────────────────────────────
    let recovery_receipt = renderer
        .enter_recovery_mode()
        .await
        .expect("enter recovery");
    assert!(recovery_receipt.aios_surfaces_only);
    assert_eq!(
        recovery_receipt.display_separation,
        "separate-wayland-display"
    );

    // Verify recovery mode is active.
    let mode = renderer.get_mode().await;
    assert_eq!(mode, RendererMode::Recovery);

    // Non-AIOS surface in recovery must be rejected.
    let recovery_text_req = AllocateSurfaceRequest {
        zone: aios_renderer_kde::CompositionZone::Content,
        claimed_by: "test-app".into(),
        node_kind: NodeKind::Text,
        requested_layer: None,
    };
    let recovery_text_result = renderer.allocate_surface(recovery_text_req).await;
    assert!(
        recovery_text_result.is_err(),
        "non-AIOS surface in recovery must be rejected"
    );

    // AIOS-owned surface (SecurityIndicator) must be admitted.
    let recovery_aios_req = AllocateSurfaceRequest {
        zone: aios_renderer_kde::CompositionZone::Recovery,
        claimed_by: "aios-chrome".into(),
        node_kind: NodeKind::SecurityIndicator,
        requested_layer: None,
    };
    let recovery_aios_desc = renderer
        .allocate_surface(recovery_aios_req)
        .await
        .expect("AIOS surface in recovery must succeed");
    assert_eq!(recovery_aios_desc.claimed_by, "aios-chrome");

    // Exit recovery.
    renderer.exit_recovery_mode().await.expect("exit recovery");
    assert_eq!(renderer.get_mode().await, RendererMode::Normal);

    // Phase 5 ─ Degraded mode (INV I7) ────────────────────────────────────
    let (degraded_mode, reason) = escalate_to_degraded(DegradedTrigger::KwinUnavailable);
    assert_eq!(reason, "kwin_unreachable");
    assert!(matches!(degraded_mode, RendererMode::Degraded(_)));

    renderer
        .enter_degraded_mode("kwin-crashed".into())
        .await
        .expect("enter degraded");
    let mode = renderer.get_mode().await;
    assert!(matches!(mode, RendererMode::Degraded(_)));

    // GPU-bearing node kind must be rejected in degraded mode.
    let gpu_req = AllocateSurfaceRequest {
        zone: aios_renderer_kde::CompositionZone::Content,
        claimed_by: "test-app".into(),
        node_kind: NodeKind::Visualization,
        requested_layer: None,
    };
    let gpu_result = renderer.allocate_surface(gpu_req).await;
    assert!(
        gpu_result.is_err(),
        "GPU-bearing kind must be rejected in degraded mode"
    );

    // ── Evidence chain integrity ────────────────────────────────────────
    emitter.verify_chain().await.expect("chain integrity");
}

// ===========================================================================
// E2E Test 2 — Wayland surface evaluation full lifecycle
// ===========================================================================

#[tokio::test]
async fn e2e_wayland_surface_evaluation_lifecycle() {
    // Connect Wayland client.
    let client = WaylandClient::connect("aios-kde")
        .await
        .expect("connect wayland");

    // Allocate content surface.
    let surface_id = KdeSurfaceId::new();
    let content_req = WaylandSurfaceRequest {
        protocol: WaylandProtocol::XdgShell,
        layer_namespace: "aios-shell".into(),
        claimed_by: "test-app".into(),
        zone: aios_renderer_kde::CompositionZone::Content,
        node_kind: NodeKind::Card,
    };
    let grant = client
        .request_surface(surface_id.clone(), content_req)
        .await
        .expect("request content surface");
    assert_eq!(grant.assigned_layer, WaylandSurfaceLayer::Top);
    assert_eq!(grant.interactivity, WaylandInteractivity::OnDemand);

    // Allocate background surface.
    let bg_id = KdeSurfaceId::new();
    let bg_req = WaylandSurfaceRequest {
        protocol: WaylandProtocol::WlCompositor,
        layer_namespace: "aios-bg".into(),
        claimed_by: "wallpaper-daemon".into(),
        zone: aios_renderer_kde::CompositionZone::Background,
        node_kind: NodeKind::Container,
    };
    let bg_grant = client
        .request_surface(bg_id.clone(), bg_req)
        .await
        .expect("request bg surface");
    assert_eq!(bg_grant.assigned_layer, WaylandSurfaceLayer::Background);
    assert_eq!(bg_grant.interactivity, WaylandInteractivity::None);

    // INV I4: reject non-chrome on Chrome zone.
    let chrome_req = WaylandSurfaceRequest {
        protocol: WaylandProtocol::WlrLayerShellV1,
        layer_namespace: "aios-chrome".into(),
        claimed_by: "malicious-client".into(),
        zone: aios_renderer_kde::CompositionZone::Chrome,
        node_kind: NodeKind::SecurityIndicator,
    };
    let chrome_result = evaluate_surface_request(&chrome_req);
    assert!(chrome_result.is_err());

    // Recovery zone grants exclusive interactivity.
    let recovery_req = WaylandSurfaceRequest {
        protocol: WaylandProtocol::WlrLayerShellV1,
        layer_namespace: "aios-recovery".into(),
        claimed_by: "aios-chrome".into(),
        zone: aios_renderer_kde::CompositionZone::Recovery,
        node_kind: NodeKind::SurfaceEmbed,
    };
    let recovery_grant = evaluate_surface_request(&recovery_req).expect("recovery zone");
    assert_eq!(recovery_grant.assigned_layer, WaylandSurfaceLayer::Overlay);
    assert_eq!(
        recovery_grant.interactivity,
        WaylandInteractivity::Exclusive
    );

    // wlr-layer-shell on Content zone must fail.
    let wlr_content_req = WaylandSurfaceRequest {
        protocol: WaylandProtocol::WlrLayerShellV1,
        layer_namespace: "aios-shell".into(),
        claimed_by: "test-app".into(),
        zone: aios_renderer_kde::CompositionZone::Content,
        node_kind: NodeKind::Card,
    };
    let wlr_result = evaluate_surface_request(&wlr_content_req);
    assert!(
        wlr_result.is_err(),
        "wlr-layer-shell on Content zone must fail"
    );

    // Verify grant tracking.
    let grants = client.list_grants().await;
    assert_eq!(grants.len(), 2);
}

// ===========================================================================
// E2E Test 3 — Apps bridge render E2E
// ===========================================================================

#[tokio::test]
async fn e2e_apps_bridge_render_package_list_as_kde_node_tree() {
    use std::collections::HashMap;

    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let mut trusted = HashMap::new();
    trusted.insert(verifying_key.to_bytes().to_vec(), "test-authority".into());

    let store = Arc::new(InMemoryPackageStore::new(trusted));
    let knowledge = Arc::new(CompatibilityKnowledgeDB::with_fixtures());
    let orchestrator = Arc::new(CompatibilityOrchestrator::new_with_defaults());
    let sessions = Arc::new(InMemorySessionDriver::new_with_defaults());
    let updates = Arc::new(InMemoryUpdateDriver::new());

    let svc = AppsServer::new(
        store.clone() as Arc<dyn PackageStore>,
        knowledge,
        sessions,
        updates,
        orchestrator,
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let ep = format!("http://{addr}");

    let router = build_router(svc);
    let _jh = tokio::spawn(async move {
        router
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Connect via AppsBridge and render empty list.
    let mut bridge = AppsBridge::connect(ep.clone())
        .await
        .expect("connect bridge");
    let tree = bridge
        .render_package_list_as_kde_tree()
        .await
        .expect("list RPC");
    assert_eq!(tree.root.kind, NodeKind::List);
    assert_eq!(tree.root.label, "Packages");
    assert!(tree.root.children.is_empty(), "empty list = zero children");

    // Register a package via raw client.
    let mut raw_client =
        aios_apps::service::proto::apps_service_client::AppsServiceClient::connect(ep)
            .await
            .unwrap();

    let manifest_json = r#"{"name":"firefox","version":"125.0.1"}"#;
    let manifest_bytes = manifest_json.as_bytes().to_vec();
    let content_hash = blake3::hash(&manifest_bytes).to_hex().to_string();
    let sig = signing_key.sign(&manifest_bytes);

    let resp = raw_client
        .register_package(tonic::Request::new(RegisterPackageRequest {
            package: Some(aios_apps::service::proto::PackageEnvelopeProto {
                package_id: "pkg_test_firefox".into(),
                name: "firefox".into(),
                version: "125.0.1".into(),
                manifest_bytes,
                content_hash_blake3: content_hash,
                ed25519_signature: sig.to_bytes().to_vec(),
                signer_public_key: verifying_key.to_bytes().to_vec(),
                registered_at: Some(prost_types::Timestamp {
                    seconds: chrono::Utc::now().timestamp(),
                    nanos: 0,
                }),
            }),
        }))
        .await
        .expect("register RPC");
    let pkg_id = resp.into_inner().package_id;

    // Render package list and validate KDE node tree shape.
    let mut bridge2 = AppsBridge::connect(format!("http://{addr}"))
        .await
        .expect("connect bridge2");
    let tree = bridge2
        .render_package_list_as_kde_tree()
        .await
        .expect("list RPC");
    assert_eq!(tree.root.kind, NodeKind::List);
    assert_eq!(tree.root.children.len(), 1);
    let card = &tree.root.children[0];
    assert_eq!(card.kind, NodeKind::Card);
    assert!(card.label.contains("firefox"));
    assert!(card.label.contains("125.0.1"));

    // Render single package show.
    let mut bridge3 = AppsBridge::connect(format!("http://{addr}"))
        .await
        .expect("connect bridge3");
    let tree = bridge3
        .render_package_show_as_kde_tree(&pkg_id)
        .await
        .expect("show RPC");
    assert_eq!(tree.root.kind, NodeKind::Card);
    assert!(tree.root.label.contains("firefox"));
    // Verify children: version Text + id Text.
    assert_eq!(tree.root.children.len(), 2);
    assert_eq!(tree.root.children[0].kind, NodeKind::Text);
    assert_eq!(tree.root.children[1].kind, NodeKind::Text);
}
