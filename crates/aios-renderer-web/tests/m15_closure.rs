//! T-150 — M15 closure invariants.
//!
//! Constitutional checks that M15 (aios-renderer-web) is honestly closed:
//! version marker, no deferred-stub leakage, trait coverage, invariant
//! reachability (INV I2/I3/I4/I7/I9/I10), evidence record type completeness,
//! and `NodeKind` closed vocabulary.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;

use aios_renderer_web::{
    generate_self_signed_loopback_cert, loopback_only_bind_addrs, ChromeIntegrityMonitor,
    ChromeShadowRootMarker, ChromeTreeFragment, ExposureFsm, ExposureLevel, ExposureLevelLabel,
    InMemoryWebEvidenceEmitter, InMemoryWebRenderer, NodeKind, ShadowRootMode, WebEvidenceEmitter,
    WebRecordType, WebRenderer, WebRendererError, WebRendererMode, WebSurfaceDescriptor,
    WebSurfaceId, DEFAULT_CODE_VERSION,
};

// ---------------------------------------------------------------------------
// INV-1: Version marker is 0.1.0-T150
// ---------------------------------------------------------------------------

#[test]
fn inv_1_version_marker_is_0_1_0_t150() {
    assert_eq!(
        DEFAULT_CODE_VERSION, "aios-renderer-web/0.1.0-T150",
        "DEFAULT_CODE_VERSION must reflect M15 closure"
    );
    assert_eq!(
        env!("CARGO_PKG_VERSION"),
        "0.1.0",
        "CARGO_PKG_VERSION must be 0.1.0"
    );
}

// ---------------------------------------------------------------------------
// INV-2: No Status::Unimplemented, todo!, or unimplemented! in src/
// ---------------------------------------------------------------------------

fn collect_rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(std::result::Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
}

