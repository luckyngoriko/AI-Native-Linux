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

use std::collections::HashMap;

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

/// Builds a canonical 3-tier trust chain for testing.
///
/// Returns:
/// - `(aios_root, publisher_catalog, signing_catalogs)` — the verifier inputs
/// - `(publisher_root_link_sig, signing_key_link_sig)` — the link signatures
/// - `signed_payload` — the opaque payload with its signature
/// - `publisher_root` + `signing_key` — the raw catalog entries
#[allow(clippy::type_complexity)]
fn build_valid_chain(
    trust_level: PublisherTrustLevel,
) -> (
    AiosRootKey,
    PublisherCatalog,
    HashMap<String, SigningKeyCatalog>,
    LinkSignature,
    LinkSignature,
    SignedPayload,
    PublisherRoot,
    PackageSigningKey,
) {
    let now = Utc::now();

    // Tier 1: AIOS root key
    let (aios_root_sk, aios_root_vk) = make_keypair();
    let aios_root = AiosRootKey::new(aios_root_vk);

    // Tier 2: publisher root
    let (publisher_sk, publisher_vk) = make_keypair();
    let publisher_root_id = PublisherRootId("pub:testsuite".into());
    let publisher_root = PublisherRoot {
        publisher_root_id: publisher_root_id.clone(),
        public_key: publisher_vk,
        trust_level,
        onboarding_evidence_pointer: Some("evid://test/onboard-001".into()),
        activated_at: now - Duration::days(30),
        retired_at: None,
    };

    // AIOS root signs the publisher root canonical bytes
    let publisher_canonical = publisher_root.canonical_entry_bytes();
    let publisher_root_sig =
        LinkSignature(aios_root_sk.sign(&publisher_canonical).to_bytes().to_vec());

    let publisher_catalog = PublisherCatalog::new(vec![publisher_root.clone()]);

    // Tier 3: package signing key
    let (signing_sk, signing_vk) = make_keypair();
    let signing_key_id = PackageSigningKeyId("pks:testsuite:ci-pipeline".into());
    let signing_key = PackageSigningKey {
        package_signing_key_id: signing_key_id.clone(),
        public_key: signing_vk,
        valid_from: now - Duration::days(7),
        valid_until: Some(now + Duration::days(365)),
        revoked_at: None,
    };

    // Publisher root signs the signing key canonical bytes
    let signing_canonical = signing_key.canonical_entry_bytes();
    let signing_key_sig = LinkSignature(publisher_sk.sign(&signing_canonical).to_bytes().to_vec());

    let mut signing_catalogs = HashMap::new();
    signing_catalogs.insert(
        "testsuite".to_string(),
        SigningKeyCatalog::new("testsuite".into(), vec![signing_key.clone()]),
    );

    // Payload: opaque bytes + signature by the signing key
    let payload = b"test-manifest-canonical-hash-abcdef0123456789".to_vec();
    let payload_sig = signing_sk.sign(&payload).to_bytes().to_vec();

    let signed_payload = SignedPayload {
        payload,
        signature: payload_sig,
        package_signing_key_id: signing_key_id,
        publisher_root_id: publisher_root_id.clone(),
    };

    (
        aios_root,
        publisher_catalog,
        signing_catalogs,
        publisher_root_sig,
        signing_key_sig,
        signed_payload,
        publisher_root,
        signing_key,
    )
}

/// Helper to verify a chain and return the result.
fn verify_chain(
    aios_root: &AiosRootKey,
    publisher_catalog: &PublisherCatalog,
    signing_catalogs: &HashMap<String, SigningKeyCatalog>,
    payload: &SignedPayload,
    publisher_sig: &LinkSignature,
    signing_sig: &LinkSignature,
    issued_at: chrono::DateTime<Utc>,
    now: chrono::DateTime<Utc>,
) -> PackageVerificationResult {
    let verifier = TrustChainVerifier::new(aios_root, publisher_catalog, signing_catalogs);
    verifier.verify(payload, publisher_sig, signing_sig, issued_at, now)
}

// ---------------------------------------------------------------------------
// 01 — valid 3-hop chain (root→publisher→signing→payload) → VerifiedPublisher
// ---------------------------------------------------------------------------

