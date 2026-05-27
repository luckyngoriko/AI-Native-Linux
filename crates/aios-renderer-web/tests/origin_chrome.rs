#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::no_effect_underscore_binding,
    missing_docs
)]

use aios_renderer_web::{
    ChromeIntegrityMonitor, ChromeTreeFragment, IframeOriginBinding, IntegrityCheckOutcome,
    OriginVerifier, WebRendererError, WebSurfaceId,
};
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;

// ---------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------

fn make_binding(surface_id: &WebSurfaceId, group_id: &str) -> IframeOriginBinding {
    IframeOriginBinding {
        iframe_origin: "https://aios.localhost:8443".to_string(),
        surface_id: surface_id.clone(),
        bound_group_id: group_id.to_string(),
        scope_binding_evidence_id: "ev_scb_01ABC".to_string(),
    }
}

fn make_signed_fragment(signing_key: &SigningKey, root_hash: &str) -> ChromeTreeFragment {
    let sig = signing_key.sign(root_hash.as_bytes());
    ChromeTreeFragment {
        root_hash: root_hash.to_string(),
        signature: sig.to_bytes().to_vec(),
        signed_at: Utc::now(),
    }
}

// ---------------------------------------------------------------
// OriginVerifier tests
// ---------------------------------------------------------------

#[tokio::test]
async fn register_binding_matching_group_succeeds() {
    let verifier = OriginVerifier::new();
    let sid = WebSurfaceId::new();
    let binding = make_binding(&sid, "aios");
    let result = verifier.register_binding(binding).await;
    assert!(result.is_ok(), "expected Ok, got: {result:?}");
}

#[tokio::test]
async fn register_binding_mismatched_group_returns_origin_verification_failed() {
    let verifier = OriginVerifier::new();
    let sid = WebSurfaceId::new();
    let binding = make_binding(&sid, "other-group");
    let err = verifier.register_binding(binding).await.unwrap_err();
    assert!(
        matches!(err, WebRendererError::OriginVerificationFailed { .. }),
        "expected OriginVerificationFailed, got: {err:?}"
    );
}

#[tokio::test]
async fn register_binding_non_app_origin_scheme_returns_error() {
    let verifier = OriginVerifier::new();
    let sid = WebSurfaceId::new();
    let binding = IframeOriginBinding {
        iframe_origin: "https://recovery.localhost:8443".to_string(),
        surface_id: sid.clone(),
        bound_group_id: "any".to_string(),
        scope_binding_evidence_id: "ev_scb_01XYZ".to_string(),
    };
    let err = verifier.register_binding(binding).await.unwrap_err();
    assert!(
        matches!(err, WebRendererError::OriginVerificationFailed { .. }),
        "expected OriginVerificationFailed for non-AppOrigin, got: {err:?}"
    );
}

#[tokio::test]
async fn verify_composition_matching_origin_succeeds() {
    let verifier = OriginVerifier::new();
    let sid = WebSurfaceId::new();
    let binding = make_binding(&sid, "aios");
    verifier.register_binding(binding).await.unwrap();
    let result = verifier
        .verify_composition(&sid, "https://aios.localhost:8443")
        .await;
    assert!(result.is_ok(), "expected Ok, got: {result:?}");
}

#[tokio::test]
async fn verify_composition_mismatched_origin_returns_origin_verification_failed() {
    let verifier = OriginVerifier::new();
    let sid = WebSurfaceId::new();
    let binding = make_binding(&sid, "aios");
    verifier.register_binding(binding).await.unwrap();
    let err = verifier
        .verify_composition(&sid, "https://evil.localhost:8443")
        .await
        .unwrap_err();
    assert!(
        matches!(err, WebRendererError::OriginVerificationFailed { .. }),
        "expected OriginVerificationFailed, got: {err:?}"
    );
}

#[tokio::test]
async fn verify_composition_unknown_surface_returns_origin_verification_failed() {
    let verifier = OriginVerifier::new();
    let sid = WebSurfaceId::new();
    let err = verifier
        .verify_composition(&sid, "https://aios.localhost:8443")
        .await
        .unwrap_err();
    assert!(
        matches!(err, WebRendererError::OriginVerificationFailed { .. }),
        "expected OriginVerificationFailed, got: {err:?}"
    );
}

#[tokio::test]
async fn revoke_binding_then_verify_returns_error() {
    let verifier = OriginVerifier::new();
    let sid = WebSurfaceId::new();
    let binding = make_binding(&sid, "aios");
    verifier.register_binding(binding).await.unwrap();
    verifier.revoke_binding(&sid).await.unwrap();
    let err = verifier
        .verify_composition(&sid, "https://aios.localhost:8443")
        .await
        .unwrap_err();
    assert!(
        matches!(err, WebRendererError::OriginVerificationFailed { .. }),
        "expected OriginVerificationFailed after revoke, got: {err:?}"
    );
}

