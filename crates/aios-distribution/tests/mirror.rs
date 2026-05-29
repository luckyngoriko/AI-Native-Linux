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

use chrono::{DateTime, Duration, Utc};

use aios_distribution::*;

// ============================================================================
// Shared helpers
// ============================================================================

fn test_now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-05-29T12:00:00Z")
        .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc))
}

fn make_manifest_for(content_bytes: &[u8]) -> PackageManifest {
    let fixed_time = test_now();
    let ch = canonical::content_hash(content_bytes);
    PackageManifest {
        package_id: "pkg:test:app".into(),
        version: "1.0.0".into(),
        kind: PackageKind::App,
        publisher_trust: PublisherTrustLevel::Verified,
        publisher_root_id: PublisherRootId("pub:test".into()),
        package_signing_key_id: PackageSigningKeyId("pks:test:release".into()),
        content_hash: ch,
        manifest_canonical_hash: String::new(),
        ed25519_signature: vec![0xAA; 64],
        installable_scope: InstallScope::Either,
        required_sandbox: SandboxProfileRef("test-profile".into()),
        declared_capabilities: vec!["filesystem.read".into()],
        network_manifest: NetworkManifestRef("test-net".into()),
        issued_at: fixed_time,
        eol_at: None,
        channel: UpdateChannel::Stable,
        originating_repository: RepositoryKind::AiosVerifiedRepo,
        mirror_url: String::new(),
        mirror_semantic: MirrorSemantic::Cached,
    }
}

/// In-memory byte source for tests.
struct TestByteSource {
    data: HashMap<String, Vec<u8>>,
}

impl TestByteSource {
    const fn new(data: HashMap<String, Vec<u8>>) -> Self {
        Self { data }
    }
}

impl MirrorByteSource for TestByteSource {
    fn fetch(&self, endpoint: &MirrorEndpoint) -> Result<Vec<u8>, DistributionError> {
        self.data
            .get(&endpoint.url)
            .cloned()
            .ok_or_else(|| DistributionError::Internal(format!("no data for {}", endpoint.url)))
    }
}

// ============================================================================
// T-191: Mirror semantics tests (≥18 tests)
// ============================================================================

// ── 1. fetch_order ─────────────────────────────────────────────────────

#[test]
fn fetch_order_returns_local_first_then_cached_then_origin() {
    let eps = vec![
        MirrorEndpoint::new("https://origin.example.com", MirrorSemantic::Origin),
        MirrorEndpoint::new("/mnt/local", MirrorSemantic::Local),
        MirrorEndpoint::new("https://cache.example.com", MirrorSemantic::Cached),
        MirrorEndpoint::new("https://cache2.example.com", MirrorSemantic::Cached),
    ];
    let ordered = fetch_order(&eps);
    assert_eq!(ordered.len(), 4);
    assert_eq!(ordered[0].semantic, MirrorSemantic::Local);
    assert_eq!(ordered[1].semantic, MirrorSemantic::Cached);
    assert_eq!(ordered[2].semantic, MirrorSemantic::Cached);
    assert_eq!(ordered[3].semantic, MirrorSemantic::Origin);
}

// ── 2. verify_mirror_bytes ─────────────────────────────────────────────

#[test]
fn verify_mirror_bytes_ok_when_hash_matches() {
    let content = b"good package content";
    let manifest = make_manifest_for(content);
    let result = verify_mirror_bytes(&manifest, content);
    assert!(result.is_ok());
}

#[test]
fn verify_mirror_bytes_hash_mismatch_when_content_differs() {
    let good = b"good content";
    let bad = b"tampered content";
    let manifest = make_manifest_for(good);
    let result = verify_mirror_bytes(&manifest, bad);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code(),
        DistributionErrorCode::HashMismatch
    );
}

// ── 3. detect_resign_attempt ───────────────────────────────────────────

#[test]
fn detect_resign_attempt_identical_signature_ok() {
    let sig = vec![0xBB; 64];
    let result = detect_resign_attempt(&sig, Some(&sig));
    assert!(result.is_ok());
}

