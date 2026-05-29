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

use chrono::TimeZone;
use chrono::Utc;

use aios_distribution::*;

// ---------------------------------------------------------------------------
// SemVer parsing tests
// ---------------------------------------------------------------------------

#[test]
fn semver_parse_valid_simple() {
    let v = parse_semver("1.2.3").expect("valid");
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 2);
    assert_eq!(v.patch, 3);
    assert!(v.prerelease.is_none());
    assert!(v.build.is_none());
}

#[test]
fn semver_parse_valid_prerelease() {
    let v = parse_semver("1.2.3-alpha").expect("valid");
    assert_eq!(v.prerelease.as_deref(), Some("alpha"));
}

#[test]
fn semver_parse_valid_dotted_prerelease() {
    let v = parse_semver("1.2.3-alpha.1").expect("valid");
    assert_eq!(v.prerelease.as_deref(), Some("alpha.1"));
}

#[test]
fn semver_parse_valid_build() {
    let v = parse_semver("1.2.3+build").expect("valid");
    assert_eq!(v.build.as_deref(), Some("build"));
}

#[test]
fn semver_parse_valid_prerelease_and_build() {
    let v = parse_semver("1.2.3-rc.1+build.5").expect("valid");
    assert_eq!(v.prerelease.as_deref(), Some("rc.1"));
    assert_eq!(v.build.as_deref(), Some("build.5"));
}

#[test]
fn semver_parse_rejects_partial() {
    assert!(parse_semver("1.2").is_err());
}

#[test]
fn semver_parse_rejects_extra() {
    assert!(parse_semver("1.2.3.4").is_err());
}

#[test]
fn semver_parse_rejects_leading_v() {
    assert!(parse_semver("v1.2.3").is_err());
}

#[test]
fn semver_parse_rejects_wildcard() {
    assert!(parse_semver("1.2.x").is_err());
}

#[test]
fn semver_parse_rejects_empty() {
    assert!(parse_semver("").is_err());
}

// ---------------------------------------------------------------------------
// SemVer ordering tests
// ---------------------------------------------------------------------------

#[test]
fn semver_ordering_precedence_chain() {
    let versions = [
        parse_semver("1.0.0-alpha").expect("valid"),
        parse_semver("1.0.0-alpha.1").expect("valid"),
        parse_semver("1.0.0-beta").expect("valid"),
        parse_semver("1.0.0").expect("valid"),
        parse_semver("1.0.1").expect("valid"),
        parse_semver("1.1.0").expect("valid"),
        parse_semver("2.0.0").expect("valid"),
    ];
    for i in 1..versions.len() {
        assert!(
            versions[i - 1] < versions[i],
            "{} < {}",
            versions[i - 1],
            versions[i]
        );
    }
}

#[test]
fn semver_build_ignored_in_ordering() {
    let a = parse_semver("1.0.0+build.123").expect("valid");
    let b = parse_semver("1.0.0").expect("valid");
    assert_eq!(a, b);
}

#[test]
fn semver_prerelease_less_than_release() {
    let a = parse_semver("1.0.0-rc.1").expect("valid");
    let b = parse_semver("1.0.0").expect("valid");
    assert!(a < b);
}

// ---------------------------------------------------------------------------
// UpdateChannel rollout tests
// ---------------------------------------------------------------------------

#[test]
fn auto_update_allowed_stable() {
    assert!(auto_update_allowed(UpdateChannel::Stable));
}

#[test]
fn auto_update_allowed_beta_false() {
    assert!(!auto_update_allowed(UpdateChannel::Beta));
}

#[test]
fn auto_update_allowed_recovery_critical_false() {
    assert!(!auto_update_allowed(UpdateChannel::RecoveryCritical));
}

#[test]
fn auto_update_allowed_deprecated_false() {
    assert!(!auto_update_allowed(UpdateChannel::DeprecatedRetention));
}

#[test]
fn stable_in_update_window() {
    let window = UpdateWindow {
        start: Utc.timestamp_opt(1000, 0).unwrap(),
        end: Utc.timestamp_opt(2000, 0).unwrap(),
    };
    assert!(stable_auto_update_permitted(
        &window,
        Utc.timestamp_opt(1500, 0).unwrap()
    ));
}

#[test]
fn stable_outside_update_window() {
    let window = UpdateWindow {
        start: Utc.timestamp_opt(1000, 0).unwrap(),
        end: Utc.timestamp_opt(2000, 0).unwrap(),
    };
    assert!(!stable_auto_update_permitted(
        &window,
        Utc.timestamp_opt(2500, 0).unwrap()
    ));
}

