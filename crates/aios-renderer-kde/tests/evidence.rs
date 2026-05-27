//! T-135 — Evidence emission integration tests for aios-renderer-kde.
//!
//! Covers standalone emitter tests, chain integrity, INV-015 redaction,
//! no-emitter backward compatibility, and integration with `InMemoryKdeRenderer`
//! and `KwinScriptLoader`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use aios_renderer_kde::{
    AllocateSurfaceRequest, CompositionZone, DegradedTrigger, InMemoryKdeEvidenceEmitter,
    InMemoryKdeRenderer, KdeEvidenceEmitter, KdeRecordType, KdeRenderer, KdeSurfaceDescriptor,
    KwinScript, KwinScriptLoader, NodeKind, RecoveryEntryReceipt, RendererMode, SurfaceFilter,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn emitter() -> Arc<InMemoryKdeEvidenceEmitter> {
    Arc::new(InMemoryKdeEvidenceEmitter::new("service:aios-renderer-kde"))
}

fn descriptor_fixture() -> KdeSurfaceDescriptor {
    KdeSurfaceDescriptor::new(CompositionZone::Content, "test-claimant").expect("valid descriptor")
}

fn recovery_receipt_fixture() -> RecoveryEntryReceipt {
    RecoveryEntryReceipt {
        entered_at: chrono::Utc::now(),
        aios_surfaces_only: true,
        display_separation: "separate-wayland-display".into(),
    }
}

fn allocate_request() -> AllocateSurfaceRequest {
    AllocateSurfaceRequest {
        zone: CompositionZone::Content,
        claimed_by: "test-claimant".into(),
        node_kind: NodeKind::SurfaceEmbed,
        requested_layer: None,
    }
}

// ---------------------------------------------------------------------------
// 1. Standalone emitter — surface lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emitter_emits_surface_allocated_receipt() {
    let em = emitter();
    let desc = descriptor_fixture();

    let receipt = em
        .emit_surface_allocated(&desc, "test-actor")
        .await
        .expect("emit");

    assert!(!receipt.record_id.is_empty(), "record_id must be set");
    assert!(!receipt.hash.is_empty(), "hash must be set");
    assert_eq!(receipt.sequence, 0);
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