#[test]
fn valid_3_hop_chain_returns_verified_publisher() {
    let (aios_root, pubcat, sigcats, pub_sig, key_sig, payload, _pr, _sk) =
        build_valid_chain(PublisherTrustLevel::Verified);

    let now = Utc::now();
    let issued_at = now - Duration::hours(1);

    let result = verify_chain(
        &aios_root, &pubcat, &sigcats, &payload, &pub_sig, &key_sig, issued_at, now,
    );

    assert_eq!(result, PackageVerificationResult::VerifiedPublisher);
}

// ---------------------------------------------------------------------------
// 02 — valid chain with publisher trust_level AiosRoot → VerifiedAiosRoot
// ---------------------------------------------------------------------------

#[test]
fn valid_chain_aios_root_trust_level_returns_verified_aios_root() {
    let (aios_root, pubcat, sigcats, pub_sig, key_sig, payload, _pr, _sk) =
        build_valid_chain(PublisherTrustLevel::AiosRoot);

    let now = Utc::now();
    let issued_at = now - Duration::hours(1);

    let result = verify_chain(
        &aios_root, &pubcat, &sigcats, &payload, &pub_sig, &key_sig, issued_at, now,
    );

    assert_eq!(result, PackageVerificationResult::VerifiedAiosRoot);
}

// ---------------------------------------------------------------------------
// 03 — tampered payload signature → SignatureFailed
// ---------------------------------------------------------------------------

#[test]
fn tampered_payload_signature_returns_signature_failed() {
    let (aios_root, pubcat, sigcats, pub_sig, key_sig, mut payload, _pr, _sk) =
        build_valid_chain(PublisherTrustLevel::Verified);

    // Flip a byte in the payload signature
    if !payload.signature.is_empty() {
        payload.signature[0] ^= 0xFF;
    }

    let now = Utc::now();
    let issued_at = now - Duration::hours(1);

    let result = verify_chain(
        &aios_root, &pubcat, &sigcats, &payload, &pub_sig, &key_sig, issued_at, now,
    );

    assert_eq!(result, PackageVerificationResult::SignatureFailed);
}

// ---------------------------------------------------------------------------
// 04 — tampered publisher-root link signature → SignatureFailed
// ---------------------------------------------------------------------------

#[test]
fn tampered_publisher_root_link_sig_returns_signature_failed() {
    let (aios_root, pubcat, sigcats, mut pub_sig, key_sig, payload, _pr, _sk) =
        build_valid_chain(PublisherTrustLevel::Verified);

    // Flip a byte in the publisher-root link signature
    if !pub_sig.0.is_empty() {
        pub_sig.0[0] ^= 0xFF;
    }

    let now = Utc::now();
    let issued_at = now - Duration::hours(1);

    let result = verify_chain(
        &aios_root, &pubcat, &sigcats, &payload, &pub_sig, &key_sig, issued_at, now,
    );

    assert_eq!(result, PackageVerificationResult::SignatureFailed);
}

// ---------------------------------------------------------------------------
// 05 — tampered signing-key link signature → SignatureFailed
// ---------------------------------------------------------------------------

#[test]
fn tampered_signing_key_link_sig_returns_signature_failed() {
    let (aios_root, pubcat, sigcats, pub_sig, mut key_sig, payload, _pr, _sk) =
        build_valid_chain(PublisherTrustLevel::Verified);

    // Flip a byte in the signing-key link signature
    if !key_sig.0.is_empty() {
        key_sig.0[0] ^= 0xFF;
    }

    let now = Utc::now();
    let issued_at = now - Duration::hours(1);

    let result = verify_chain(
        &aios_root, &pubcat, &sigcats, &payload, &pub_sig, &key_sig, issued_at, now,
    );

    assert_eq!(result, PackageVerificationResult::SignatureFailed);
}

// ---------------------------------------------------------------------------
// 06 — publisher_root_id absent from catalog → TrustChainBroken
// ---------------------------------------------------------------------------

