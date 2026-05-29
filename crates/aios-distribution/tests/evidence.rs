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

use aios_distribution::*;
use aios_evidence::RetentionClass;

// ---------------------------------------------------------------------------
// DEFAULT_CODE_VERSION
// ---------------------------------------------------------------------------

#[test]
fn default_code_version_constant_is_correct() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-distribution/0.0.1-T196");
}

// ---------------------------------------------------------------------------
// all() completeness
// ---------------------------------------------------------------------------

#[test]
fn all_has_exactly_19_variants() {
    let all = DistributionRecordType::all();
    assert_eq!(all.len(), 19);
}

#[test]
fn all_contains_no_duplicates() {
    let all = DistributionRecordType::all();
    for i in 0..all.len() {
        for j in (i + 1)..all.len() {
            assert_ne!(all[i], all[j], "duplicate at indices {i},{j}");
        }
    }
}

// ---------------------------------------------------------------------------
// retention — per-variant assertions
// ---------------------------------------------------------------------------

#[test]
fn retention_package_fetch_started_is_standard_24m() {
    assert_eq!(
        DistributionRecordType::PackageFetchStarted.retention(),
        RetentionClass::Standard24M
    );
}

#[test]
fn retention_package_verified_is_standard_24m() {
    assert_eq!(
        DistributionRecordType::PackageVerified.retention(),
        RetentionClass::Standard24M
    );
}

#[test]
fn retention_package_verification_failed_is_extended_60m() {
    assert_eq!(
        DistributionRecordType::PackageVerificationFailed.retention(),
        RetentionClass::Extended60M
    );
}

#[test]
fn retention_package_approval_requested_is_standard_24m() {
    assert_eq!(
        DistributionRecordType::PackageApprovalRequested.retention(),
        RetentionClass::Standard24M
    );
}

#[test]
fn retention_package_installed_is_standard_24m() {
    assert_eq!(
        DistributionRecordType::PackageInstalled.retention(),
        RetentionClass::Standard24M
    );
}

#[test]
fn retention_package_install_failed_is_extended_60m() {
    assert_eq!(
        DistributionRecordType::PackageInstallFailed.retention(),
        RetentionClass::Extended60M
    );
}

#[test]
fn retention_package_quarantined_is_forever() {
    assert_eq!(
        DistributionRecordType::PackageQuarantined.retention(),
        RetentionClass::Forever
    );
}

#[test]
fn retention_package_uninstalled_is_standard_24m() {
    assert_eq!(
        DistributionRecordType::PackageUninstalled.retention(),
        RetentionClass::Standard24M
    );
}

#[test]
fn retention_package_downgrade_blocked_is_extended_60m() {
    assert_eq!(
        DistributionRecordType::PackageDowngradeBlocked.retention(),
        RetentionClass::Extended60M
    );
}

#[test]
fn retention_capability_lie_detected_is_forever() {
    assert_eq!(
        DistributionRecordType::CapabilityLieDetected.retention(),
        RetentionClass::Forever
    );
}

#[test]
fn retention_trust_chain_broken_is_forever() {
    assert_eq!(
        DistributionRecordType::TrustChainBroken.retention(),
        RetentionClass::Forever
    );
}

#[test]
fn retention_trust_chain_too_deep_is_forever() {
    assert_eq!(
        DistributionRecordType::TrustChainTooDeep.retention(),
        RetentionClass::Forever
    );
}

#[test]
fn retention_manifest_forged_is_forever() {
    assert_eq!(
        DistributionRecordType::ManifestForged.retention(),
        RetentionClass::Forever
    );
}

#[test]
fn retention_mirror_hash_mismatch_blacklisted_is_forever() {
    assert_eq!(
        DistributionRecordType::MirrorHashMismatchBlacklisted.retention(),
        RetentionClass::Forever
    );
}

#[test]
fn retention_publisher_key_rotated_is_forever() {
    assert_eq!(
        DistributionRecordType::PublisherKeyRotated.retention(),
        RetentionClass::Forever
    );
}

