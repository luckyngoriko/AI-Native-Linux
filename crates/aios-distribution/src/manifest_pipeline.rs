//! Manifest field validation (§5.1) and signature verification pipeline.
//!
//! This module implements the two-step verification surface:
//!
//! 1. **Field validation** via [`validate_fields`] — walks every field in the
//!    [`PackageManifest`] per the §5.1 table, returning a
//!    `(ManifestField, PackageVerificationResult)` tuple on the first failure.
//! 2. **Signature + chain verification** via [`verify_manifest`] — recomputes the
//!    canonical hash, checks the fetched content hash, then delegates to the
//!    T-188 [`TrustChainVerifier`] for Ed25519 signature verification and trust
//!    chain walking.
//!
//! [`is_eol`] is a predicate consumed by T-190 to auto-quarantine packages whose
//! `eol_at` has passed.

use chrono::{DateTime, Duration, Utc};

use crate::canonical;
use crate::install_state::PackageVerificationResult;
use crate::manifest::PackageManifest;
use crate::package_kind::{InstallScope, PackageKind};
use crate::repository::{RepositoryKind, UpdateChannel};
use crate::trust_chain::SignedPayload;
use crate::verifier::TrustChainVerifier;

// ---------------------------------------------------------------------------
// ManifestField — closed field identifier for validation error reporting
// ---------------------------------------------------------------------------

/// Identifies which [`PackageManifest`] field failed validation.
///
/// Each variant maps to the field validated by the §5.1 table. The closed
/// set covers every field that `validate_fields` inspects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestField {
    /// `package_id` — regex + vendor cross-check.
    PackageId,
    /// `version` — strict semver.
    Version,
    /// `kind` — closed enum value (guaranteed by the Rust type system).
    Kind,
    /// `publisher_trust` — catalog cross-check (deferred to verifier).
    PublisherTrust,
    /// `publisher_root_id` — regex `^pub:[a-z0-9-]{1,64}$`.
    PublisherRootId,
    /// `package_signing_key_id` — regex `^pks:[a-z0-9-]{1,64}:[a-z0-9-]{1,64}$`.
    PackageSigningKeyId,
    /// `content_hash` — 32-char lowercase hex.
    ContentHash,
    /// `manifest_canonical_hash` — 32-char lowercase hex.
    ManifestCanonicalHash,
    /// `ed25519_signature` — 64-byte Ed25519 signature.
    Ed25519Signature,
    /// `installable_scope` — ∈ \{[`SystemOnly`](InstallScope::SystemOnly),
    /// [`GroupScoped`](InstallScope::GroupScoped), [`Either`](InstallScope::Either)\}.
    InstallableScope,
    /// `required_sandbox` — validated by T-197.
    RequiredSandbox,
    /// `declared_capabilities` — empty only for Theme.
    DeclaredCapabilities,
    /// `network_manifest` — validated by T-197.
    NetworkManifest,
    /// `issued_at` — must precede `now + max_future_drift` and precede `eol_at`.
    IssuedAt,
    /// `eol_at` — optional; no standalone validation.
    EolAt,
    /// `channel` — `RecoveryCritical` only on `AiosRecoveryRepo`.
    Channel,
    /// `originating_repository` — cross-checked against package kind.
    OriginatingRepository,
    /// `mirror_semantic` — Origin permitted, deeper checks T-191.
    MirrorSemantic,
}

// ---------------------------------------------------------------------------
// §5.1 field validation
// ---------------------------------------------------------------------------