#[test]
fn publisher_root_id_absent_from_catalog_returns_trust_chain_broken() {
    let (aios_root, _pubcat, sigcats, pub_sig, key_sig, payload, _pr, _sk) =
        build_valid_chain(PublisherTrustLevel::Verified);

    // Use an empty catalog — the publisher_root_id won't be found
    let empty_catalog = PublisherCatalog::default();

    let now = Utc::now();
    let issued_at = now - Duration::hours(1);

    let result = verify_chain(
        &aios_root,
        &empty_catalog,
        &sigcats,
        &payload,
        &pub_sig,
        &key_sig,
        issued_at,
        now,
    );

    assert_eq!(result, PackageVerificationResult::TrustChainBroken);
}

// ---------------------------------------------------------------------------
// 07 — package_signing_key_id absent from signing catalog → TrustChainBroken
// ---------------------------------------------------------------------------

#[test]
fn package_signing_key_id_absent_from_signing_catalog_returns_trust_chain_broken() {
    let (aios_root, pubcat, _sigcats, pub_sig, key_sig, payload, _pr, _sk) =
        build_valid_chain(PublisherTrustLevel::Verified);

    // Empty signing catalogs — the signing_key_id won't be found
    let empty_sigcats: HashMap<String, SigningKeyCatalog> = HashMap::new();

    let now = Utc::now();
    let issued_at = now - Duration::hours(1);

    let result = verify_chain(
        &aios_root,
        &pubcat,
        &empty_sigcats,
        &payload,
        &pub_sig,
        &key_sig,
        issued_at,
        now,
    );

    assert_eq!(result, PackageVerificationResult::TrustChainBroken);
}

// ---------------------------------------------------------------------------
// 08 — signing key revoked BEFORE issued_at → TrustChainBroken
// ---------------------------------------------------------------------------

#[test]
fn signing_key_revoked_before_issued_at_returns_trust_chain_broken() {
    let (aios_root_sk, aios_root_vk) = make_keypair();
    let aios_root = AiosRootKey::new(aios_root_vk);

    let (publisher_sk, publisher_vk) = make_keypair();
    let publisher_root_id = PublisherRootId("pub:revokedtest".into());

    let activated = Utc::now() - Duration::days(60);
    let publisher_root = PublisherRoot {
        publisher_root_id: publisher_root_id.clone(),
        public_key: publisher_vk,
        trust_level: PublisherTrustLevel::Verified,
        onboarding_evidence_pointer: None,
        activated_at: activated,
        retired_at: None,
    };

    let publisher_root_sig = LinkSignature(
        aios_root_sk
            .sign(&publisher_root.canonical_entry_bytes())
            .to_bytes()
            .to_vec(),
    );
    let pubcat = PublisherCatalog::new(vec![publisher_root.clone()]);

    let (signing_sk, signing_vk) = make_keypair();
    let signing_key_id = PackageSigningKeyId("pks:revokedtest:release".into());

    let valid_from = Utc::now() - Duration::days(30);
    let signing_key = PackageSigningKey {
        package_signing_key_id: signing_key_id.clone(),
        public_key: signing_vk,
        valid_from,
        valid_until: None,
        // Key revoked 5 days ago
        revoked_at: Some(Utc::now() - Duration::days(5)),
    };

    let signing_key_sig = LinkSignature(
        publisher_sk
            .sign(&signing_key.canonical_entry_bytes())
            .to_bytes()
            .to_vec(),
    );

    let mut sigcats = HashMap::new();
    sigcats.insert(
        "revokedtest".to_string(),
        SigningKeyCatalog::new("revokedtest".into(), vec![signing_key]),
    );

    let payload = b"some-payload".to_vec();
    let payload_sig = signing_sk.sign(&payload).to_bytes().to_vec();
    let signed_payload = SignedPayload {
        payload,
        signature: payload_sig,
        package_signing_key_id: signing_key_id,
        publisher_root_id,
    };

    let now = Utc::now();
    // issued_at is NOW, but key was revoked 5 days ago → revoked_at ≤ issued_at
    let issued_at = now;

    let result = verify_chain(
        &aios_root,
        &pubcat,
        &sigcats,
        &signed_payload,
        &publisher_root_sig,
        &signing_key_sig,
        issued_at,
        now,
    );

    assert_eq!(result, PackageVerificationResult::TrustChainBroken);
}