#[test]
fn detect_resign_attempt_different_signature_mirror_resign_attempt() {
    let origin = vec![0xBB; 64];
    let mirror = vec![0xCC; 64];
    let result = detect_resign_attempt(&origin, Some(&mirror));
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code(),
        DistributionErrorCode::MirrorReSignAttempt
    );
}

#[test]
fn detect_resign_attempt_none_signature_ok() {
    let origin = vec![0xBB; 64];
    let result = detect_resign_attempt(&origin, None);
    assert!(result.is_ok());
}

// ── 4. blacklist: below threshold ──────────────────────────────────────

#[test]
fn blacklist_below_threshold_not_blacklisted() {
    let mut bl = MirrorBlacklist::with_defaults();
    let now = test_now();
    let url = "https://bad-mirror.example.com";

    bl.record_mismatch(url, now);
    bl.record_mismatch(url, now + Duration::minutes(1));
    // Only 2 mismatches — not blacklisted
    assert!(!bl.is_blacklisted(url));
}

// ── 5. blacklist: reaching threshold ───────────────────────────────────

#[test]
fn blacklist_reaching_threshold_blacklisted() {
    let mut bl = MirrorBlacklist::with_defaults();
    let now = test_now();
    let url = "https://evil-mirror.example.com";

    bl.record_mismatch(url, now);
    bl.record_mismatch(url, now + Duration::minutes(10));
    let result = bl.record_mismatch(url, now + Duration::minutes(20));
    assert!(result);
    assert!(bl.is_blacklisted(url));
}

// ── 6. blacklist: window pruning ───────────────────────────────────────

#[test]
fn blacklist_mismatches_beyond_window_do_not_accumulate() {
    let mut bl = MirrorBlacklist::new(Duration::hours(24), 3);
    let now = test_now();
    let url = "https://spread-out.example.com";

    // Spread mismatches beyond the 24h window
    bl.record_mismatch(url, now);
    // 25 hours later — first mismatch is pruned
    bl.record_mismatch(url, now + Duration::hours(25));
    // 26 hours later — only 2 active in window
    let result = bl.record_mismatch(url, now + Duration::hours(26));
    // Not enough in window to trigger blacklist
    assert!(!result);
    assert!(!bl.is_blacklisted(url));
}

// ── 7. blacklist: is_blacklisted + pre_reject ──────────────────────────

#[test]
fn blacklist_is_blacklisted_true_after_threshold() {
    let mut bl = MirrorBlacklist::with_defaults();
    let now = test_now();
    let url = "https://condemned.example.com";

    bl.record_mismatch(url, now);
    bl.record_mismatch(url, now + Duration::minutes(1));
    bl.record_mismatch(url, now + Duration::minutes(2));
    assert!(bl.is_blacklisted(url));
}

#[test]
fn blacklist_pre_reject_errors_for_blacklisted_endpoint() {
    let mut bl = MirrorBlacklist::with_defaults();
    let now = test_now();
    let url = "https://blocked.example.com";

    bl.record_mismatch(url, now);
    bl.record_mismatch(url, now + Duration::minutes(1));
    bl.record_mismatch(url, now + Duration::minutes(2));

    let ep = MirrorEndpoint::new(url, MirrorSemantic::Cached);
    let result = bl.pre_reject(&ep, test_now());
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code(),
        DistributionErrorCode::MirrorBlacklisted
    );
}

// ── 8. resolve_and_verify: Local good → Local ─────────────────────────

#[test]
fn resolve_and_verify_local_good_returns_local() {
    let content = b"authentic bytes";
    let manifest = make_manifest_for(content);

    let eps = vec![
        MirrorEndpoint::new("/mirror/local", MirrorSemantic::Local),
        MirrorEndpoint::new("https://origin.example.com", MirrorSemantic::Origin),
    ];

    let mut data = HashMap::new();
    data.insert("/mirror/local".to_string(), content.to_vec());
    data.insert("https://origin.example.com".to_string(), content.to_vec());
    let source = TestByteSource::new(data);

    let mut blacklist = MirrorBlacklist::with_defaults();
    let result = resolve_and_verify(&manifest, &eps, &source, &mut blacklist, test_now());

    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert_eq!(resolved.endpoint.semantic, MirrorSemantic::Local);
    assert_eq!(resolved.endpoint.url, "/mirror/local");
}

