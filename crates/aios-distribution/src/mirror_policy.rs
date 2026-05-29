//! Mirror endpoint ordering and byte verification per S11.1 §3.8 + §6.1 + §6.5.
//!
//! # Mirror discipline (the cardinal rule)
//!
//! **Mirrors NEVER re-sign packages.** They serve the same signed bytes verbatim
//! or fail. Mirror tampering is detected by the host-side content-hash check
//! before unpacking (§5 step 5, expanded in §6.5).
//!
//! # Fetch order (§6.1)
//!
//! The host attempts fetch tiers in priority order: `Local → Cached → Origin`.
//! Failure of one tier moves to the next. Within a tier, endpoints are returned
//! in stable (insertion) order.

use crate::canonical::content_hash;
use crate::error::DistributionError;
use crate::manifest::PackageManifest;
use crate::mirror::MirrorSemantic;

/// A mirror endpoint with its semantic classification.
///
/// Every mirror the host knows about is represented by a URL and a
/// [`MirrorSemantic`] that controls where it sits in the fetch hierarchy
/// and which tampering rules apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorEndpoint {
    /// The fetch URL for this endpoint.
    pub url: String,
    /// Semantic classification (Origin / Cached / Local) per S11.1 §3.8.
    pub semantic: MirrorSemantic,
}

impl MirrorEndpoint {
    /// Creates a new `MirrorEndpoint`.
    #[must_use]
    pub fn new(url: impl Into<String>, semantic: MirrorSemantic) -> Self {
        Self {
            url: url.into(),
            semantic,
        }
    }
}

/// Returns endpoints ordered by fetch priority: **Local → Cached → Origin**.
///
/// Per S11.1 §6.1, the host attempts `LOCAL` first, then `CACHED`, then
/// `ORIGIN`. Within a tier, endpoints appear in the order they were provided
/// (stable sort preserves insertion order for same-tier entries).
///
/// # Example
///
/// ```ignore
/// use aios_distribution::mirror_policy::{fetch_order, MirrorEndpoint};
/// use aios_distribution::mirror::MirrorSemantic;
///
/// let endpoints = vec![
///     MirrorEndpoint::new("https://origin.example.com", MirrorSemantic::Origin),
///     MirrorEndpoint::new("/mnt/offline-mirror", MirrorSemantic::Local),
///     MirrorEndpoint::new("https://mirror.example.com", MirrorSemantic::Cached),
/// ];
/// let ordered = fetch_order(&endpoints);
/// assert_eq!(ordered[0].semantic, MirrorSemantic::Local);
/// assert_eq!(ordered[1].semantic, MirrorSemantic::Cached);
/// assert_eq!(ordered[2].semantic, MirrorSemantic::Origin);
/// ```
#[must_use]
pub fn fetch_order(endpoints: &[MirrorEndpoint]) -> Vec<MirrorEndpoint> {
    // Partition by tier to preserve stable ordering within each tier,
    // then concatenate in priority order.
    let locals: Vec<&MirrorEndpoint> = endpoints
        .iter()
        .filter(|e| e.semantic == MirrorSemantic::Local)
        .collect();
    let cached: Vec<&MirrorEndpoint> = endpoints
        .iter()
        .filter(|e| e.semantic == MirrorSemantic::Cached)
        .collect();
    let origins: Vec<&MirrorEndpoint> = endpoints
        .iter()
        .filter(|e| e.semantic == MirrorSemantic::Origin)
        .collect();

    let mut ordered = Vec::with_capacity(endpoints.len());
    for ep in locals {
        ordered.push((*ep).clone());
    }
    for ep in cached {
        ordered.push((*ep).clone());
    }
    for ep in origins {
        ordered.push((*ep).clone());
    }
    ordered
}