// ---------------------------------------------------------------------------
// 09 — signing key revoked AFTER issued_at still verifies (continuity)
// ---------------------------------------------------------------------------

#[test]
fn signing_key_revoked_after_issued_at_still_verifies() {
    let (aios_root_sk, aios_root_vk) = make_keypair();
    let aios_root = AiosRootKey::new(aios_root_vk);

    let (publisher_sk, publisher_vk) = make_keypair();
    let publisher_root_id = PublisherRootId("pub:continuitytest".into());

    let activated = Utc::now() - Duration::days(60);
    let publisher_root = PublisherRoot {
        publisher_root_id: publisher_root_id.clone(),
        public_key: publisher_vk,
        trust_level: PublisherTrustLevel::Verified,
        onboarding_evidence_pointer: None,
        activated_at: activated,
        retired_at: None,
    };

    let publisher_root_sig = LinkSignature(
        aios_root_sk
            .sign(&publisher_root.canonical_entry_bytes())
            .to_bytes()
            .to_vec(),
    );
    let pubcat = PublisherCatalog::new(vec![publisher_root.clone()]);

    let (signing_sk, signing_vk) = make_keypair();
    let signing_key_id = PackageSigningKeyId("pks:continuitytest:release".into());

    let valid_from = Utc::now() - Duration::days(30);
    // issued_at is 10 days ago; revoked_at is 3 days ago → revoked_at > issued_at
    let issued_at = Utc::now() - Duration::days(10);
    let signing_key = PackageSigningKey {
        package_signing_key_id: signing_key_id.clone(),
        public_key: signing_vk,
        valid_from,
        valid_until: None,
        revoked_at: Some(Utc::now() - Duration::days(3)),
    };

    let signing_key_sig = LinkSignature(
        publisher_sk
            .sign(&signing_key.canonical_entry_bytes())
            .to_bytes()
            .to_vec(),
    );

    let mut sigcats = HashMap::new();
    sigcats.insert(
        "continuitytest".to_string(),
        SigningKeyCatalog::new("continuitytest".into(), vec![signing_key]),
    );

    let payload = b"continuity-payload".to_vec();
    let payload_sig = signing_sk.sign(&payload).to_bytes().to_vec();
    let signed_payload = SignedPayload {
        payload,
        signature: payload_sig,
        package_signing_key_id: signing_key_id,
        publisher_root_id,
    };

    let now = Utc::now();

    let result = verify_chain(
        &aios_root,
        &pubcat,
        &sigcats,
        &signed_payload,
        &publisher_root_sig,
        &signing_key_sig,
        issued_at,
        now,
    );

    assert_eq!(result, PackageVerificationResult::VerifiedPublisher);
}

// ---------------------------------------------------------------------------
// 10 — retired publisher root (retired_at in the past) → TrustChainBroken
// ---------------------------------------------------------------------------