/// Validates every field in `m` per the §5.1 table.
///
/// Returns `Ok(())` if all fields pass, or `Err((ManifestField,
/// PackageVerificationResult))` for the first field that fails.
///
/// # Parameters
///
/// - `now` — current host time for `issued_at` drift checks.
/// - `max_future_drift` — maximum allowed future drift for `issued_at`
///   (default 5 minutes per spec).
///
/// # Validation rules (S11.1 §5.1)
///
/// | Field                    | Rule                                                    | Failure mode         |
/// |--------------------------|---------------------------------------------------------|----------------------|
/// | `package_id`             | regex + vendor = `publisher_root_id` vendor             | `ManifestForged`     |
/// | `version`                | strict semver                                           | `ManifestForged`     |
/// | `publisher_root_id`      | regex `^pub:[a-z0-9-]{1,64}$`                           | `TrustChainBroken`   |
/// | `package_signing_key_id` | regex `^pks:[a-z0-9-]{1,64}:[a-z0-9-]{1,64}$`          | `TrustChainBroken`   |
/// | `content_hash`           | 32-char lowercase hex                                   | `HashMismatch`       |
/// | `manifest_canonical_hash`| 32-char lowercase hex                                   | `ManifestForged`     |
/// | `installable_scope`      | ∈ \{`SystemOnly`, `GroupScoped`, `Either`\}             | `ManifestForged`     |
/// | `declared_capabilities`  | empty only for `Theme`                                  | `BundleTampered`     |
/// | `issued_at`              | ≤ `now`+drift; ≤ `eol_at` if set                        | `ManifestForged`     |
/// | `channel`                | `RecoveryCritical` only on `AiosRecoveryRepo`           | `ManifestForged`     |
/// | `originating_repository` | kind admission (`KernelCandidate` → `AiosRoot`/`AiosRecovery`)| `RepositoryKindMismatch` |
///
/// `publisher_trust` catalog cross-check, `required_sandbox` validation
/// (S3.2), and `network_manifest` validation (S8.1) are deferred to later
/// pipeline steps / T-197.
///
/// # Errors
///
/// Returns the first `(ManifestField, PackageVerificationResult)` pair for the
/// field that fails validation. The failure modes match the §5.1 table above.
#[allow(clippy::too_many_lines)]
pub fn validate_fields(
    m: &PackageManifest,
    now: DateTime<Utc>,
    max_future_drift: Duration,
) -> Result<(), (ManifestField, PackageVerificationResult)> {
    // ── package_id ──────────────────────────────────────────────────────
    validate_package_id(&m.package_id, &m.publisher_root_id.0)?;

    // ── version ─────────────────────────────────────────────────────────
    validate_semver(&m.version)?;

    // ── publisher_root_id ───────────────────────────────────────────────
    validate_publisher_root_id(&m.publisher_root_id.0)?;

    // ── package_signing_key_id ──────────────────────────────────────────
    validate_package_signing_key_id(&m.package_signing_key_id.0)?;

    // ── content_hash ────────────────────────────────────────────────────
    if !is_32_lower_hex(&m.content_hash) {
        return Err((
            ManifestField::ContentHash,
            PackageVerificationResult::HashMismatch,
        ));
    }

    // ── manifest_canonical_hash ─────────────────────────────────────────
    if !is_32_lower_hex(&m.manifest_canonical_hash) {
        return Err((
            ManifestField::ManifestCanonicalHash,
            PackageVerificationResult::ManifestForged,
        ));
    }

    // ── installable_scope ───────────────────────────────────────────────
    validate_installable_scope(m.installable_scope)?;

    // ── declared_capabilities ───────────────────────────────────────────
    if m.declared_capabilities.is_empty() && m.kind != PackageKind::Theme {
        return Err((
            ManifestField::DeclaredCapabilities,
            PackageVerificationResult::BundleTampered,
        ));
    }

    // ── issued_at ───────────────────────────────────────────────────────
    let drift_limit = now + max_future_drift;
    if m.issued_at > drift_limit {
        return Err((
            ManifestField::IssuedAt,
            PackageVerificationResult::ManifestForged,
        ));
    }
    if let Some(eol) = m.eol_at {
        if m.issued_at >= eol {
            return Err((
                ManifestField::IssuedAt,
                PackageVerificationResult::ManifestForged,
            ));
        }
    }

    // ── channel ─────────────────────────────────────────────────────────
    if m.channel == UpdateChannel::RecoveryCritical
        && m.originating_repository != RepositoryKind::AiosRecoveryRepo
    {
        return Err((
            ManifestField::Channel,
            PackageVerificationResult::ManifestForged,
        ));
    }

    // ── originating_repository (kind admission) ─────────────────────────
    validate_repository_kind_admission(m.kind, m.originating_repository)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Individual field validators (private)
// ---------------------------------------------------------------------------

/// Checks `package_id` regex + vendor cross-check vs `publisher_root_id`.
fn validate_package_id(
    package_id: &str,
    publisher_root_id: &str,
) -> Result<(), (ManifestField, PackageVerificationResult)> {
    // Must start with "pkg:"
    let Some(rest) = package_id.strip_prefix("pkg:") else {
        return Err((
            ManifestField::PackageId,
            PackageVerificationResult::ManifestForged,
        ));
    };

    // Split into vendor:name — exactly two segments
    let mut parts = rest.splitn(2, ':');
    let Some(vendor) = parts.next() else {
        return Err((
            ManifestField::PackageId,
            PackageVerificationResult::ManifestForged,
        ));
    };
    if vendor.is_empty() {
        return Err((
            ManifestField::PackageId,
            PackageVerificationResult::ManifestForged,
        ));
    }
    let Some(name) = parts.next() else {
        return Err((
            ManifestField::PackageId,
            PackageVerificationResult::ManifestForged,
        ));
    };
    if name.is_empty() || name.contains(':') {
        return Err((
            ManifestField::PackageId,
            PackageVerificationResult::ManifestForged,
        ));
    }

    // Vendor: 1–64 lowercase alphanumeric or hyphen
    if vendor.len() > 64
        || !vendor
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err((
            ManifestField::PackageId,
            PackageVerificationResult::ManifestForged,
        ));
    }
    // Name: 1–128 lowercase alphanumeric or hyphen
    if name.len() > 128
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err((
            ManifestField::PackageId,
            PackageVerificationResult::ManifestForged,
        ));
    }

    // Vendor segment must equal publisher_root_id vendor segment
    let pub_vendor = publisher_root_id
        .strip_prefix("pub:")
        .unwrap_or(publisher_root_id); // fallback — publisher_root_id regex checked separately
    if vendor != pub_vendor {
        return Err((
            ManifestField::PackageId,
            PackageVerificationResult::ManifestForged,
        ));
    }

    Ok(())
}

