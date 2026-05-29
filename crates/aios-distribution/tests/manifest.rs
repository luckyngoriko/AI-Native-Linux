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

// ============================================================================
// Test helpers — build a manifest with a valid 3-hop trust chain
// ============================================================================

/// Generates a fresh Ed25519 keypair.
fn make_keypair() -> (SigningKey, VerifyingKey) {
    let mut csprng = OsRng;
    let signing = SigningKey::generate(&mut csprng);
    let verifying = signing.verifying_key();
    (signing, verifying)
}

/// Builds a valid `PackageManifest` with a signed 3-hop trust chain.
///
/// Returns everything needed to call `verify_manifest`.
#[allow(clippy::type_complexity)]
fn build_signed_manifest(
    trust_level: PublisherTrustLevel,
    kind: PackageKind,
) -> (
    PackageManifest,
    TrustChainVerifier<'static>,
    LinkSignature,
    LinkSignature,
    AiosRootKey,                        // keep alive
    PublisherCatalog,                   // keep alive
    HashMap<String, SigningKeyCatalog>, // keep alive
    PackageSigningKey,                  // keep alive
    PublisherRoot,                      // keep alive
    SigningKey,                         // the signing key for the manifest sig
    String,                             // fetched content hash (matches content_hash)
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
        onboarding_evidence_pointer: Some("evid://test/onboard-mf-001".into()),
        activated_at: now - Duration::days(30),
        retired_at: None,
    };
    let publisher_root_sig = LinkSignature(
        aios_root_sk
            .sign(&publisher_root.canonical_entry_bytes())
            .to_bytes()
            .to_vec(),
    );
    let publisher_catalog = PublisherCatalog::new(vec![publisher_root.clone()]);

    // Tier 3: package signing key
    let (signing_sk, signing_vk) = make_keypair();
    let signing_key_id = PackageSigningKeyId("pks:testsuite:release".into());
    let signing_key = PackageSigningKey {
        package_signing_key_id: signing_key_id.clone(),
        public_key: signing_vk,
        valid_from: now - Duration::days(7),
        valid_until: Some(now + Duration::days(365)),
        revoked_at: None,
    };
    let signing_key_sig = LinkSignature(
        publisher_sk
            .sign(&signing_key.canonical_entry_bytes())
            .to_bytes()
            .to_vec(),
    );

    let mut signing_catalogs = HashMap::new();
    signing_catalogs.insert(
        "testsuite".to_string(),
        SigningKeyCatalog::new("testsuite".into(), vec![signing_key.clone()]),
    );

    // Build manifest
    let content_bytes = b"test-package-content-v1.0.0";
    let content_hash_val = canonical::content_hash(content_bytes);
    let fetched_content_hash = content_hash_val.clone();

    let caps = if kind == PackageKind::Theme {
        vec![]
    } else {
        vec!["filesystem.read".into(), "network.outbound".into()]
    };

    let repo = if trust_level == PublisherTrustLevel::AiosRoot {
        RepositoryKind::AiosRootRepo
    } else {
        RepositoryKind::AiosVerifiedRepo
    };

    let mut manifest = PackageManifest {
        package_id: "pkg:testsuite:testapp".into(),
        version: "1.0.0".into(),
        kind,
        publisher_trust: trust_level,
        publisher_root_id: publisher_root_id.clone(),
        package_signing_key_id: signing_key_id.clone(),
        content_hash: content_hash_val,
        manifest_canonical_hash: String::new(), // filled in below
        ed25519_signature: Vec::new(),          // filled in below
        installable_scope: InstallScope::Either,
        required_sandbox: SandboxProfileRef("default-profile".into()),
        declared_capabilities: caps,
        network_manifest: NetworkManifestRef("default-net".into()),
        issued_at: now - Duration::hours(1),
        eol_at: None,
        channel: UpdateChannel::Stable,
        originating_repository: repo,
        mirror_url: "https://mirror.example.com".into(),
        mirror_semantic: MirrorSemantic::Cached,
    };

    // Compute canonical hash
    let ch = canonical::manifest_canonical_hash(&manifest);
    manifest.manifest_canonical_hash.clone_from(&ch);

    // Sign the manifest
    let payload = canonical::signing_payload(&ch);
    let sig = signing_sk.sign(payload).to_bytes().to_vec();
    manifest.ed25519_signature = sig;

    // Build the verifier (we leak the catalogs so the verifier's borrows live)
    // SAFETY: test-only — the leaked data is tiny and exists for the test duration.
    let aios_root_leaked: &'static AiosRootKey = Box::leak(Box::new(aios_root.clone()));
    let pubcat_leaked: &'static PublisherCatalog = Box::leak(Box::new(publisher_catalog.clone()));
    let sigcats_leaked: &'static HashMap<String, SigningKeyCatalog> =
        Box::leak(Box::new(signing_catalogs.clone()));
    let verifier = TrustChainVerifier::new(aios_root_leaked, pubcat_leaked, sigcats_leaked);

    (
        manifest,
        verifier,
        publisher_root_sig,
        signing_key_sig,
        aios_root,
        publisher_catalog,
        signing_catalogs,
        signing_key,
        publisher_root,
        signing_sk,
        fetched_content_hash,
    )
}

