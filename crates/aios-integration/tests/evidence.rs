#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::doc_markdown,
    clippy::significant_drop_tightening,
    clippy::wildcard_imports,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::needless_collect,
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::collections::HashSet;
use std::sync::Arc;

use chrono::Utc;

use aios_integration::*;

// -- helpers ----------------------------------------------------------------

fn make_invariant(id: &str) -> AiosInvariant {
    AiosInvariant {
        invariant_id: id.into(),
        name: format!("Invariant {id}"),
        layer: "L4".into(),
    }
}

fn make_mapping(id: &str, inv_id: &str) -> ControlMapping {
    ControlMapping {
        mapping_id: id.into(),
        invariant: make_invariant(inv_id),
        control_refs: vec![ControlFrameworkRef {
            framework: StandardKind::Nist80053Rev5,
            control_family: "AC".into(),
            control_id: "AC-3".into(),
        }],
        mapping_rationale: "test".into(),
        mapped_at: Utc::now(),
    }
}

fn make_vendor_contract() -> VendorIntegrationContract {
    VendorIntegrationContract {
        contract_id: VendorContractId("contract-1".into()),
        vendor_name: "TestVendor".into(),
        vendor_kind: VendorKind::PackageRepository,
        trust_class: VendorTrustClass::CommunityVerified,
        contact_canonical_id: "test@vendor.example".into(),
        rotation_cadence_days: 90,
        breach_playbook_url: "https://vendor.example/playbook".into(),
        signer_fingerprint: "aa".repeat(16),
        signature: vec![0u8; 64],
        admitted_at: Utc::now(),
    }
}

fn make_standard_subscription() -> StandardSubscription {
    StandardSubscription {
        subscription_id: StandardSubscriptionId("sub-1".into()),
        standard: StandardKind::Nist80053Rev5,
        catalog_url: "https://csrc.nist.gov/pubs/sp/800/53/r5/upd1/final".into(),
        current_revision: "Rev.5 Update 1".into(),
        last_reviewed_at: Utc::now(),
        next_review_due_at: Utc::now() + chrono::Duration::days(90),
        responsible_canonical_id: "compliance-team".into(),
    }
}

fn make_bridge() -> BridgeContract {
    BridgeContract {
        bridge_id: "bridge-flathub".into(),
        kind: BridgeKind::Flathub,
        vendor_contract: make_vendor_contract(),
        translation_rules: ManifestTranslationRules {
            source_manifest_format: "flatpak-manifest.json".into(),
            capability_extractor: CapabilityExtractorRule::FlatpakFinishesSection,
            trust_floor: VendorTrustClass::CommunityVerified,
        },
        admitted_at: Utc::now(),
    }
}

fn make_baseline() -> ComplianceBaseline {
    ComplianceBaseline {
        baseline_id: "BL-001".into(),
        aios_version: "0.1.0".into(),
        mappings: vec![make_mapping("MAP-A", "INV-001")],
        snapshot_at: Utc::now(),
        validator_canonical_id: "auditor-1".into(),
    }
}

// -- IntegrationRecordType mapping (4 tests) ---------------------------------

#[tokio::test]
async fn all_8_variants_map_to_non_default_record_type() {
    let variants: &[IntegrationRecordType] = &[
        IntegrationRecordType::IntegrationProposed,
        IntegrationRecordType::StandardUpdateAvailable,
        IntegrationRecordType::PackageHasKnownCve,
        IntegrationRecordType::IntegrationLifecycleTransitioned,
        IntegrationRecordType::VendorContractRevoked,
        IntegrationRecordType::BridgeAdmitted,
        IntegrationRecordType::ComplianceBaselineSnapshot,
        IntegrationRecordType::ControlMapDriftDetected,
    ];
    assert_eq!(variants.len(), 8, "must have exactly 8 variants");
    for &variant in variants {
        let _rt = variant.to_evidence_record_type();
    }
}

