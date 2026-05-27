//! T-127 skeleton tests — 15 unit tests covering the typed core of
//! aios-renderer-kde per S7.4.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::no_effect_underscore_binding
)]

use aios_renderer_kde::{
    CompositionZone, KdeRendererError, KdeSurfaceDescriptor, KdeSurfaceId, NodeKind,
    RecoveryShellMode, RendererMode, VisualToken, VisualTokenKind, ZoneLayer, DEFAULT_CODE_VERSION,
};

// ── NodeKind ────────────────────────────────────────────────────────────────

#[test]
fn node_kind_has_19_variants() {
    assert_eq!(
        NodeKind::LEN,
        19,
        "S7.2 declares exactly 19 NodeKind values"
    );
}

#[test]
fn every_node_kind_has_compilation_hint() {
    // Collect all 19 variants and verify each returns a hint with a non-empty
    // qml_primitive and the same kind reflected back.
    let all = all_node_kinds();
    assert_eq!(all.len(), 19);
    for kind in &all {
        let hint = kind.compilation_hint();
        assert_eq!(hint.kind, *kind);
        assert!(
            !hint.qml_primitive.is_empty(),
            "NodeKind::{kind:?} must have a non-empty qml_primitive"
        );
    }
}

#[test]
fn gpu_bearing_set_matches_spec() {
    // Per S7.4 §4: Visualization, Stream, SurfaceEmbed are GPU-bearing;
    // all other 16 kinds are not.
    for kind in &all_node_kinds() {
        let hint = kind.compilation_hint();
        match kind {
            NodeKind::Visualization | NodeKind::Stream | NodeKind::SurfaceEmbed => {
                assert!(
                    hint.is_gpu_bearing,
                    "NodeKind::{kind:?} must be GPU-bearing per S7.4 §4"
                );
            }
            _ => {
                assert!(
                    !hint.is_gpu_bearing,
                    "NodeKind::{kind:?} must NOT be GPU-bearing per S7.4 §4"
                );
            }
        }
    }
}

// ── CompositionZone → ZoneLayer mapping ─────────────────────────────────────

#[test]
fn composition_zone_chrome_maps_to_overlay_layer() {
    assert_eq!(CompositionZone::Chrome.allowed_layer(), ZoneLayer::Overlay);
}

#[test]
fn composition_zone_recovery_maps_to_overlay_layer() {
    assert_eq!(
        CompositionZone::Recovery.allowed_layer(),
        ZoneLayer::Overlay
    );
}

#[test]
fn composition_zone_content_maps_to_top_layer() {
    assert_eq!(CompositionZone::Content.allowed_layer(), ZoneLayer::Top);
}

#[test]
fn composition_zone_background_maps_to_background_layer() {
    assert_eq!(
        CompositionZone::Background.allowed_layer(),
        ZoneLayer::Background
    );
}

// ── KdeSurfaceDescriptor + INV I4 enforcement ───────────────────────────────

#[test]
fn kde_surface_descriptor_new_chrome_zone_requires_aios_chrome_claimant() {
    let result = KdeSurfaceDescriptor::new(CompositionZone::Chrome, "aios-chrome");
    assert!(
        result.is_ok(),
        "aios-chrome claimant on Chrome zone must succeed"
    );
    let desc = result.unwrap();
    assert_eq!(desc.zone, CompositionZone::Chrome);
    assert_eq!(desc.layer, ZoneLayer::Overlay);
    assert_eq!(desc.claimed_by, "aios-chrome");
}

#[test]
fn kde_surface_descriptor_non_chrome_client_on_chrome_zone_returns_overlay_forbidden() {
    let result = KdeSurfaceDescriptor::new(CompositionZone::Chrome, "family:app:com.example");
    assert!(result.is_err());
    match result.unwrap_err() {
        KdeRendererError::OverlayLayerForbidden { client_id } => {
            assert_eq!(client_id, "family:app:com.example");
        }
        other => panic!("expected OverlayLayerForbidden, got {other:?}"),
    }
}

#[test]
fn kde_surface_descriptor_content_zone_accepts_any_claimant() {
    let result = KdeSurfaceDescriptor::new(CompositionZone::Content, "any-client");
    assert!(result.is_ok());
    let desc = result.unwrap();
    assert_eq!(desc.zone, CompositionZone::Content);
    assert_eq!(desc.layer, ZoneLayer::Top);
}

// ── RecoveryShellMode + RendererMode ────────────────────────────────────────

