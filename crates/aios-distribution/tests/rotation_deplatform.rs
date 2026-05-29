#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::doc_markdown,
    clippy::significant_drop_tightening,
    clippy::wildcard_imports,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::needless_collect,
    clippy::format_collect,
    clippy::too_many_arguments,
    clippy::float_cmp,
    clippy::redundant_clone,
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;

use aios_distribution::*;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Generates a fresh Ed25519 keypair for test use.
fn make_keypair() -> (SigningKey, VerifyingKey) {
    let mut csprng = OsRng;
    let signing = SigningKey::generate(&mut csprng);
    let verifying = signing.verifying_key();
    (signing, verifying)
}

/// Builds a minimal test fixture: a PublisherCatalog with one publisher, a
/// SigningKeyCatalog with one signing key, and the AIOS root key.
struct TestFixture {
    aios_root_key: AiosRootKey,
    aios_root_signing: SigningKey,
    publisher_signing: SigningKey,
    publisher_root_id: PublisherRootId,
    publisher_catalog: PublisherCatalog,
    signing_catalog: SigningKeyCatalog,
    signing_key_id: PackageSigningKeyId,
}

impl TestFixture {
    fn new() -> Self {
        let (aios_signing, aios_vk) = make_keypair();
        let (pub_signing, pub_vk) = make_keypair();
        let (_sk_signing, sk_vk) = make_keypair();
        let now = Utc::now();
        let publisher_root_id = PublisherRootId("pub:testvendor".into());

        let publisher_root = PublisherRoot {
            publisher_root_id: publisher_root_id.clone(),
            public_key: pub_vk,
            trust_level: PublisherTrustLevel::Verified,
            onboarding_evidence_pointer: Some("test://onboarding/1".into()),
            activated_at: now,
            retired_at: None,
        };

        let signing_key_id = PackageSigningKeyId("pks:testvendor:release".into());
        let signing_key = PackageSigningKey {
            package_signing_key_id: signing_key_id.clone(),
            public_key: sk_vk,
            valid_from: now,
            valid_until: None,
            revoked_at: None,
        };

        Self {
            aios_root_key: AiosRootKey::new(aios_vk),
            aios_root_signing: aios_signing,
            publisher_signing: pub_signing,
            publisher_root_id,
            publisher_catalog: PublisherCatalog::new(vec![publisher_root]),
            signing_catalog: SigningKeyCatalog::new("testvendor".into(), vec![signing_key]),
            signing_key_id,
        }
    }

    /// Signs a rotation event with the old publisher root and the AIOS root.
    fn sign_rotation(&self, event: &mut PublisherRotationEvent) {
        let canonical = rotation_canonical_raw(event);
        // Old publisher root signs (chain continuity)
        let old_sig = self.publisher_signing.sign(&canonical);
        event.old_root_signature_over_new = old_sig.to_bytes().to_vec();
        // AIOS root co-signs
        let aios_sig = self.aios_root_signing.sign(&canonical);
        event.aios_root_signature = aios_sig.to_bytes().to_vec();
    }

    /// Signs a deplatform event with the AIOS root.
    fn sign_deplatform(&self, event: &mut PublisherDeplatformEvent) {
        let canonical = deplatform_canonical_raw(event);
        let aios_sig = self.aios_root_signing.sign(&canonical);
        event.aios_root_signature = aios_sig.to_bytes().to_vec();
    }
}

