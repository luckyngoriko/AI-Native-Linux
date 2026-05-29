//! Strict `SemVer` parser per semver.org §2 / §10 / §11.
//!
//! Format: `MAJOR.MINOR.PATCH[-prerelease][+build]`.
//! Ordering: compare major, minor, patch numerically; a version with a
//! prerelease has **lower** precedence than the same version without;
//! build metadata is **ignored** in ordering per the semver spec.
//!
//! No external semver crate — self-contained, deterministic, tested.

use crate::error::DistributionError;

/// A strict `SemVer` version per semver.org.
///
/// Build metadata is stored but ignored for ordering (semver §10) and
/// for equality comparison (semver §11).
#[derive(Debug, Clone)]
pub struct SemVer {
    /// Major version number.
    pub major: u64,
    /// Minor version number.
    pub minor: u64,
    /// Patch version number.
    pub patch: u64,
    /// Optional pre-release identifier (e.g. "alpha.1").
    pub prerelease: Option<String>,
    /// Optional build metadata (e.g. "build.5"); ignored in ordering.
    pub build: Option<String>,
}

// Build metadata is ignored in equality and hash per semver §10 / §11.
impl PartialEq for SemVer {
    fn eq(&self, other: &Self) -> bool {
        self.major == other.major
            && self.minor == other.minor
            && self.patch == other.patch
            && self.prerelease == other.prerelease
        // build intentionally excluded
    }
}

impl Eq for SemVer {}

impl std::hash::Hash for SemVer {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.major.hash(state);
        self.minor.hash(state);
        self.patch.hash(state);
        self.prerelease.hash(state);
        // build intentionally excluded
    }
}

/// Parse a strict `SemVer` string.
///
/// Accepts: `MAJOR.MINOR.PATCH`, `MAJOR.MINOR.PATCH-prerelease`,
/// `MAJOR.MINOR.PATCH+build`, `MAJOR.MINOR.PATCH-prerelease+build`.
/// Rejects: leading `v`, partial versions, wildcards, extra components.
///
/// # Errors
///
/// Returns [`DistributionError::ManifestForged`] for any parse failure —
/// the version string is part of the signed manifest, so a malformed
/// version is a forgery signal.
pub fn parse(s: &str) -> Result<SemVer, DistributionError> {
    if s.is_empty() {
        return Err(DistributionError::ManifestForged(
            "version string is empty".into(),
        ));
    }

    // Split build metadata first (appears last, preceded by '+')
    let (before_build, build) = match s.split_once('+') {
        Some((pre, bld)) => {
            if bld.is_empty() {
                return Err(DistributionError::ManifestForged(
                    "empty build metadata after '+'".into(),
                ));
            }
            // Build metadata can contain alphanumeric and hyphens
            if !bld
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
            {
                return Err(DistributionError::ManifestForged(format!(
                    "invalid character in build metadata: {bld}"
                )));
            }
            (pre, Some(bld.to_string()))
        }
        None => (s, None),
    };

    // Split prerelease (preceded by '-')
    let (core, prerelease) = match before_build.split_once('-') {
        Some((pre, pr)) => {
            if pr.is_empty() {
                return Err(DistributionError::ManifestForged(
                    "empty prerelease after '-'".into(),
                ));
            }
            // Prerelease can contain alphanumeric and hyphens
            if !pr
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
            {
                return Err(DistributionError::ManifestForged(format!(
                    "invalid character in prerelease: {pr}"
                )));
            }
            (pre, Some(pr.to_string()))
        }
        None => (before_build, None),
    };

    // Core must be MAJOR.MINOR.PATCH — exactly three dot-separated
    // non-negative integers, no leading 'v', no leading zeros unless the
    // number is exactly "0".
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return Err(DistributionError::ManifestForged(format!(
            "expected MAJOR.MINOR.PATCH, got {parts_len} dot-separated parts: {core}",
            parts_len = parts.len()
        )));
    }

    let major = parse_non_negative(parts[0], "major", core)?;
    let minor = parse_non_negative(parts[1], "minor", core)?;
    let patch = parse_non_negative(parts[2], "patch", core)?;

    Ok(SemVer {
        major,
        minor,
        patch,
        prerelease,
        build,
    })
}