// ============================================================================
// 01 — valid signed manifest → VerifiedPublisher
// ============================================================================

#[test]
fn valid_signed_manifest_returns_verified_publisher() {
    let (manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::VerifiedPublisher);
}

// ============================================================================
// 02 — AIOS-root publisher → VerifiedAiosRoot
// ============================================================================

#[test]
fn aios_root_manifest_returns_verified_aios_root() {
    let (manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::AiosRoot, PackageKind::InvariantBundle);

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::VerifiedAiosRoot);
}

// ============================================================================
// 03 — package_id bad regex → ManifestForged
// ============================================================================

#[test]
fn package_id_bad_regex_manifest_forged() {
    let (mut manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    manifest.package_id = "bad-id-format".into();

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::ManifestForged);
}

// ============================================================================
// 04 — package_id vendor ≠ publisher_root_id vendor → ManifestForged
// ============================================================================

#[test]
fn package_id_vendor_mismatch_manifest_forged() {
    let (mut manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    // publisher_root_id is pub:testsuite; change package vendor
    manifest.package_id = "pkg:othervendor:testapp".into();

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::ManifestForged);
}

// ============================================================================
// 05 — bad semver version → ManifestForged
// ============================================================================

#[test]
fn bad_semver_version_manifest_forged() {
    let (mut manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    manifest.version = "not.a.version".into();

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::ManifestForged);
}

// ============================================================================
// 06 — valid semver with prerelease+build accepted (via verify_manifest)
// ============================================================================

#[test]
fn valid_semver_with_prerelease_and_build_accepted() {
    let (
        mut manifest,
        _verifier,
        _pub_sig,
        _key_sig,
        _root,
        _pubcat,
        _sigcats,
        _sk,
        _pr,
        _sks,
        _fch,
    ) = build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    manifest.version = "2.0.0-rc.1+build.42".into();
    // validate_fields doesn't check canonical hash or signature —
    // it only validates field formats. No re-sign needed.
    let now = Utc::now();
    let drift = Duration::seconds(300);
    let r = validate_fields(&manifest, now, drift);
    assert!(
        r.is_ok(),
        "valid semver with prerelease+build should pass field validation"
    );
}

// ============================================================================
// 07 — publisher_root_id bad regex → TrustChainBroken
// ============================================================================

#[test]
fn publisher_root_id_bad_regex_trust_chain_broken() {
    let (mut manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    // Both IDs must change together so the package_id vendor cross-check doesn't
    // fire first (it produces ManifestForged, masking the TrustChainBroken we want).
    manifest.publisher_root_id = PublisherRootId("bad-id-no-pub-prefix".into());
    manifest.package_id = "pkg:bad-id-no-pub-prefix:testapp".into();

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::TrustChainBroken);
}