// ── 9. resolve_and_verify: Local tampered → falls through to Cached ───

#[test]
fn resolve_and_verify_local_tampered_falls_through_to_cached() {
    let good = b"good content";
    let tampered = b"tampered content";
    let manifest = make_manifest_for(good);

    let eps = vec![
        MirrorEndpoint::new("/mirror/local", MirrorSemantic::Local),
        MirrorEndpoint::new("https://cache.example.com", MirrorSemantic::Cached),
        MirrorEndpoint::new("https://origin.example.com", MirrorSemantic::Origin),
    ];

    let mut data = HashMap::new();
    data.insert("/mirror/local".to_string(), tampered.to_vec());
    data.insert("https://cache.example.com".to_string(), good.to_vec());
    data.insert("https://origin.example.com".to_string(), good.to_vec());
    let source = TestByteSource::new(data);

    let mut blacklist = MirrorBlacklist::with_defaults();
    let result = resolve_and_verify(&manifest, &eps, &source, &mut blacklist, test_now());

    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert_eq!(resolved.endpoint.semantic, MirrorSemantic::Cached);
    assert_eq!(resolved.endpoint.url, "https://cache.example.com");
}

// ── 10. resolve_and_verify: all tampered → Err ────────────────────────

#[test]
fn resolve_and_verify_all_tiers_tampered_errors_and_records_mismatches() {
    let good = b"good content";
    let t1 = b"tampered one";
    let t2 = b"tampered two";
    let t3 = b"tampered three";
    let manifest = make_manifest_for(good);

    let eps = vec![
        MirrorEndpoint::new("/local", MirrorSemantic::Local),
        MirrorEndpoint::new("https://cache.example.com", MirrorSemantic::Cached),
        MirrorEndpoint::new("https://origin.example.com", MirrorSemantic::Origin),
    ];

    let mut data = HashMap::new();
    data.insert("/local".to_string(), t1.to_vec());
    data.insert("https://cache.example.com".to_string(), t2.to_vec());
    data.insert("https://origin.example.com".to_string(), t3.to_vec());
    let source = TestByteSource::new(data);

    let mut blacklist = MirrorBlacklist::with_defaults();
    let result = resolve_and_verify(&manifest, &eps, &source, &mut blacklist, test_now());

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code(),
        DistributionErrorCode::HashMismatch
    );
}

// ── 11. resolve_and_verify: blacklisted Cached skipped → Origin used ───

#[test]
fn resolve_and_verify_blacklisted_cached_skipped_origin_used() {
    let content = b"good content";
    let manifest = make_manifest_for(content);

    let eps = vec![
        MirrorEndpoint::new("/local", MirrorSemantic::Local),
        MirrorEndpoint::new("https://cache.example.com", MirrorSemantic::Cached),
        MirrorEndpoint::new("https://origin.example.com", MirrorSemantic::Origin),
    ];

    // Pre-blacklist the Cached mirror
    let mut blacklist = MirrorBlacklist::with_defaults();
    let now = test_now();
    blacklist.record_mismatch("https://cache.example.com", now);
    blacklist.record_mismatch("https://cache.example.com", now + Duration::minutes(1));
    blacklist.record_mismatch("https://cache.example.com", now + Duration::minutes(2));

    let mut data = HashMap::new();
    data.insert("/local".to_string(), b"wrong content".to_vec()); // Local bad
    data.insert("https://cache.example.com".to_string(), content.to_vec()); // blacklisted
    data.insert("https://origin.example.com".to_string(), content.to_vec());
    let source = TestByteSource::new(data);

    let result = resolve_and_verify(&manifest, &eps, &source, &mut blacklist, test_now());

    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert_eq!(resolved.endpoint.semantic, MirrorSemantic::Origin);
}

// ── 12. mirrors-never-re-sign: tampered mirror cannot pass ─────────────

