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

use aios_evidence::{RecordType, RetentionClass};
use aios_integration::*;

// -- default_index_entries (3 tests) -----------------------------------------

#[test]
fn default_index_entries_returns_at_least_50() {
    let entries = default_index_entries();
    assert!(entries.len() >= 50, "expected >= 50, got {}", entries.len());
}

#[test]
fn default_index_entries_has_exactly_64() {
    let entries = default_index_entries();
    assert_eq!(
        entries.len(),
        64,
        "default_index_entries must be exactly 64"
    );
}

#[test]
fn default_index_entries_all_wire_names_unique() {
    let entries = default_index_entries();
    let mut seen = HashSet::new();
    for entry in &entries {
        let wire = entry.record_type.as_wire_str();
        assert!(seen.insert(wire), "duplicate wire name: {wire}");
    }
}

// -- UnifiedRecordCatalogue construction (3 tests) ---------------------------

#[test]
fn new_catalogue_is_empty() {
    let cat = UnifiedRecordCatalogue::new();
    assert!(cat.is_empty());
    assert_eq!(cat.len(), 0);
}

#[test]
fn default_catalogue_is_empty() {
    let cat = UnifiedRecordCatalogue::default();
    assert!(cat.is_empty());
}

#[test]
fn catalogue_populated_from_default_entries_has_64() {
    let mut cat = UnifiedRecordCatalogue::new();
    for entry in default_index_entries() {
        cat.register(entry).expect("register");
    }
    assert_eq!(cat.len(), 64);
}

// -- register / get (3 tests) ------------------------------------------------

#[test]
fn register_and_get_roundtrip() {
    let mut cat = UnifiedRecordCatalogue::new();
    let entry = CatalogueEntry {
        ownership: RecordTypeOwnership::AiosIntegration,
        record_type: RecordType::ExternalBridgePackageAdmitted,
        retention: RetentionClass::Standard24M,
        description: "test entry",
    };
    cat.register(entry.clone()).expect("register");
    let retrieved = cat.get(RecordType::ExternalBridgePackageAdmitted.as_wire_str());
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), &entry);
}

#[test]
fn get_unknown_returns_none() {
    let cat = UnifiedRecordCatalogue::new();
    assert!(cat.get("NONEXISTENT").is_none());
}

#[test]
fn duplicate_register_returns_err() {
    let mut cat = UnifiedRecordCatalogue::new();
    let entry = CatalogueEntry {
        ownership: RecordTypeOwnership::AiosEvidence,
        record_type: RecordType::ChainCheckpoint,
        retention: RetentionClass::Forever,
        description: "first",
    };
    cat.register(entry.clone()).expect("first register");
    let result = cat.register(entry);
    assert!(result.is_err());
}

// -- list_by_owner (3 tests) -------------------------------------------------

#[test]
fn list_by_owner_aios_integration_has_2_entries() {
    let mut cat = UnifiedRecordCatalogue::new();
    for entry in default_index_entries() {
        cat.register(entry).expect("register");
    }
    let owned = cat.list_by_owner(RecordTypeOwnership::AiosIntegration);
    assert_eq!(owned.len(), 2);
}

#[test]
fn list_by_owner_aios_action_has_12_entries() {
    let mut cat = UnifiedRecordCatalogue::new();
    for entry in default_index_entries() {
        cat.register(entry).expect("register");
    }
    let owned = cat.list_by_owner(RecordTypeOwnership::AiosAction);
    assert_eq!(owned.len(), 12);
}

#[test]
fn list_by_owner_unknown_returns_empty() {
    let cat = UnifiedRecordCatalogue::new();
    let owned = cat.list_by_owner(RecordTypeOwnership::Reserved);
    assert!(owned.is_empty());
}

// -- list_by_retention (3 tests) ---------------------------------------------

#[test]
fn list_by_retention_forever_has_expected_count() {
    let mut cat = UnifiedRecordCatalogue::new();
    for entry in default_index_entries() {
        cat.register(entry).expect("register");
    }
    let forever = cat.list_by_retention(RetentionClass::Forever);
    // Expected: ChainCheckpoint, ChainInconsistencyDetected, TamperDetected,
    // ApprovalDenied, QuarantineEvent, RawSocketBypassAttempted, DeviceQuarantined,
    // KernelDivergedRegression, FirstBootComplete, RecoveryEvent,
    // EmergencyOverrideGrant = 11
    assert_eq!(forever.len(), 11);
}

#[test]
fn list_forever_records_same_as_filter() {
    let mut cat = UnifiedRecordCatalogue::new();
    for entry in default_index_entries() {
        cat.register(entry).expect("register");
    }
    let forever = cat.list_forever_records();
    let filtered = cat.list_by_retention(RetentionClass::Forever);
    assert_eq!(forever.len(), filtered.len());
    assert_eq!(forever.len(), 11);
}

#[test]
fn list_by_retention_standard_24m_has_expected_count() {
    let mut cat = UnifiedRecordCatalogue::new();
    for entry in default_index_entries() {
        cat.register(entry).expect("register");
    }
    let std = cat.list_by_retention(RetentionClass::Standard24M);
    // 64 total - 11 Forever - 11 Extended60M = 40 Standard24M (roughly)
    assert!(std.len() > 30);
}

// -- RecordTypeOwnership as_str (2 tests) ------------------------------------

#[test]
fn all_18_ownership_variants_have_unique_non_empty_labels() {
    let variants: &[RecordTypeOwnership] = &[
        RecordTypeOwnership::AiosAction,
        RecordTypeOwnership::AiosEvidence,
        RecordTypeOwnership::AiosPolicy,
        RecordTypeOwnership::AiosSgr,
        RecordTypeOwnership::AiosIdentity,
        RecordTypeOwnership::AiosFs,
        RecordTypeOwnership::AiosNetwork,
        RecordTypeOwnership::AiosHardware,
        RecordTypeOwnership::AiosKernel,
        RecordTypeOwnership::AiosCognitive,
        RecordTypeOwnership::AiosRenderer,
        RecordTypeOwnership::AiosCompat,
        RecordTypeOwnership::AiosRepo,
        RecordTypeOwnership::AiosMarketplace,
        RecordTypeOwnership::AiosObservability,
        RecordTypeOwnership::AiosIntegration,
        RecordTypeOwnership::AiosDistribution,
        RecordTypeOwnership::Reserved,
    ];
    assert_eq!(variants.len(), 18, "must have exactly 18 variants");
    let mut seen = HashSet::new();
    for &variant in variants {
        let s = variant.as_str();
        assert!(!s.is_empty(), "{variant:?} as_str() is empty");
        assert!(seen.insert(s), "duplicate as_str: {s}");
    }
}

#[test]
fn catalogue_entry_list_returns_all_entries() {
    let mut cat = UnifiedRecordCatalogue::new();
    for entry in default_index_entries() {
        cat.register(entry).expect("register");
    }
    let all = cat.list();
    assert_eq!(all.len(), 64);
}

#[test]
fn catalogue_entry_fields_are_non_empty() {
    let entries = default_index_entries();
    for entry in &entries {
        assert!(!entry.record_type.as_wire_str().is_empty());
        assert!(
            !entry.description.is_empty(),
            "empty description for {:?}",
            entry.record_type
        );
    }
}