/// Replicates `rotation::rotation_canonical_bytes` for test signers.
fn rotation_canonical_raw(event: &PublisherRotationEvent) -> Vec<u8> {
    let mut buf = Vec::with_capacity(128);
    buf.extend_from_slice(event.publisher_root_id.0.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(&event.new_public_key);
    buf.push(b'\n');
    buf.extend_from_slice(event.rotated_at.to_rfc3339().as_bytes());
    buf
}

/// Replicates `deplatform::deplatform_canonical_bytes` for test signers.
fn deplatform_canonical_raw(event: &PublisherDeplatformEvent) -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);
    buf.extend_from_slice(event.publisher_root_id.0.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(event.reason.label().as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(event.deplatformed_at.to_rfc3339().as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(event.grace_period_ends_at.to_rfc3339().as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(event.evidence_pointer.as_bytes());
    buf
}

// ============================================================================
// 1 — valid rotation (old root signs new, AIOS root cosigns) → catalog
//     publisher public_key updated
// ============================================================================

#[test]
fn valid_rotation_updates_publisher_public_key() {
    let fix = TestFixture::new();
    let (_new_sk, new_vk) = make_keypair();
    let now = Utc::now();

    let mut event = PublisherRotationEvent {
        publisher_root_id: fix.publisher_root_id.clone(),
        old_public_key: fix.publisher_signing.verifying_key().to_bytes().to_vec(),
        new_public_key: new_vk.to_bytes().to_vec(),
        old_root_signature_over_new: vec![],
        aios_root_signature: vec![],
        rotated_at: now,
        reason: TakedownReason::PublisherRequest,
        compromise_window_start: None,
    };

    fix.sign_rotation(&mut event);

    // Verify separately
    let publisher = fix
        .publisher_catalog
        .lookup(&fix.publisher_root_id)
        .unwrap();
    let result = verify_rotation_event(&event, publisher, &fix.aios_root_key);
    assert!(result.is_ok(), "verify_rotation_event should succeed");

    // Apply and check catalog update
    let mut cat = fix.publisher_catalog.clone();
    let mut skcat = fix.signing_catalog.clone();
    let outcome =
        apply_publisher_rotation(&mut cat, &mut skcat, &event, &fix.aios_root_key, now).unwrap();

    assert!(
        !outcome.reactive,
        "non-KeyCompromise rotation should not be reactive"
    );
    assert!(
        outcome.revoked_signing_key_ids.is_empty(),
        "non-KeyCompromise rotation should not revoke keys"
    );

    let updated = cat.lookup(&fix.publisher_root_id).unwrap();
    assert_eq!(
        updated.public_key.as_bytes(),
        new_vk.as_bytes(),
        "publisher public_key should be updated to new key"
    );
}

// ============================================================================
// 2 — bad old_root_signature_over_new → error (continuity broken)
// ============================================================================

#[test]
fn bad_old_root_signature_errors() {
    let fix = TestFixture::new();
    let (_new_sk, new_vk) = make_keypair();
    let now = Utc::now();

    let mut event = PublisherRotationEvent {
        publisher_root_id: fix.publisher_root_id.clone(),
        old_public_key: fix.publisher_signing.verifying_key().to_bytes().to_vec(),
        new_public_key: new_vk.to_bytes().to_vec(),
        old_root_signature_over_new: vec![],
        aios_root_signature: vec![],
        rotated_at: now,
        reason: TakedownReason::PublisherRequest,
        compromise_window_start: None,
    };

    // Sign with correct AIOS root, but the old-root sig is wrong bytes
    let canonical = rotation_canonical_raw(&event);
    let aios_sig = fix.aios_root_signing.sign(&canonical);
    event.aios_root_signature = aios_sig.to_bytes().to_vec();
    // Tampered old-root signature
    event.old_root_signature_over_new = vec![0xAAu8; 64];

    let publisher = fix
        .publisher_catalog
        .lookup(&fix.publisher_root_id)
        .unwrap();
    let result = verify_rotation_event(&event, publisher, &fix.aios_root_key);
    assert!(result.is_err(), "bad old_root_signature should error");
    let err = result.unwrap_err();
    assert_eq!(
        err.code(),
        DistributionErrorCode::SignatureFailed,
        "should be SignatureFailed"
    );
}

// ============================================================================
// 3 — bad aios_root_signature → error (unauthorised break)
// ============================================================================

#[test]
fn bad_aios_root_signature_errors() {
    let fix = TestFixture::new();
    let (_new_sk, new_vk) = make_keypair();
    let now = Utc::now();

    let mut event = PublisherRotationEvent {
        publisher_root_id: fix.publisher_root_id.clone(),
        old_public_key: fix.publisher_signing.verifying_key().to_bytes().to_vec(),
        new_public_key: new_vk.to_bytes().to_vec(),
        old_root_signature_over_new: vec![],
        aios_root_signature: vec![],
        rotated_at: now,
        reason: TakedownReason::PublisherRequest,
        compromise_window_start: None,
    };

    // Correct old-root sig, tampered AIOS sig
    let canonical = rotation_canonical_raw(&event);
    let old_sig = fix.publisher_signing.sign(&canonical);
    event.old_root_signature_over_new = old_sig.to_bytes().to_vec();
    event.aios_root_signature = vec![0xBBu8; 64];

    let publisher = fix
        .publisher_catalog
        .lookup(&fix.publisher_root_id)
        .unwrap();
    let result = verify_rotation_event(&event, publisher, &fix.aios_root_key);
    assert!(result.is_err(), "bad aios_root_signature should error");
    let err = result.unwrap_err();
    assert_eq!(
        err.code(),
        DistributionErrorCode::SignatureFailed,
        "should be SignatureFailed"
    );
}

// ============================================================================
// 4 — old signing keys still verify after a non-reactive rotation
// ============================================================================

#[test]
fn old_signing_keys_still_active_after_non_reactive_rotation() {
    let fix = TestFixture::new();
    let (_new_sk, new_vk) = make_keypair();
    let now = Utc::now();

    let mut event = PublisherRotationEvent {
        publisher_root_id: fix.publisher_root_id.clone(),
        old_public_key: fix.publisher_signing.verifying_key().to_bytes().to_vec(),
        new_public_key: new_vk.to_bytes().to_vec(),
        old_root_signature_over_new: vec![],
        aios_root_signature: vec![],
        rotated_at: now,
        reason: TakedownReason::PublisherRequest,
        compromise_window_start: None,
    };

    fix.sign_rotation(&mut event);

    let mut cat = fix.publisher_catalog.clone();
    let mut skcat = fix.signing_catalog.clone();
    let outcome =
        apply_publisher_rotation(&mut cat, &mut skcat, &event, &fix.aios_root_key, now).unwrap();

    assert!(!outcome.reactive);
    // The old signing key should NOT be revoked
    let sk = skcat.lookup(&fix.signing_key_id).unwrap();
    assert!(
        sk.revoked_at.is_none(),
        "non-reactive rotation should not revoke old signing keys"
    );
}

// ============================================================================
// 5 — reactive KeyCompromise rotation revokes signing keys
// ============================================================================

#[test]
fn reactive_key_compromise_revokes_signing_keys() {
    let fix = TestFixture::new();
    let (_new_sk, new_vk) = make_keypair();
    let now = Utc::now();

    let mut event = PublisherRotationEvent {
        publisher_root_id: fix.publisher_root_id.clone(),
        old_public_key: fix.publisher_signing.verifying_key().to_bytes().to_vec(),
        new_public_key: new_vk.to_bytes().to_vec(),
        old_root_signature_over_new: vec![],
        aios_root_signature: vec![],
        rotated_at: now,
        reason: TakedownReason::KeyCompromise,
        compromise_window_start: None,
    };

    fix.sign_rotation(&mut event);

    let mut cat = fix.publisher_catalog.clone();
    let mut skcat = fix.signing_catalog.clone();
    let outcome =
        apply_publisher_rotation(&mut cat, &mut skcat, &event, &fix.aios_root_key, now).unwrap();

    assert!(outcome.reactive, "KeyCompromise rotation must be reactive");
    assert!(
        !outcome.revoked_signing_key_ids.is_empty(),
        "KeyCompromise rotation must revoke signing keys"
    );
    assert!(
        outcome
            .revoked_signing_key_ids
            .contains(&fix.signing_key_id),
        "the existing signing key should be in revoked list"
    );

    let sk = skcat.lookup(&fix.signing_key_id).unwrap();
    assert!(
        sk.revoked_at.is_some(),
        "signing key should be revoked after KeyCompromise rotation"
    );
}

// ============================================================================
// 6 — verify_rotation_event with mismatched old key errors
// ============================================================================

#[test]
fn verify_rotation_event_mismatched_old_key_errors() {
    let fix = TestFixture::new();
    let (_new_sk, new_vk) = make_keypair();
    let (other_sk, other_vk) = make_keypair();
    let now = Utc::now();

    // Build an event with the wrong old_public_key (different from catalog)
    let mut event = PublisherRotationEvent {
        publisher_root_id: fix.publisher_root_id.clone(),
        old_public_key: other_vk.to_bytes().to_vec(), // WRONG — not the catalog key
        new_public_key: new_vk.to_bytes().to_vec(),
        old_root_signature_over_new: vec![],
        aios_root_signature: vec![],
        rotated_at: now,
        reason: TakedownReason::PublisherRequest,
        compromise_window_start: None,
    };

    // Sign the canonical bytes with the OTHER key (to make that sig verify
    // against other_vk), but use the correct AIOS root
    let canonical = rotation_canonical_raw(&event);
    let old_sig = other_sk.sign(&canonical);
    event.old_root_signature_over_new = old_sig.to_bytes().to_vec();
    let aios_sig = fix.aios_root_signing.sign(&canonical);
    event.aios_root_signature = aios_sig.to_bytes().to_vec();

    let publisher = fix
        .publisher_catalog
        .lookup(&fix.publisher_root_id)
        .unwrap();
    let result = verify_rotation_event(&event, publisher, &fix.aios_root_key);
    assert!(
        result.is_err(),
        "old_public_key mismatch with catalog should error"
    );
}

// ============================================================================
// 7 — AiosRootRotationEvent::requires_recovery() is true
// ============================================================================

#[test]
fn aios_root_rotation_requires_recovery() {
    assert!(
        AiosRootRotationEvent::requires_recovery(),
        "AIOS root rotation MUST require recovery boot per §4.1"
    );
}

// ============================================================================
// 8 — AiosRootRotationEvent::requires_dual_approval() is true
// ============================================================================

#[test]
fn aios_root_rotation_requires_dual_approval() {
    assert!(
        AiosRootRotationEvent::requires_dual_approval(),
        "AIOS root rotation MUST require dual human approval per §4.1"
    );
}

// ============================================================================
// 9 — valid deplatform → publisher trust_level == Deplatformed, retired_at set
// ============================================================================

#[test]
fn valid_deplatform_sets_trust_level_and_retired_at() {
    let fix = TestFixture::new();
    let now = Utc::now();
    let grace_end = default_grace_end(now);

    let mut event = PublisherDeplatformEvent {
        publisher_root_id: fix.publisher_root_id.clone(),
        reason: TakedownReason::MaliciousBehaviorDetected,
        deplatformed_at: now,
        grace_period_ends_at: grace_end,
        evidence_pointer: "test://evidence/deplatform/1".into(),
        aios_root_signature: vec![],
        extended: false,
    };

    fix.sign_deplatform(&mut event);

    // Verify
    let ver_result = verify_deplatform_event(&event, &fix.aios_root_key);
    assert!(ver_result.is_ok(), "valid deplatform should verify");

    // Apply
    let mut cat = fix.publisher_catalog.clone();
    let result = apply_deplatform(&mut cat, &event, &fix.aios_root_key, now);
    assert!(result.is_ok());

    let publisher = cat.lookup(&fix.publisher_root_id).unwrap();
    assert_eq!(
        publisher.trust_level,
        PublisherTrustLevel::Deplatformed,
        "trust_level must be Deplatformed"
    );
    assert!(publisher.retired_at.is_some(), "retired_at must be set");
    assert_eq!(
        publisher.retired_at.unwrap(),
        now,
        "retired_at must equal deplatformed_at"
    );
}

// ============================================================================
// 10 — bad aios_root_signature on deplatform → error
// ============================================================================

#[test]
fn bad_aios_root_signature_on_deplatform_errors() {
    let fix = TestFixture::new();
    let now = Utc::now();
    let grace_end = default_grace_end(now);

    let event = PublisherDeplatformEvent {
        publisher_root_id: fix.publisher_root_id.clone(),
        reason: TakedownReason::MaliciousBehaviorDetected,
        deplatformed_at: now,
        grace_period_ends_at: grace_end,
        evidence_pointer: "test://evidence/deplatform/2".into(),
        aios_root_signature: vec![0xCCu8; 64], // tampered
        extended: false,
    };

    let ver_result = verify_deplatform_event(&event, &fix.aios_root_key);
    assert!(ver_result.is_err(), "bad AIOS root sig should be rejected");

    let mut cat = fix.publisher_catalog.clone();
    let result = apply_deplatform(&mut cat, &event, &fix.aios_root_key, now);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), DistributionErrorCode::SignatureFailed);
}

// ============================================================================
// 11 — default_grace_end == deplatformed_at + 30 days
// ============================================================================

#[test]
fn default_grace_end_is_30_days_after_deplatform() {
    let now = Utc::now();
    let grace_end = default_grace_end(now);
    let diff = grace_end - now;
    assert!(
        diff.num_days() >= 29 && diff.num_days() <= 31,
        "grace end must be ~30 days after deplatform; got {} days",
        diff.num_days()
    );
}

// ============================================================================
// 12 — health_check_quarantine: Active package from deplatformed
//      publisher → Quarantined + returned in list
// ============================================================================

#[test]
fn health_check_quarantines_deplatformed_publisher_packages() {
    let fix = TestFixture::new();
    let now = Utc::now();
    let grace_end = default_grace_end(now);

    // First deplatform the publisher
    let mut event = PublisherDeplatformEvent {
        publisher_root_id: fix.publisher_root_id.clone(),
        reason: TakedownReason::MaliciousBehaviorDetected,
        deplatformed_at: now,
        grace_period_ends_at: grace_end,
        evidence_pointer: "test://evidence/deplatform/3".into(),
        aios_root_signature: vec![],
        extended: false,
    };
    fix.sign_deplatform(&mut event);

    let mut cat = fix.publisher_catalog.clone();
    apply_deplatform(&mut cat, &event, &fix.aios_root_key, now).unwrap();

    // Create an installed package from this publisher
    let mut installed = vec![InstalledPackageRecord {
        package_id: PackageId("pkg:testvendor:myapp".into()),
        publisher_root_id: fix.publisher_root_id.clone(),
        signing_key_id: fix.signing_key_id.clone(),
        state: PackageInstallState::Active,
        installed_at: now,
    }];

    let revoked: Vec<PackageSigningKeyId> = vec![];
    let quarantined = health_check_quarantine(&mut installed, &cat, &revoked, now);

    assert_eq!(quarantined.len(), 1);
    assert_eq!(
        installed[0].state,
        PackageInstallState::Quarantined,
        "Active package from deplatformed publisher must be Quarantined"
    );
    assert_eq!(quarantined[0].0, "pkg:testvendor:myapp");
}

// ============================================================================
// 13 — health_check_quarantine: Active package from healthy publisher
//      → stays Active
// ============================================================================

#[test]
fn health_check_does_not_quarantine_healthy_publisher_packages() {
    let fix = TestFixture::new();
    let now = Utc::now();

    let mut installed = vec![InstalledPackageRecord {
        package_id: PackageId("pkg:testvendor:myapp".into()),
        publisher_root_id: fix.publisher_root_id.clone(),
        signing_key_id: fix.signing_key_id.clone(),
        state: PackageInstallState::Active,
        installed_at: now,
    }];

    let revoked: Vec<PackageSigningKeyId> = vec![];
    let quarantined =
        health_check_quarantine(&mut installed, &fix.publisher_catalog, &revoked, now);

    assert!(quarantined.is_empty());
    assert_eq!(
        installed[0].state,
        PackageInstallState::Active,
        "Active package from healthy publisher must stay Active"
    );
}

// ============================================================================
// 14 — health_check_quarantine: Active package whose signing key was
//      revoked (reactive rotation) → Quarantined
// ============================================================================

#[test]
fn health_check_quarantines_revoked_signing_key_packages() {
    let fix = TestFixture::new();
    let now = Utc::now();

    let revoked = vec![fix.signing_key_id.clone()];

    let mut installed = vec![InstalledPackageRecord {
        package_id: PackageId("pkg:testvendor:myapp".into()),
        publisher_root_id: fix.publisher_root_id.clone(),
        signing_key_id: fix.signing_key_id.clone(),
        state: PackageInstallState::Active,
        installed_at: now,
    }];

    let quarantined =
        health_check_quarantine(&mut installed, &fix.publisher_catalog, &revoked, now);

    assert_eq!(quarantined.len(), 1);
    assert_eq!(
        installed[0].state,
        PackageInstallState::Quarantined,
        "Active package with revoked signing key must be Quarantined"
    );
}

// ============================================================================
// 15 — grace_expired false before, true after grace end
// ============================================================================

#[test]
fn grace_expired_transitions_at_boundary() {
    let fix = TestFixture::new();
    let now = Utc::now();
    let grace_end = default_grace_end(now);

    let mut event = PublisherDeplatformEvent {
        publisher_root_id: fix.publisher_root_id.clone(),
        reason: TakedownReason::MaliciousBehaviorDetected,
        deplatformed_at: now,
        grace_period_ends_at: grace_end,
        evidence_pointer: "test://evidence/grace/1".into(),
        aios_root_signature: vec![],
        extended: false,
    };
    fix.sign_deplatform(&mut event);

    // Before grace end
    assert!(
        !grace_expired(&event, grace_end - Duration::try_seconds(1).unwrap()),
        "grace should not be expired just before boundary"
    );
    // At grace end (now >= grace_end)
    assert!(
        grace_expired(&event, grace_end),
        "grace should be expired at exact boundary"
    );
    // After grace end
    assert!(
        grace_expired(&event, grace_end + Duration::try_days(1).unwrap()),
        "grace should be expired after boundary"
    );
}

// ============================================================================
// 16 — extend_grace: first extension ≤ 30 days OK
// ============================================================================

#[test]
fn extend_grace_first_extension_up_to_30_days_succeeds() {
    let deplatformed_at = Utc::now();
    let grace_end = default_grace_end(deplatformed_at);

    let mut event = PublisherDeplatformEvent {
        publisher_root_id: PublisherRootId("pub:extendtest".into()),
        reason: TakedownReason::LegalRequirement,
        deplatformed_at,
        grace_period_ends_at: grace_end,
        evidence_pointer: "test://evidence/extend/1".into(),
        aios_root_signature: vec![0u8; 64],
        extended: false,
    };

    let original = event.grace_period_ends_at;
    let ext = Duration::try_days(15).unwrap();
    let result = extend_grace(&mut event, ext, Utc::now());

    assert!(result.is_ok());
    assert!(event.extended);
    assert_eq!(event.grace_period_ends_at, original + ext);
}

// ============================================================================
// 17 — extend_grace: second extension → error
// ============================================================================

#[test]
fn extend_grace_second_extension_errors() {
    let deplatformed_at = Utc::now();
    let grace_end = default_grace_end(deplatformed_at);

    let mut event = PublisherDeplatformEvent {
        publisher_root_id: PublisherRootId("pub:extendtest2".into()),
        reason: TakedownReason::LegalRequirement,
        deplatformed_at,
        grace_period_ends_at: grace_end,
        evidence_pointer: "test://evidence/extend/2".into(),
        aios_root_signature: vec![0u8; 64],
        extended: false,
    };

    // First extension succeeds
    let _ = extend_grace(&mut event, Duration::try_days(10).unwrap(), Utc::now());
    // Second extension fails
    let result = extend_grace(&mut event, Duration::try_days(10).unwrap(), Utc::now());
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), DistributionErrorCode::TakedownActive);
}