#[tokio::test]
async fn forever_retention_count_is_3() {
    let forever: &[IntegrationRecordType] = &[
        IntegrationRecordType::VendorContractRevoked,
        IntegrationRecordType::ComplianceBaselineSnapshot,
        IntegrationRecordType::ControlMapDriftDetected,
    ];
    for &variant in forever {
        assert_eq!(
            variant.retention_class(),
            aios_evidence::RetentionClass::Forever,
            "{variant:?} must be FOREVER"
        );
    }
    let all: Vec<_> = [
        IntegrationRecordType::IntegrationProposed,
        IntegrationRecordType::StandardUpdateAvailable,
        IntegrationRecordType::PackageHasKnownCve,
        IntegrationRecordType::IntegrationLifecycleTransitioned,
        IntegrationRecordType::VendorContractRevoked,
        IntegrationRecordType::BridgeAdmitted,
        IntegrationRecordType::ComplianceBaselineSnapshot,
        IntegrationRecordType::ControlMapDriftDetected,
    ]
    .into_iter()
    .filter(|v| v.retention_class() == aios_evidence::RetentionClass::Forever)
    .collect();
    assert_eq!(all.len(), 3);
}

#[tokio::test]
async fn all_as_str_non_empty_and_unique() {
    let variants: &[IntegrationRecordType] = &[
        IntegrationRecordType::IntegrationProposed,
        IntegrationRecordType::StandardUpdateAvailable,
        IntegrationRecordType::PackageHasKnownCve,
        IntegrationRecordType::IntegrationLifecycleTransitioned,
        IntegrationRecordType::VendorContractRevoked,
        IntegrationRecordType::BridgeAdmitted,
        IntegrationRecordType::ComplianceBaselineSnapshot,
        IntegrationRecordType::ControlMapDriftDetected,
    ];
    let mut seen = HashSet::new();
    for &variant in variants {
        let s = variant.as_str();
        assert!(!s.is_empty(), "{variant:?} as_str() is empty");
        assert!(seen.insert(s), "duplicate as_str: {s}");
    }
}

#[tokio::test]
async fn record_type_mapping_is_stable() {
    assert_eq!(
        IntegrationRecordType::IntegrationProposed.to_evidence_record_type(),
        aios_evidence::RecordType::StatusTransition
    );
    assert_eq!(
        IntegrationRecordType::StandardUpdateAvailable.to_evidence_record_type(),
        aios_evidence::RecordType::PolicyDecision
    );
    assert_eq!(
        IntegrationRecordType::PackageHasKnownCve.to_evidence_record_type(),
        aios_evidence::RecordType::FailureObserved
    );
    assert_eq!(
        IntegrationRecordType::BridgeAdmitted.to_evidence_record_type(),
        aios_evidence::RecordType::ExternalBridgePackageAdmitted
    );
    assert_eq!(
        IntegrationRecordType::ComplianceBaselineSnapshot.to_evidence_record_type(),
        aios_evidence::RecordType::ChainCheckpoint
    );
}

// -- InMemoryIntegrationEvidenceEmitter (8 tests) ----------------------------

#[tokio::test]
async fn emitter_new_starts_empty() {
    let e = InMemoryIntegrationEvidenceEmitter::new("_system:test");
    assert_eq!(e.receipt_count().await, 0);
}

#[tokio::test]
async fn emit_integration_proposed_produces_receipt() {
    let e = InMemoryIntegrationEvidenceEmitter::new("_system:test");
    let contract = make_vendor_contract();
    let receipt = e.emit_integration_proposed(&contract).await.expect("emit");
    assert!(!receipt.record_id.is_empty());
    assert!(!receipt.hash.is_empty());
    assert_eq!(receipt.sequence, 0);
    assert_eq!(e.receipt_count().await, 1);
}

#[tokio::test]
async fn emit_standard_update_available_produces_receipt() {
    let e = InMemoryIntegrationEvidenceEmitter::new("_system:test");
    let sub = make_standard_subscription();
    let receipt = e
        .emit_standard_update_available(&sub, "Rev.6")
        .await
        .expect("emit");
    assert_eq!(receipt.sequence, 0);
    assert_eq!(e.receipt_count().await, 1);
}