#[test]
fn retired_publisher_root_returns_trust_chain_broken() {
    let (aios_root_sk, aios_root_vk) = make_keypair();
    let aios_root = AiosRootKey::new(aios_root_vk);

    let (publisher_sk, publisher_vk) = make_keypair();
    let publisher_root_id = PublisherRootId("pub:retiredtest".into());

    let activated = Utc::now() - Duration::days(120);
    // Retired 10 days ago
    let retired_at = Some(Utc::now() - Duration::days(10));
    let publisher_root = PublisherRoot {
        publisher_root_id: publisher_root_id.clone(),
        public_key: publisher_vk,
        trust_level: PublisherTrustLevel::Verified,
        onboarding_evidence_pointer: None,
        activated_at: activated,
        retired_at,
    };

    let publisher_root_sig = LinkSignature(
        aios_root_sk
            .sign(&publisher_root.canonical_entry_bytes())
            .to_bytes()
            .to_vec(),
    );
    let pubcat = PublisherCatalog::new(vec![publisher_root.clone()]);

    let (signing_sk, signing_vk) = make_keypair();
    let signing_key_id = PackageSigningKeyId("pks:retiredtest:release".into());

    let valid_from = Utc::now() - Duration::days(60);
    let signing_key = PackageSigningKey {
        package_signing_key_id: signing_key_id.clone(),
        public_key: signing_vk,
        valid_from,
        valid_until: None,
        revoked_at: None,
    };

    let signing_key_sig = LinkSignature(
        publisher_sk
            .sign(&signing_key.canonical_entry_bytes())
            .to_bytes()
            .to_vec(),
    );

    let mut sigcats = HashMap::new();
    sigcats.insert(
        "retiredtest".to_string(),
        SigningKeyCatalog::new("retiredtest".into(), vec![signing_key]),
    );

    let payload = b"retired-payload".to_vec();
    let payload_sig = signing_sk.sign(&payload).to_bytes().to_vec();
    let signed_payload = SignedPayload {
        payload,
        signature: payload_sig,
        package_signing_key_id: signing_key_id,
        publisher_root_id,
    };

    let now = Utc::now();
    let issued_at = now - Duration::hours(1);

    let result = verify_chain(
        &aios_root,
        &pubcat,
        &sigcats,
        &signed_payload,
        &publisher_root_sig,
        &signing_key_sig,
        issued_at,
        now,
    );

    assert_eq!(result, PackageVerificationResult::TrustChainBroken);
}

// ---------------------------------------------------------------------------
// 11 — publisher root trust_level Deplatformed → PublisherDeplatformed
// ---------------------------------------------------------------------------

#[test]
fn publisher_deplatformed_returns_publisher_deplatformed() {
    let (aios_root, pubcat, sigcats, pub_sig, key_sig, payload, _pr, _sk) =
        build_valid_chain(PublisherTrustLevel::Deplatformed);

    let now = Utc::now();
    let issued_at = now - Duration::hours(1);

    let result = verify_chain(
        &aios_root, &pubcat, &sigcats, &payload, &pub_sig, &key_sig, issued_at, now,
    );

    assert_eq!(result, PackageVerificationResult::PublisherDeplatformed);
}

// ---------------------------------------------------------------------------
// 12 — direct AIOS-root-signed payload (bypass, no publisher hop) → TrustChainBroken
// ---------------------------------------------------------------------------

#[test]
fn direct_aios_root_signed_payload_bypass_returns_trust_chain_broken() {
    // AIOS root keypair — both signing and verifying
    let (aios_root_sk, aios_root_vk) = make_keypair();
    let aios_root = AiosRootKey::new(aios_root_vk);

    // Create a publisher root in the catalog (so it's found)
    let (publisher_sk, publisher_vk) = make_keypair();
    let publisher_root_id = PublisherRootId("pub:bypasstest".into());
    let activated = Utc::now() - Duration::days(30);
    let publisher_root = PublisherRoot {
        publisher_root_id: publisher_root_id.clone(),
        public_key: publisher_vk,
        trust_level: PublisherTrustLevel::Verified,
        onboarding_evidence_pointer: None,
        activated_at: activated,
        retired_at: None,
    };
    let publisher_root_sig = LinkSignature(
        aios_root_sk
            .sign(&publisher_root.canonical_entry_bytes())
            .to_bytes()
            .to_vec(),
    );
    let pubcat = PublisherCatalog::new(vec![publisher_root.clone()]);

    // The "signing key" in the catalog is actually the AIOS root key itself
    let signing_key_id = PackageSigningKeyId("pks:bypasstest:bypass".into());
    let signing_key_entry = PackageSigningKey {
        package_signing_key_id: signing_key_id.clone(),
        public_key: aios_root_vk, // SAME as AIOS root — this IS the bypass
        valid_from: activated,
        valid_until: None,
        revoked_at: None,
    };
    let signing_key_sig = LinkSignature(
        publisher_sk
            .sign(&signing_key_entry.canonical_entry_bytes())
            .to_bytes()
            .to_vec(),
    );

    let mut sigcats = HashMap::new();
    sigcats.insert(
        "bypasstest".to_string(),
        SigningKeyCatalog::new("bypasstest".into(), vec![signing_key_entry]),
    );

    // Payload is signed by the AIOS root key directly (bypass attempt)
    let payload = b"bypass-payload".to_vec();
    let payload_sig = aios_root_sk.sign(&payload).to_bytes().to_vec();
    let signed_payload = SignedPayload {
        payload,
        signature: payload_sig,
        package_signing_key_id: signing_key_id,
        publisher_root_id: publisher_root_id.clone(),
    };

    let now = Utc::now();
    let issued_at = now - Duration::hours(1);

    let result = verify_chain(
        &aios_root,
        &pubcat,
        &sigcats,
        &signed_payload,
        &publisher_root_sig,
        &signing_key_sig,
        issued_at,
        now,
    );

    // The signing key's public key == AIOS root's public key → bypass → TrustChainBroken
    assert_eq!(result, PackageVerificationResult::TrustChainBroken);
}