// ============================================================================
// 18 — extend_grace: extension > 30 days → error
// ============================================================================

#[test]
fn extend_grace_exceeds_30_days_errors() {
    let deplatformed_at = Utc::now();
    let grace_end = default_grace_end(deplatformed_at);

    let mut event = PublisherDeplatformEvent {
        publisher_root_id: PublisherRootId("pub:extendtest3".into()),
        reason: TakedownReason::LegalRequirement,
        deplatformed_at,
        grace_period_ends_at: grace_end,
        evidence_pointer: "test://evidence/extend/3".into(),
        aios_root_signature: vec![0u8; 64],
        extended: false,
    };

    let result = extend_grace(&mut event, Duration::try_days(31).unwrap(), Utc::now());
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), DistributionErrorCode::TakedownActive);
}

// ============================================================================
// 19 — returning_publisher_default_trust == Community
// ============================================================================

#[test]
fn returning_publisher_default_trust_is_community() {
    assert_eq!(
        returning_publisher_default_trust(),
        PublisherTrustLevel::Community
    );
}

// ============================================================================
// 20 — non-reversibility: no "undeplatform" fn exists for publisher-only use
//      (structural assertion — the only way back is a fresh Community publisher)
// ============================================================================

#[test]
fn deplatform_is_not_reversible_by_publisher_action() {
    let fix = TestFixture::new();
    let now = Utc::now();
    let grace_end = default_grace_end(now);

    // Deplatform the publisher
    let mut event = PublisherDeplatformEvent {
        publisher_root_id: fix.publisher_root_id.clone(),
        reason: TakedownReason::MaliciousBehaviorDetected,
        deplatformed_at: now,
        grace_period_ends_at: grace_end,
        evidence_pointer: "test://evidence/irreversible/1".into(),
        aios_root_signature: vec![],
        extended: false,
    };
    fix.sign_deplatform(&mut event);

    let mut cat = fix.publisher_catalog.clone();
    apply_deplatform(&mut cat, &event, &fix.aios_root_key, now).unwrap();

    // After deplatform, verify the publisher is Deplatformed
    let publisher = cat.lookup(&fix.publisher_root_id).unwrap();
    assert_eq!(publisher.trust_level, PublisherTrustLevel::Deplatformed);

    // The only way to "return" is to be treated as a fresh Community publisher
    // (structural assertion: there is no public "undeplatform" or
    // "restore_publisher" function in the crate API)
    assert_eq!(
        returning_publisher_default_trust(),
        PublisherTrustLevel::Community,
        "a returning publisher must re-onboard as Community"
    );

    // Verify the publisher cannot simply change its trust level back
    // (there is no API for this — the trust level is set via catalog operations
    // that the publisher cannot invoke without AIOS root co-signature)
    assert!(
        publisher.trust_level != PublisherTrustLevel::Verified,
        "publisher remains Deplatformed — cannot be restored without re-onboarding"
    );
}