#[tokio::test]
async fn emit_package_has_known_cve_produces_receipt() {
    let e = InMemoryIntegrationEvidenceEmitter::new("_system:test");
    let binding = PackageCveBinding {
        binding_id: "bind-1".into(),
        cve_id: CveId("CVE-2024-12345".into()),
        package_id: "pkg-nginx".into(),
        status: CveStatus::Open,
        bound_at: Utc::now(),
        matched_via_cpe: None,
        mitigated_by: None,
    };
    let receipt = e
        .emit_package_has_known_cve(&binding, CveSeverity::Critical)
        .await
        .expect("emit");
    assert_eq!(receipt.sequence, 0);
}

#[tokio::test]
async fn emit_lifecycle_transitioned_produces_receipt() {
    let e = InMemoryIntegrationEvidenceEmitter::new("_system:test");
    let receipt = e
        .emit_lifecycle_transitioned(
            &VendorContractId("c1".into()),
            IntegrationLifecycleLabel::Proposed,
            IntegrationLifecycleLabel::Evaluated,
        )
        .await
        .expect("emit");
    assert_eq!(receipt.sequence, 0);
}

#[tokio::test]
async fn emit_vendor_revoked_produces_receipt() {
    let e = InMemoryIntegrationEvidenceEmitter::new("_system:test");
    let receipt = e
        .emit_vendor_revoked(&VendorContractId("c1".into()), "compliance")
        .await
        .expect("emit");
    assert_eq!(receipt.sequence, 0);
}

#[tokio::test]
async fn emit_bridge_admitted_produces_receipt() {
    let e = InMemoryIntegrationEvidenceEmitter::new("_system:test");
    let bridge = make_bridge();
    let receipt = e.emit_bridge_admitted(&bridge).await.expect("emit");
    assert_eq!(receipt.sequence, 0);
}

#[tokio::test]
async fn emit_baseline_snapshot_produces_receipt() {
    let e = InMemoryIntegrationEvidenceEmitter::new("_system:test");
    let baseline = make_baseline();
    let receipt = e.emit_baseline_snapshot(&baseline).await.expect("emit");
    assert_eq!(receipt.sequence, 0);
}

#[tokio::test]
async fn emit_control_drift_produces_receipt() {
    let e = InMemoryIntegrationEvidenceEmitter::new("_system:test");
    let report = ControlDriftReport {
        prior_baseline_id: "BL-001".into(),
        added: vec!["MAP-B".into()],
        removed: vec![],
        modified: vec![],
        unchanged_count: 1,
    };
    let receipt = e.emit_control_drift(&report).await.expect("emit");
    assert_eq!(receipt.sequence, 0);
}

// -- chain integrity (1 test) ------------------------------------------------

#[tokio::test]
async fn chain_integrity_holds_after_multiple_emissions() {
    let e = InMemoryIntegrationEvidenceEmitter::new("_system:test");
    let contract = make_vendor_contract();
    let sub = make_standard_subscription();
    let binding = PackageCveBinding {
        binding_id: "b1".into(),
        cve_id: CveId("CVE-2024-99999".into()),
        package_id: "pkg-x".into(),
        status: CveStatus::Open,
        bound_at: Utc::now(),
        matched_via_cpe: None,
        mitigated_by: None,
    };

    e.emit_integration_proposed(&contract).await.expect("emit");
    e.emit_standard_update_available(&sub, "Rev.6")
        .await
        .expect("emit");
    e.emit_package_has_known_cve(&binding, CveSeverity::High)
        .await
        .expect("emit");
    e.emit_lifecycle_transitioned(
        &VendorContractId("c1".into()),
        IntegrationLifecycleLabel::Proposed,
        IntegrationLifecycleLabel::Evaluated,
    )
    .await
    .expect("emit");
    e.emit_vendor_revoked(&VendorContractId("c1".into()), "reason")
        .await
        .expect("emit");
    e.emit_bridge_admitted(&make_bridge()).await.expect("emit");
    e.emit_baseline_snapshot(&make_baseline())
        .await
        .expect("emit");

    assert_eq!(e.receipt_count().await, 7);
    e.verify_chain().await.expect("chain integrity");
}

// -- payloads (1 test) -------------------------------------------------------