// ---------------------------------------------------------------------------
// 13 — chain depth exactly 3 accepted
// ---------------------------------------------------------------------------

#[test]
fn chain_depth_exactly_3_accepted() {
    let (aios_root, pubcat, sigcats, pub_sig, key_sig, payload, _pr, _sk) =
        build_valid_chain(PublisherTrustLevel::Verified);

    let now = Utc::now();
    let issued_at = now - Duration::hours(1);

    let verifier = TrustChainVerifier::new(&aios_root, &pubcat, &sigcats);
    let result = verifier.verify(&payload, &pub_sig, &key_sig, issued_at, now);

    assert_eq!(result, PackageVerificationResult::VerifiedPublisher);
    // canonical_depth() returns 3, which is ≤ MAX_CHAIN_DEPTH (3)
    assert!(canonical_depth() <= MAX_CHAIN_DEPTH);
}

// ---------------------------------------------------------------------------
// 14 — chain depth 4 → TrustChainTooDeep
// ---------------------------------------------------------------------------

#[test]
fn chain_depth_4_rejected_with_trust_chain_too_deep() {
    // T-188 canonical chains are depth 3. To test the depth guard, we
    // construct a verifier with a reduced max_depth (2) so that a normal
    // 3-hop chain triggers TrustChainTooDeep. This validates that the
    // guard fires when depth > max_depth, which matters when T-189+
    // introduces intermediate signing patterns.
    let (aios_root, pubcat, sigcats, pub_sig, key_sig, payload, _pr, _sk) =
        build_valid_chain(PublisherTrustLevel::Verified);

    let now = Utc::now();
    let issued_at = now - Duration::hours(1);

    // Factory method with reduced max_depth for testing the guard
    let verifier = TrustChainVerifier::with_max_depth(&aios_root, &pubcat, &sigcats, 2);
    let result = verifier.verify(&payload, &pub_sig, &key_sig, issued_at, now);

    assert_eq!(result, PackageVerificationResult::TrustChainTooDeep);
}

// ---------------------------------------------------------------------------
// 15 — MAX_CHAIN_DEPTH constant == 3
// ---------------------------------------------------------------------------

#[test]
fn max_chain_depth_constant_equals_3() {
    assert_eq!(MAX_CHAIN_DEPTH, 3);
    assert_eq!(canonical_depth(), 3);
}

// ---------------------------------------------------------------------------
// 16 — wrong AIOS root key (publisher root not signed by pinned root)
//      → SignatureFailed
// ---------------------------------------------------------------------------