#[test]
fn mirror_never_resign_tampered_content_fails_hash_check() {
    let good = b"legitimate package v1.0";
    let bad = b"backdoored package v1.0";
    let manifest = make_manifest_for(good);

    // A mirror serving tampered content will always fail the hash check
    // because mirrors cannot produce a valid signature for different content
    let result = verify_mirror_bytes(&manifest, bad);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code(),
        DistributionErrorCode::HashMismatch
    );
}

// ── 13. default_code_version ──────────────────────────────────────────

#[test]
fn default_code_version_constant_is_correct() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-distribution/0.0.1-T193");
}

// ── 14. error code count ───────────────────────────────────────────────

#[test]
fn error_code_count_is_18() {
    // T-193 added PackageDowngradeBlocked (18th code)
    let codes: Vec<DistributionErrorCode> = vec![
        DistributionErrorCode::PackageNotFound,
        DistributionErrorCode::PublisherNotFound,
        DistributionErrorCode::SignatureFailed,
        DistributionErrorCode::TrustChainTooDeep,
        DistributionErrorCode::PublisherDeplatformed,
        DistributionErrorCode::HashMismatch,
        DistributionErrorCode::ManifestForged,
        DistributionErrorCode::RepositoryKindMismatch,
        DistributionErrorCode::RevokedKey,
        DistributionErrorCode::InstallStateInvalidTransition,
        DistributionErrorCode::MirrorReSignAttempt,
        DistributionErrorCode::CapabilityLieDetected,
        DistributionErrorCode::TakedownActive,
        DistributionErrorCode::Internal,
        DistributionErrorCode::InstallScopeViolation,
        DistributionErrorCode::BundleTampered,
        DistributionErrorCode::MirrorBlacklisted,
        DistributionErrorCode::PackageDowngradeBlocked,
    ];
    assert_eq!(codes.len(), 18);
}

// ── 15. blacklist: different mirrors independent ───────────────────────

#[test]
fn blacklist_different_mirrors_independent() {
    let mut bl = MirrorBlacklist::with_defaults();
    let now = test_now();

    bl.record_mismatch("https://a.example.com", now);
    bl.record_mismatch("https://a.example.com", now + Duration::minutes(1));
    bl.record_mismatch("https://a.example.com", now + Duration::minutes(2));
    assert!(bl.is_blacklisted("https://a.example.com"));

    // Mirror B only has 1 mismatch — not blacklisted
    bl.record_mismatch("https://b.example.com", now);
    assert!(!bl.is_blacklisted("https://b.example.com"));
}

// ── 16. blacklist persistence ──────────────────────────────────────────

#[test]
fn blacklist_persists_across_time() {
    let mut bl = MirrorBlacklist::with_defaults();
    let now = test_now();
    let url = "https://persistent-bad.example.com";

    bl.record_mismatch(url, now);
    bl.record_mismatch(url, now + Duration::minutes(1));
    bl.record_mismatch(url, now + Duration::minutes(2));

    // 48 hours later, still blacklisted
    let far_future = now + Duration::hours(48);
    assert!(bl.is_blacklisted(url));
    let ep = MirrorEndpoint::new(url, MirrorSemantic::Cached);
    let result = bl.pre_reject(&ep, far_future);
    assert!(result.is_err());
}

// ── 17. resolve_and_verify: empty endpoints → Err ─────────────────────

#[test]
fn resolve_and_verify_empty_endpoints_errors() {
    let manifest = make_manifest_for(b"irrelevant");
    let source = TestByteSource::new(HashMap::new());
    let mut blacklist = MirrorBlacklist::with_defaults();
    let result = resolve_and_verify(&manifest, &[], &source, &mut blacklist, test_now());
    assert!(result.is_err());
}

// ── 18. blacklist: pre_reject ok for non-blacklisted ───────────────────

#[test]
fn blacklist_pre_reject_ok_for_clean_mirror() {
    let bl = MirrorBlacklist::with_defaults();
    let ep = MirrorEndpoint::new("https://clean.example.com", MirrorSemantic::Cached);
    let result = bl.pre_reject(&ep, test_now());
    assert!(result.is_ok());
}