// ============================================================================
// 21 — DEFAULT_CODE_VERSION constant is "aios-distribution/0.0.1-T194"
// ============================================================================

#[test]
fn default_code_version_constant_is_correct() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-distribution/0.0.1-T194");
}

// ============================================================================
// 22 — health_check_quarantine: mixed list with both healthy and deplatformed
//      publisher packages — only the right ones get quarantined
// ============================================================================

#[test]
fn health_check_quarantine_mixed_publishers_quarantines_selectively() {
    // Publisher A: deplatformed
    // Publisher B: healthy (Verified)
    let now = Utc::now();

    let (_aios_signing, aios_vk) = make_keypair();
    let _aios_root = AiosRootKey::new(aios_vk);

    let (_pub_a_signing, pub_a_vk) = make_keypair();
    let (_pub_b_signing, pub_b_vk) = make_keypair();

    let pub_a_id = PublisherRootId("pub:deplatformed-a".into());
    let pub_b_id = PublisherRootId("pub:healthy-b".into());

    // Build catalog: both publishers exist
    let cat = PublisherCatalog::new(vec![
        PublisherRoot {
            publisher_root_id: pub_a_id.clone(),
            public_key: pub_a_vk,
            trust_level: PublisherTrustLevel::Deplatformed,
            onboarding_evidence_pointer: None,
            activated_at: now,
            retired_at: Some(now),
        },
        PublisherRoot {
            publisher_root_id: pub_b_id.clone(),
            public_key: pub_b_vk,
            trust_level: PublisherTrustLevel::Verified,
            onboarding_evidence_pointer: None,
            activated_at: now,
            retired_at: None,
        },
    ]);

    let mut installed = vec![
        InstalledPackageRecord {
            package_id: PackageId("pkg:deplatformed-a:app1".into()),
            publisher_root_id: pub_a_id.clone(),
            signing_key_id: PackageSigningKeyId("pks:deplatformed-a:release".into()),
            state: PackageInstallState::Active,
            installed_at: now,
        },
        InstalledPackageRecord {
            package_id: PackageId("pkg:healthy-b:app2".into()),
            publisher_root_id: pub_b_id.clone(),
            signing_key_id: PackageSigningKeyId("pks:healthy-b:release".into()),
            state: PackageInstallState::Active,
            installed_at: now,
        },
        InstalledPackageRecord {
            package_id: PackageId("pkg:deplatformed-a:app3".into()),
            publisher_root_id: pub_a_id.clone(),
            signing_key_id: PackageSigningKeyId("pks:deplatformed-a:nightly".into()),
            state: PackageInstallState::Active,
            installed_at: now,
        },
    ];

    let revoked: Vec<PackageSigningKeyId> = vec![];
    let quarantined = health_check_quarantine(&mut installed, &cat, &revoked, now);

    assert_eq!(quarantined.len(), 2);
    assert_eq!(installed[0].state, PackageInstallState::Quarantined);
    assert_eq!(installed[1].state, PackageInstallState::Active); // healthy publisher stays Active
    assert_eq!(installed[2].state, PackageInstallState::Quarantined);
}