#[test]
fn wrong_aios_root_key_returns_signature_failed() {
    // Build a valid chain with one AIOS root
    let (aios_root_sk, aios_root_vk) = make_keypair();
    let _aios_root = AiosRootKey::new(aios_root_vk);

    let (publisher_sk, publisher_vk) = make_keypair();
    let publisher_root_id = PublisherRootId("pub:wrangler".into());
    let activated = Utc::now() - Duration::days(30);
    let publisher_root = PublisherRoot {
        publisher_root_id: publisher_root_id.clone(),
        public_key: publisher_vk,
        trust_level: PublisherTrustLevel::Verified,
        onboarding_evidence_pointer: None,
        activated_at: activated,
        retired_at: None,
    };
    let publisher_root_sig = LinkSignature(
        aios_root_sk
            .sign(&publisher_root.canonical_entry_bytes())
            .to_bytes()
            .to_vec(),
    );
    let pubcat = PublisherCatalog::new(vec![publisher_root.clone()]);

    let (signing_sk, signing_vk) = make_keypair();
    let signing_key_id = PackageSigningKeyId("pks:wrangler:ci".into());
    let signing_key = PackageSigningKey {
        package_signing_key_id: signing_key_id.clone(),
        public_key: signing_vk,
        valid_from: activated,
        valid_until: None,
        revoked_at: None,
    };
    let signing_key_sig = LinkSignature(
        publisher_sk
            .sign(&signing_key.canonical_entry_bytes())
            .to_bytes()
            .to_vec(),
    );
    let mut sigcats = HashMap::new();
    sigcats.insert(
        "wrangler".to_string(),
        SigningKeyCatalog::new("wrangler".into(), vec![signing_key]),
    );

    let payload_bytes = b"wrong-root-payload".to_vec();
    let payload_sig = signing_sk.sign(&payload_bytes).to_bytes().to_vec();
    let signed_payload = SignedPayload {
        payload: payload_bytes,
        signature: payload_sig,
        package_signing_key_id: signing_key_id,
        publisher_root_id,
    };

    // Use a WRONG AIOS root (a different keypair) — the publisher_root_link_sig
    // was signed by the real root, so verifying with a different root's public
    // key will fail Ed25519 → SignatureFailed.
    let (_wrong_signing, wrong_vk) = make_keypair();
    let wrong_root = AiosRootKey::new(wrong_vk);

    let now = Utc::now();
    let issued_at = now - Duration::hours(1);

    let result = verify_chain(
        &wrong_root,
        &pubcat,
        &sigcats,
        &signed_payload,
        &publisher_root_sig,
        &signing_key_sig,
        issued_at,
        now,
    );

    assert_eq!(result, PackageVerificationResult::SignatureFailed);
}

// ---------------------------------------------------------------------------
// 17 — DEFAULT_CODE_VERSION constant is correct
// ---------------------------------------------------------------------------

#[test]
fn default_code_version_constant_is_correct() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-distribution/0.0.1-T190");
}

// ---------------------------------------------------------------------------
// 18 — catalog lookup hit and miss
// ---------------------------------------------------------------------------

#[test]
fn catalog_lookup_hit_and_miss() {
    // PublisherCatalog hit/miss
    let empty_cat = PublisherCatalog::default();
    let unknown_id = PublisherRootId("pub:nonexistent".into());
    assert!(empty_cat.lookup(&unknown_id).is_none());
    assert!(!empty_cat.is_active(&unknown_id, &Utc::now()));

    // SigningKeyCatalog hit/miss
    let empty_skcat = SigningKeyCatalog::new("testvendor".into(), vec![]);
    let unknown_sk_id = PackageSigningKeyId("pks:testvendor:ghost".into());
    assert!(empty_skcat.lookup(&unknown_sk_id).is_none());
    assert!(!empty_skcat.is_revoked_at(&unknown_sk_id, &Utc::now()));
}

// ---------------------------------------------------------------------------
// 19 — VerifiedAiosRoot result is_success() == true
// ---------------------------------------------------------------------------

#[test]
fn verified_aios_root_is_success() {
    assert!(PackageVerificationResult::VerifiedAiosRoot.is_success());
}

// ---------------------------------------------------------------------------
// 20 — VerifiedPublisher result is_success() == true
// ---------------------------------------------------------------------------

#[test]
fn verified_publisher_is_success() {
    assert!(PackageVerificationResult::VerifiedPublisher.is_success());
}

// ---------------------------------------------------------------------------
// 21 — SignatureFailed, TrustChainBroken, TrustChainTooDeep are NOT success
// ---------------------------------------------------------------------------

#[test]
fn failure_results_are_not_success() {
    assert!(!PackageVerificationResult::SignatureFailed.is_success());
    assert!(!PackageVerificationResult::TrustChainBroken.is_success());
    assert!(!PackageVerificationResult::TrustChainTooDeep.is_success());
    assert!(!PackageVerificationResult::PublisherDeplatformed.is_success());
}
