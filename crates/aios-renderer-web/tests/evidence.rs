//! T-147 — Evidence emission integration tests for aios-renderer-web.
//!
//! Covers standalone emitter tests, chain integrity, INV-015 payload
//! invariants, no-emitter backward compatibility, and the full set of
//! 9 `WebRecordType` variants.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::no_effect_underscore_binding,
    missing_docs,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use ed25519_dalek::Signer;

use aios_renderer_kde::NodeKind;
use aios_renderer_web::{
    ChromeIntegrityMonitor, ExposureFsm, ExposureLevel, InMemoryWebEvidenceEmitter,
    InMemoryWebRenderer, OriginScheme, WebEvidenceEmitter, WebRecordType, WebRenderer,
    WebSurfaceDescriptor,
};

// ── Helpers ──────────────────────────────────────────────────────────────

fn emitter() -> Arc<InMemoryWebEvidenceEmitter> {
    Arc::new(InMemoryWebEvidenceEmitter::new("service:aios-renderer-web"))
}

fn descriptor_fixture() -> WebSurfaceDescriptor {
    let origin = OriginScheme::parse("https://acme-app.aios.localhost:8443").unwrap();
    WebSurfaceDescriptor::new(origin, NodeKind::Container, "test-claimant").unwrap()
}

fn exposure_level_lan_active() -> ExposureLevel {
    let now = chrono::Utc::now();
    ExposureLevel::LanActive {
        activated_at: now,
        last_heartbeat_at: now,
    }
}

// ── 1. Standalone emitter — surface allocated ────────────────────────────

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

// ── 2. Standalone emitter — surface released ─────────────────────────────

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

// ── 3. Standalone emitter — exposure transition ──────────────────────────

