//! Mirror fetch resolution tying together the fetch-order, byte-verification,
//! and auto-blacklist subsystems per S11.1 §3.8 + §6.1 + §6.5.
//!
//! This module is the **host-side resolver** that walks the tier order,
//! skips blacklisted mirrors, fetches bytes from each candidate, verifies
//! the content hash, records mismatches, and falls through tiers on failure.
//!
//! All operations are pure in-memory — no real network IO. Byte sources are
//! injected via the [`MirrorByteSource`] trait so tests supply arbitrary bytes.

use crate::error::DistributionError;
use crate::manifest::PackageManifest;
use crate::mirror::MirrorSemantic;
use crate::mirror_blacklist::MirrorBlacklist;
use crate::mirror_policy::{fetch_order, verify_mirror_bytes, MirrorEndpoint};
use chrono::{DateTime, Utc};

/// A byte source that resolves a [`MirrorEndpoint`] to raw package content.
///
/// This trait abstracts the actual fetch transport (HTTP, filesystem, cache).
/// Implementations are responsible for obtaining the bytes; the resolver
/// only cares about the result.
///
/// # Implementations
///
/// - **Test**: a closure or mock that returns pre-canned bytes.
/// - **Production (T-195+)**: a gRPC or HTTP-backed source with retry and
///   budget enforcement per §6.1.
pub trait MirrorByteSource {
    /// Fetch the bytes for a package from this endpoint.
    ///
    /// # Errors
    ///
    /// Returns [`DistributionError`] on any transport-level failure.
    fn fetch(&self, endpoint: &MirrorEndpoint) -> Result<Vec<u8>, DistributionError>;
}

/// The outcome of a `resolve_and_verify` call.
///
/// Contains the serving endpoint and the verified bytes that passed the
/// content-hash check against the manifest.
#[derive(Debug, Clone)]
pub struct ResolvedBytes {
    /// The endpoint that successfully served the verified bytes.
    pub endpoint: MirrorEndpoint,
    /// The verified package content (content hash matched the manifest).
    pub bytes: Vec<u8>,
}

/// Resolves a package from the available endpoint hierarchy and verifies the
/// content hash against the manifest.
///
/// Per S11.1 §6.1 + §6.5:
///
/// 1. Compute fetch order (`Local → Cached → Origin`).
/// 2. Iterate endpoints in order:
///    a. Skip if blacklisted ([`MirrorBlacklist::pre_reject`]).
///    b. Fetch bytes from the source.
///    c. Verify the content hash ([`verify_mirror_bytes`]).
///    d. On success → return the serving endpoint + verified bytes.
///    e. On `HashMismatch` from a Cached/Local mirror, record the
///    mismatch via [`MirrorBlacklist::record_mismatch`] and try the
///    next tier.
///    f. On any other error → try the next tier.
/// 3. If all tiers fail, return the last error encountered.
///
/// # Parameters
///
/// * `manifest` — the signed package manifest with the authoritative `content_hash`.
/// * `endpoints` — the full list of known endpoints (all three tiers).
/// * `source` — a [`MirrorByteSource`] that yields bytes for each endpoint.
/// * `blacklist` — the mutable blacklist tracker; mismatches are recorded here.
/// * `now` — the current host time for blacklist window pruning.
///
/// # Returns
///
/// * `Ok(ResolvedBytes)` with the serving endpoint and verified content.
/// * `Err(...)` with the last error encountered after exhausting all tiers.
///
/// # Errors
///
/// Returns the **last** error after all tiers are exhausted. If a mirror
/// serves tampered bytes, its mismatch is recorded but the resolver continues
/// to the next tier rather than failing immediately.
pub fn resolve_and_verify(
    manifest: &PackageManifest,
    endpoints: &[MirrorEndpoint],
    source: &dyn MirrorByteSource,
    blacklist: &mut MirrorBlacklist,
    now: DateTime<Utc>,
) -> Result<ResolvedBytes, DistributionError> {
    let ordered = fetch_order(endpoints);

    if ordered.is_empty() {
        return Err(DistributionError::Internal(
            "resolve_and_verify: no endpoints provided".into(),
        ));
    }

    let mut last_error: Option<DistributionError> = None;

    for ep in &ordered {
        // Step 2a: skip blacklisted mirrors
        if let Err(e) = blacklist.pre_reject(ep, now) {
            last_error = Some(e);
            continue;
        }

        // Step 2b: fetch bytes
        let bytes = match source.fetch(ep) {
            Ok(b) => b,
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        };

        // Step 2c: verify content hash
        match verify_mirror_bytes(manifest, &bytes) {
            Ok(()) => {
                return Ok(ResolvedBytes {
                    endpoint: ep.clone(),
                    bytes,
                });
            }
            Err(e) => {
                // Step 2e: on HashMismatch from a Cached or Local mirror,
                // record the mismatch so the blacklist can act.
                // Origin (publisher's authoritative server) is NOT a mirror
                // — its mismatches are treated as publisher errors, not mirror
                // tampering.
                if e.code() == crate::error::DistributionErrorCode::HashMismatch
                    && ep.semantic != MirrorSemantic::Origin
                {
                    blacklist.record_mismatch(&ep.url, now);
                }
                last_error = Some(e);
                // Continue to next tier
            }
        }
    }

    // Step 3: all tiers exhausted — return the last error
    Err(last_error.unwrap_or_else(|| {
        DistributionError::Internal(
            "resolve_and_verify: all tiers exhausted with no error captured".into(),
        )
    }))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::needless_collect,
    clippy::format_collect,
    clippy::too_many_arguments,
    clippy::float_cmp,
    clippy::use_self
)]
mod tests {
    use super::*;
    use crate::canonical::content_hash;
    use crate::error::DistributionErrorCode;
    use crate::ids::{PackageSigningKeyId, PublisherRootId};
    use crate::manifest::{NetworkManifestRef, SandboxProfileRef};
    use crate::mirror::MirrorSemantic;
    use crate::mirror_blacklist::MirrorBlacklist;
    use crate::package_kind::{InstallScope, PackageKind};
    use crate::repository::{RepositoryKind, UpdateChannel};
    use crate::trust::PublisherTrustLevel;
    use chrono::{DateTime, Duration};
    use std::collections::HashMap;

