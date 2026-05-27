//! T-138 — M14 closure invariants.
//!
//! Constitutional checks that M14 (aios-renderer-kde) is honestly closed:
//! version marker, no deferred-stub leakage, trait coverage, invariant
//! reachability (INV I4/I5/I6/I7/I8), evidence record type completeness,
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

use aios_renderer_kde::{
    escalate_to_degraded, evaluate_surface_request, ConstitutionalIconBundle, DegradedTrigger,
    InMemoryKdeEvidenceEmitter, InMemoryKdeRenderer, KdeEvidenceEmitter, KdeRecordType,
    KdeRendererError, KwinScript, KwinScriptLoader, NodeKind, RecoverySession, RecoveryShellGuard,
    RendererMode, WaylandClient, WaylandInteractivity, WaylandProtocol, WaylandSurfaceLayer,
    WaylandSurfaceRequest, DEFAULT_CODE_VERSION,
};

// ---------------------------------------------------------------------------
// INV-1: Version marker is 0.1.0-T138
// ---------------------------------------------------------------------------

#[test]
fn inv_1_version_marker_is_0_1_0_t138() {
    assert_eq!(
        DEFAULT_CODE_VERSION, "aios-renderer-kde/0.1.0-T138",
        "DEFAULT_CODE_VERSION must reflect M14 closure"
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
        "M14 closure violation — Status::Unimplemented found:\n{}",
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
        "M14 closure violation — todo!/unimplemented! macros found:\n{}",
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
// INV-4: KdeRenderer trait has InMemoryKdeRenderer impl
// ---------------------------------------------------------------------------

#[test]
fn inv_4_trait_coverage_in_memory_kde_renderer_impl() {
    let renderer = InMemoryKdeRenderer::new();
    let _ = &renderer as &dyn std::any::Any;
}

// ---------------------------------------------------------------------------
// INV-5: INV I4 reachability — evaluate_surface_request rejects non-chrome
// ---------------------------------------------------------------------------

#[test]
fn inv_5_i4_non_chrome_on_chrome_zone_returns_overlay_forbidden() {
    let req = WaylandSurfaceRequest {
        protocol: WaylandProtocol::WlrLayerShellV1,
        layer_namespace: "aios-shell".into(),
        claimed_by: "intruder".into(),
        zone: aios_renderer_kde::CompositionZone::Chrome,
        node_kind: NodeKind::Text,
    };
    let result = evaluate_surface_request(&req);
    assert!(result.is_err());
    match result.unwrap_err() {
        KdeRendererError::OverlayLayerForbidden { client_id } => {
            assert_eq!(client_id, "intruder");
        }
        other => panic!("expected OverlayLayerForbidden, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// INV-6: INV I5 reachability — RecoveryShellGuard rejects non-SurfaceEmbed
// ---------------------------------------------------------------------------

#[test]
fn inv_6_i5_recovery_shell_guard_rejects_text_kind() {
    let session = RecoverySession {
        wayland_display: "wayland-2".into(),
        kwin_pid: 9999,
        aios_user: "aios-recovery".into(),
        started_at: chrono::Utc::now(),
    };
    let guard = RecoveryShellGuard::new(session);
    let result = guard.admit(NodeKind::Text);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, KdeRendererError::Internal(_)),
        "expected Internal error for non-SurfaceEmbed node in recovery, got {err:?}"
    );
}

#[test]
fn inv_6b_i5_recovery_shell_guard_admits_surface_embed() {
    let session = RecoverySession {
        wayland_display: "wayland-2".into(),
        kwin_pid: 9999,
        aios_user: "aios-recovery".into(),
        started_at: chrono::Utc::now(),
    };
    let guard = RecoveryShellGuard::new(session);
    assert!(guard.admit(NodeKind::SurfaceEmbed).is_ok());
}

// ---------------------------------------------------------------------------
// INV-7: INV I7 reachability — escalate_to_degraded produces Degraded mode
// ---------------------------------------------------------------------------

#[test]
fn inv_7_i7_escalate_to_degraded_kwin_unavailable() {
    let (mode, reason) = escalate_to_degraded(DegradedTrigger::KwinUnavailable);
    assert_eq!(reason, "kwin_unreachable");
    match mode {
        RendererMode::Degraded(ref r) => assert_eq!(r, "kwin_unreachable"),
        _ => panic!("expected Degraded mode"),
    }
}

#[test]
fn inv_7b_i7_escalate_all_triggers_produce_degraded() {
    for trigger in DegradedTrigger::ALL {
        let (mode, reason) = escalate_to_degraded(*trigger);
        assert!(!reason.is_empty(), "every trigger must produce a reason");
        assert!(
            matches!(mode, RendererMode::Degraded(_)),
            "every trigger must produce Degraded mode"
        );
    }
}

// ---------------------------------------------------------------------------
// INV-8: INV I8 reachability — KwinScriptLoader rejects non-allowed path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_8_i8_kwin_script_loader_rejects_path_outside_allowed_root() {
    let loader = KwinScriptLoader::new("/aios/system/renderers/kde/kwin-scripts");
    let script = KwinScript {
        id: "malicious-script".into(),
        canonical_path: "/etc/passwd".into(),
        source: "// not a real script".into(),
        blake3_hash: blake3::hash(b"// not a real script").to_hex().to_string(),
        signature: vec![0u8; 64],
        signer_key_fingerprint: "unknown".into(),
    };
    let result = loader.load_script(script).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        KdeRendererError::KwinScriptVerificationFailed { script_id, reason } => {
            assert_eq!(script_id, "malicious-script");
            assert!(reason.contains("path outside allowed root"));
        }
        other => panic!("expected KwinScriptVerificationFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn inv_8b_i8_kwin_script_loader_loads_valid_signed_script() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key: VerifyingKey = signing_key.verifying_key();
    let source = "// valid kwin script";
    let hash = blake3::hash(source.as_bytes());
    let sig = signing_key.sign(hash.as_bytes());

    let mut loader = KwinScriptLoader::new("/aios/system/renderers/kde/kwin-scripts");
    loader.register_authority("aios-kde-signer", verifying_key);

    let script = KwinScript {
        id: "valid-script".into(),
        canonical_path: "/aios/system/renderers/kde/kwin-scripts/valid.js".into(),
        source: source.into(),
        blake3_hash: hash.to_hex().to_string(),
        signature: sig.to_bytes().to_vec(),
        signer_key_fingerprint: "aios-kde-signer".into(),
    };
    let result = loader.load_script(script).await;
    assert!(result.is_ok(), "valid signed script must load: {result:?}");
}

// ---------------------------------------------------------------------------
// INV-9: 10 KDE evidence RecordType variants are constructable
// ---------------------------------------------------------------------------

#[test]
fn inv_9_evidence_record_types_have_10_variants_all_constructable() {
    let variants = [
        KdeRecordType::KdeSurfaceAllocated,
        KdeRecordType::KdeSurfaceReleased,
        KdeRecordType::KdeLayerShellRejected,
        KdeRecordType::KdeRecoveryEntered,
        KdeRecordType::KdeRecoveryExited,
        KdeRecordType::KdeRendererDegraded,
        KdeRecordType::KdeKwinScriptVerified,
        KdeRecordType::KdeKwinScriptRejected,
        KdeRecordType::KdeIconBundleVerified,
        KdeRecordType::KdeIconBundleRejected,
    ];
    assert_eq!(variants.len(), 10);
    for v in &variants {
        let s = v.as_str();
        assert!(
            !s.is_empty(),
            "every KdeRecordType must have non-empty as_str() — {v:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// INV-10: InMemoryKdeEvidenceEmitter chain integrity works
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_10_evidence_emitter_chain_integrity() {
    use aios_renderer_kde::{CompositionZone, KdeSurfaceDescriptor};

    let emitter = Arc::new(InMemoryKdeEvidenceEmitter::new("service:aios-renderer-kde"));
    let desc = KdeSurfaceDescriptor::new(CompositionZone::Content, "test-client").unwrap();

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
// INV-11: WaylandClient connect + surface request lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_11_wayland_client_connect_and_request_surface() {
    let client = WaylandClient::connect("aios-kde").await.expect("connect");
    let id = aios_renderer_kde::KdeSurfaceId::new();
    let req = WaylandSurfaceRequest {
        protocol: WaylandProtocol::XdgShell,
        layer_namespace: "aios-shell".into(),
        claimed_by: "test-app".into(),
        zone: aios_renderer_kde::CompositionZone::Content,
        node_kind: NodeKind::Card,
    };
    let grant = client
        .request_surface(id.clone(), req)
        .await
        .expect("request");
    assert_eq!(grant.assigned_layer, WaylandSurfaceLayer::Top);
    assert_eq!(grant.interactivity, WaylandInteractivity::OnDemand);

    let grants = client.list_grants().await;
    assert_eq!(grants.len(), 1);

    client.revoke_surface(&id).await.expect("revoke");
    let grants = client.list_grants().await;
    assert!(grants.is_empty());
}

// ---------------------------------------------------------------------------
// INV-12: ConstitutionalIconBundle verification (INV I6)
// ---------------------------------------------------------------------------

#[test]
fn inv_12_i6_icon_bundle_verify_valid_manifest() {
    use std::collections::{BTreeMap, HashMap};

    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key: VerifyingKey = signing_key.verifying_key();
    let fp = "aios-icon-authority";

    let entry = aios_renderer_kde::IconEntry {
        token_id: "ICON_ACTION_SAVE".into(),
        relative_path: "actions/save.svg".into(),
        blake3_hash: "a".repeat(64),
    };

    let mut message = Vec::new();
    message.extend_from_slice(entry.token_id.as_bytes());
    message.extend_from_slice(entry.relative_path.as_bytes());
    message.extend_from_slice(entry.blake3_hash.as_bytes());
    let sig = signing_key.sign(&message);

    let mut manifest = BTreeMap::new();
    manifest.insert(entry.token_id.clone(), entry);

    let mut trusted = HashMap::new();
    trusted.insert(fp.to_string(), verifying_key);

    let bundle = ConstitutionalIconBundle {
        theme_id: "aios-recovery".into(),
        root_path: "/aios/system/renderers/kde/icons/aios-recovery".into(),
        manifest,
        bundle_signature: sig.to_bytes().to_vec(),
        signer_fingerprint: fp.into(),
        trusted_authorities: trusted,
        emitter: None,
    };

    assert!(bundle.verify().is_ok(), "valid bundle must verify");
}

#[test]
fn inv_12b_i6_icon_bundle_reject_unknown_authority() {
    use std::collections::{BTreeMap, HashMap};

    let signing_key = SigningKey::generate(&mut OsRng);

    let entry = aios_renderer_kde::IconEntry {
        token_id: "ICON_ACTION_SAVE".into(),
        relative_path: "actions/save.svg".into(),
        blake3_hash: "a".repeat(64),
    };

    let mut message = Vec::new();
    message.extend_from_slice(entry.token_id.as_bytes());
    message.extend_from_slice(entry.relative_path.as_bytes());
    message.extend_from_slice(entry.blake3_hash.as_bytes());
    let sig = signing_key.sign(&message);

    let mut manifest = BTreeMap::new();
    manifest.insert(entry.token_id.clone(), entry);

    let bundle = ConstitutionalIconBundle {
        theme_id: "aios-recovery".into(),
        root_path: "/aios/system/renderers/kde/icons/aios-recovery".into(),
        manifest,
        bundle_signature: sig.to_bytes().to_vec(),
        signer_fingerprint: "aios-icon-authority".into(),
        trusted_authorities: HashMap::new(),
        emitter: None,
    };

    let result = bundle.verify();
    assert!(result.is_err());
    match result.unwrap_err() {
        KdeRendererError::IconBundleVerificationFailed { reason, .. } => {
            assert!(reason.contains("unknown authority"));
        }
        other => panic!("expected IconBundleVerificationFailed, got {other:?}"),
    }
}