// ============================================================================
// 23 — health_check_quarantine: packages already Quarantined are not
//      double-counted
// ============================================================================

#[test]
fn health_check_quarantine_skips_already_quarantined() {
    let now = Utc::now();
    let (_sk, vk) = make_keypair();

    let pub_id = PublisherRootId("pub:deplatformed-x-a".to_string());

    let cat = PublisherCatalog::new(vec![PublisherRoot {
        publisher_root_id: pub_id.clone(),
        public_key: vk,
        trust_level: PublisherTrustLevel::Deplatformed,
        onboarding_evidence_pointer: None,
        activated_at: now,
        retired_at: Some(now),
    }]);

    let mut installed = vec![InstalledPackageRecord {
        package_id: PackageId("pkg:deplatformed-x-a:app".to_string()),
        publisher_root_id: pub_id.clone(),
        signing_key_id: PackageSigningKeyId("pks:deplatformed-x-a:release".to_string()),
        state: PackageInstallState::Quarantined, // already quarantined
        installed_at: now,
    }];

    let revoked: Vec<PackageSigningKeyId> = vec![];
    let quarantined = health_check_quarantine(&mut installed, &cat, &revoked, now);

    assert!(
        quarantined.is_empty(),
        "already-quarantined should not be re-reported"
    );
}