#[tokio::test]
async fn emitter_emits_exposure_transition_receipt() {
    let em = emitter();

    let receipt = em
        .emit_exposure_transition("Localhost", "LanPending", "operator request")
        .await
        .expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

// ── 4. Standalone emitter — exposure granted (FOREVER) ───────────────────

#[tokio::test]
async fn emitter_emits_exposure_granted_receipt() {
    let em = emitter();
    let level = exposure_level_lan_active();

    let receipt = em
        .emit_exposure_granted(&level, "dec-001")
        .await
        .expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

// ── 5. Standalone emitter — LAN exposure active ──────────────────────────

#[tokio::test]
async fn emitter_emits_lan_exposure_active_receipt() {
    let em = emitter();
    let level = exposure_level_lan_active();

    let receipt = em.emit_lan_exposure_active(&level).await.expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

// ── 6. Standalone emitter — renderer degraded ────────────────────────────

#[tokio::test]
async fn emitter_emits_renderer_degraded_receipt() {
    let em = emitter();

    let receipt = em.emit_renderer_degraded("gpu fault").await.expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

// ── 7. Standalone emitter — extension interference ───────────────────────

#[tokio::test]
async fn emitter_emits_extension_interference_receipt() {
    let em = emitter();

    let receipt = em
        .emit_extension_interference("deadbeef", "unknown-subtree")
        .await
        .expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

// ── 8. Standalone emitter — icon bundle verified ─────────────────────────

#[tokio::test]
async fn emitter_emits_icon_bundle_verified_receipt() {
    let em = emitter();

    let receipt = em
        .emit_icon_bundle_verified("theme-dk", "fp:abc123")
        .await
        .expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

// ── 9. Standalone emitter — icon bundle rejected ─────────────────────────

#[tokio::test]
async fn emitter_emits_icon_bundle_rejected_receipt() {
    let em = emitter();

    let receipt = em
        .emit_icon_bundle_rejected("theme-dk", "invalid signature")
        .await
        .expect("emit");

    assert!(!receipt.record_id.is_empty());
    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain integrity");
}

// ── 10. Chain integrity holds across multiple emissions ───────────────────

#[tokio::test]
async fn evidence_chain_integrity_holds_across_multiple_emissions() {
    let em = emitter();
    let desc = descriptor_fixture();
    let level = exposure_level_lan_active();

    // Emit 5 different events
    em.emit_surface_allocated(&desc, "actor").await.unwrap();
    em.emit_exposure_transition("Localhost", "LanPending", "reason")
        .await
        .unwrap();
    em.emit_exposure_granted(&level, "dec-001").await.unwrap();
    em.emit_lan_exposure_active(&level).await.unwrap();
    em.emit_extension_interference("hash1", "kind")
        .await
        .unwrap();

    assert_eq!(em.receipt_count().await, 5);
    em.verify_chain()
        .await
        .expect("chain integrity after 5 emissions");
}

// ── 11. No emitter on renderer preserves existing behavior ────────────────

#[tokio::test]
async fn no_emitter_on_renderer_preserves_allocation_behavior() {
    let r = InMemoryWebRenderer::new();
    let origin = OriginScheme::parse("https://acme-app.aios.localhost:8443").unwrap();
    let req = aios_renderer_web::AllocateWebSurfaceRequest {
        origin,
        node_kind: NodeKind::Container,
        claimed_by: "test".into(),
        expected_group_id: None,
    };
    let result = r.allocate_surface(req).await;
    assert!(result.is_ok(), "allocation must succeed without emitter");
}

// ── 12. No emitter on ExposureFsm preserves behavior ──────────────────────

#[tokio::test]
async fn no_emitter_on_exposure_fsm_preserves_escalation() {
    let fsm = ExposureFsm::new();
    fsm.request_lan_escalation("operator:root").await.unwrap();
    let current = fsm.current().await;
    assert!(matches!(current, ExposureLevel::LanPending { .. }));
}

// ── 13. No emitter on ChromeIntegrityMonitor preserves behavior ───────────

#[tokio::test]
async fn no_emitter_on_chrome_integrity_preserves_admit_and_check() {
    let signing_key = ed25519_dalek::SigningKey::generate(&mut rand_core::OsRng);
    let monitor = ChromeIntegrityMonitor::new(signing_key.verifying_key());

    let root_hash = "abc123";
    let sig = signing_key.sign(root_hash.as_bytes());
    let fragment = aios_renderer_web::ChromeTreeFragment {
        root_hash: root_hash.to_string(),
        signature: sig.to_bytes().to_vec(),
        signed_at: chrono::Utc::now(),
    };
    monitor.admit_signed_fragment(fragment).await.unwrap();
    monitor.check_observed_hash(root_hash).await.unwrap();

    assert_eq!(monitor.history().await.len(), 1);
}

// ── 14. WebRecordType as_str covers all 9 variants ────────────────────────

#[test]
fn web_record_type_as_str_covers_all_nine_variants() {
    let variants = [
        (WebRecordType::WebSurfaceAllocated, "WEB_SURFACE_ALLOCATED"),
        (WebRecordType::WebSurfaceReleased, "WEB_SURFACE_RELEASED"),
        (
            WebRecordType::WebExposureTransition,
            "WEB_EXPOSURE_TRANSITION",
        ),
        (WebRecordType::WebExposureGranted, "WEB_EXPOSURE_GRANTED"),
        (
            WebRecordType::WebLanExposureActive,
            "WEB_LAN_EXPOSURE_ACTIVE",
        ),
        (WebRecordType::WebRendererDegraded, "WEB_RENDERER_DEGRADED"),
        (
            WebRecordType::WebExtensionInterference,
            "WEB_EXTENSION_INTERFERENCE",
        ),
        (
            WebRecordType::WebIconBundleVerified,
            "WEB_ICON_BUNDLE_VERIFIED",
        ),
        (
            WebRecordType::WebIconBundleRejected,
            "WEB_ICON_BUNDLE_REJECTED",
        ),
    ];
    for (variant, expected) in variants {
        assert_eq!(variant.as_str(), expected);
    }
}

// ── 15. get_payload returns correct JSON ──────────────────────────────────

#[tokio::test]
async fn get_payload_returns_correct_json() {
    let em = emitter();
    let desc = descriptor_fixture();

    em.emit_surface_allocated(&desc, "test-actor")
        .await
        .unwrap();

    let payload = em.get_payload(0).await.unwrap();
    assert_eq!(
        payload["surface_id"],
        serde_json::Value::String(desc.id.to_string())
    );
    assert_eq!(
        payload["origin"],
        serde_json::Value::String(desc.origin.full_origin)
    );
    assert_eq!(
        payload["actor"],
        serde_json::Value::String("test-actor".to_string())
    );
}

// ── 16. verify_chain on empty chain returns error ─────────────────────────

#[tokio::test]
async fn verify_chain_on_empty_chain_returns_error() {
    let em = emitter();
    let result = em.verify_chain().await;
    assert!(result.is_err(), "empty chain should return an error");
    assert_eq!(em.receipt_count().await, 0);
}