/// Validates `publisher_root_id` against `^pub:[a-z0-9-]{1,64}$`.
fn validate_publisher_root_id(id: &str) -> Result<(), (ManifestField, PackageVerificationResult)> {
    let Some(vendor) = id.strip_prefix("pub:") else {
        return Err((
            ManifestField::PublisherRootId,
            PackageVerificationResult::TrustChainBroken,
        ));
    };
    if vendor.is_empty() {
        return Err((
            ManifestField::PublisherRootId,
            PackageVerificationResult::TrustChainBroken,
        ));
    }
    if vendor.len() > 64 {
        return Err((
            ManifestField::PublisherRootId,
            PackageVerificationResult::TrustChainBroken,
        ));
    }
    if !vendor
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err((
            ManifestField::PublisherRootId,
            PackageVerificationResult::TrustChainBroken,
        ));
    }
    Ok(())
}

/// Validates `package_signing_key_id` against `^pks:[a-z0-9-]{1,64}:[a-z0-9-]{1,64}$`.
fn validate_package_signing_key_id(
    id: &str,
) -> Result<(), (ManifestField, PackageVerificationResult)> {
    let Some(rest) = id.strip_prefix("pks:") else {
        return Err((
            ManifestField::PackageSigningKeyId,
            PackageVerificationResult::TrustChainBroken,
        ));
    };
    let mut parts = rest.splitn(2, ':');
    let Some(vendor) = parts.next() else {
        return Err((
            ManifestField::PackageSigningKeyId,
            PackageVerificationResult::TrustChainBroken,
        ));
    };
    if vendor.is_empty() {
        return Err((
            ManifestField::PackageSigningKeyId,
            PackageVerificationResult::TrustChainBroken,
        ));
    }
    let Some(role) = parts.next() else {
        return Err((
            ManifestField::PackageSigningKeyId,
            PackageVerificationResult::TrustChainBroken,
        ));
    };
    if role.is_empty() || role.contains(':') {
        return Err((
            ManifestField::PackageSigningKeyId,
            PackageVerificationResult::TrustChainBroken,
        ));
    }
    if vendor.len() > 64
        || !vendor
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err((
            ManifestField::PackageSigningKeyId,
            PackageVerificationResult::TrustChainBroken,
        ));
    }
    if role.len() > 64
        || !role
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err((
            ManifestField::PackageSigningKeyId,
            PackageVerificationResult::TrustChainBroken,
        ));
    }
    Ok(())
}

/// Validates `installable_scope` ∈ \{`SystemOnly`, `GroupScoped`, `Either`\}.
const fn validate_installable_scope(
    scope: InstallScope,
) -> Result<(), (ManifestField, PackageVerificationResult)> {
    match scope {
        InstallScope::SystemOnly | InstallScope::GroupScoped | InstallScope::Either => Ok(()),
        InstallScope::UserScoped => Err((
            ManifestField::InstallableScope,
            PackageVerificationResult::ManifestForged,
        )),
    }
}