// ============================================================================
// 24 — apply_deplatform nonexistent publisher → PublisherNotFound error
// ============================================================================

#[test]
fn apply_deplatform_nonexistent_publisher_errors() {
    let (aios_signing, aios_vk) = make_keypair();
    let aios_root = AiosRootKey::new(aios_vk);
    let now = Utc::now();
    let grace_end = default_grace_end(now);

    let mut event = PublisherDeplatformEvent {
        publisher_root_id: PublisherRootId("pub:nonexistent-test".to_string()),
        reason: TakedownReason::MaliciousBehaviorDetected,
        deplatformed_at: now,
        grace_period_ends_at: grace_end,
        evidence_pointer: "test://evidence/nonexistent-test".to_string(),
        aios_root_signature: vec![0u8; 64],
        extended: false,
    };

    // Sign it with the AIOS root
    let canonical = deplatform_canonical_raw(&event);
    let aios_sig = aios_signing.sign(&canonical);
    event.aios_root_signature = aios_sig.to_bytes().to_vec();

    let mut cat = PublisherCatalog::new(vec![]);
    let result = apply_deplatform(&mut cat, &event, &aios_root, now);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), DistributionErrorCode::PublisherNotFound);
}

// ============================================================================
// 25 — verify_deplatform_event valid → Ok(())
// ============================================================================