/// Parse a single numeric component, rejecting leading zeros (unless the
/// number is exactly `"0"`).
fn parse_non_negative(s: &str, field: &str, full: &str) -> Result<u64, DistributionError> {
    if s.is_empty() {
        return Err(DistributionError::ManifestForged(format!(
            "empty {field} segment in version: {full}"
        )));
    }
    // Reject leading zeros: "0" is ok, "01" is not
    if s.len() > 1 && s.starts_with('0') {
        return Err(DistributionError::ManifestForged(format!(
            "leading zero in {field} segment: {s} (full: {full})"
        )));
    }
    // Must be all ASCII digits
    if !s.chars().all(|c| c.is_ascii_digit()) {
        return Err(DistributionError::ManifestForged(format!(
            "non-digit in {field} segment: {s} (full: {full})"
        )));
    }
    s.parse::<u64>()
        .map_err(|_| DistributionError::ManifestForged(format!("{field} segment overflow: {s}")))
}

impl SemVer {
    /// Returns `true` if this is a pre-release (has a `prerelease`
    /// component).
    #[must_use]
    pub const fn is_prerelease(&self) -> bool {
        self.prerelease.is_some()
    }

    /// String representation without build metadata (for display).
    #[must_use]
    pub fn to_core_string(&self) -> String {
        let base = format!("{}.{}.{}", self.major, self.minor, self.patch);
        if let Some(ref pr) = self.prerelease {
            format!("{base}-{pr}")
        } else {
            base
        }
    }
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(ref pr) = self.prerelease {
            write!(f, "-{pr}")?;
        }
        if let Some(ref bld) = self.build {
            write!(f, "+{bld}")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PartialOrd / Ord — semver precedence (semver.org §11)
// ---------------------------------------------------------------------------

/// Compare two prerelease identifiers.
///
/// Per semver §11.4: numeric identifiers compare numerically; alphanumeric
/// identifiers compare lexically in ASCII sort order. Numeric identifiers
/// always have lower precedence than non-numeric identifiers.
fn compare_prerelease_id(a: &str, b: &str) -> std::cmp::Ordering {
    let a_is_digits = a.chars().all(|c| c.is_ascii_digit());
    let b_is_digits = b.chars().all(|c| c.is_ascii_digit());
    match (a_is_digits, b_is_digits) {
        (true, true) => {
            let na: u64 = a.parse().unwrap_or(0);
            let nb: u64 = b.parse().unwrap_or(0);
            na.cmp(&nb)
        }
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (false, false) => a.cmp(b),
    }
}

/// Compare two prerelease strings by splitting on '.' and comparing each
/// identifier field-by-field. A smaller set of fields has lower precedence
/// than a larger set if all preceding fields are equal.
fn compare_prerelease(a: &str, b: &str) -> std::cmp::Ordering {
    let a_parts: Vec<&str> = a.split('.').collect();
    let b_parts: Vec<&str> = b.split('.').collect();
    let min_len = a_parts.len().min(b_parts.len());
    for i in 0..min_len {
        let ord = compare_prerelease_id(a_parts[i], b_parts[i]);
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    a_parts.len().cmp(&b_parts.len())
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Compare major.minor.patch numerically
        let ordering = self
            .major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch));

        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }

