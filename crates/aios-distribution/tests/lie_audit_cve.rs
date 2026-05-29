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
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};

use aios_distribution::*;

// ============================================================================
// Helpers
// ============================================================================

#[allow(clippy::missing_const_for_fn)]
fn epoch() -> DateTime<Utc> {
    DateTime::from_timestamp(0, 0).expect("UNIX epoch is valid")
}

fn declared_set(caps: &[&str]) -> BTreeSet<String> {
    caps.iter().map(|s| (*s).to_string()).collect()
}

fn test_package_id() -> PackageId {
    PackageId("pkg:test:audit-app".into())
}

fn make_obs(cap_id: &str, at: DateTime<Utc>, source: ObservationSource) -> CapabilityObservation {
    CapabilityObservation {
        capability_id: cap_id.to_string(),
        source,
        at,
    }
}

// ============================================================================
// Lie audit tests
// ============================================================================

#[test]
fn observed_subset_of_declared_passes_and_stays_active() {
    let declared = declared_set(&["cap.a", "cap.b", "cap.c"]);
    let mut audit = FirstRunAudit::new(test_package_id(), declared, epoch());

    audit.record(make_obs(
        "cap.a",
        epoch() + Duration::seconds(10),
        ObservationSource::L3CapabilityRuntime,
    ));
    audit.record(make_obs(
        "cap.b",
        epoch() + Duration::seconds(20),
        ObservationSource::L4VaultBroker,
    ));

    let outcome = audit.evaluate(epoch() + Duration::seconds(61));
    assert_eq!(outcome, AuditOutcome::Passed);

    let mut state = PackageInstallState::Active;
    let (final_state, result) = apply_audit_outcome(&mut state, &outcome);
    assert_eq!(final_state, PackageInstallState::Active);
    assert_eq!(result, None);
}

#[test]
fn undeclared_capability_causes_failed_with_drift_and_quarantined() {
    let declared = declared_set(&["cap.a"]);
    let mut audit = FirstRunAudit::new(test_package_id(), declared, epoch());

    audit.record(make_obs(
        "cap.a",
        epoch() + Duration::seconds(10),
        ObservationSource::L3CapabilityRuntime,
    ));
    audit.record(make_obs(
        "cap.undeclared",
        epoch() + Duration::seconds(20),
        ObservationSource::L4VaultBroker,
    ));

    let outcome = audit.evaluate(epoch() + Duration::seconds(61));
    assert_eq!(
        outcome,
        AuditOutcome::Failed {
            drift: vec!["cap.undeclared".to_string()]
        }
    );

    let mut state = PackageInstallState::Active;
    let (final_state, result) = apply_audit_outcome(&mut state, &outcome);
    assert_eq!(final_state, PackageInstallState::Quarantined);
    assert_eq!(result, Some(PackageVerificationResult::CapabilityLie));
}

#[test]
fn empty_observed_passes_trivially_and_stays_active() {
    let declared = declared_set(&["cap.a", "cap.b"]);
    let audit = FirstRunAudit::new(test_package_id(), declared, epoch());

    let outcome = audit.evaluate(epoch() + Duration::seconds(61));
    assert_eq!(outcome, AuditOutcome::Passed);

    let mut state = PackageInstallState::Active;
    let (final_state, result) = apply_audit_outcome(&mut state, &outcome);
    assert_eq!(final_state, PackageInstallState::Active);
    assert_eq!(result, None);
}

#[test]
fn over_declaration_passes() {
    // Declared MORE than observed — over-declaration is fine per §9.
    let declared = declared_set(&["cap.a", "cap.b", "cap.c", "cap.extra"]);
    let mut audit = FirstRunAudit::new(test_package_id(), declared, epoch());

    audit.record(make_obs(
        "cap.a",
        epoch() + Duration::seconds(5),
        ObservationSource::L3CapabilityRuntime,
    ));
    audit.record(make_obs(
        "cap.b",
        epoch() + Duration::seconds(10),
        ObservationSource::L7Renderer,
    ));

    let outcome = audit.evaluate(epoch() + Duration::seconds(61));
    assert_eq!(outcome, AuditOutcome::Passed);
}

#[test]
fn observation_after_60s_window_is_ignored() {
    let declared = declared_set(&["cap.a"]);
    let mut audit = FirstRunAudit::new(test_package_id(), declared, epoch());

    // Within window
    audit.record(make_obs(
        "cap.a",
        epoch() + Duration::seconds(30),
        ObservationSource::L3CapabilityRuntime,
    ));
    // After window (at 65s — should be ignored per §9.3 one-shot)
    audit.record(make_obs(
        "cap.undeclared",
        epoch() + Duration::seconds(65),
        ObservationSource::L4VaultBroker,
    ));

    let outcome = audit.evaluate(epoch() + Duration::seconds(70));
    assert_eq!(outcome, AuditOutcome::Passed);
}