#[test]
fn retention_publisher_deplatformed_is_forever() {
    assert_eq!(
        DistributionRecordType::PublisherDeplatformed.retention(),
        RetentionClass::Forever
    );
}

#[test]
fn retention_external_bridge_package_admitted_is_standard_24m() {
    assert_eq!(
        DistributionRecordType::ExternalBridgePackageAdmitted.retention(),
        RetentionClass::Standard24M
    );
}

#[test]
fn retention_external_bridge_upstream_signature_failed_is_extended_60m() {
    assert_eq!(
        DistributionRecordType::ExternalBridgeUpstreamSignatureFailed.retention(),
        RetentionClass::Extended60M
    );
}

#[test]
fn retention_aios_root_key_rotated_is_forever() {
    assert_eq!(
        DistributionRecordType::AiosRootKeyRotated.retention(),
        RetentionClass::Forever
    );
}

// ---------------------------------------------------------------------------
// retention group count assertions
// ---------------------------------------------------------------------------

#[test]
fn exactly_9_variants_are_forever() {
    let count = DistributionRecordType::all()
        .iter()
        .filter(|v| v.retention() == RetentionClass::Forever)
        .count();
    assert_eq!(count, 9);
}

#[test]
fn exactly_4_variants_are_extended_60m() {
    let count = DistributionRecordType::all()
        .iter()
        .filter(|v| v.retention() == RetentionClass::Extended60M)
        .count();
    assert_eq!(count, 4);
}

#[test]
fn exactly_6_variants_are_standard_24m() {
    let count = DistributionRecordType::all()
        .iter()
        .filter(|v| v.retention() == RetentionClass::Standard24M)
        .count();
    assert_eq!(count, 6);
}

// ---------------------------------------------------------------------------
// wire_name — every variant
// ---------------------------------------------------------------------------

#[test]
fn wire_name_for_all_19_variants_matches_spec() {
    let spec: &[(&str, DistributionRecordType)] = &[
        (
            "PACKAGE_FETCH_STARTED",
            DistributionRecordType::PackageFetchStarted,
        ),
        ("PACKAGE_VERIFIED", DistributionRecordType::PackageVerified),
        (
            "PACKAGE_VERIFICATION_FAILED",
            DistributionRecordType::PackageVerificationFailed,
        ),
        (
            "PACKAGE_APPROVAL_REQUESTED",
            DistributionRecordType::PackageApprovalRequested,
        ),
        ("PACKAGE_INSTALLED", DistributionRecordType::PackageInstalled),
        (
            "PACKAGE_INSTALL_FAILED",
            DistributionRecordType::PackageInstallFailed,
        ),
        (
            "PACKAGE_QUARANTINED",
            DistributionRecordType::PackageQuarantined,
        ),
        (
            "PACKAGE_UNINSTALLED",
            DistributionRecordType::PackageUninstalled,
        ),
        (
            "PACKAGE_DOWNGRADE_BLOCKED",
            DistributionRecordType::PackageDowngradeBlocked,
        ),
        (
            "CAPABILITY_LIE_DETECTED",
            DistributionRecordType::CapabilityLieDetected,
        ),
        (
            "TRUST_CHAIN_BROKEN",
            DistributionRecordType::TrustChainBroken,
        ),
        (
            "TRUST_CHAIN_TOO_DEEP",
            DistributionRecordType::TrustChainTooDeep,
        ),
        ("MANIFEST_FORGED", DistributionRecordType::ManifestForged),
        (
            "MIRROR_HASH_MISMATCH_BLACKLISTED",
            DistributionRecordType::MirrorHashMismatchBlacklisted,
        ),
        (
            "PUBLISHER_KEY_ROTATED",
            DistributionRecordType::PublisherKeyRotated,
        ),
        (
            "PUBLISHER_DEPLATFORMED",
            DistributionRecordType::PublisherDeplatformed,
        ),
        (
            "EXTERNAL_BRIDGE_PACKAGE_ADMITTED",
            DistributionRecordType::ExternalBridgePackageAdmitted,
        ),
        (
            "EXTERNAL_BRIDGE_UPSTREAM_SIGNATURE_FAILED",
            DistributionRecordType::ExternalBridgeUpstreamSignatureFailed,
        ),
        (
            "AIOS_ROOT_KEY_ROTATED",
            DistributionRecordType::AiosRootKeyRotated,
        ),
    ];
    assert_eq!(spec.len(), 19);
    for (spec_name, variant) in spec {
        assert_eq!(
            variant.wire_name(),
            *spec_name,
            "wire_name mismatch for {variant:?}"
        );
    }
}