/// Validates that `kind` is admissible from `repo`.
///
/// Recovery-only kinds (`KernelCandidate`, `InvariantBundle`, `IdentityBundle`,
/// `CapabilityCatalogDelta`) require `AiosRootRepo` or `AiosRecoveryRepo`.
fn validate_repository_kind_admission(
    kind: PackageKind,
    repo: RepositoryKind,
) -> Result<(), (ManifestField, PackageVerificationResult)> {
    let needs_privileged = matches!(
        kind,
        PackageKind::KernelCandidate
            | PackageKind::InvariantBundle
            | PackageKind::IdentityBundle
            | PackageKind::CapabilityCatalogDelta
    );
    if needs_privileged
        && repo != RepositoryKind::AiosRootRepo
        && repo != RepositoryKind::AiosRecoveryRepo
    {
        return Err((
            ManifestField::OriginatingRepository,
            PackageVerificationResult::RepositoryKindMismatch,
        ));
    }
    Ok(())
}

/// Validates strict semver: `MAJOR.MINOR.PATCH[-prerelease][+build]`.
fn validate_semver(version: &str) -> Result<(), (ManifestField, PackageVerificationResult)> {
    // Split off build metadata first (`+build`)
    let core = match version.split_once('+') {
        Some((c, b)) => {
            if !is_valid_semver_ident_segment(b) {
                return Err((
                    ManifestField::Version,
                    PackageVerificationResult::ManifestForged,
                ));
            }
            c
        }
        None => version,
    };

    // Split off pre-release (`-prerelease`)
    let numeric = match core.split_once('-') {
        Some((n, p)) => {
            if p.is_empty() || !is_valid_semver_ident_segment(p) {
                return Err((
                    ManifestField::Version,
                    PackageVerificationResult::ManifestForged,
                ));
            }
            n
        }
        None => core,
    };

    // Parse MAJOR.MINOR.PATCH
    let mut parts = numeric.splitn(3, '.');
    let major = parts.next().unwrap_or("");
    let minor = parts.next().unwrap_or("");
    let patch = parts.next().unwrap_or("");

    if parts.next().is_some() {
        // More than three dot-separated segments
        return Err((
            ManifestField::Version,
            PackageVerificationResult::ManifestForged,
        ));
    }

    if !is_non_negative_integer(major)
        || !is_non_negative_integer(minor)
        || !is_non_negative_integer(patch)
    {
        return Err((
            ManifestField::Version,
            PackageVerificationResult::ManifestForged,
        ));
    }

    // Leading zeros on numeric identifiers are forbidden (except "0" itself)
    if has_leading_zero(major) || has_leading_zero(minor) || has_leading_zero(patch) {
        return Err((
            ManifestField::Version,
            PackageVerificationResult::ManifestForged,
        ));
    }

    Ok(())
}

/// Returns `true` if `s` is a non-negative integer (all ASCII digits, non-empty).
fn is_non_negative_integer(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// Returns `true` if `s` has a leading zero and is not exactly "0".
fn has_leading_zero(s: &str) -> bool {
    s.len() > 1 && s.starts_with('0')
}

/// Returns `true` if `s` is a valid semver identifier segment.
///
/// A segment is dot-separated, each part is alphanumeric-or-hyphen, non-empty,
/// and no part is empty between dots.
fn is_valid_semver_ident_segment(s: &str) -> bool {
    for part in s.split('.') {
        if part.is_empty() {
            return false;
        }
        if !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return false;
        }
    }
    true
}