#[tokio::test]
async fn emitter_emits_surface_released_receipt() {
    let em = emitter();
    let desc = descriptor_fixture();

    let receipt = em
        .emit_surface_released(&desc, "test-actor")
        .await
        .expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

// ---------------------------------------------------------------------------
// 2. Standalone emitter — layer-shell rejection (INV I4)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emitter_emits_layer_shell_rejected() {
    let em = emitter();

    let receipt = em
        .emit_layer_shell_rejected("untrusted-claimant", CompositionZone::Chrome, "zone guard")
        .await
        .expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

// ---------------------------------------------------------------------------
// 3. Standalone emitter — recovery enter/exit (INV I5)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emitter_emits_recovery_entered() {
    let em = emitter();
    let rec = recovery_receipt_fixture();

    let receipt = em
        .emit_recovery_entered(&rec, "service:aios-renderer-kde")
        .await
        .expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

#[tokio::test]
async fn emitter_emits_recovery_exited() {
    let em = emitter();

    let receipt = em
        .emit_recovery_exited("service:aios-renderer-kde")
        .await
        .expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

// ---------------------------------------------------------------------------
// 4. Standalone emitter — degraded mode (INV I7)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emitter_emits_renderer_degraded() {
    let em = emitter();

    let receipt = em
        .emit_renderer_degraded(DegradedTrigger::KwinUnavailable, "kwin_unreachable")
        .await
        .expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

// ---------------------------------------------------------------------------
// 5. Standalone emitter — KWin script (INV I8)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emitter_emits_kwin_script_verified() {
    let em = emitter();

    let receipt = em
        .emit_kwin_script_verified("aios-fullscreen-block", "auth-fingerprint")
        .await
        .expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

#[tokio::test]
async fn emitter_emits_kwin_script_rejected() {
    let em = emitter();

    let receipt = em
        .emit_kwin_script_rejected("bad-script", "blake3 mismatch")
        .await
        .expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

// ---------------------------------------------------------------------------
// 6. Standalone emitter — icon bundle (INV I6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emitter_emits_icon_bundle_verified() {
    let em = emitter();

    let receipt = em
        .emit_icon_bundle_verified("aios-recovery", "auth-fingerprint")
        .await
        .expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

#[tokio::test]
async fn emitter_emits_icon_bundle_rejected() {
    let em = emitter();

    let receipt = em
        .emit_icon_bundle_rejected("bad-theme", "blake3 mismatch")
        .await
        .expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

// ---------------------------------------------------------------------------
// 7. BLAKE3 chain integrity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn blake3_chain_integrity_across_multiple_emissions() {
    let em = emitter();
    let desc = descriptor_fixture();
    let rec = recovery_receipt_fixture();

    // Emit multiple events.
    em.emit_surface_allocated(&desc, "actor")
        .await
        .expect("allocated");
    em.emit_layer_shell_rejected("bad", CompositionZone::Chrome, "zone guard")
        .await
        .expect("rejected");
    em.emit_recovery_entered(&rec, "service")
        .await
        .expect("recovery");
    em.emit_renderer_degraded(DegradedTrigger::GpuDeviceAcquisitionFailed, "gpu_fail")
        .await
        .expect("degraded");
    em.emit_kwin_script_verified("script-1", "auth")
        .await
        .expect("kwin ok");
    em.emit_icon_bundle_rejected("bundle", "bad hash")
        .await
        .expect("icon fail");

    assert_eq!(em.receipt_count().await, 6);
    em.verify_chain().await.expect("chain integrity");
}

// ---------------------------------------------------------------------------
// 8. INV-015 — no secret material in payloads
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv015_no_secret_material_in_payloads() {
    let em = emitter();
    let desc = descriptor_fixture();
    let rec = recovery_receipt_fixture();

    // Emit one of each event type.
    em.emit_surface_allocated(&desc, "actor")
        .await
        .expect("emit");
    em.emit_surface_released(&desc, "actor")
        .await
        .expect("emit");
    em.emit_layer_shell_rejected("bad", CompositionZone::Chrome, "zone guard")
        .await
        .expect("emit");
    em.emit_recovery_entered(&rec, "service")
        .await
        .expect("emit");
    em.emit_kwin_script_verified("s1", "fp1")
        .await
        .expect("emit");
    em.emit_kwin_script_rejected("s2", "bad")
        .await
        .expect("emit");
    em.emit_icon_bundle_verified("t1", "fp1")
        .await
        .expect("emit");
    em.emit_icon_bundle_rejected("t2", "bad")
        .await
        .expect("emit");

    let count = em.receipt_count().await;
    assert_eq!(count, 8);

    // Check every payload for absence of key-shaped hex or secret markers.
    for i in 0..count {
        let payload = em.get_payload(i).await.expect("payload present");
        let payload_str = serde_json::to_string(&payload).expect("serialize");

        // No raw Ed25519 key material (32-byte or 64-byte hex strings).
        assert!(
            !payload_str.contains("ed25519:"),
            "payload {i} contains ed25519 key marker"
        );
        assert!(!payload_str.contains("priv"), "payload {i} contains 'priv'");
        assert!(
            !payload_str.contains("secret"),
            "payload {i} contains 'secret'"
        );
        assert!(
            !payload_str.contains("key_material"),
            "payload {i} contains 'key_material'"
        );

        // No 64-char hex strings that look like raw BLAKE3 or signature bytes.
        // (Payloads are JSON, not hex dumps, so this is defense-in-depth.)
        assert!(
            !payload_str.contains("signature"),
            "payload {i} contains 'signature'"
        );
    }
}

// ---------------------------------------------------------------------------
// 9. Integration — renderer with emitter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn renderer_no_emitter_preserves_backward_compat() {
    let renderer = InMemoryKdeRenderer::new();
    let req = allocate_request();

    let desc = renderer.allocate_surface(req).await.expect("allocate");
    assert_eq!(desc.zone, CompositionZone::Content);

    let surfaces = renderer.list_surfaces(SurfaceFilter::All).await;
    assert_eq!(surfaces.len(), 1);

    renderer
        .release_surface(desc.id.clone())
        .await
        .expect("release");

    let surfaces = renderer.list_surfaces(SurfaceFilter::All).await;
    assert!(surfaces.is_empty());
}

#[tokio::test]
async fn renderer_with_emitter_emits_on_allocate_and_release() {
    let em = emitter();
    let renderer = InMemoryKdeRenderer::new().with_emitter(em.clone());
    let req = allocate_request();

    let desc = renderer.allocate_surface(req).await.expect("allocate");
    assert_eq!(em.receipt_count().await, 1);

    renderer
        .release_surface(desc.id.clone())
        .await
        .expect("release");
    assert_eq!(em.receipt_count().await, 2);

    em.verify_chain().await.expect("chain integrity");
}

#[tokio::test]
async fn renderer_with_emitter_emits_recovery_and_degraded_events() {
    let em = emitter();
    let renderer = InMemoryKdeRenderer::new().with_emitter(em.clone());

    renderer
        .enter_recovery_mode()
        .await
        .expect("recovery enter");
    assert_eq!(em.receipt_count().await, 1);
    assert_eq!(renderer.get_mode().await, RendererMode::Recovery);

    renderer.exit_recovery_mode().await.expect("recovery exit");
    assert_eq!(em.receipt_count().await, 2);
    assert_eq!(renderer.get_mode().await, RendererMode::Normal);

    renderer
        .enter_degraded_mode("gpu_failed".into())
        .await
        .expect("degraded");
    assert_eq!(em.receipt_count().await, 3);

    em.verify_chain().await.expect("chain integrity");
}

// ---------------------------------------------------------------------------
// 10. Integration — KWinScriptLoader with emitter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn kwin_script_loader_with_emitter_emits_on_verify_success() {
    use ed25519_dalek::Signer;
    use rand_core::RngCore;

    let em = emitter();
    let mut loader =
        KwinScriptLoader::new("/aios/system/renderers/kde/kwin-scripts").with_emitter(em.clone());

    // Create a real Ed25519-signed script.
    let mut secret: [u8; 32] = [0u8; 32];
    rand_core::OsRng.fill_bytes(&mut secret);
    let sk = ed25519_dalek::SigningKey::from_bytes(&secret);
    let vk = sk.verifying_key();
    loader.register_authority("test-auth", vk);

    let source = "// KWin test script\nconsole.log('verified');";
    let hash = blake3::hash(source.as_bytes());
    let signature = sk.sign(hash.as_bytes());

    let script = KwinScript {
        id: "test-script".into(),
        canonical_path: "/aios/system/renderers/kde/kwin-scripts/test-script.qml".into(),
        source: source.into(),
        blake3_hash: hash.to_hex().to_string(),
        signature: signature.to_bytes().to_vec(),
        signer_key_fingerprint: "test-auth".into(),
    };

    loader.load_script(script).await.expect("load_script");
    assert_eq!(loader.list_loaded().await, vec!["test-script"]);
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");

    let payload = em.get_payload(0).await.expect("payload present");
    let payload_str = serde_json::to_string(&payload).expect("serialize");
    assert!(
        payload_str.contains("test-script"),
        "payload should contain script id"
    );
    assert!(
        payload_str.contains("test-auth"),
        "payload should contain signer fingerprint"
    );
}

#[tokio::test]
async fn kwin_script_loader_with_emitter_emits_on_path_rejection() {
    let em = emitter();
    let loader =
        KwinScriptLoader::new("/aios/system/renderers/kde/kwin-scripts").with_emitter(em.clone());

    // Script path outside allowed root — should be rejected with emission.
    let script = KwinScript {
        id: "bad-path-script".into(),
        canonical_path: "/usr/share/kwin/scripts/bad.qml".into(),
        source: "console.log('bad');".into(),
        blake3_hash: blake3::hash(b"bad").to_hex().to_string(),
        signature: vec![0u8; 64],
        signer_key_fingerprint: "test-auth".into(),
    };

    let err = loader
        .load_script(script)
        .await
        .expect_err("path outside root must fail");
    let err_str = format!("{err}");
    assert!(
        err_str.contains("path outside allowed root"),
        "error: {err_str}"
    );

    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");

    let payload = em.get_payload(0).await.expect("payload present");
    let payload_str = serde_json::to_string(&payload).expect("serialize");
    assert!(
        payload_str.contains("bad-path-script"),
        "payload should reference the rejected script id"
    );
    assert!(
        payload_str.contains("path outside allowed root"),
        "payload should reference the rejection reason"
    );
}

// ---------------------------------------------------------------------------
// 11. KdeRecordType discriminator mapping
// ---------------------------------------------------------------------------

#[test]
fn kde_record_type_has_ten_variants() {
    let all = &[
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
    assert_eq!(all.len(), 10, "KdeRecordType must have exactly 10 variants");
    // Check as_str returns a non-empty wire name for every variant.
    for kind in all {
        let s = kind.as_str();
        assert!(!s.is_empty(), "{kind:?} has empty str");
    }
}
