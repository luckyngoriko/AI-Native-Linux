//! M19 acceptance gate — `aios-distribution` v0.1.0 closure (S11.1).
//!
//! Asserts the headline L10 distribution guarantees hold as a single acceptance
//! surface. Closing M19 marks **Rev.2 FULL-REAL** (19/19 implementation
//! milestones). Each `ac_*` test maps to one S11.1 acceptance criterion.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::unwrap_in_result,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use aios_distribution::{
    apply_cve_binding, extended_60m_variants, forever_variants, record_type_for_failure,
    standard_24m_variants, CveAction, CveEnforcementLevel, DistributionEvidenceEmitter,
    DistributionRecordType, PackageCveBinding, PackageId, PackageInstallState,
    PackageVerificationResult,
};

/// AC-1 — the distribution evidence vocabulary is a closed set of 19 record
/// types, split 9 `FOREVER` / 4 `EXTENDED_60M` / 6 `STANDARD_24M` (S11.1 §17).
#[test]
fn ac1_evidence_vocabulary_is_19_with_correct_retention_split() {
    assert_eq!(DistributionRecordType::all().len(), 19);
    assert_eq!(forever_variants().len(), 9);
    assert_eq!(extended_60m_variants().len(), 4);
    assert_eq!(standard_24m_variants().len(), 6);
    assert_eq!(
        forever_variants().len() + extended_60m_variants().len() + standard_24m_variants().len(),
        19,
        "retention classes must partition all 19 record types"
    );
}

/// AC-2 — a critical-severity CVE auto-quarantines an active package
/// (`Active → Quarantined`) per S11.1 §9.
#[test]
fn ac2_critical_cve_auto_quarantines_active_package() {
    let binding = PackageCveBinding::new(
        PackageId("pkg:acme:critical".into()),
        "CVE-2026-9999".to_owned(),
        9.8,
    );
    assert_eq!(binding.enforcement, CveEnforcementLevel::AutoQuarantine);

    let mut state = PackageInstallState::Active;
    assert_eq!(
        apply_cve_binding(&mut state, &binding),
        CveAction::Quarantined
    );
    assert_eq!(state, PackageInstallState::Quarantined);
}

/// AC-3 — every package verification failure maps to a distribution evidence
/// record (fail-closed accountability; S11.1 §17).
#[test]
fn ac3_verification_failures_map_to_evidence_records() {
    let failures = [
        PackageVerificationResult::SignatureFailed,
        PackageVerificationResult::TrustChainBroken,
        PackageVerificationResult::TrustChainTooDeep,
        PackageVerificationResult::PublisherDeplatformed,
        PackageVerificationResult::HashMismatch,
        PackageVerificationResult::ManifestForged,
        PackageVerificationResult::CapabilityLie,
    ];
    for f in failures {
        // Must produce a record (never panics for a failure variant).
        let _ = record_type_for_failure(f);
    }
    assert_eq!(
        record_type_for_failure(PackageVerificationResult::TrustChainTooDeep),
        DistributionRecordType::TrustChainTooDeep
    );
    assert_eq!(
        record_type_for_failure(PackageVerificationResult::CapabilityLie),
        DistributionRecordType::CapabilityLieDetected
    );
}

/// AC-4 — the distribution evidence log is append-only and hash-chain
/// verifiable (S11.1 §17 ↔ S3.1, INV-005).
#[tokio::test]
async fn ac4_evidence_chain_is_append_only_and_verifiable() {
    let emitter = DistributionEvidenceEmitter::new("service:aios-distribution");
    let pkg = PackageId("pkg:acme:app".into());

    emitter.emit_package_installed(&pkg).await.expect("install");
    emitter
        .emit_package_quarantined(&pkg, "post-install CVE")
        .await
        .expect("quarantine");

    assert_eq!(emitter.receipt_count().await, 2);
    emitter
        .verify_chain()
        .await
        .expect("append-only chain must verify");
}
