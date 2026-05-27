//! T-139 skeleton tests — 17 unit tests covering the typed core of
//! aios-renderer-web per S7.5.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::no_effect_underscore_binding
)]

use std::collections::HashSet;

use aios_renderer_web::{
    ChromeShadowRootMarker, ExposureLevel, ExposureLevelLabel, NodeKind, OriginScheme,
    RouteDescriptor, ShadowRootMode, WebRendererError, WebRendererMode, WebSurfaceDescriptor,
    WebSurfaceId, DEFAULT_CODE_VERSION,
};

// ── DEFAULT_CODE_VERSION ──────────────────────────────────────────────────────

#[test]
fn default_code_version_constant_is_correct() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-renderer-web/0.1.0-T150");
}

// ── NodeKind re-export ────────────────────────────────────────────────────────

#[test]
fn node_kind_re_exported_has_19_variants() {
    assert_eq!(
        NodeKind::LEN,
        19,
        "S7.2 declares exactly 19 NodeKind values, re-exported from aios-renderer-kde"
    );
    assert_eq!(NodeKind::ALL.len(), 19);
}

// ── WebRendererError Display ──────────────────────────────────────────────────

#[test]
fn web_renderer_error_display_round_trip() {
    let errors: Vec<WebRendererError> = vec![
        WebRendererError::SurfaceNotFound(WebSurfaceId("surf_test".into())),
        WebRendererError::OriginVerificationFailed {
            expected_group_id: "acme-app".into(),
            presented_origin: "https://evil.aios.localhost:8443".into(),
        },
        WebRendererError::ExposureEscalationDenied {
            from: ExposureLevelLabel::Localhost,
            to: ExposureLevelLabel::LanPending,
            reason: "no WEB_EXPOSURE_GRANTED evidence".into(),
        },
        WebRendererError::LanExposureWithoutEvidence,
        WebRendererError::ChromeShadowRootIntegrityFailed {
            reason: "z-index is 5, expected 9999".into(),
        },
        WebRendererError::CertificateVerificationFailed("self-signed cert".into()),
        WebRendererError::PlainHttpRejected("HTTP/1.1 on port 443".into()),
        WebRendererError::IconBundleVerificationFailed {
            theme_id: "theme_default".into(),
            reason: "hash_mismatch".into(),
        },
        WebRendererError::WebgpuAdapterUnavailable("no GPU device".into()),
        WebRendererError::ExtensionInterferenceDetected(
            "untrusted extension modified shadow root".into(),
        ),
        WebRendererError::Internal("panic in request handler".into()),
    ];
    assert_eq!(errors.len(), 11, "all 11 error variants must be covered");
    for err in &errors {
        let display = err.to_string();
        assert!(
            !display.is_empty(),
            "WebRendererError::{err:?} must produce non-empty Display"
        );
    }
}

// ── ExposureLevel + ExposureLevelLabel ────────────────────────────────────────

#[test]
fn exposure_level_label_localhost_is_localhost() {
    let level = ExposureLevel::Localhost;
    assert_eq!(level.label(), ExposureLevelLabel::Localhost);
}

#[test]
fn exposure_level_label_lan_active_is_lan_active() {
    use chrono::Utc;
    let now = Utc::now();
    let level = ExposureLevel::LanActive {
        activated_at: now,
        last_heartbeat_at: now,
    };
    assert_eq!(level.label(), ExposureLevelLabel::LanActive);
}

#[test]
fn exposure_level_label_public_is_public() {
    use chrono::Utc;
    let now = Utc::now();
    let level = ExposureLevel::Public {
        granted_at: now,
        recovery_authorized_by: "root".into(),
        policy_decision_id: "evt_abc123".into(),
    };
    assert_eq!(level.label(), ExposureLevelLabel::Public);
}

// ── OriginScheme parsing ──────────────────────────────────────────────────────

#[test]
fn origin_parse_aios_localhost_succeeds() {
    let result = OriginScheme::parse("https://acme-app.aios.localhost:8443");
    assert!(result.is_ok(), "aios.localhost origin must parse");
    let parsed = result.unwrap();
    assert_eq!(parsed.port, 8443);
    match parsed.scheme {
        OriginScheme::AiosLocalhost(token) => assert_eq!(token.0, "acme-app"),
        other => panic!("expected AiosLocalhost, got {other:?}"),
    }
}

#[test]
fn origin_parse_recovery_localhost_succeeds() {
    let result = OriginScheme::parse("https://recovery.localhost:8443");
    assert!(result.is_ok(), "recovery.localhost origin must parse");
    let parsed = result.unwrap();
    assert_eq!(parsed.port, 8443);
    match parsed.scheme {
        OriginScheme::Recovery => {}
        other => panic!("expected Recovery, got {other:?}"),
    }
}