/// Returns `true` if `s` is exactly 32 lowercase hex characters.
fn is_32_lower_hex(s: &str) -> bool {
    s.len() == 32
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

// ---------------------------------------------------------------------------
// §5.2 signature + chain verification
// ---------------------------------------------------------------------------

/// Verifies a [`PackageManifest`] through the complete pipeline.
///
/// # Steps
///
/// 1. Call [`validate_fields`] — return its result on failure.
/// 2. Recompute `manifest_canonical_hash`; if it differs from
///    `m.manifest_canonical_hash` → `ManifestForged`.
/// 3. If `fetched_content_hash` ≠ `m.content_hash` → `HashMismatch`.
/// 4. Construct a [`SignedPayload`] from the manifest and delegate to
///    the T-188 [`TrustChainVerifier`], which verifies the Ed25519 signature
///    over [`canonical::signing_payload`] and walks the trust chain.
///
/// # Parameters
///
/// - `m` — the manifest to verify.
/// - `verifier` — the T-188 [`TrustChainVerifier`] holding the catalogs.
/// - `fetched_content_hash` — the host-computed `BLAKE3(content)` at fetch time.
/// - `publisher_root_link_sig` — AIOS root's signature over the publisher root entry.
/// - `signing_key_link_sig` — publisher root's signature over the signing key entry.
/// - `now` — current host time.
#[must_use]
pub fn verify_manifest(
    m: &PackageManifest,
    verifier: &TrustChainVerifier<'_>,
    fetched_content_hash: &str,
    publisher_root_link_sig: &crate::trust_chain::LinkSignature,
    signing_key_link_sig: &crate::trust_chain::LinkSignature,
    now: DateTime<Utc>,
) -> PackageVerificationResult {
    // Step 1: field validation
    let max_future_drift = Duration::seconds(300); // default 5 min per spec
    if let Err((_field, result)) = validate_fields(m, now, max_future_drift) {
        return result;
    }

    // Step 2: recompute canonical hash
    let recomputed = canonical::manifest_canonical_hash(m);
    if recomputed != m.manifest_canonical_hash {
        return PackageVerificationResult::ManifestForged;
    }

    // Step 3: content hash check
    if fetched_content_hash != m.content_hash {
        return PackageVerificationResult::HashMismatch;
    }

    // Steps 4–5: Ed25519 signature + trust chain via T-188 verifier
    let payload_bytes = canonical::signing_payload(&m.manifest_canonical_hash).to_vec();
    let input = SignedPayload {
        payload: payload_bytes,
        signature: m.ed25519_signature.clone(),
        package_signing_key_id: m.package_signing_key_id.clone(),
        publisher_root_id: m.publisher_root_id.clone(),
    };

    verifier.verify(
        &input,
        publisher_root_link_sig,
        signing_key_link_sig,
        m.issued_at,
        now,
    )
}

/// Returns `true` if the manifest has reached end-of-life.
///
/// An EOL manifest is one whose `eol_at` is set and `eol_at <= now`.
/// T-190 uses this predicate to auto-quarantine packages during health checks.
#[must_use]
pub fn is_eol(m: &PackageManifest, now: DateTime<Utc>) -> bool {
    m.eol_at.is_some_and(|eol| eol <= now)
}

// ---------------------------------------------------------------------------
// Unit tests — basic validation logic (integration tests in tests/manifest.rs)
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

    // ------------------------------------------------------------------
    // is_32_lower_hex
    // ------------------------------------------------------------------

    #[test]
    fn hex_check_valid_32_lower() {
        assert!(is_32_lower_hex("abcdef0123456789abcdef0123456789"));
    }

    #[test]
    fn hex_check_rejects_uppercase() {
        assert!(!is_32_lower_hex("ABCDEF0123456789ABCDEF0123456789"));
    }

    #[test]
    fn hex_check_rejects_wrong_length() {
        assert!(!is_32_lower_hex("abcdef0123456789abcdef012345678"));
    }

    #[test]
    fn hex_check_rejects_non_hex() {
        assert!(!is_32_lower_hex("ghijkl0123456789abcdef0123456789"));
    }

    // ------------------------------------------------------------------
    // Semver
    // ------------------------------------------------------------------

    #[test]
    fn semver_valid_simple() {
        assert!(validate_semver("1.2.3").is_ok());
    }

    #[test]
    fn semver_valid_with_prerelease() {
        assert!(validate_semver("1.2.3-alpha.1").is_ok());
    }

    #[test]
    fn semver_valid_with_build() {
        assert!(validate_semver("1.2.3+build.42").is_ok());
    }

    #[test]
    fn semver_valid_with_prerelease_and_build() {
        assert!(validate_semver("1.2.3-rc.1+build.42").is_ok());
    }

    #[test]
    fn semver_rejects_leading_zero() {
        assert!(validate_semver("01.2.3").is_err());
    }

    #[test]
    fn semver_rejects_four_segments() {
        assert!(validate_semver("1.2.3.4").is_err());
    }

    #[test]
    fn semver_rejects_empty() {
        assert!(validate_semver("").is_err());
    }

    #[test]
    fn semver_rejects_letters_in_numeric() {
        assert!(validate_semver("1.x.3").is_err());
    }

    // ------------------------------------------------------------------
    // Package ID regex + vendor cross-check
    // ------------------------------------------------------------------

    #[test]
    fn package_id_valid_matches_publisher_vendor() {
        assert!(validate_package_id("pkg:myvendor:myapp", "pub:myvendor").is_ok());
    }

    #[test]
    fn package_id_vendor_mismatch() {
        let r = validate_package_id("pkg:other:myapp", "pub:myvendor");
        assert!(r.is_err());
        if let Err((f, v)) = r {
            assert_eq!(f, ManifestField::PackageId);
            assert_eq!(v, PackageVerificationResult::ManifestForged);
        }
    }

    #[test]
    fn package_id_missing_prefix() {
        assert!(validate_package_id("myapp", "pub:myvendor").is_err());
    }

    #[test]
    fn package_id_no_name_segment() {
        assert!(validate_package_id("pkg:myvendor", "pub:myvendor").is_err());
    }

    #[test]
    fn package_id_uppercase_rejected() {
        assert!(validate_package_id("pkg:MyVendor:myapp", "pub:MyVendor").is_err());
    }

    // ------------------------------------------------------------------
    // Publisher root ID
    // ------------------------------------------------------------------

    #[test]
    fn publisher_root_id_valid() {
        assert!(validate_publisher_root_id("pub:myvendor").is_ok());
    }

    #[test]
    fn publisher_root_id_missing_prefix() {
        let r = validate_publisher_root_id("myvendor");
        assert!(r.is_err());
        if let Err((f, v)) = r {
            assert_eq!(f, ManifestField::PublisherRootId);
            assert_eq!(v, PackageVerificationResult::TrustChainBroken);
        }
    }

    #[test]
    fn publisher_root_id_too_long() {
        let long = format!("pub:{}", "a".repeat(65));
        assert!(validate_publisher_root_id(&long).is_err());
    }

    // ------------------------------------------------------------------
    // Package signing key ID
    // ------------------------------------------------------------------

    #[test]
    fn signing_key_id_valid() {
        assert!(validate_package_signing_key_id("pks:myvendor:release").is_ok());
    }

    #[test]
    fn signing_key_id_missing_prefix() {
        let r = validate_package_signing_key_id("myvendor:release");
        assert!(r.is_err());
        if let Err((f, v)) = r {
            assert_eq!(f, ManifestField::PackageSigningKeyId);
            assert_eq!(v, PackageVerificationResult::TrustChainBroken);
        }
    }

    // ------------------------------------------------------------------
    // installable_scope
    // ------------------------------------------------------------------

    #[test]
    fn installable_scope_user_scoped_rejected() {
        let r = validate_installable_scope(InstallScope::UserScoped);
        assert!(r.is_err());
        if let Err((f, v)) = r {
            assert_eq!(f, ManifestField::InstallableScope);
            assert_eq!(v, PackageVerificationResult::ManifestForged);
        }
    }

    #[test]
    fn installable_scope_valid_variants_accepted() {
        assert!(validate_installable_scope(InstallScope::SystemOnly).is_ok());
        assert!(validate_installable_scope(InstallScope::GroupScoped).is_ok());
        assert!(validate_installable_scope(InstallScope::Either).is_ok());
    }

    // ------------------------------------------------------------------
    // Repository kind admission
    // ------------------------------------------------------------------

    #[test]
    fn kernel_candidate_from_verified_repo_rejected() {
        let r = validate_repository_kind_admission(
            PackageKind::KernelCandidate,
            RepositoryKind::AiosVerifiedRepo,
        );
        assert!(r.is_err());
        if let Err((f, v)) = r {
            assert_eq!(f, ManifestField::OriginatingRepository);
            assert_eq!(v, PackageVerificationResult::RepositoryKindMismatch);
        }
    }

    #[test]
    fn kernel_candidate_from_root_repo_accepted() {
        assert!(validate_repository_kind_admission(
            PackageKind::KernelCandidate,
            RepositoryKind::AiosRootRepo,
        )
        .is_ok());
    }

    #[test]
    fn app_from_verified_repo_accepted() {
        assert!(validate_repository_kind_admission(
            PackageKind::App,
            RepositoryKind::AiosVerifiedRepo
        )
        .is_ok());
    }
}