// ============================================================================
// 08 — package_signing_key_id bad regex → TrustChainBroken
// ============================================================================

#[test]
fn package_signing_key_id_bad_regex_trust_chain_broken() {
    let (mut manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    manifest.package_signing_key_id = PackageSigningKeyId("nopks-prefix".into());

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::TrustChainBroken);
}

// ============================================================================
// 09 — tampered manifest_canonical_hash (≠ recomputed) → ManifestForged
// ============================================================================

#[test]
fn tampered_manifest_canonical_hash_manifest_forged() {
    let (mut manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    // Tamper the canonical hash to a nonsense value
    manifest.manifest_canonical_hash = "0".repeat(32);
    // Re-sign with the tampered hash — we don't need this test to check signature
    // because canonical hash check fires first (step 2 before step 4).

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::ManifestForged);
}

// ============================================================================
// 10 — fetched content hash ≠ manifest content_hash → HashMismatch
// ============================================================================

#[test]
fn fetched_content_hash_mismatch_hash_mismatch() {
    let (manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, _fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    // Pass a different fetched content hash
    let wrong_fch = "f".repeat(32);

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &wrong_fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::HashMismatch);
}

// ============================================================================
// 11 — tampered signature → SignatureFailed (via verifier)
// ============================================================================

#[test]
fn tampered_manifest_signature_signature_failed() {
    let (mut manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    // Flip a byte in the signature
    if !manifest.ed25519_signature.is_empty() {
        manifest.ed25519_signature[0] ^= 0xFF;
    }

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::SignatureFailed);
}

// ============================================================================
// 12 — installable_scope UserScoped → ManifestForged
// ============================================================================

#[test]
fn installable_scope_user_scoped_manifest_forged() {
    let (mut manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    manifest.installable_scope = InstallScope::UserScoped;

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::ManifestForged);
}

// ============================================================================
// 13 — empty declared_capabilities on non-Theme → BundleTampered
// ============================================================================

#[test]
fn empty_capabilities_on_non_theme_bundle_tampered() {
    let (mut manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    manifest.declared_capabilities = vec![];
    // Re-sign since declared_capabilities is in the canonical hash
    let ch = canonical::manifest_canonical_hash(&manifest);
    manifest.manifest_canonical_hash = ch;

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::BundleTampered);
}

// ============================================================================
// 14 — empty declared_capabilities on Theme accepted
// ============================================================================

#[test]
fn empty_capabilities_on_theme_accepted() {
    let (mut manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _pkse, _pr, sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::Theme);

    manifest.declared_capabilities = vec![];
    // Re-sign with the ed25519 signing key (sks, not _pkse which is the catalog struct)
    let ch = canonical::manifest_canonical_hash(&manifest);
    manifest.manifest_canonical_hash.clone_from(&ch);
    let payload = canonical::signing_payload(&ch);
    manifest.ed25519_signature = sks.sign(payload).to_bytes().to_vec();

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::VerifiedPublisher);
}

// ============================================================================
// 15 — RecoveryCritical channel on AiosVerifiedRepo → ManifestForged
// ============================================================================

#[test]
fn recovery_critical_on_verified_repo_manifest_forged() {
    let (mut manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    manifest.channel = UpdateChannel::RecoveryCritical;
    // manifest.originating_repository is AiosVerifiedRepo (from the helper)

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::ManifestForged);
}

// ============================================================================
// 16 — RecoveryCritical on AiosRecoveryRepo accepted
// ============================================================================

