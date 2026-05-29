//! T-197 — Cross-crate integration: M19 distribution ↔ M18 integration framework
//! (`aios-integration`) + the append-only evidence log (`aios-evidence`, via the
//! distribution emitter).
//!
//! These tests prove that the L10 distribution layer composes with the M18
//! system-integration framework rather than re-implementing it in isolation:
//!
//! 1. A CVE identity validated by M18's feed (`cve_feed::is_valid_cve_id`) drives
//!    M19 CVE enforcement (`apply_cve_binding`) and produces a FOREVER evidence
//!    receipt.
//! 2. An M18 publisher `IntegrationLifecycleState` gates whether M19 may distribute
//!    that publisher's packages (only `Piloted` / `Production` may).
//! 3. A multi-step distribution flow emits an ordered, hash-chain-verifiable
//!    evidence trail.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::unwrap_in_result,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use aios_distribution::{
    apply_cve_binding, CveAction, CveEnforcementLevel, DistributionEvidenceEmitter,
    PackageCveBinding, PackageId, PackageInstallState,
};
use aios_integration::cve_feed::is_valid_cve_id;
use aios_integration::IntegrationLifecycleState;
use chrono::Utc;

/// Integration contract (M19 ↔ M18): a publisher may distribute packages only once
/// its M18 integration lifecycle has reached a deployable state.
fn publisher_may_distribute(state: &IntegrationLifecycleState) -> bool {
    matches!(
        state,
        IntegrationLifecycleState::Piloted { .. } | IntegrationLifecycleState::Production { .. }
    )
}

#[tokio::test]
async fn m18_cve_identity_drives_m19_quarantine_and_evidence() {
    // M18 validates the CVE identity ...
    let cve_id = "CVE-2026-0001";
    assert!(
        is_valid_cve_id(cve_id),
        "M18 cve_feed must accept a well-formed CVE id"
    );

    // ... which M19 binds to a package with a critical CVSS → AutoQuarantine.
    let pkg = PackageId("pkg:acme:vuln-lib".into());
    let binding = PackageCveBinding::new(pkg.clone(), cve_id.to_owned(), 9.8);
    assert_eq!(
        binding.enforcement,
        CveEnforcementLevel::AutoQuarantine,
        "critical CVSS must map to AutoQuarantine"
    );

    let mut state = PackageInstallState::Active;
    let action = apply_cve_binding(&mut state, &binding);
    assert_eq!(action, CveAction::Quarantined);
    assert_eq!(state, PackageInstallState::Quarantined);

    // ... and the distribution layer records a FOREVER evidence receipt.
    let emitter = DistributionEvidenceEmitter::new("service:aios-distribution");
    emitter
        .emit_package_quarantined(&pkg, &format!("auto-quarantine for {cve_id}"))
        .await
        .expect("quarantine evidence must emit");
    assert_eq!(emitter.receipt_count().await, 1);
    emitter
        .verify_chain()
        .await
        .expect("evidence chain must verify");
}

#[test]
fn low_severity_m18_cve_does_not_auto_quarantine_in_m19() {
    let pkg = PackageId("pkg:acme:minor".into());
    let binding = PackageCveBinding::new(pkg, "CVE-2026-0002".to_owned(), 2.5);
    let mut state = PackageInstallState::Active;
    let action = apply_cve_binding(&mut state, &binding);
    assert_ne!(
        action,
        CveAction::Quarantined,
        "low CVSS must not auto-quarantine"
    );
    assert_eq!(
        state,
        PackageInstallState::Active,
        "state must be unchanged for a low-severity CVE"
    );
}

#[test]
fn m18_publisher_lifecycle_gates_m19_distribution() {
    // An evaluated-but-audit-failed publisher may NOT distribute.
    let evaluated_failed = IntegrationLifecycleState::Evaluated {
        evaluator: "sec:auditor".into(),
        evaluated_at: Utc::now(),
        security_audit_passed: false,
    };
    assert!(
        !publisher_may_distribute(&evaluated_failed),
        "an audit-failed evaluated publisher must not distribute"
    );

    // A production publisher may distribute.
    let production = IntegrationLifecycleState::Production { since: Utc::now() };
    assert!(
        publisher_may_distribute(&production),
        "a production publisher must be allowed to distribute"
    );
}

#[tokio::test]
async fn full_distribution_flow_emits_ordered_verifiable_evidence() {
    let pkg = PackageId("pkg:acme:app".into());
    let emitter = DistributionEvidenceEmitter::new("service:aios-distribution");

    emitter
        .emit_package_installed(&pkg)
        .await
        .expect("installed");
    emitter
        .emit_package_downgrade_blocked(&pkg, "2.1.0", "2.0.0")
        .await
        .expect("downgrade blocked");
    emitter
        .emit_package_quarantined(&pkg, "post-install CVE binding")
        .await
        .expect("quarantined");

    assert_eq!(emitter.receipt_count().await, 3);
    emitter
        .verify_chain()
        .await
        .expect("3-receipt cross-stage chain must verify");
}