/// Verifies that fetched package bytes match the manifest's content hash.
///
/// Per S11.1 §6.5 step 5: compute `BLAKE3(content)` over the fetched bytes
/// and compare against `manifest.content_hash`. On mismatch, returns
/// `DistributionErrorCode::HashMismatch`.
///
/// **Mirrors never re-sign** — they cannot pass this check on tampered
/// content because the manifest (including `content_hash`) is signed by
/// the **publisher** using their package signing key. A mirror cannot
/// produce a valid alternative signature, and the host never consults
/// a mirror-provided signature anyway.
///
/// # Returns
///
/// - `Ok(())` when BLAKE3 bytes match the manifest hash.
/// - `Err(DistributionError::HashMismatch(...))` when they do not.
///
/// # Errors
///
/// Returns [`DistributionError::HashMismatch`] if the computed hash
/// differs from `manifest.content_hash`.
pub fn verify_mirror_bytes(
    manifest: &PackageManifest,
    fetched_bytes: &[u8],
) -> Result<(), DistributionError> {
    let computed = content_hash(fetched_bytes);
    if computed == manifest.content_hash {
        Ok(())
    } else {
        Err(DistributionError::HashMismatch(format!(
            "content hash mismatch: expected {}, computed {}",
            manifest.content_hash, computed
        )))
    }
}

/// Detects whether a mirror attempted to re-sign a package.
///
/// Per S11.1 §3.8 and §10, **mirrors NEVER re-sign**. A mirror presenting
/// a signature that differs from the origin manifest's `ed25519_signature`
/// is an explicit protocol violation.
///
/// # Parameters
///
/// * `origin_manifest_signature` — the Ed25519 signature from the origin
///   publisher's signed manifest (the authoritative signature).
/// * `mirror_presented_signature` — the signature the mirror presented,
///   if any. A `None` value means the mirror served only bytes (no
///   signature claim), which is fine — mirrors are supposed to serve
///   verbatim bytes.
///
/// # Returns
///
/// - `Ok(())` when the mirror presents no signature (`None`) or when the
///   mirror's signature matches the origin manifest signature identically.
/// - `Err(DistributionError::MirrorReSignAttempt(...))` when the mirror
///   presents a different signature.
///
/// # Errors
///
/// Returns [`DistributionError::MirrorReSignAttempt`] when the mirror
/// presents a signature that differs from the origin's.
pub fn detect_resign_attempt(
    origin_manifest_signature: &[u8],
    mirror_presented_signature: Option<&[u8]>,
) -> Result<(), DistributionError> {
    match mirror_presented_signature {
        None => Ok(()),
        Some(sig) if sig == origin_manifest_signature => Ok(()),
        Some(_different) => Err(DistributionError::MirrorReSignAttempt(
            "mirror presented a signature that differs from the origin manifest signature; \
             mirrors never re-sign (S11.1 §3.8 / §10)"
                .into(),
        )),
    }
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
    clippy::float_cmp
)]
mod tests {
    use super::*;
    use crate::canonical::content_hash;
    use crate::error::DistributionErrorCode;
    use crate::ids::{PackageSigningKeyId, PublisherRootId};
    use crate::manifest::{NetworkManifestRef, SandboxProfileRef};
    use crate::package_kind::{InstallScope, PackageKind};
    use crate::repository::{RepositoryKind, UpdateChannel};
    use crate::trust::PublisherTrustLevel;
    use chrono::{DateTime, Utc};