#[test]
fn wire_names_are_distinct() {
    let all = DistributionRecordType::all();
    let mut names: Vec<&str> = all.iter().map(|v| v.wire_name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 19, "all 19 wire_names must be distinct");
}

// ---------------------------------------------------------------------------
// record_type_for_failure
// ---------------------------------------------------------------------------

#[test]
fn record_type_for_failure_trust_chain_too_deep_maps_correctly() {
    assert_eq!(
        record_type_for_failure(PackageVerificationResult::TrustChainTooDeep),
        DistributionRecordType::TrustChainTooDeep
    );
}

#[test]
fn record_type_for_failure_capability_lie_maps_correctly() {
    assert_eq!(
        record_type_for_failure(PackageVerificationResult::CapabilityLie),
        DistributionRecordType::CapabilityLieDetected
    );
}

#[test]
fn record_type_for_failure_manifest_forged_maps_correctly() {
    assert_eq!(
        record_type_for_failure(PackageVerificationResult::ManifestForged),
        DistributionRecordType::ManifestForged
    );
}

#[test]
fn record_type_for_failure_hash_mismatch_maps_to_mirror_hash() {
    assert_eq!(
        record_type_for_failure(PackageVerificationResult::HashMismatch),
        DistributionRecordType::MirrorHashMismatchBlacklisted
    );
}

#[test]
fn record_type_for_failure_signature_failed_maps_to_verification_failed() {
    assert_eq!(
        record_type_for_failure(PackageVerificationResult::SignatureFailed),
        DistributionRecordType::PackageVerificationFailed
    );
}

#[test]
fn record_type_for_failure_bundle_tampered_maps_to_verification_failed() {
    assert_eq!(
        record_type_for_failure(PackageVerificationResult::BundleTampered),
        DistributionRecordType::PackageVerificationFailed
    );
}

#[test]
fn record_type_for_failure_repo_kind_mismatch_maps_to_verification_failed() {
    assert_eq!(
        record_type_for_failure(PackageVerificationResult::RepositoryKindMismatch),
        DistributionRecordType::PackageVerificationFailed
    );
}

#[test]
fn record_type_for_failure_publisher_deplatformed_maps_correctly() {
    assert_eq!(
        record_type_for_failure(PackageVerificationResult::PublisherDeplatformed),
        DistributionRecordType::PublisherDeplatformed
    );
}

// ---------------------------------------------------------------------------
// emitter — typed helpers (integration tests with tokio)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emitter_emit_package_installed_standard_24m() {
    let emitter = DistributionEvidenceEmitter::new("test:distribution");
    let pkg = PackageId("pkg:acme:hello".into());

    let receipt = emitter
        .emit_package_installed(&pkg)
        .await
        .expect("emit_package_installed");

    assert!(!receipt.record_id.is_empty());
    assert!(!receipt.hash.is_empty());
    assert_eq!(receipt.sequence, 0);
    assert_eq!(emitter.receipt_count().await, 1);

    let payload = emitter.get_payload(0).await.expect("payload");
    assert_eq!(payload["package_id"], "pkg:acme:hello");
    assert_eq!(payload["state"], "ACTIVE");
}