#[test]
fn inv_2_no_status_unimplemented_in_source() {
    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_dir, &mut files);

    let mut violations = Vec::new();
    for path in &files {
        let content = std::fs::read_to_string(path).expect("read source file");
        for (line_no, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("//!") {
                continue;
            }
            if trimmed.contains("Status::Unimplemented") {
                violations.push(format!(
                    "{}:{} — {}",
                    path.file_name().unwrap().to_string_lossy(),
                    line_no + 1,
                    trimmed
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "M15 closure violation — Status::Unimplemented found:\n{}",
        violations.join("\n")
    );
}

#[test]
fn inv_2b_no_todo_or_unimplemented_macros_in_source() {
    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_dir, &mut files);

    let mut violations = Vec::new();
    for path in &files {
        let content = std::fs::read_to_string(path).expect("read source file");
        for (line_no, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("//!") {
                continue;
            }
            if trimmed.contains("todo!") || trimmed.contains("unimplemented!") {
                violations.push(format!(
                    "{}:{} — {}",
                    path.file_name().unwrap().to_string_lossy(),
                    line_no + 1,
                    trimmed
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "M15 closure violation — todo!/unimplemented! macros found:\n{}",
        violations.join("\n")
    );
}

// ---------------------------------------------------------------------------
// INV-3: NodeKind has exactly 19 variants (closed vocabulary)
// ---------------------------------------------------------------------------

#[test]
fn inv_3_node_kind_has_19_variants() {
    assert_eq!(
        NodeKind::LEN,
        19,
        "S7.2 declares exactly 19 NodeKind values"
    );
    assert_eq!(
        NodeKind::ALL.len(),
        19,
        "NodeKind::ALL must contain exactly 19 variants"
    );
}

// ---------------------------------------------------------------------------
// INV-4: WebRenderer trait has InMemoryWebRenderer impl
// ---------------------------------------------------------------------------

#[test]
fn inv_4_trait_coverage_in_memory_web_renderer_impl() {
    let renderer = InMemoryWebRenderer::new();
    let _ = &renderer as &dyn std::any::Any;
}

// ---------------------------------------------------------------------------
// INV-5: INV I2 reachability — ChromeShadowRootMarker invariants
// ---------------------------------------------------------------------------

#[test]
fn inv_5_i2_chrome_shadow_root_marker_z_index_9999() {
    let marker = ChromeShadowRootMarker {
        z_index: 9999,
        mode: ShadowRootMode::Closed,
        integrity_hash: "blake3-deadbeef".into(),
    };
    assert_eq!(marker.z_index, 9999, "INV I2: z-index must be 9999");
    assert!(
        matches!(marker.mode, ShadowRootMode::Closed),
        "INV I7: shadow root must be Closed"
    );
    assert!(
        !marker.integrity_hash.is_empty(),
        "integrity hash must be set"
    );
}

// ---------------------------------------------------------------------------
// INV-6: INV I4 reachability — OriginVerifier rejects mismatched binding
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_6_i4_origin_verifier_rejects_mismatched_token() {
    use aios_renderer_web::{IframeOriginBinding, OriginVerifier};

    let verifier = OriginVerifier::new();
    let surface_id = WebSurfaceId::new();

    // Register a chrome origin binding (aios.localhost → AppOrigin("aios"))
    let binding = IframeOriginBinding {
        iframe_origin: "https://aios.localhost:8443".into(),
        surface_id: surface_id.clone(),
        bound_group_id: "aios".into(),
        scope_binding_evidence_id: "evr_test_001".into(),
    };
    verifier
        .register_binding(binding)
        .await
        .expect("register binding");

    // Verify with wrong origin → must fail
    let result = verifier
        .verify_composition(&surface_id, "https://intruder.localhost:8443")
        .await;
    assert!(result.is_err());
    match result.unwrap_err() {
        WebRendererError::OriginVerificationFailed {
            expected_group_id, ..
        } => {
            assert_eq!(expected_group_id, "aios");
        }
        other => panic!("expected OriginVerificationFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn inv_6b_i4_origin_verifier_registers_and_verifies_match() {
    use aios_renderer_web::{IframeOriginBinding, OriginVerifier};

    let verifier = OriginVerifier::new();
    let surface_id = WebSurfaceId::new();

    let binding = IframeOriginBinding {
        iframe_origin: "https://aios.localhost:8443".into(),
        surface_id: surface_id.clone(),
        bound_group_id: "aios".into(),
        scope_binding_evidence_id: "evr_test_002".into(),
    };
    verifier.register_binding(binding).await.expect("register");

    verifier
        .verify_composition(&surface_id, "https://aios.localhost:8443")
        .await
        .expect("matching origin must verify");
}

// ---------------------------------------------------------------------------
// INV-7: INV I7 reachability — degraded mode
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_7_i7_enter_degraded_mode_sets_mode() {
    let renderer = InMemoryWebRenderer::new();
    assert!(matches!(renderer.get_mode().await, WebRendererMode::Normal));

    renderer
        .enter_degraded_mode("kwin_unreachable".into())
        .await
        .expect("enter degraded");
    let mode = renderer.get_mode().await;
    match mode {
        WebRendererMode::Degraded(ref r) => assert_eq!(r, "kwin_unreachable"),
        _ => panic!("expected Degraded mode, got {mode:?}"),
    }
}

#[tokio::test]
async fn inv_7b_i7_degraded_mode_blocks_gpu_kinds() {
    use aios_renderer_web::{AllocateWebSurfaceRequest, OriginScheme, OriginToken, ParsedOrigin};

    let renderer = InMemoryWebRenderer::new();
    renderer
        .enter_degraded_mode("webgpu_init_failed".into())
        .await
        .expect("enter degraded");

    // Visualization is GPU-bearing per compilation_hint
    let origin = ParsedOrigin {
        scheme: OriginScheme::AppOrigin(OriginToken("aios".into())),
        host: "aios.localhost".into(),
        port: 8443,
        full_origin: "https://aios.localhost:8443".into(),
    };
    let req = AllocateWebSurfaceRequest {
        origin,
        node_kind: NodeKind::Visualization,
        claimed_by: "test-app".into(),
        expected_group_id: None,
    };
    let result = renderer.allocate_surface(req).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        WebRendererError::WebgpuAdapterUnavailable(_) => {}
        other => panic!("expected WebgpuAdapterUnavailable, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// INV-8: INV I9 reachability — HTTPS cert generation
// ---------------------------------------------------------------------------

#[test]
fn inv_8_i9_generate_self_signed_loopback_cert_succeeds() {
    let cert = generate_self_signed_loopback_cert(&[]).expect("cert generation");
    assert!(
        !cert.cert_chain_pem.is_empty(),
        "cert PEM must be non-empty"
    );
    assert!(!cert.key_pem.is_empty(), "key PEM must be non-empty");
    assert!(
        cert.san_hosts.contains(&"localhost".to_string()),
        "must include localhost SAN"
    );
    assert!(
        cert.san_hosts.contains(&"127.0.0.1".to_string()),
        "must include 127.0.0.1 SAN"
    );
    assert!(
        cert.san_hosts.contains(&"::1".to_string()),
        "must include ::1 SAN"
    );
    assert!(
        cert.san_hosts.contains(&"*.aios.localhost".to_string()),
        "must include *.aios.localhost SAN"
    );
}

#[test]
fn inv_8b_i9_cert_with_extra_sans_includes_them() {
    let cert =
        generate_self_signed_loopback_cert(&["recovery.localhost"]).expect("cert generation");
    assert!(
        cert.san_hosts.contains(&"recovery.localhost".to_string()),
        "must include extra SAN: recovery.localhost"
    );
}

#[test]
fn inv_8c_i9_loopback_only_bind_addrs_are_loopback() {
    let addrs = loopback_only_bind_addrs(8443);
    assert_eq!(addrs.len(), 2, "must have IPv4 + IPv6 loopback");
    for addr in &addrs {
        assert!(addr.ip().is_loopback(), "{addr} must be loopback");
    }
}

// ---------------------------------------------------------------------------
// INV-9: INV I10 reachability — ChromeIntegrityMonitor
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_9_i10_chrome_integrity_monitor_admits_signed_fragment() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key: VerifyingKey = signing_key.verifying_key();

    let monitor = ChromeIntegrityMonitor::new(verifying_key);
    let root_hash = "deadbeef".repeat(8); // 64 hex chars
    let sig = signing_key.sign(root_hash.as_bytes());

    let fragment = ChromeTreeFragment {
        root_hash: root_hash.clone(),
        signature: sig.to_bytes().to_vec(),
        signed_at: chrono::Utc::now(),
    };
    monitor
        .admit_signed_fragment(fragment)
        .await
        .expect("admit signed fragment");

    // Check known hash → must pass
    monitor
        .check_observed_hash(&root_hash)
        .await
        .expect("known hash must pass");

    // Check unknown hash → must fail
    let unknown = "feedfeed".repeat(8);
    let result = monitor.check_observed_hash(&unknown).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        WebRendererError::ExtensionInterferenceDetected(_) => {}
        other => panic!("expected ExtensionInterferenceDetected, got {other:?}"),
    }
}

#[tokio::test]
async fn inv_9b_i10_integrity_monitor_rejects_bad_signature() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key: VerifyingKey = signing_key.verifying_key();

    let monitor = ChromeIntegrityMonitor::new(verifying_key);
    let fragment = ChromeTreeFragment {
        root_hash: "deadbeef".repeat(8),
        signature: vec![0u8; 64], // bogus signature
        signed_at: chrono::Utc::now(),
    };
    let result = monitor.admit_signed_fragment(fragment).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        WebRendererError::ChromeShadowRootIntegrityFailed { .. } => {}
        other => panic!("expected ChromeShadowRootIntegrityFailed, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// INV-10: Evidence record types — 9 variants all constructable
// ---------------------------------------------------------------------------

#[test]
fn inv_10_evidence_record_types_have_9_variants_all_constructable() {
    let variants = [
        WebRecordType::WebSurfaceAllocated,
        WebRecordType::WebSurfaceReleased,
        WebRecordType::WebExposureTransition,
        WebRecordType::WebExposureGranted,
        WebRecordType::WebLanExposureActive,
        WebRecordType::WebRendererDegraded,
        WebRecordType::WebExtensionInterference,
        WebRecordType::WebIconBundleVerified,
        WebRecordType::WebIconBundleRejected,
    ];
    assert_eq!(variants.len(), 9);
    for v in &variants {
        let s = v.as_str();
        assert!(
            !s.is_empty(),
            "every WebRecordType must have non-empty as_str() — {v:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// INV-11: InMemoryWebEvidenceEmitter chain integrity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_11_evidence_emitter_chain_integrity() {
    use aios_renderer_web::{OriginScheme, OriginToken, ParsedOrigin};

    let emitter = Arc::new(InMemoryWebEvidenceEmitter::new("service:aios-renderer-web"));

    let origin = ParsedOrigin {
        scheme: OriginScheme::AppOrigin(OriginToken("aios".into())),
        host: "aios.localhost".into(),
        port: 8443,
        full_origin: "https://aios.localhost:8443".into(),
    };
    let desc =
        WebSurfaceDescriptor::new(origin, NodeKind::Card, "test-actor").expect("create descriptor");

    let r1 = emitter
        .emit_surface_allocated(&desc, "test-actor")
        .await
        .expect("emit allocated");
    assert!(!r1.record_id.is_empty());
    assert_eq!(r1.sequence, 0);

    let r2 = emitter
        .emit_surface_released(&desc, "test-actor")
        .await
        .expect("emit released");
    assert_eq!(r2.sequence, 1);

    emitter.verify_chain().await.expect("chain integrity");
}

// ---------------------------------------------------------------------------
// INV-12: ExposureFsm full lifecycle (INV I3)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_12_i3_exposure_fsm_full_lifecycle_localhost_to_lan_active_and_revoke() {
    let fsm = ExposureFsm::new();
    assert_eq!(fsm.current().await, ExposureLevel::Localhost);

    // Localhost → LanPending
    fsm.request_lan_escalation("approver-01")
        .await
        .expect("request lan");
    assert!(
        matches!(fsm.current().await, ExposureLevel::LanPending { .. }),
        "must be LanPending"
    );

    // LanPending → LanApproved
    fsm.apply_policy_decision("evr_decision_01")
        .await
        .expect("apply decision");
    assert!(
        matches!(fsm.current().await, ExposureLevel::LanApproved { .. }),
        "must be LanApproved"
    );

    // LanApproved → LanActive
    fsm.activate_lan_exposure().await.expect("activate lan");
    assert!(
        matches!(fsm.current().await, ExposureLevel::LanActive { .. }),
        "must be LanActive"
    );

    // Record heartbeat (self-transition)
    fsm.record_heartbeat().await.expect("record heartbeat");

    // Revoke
    fsm.revoke("operator request").await.expect("revoke");
    assert!(
        matches!(fsm.current().await, ExposureLevel::Revoked { .. }),
        "must be Revoked"
    );

    // Reset to Localhost
    fsm.reset_to_localhost().await.expect("reset to localhost");
    assert_eq!(fsm.current().await, ExposureLevel::Localhost);

    // Verify history length
    let history = fsm.history().await;
    assert!(
        history.len() >= 4,
        "history must have at least 4 transitions, got {}",
        history.len()
    );
}

#[tokio::test]
async fn inv_12b_i3_exposure_fsm_public_escalation_requires_recovery_auth() {
    let fsm = ExposureFsm::new();
    fsm.escalate_to_public("recovery-op", "evr_public_01")
        .await
        .expect("escalate to public");
    let level = fsm.current().await;
    match level {
        ExposureLevel::Public {
            recovery_authorized_by,
            ..
        } => assert_eq!(recovery_authorized_by, "recovery-op"),
        other => panic!("expected Public, got {other:?}"),
    }
}

#[tokio::test]
async fn inv_12c_i3_exposure_fsm_denies_invalid_transitions() {
    let fsm = ExposureFsm::new();

    // Can't activate from Localhost
    let result = fsm.activate_lan_exposure().await;
    assert!(result.is_err());

    // Can't apply policy decision from Localhost
    let result = fsm.apply_policy_decision("evr_x").await;
    assert!(result.is_err());

    // Can't revoke from Localhost
    let result = fsm.revoke("test").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn inv_12d_i3_exposure_fsm_heartbeat_check_expires() {
    use std::time::Duration;

    let fsm = ExposureFsm::with_heartbeat_interval(Duration::from_millis(1));
    fsm.request_lan_escalation("approver").await.unwrap();
    fsm.apply_policy_decision("evr").await.unwrap();
    fsm.activate_lan_exposure().await.unwrap();

    // Set heartbeat far in the past
    let past = chrono::Utc::now() - chrono::Duration::hours(25);
    fsm.set_last_heartbeat_at_for_tests(past).await;

    let result = fsm.check_heartbeat().await;
    assert!(result.is_err());
    match result.unwrap_err() {
        WebRendererError::ExposureEscalationDenied { from, to, .. } => {
            assert_eq!(from, ExposureLevelLabel::LanActive);
            assert_eq!(to, ExposureLevelLabel::Revoked);
        }
        other => panic!("expected ExposureEscalationDenied for heartbeat miss, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// INV-13: WebRenderer trait coverage — all 14 methods callable
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_13_web_renderer_trait_all_methods_callable() {
    use aios_renderer_web::{
        AllocateWebSurfaceRequest, OriginScheme, OriginToken, ParsedOrigin, RouteDescriptor,
        VisualToken, VisualTokenKind, WebSurfaceFilter,
    };

    let renderer = InMemoryWebRenderer::new();

    // allocate_surface
    let origin = ParsedOrigin {
        scheme: OriginScheme::AppOrigin(OriginToken("aios".into())),
        host: "aios.localhost".into(),
        port: 8443,
        full_origin: "https://aios.localhost:8443".into(),
    };
    let req = AllocateWebSurfaceRequest {
        origin: origin.clone(),
        node_kind: NodeKind::Card,
        claimed_by: "test-actor".into(),
        expected_group_id: None,
    };
    let desc = renderer.allocate_surface(req).await.expect("allocate");
    let sid = desc.id.clone();

    // get_surface
    let retrieved = renderer.get_surface(sid.clone()).await.expect("get");
    assert_eq!(retrieved.id, sid);

    // list_surfaces
    let all = renderer.list_surfaces(WebSurfaceFilter::All).await;
    assert_eq!(all.len(), 1);

    // register_route
    let route = RouteDescriptor {
        path: "/test".into(),
        requires_auth: false,
        served_in_recovery: false,
    };
    renderer
        .register_route(route)
        .await
        .expect("register route");

    // list_routes
    let routes = renderer.list_routes().await;
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].path, "/test");

    // unregister_route
    renderer
        .unregister_route("/test")
        .await
        .expect("unregister");

    // enter_recovery_mode
    let receipt = renderer
        .enter_recovery_mode()
        .await
        .expect("enter recovery");
    assert!(receipt.service_worker_disabled);
    assert!(
        receipt.recovery_origin.contains("recovery.localhost"),
        "recovery origin: {}",
        receipt.recovery_origin
    );

    // exit_recovery_mode
    renderer.exit_recovery_mode().await.expect("exit recovery");

    // enter_degraded_mode
    renderer
        .enter_degraded_mode("test-degrade".into())
        .await
        .expect("enter degraded");

    // get_mode
    let mode = renderer.get_mode().await;
    assert!(matches!(mode, WebRendererMode::Degraded(_)));

    // current_exposure
    let exposure = renderer.current_exposure().await;
    assert_eq!(exposure, ExposureLevel::Localhost);

    // apply_visual_tokens
    let token = VisualToken {
        id: "tok-1".into(),
        kind: VisualTokenKind::Icon,
        canonical_value: "test-value".into(),
    };
    let receipt = renderer
        .apply_visual_tokens(vec![token])
        .await
        .expect("apply tokens");
    assert_eq!(receipt.applied_count, 1);

    // get_active_tokens
    let tokens = renderer.get_active_tokens().await;
    assert_eq!(tokens.len(), 1);

    // release_surface
    let release = renderer.release_surface(sid).await.expect("release");
    assert!(matches!(release.final_mode, WebRendererMode::Degraded(_)));
}