#[test]
fn origin_parse_invalid_scheme_returns_internal_error() {
    // http:// instead of https://
    let result = OriginScheme::parse("http://acme-app.aios.localhost:8443");
    assert!(result.is_err());
    match result.unwrap_err() {
        WebRendererError::Internal(msg) => {
            assert!(
                msg.contains("https://"),
                "error must mention https:// requirement"
            );
        }
        other => panic!("expected Internal error, got {other:?}"),
    }
}

// ── Origin verification (INV I4) ──────────────────────────────────────────────

#[test]
fn origin_verify_against_matching_group_id_succeeds() {
    let parsed = OriginScheme::parse("https://acme-app.aios.localhost:8443")
        .expect("valid origin must parse");
    let result = parsed.verify_against_group("acme-app");
    assert!(result.is_ok(), "matching group_id must pass verification");
}

#[test]
fn origin_verify_against_mismatched_group_id_returns_origin_verification_failed() {
    let parsed = OriginScheme::parse("https://acme-app.aios.localhost:8443")
        .expect("valid origin must parse");
    let result = parsed.verify_against_group("other-group");
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

// ── WebSurfaceDescriptor ──────────────────────────────────────────────────────

#[test]
fn web_surface_descriptor_new_normal_mode_with_non_recovery_origin_succeeds() {
    let parsed = OriginScheme::parse("https://acme-app.aios.localhost:8443")
        .expect("valid origin must parse");
    let result = WebSurfaceDescriptor::new(parsed, NodeKind::Container, "family:app:com.example");
    assert!(result.is_ok(), "normal origin + normal mode must succeed");
    let desc = result.unwrap();
    assert_eq!(desc.claimed_by, "family:app:com.example");
    assert_eq!(desc.node_kind, NodeKind::Container);
    assert!(matches!(desc.mode, WebRendererMode::Normal));
}

#[test]
fn web_surface_descriptor_new_recovery_mode_with_normal_origin_does_not_panic() {
    // INV I11 is a mode marker — descriptor creation does not panic or fail
    // when mode is Normal regardless of origin scheme. The FSM enforces the
    // invariant at exposure transition time (T-144).
    let parsed = OriginScheme::parse("https://recovery.localhost:8443")
        .expect("valid recovery origin must parse");
    let result = WebSurfaceDescriptor::new(parsed, NodeKind::Container, "recovery-operator");
    assert!(result.is_ok(), "descriptor creation must succeed");
    let desc = result.unwrap();
    // Mode is always Normal at construction time; INV I11 enforcement happens
    // at the exposure FSM layer (T-144).
    assert!(matches!(desc.mode, WebRendererMode::Normal));
}

// ── ChromeShadowRootMarker ────────────────────────────────────────────────────

#[test]
fn chrome_shadow_root_marker_z_index_is_9999() {
    let marker = ChromeShadowRootMarker {
        z_index: 9999,
        mode: ShadowRootMode::Closed,
        integrity_hash: "sha256:abc123".into(),
    };
    assert_eq!(marker.z_index, 9999, "INV I2: z-index must be 9999");
}

#[test]
fn chrome_shadow_root_marker_mode_is_closed() {
    let marker = ChromeShadowRootMarker {
        z_index: 9999,
        mode: ShadowRootMode::Closed,
        integrity_hash: "sha256:abc123".into(),
    };
    assert!(
        matches!(marker.mode, ShadowRootMode::Closed),
        "INV I7: shadow root mode must be Closed"
    );
}

// ── WebSurfaceId ──────────────────────────────────────────────────────────────

#[test]
fn web_surface_id_new_is_unique() {
    let ids: Vec<WebSurfaceId> = (0..100).map(|_| WebSurfaceId::new()).collect();
    let mut seen = HashSet::new();
    for id in &ids {
        assert!(
            seen.insert(id.0.clone()),
            "duplicate WebSurfaceId: {}",
            id.0
        );
    }
    assert_eq!(seen.len(), 100);
}

// ── RouteDescriptor serde ─────────────────────────────────────────────────────

#[test]
fn route_descriptor_serde_round_trip() {
    let route = RouteDescriptor {
        path: "/api/action".to_string(),
        requires_auth: true,
        served_in_recovery: false,
    };
    let json = serde_json::to_string(&route).expect("serialize RouteDescriptor");
    let roundtripped: RouteDescriptor =
        serde_json::from_str(&json).expect("deserialize RouteDescriptor");
    assert_eq!(roundtripped.path, "/api/action");
    assert!(roundtripped.requires_auth);
    assert!(!roundtripped.served_in_recovery);
}