#[test]
fn evaluate_before_window_close_is_deterministic() {
    // §9 pre: `now >= opened_at + window`. Calling evaluate before the window
    // closes is deterministic (checks what's been observed so far) but the
    // result is preliminary.
    let declared = declared_set(&["cap.a", "cap.b"]);
    let mut audit = FirstRunAudit::new(test_package_id(), declared, epoch());

    audit.record(make_obs(
        "cap.a",
        epoch() + Duration::seconds(10),
        ObservationSource::L3CapabilityRuntime,
    ));

    // Called at t=30s — window not yet closed
    let outcome = audit.evaluate(epoch() + Duration::seconds(30));
    // All observed so far are declared → Passed
    assert_eq!(outcome, AuditOutcome::Passed);
}

#[test]
fn drift_is_sorted_and_deterministic() {
    let declared = declared_set(&["cap.a"]);
    let mut audit = FirstRunAudit::new(test_package_id(), declared, epoch());

    // Record in non-alphabetical order
    audit.record(make_obs(
        "cap.z",
        epoch() + Duration::seconds(1),
        ObservationSource::L3CapabilityRuntime,
    ));
    audit.record(make_obs(
        "cap.m",
        epoch() + Duration::seconds(2),
        ObservationSource::L4VaultBroker,
    ));
    audit.record(make_obs(
        "cap.a",
        epoch() + Duration::seconds(3),
        ObservationSource::L8NetworkPolicy,
    ));

    let outcome = audit.evaluate(epoch() + Duration::seconds(61));
    assert_eq!(
        outcome,
        AuditOutcome::Failed {
            drift: vec!["cap.m".to_string(), "cap.z".to_string()]
        }
    );
}

#[test]
fn each_observation_source_recorded_correctly() {
    let declared = declared_set(&["cap.a", "cap.b", "cap.c", "cap.d"]);
    let mut audit = FirstRunAudit::new(test_package_id(), declared, epoch());

    audit.record(make_obs(
        "cap.a",
        epoch() + Duration::seconds(1),
        ObservationSource::L3CapabilityRuntime,
    ));
    audit.record(make_obs(
        "cap.b",
        epoch() + Duration::seconds(2),
        ObservationSource::L4VaultBroker,
    ));
    audit.record(make_obs(
        "cap.c",
        epoch() + Duration::seconds(3),
        ObservationSource::L8NetworkPolicy,
    ));
    audit.record(make_obs(
        "cap.d",
        epoch() + Duration::seconds(4),
        ObservationSource::L7Renderer,
    ));

    let outcome = audit.evaluate(epoch() + Duration::seconds(61));
    assert_eq!(outcome, AuditOutcome::Passed);

    // All four capabilities should be in the observed set
    assert!(audit.observed.contains("cap.a"));
    assert!(audit.observed.contains("cap.b"));
    assert!(audit.observed.contains("cap.c"));
    assert!(audit.observed.contains("cap.d"));
}

#[test]
fn apply_audit_outcome_passed_returns_active_none() {
    let mut state = PackageInstallState::Active;
    let outcome = AuditOutcome::Passed;

    let (final_state, result) = apply_audit_outcome(&mut state, &outcome);
    assert_eq!(final_state, PackageInstallState::Active);
    assert_eq!(result, None);
}

#[test]
fn apply_audit_outcome_failed_returns_quarantined_some_capability_lie() {
    let mut state = PackageInstallState::Active;
    let outcome = AuditOutcome::Failed {
        drift: vec!["cap.undeclared".to_string()],
    };

    let (final_state, result) = apply_audit_outcome(&mut state, &outcome);
    assert_eq!(final_state, PackageInstallState::Quarantined);
    assert_eq!(result, Some(PackageVerificationResult::CapabilityLie));
}

#[test]
fn per_install_freshness_two_audit_instances_independent() {
    // §9.3: per-install, not per-package-id — fresh audit each install.
    let declared = declared_set(&["cap.a"]);

    let mut audit1 = FirstRunAudit::new(test_package_id(), declared.clone(), epoch());
    let mut audit2 = FirstRunAudit::new(test_package_id(), declared, epoch());

    // audit1: undeclared → should fail
    audit1.record(make_obs(
        "cap.undeclared",
        epoch() + Duration::seconds(10),
        ObservationSource::L3CapabilityRuntime,
    ));

    // audit2: only declared → should pass
    audit2.record(make_obs(
        "cap.a",
        epoch() + Duration::seconds(10),
        ObservationSource::L4VaultBroker,
    ));

    let outcome1 = audit1.evaluate(epoch() + Duration::seconds(61));
    let outcome2 = audit2.evaluate(epoch() + Duration::seconds(61));

    assert_eq!(
        outcome1,
        AuditOutcome::Failed {
            drift: vec!["cap.undeclared".to_string()]
        }
    );
    assert_eq!(outcome2, AuditOutcome::Passed);
}