        // Core versions equal — prerelease decides (semver §11.3)
        match (&self.prerelease, &other.prerelease) {
            (None, None) => std::cmp::Ordering::Equal,
            (Some(_), None) => std::cmp::Ordering::Less, // prerelease < release
            (None, Some(_)) => std::cmp::Ordering::Greater, // release > prerelease
            (Some(a), Some(b)) => compare_prerelease(a, b),
        }
        // Build metadata is intentionally ignored — semver §10
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::doc_markdown,
    clippy::similar_names,
    reason = "unit tests in the same module"
)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let v = parse("1.2.3").expect("valid semver");
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.prerelease.is_none());
        assert!(v.build.is_none());
    }

    #[test]
    fn parse_with_prerelease() {
        let v = parse("1.2.3-alpha").expect("valid semver with prerelease");
        assert_eq!(v.prerelease.as_deref(), Some("alpha"));
    }

    #[test]
    fn parse_with_prerelease_dotted() {
        let v = parse("1.2.3-alpha.1").expect("valid dotted prerelease");
        assert_eq!(v.prerelease.as_deref(), Some("alpha.1"));
    }

    #[test]
    fn parse_with_build() {
        let v = parse("1.2.3+build.5").expect("valid semver with build");
        assert_eq!(v.build.as_deref(), Some("build.5"));
        assert!(v.prerelease.is_none());
    }

    #[test]
    fn parse_with_prerelease_and_build() {
        let v = parse("1.2.3-rc.1+build.5").expect("valid semver with prerelease and build");
        assert_eq!(v.prerelease.as_deref(), Some("rc.1"));
        assert_eq!(v.build.as_deref(), Some("build.5"));
    }

    #[test]
    fn parse_zero_patch() {
        let v = parse("0.0.0").expect("0.0.0 is valid");
        assert_eq!(v.major, 0);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn parse_rejects_partial() {
        assert!(parse("1.2").is_err());
        assert!(parse("1").is_err());
    }

    #[test]
    fn parse_rejects_extra() {
        assert!(parse("1.2.3.4").is_err());
    }

    #[test]
    fn parse_rejects_leading_v() {
        assert!(parse("v1.2.3").is_err());
    }

    #[test]
    fn parse_rejects_wildcard() {
        assert!(parse("1.2.x").is_err());
        assert!(parse("1.x.3").is_err());
    }

    #[test]
    fn parse_rejects_empty() {
        assert!(parse("").is_err());
    }

    #[test]
    fn parse_rejects_leading_zero() {
        assert!(parse("01.2.3").is_err());
        assert!(parse("1.02.3").is_err());
        assert!(parse("1.2.03").is_err());
    }

    #[test]
    fn ordering_prerelease_less_than_release() {
        let a = parse("1.0.0-alpha").expect("valid");
        let b = parse("1.0.0").expect("valid");
        assert!(a < b);
        assert!(b > a);
    }

    #[test]
    fn ordering_prerelease_chain() {
        let v1 = parse("1.0.0-alpha").expect("valid");
        let v2 = parse("1.0.0-alpha.1").expect("valid");
        let v3 = parse("1.0.0-beta").expect("valid");
        let v4 = parse("1.0.0").expect("valid");
        let v5 = parse("1.0.1").expect("valid");
        let v6 = parse("1.1.0").expect("valid");
        let v7 = parse("2.0.0").expect("valid");
        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v3 < v4);
        assert!(v4 < v5);
        assert!(v5 < v6);
        assert!(v6 < v7);
    }

    #[test]
    fn ordering_build_ignored() {
        let a = parse("1.0.0+build").expect("valid");
        let b = parse("1.0.0").expect("valid");
        assert_eq!(a, b);
    }

    #[test]
    fn ordering_major_dominates() {
        let a = parse("1.9.9").expect("valid");
        let b = parse("2.0.0").expect("valid");
        assert!(a < b);
    }

    #[test]
    fn ordering_patch_dominates_minor_when_same() {
        let a = parse("1.0.0").expect("valid");
        let b = parse("1.0.1").expect("valid");
        assert!(a < b);
    }

    #[test]
    fn display_roundtrip() {
        let v = parse("1.2.3-rc.1+build.5").expect("valid");
        assert_eq!(v.to_string(), "1.2.3-rc.1+build.5");
    }

    #[test]
    fn to_core_string_no_prerelease() {
        let v = parse("1.2.3+build").expect("valid");
        assert_eq!(v.to_core_string(), "1.2.3");
    }

    #[test]
    fn to_core_string_with_prerelease() {
        let v = parse("1.2.3-rc.1+build").expect("valid");
        assert_eq!(v.to_core_string(), "1.2.3-rc.1");
    }

    #[test]
    fn is_prerelease_detects_prerelease() {
        assert!(parse("1.0.0-alpha").expect("valid").is_prerelease());
        assert!(!parse("1.0.0").expect("valid").is_prerelease());
        assert!(!parse("1.0.0+build").expect("valid").is_prerelease());
    }
}
