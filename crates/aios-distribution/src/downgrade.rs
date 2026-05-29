//! Per-package monotonic downgrade protection per S11.1 §15.2.
//!
//! The host tracks the highest `SemVer` ever installed for each `PackageId`.
//! Installing a strictly-older version is rejected with
//! `PackageDowngradeBlocked`.

use std::collections::HashMap;

use crate::error::DistributionError;
use crate::ids::PackageId;
use crate::version::SemVer;

/// Tracks the highest installed version per package ID.
///
/// This is a monotonic counter: once a version is recorded for a package,
/// only equal or newer versions are accepted.
///
/// Re-installing the same version is allowed (idempotent) but does not
/// change the counter. Installing a newer version updates the counter.
/// Installing a strictly-older version returns `PackageDowngradeBlocked`.
#[derive(Debug, Clone)]
pub struct VersionMonotonicCounter {
    highest: HashMap<PackageId, SemVer>,
}

impl VersionMonotonicCounter {
    /// Creates an empty counter — no packages have been installed yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            highest: HashMap::new(),
        }
    }

    /// Returns the currently recorded highest version for `package_id`,
    /// if any.
    #[must_use]
    pub fn get(&self, package_id: &PackageId) -> Option<&SemVer> {
        self.highest.get(package_id)
    }

    /// Checks whether `version` is allowed given the recorded history, and
    /// updates the counter if the version is newer.
    ///
    /// - First install (no recorded highest) → always allowed, counter set.
    /// - Equal version → allowed (re-install), counter unchanged.
    /// - Newer version → allowed, counter updated.
    /// - Strictly older version → `PackageDowngradeBlocked`.
    ///
    /// # Errors
    ///
    /// Returns `PackageDowngradeBlocked` when `version` is strictly older
    /// than the recorded highest for this `package_id`.
    pub fn check_and_record(
        &mut self,
        package_id: &PackageId,
        version: &SemVer,
    ) -> Result<(), DistributionError> {
        match self.highest.get(package_id) {
            Some(existing) if version < existing => {
                Err(DistributionError::PackageDowngradeBlocked(format!(
                    "downgrade blocked: package {} had version {}; attempted install of {}",
                    package_id.0, existing, version
                )))
            }
            Some(existing) if version == existing => {
                // Re-install same version — idempotent, no change.
                Ok(())
            }
            Some(_) | None => {
                // Newer version or first install — record and allow.
                self.highest.insert(package_id.clone(), version.clone());
                Ok(())
            }
        }
    }

    /// Returns the number of packages currently tracked.
    #[must_use]
    pub fn len(&self) -> usize {
        self.highest.len()
    }

    /// Returns `true` if no packages have been recorded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.highest.is_empty()
    }
}

impl Default for VersionMonotonicCounter {
    fn default() -> Self {
        Self::new()
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
    use crate::error::DistributionErrorCode;
    use crate::version::parse;

    fn pkg(id: &str) -> PackageId {
        PackageId(id.to_string())
    }

    #[test]
    fn first_install_allowed() {
        let mut counter = VersionMonotonicCounter::new();
        let v = parse("1.0.0").expect("valid");
        assert!(counter.check_and_record(&pkg("pkg:a:test"), &v).is_ok());
    }

    #[test]
    fn newer_version_allowed_and_updates_counter() {
        let mut counter = VersionMonotonicCounter::new();
        let p = pkg("pkg:a:test");
        counter
            .check_and_record(&p, &parse("1.0.0").expect("valid"))
            .expect("first install ok");
        let result = counter.check_and_record(&p, &parse("1.1.0").expect("valid"));
        assert!(result.is_ok());
        assert_eq!(
            counter
                .get(&p)
                .expect("should have a recorded version")
                .to_string(),
            "1.1.0"
        );
    }

    #[test]
    fn reinstall_same_version_allowed() {
        let mut counter = VersionMonotonicCounter::new();
        let p = pkg("pkg:a:test");
        let v = parse("1.1.0").expect("valid");
        counter.check_and_record(&p, &v).expect("first ok");
        // Re-install same version
        assert!(counter.check_and_record(&p, &v).is_ok());
        // Counter should still reflect 1.1.0
        assert_eq!(
            counter
                .get(&p)
                .expect("should still be recorded")
                .to_string(),
            "1.1.0"
        );
    }

    #[test]
    fn downgrade_blocked() {
        let mut counter = VersionMonotonicCounter::new();
        let p = pkg("pkg:a:test");
        counter
            .check_and_record(&p, &parse("1.1.0").expect("valid"))
            .expect("first ok");
        let result = counter.check_and_record(&p, &parse("1.0.0").expect("valid"));
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            DistributionErrorCode::PackageDowngradeBlocked
        );
    }

    #[test]
    fn prerelease_downgrade_blocked() {
        let mut counter = VersionMonotonicCounter::new();
        let p = pkg("pkg:a:test");
        counter
            .check_and_record(&p, &parse("1.0.0").expect("valid"))
            .expect("first ok");
        // 1.0.0-rc.1 is strictly older than 1.0.0
        let result = counter.check_and_record(&p, &parse("1.0.0-rc.1").expect("valid"));
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            DistributionErrorCode::PackageDowngradeBlocked
        );
    }

    #[test]
    fn different_packages_independent() {
        let mut counter = VersionMonotonicCounter::new();
        let pa = pkg("pkg:a:alpha");
        let pb = pkg("pkg:b:beta");
        counter
            .check_and_record(&pa, &parse("2.0.0").expect("valid"))
            .expect("a ok");
        // b can install 1.0.0 independently — its counter is separate
        assert!(counter
            .check_and_record(&pb, &parse("1.0.0").expect("valid"))
            .is_ok());
        // a is still at 2.0.0
        assert_eq!(
            counter.get(&pa).expect("should be 2.0.0").to_string(),
            "2.0.0"
        );
    }

    #[test]
    fn default_is_empty() {
        let counter = VersionMonotonicCounter::default();
        assert!(counter.is_empty());
        assert_eq!(counter.len(), 0);
    }

    #[test]
    fn new_is_empty() {
        let counter = VersionMonotonicCounter::new();
        assert!(counter.is_empty());
    }
}