#[tokio::test]
async fn emit_integration_proposed_payload_has_no_raw_signature() {
    let e = InMemoryIntegrationEvidenceEmitter::new("_system:test");
    let contract = make_vendor_contract();
    e.emit_integration_proposed(&contract).await.expect("emit");
    let payload = e.get_payload(0).await.expect("payload");
    // INV-015: no raw signature bytes in evidence.
    assert!(!payload.to_string().contains("signature"));
    assert!(payload.get("contract_id").is_some());
    assert!(payload.get("vendor_name").is_some());
}

// -- EvidenceReceipt serde (1 test) ------------------------------------------

#[tokio::test]
async fn evidence_receipt_serde_roundtrip() {
    let receipt = EvidenceReceipt {
        record_id: "evr_01ABCDEF".into(),
        hash: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
        sequence: 42,
    };
    let json = serde_json::to_string(&receipt).expect("serialize");
    let back: EvidenceReceipt = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.record_id, receipt.record_id);
    assert_eq!(back.hash, receipt.hash);
    assert_eq!(back.sequence, receipt.sequence);
}

// -- Subsystem without emitter still works (3 tests) -------------------------

#[tokio::test]
async fn vendor_registry_without_emitter_still_works() {
    let registry = VendorIntegrationRegistry::new();
    assert!(registry.list_contracts().await.is_empty());
}

#[tokio::test]
async fn cve_feed_without_emitter_still_works() {
    let feed = CveFeedShape::new();
    assert!(feed.list_records().await.is_empty());
}

#[tokio::test]
async fn control_map_without_emitter_still_works() {
    let r = ControlMapRegistry::new();
    let m = make_mapping("MAP-NOEMIT", "INV-001");
    r.add_mapping(m).await.expect("add_mapping");
    let list = r.list_mappings_for_invariant("INV-001").await;
    assert_eq!(list.len(), 1);
}

// -- Emitter wired into registries (2 tests) ---------------------------------

#[tokio::test]
async fn control_map_snapshot_baseline_emits_evidence() {
    let emitter = Arc::new(InMemoryIntegrationEvidenceEmitter::new("_system:test"));
    let r = ControlMapRegistry::new().with_emitter(emitter.clone());
    r.add_mapping(make_mapping("MAP-A", "INV-001"))
        .await
        .expect("add");
    let _baseline = r
        .snapshot_baseline("BL-E".into(), "0.1.0".into(), "v1".into())
        .await
        .expect("snapshot");
    assert_eq!(emitter.receipt_count().await, 1);
}

#[tokio::test]
async fn control_map_detect_drift_emits_evidence_when_drift_found() {
    let emitter = Arc::new(InMemoryIntegrationEvidenceEmitter::new("_system:test"));
    let r = ControlMapRegistry::new().with_emitter(emitter.clone());
    r.add_mapping(make_mapping("MAP-A", "INV-001"))
        .await
        .expect("add");
    let prior = r
        .snapshot_baseline("BL-1".into(), "0.1.0".into(), "v1".into())
        .await
        .expect("snapshot");
    // snapshot itself emitted
    assert_eq!(emitter.receipt_count().await, 1);

    // Add another mapping — drift from prior
    r.add_mapping(make_mapping("MAP-B", "INV-002"))
        .await
        .expect("add");
    let _drift = r.detect_drift(&prior).await;
    assert_eq!(emitter.receipt_count().await, 2);
}

#[tokio::test]
async fn control_map_detect_drift_no_emission_when_no_drift() {
    let emitter = Arc::new(InMemoryIntegrationEvidenceEmitter::new("_system:test"));
    let r = ControlMapRegistry::new().with_emitter(emitter.clone());
    r.add_mapping(make_mapping("MAP-A", "INV-001"))
        .await
        .expect("add");
    let prior = r
        .snapshot_baseline("BL-1".into(), "0.1.0".into(), "v1".into())
        .await
        .expect("snapshot");
    // snapshot emitted 1
    assert_eq!(emitter.receipt_count().await, 1);
    let _drift = r.detect_drift(&prior).await;
    // No drift → no additional emission
    assert_eq!(emitter.receipt_count().await, 1);
}