#[tokio::test]
async fn list_bindings_after_3_registers_returns_3() {
    let verifier = OriginVerifier::new();
    for _ in 0..3 {
        let sid = WebSurfaceId::new();
        let binding = make_binding(&sid, "aios");
        verifier.register_binding(binding).await.unwrap();
    }
    let bindings = verifier.list_bindings().await;
    assert_eq!(bindings.len(), 3);
}

// ---------------------------------------------------------------
// ChromeIntegrityMonitor tests
// ---------------------------------------------------------------

#[tokio::test]
async fn chrome_integrity_admit_valid_signed_fragment_succeeds() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = VerifyingKey::from(&signing_key);
    let monitor = ChromeIntegrityMonitor::new(verifying_key);
    let fragment = make_signed_fragment(&signing_key, "abc123");
    let result = monitor.admit_signed_fragment(fragment).await;
    assert!(result.is_ok(), "expected Ok, got: {result:?}");
}

#[tokio::test]
async fn chrome_integrity_admit_invalid_signature_returns_chrome_shadow_root_integrity_failed() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = VerifyingKey::from(&signing_key);
    let monitor = ChromeIntegrityMonitor::new(verifying_key);

    let other_key = SigningKey::generate(&mut OsRng);
    let sig = other_key.sign(b"abc123");
    let fragment = ChromeTreeFragment {
        root_hash: "abc123".to_string(),
        signature: sig.to_bytes().to_vec(),
        signed_at: Utc::now(),
    };
    let err = monitor.admit_signed_fragment(fragment).await.unwrap_err();
    assert!(
        matches!(
            err,
            WebRendererError::ChromeShadowRootIntegrityFailed { .. }
        ),
        "expected ChromeShadowRootIntegrityFailed, got: {err:?}"
    );
}

#[tokio::test]
async fn chrome_integrity_check_observed_hash_in_signed_registry_returns_ok() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = VerifyingKey::from(&signing_key);
    let monitor = ChromeIntegrityMonitor::new(verifying_key);
    let fragment = make_signed_fragment(&signing_key, "abc123");
    monitor.admit_signed_fragment(fragment).await.unwrap();
    let result = monitor.check_observed_hash("abc123").await;
    assert!(result.is_ok(), "expected Ok, got: {result:?}");
}

#[tokio::test]
async fn chrome_integrity_check_observed_hash_not_in_registry_returns_extension_interference_detected(
) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = VerifyingKey::from(&signing_key);
    let monitor = ChromeIntegrityMonitor::new(verifying_key);
    let fragment = make_signed_fragment(&signing_key, "abc123");
    monitor.admit_signed_fragment(fragment).await.unwrap();
    let err = monitor.check_observed_hash("xyz999").await.unwrap_err();
    assert!(
        matches!(err, WebRendererError::ExtensionInterferenceDetected(_)),
        "expected ExtensionInterferenceDetected, got: {err:?}"
    );
}

#[tokio::test]
async fn chrome_integrity_history_records_outcome() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = VerifyingKey::from(&signing_key);
    let monitor = ChromeIntegrityMonitor::new(verifying_key);
    let fragment = make_signed_fragment(&signing_key, "abc123");
    monitor.admit_signed_fragment(fragment).await.unwrap();

    let _ = monitor.check_observed_hash("abc123").await;
    let _ = monitor.check_observed_hash("xyz999").await;

    let history = monitor.history().await;
    assert_eq!(history.len(), 2);
    assert!(matches!(history[0].outcome, IntegrityCheckOutcome::Ok));
    assert!(matches!(
        history[1].outcome,
        IntegrityCheckOutcome::ExtensionInterferenceDetected { .. }
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_admit_3_fragments_no_panic() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = VerifyingKey::from(&signing_key);
    let monitor = std::sync::Arc::new(ChromeIntegrityMonitor::new(verifying_key));

    let mut handles = Vec::new();
    for i in 0..3 {
        let m = monitor.clone();
        let sk = SigningKey::generate(&mut OsRng);
        let hash = format!("hash-{i}");
        let sig = sk.sign(hash.as_bytes());
        handles.push(tokio::spawn(async move {
            let fragment = ChromeTreeFragment {
                root_hash: hash,
                signature: sig.to_bytes().to_vec(),
                signed_at: Utc::now(),
            };
            // Each fragment has a different signing key, so they may fail verification.
            // The key point is no panic.
            let _ = m.admit_signed_fragment(fragment).await;
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_verify_3_compositions_no_panic() {
    let verifier = std::sync::Arc::new(OriginVerifier::new());
    let sid = WebSurfaceId::new();
    let binding = make_binding(&sid, "aios");
    verifier.register_binding(binding).await.unwrap();

    let mut handles = Vec::new();
    for _ in 0..3 {
        let v = verifier.clone();
        let s = sid.clone();
        handles.push(tokio::spawn(async move {
            let _ = v
                .verify_composition(&s, "https://aios.localhost:8443")
                .await;
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
}
