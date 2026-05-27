//! T-150 — M15 acceptance fixtures.
//!
//! End-to-end scenarios across 6 phases proving the aios-renderer-web crate
//! is honestly closed per the M15 spec contract.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::cast_possible_wrap,
    clippy::items_after_statements,
    clippy::significant_drop_tightening,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;

use aios_renderer_web::{
    assert_three_way_parity_for_apps_domain, AllocateWebSurfaceRequest, ChromeIntegrityMonitor,
    ChromeTreeFragment, ExposureFsm, ExposureLevel, IframeOriginBinding,
    InMemoryWebEvidenceEmitter, InMemoryWebRenderer, NodeKind, OriginScheme, OriginToken,
    OriginVerifier, ParsedOrigin, WebRenderer, WebRendererMode, WebSurfaceId,
};

// =========================================================================
// Phase 1 — Chrome surface allocation
// =========================================================================

/// 1. Allocate a chrome surface and verify its descriptor.
#[tokio::test]
async fn phase_1_allocate_chrome_surface_produces_valid_descriptor() {
    let renderer = InMemoryWebRenderer::new();

    let origin = ParsedOrigin {
        scheme: OriginScheme::AppOrigin(OriginToken("aios".into())),
        host: "aios.localhost".into(),
        port: 8443,
        full_origin: "https://aios.localhost:8443".into(),
    };
    let req = AllocateWebSurfaceRequest {
        origin: origin.clone(),
        node_kind: NodeKind::SurfaceEmbed,
        claimed_by: "chrome-service".into(),
        expected_group_id: None,
    };
    let desc = renderer
        .allocate_surface(req)
        .await
        .expect("allocate chrome surface");

    assert_eq!(desc.claimed_by, "chrome-service");
    assert_eq!(desc.node_kind, NodeKind::SurfaceEmbed);
    assert!(matches!(desc.mode, WebRendererMode::Normal));
    assert_eq!(desc.origin.full_origin, "https://aios.localhost:8443");

    // Verify via get_surface
    let retrieved = renderer
        .get_surface(desc.id.clone())
        .await
        .expect("get surface");
    assert_eq!(retrieved.id, desc.id);
}

// =========================================================================
// Phase 2 — INV I4 negative: origin verifier rejects mismatched binding
// =========================================================================

/// 2. OriginVerifier rejects a composition with wrong group id.
#[tokio::test]
async fn phase_2_origin_verifier_rejects_wrong_group() {
    let verifier = OriginVerifier::new();
    let surface_id = WebSurfaceId::new();

    // Register a binding for the chrome origin (aios.localhost → AppOrigin)
    let binding = IframeOriginBinding {
        iframe_origin: "https://aios.localhost:8443".into(),
        surface_id: surface_id.clone(),
        bound_group_id: "aios".into(),
        scope_binding_evidence_id: "evr_phase2_001".into(),
    };
    verifier.register_binding(binding).await.expect("register");

    // Attempt to verify with a different origin → must fail
    let result = verifier
        .verify_composition(&surface_id, "https://intruder.localhost:8443")
        .await;
    assert!(result.is_err(), "must reject mismatched origin");
}

// =========================================================================
// Phase 3 — Exposure escalation: full FSM chain
// =========================================================================

/// 3. Full Localhost → LanPending → LanApproved → LanActive → heartbeat → revoke.
#[tokio::test]
async fn phase_3_full_exposure_escalation_chain() {
    let emitter = Arc::new(InMemoryWebEvidenceEmitter::new("service:aios-renderer-web"));
    let fsm = ExposureFsm::new().with_evidence_emitter(emitter.clone());

    // Step A: start at Localhost
    assert_eq!(fsm.current().await, ExposureLevel::Localhost);

    // Step B: request LAN escalation
    fsm.request_lan_escalation("operator-01")
        .await
        .expect("request lan");

    // Step C: policy approves
    fsm.apply_policy_decision("evr_decision_phase3")
        .await
        .expect("policy decision");

    // Step D: activate LAN
    fsm.activate_lan_exposure().await.expect("activate lan");
    assert!(
        matches!(fsm.current().await, ExposureLevel::LanActive { .. }),
        "must be LanActive"
    );

    // Step E: record heartbeat
    fsm.record_heartbeat().await.expect("heartbeat");

    // Step F: revoke
    fsm.revoke("operator decommission").await.expect("revoke");
    assert!(
        matches!(fsm.current().await, ExposureLevel::Revoked { .. }),
        "must be Revoked"
    );

    // Step G: reset to localhost
    fsm.reset_to_localhost().await.expect("reset");
    assert_eq!(fsm.current().await, ExposureLevel::Localhost);
}