    fn make_test_manifest() -> PackageManifest {
        let fixed_time = DateTime::parse_from_rfc3339("2026-05-29T12:00:00Z")
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));
        let content_bytes = b"test package content";
        PackageManifest {
            package_id: "pkg:testvendor:testapp".into(),
            version: "1.0.0".into(),
            kind: PackageKind::App,
            publisher_trust: PublisherTrustLevel::Verified,
            publisher_root_id: PublisherRootId("pub:testvendor".into()),
            package_signing_key_id: PackageSigningKeyId("pks:testvendor:release".into()),
            content_hash: content_hash(content_bytes),
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
            mirror_url: "https://mirror.example.com".into(),
            mirror_semantic: MirrorSemantic::Cached,
        }
    }

    // ── fetch_order ─────────────────────────────────────────────────────

    #[test]
    fn fetch_order_returns_local_first_then_cached_then_origin() {
        let eps = vec![
            MirrorEndpoint::new("https://origin.example.com", MirrorSemantic::Origin),
            MirrorEndpoint::new("/mnt/local-mirror", MirrorSemantic::Local),
            MirrorEndpoint::new("https://cached.example.com", MirrorSemantic::Cached),
            MirrorEndpoint::new("https://cached2.example.com", MirrorSemantic::Cached),
        ];
        let ordered = fetch_order(&eps);
        assert_eq!(ordered.len(), 4);
        assert_eq!(ordered[0].semantic, MirrorSemantic::Local);
        assert_eq!(ordered[0].url, "/mnt/local-mirror");
        assert_eq!(ordered[1].semantic, MirrorSemantic::Cached);
        assert_eq!(ordered[2].semantic, MirrorSemantic::Cached);
        assert_eq!(ordered[3].semantic, MirrorSemantic::Origin);
    }

    #[test]
    fn fetch_order_stable_within_tier() {
        // Two Cached mirrors — insertion order preserved
        let eps = vec![
            MirrorEndpoint::new("https://cache-b.example.com", MirrorSemantic::Cached),
            MirrorEndpoint::new("https://cache-a.example.com", MirrorSemantic::Cached),
        ];
        let ordered = fetch_order(&eps);
        assert_eq!(ordered[0].url, "https://cache-b.example.com");
        assert_eq!(ordered[1].url, "https://cache-a.example.com");
    }

    #[test]
    fn fetch_order_empty_input() {
        let ordered = fetch_order(&[]);
        assert!(ordered.is_empty());
    }

    // ── verify_mirror_bytes ─────────────────────────────────────────────

    #[test]
    fn verify_mirror_bytes_ok_when_hash_matches() {
        let content_bytes = b"test package content";
        let manifest = make_test_manifest();
        let result = verify_mirror_bytes(&manifest, content_bytes);
        assert!(result.is_ok());
    }

    #[test]
    fn verify_mirror_bytes_hash_mismatch_when_content_differs() {
        let manifest = make_test_manifest();
        let tampered_bytes = b"tampered content";
        let result = verify_mirror_bytes(&manifest, tampered_bytes);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            DistributionErrorCode::HashMismatch
        );
    }

    #[test]
    fn verify_mirror_bytes_hash_mismatch_even_on_single_byte_change() {
        let manifest = make_test_manifest();
        let mut content = b"test package content".to_vec();
        content[0] ^= 0x01; // flip one bit
        let result = verify_mirror_bytes(&manifest, &content);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            DistributionErrorCode::HashMismatch
        );
    }

    // ── detect_resign_attempt ───────────────────────────────────────────

    #[test]
    fn detect_resign_attempt_identical_sig_ok() {
        let origin_sig = vec![0xBB; 64];
        let result = detect_resign_attempt(&origin_sig, Some(&origin_sig));
        assert!(result.is_ok());
    }

    #[test]
    fn detect_resign_attempt_different_sig_mirror_resign_attempt() {
        let origin_sig = vec![0xBB; 64];
        let mirror_sig = vec![0xCC; 64];
        let result = detect_resign_attempt(&origin_sig, Some(&mirror_sig));
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            DistributionErrorCode::MirrorReSignAttempt
        );
    }

    #[test]
    fn detect_resign_attempt_none_is_ok() {
        let origin_sig = vec![0xBB; 64];
        let result = detect_resign_attempt(&origin_sig, None);
        assert!(result.is_ok());
    }

    #[test]
    fn detect_resign_attempt_same_bytes_different_vec_ok() {
        // Identical content, different allocations — should still pass
        let origin_sig = vec![0xDD; 64];
        let mirror_sig = vec![0xDD; 64];
        let result = detect_resign_attempt(&origin_sig, Some(&mirror_sig));
        assert!(result.is_ok());
    }
}