#[test]
fn verify_deplatform_event_valid_signature_returns_ok() {
    let (aios_signing, aios_vk) = make_keypair();
    let aios_root = AiosRootKey::new(aios_vk);
    let now = Utc::now();
    let grace_end = default_grace_end(now);

    let mut event = PublisherDeplatformEvent {
        publisher_root_id: PublisherRootId("pub:verify-test".into()),
        reason: TakedownReason::SupplyChainCompromise,
        deplatformed_at: now,
        grace_period_ends_at: grace_end,
        evidence_pointer: "test://evidence/verify-ok".into(),
        aios_root_signature: vec![],
        extended: false,
    };

    let canonical = deplatform_canonical_raw(&event);
    let aios_sig = aios_signing.sign(&canonical);
    event.aios_root_signature = aios_sig.to_bytes().to_vec();

    let result = verify_deplatform_event(&event, &aios_root);
    assert!(result.is_ok(), "valid AIOS root signature should verify OK");
}

// ============================================================================
// 26 — old_root_signature_over_new made by a DIFFERENT old key than the
//      catalog → error (belt-and-suspenders)
// ============================================================================

#[test]
fn rotation_event_old_key_must_match_catalog_key() {
    let fix = TestFixture::new();
    let (_new_sk, new_vk) = make_keypair();
    let (wrong_sk, _wrong_vk) = make_keypair();
    let now = Utc::now();

    // Build an event claiming old_public_key = the catalog key, but the
    // signature is made with a DIFFERENT old key
    let mut event = PublisherRotationEvent {
        publisher_root_id: fix.publisher_root_id.clone(),
        old_public_key: fix.publisher_signing.verifying_key().to_bytes().to_vec(),
        new_public_key: new_vk.to_bytes().to_vec(),
        old_root_signature_over_new: vec![],
        aios_root_signature: vec![],
        rotated_at: now,
        reason: TakedownReason::PublisherRequest,
        compromise_window_start: None,
    };

    // Tampered: old_root_signature is made with wrong_sk, not publisher_signing
    let canonical = rotation_canonical_raw(&event);
    let wrong_sig = wrong_sk.sign(&canonical);
    event.old_root_signature_over_new = wrong_sig.to_bytes().to_vec();
    // AIOS root sig is correct
    let aios_sig = fix.aios_root_signing.sign(&canonical);
    event.aios_root_signature = aios_sig.to_bytes().to_vec();

    let publisher = fix
        .publisher_catalog
        .lookup(&fix.publisher_root_id)
        .unwrap();
    let result = verify_rotation_event(&event, publisher, &fix.aios_root_key);
    assert!(
        result.is_err(),
        "old_root_signature made with wrong key should fail verification"
    );
}