#[test]
fn beta_to_stable_requires_reissue() {
    assert!(requires_reissue_for_channel_change(
        UpdateChannel::Beta,
        UpdateChannel::Stable
    ));
}

#[test]
fn channel_widening_stable_to_beta() {
    assert!(channel_widening_requires_approval(
        UpdateChannel::Stable,
        UpdateChannel::Beta
    ));
}

#[test]
fn recovery_critical_on_verified_repo_err() {
    assert!(validate_channel_for_repo(
        UpdateChannel::RecoveryCritical,
        RepositoryKind::AiosVerifiedRepo
    )
    .is_err());
}

#[test]
fn recovery_critical_on_recovery_repo_ok() {
    assert!(validate_channel_for_repo(
        UpdateChannel::RecoveryCritical,
        RepositoryKind::AiosRecoveryRepo
    )
    .is_ok());
}

#[test]
fn recovery_critical_requires_recovery_true() {
    assert!(recovery_critical_requires_recovery(
        UpdateChannel::RecoveryCritical
    ));
}

#[test]
fn stable_does_not_require_recovery() {
    assert!(!recovery_critical_requires_recovery(UpdateChannel::Stable));
}

// ---------------------------------------------------------------------------
// VersionMonotonicCounter tests (§15.2)
// ---------------------------------------------------------------------------

#[test]
fn downgrade_first_install_allowed() {
    let mut counter = VersionMonotonicCounter::new();
    assert!(counter
        .check_and_record(
            &PackageId("pkg:a:test".into()),
            &parse_semver("1.0.0").expect("valid")
        )
        .is_ok());
}

#[test]
fn downgrade_newer_allowed() {
    let mut counter = VersionMonotonicCounter::new();
    let pid = PackageId("pkg:a:test".into());
    counter
        .check_and_record(&pid, &parse_semver("1.0.0").expect("valid"))
        .expect("first ok");
    assert!(counter
        .check_and_record(&pid, &parse_semver("1.1.0").expect("valid"))
        .is_ok());
}

#[test]
fn downgrade_reinstall_same_allowed() {
    let mut counter = VersionMonotonicCounter::new();
    let pid = PackageId("pkg:a:test".into());
    let v = parse_semver("1.1.0").expect("valid");
    counter.check_and_record(&pid, &v).expect("first ok");
    assert!(counter.check_and_record(&pid, &v).is_ok());
}

#[test]
fn downgrade_strictly_older_blocked() {
    let mut counter = VersionMonotonicCounter::new();
    let pid = PackageId("pkg:a:test".into());
    counter
        .check_and_record(&pid, &parse_semver("1.1.0").expect("valid"))
        .expect("first ok");
    let result = counter.check_and_record(&pid, &parse_semver("1.0.0").expect("valid"));
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code(),
        DistributionErrorCode::PackageDowngradeBlocked
    );
}

#[test]
fn downgrade_prerelease_after_release_blocked() {
    let mut counter = VersionMonotonicCounter::new();
    let pid = PackageId("pkg:a:test".into());
    counter
        .check_and_record(&pid, &parse_semver("1.0.0").expect("valid"))
        .expect("first ok");
    // 1.0.0-rc.1 < 1.0.0 per semver precedence
    let result = counter.check_and_record(&pid, &parse_semver("1.0.0-rc.1").expect("valid"));
    assert!(result.is_err());
}

#[test]
fn downgrade_different_package_independent() {
    let mut counter = VersionMonotonicCounter::new();
    let a = PackageId("pkg:a:alpha".into());
    let b = PackageId("pkg:b:beta".into());
    counter
        .check_and_record(&a, &parse_semver("2.0.0").expect("valid"))
        .expect("a ok");
    // b's counter is independent
    assert!(counter
        .check_and_record(&b, &parse_semver("1.0.0").expect("valid"))
        .is_ok());
}

#[test]
fn downgrade_code_maps_correctly() {
    let err = DistributionError::PackageDowngradeBlocked("test".into());
    assert_eq!(err.code(), DistributionErrorCode::PackageDowngradeBlocked);
}

// ---------------------------------------------------------------------------
// DEFAULT_CODE_VERSION
// ---------------------------------------------------------------------------

#[test]
fn default_code_version_is_t194() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-distribution/0.0.1-T194");
}