// ============================================================================
// CVE binding tests
// ============================================================================

#[test]
fn from_cvss_3_9_is_monitor() {
    assert_eq!(
        CveEnforcementLevel::from_cvss(3.9),
        CveEnforcementLevel::Monitor
    );
}

#[test]
fn from_cvss_4_0_is_notify() {
    assert_eq!(
        CveEnforcementLevel::from_cvss(4.0),
        CveEnforcementLevel::Notify
    );
}

#[test]
fn from_cvss_6_9_is_notify() {
    assert_eq!(
        CveEnforcementLevel::from_cvss(6.9),
        CveEnforcementLevel::Notify
    );
}

#[test]
fn from_cvss_7_0_is_quarantine_candidate() {
    assert_eq!(
        CveEnforcementLevel::from_cvss(7.0),
        CveEnforcementLevel::QuarantineCandidate
    );
}

#[test]
fn from_cvss_8_9_is_quarantine_candidate() {
    assert_eq!(
        CveEnforcementLevel::from_cvss(8.9),
        CveEnforcementLevel::QuarantineCandidate
    );
}

#[test]
fn from_cvss_9_0_is_auto_quarantine() {
    assert_eq!(
        CveEnforcementLevel::from_cvss(9.0),
        CveEnforcementLevel::AutoQuarantine
    );
}

#[test]
fn from_cvss_10_0_is_auto_quarantine() {
    assert_eq!(
        CveEnforcementLevel::from_cvss(10.0),
        CveEnforcementLevel::AutoQuarantine
    );
}

#[test]
fn package_cve_binding_new_derives_enforcement_from_cvss() {
    let binding = PackageCveBinding::new(
        PackageId("pkg:test:cve-app".into()),
        "CVE-2026-12345".into(),
        9.8,
    );
    assert_eq!(binding.package_id, PackageId("pkg:test:cve-app".into()));
    assert_eq!(binding.cve_id, "CVE-2026-12345");
    assert_eq!(binding.enforcement, CveEnforcementLevel::AutoQuarantine);
    // Verify cvss stored as-is
    assert!((binding.cvss - 9.8).abs() < f32::EPSILON);
}

#[test]
fn apply_cve_binding_auto_quarantine_transitions_to_quarantined() {
    let mut state = PackageInstallState::Active;
    let binding = PackageCveBinding::new(
        PackageId("pkg:test:cve-app".into()),
        "CVE-2026-CRITICAL".into(),
        9.5,
    );

    let action = apply_cve_binding(&mut state, &binding);
    assert_eq!(action, CveAction::Quarantined);
    assert_eq!(state, PackageInstallState::Quarantined);
}

#[test]
fn apply_cve_binding_quarantine_candidate_no_transition() {
    let mut state = PackageInstallState::Active;
    let binding = PackageCveBinding::new(
        PackageId("pkg:test:cve-app".into()),
        "CVE-2026-HIGH".into(),
        7.5,
    );

    let action = apply_cve_binding(&mut state, &binding);
    assert_eq!(action, CveAction::QuarantineCandidate);
    assert_eq!(state, PackageInstallState::Active);
}

#[test]
fn apply_cve_binding_notify_returns_notified_no_transition() {
    let mut state = PackageInstallState::Active;
    let binding = PackageCveBinding::new(
        PackageId("pkg:test:cve-app".into()),
        "CVE-2026-MEDIUM".into(),
        5.0,
    );

    let action = apply_cve_binding(&mut state, &binding);
    assert_eq!(action, CveAction::Notified);
    assert_eq!(state, PackageInstallState::Active);
}

#[test]
fn apply_cve_binding_monitor_returns_recorded_no_transition() {
    let mut state = PackageInstallState::Active;
    let binding = PackageCveBinding::new(
        PackageId("pkg:test:cve-app".into()),
        "CVE-2026-LOW".into(),
        2.0,
    );

    let action = apply_cve_binding(&mut state, &binding);
    assert_eq!(action, CveAction::Recorded);
    assert_eq!(state, PackageInstallState::Active);
}

#[test]
fn cve_enforcement_level_label_non_empty_for_all_four() {
    let levels = [
        CveEnforcementLevel::Monitor,
        CveEnforcementLevel::Notify,
        CveEnforcementLevel::QuarantineCandidate,
        CveEnforcementLevel::AutoQuarantine,
    ];
    for level in &levels {
        let label = level.label();
        assert!(!label.is_empty(), "label for {level:?} is empty");
    }
}

// ============================================================================
// DEFAULT_CODE_VERSION check
// ============================================================================

#[test]
fn default_code_version_constant_is_correct() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-distribution/0.0.1-T194");
}