    /// In-memory byte source for tests: maps endpoint URL → bytes.
    struct InMemoryByteSource {
        data: HashMap<String, Vec<u8>>,
    }

    impl InMemoryByteSource {
        fn new(data: HashMap<String, Vec<u8>>) -> Self {
            Self { data }
        }
    }

    impl MirrorByteSource for InMemoryByteSource {
        fn fetch(&self, endpoint: &MirrorEndpoint) -> Result<Vec<u8>, DistributionError> {
            self.data.get(&endpoint.url).cloned().ok_or_else(|| {
                DistributionError::Internal(format!("no bytes registered for {}", endpoint.url))
            })
        }
    }

    fn test_now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-29T12:00:00Z")
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc))
    }

    fn make_manifest(content: &[u8]) -> PackageManifest {
        let fixed_time = test_now();
        PackageManifest {
            package_id: "pkg:test:app".into(),
            version: "1.0.0".into(),
            kind: PackageKind::App,
            publisher_trust: PublisherTrustLevel::Verified,
            publisher_root_id: PublisherRootId("pub:test".into()),
            package_signing_key_id: PackageSigningKeyId("pks:test:release".into()),
            content_hash: content_hash(content),
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

    // ── resolve_and_verify: happy paths ────────────────────────────────

    #[test]
    fn resolve_and_verify_local_serves_good_bytes_returns_local() {
        let content = b"good content";
        let manifest = make_manifest(content);

        let eps = vec![
            MirrorEndpoint::new("/mnt/local", MirrorSemantic::Local),
            MirrorEndpoint::new("https://origin.example.com", MirrorSemantic::Origin),
        ];

        let mut data = HashMap::new();
        data.insert("/mnt/local".to_string(), content.to_vec());
        data.insert("https://origin.example.com".to_string(), content.to_vec());
        let source = InMemoryByteSource::new(data);

        let mut blacklist = MirrorBlacklist::with_defaults();
        let result = resolve_and_verify(&manifest, &eps, &source, &mut blacklist, test_now());

        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.endpoint.semantic, MirrorSemantic::Local);
        assert_eq!(resolved.endpoint.url, "/mnt/local");
        assert_eq!(resolved.bytes, content);
    }

    #[test]
    fn resolve_and_verify_local_tampered_falls_through_to_cached_good() {
        let good = b"good content";
        let tampered = b"tampered!!!";
        let manifest = make_manifest(good);

        let eps = vec![
            MirrorEndpoint::new("/mnt/local", MirrorSemantic::Local),
            MirrorEndpoint::new("https://cache.example.com", MirrorSemantic::Cached),
            MirrorEndpoint::new("https://origin.example.com", MirrorSemantic::Origin),
        ];

        let mut data = HashMap::new();
        data.insert("/mnt/local".to_string(), tampered.to_vec());
        data.insert("https://cache.example.com".to_string(), good.to_vec());
        data.insert("https://origin.example.com".to_string(), good.to_vec());
        let source = InMemoryByteSource::new(data);

        let mut blacklist = MirrorBlacklist::with_defaults();
        let result = resolve_and_verify(&manifest, &eps, &source, &mut blacklist, test_now());

        assert!(result.is_ok());
        let resolved = result.unwrap();
        // Should have fallen through from Local(tampered) to Cached(good)
        assert_eq!(resolved.endpoint.semantic, MirrorSemantic::Cached);
        assert_eq!(resolved.endpoint.url, "https://cache.example.com");

        // Local should have recorded a mismatch
        assert_eq!(blacklist.mismatch_count("/mnt/local"), 1);
    }

    #[test]
    fn resolve_and_verify_all_tiers_tampered_returns_err() {
        let good = b"good content";
        let tampered_a = b"tampered aaa";
        let tampered_b = b"tampered bbb";
        let tampered_c = b"tampered ccc";
        let manifest = make_manifest(good);

        let eps = vec![
            MirrorEndpoint::new("/mnt/local", MirrorSemantic::Local),
            MirrorEndpoint::new("https://cache.example.com", MirrorSemantic::Cached),
            MirrorEndpoint::new("https://origin.example.com", MirrorSemantic::Origin),
        ];

        let mut data = HashMap::new();
        data.insert("/mnt/local".to_string(), tampered_a.to_vec());
        data.insert("https://cache.example.com".to_string(), tampered_b.to_vec());
        data.insert(
            "https://origin.example.com".to_string(),
            tampered_c.to_vec(),
        );
        let source = InMemoryByteSource::new(data);

        let mut blacklist = MirrorBlacklist::with_defaults();
        let result = resolve_and_verify(&manifest, &eps, &source, &mut blacklist, test_now());

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            DistributionErrorCode::HashMismatch
        );

        // Each mirror got a mismatch recorded (Origin is NOT a mirror)
        assert_eq!(blacklist.mismatch_count("/mnt/local"), 1);
        assert_eq!(blacklist.mismatch_count("https://cache.example.com"), 1);
        // Origin is NOT a mirror — mismatches from the publisher's
        // authoritative server are NOT recorded in the blacklist
        assert_eq!(blacklist.mismatch_count("https://origin.example.com"), 0);
    }

    #[test]
    fn resolve_and_verify_blacklisted_cached_skipped_origin_used() {
        let good = b"good content";
        let manifest = make_manifest(good);

        let eps = vec![
            MirrorEndpoint::new("/mnt/local", MirrorSemantic::Local),
            MirrorEndpoint::new("https://cache.example.com", MirrorSemantic::Cached),
            MirrorEndpoint::new("https://origin.example.com", MirrorSemantic::Origin),
        ];

        // Pre-blacklist the Cached mirror
        let mut blacklist = MirrorBlacklist::with_defaults();
        let now = test_now();
        blacklist.record_mismatch("https://cache.example.com", now);
        blacklist.record_mismatch("https://cache.example.com", now + Duration::minutes(1));
        blacklist.record_mismatch("https://cache.example.com", now + Duration::minutes(2));
        assert!(blacklist.is_blacklisted("https://cache.example.com"));

        let mut data = HashMap::new();
        data.insert("/mnt/local".to_string(), b"tampered".to_vec()); // Local tampered
        data.insert("https://cache.example.com".to_string(), good.to_vec()); // Blacklisted anyway
        data.insert("https://origin.example.com".to_string(), good.to_vec());
        let source = InMemoryByteSource::new(data);

        let result = resolve_and_verify(&manifest, &eps, &source, &mut blacklist, test_now());

        // Should skip the blacklisted Cached mirror and go to Origin
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.endpoint.semantic, MirrorSemantic::Origin);
        assert_eq!(resolved.endpoint.url, "https://origin.example.com");
    }

    #[test]
    fn resolve_and_verify_mirror_tampered_cannot_pass_verify() {
        // This test encodes the cardinal "mirrors never re-sign" rule:
        // a tampered mirror's bytes will always fail verify_mirror_bytes.
        let good = b"authentic package content v1.0";
        let tampered = b"ev il pa yl oa d!!";
        let manifest = make_manifest(good);

        let result = verify_mirror_bytes(&manifest, tampered);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            DistributionErrorCode::HashMismatch
        );
    }

    // ── resolve_and_verify: edge cases ─────────────────────────────────

    #[test]
    fn resolve_and_verify_single_endpoint_good_returns_it() {
        let content = b"single source";
        let manifest = make_manifest(content);

        let eps = vec![MirrorEndpoint::new(
            "https://only.example.com",
            MirrorSemantic::Origin,
        )];

        let mut data = HashMap::new();
        data.insert("https://only.example.com".to_string(), content.to_vec());
        let source = InMemoryByteSource::new(data);

        let mut blacklist = MirrorBlacklist::with_defaults();
        let result = resolve_and_verify(&manifest, &eps, &source, &mut blacklist, test_now());

        assert!(result.is_ok());
    }

    #[test]
    fn resolve_and_verify_empty_endpoints_errors() {
        let manifest = make_manifest(b"irrelevant");
        let source = InMemoryByteSource::new(HashMap::new());
        let mut blacklist = MirrorBlacklist::with_defaults();
        let result = resolve_and_verify(&manifest, &[], &source, &mut blacklist, test_now());
        assert!(result.is_err());
    }
}