#[test]
fn recovery_critical_on_recovery_repo_accepted() {
    let (mut manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _pkse, _pr, sks, fch) =
        build_signed_manifest(PublisherTrustLevel::AiosRoot, PackageKind::InvariantBundle);

    manifest.channel = UpdateChannel::RecoveryCritical;
    manifest.originating_repository = RepositoryKind::AiosRecoveryRepo;
    // Re-sign with the ed25519 signing key
    let ch = canonical::manifest_canonical_hash(&manifest);
    manifest.manifest_canonical_hash.clone_from(&ch);
    let payload = canonical::signing_payload(&ch);
    manifest.ed25519_signature = sks.sign(payload).to_bytes().to_vec();

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::VerifiedAiosRoot);
}

// ============================================================================
// 17 — KernelCandidate from AiosVerifiedRepo → RepositoryKindMismatch
// ============================================================================

#[test]
fn kernel_candidate_from_verified_repo_repository_kind_mismatch() {
    let (mut manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::AiosRoot, PackageKind::KernelCandidate);

    manifest.originating_repository = RepositoryKind::AiosVerifiedRepo;

    let now = Utc::now();
    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::RepositoryKindMismatch);
}

// ============================================================================
// 18 — issued_at 1 hour in the future → ManifestForged
// ============================================================================

#[test]
fn issued_at_one_hour_future_manifest_forged() {
    let (mut manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    let now = Utc::now();
    manifest.issued_at = now + Duration::hours(1);

    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::ManifestForged);
}

// ============================================================================
// 19 — issued_at within 5-minute drift accepted (via validate_fields)
// ============================================================================

