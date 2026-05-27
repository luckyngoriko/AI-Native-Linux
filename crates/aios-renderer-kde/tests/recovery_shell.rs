//! T-133 integration tests — 15+ tests covering [`RecoveryShellGuard`] INV I5,
//! [`ConstitutionalIconBundle`] INV I6, and [`escalate_to_degraded`] INV I7
//! enforcement per S7.4.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::needless_raw_string_hashes,
    reason = "test code; panic-on-failure is idiomatic"
)]

use std::collections::{BTreeMap, HashMap};

use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;

use aios_renderer_kde::{
    escalate_to_degraded, ConstitutionalIconBundle, DegradedTrigger, IconEntry, KdeRendererError,
    NodeKind, RecoverySession, RecoveryShellGuard, RendererMode,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Mint a fresh Ed25519 keypair via the OS CSPRNG.
fn fresh_keypair() -> (SigningKey, VerifyingKey) {
    let sk = SigningKey::generate(&mut OsRng);
    let vk = sk.verifying_key();
    (sk, vk)
}

/// Build a valid 64-character hex blake3 hash string.
fn dummy_blake3() -> String {
    "a".repeat(64)
}

/// Create a new recovery session with test data.
fn test_recovery_session() -> RecoverySession {
    RecoverySession {
        wayland_display: "wayland-2".into(),
        kwin_pid: 9999,
        aios_user: "aios-recovery".into(),
        started_at: Utc::now(),
    }
}

/// Build a signed [`ConstitutionalIconBundle`] with the given entries and
/// signing key.
fn signed_bundle(
    theme_id: &str,
    entries: BTreeMap<String, IconEntry>,
    fingerprint: &str,
    sk: &SigningKey,
) -> ConstitutionalIconBundle {
    let mut message = Vec::new();
    for (token_id, entry) in &entries {
        message.extend_from_slice(token_id.as_bytes());
        message.extend_from_slice(entry.relative_path.as_bytes());
        message.extend_from_slice(entry.blake3_hash.as_bytes());
    }
    let sig = sk.sign(&message);

    let mut trusted = HashMap::new();
    trusted.insert(fingerprint.to_string(), sk.verifying_key());

    ConstitutionalIconBundle {
        theme_id: theme_id.to_string(),
        root_path: "/aios/icons/recovery".into(),
        manifest: entries,
        bundle_signature: sig.to_bytes().to_vec(),
        signer_fingerprint: fingerprint.to_string(),
        trusted_authorities: trusted,
        emitter: None,
    }
}

/// Build a single icon entry.
fn icon_entry(token_id: &str, relative_path: &str) -> IconEntry {
    IconEntry {
        token_id: token_id.to_string(),
        relative_path: relative_path.to_string(),
        blake3_hash: dummy_blake3(),
    }
}

/// Build a 2-entry manifest.
fn two_entry_manifest() -> BTreeMap<String, IconEntry> {
    let mut m = BTreeMap::new();
    m.insert("icon-a".into(), icon_entry("icon-a", "actions/icon-a.svg"));
    m.insert("icon-b".into(), icon_entry("icon-b", "actions/icon-b.svg"));
    m
}

// ── Recovery session tests (INV I5) ──────────────────────────────────────────

#[test]
fn recovery_session_new_carries_wayland_display() {
    let session = test_recovery_session();
    assert_eq!(session.wayland_display, "wayland-2");
    assert_eq!(session.kwin_pid, 9999);
    assert_eq!(session.aios_user, "aios-recovery");
}

#[test]
fn recovery_shell_guard_admits_aios_surface_kind_only() {
    let guard = RecoveryShellGuard::new(test_recovery_session());
    // SurfaceEmbed is the only allowed kind.
    guard
        .admit(NodeKind::SurfaceEmbed)
        .expect("SurfaceEmbed must be admitted");
}

#[test]
fn recovery_shell_guard_rejects_text_kind_with_internal_error() {
    let guard = RecoveryShellGuard::new(test_recovery_session());
    let err = guard
        .admit(NodeKind::Text)
        .expect_err("Text must be rejected");
    match err {
        KdeRendererError::Internal(msg) => {
            assert!(msg.contains("AIOS_SURFACE"), "msg = {msg}");
            assert!(msg.contains("INV I5"), "msg = {msg}");
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

#[test]
fn recovery_shell_guard_rejects_visualization_kind() {
    let guard = RecoveryShellGuard::new(test_recovery_session());
    let err = guard
        .admit(NodeKind::Visualization)
        .expect_err("Visualization must be rejected");
    match err {
        KdeRendererError::Internal(msg) => {
            assert!(msg.contains("AIOS_SURFACE only"), "msg = {msg}");
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

#[test]
fn recovery_shell_guard_rejects_security_indicator_in_recovery() {
    // SecurityIndicator is AIOS-owned but NOT SurfaceEmbed — recovery shell
    // only admits SurfaceEmbed.
    let guard = RecoveryShellGuard::new(test_recovery_session());
    let err = guard
        .admit(NodeKind::SecurityIndicator)
        .expect_err("SecurityIndicator must be rejected by recovery shell");
    assert!(matches!(err, KdeRendererError::Internal(_)));
}

#[test]
fn recovery_shell_guard_session_isolation_marker_has_separate_user_true() {
    let guard = RecoveryShellGuard::new(test_recovery_session());
    let marker = guard.session_isolation_marker();
    assert_eq!(marker.wayland_display, "wayland-2");
    assert_eq!(marker.kwin_pid, 9999);
    assert!(marker.separate_user, "separate_user must be true");
}

// ── Constitutional icon bundle tests (INV I6) ────────────────────────────────

#[test]
fn constitutional_icon_bundle_verify_with_valid_signature_succeeds() {
    let (sk, _vk) = fresh_keypair();
    let bundle = signed_bundle("aios-recovery", two_entry_manifest(), "auth1", &sk);
    bundle.verify().expect("valid bundle must verify");
}

#[test]
fn constitutional_icon_bundle_verify_unknown_authority_returns_icon_bundle_verification_failed() {
    let (sk, _vk) = fresh_keypair();
    let mut bundle = signed_bundle("aios-recovery", two_entry_manifest(), "auth1", &sk);
    // Replace trusted_authorities with an empty map so the signer is unknown.
    bundle.trusted_authorities = HashMap::new();
    bundle.signer_fingerprint = "unknown".into();

    let err = bundle.verify().expect_err("unknown authority must fail");
    match err {
        KdeRendererError::IconBundleVerificationFailed { theme_id, reason } => {
            assert_eq!(theme_id, "aios-recovery");
            assert!(reason.contains("unknown authority"), "reason = {reason}");
        }
        other => panic!("expected IconBundleVerificationFailed, got {other:?}"),
    }
}

#[test]
fn constitutional_icon_bundle_verify_invalid_signature_returns_failed() {
    let (sk, _vk) = fresh_keypair();
    let mut bundle = signed_bundle("aios-recovery", two_entry_manifest(), "auth1", &sk);
    // Tamper with the signature.
    if let Some(b) = bundle.bundle_signature.first_mut() {
        *b ^= 0x01;
    }

    let err = bundle.verify().expect_err("tampered signature must fail");
    match err {
        KdeRendererError::IconBundleVerificationFailed { theme_id, reason } => {
            assert_eq!(theme_id, "aios-recovery");
            assert!(
                reason.contains("invalid ed25519 signature"),
                "reason = {reason}"
            );
        }
        other => panic!("expected IconBundleVerificationFailed, got {other:?}"),
    }
}

#[test]
fn constitutional_icon_bundle_verify_blake3_empty_rejected() {
    let (sk, _vk) = fresh_keypair();
    let mut bad_entry = icon_entry("bad", "bad.svg");
    bad_entry.blake3_hash = String::new();
    let mut manifest: BTreeMap<String, IconEntry> = BTreeMap::new();
    manifest.insert("bad".into(), bad_entry);

    // Create and re-sign because the message changed.
    let mut message = Vec::new();
    for (token_id, entry) in &manifest {
        message.extend_from_slice(token_id.as_bytes());
        message.extend_from_slice(entry.relative_path.as_bytes());
        message.extend_from_slice(entry.blake3_hash.as_bytes());
    }
    let sig = sk.sign(&message);
    let mut trusted = HashMap::new();
    trusted.insert("auth1".to_string(), sk.verifying_key());

    let bundle = ConstitutionalIconBundle {
        theme_id: "bad-theme".into(),
        root_path: "/tmp".into(),
        manifest,
        bundle_signature: sig.to_bytes().to_vec(),
        signer_fingerprint: "auth1".into(),
        trusted_authorities: trusted,
        emitter: None,
    };

    let err = bundle.verify().expect_err("empty blake3 must fail");
    match err {
        KdeRendererError::IconBundleVerificationFailed { reason, .. } => {
            assert!(reason.contains("blake3 mismatch"), "reason = {reason}");
        }
        other => panic!("expected IconBundleVerificationFailed, got {other:?}"),
    }
}

#[test]
fn constitutional_icon_bundle_verify_bad_signature_length_rejected() {
    let (sk, _vk) = fresh_keypair();
    let bundle = signed_bundle("aios-recovery", two_entry_manifest(), "auth1", &sk);
    let mut bad_bundle = bundle;
    // Truncate signature to invalid length.
    bad_bundle.bundle_signature = vec![0xAA; 16];

    let err = bad_bundle
        .verify()
        .expect_err("bad signature length must fail");
    assert!(matches!(
        err,
        KdeRendererError::IconBundleVerificationFailed { .. }
    ));
}

#[test]
fn constitutional_icon_bundle_lookup_known_token_returns_entry() {
    let (sk, _vk) = fresh_keypair();
    let bundle = signed_bundle("theme", two_entry_manifest(), "auth1", &sk);
    let entry = bundle.lookup("icon-a").expect("icon-a must be found");
    assert_eq!(entry.token_id, "icon-a");
    assert_eq!(entry.relative_path, "actions/icon-a.svg");
}

#[test]
fn constitutional_icon_bundle_lookup_unknown_token_returns_none() {
    let (sk, _vk) = fresh_keypair();
    let bundle = signed_bundle("theme", two_entry_manifest(), "auth1", &sk);
    assert!(bundle.lookup("nonexistent").is_none());
}

#[test]
fn constitutional_icon_bundle_verify_different_signer_key_fails() {
    let (sk, _vk) = fresh_keypair();
    let bundle = signed_bundle("aios-recovery", two_entry_manifest(), "auth1", &sk);

    // Register a different key under "auth1".
    let (_sk2, vk2) = fresh_keypair();
    let mut bad_bundle = bundle;
    bad_bundle.trusted_authorities = {
        let mut m = HashMap::new();
        m.insert("auth1".to_string(), vk2);
        m
    };

    let err = bad_bundle.verify().expect_err("wrong key must fail");
    assert!(matches!(
        err,
        KdeRendererError::IconBundleVerificationFailed { .. }
    ));
}

// ── Degraded mode escalation tests (INV I7) ──────────────────────────────────

#[test]
fn escalate_to_degraded_kwin_unavailable_returns_degraded_mode() {
    let (mode, reason) = escalate_to_degraded(DegradedTrigger::KwinUnavailable);
    assert_eq!(reason, "kwin_unreachable");
    assert_eq!(mode, RendererMode::Degraded("kwin_unreachable".into()));
}

#[test]
fn escalate_to_degraded_wayland_failed_returns_degraded_mode() {
    let (mode, reason) = escalate_to_degraded(DegradedTrigger::WaylandConnectFailed);
    assert_eq!(reason, "wayland_connect_failed");
    assert_eq!(
        mode,
        RendererMode::Degraded("wayland_connect_failed".into())
    );
}

#[test]
fn escalate_to_degraded_gpu_failed_returns_degraded_mode() {
    let (mode, reason) = escalate_to_degraded(DegradedTrigger::GpuDeviceAcquisitionFailed);
    assert_eq!(reason, "gpu_acquisition_failed");
    assert_eq!(
        mode,
        RendererMode::Degraded("gpu_acquisition_failed".into())
    );
}

#[test]
fn escalate_to_degraded_icon_bundle_failed_returns_degraded_mode() {
    let (mode, reason) = escalate_to_degraded(DegradedTrigger::IconBundleVerificationFailed);
    assert_eq!(reason, "icon_bundle_verification_failed");
    assert_eq!(
        mode,
        RendererMode::Degraded("icon_bundle_verification_failed".into())
    );
}

#[test]
fn escalate_to_degraded_kwin_script_returns_degraded_mode() {
    let (mode, reason) = escalate_to_degraded(DegradedTrigger::KwinScriptVerificationFailed);
    assert_eq!(reason, "kwin_script_verification_failed");
    assert_eq!(
        mode,
        RendererMode::Degraded("kwin_script_verification_failed".into())
    );
}

#[test]
fn degraded_reason_strings_are_non_empty_for_every_trigger() {
    for &trigger in DegradedTrigger::ALL {
        let (_mode, reason) = escalate_to_degraded(trigger);
        assert!(!reason.is_empty(), "reason empty for {trigger:?}");
    }
}

#[test]
fn recovery_shell_guard_rejects_all_19_kinds_except_surface_embed() {
    let guard = RecoveryShellGuard::new(test_recovery_session());
    for &kind in NodeKind::ALL {
        let result = guard.admit(kind);
        if kind == NodeKind::SurfaceEmbed {
            assert!(result.is_ok(), "SurfaceEmbed must be admitted");
        } else {
            assert!(
                result.is_err(),
                "{kind:?} must be rejected by recovery shell"
            );
        }
    }
}

#[test]
fn session_isolation_marker_fields_match_session() {
    let session = RecoverySession {
        wayland_display: "wayland-5".into(),
        kwin_pid: 4242,
        aios_user: "aios-ops".into(),
        started_at: Utc::now(),
    };
    let guard = RecoveryShellGuard::new(session);
    let marker = guard.session_isolation_marker();
    assert_eq!(marker.wayland_display, "wayland-5");
    assert_eq!(marker.kwin_pid, 4242);
    assert!(marker.separate_user);
}