// =========================================================================
// Phase 4 — Chrome shadow-root integrity (INV I10)
// =========================================================================

/// 4. ChromeIntegrityMonitor admits signed fragments, detects interference.
#[tokio::test]
async fn phase_4_chrome_integrity_workflow() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key: VerifyingKey = signing_key.verifying_key();

    let emitter = Arc::new(InMemoryWebEvidenceEmitter::new("service:aios-chrome"));
    let monitor = ChromeIntegrityMonitor::new(verifying_key).with_evidence_emitter(emitter.clone());

    // Admit a signed fragment
    let root_hash = "abcd1234".repeat(8); // 64 hex chars
    let sig = signing_key.sign(root_hash.as_bytes());
    let fragment = ChromeTreeFragment {
        root_hash: root_hash.clone(),
        signature: sig.to_bytes().to_vec(),
        signed_at: chrono::Utc::now(),
    };
    monitor
        .admit_signed_fragment(fragment)
        .await
        .expect("admit fragment");

    // Known hash → OK
    monitor
        .check_observed_hash(&root_hash)
        .await
        .expect("known hash must pass");

    // Unknown hash → extension interference detected
    let unknown = "bad11111".repeat(8);
    let result = monitor.check_observed_hash(&unknown).await;
    assert!(result.is_err(), "unknown hash must trigger interference");

    // History must have 2 entries
    let history = monitor.history().await;
    assert_eq!(history.len(), 2, "must have 2 integrity check records");
}

// =========================================================================
// Phase 5 — Recovery + degraded mode (INV I7/I8)
// =========================================================================

/// 5. Enter recovery mode → verify service worker disabled → exit → degrade.
#[tokio::test]
async fn phase_5_recovery_and_degraded_mode_cycle() {
    let renderer = InMemoryWebRenderer::new();

    // Enter recovery
    let receipt = renderer
        .enter_recovery_mode()
        .await
        .expect("enter recovery");
    assert!(
        receipt.service_worker_disabled,
        "INV I8: SW must be disabled in recovery"
    );
    assert_eq!(renderer.get_mode().await, WebRendererMode::Recovery);

    // Recovery mode: only recovery origin allowed for allocation
    let recovery_origin = ParsedOrigin {
        scheme: OriginScheme::Recovery,
        host: "recovery.localhost".into(),
        port: 8443,
        full_origin: "https://recovery.localhost:8443".into(),
    };
    let req = AllocateWebSurfaceRequest {
        origin: recovery_origin,
        node_kind: NodeKind::SurfaceEmbed,
        claimed_by: "recovery-shell".into(),
        expected_group_id: None,
    };
    let desc = renderer
        .allocate_surface(req)
        .await
        .expect("allocate recovery surface");
    assert_eq!(desc.claimed_by, "recovery-shell");

    // Exit recovery
    renderer.exit_recovery_mode().await.expect("exit recovery");
    assert_eq!(renderer.get_mode().await, WebRendererMode::Normal);

    // Enter degraded mode
    renderer
        .enter_degraded_mode("cert_expired".into())
        .await
        .expect("enter degraded");
    assert!(
        matches!(renderer.get_mode().await, WebRendererMode::Degraded(ref r) if r == "cert_expired")
    );

    // Release surface still works in degraded mode
    let release = renderer
        .release_surface(desc.id)
        .await
        .expect("release in degraded");
    assert!(matches!(release.final_mode, WebRendererMode::Degraded(_)));
}

// =========================================================================
// Phase 6 — Apps bridge parity (three-way renderer domain check)
// =========================================================================

/// 6. Three-way parity proves CLI + KDE + Web agree on AppPackage rendering.
#[test]
fn phase_6_three_way_parity_for_apps_domain() {
    let parity = assert_three_way_parity_for_apps_domain().expect("parity");
    assert!(!parity.entries.is_empty());
    let entry = &parity.entries[0];
    assert!(entry.parses_in_cli);
    assert!(entry.parses_in_kde);
    assert!(entry.parses_in_web);
}