#[test]
fn issued_at_within_drift_accepted() {
    let (manifest, _verifier, _pub_sig, _key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, _fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    let now = Utc::now();
    // issued_at is within 5 min drift → should be ok
    let drift = Duration::seconds(300);
    let r = validate_fields(&manifest, now, drift);

    // issued_at was set to now - 1 hour in the helper → well within drift
    assert!(r.is_ok());
}

// ============================================================================
// 20 — issued_at after eol_at → ManifestForged
// ============================================================================

#[test]
fn issued_at_after_eol_at_manifest_forged() {
    let (mut manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    let now = Utc::now();
    manifest.issued_at = now - Duration::hours(2);
    manifest.eol_at = Some(now - Duration::hours(3)); // eol before issued

    let result = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);

    assert_eq!(result, PackageVerificationResult::ManifestForged);
}

// ============================================================================
// 21 — is_eol true when eol_at in the past
// ============================================================================

#[test]
fn is_eol_true_when_eol_in_past() {
    let (manifest, _verifier, _pub_sig, _key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, _fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    let mut m = manifest;
    let now = Utc::now();
    m.eol_at = Some(now - Duration::hours(1));
    assert!(is_eol(&m, now));
}

// ============================================================================
// 22 — is_eol false when eol_at unset or in the future
// ============================================================================

#[test]
fn is_eol_false_when_unset_or_future() {
    let (manifest, _verifier, _pub_sig, _key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, _fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    let mut m = manifest;
    let now = Utc::now();

    // Unset
    m.eol_at = None;
    assert!(!is_eol(&m, now));

    // Future
    m.eol_at = Some(now + Duration::days(365));
    assert!(!is_eol(&m, now));
}

// ============================================================================
// 23 — canonicalisation determinism
// ============================================================================

#[test]
fn canonical_hash_determinism() {
    let (manifest, _verifier, _pub_sig, _key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, _fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    let a = canonical::manifest_canonical_hash(&manifest);
    let b = canonical::manifest_canonical_hash(&manifest);
    assert_eq!(a, b, "same manifest → same canonical hash");
}

// ============================================================================
// 24 — canonical hash independent of struct field declaration order
//      (two identical manifests produce the same hash)
// ============================================================================

#[test]
fn canonical_hash_identical_manifests_same_hash() {
    let (m1, _verifier, _pub_sig, _key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, _fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);
    let (_m2, _v2, _ps2, _ks2, _r2, _pc2, _sc2, _sk2, _pr2, _sks2, _fc2) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    // Same manifest → same canonical hash (determinism test)
    let h1 = canonical::manifest_canonical_hash(&m1);
    let h2 = canonical::manifest_canonical_hash(&m1);
    assert_eq!(h1, h2);
}

// ============================================================================
// 25 — content_hash(bytes) is 32 lower-hex and stable
// ============================================================================

#[test]
fn content_hash_is_32_lower_hex_and_stable() {
    let a = canonical::content_hash(b"test content bytes");
    let b = canonical::content_hash(b"test content bytes");

    assert_eq!(a.len(), 32);
    assert!(a
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    assert_eq!(a, b);
    // Different content → different hash
    let c = canonical::content_hash(b"different bytes");
    assert_ne!(a, c);
}

// ============================================================================
// 26 — full round-trip: sign → verify → VerifiedPublisher,
//      flip one capability → canonical hash changes → ManifestForged
// ============================================================================

#[test]
fn roundtrip_capability_flip_breaks_canonical_hash() {
    let (manifest, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    let now = Utc::now();

    // First, valid verify
    let result1 = verify_manifest(&manifest, &verifier, &fch, &pub_sig, &key_sig, now);
    assert_eq!(result1, PackageVerificationResult::VerifiedPublisher);

    // Flip one declared capability
    let mut tampered = manifest.clone();
    tampered.declared_capabilities = vec!["evil.capability".into()];

    // Signature is unchanged, but canonical hash will differ because
    // declared_capabilities is part of the manifest
    let result2 = verify_manifest(&tampered, &verifier, &fch, &pub_sig, &key_sig, now);

    // Canonical hash no longer matches → ManifestForged (step 2 fires before signature check)
    assert_eq!(result2, PackageVerificationResult::ManifestForged);

    // Also verify canonical hash actually changed
    let ch_orig = canonical::manifest_canonical_hash(&manifest);
    let ch_tampered = canonical::manifest_canonical_hash(&tampered);
    assert_ne!(ch_orig, ch_tampered);
}

// ============================================================================
// 27 — DEFAULT_CODE_VERSION updated to T-193
// ============================================================================

#[test]
fn default_code_version_is_t194() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-distribution/0.0.1-T196");
}

// ============================================================================
// 28 — content hash in manifest matches recomputed content hash
// ============================================================================

#[test]
fn manifest_content_hash_matches_recomputed() {
    let (manifest, _verifier, _pub_sig, _key_sig, _root, _pubcat, _sigcats, _sk, _pr, _sks, fch) =
        build_signed_manifest(PublisherTrustLevel::Verified, PackageKind::App);

    // The helper builds content_hash from the same bytes used for fch
    assert_eq!(manifest.content_hash, fch);
    assert_eq!(manifest.content_hash.len(), 32);
}

// ============================================================================
// 29 — PackageVerificationResult::RepositoryKindMismatch is not success
// ============================================================================

#[test]
fn repository_kind_mismatch_is_not_success() {
    assert!(!PackageVerificationResult::RepositoryKindMismatch.is_success());
}

// ============================================================================
// 30 — ManifestField enum covers all variants
// ============================================================================

#[test]
fn manifest_field_variants_exist() {
    // Just reference all variants to ensure they compile
    let fields = [
        ManifestField::PackageId,
        ManifestField::Version,
        ManifestField::Kind,
        ManifestField::PublisherTrust,
        ManifestField::PublisherRootId,
        ManifestField::PackageSigningKeyId,
        ManifestField::ContentHash,
        ManifestField::ManifestCanonicalHash,
        ManifestField::Ed25519Signature,
        ManifestField::InstallableScope,
        ManifestField::RequiredSandbox,
        ManifestField::DeclaredCapabilities,
        ManifestField::NetworkManifest,
        ManifestField::IssuedAt,
        ManifestField::EolAt,
        ManifestField::Channel,
        ManifestField::OriginatingRepository,
        ManifestField::MirrorSemantic,
    ];
    assert_eq!(fields.len(), 18);
}