#[test]
fn recovery_shell_mode_default_is_not_recovery() {
    assert_eq!(RecoveryShellMode::default(), RecoveryShellMode::NotRecovery);
}

#[test]
fn renderer_mode_degraded_carries_reason() {
    let reason = "kwin_unreachable";
    let mode = RendererMode::Degraded(reason.to_string());
    match mode {
        RendererMode::Degraded(ref r) => assert_eq!(r, reason),
        _ => panic!("expected Degraded variant"),
    }
}

// ── KdeSurfaceId ────────────────────────────────────────────────────────────

#[test]
fn kde_surface_id_new_is_unique() {
    let ids: Vec<KdeSurfaceId> = (0..100).map(|_| KdeSurfaceId::new()).collect();
    // All 100 ids must be distinct.
    let mut seen = std::collections::HashSet::new();
    for id in &ids {
        assert!(
            seen.insert(id.0.clone()),
            "duplicate KdeSurfaceId: {}",
            id.0
        );
    }
    assert_eq!(seen.len(), 100);
}

// ── VisualToken ─────────────────────────────────────────────────────────────

#[test]
fn visual_token_kinds_round_trip_through_serde() {
    let kinds = [
        VisualTokenKind::Color,
        VisualTokenKind::Font,
        VisualTokenKind::Spacing,
        VisualTokenKind::Motion,
        VisualTokenKind::Icon,
        VisualTokenKind::Shape,
        VisualTokenKind::Elevation,
    ];
    for kind in &kinds {
        let json = serde_json::to_string(kind).expect("serialize VisualTokenKind");
        let roundtripped: VisualTokenKind =
            serde_json::from_str(&json).expect("deserialize VisualTokenKind");
        assert_eq!(roundtripped, *kind);
    }
}

#[test]
fn visual_token_round_trip_through_serde() {
    let token = VisualToken {
        id: "COLOR_ACTION_AI".to_string(),
        kind: VisualTokenKind::Color,
        canonical_value: "#FF6B6B".to_string(),
    };
    let json = serde_json::to_string(&token).expect("serialize VisualToken");
    let roundtripped: VisualToken = serde_json::from_str(&json).expect("deserialize VisualToken");
    assert_eq!(roundtripped, token);
}

// ── DefaultCodeVersion ──────────────────────────────────────────────────────

#[test]
fn default_code_version_constant_is_correct() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-renderer-kde/0.1.0-T138");
}

// ── KdeRendererError Display ────────────────────────────────────────────────

#[test]
fn kde_renderer_error_display_round_trip() {
    // Every error variant must produce a non-empty Display string.
    let errors: Vec<KdeRendererError> = vec![
        KdeRendererError::SurfaceNotFound(KdeSurfaceId("surf_test".into())),
        KdeRendererError::InvalidZoneTransition {
            from: ZoneLayer::Top,
            to: ZoneLayer::Overlay,
        },
        KdeRendererError::OverlayLayerForbidden {
            client_id: "intruder".into(),
        },
        KdeRendererError::WaylandConnectError("connection refused".into()),
        KdeRendererError::KwinScriptVerificationFailed {
            script_id: "aios-fullscreen-block".into(),
            reason: "unsigned".into(),
        },
        KdeRendererError::IconBundleVerificationFailed {
            theme_id: "theme_default".into(),
            reason: "hash_mismatch".into(),
        },
        KdeRendererError::GpuBindingUnavailable("VkDevice init failed".into()),
        KdeRendererError::Degraded("kwin_unreachable".into()),
        KdeRendererError::Internal("panic in compositor loop".into()),
    ];
    for err in &errors {
        let display = err.to_string();
        assert!(
            !display.is_empty(),
            "KdeRendererError::{err:?} must produce non-empty Display"
        );
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Return all 19 declared `NodeKind` variants.
fn all_node_kinds() -> Vec<NodeKind> {
    vec![
        NodeKind::Container,
        NodeKind::Divider,
        NodeKind::Spacer,
        NodeKind::Text,
        NodeKind::Heading,
        NodeKind::InlineCode,
        NodeKind::CodeBlock,
        NodeKind::Card,
        NodeKind::List,
        NodeKind::Table,
        NodeKind::Form,
        NodeKind::ActionButton,
        NodeKind::Visualization,
        NodeKind::Stream,
        NodeKind::SurfaceEmbed,
        NodeKind::SecurityIndicator,
        NodeKind::ApprovalPrompt,
        NodeKind::EvidenceLink,
        NodeKind::AgentMessage,
    ]
}