#[tokio::test]
async fn emitter_emit_package_quarantined_forever() {
    let emitter = DistributionEvidenceEmitter::new("test:distribution");
    let pkg = PackageId("pkg:evil:tool".into());

    emitter
        .emit_package_quarantined(&pkg, "deplatform event")
        .await
        .expect("emit_package_quarantined");

    let rc = emitter.get_retention_class(0).await.expect("retention");
    assert_eq!(rc, RetentionClass::Forever);

    let payload = emitter.get_payload(0).await.expect("payload");
    assert_eq!(payload["reason"], "deplatform event");
}

#[tokio::test]
async fn emitter_emit_capability_lie_detected_forever_with_drift_payload() {
    let emitter = DistributionEvidenceEmitter::new("test:distribution");
    let pkg = PackageId("pkg:shady:lib".into());

    emitter
        .emit_capability_lie_detected(&pkg, "GPU claim but no GPU present")
        .await
        .expect("emit_capability_lie_detected");

    let rc = emitter.get_retention_class(0).await.expect("retention");
    assert_eq!(rc, RetentionClass::Forever);

    let payload = emitter.get_payload(0).await.expect("payload");
    assert_eq!(payload["drift"], "GPU claim but no GPU present");
}

#[tokio::test]
async fn emitter_emit_publisher_deplatformed_carries_reason() {
    let emitter = DistributionEvidenceEmitter::new("test:distribution");

    emitter
        .emit_publisher_deplatformed("governance vote #99")
        .await
        .expect("emit_publisher_deplatformed");

    let payload = emitter.get_payload(0).await.expect("payload");
    assert_eq!(payload["reason"], "governance vote #99");
}

#[tokio::test]
async fn emitter_emit_package_downgrade_blocked_extended_60m() {
    let emitter = DistributionEvidenceEmitter::new("test:distribution");
    let pkg = PackageId("pkg:acme:app".into());

    emitter
        .emit_package_downgrade_blocked(&pkg, "3.0.0", "2.0.0")
        .await
        .expect("emit_package_downgrade_blocked");

    let rc = emitter.get_retention_class(0).await.expect("retention");
    assert_eq!(rc, RetentionClass::Extended60M);
}

#[tokio::test]
async fn emitter_inserts_evidence_receipt() {
    let emitter = DistributionEvidenceEmitter::new("test:distribution");
    let pkg = PackageId("pkg:test:lib".into());

    emitter
        .emit_package_installed(&pkg)
        .await
        .expect("emit");
    emitter
        .emit_package_quarantined(&pkg, "test")
        .await
        .expect("emit");

    assert_eq!(emitter.receipt_count().await, 2);
}

#[tokio::test]
async fn emitter_verify_chain_multi_emission_unchanged() {
    let emitter = DistributionEvidenceEmitter::new("test:distribution");
    let pkg = PackageId("pkg:test:lib".into());

    for i in 0..5 {
        emitter
            .emit(
                DistributionRecordType::all()[i],
                Some(&pkg),
                serde_json::json!({"idx": i}),
                chrono::Utc::now(),
            )
            .await
            .expect("emit");
    }

    emitter.verify_chain().await.expect("5-receipt chain ok");
}

// ---------------------------------------------------------------------------
// DistributionEvidenceReceipt
// ---------------------------------------------------------------------------

#[test]
fn distribution_evidence_receipt_serialization_roundtrip() {
    let receipt = DistributionEvidenceReceipt {
        record_id: "evr_01J00000000000000000000000".into(),
        hash: "a".repeat(64),
        sequence: 7,
    };
    let json = serde_json::to_string(&receipt).expect("serialize");
    let back: DistributionEvidenceReceipt = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.record_id, receipt.record_id);
    assert_eq!(back.hash, receipt.hash);
    assert_eq!(back.sequence, receipt.sequence);
}

// ---------------------------------------------------------------------------
// Serialization round-trip for DistributionRecordType
// ---------------------------------------------------------------------------

#[test]
fn distribution_record_type_serde_roundtrip_all_19() {
    for variant in DistributionRecordType::all() {
        let json = serde_json::to_string(&variant).expect("serialize");
        let back: DistributionRecordType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, variant, "serde roundtrip failed for {variant:?}");
    }
}
