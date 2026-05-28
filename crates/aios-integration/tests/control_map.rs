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
    clippy::missing_const_for_fn,
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use aios_integration::*;
use chrono::Utc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_inv(id: &str) -> AiosInvariant {
    AiosInvariant {
        invariant_id: id.into(),
        name: format!("Invariant {id}"),
        layer: "L4".into(),
    }
}

fn make_mapping(id: &str, inv_id: &str) -> ControlMapping {
    ControlMapping {
        mapping_id: id.into(),
        invariant: make_inv(inv_id),
        control_refs: vec![ControlFrameworkRef {
            framework: StandardKind::Nist80053Rev5,
            control_family: "AC".into(),
            control_id: "AC-3".into(),
        }],
        mapping_rationale: "access enforcement".into(),
        mapped_at: Utc::now(),
    }
}

fn make_mapping_with_kind(id: &str, inv_id: &str, kind: StandardKind) -> ControlMapping {
    ControlMapping {
        mapping_id: id.into(),
        invariant: make_inv(inv_id),
        control_refs: vec![ControlFrameworkRef {
            framework: kind,
            control_family: "SC".into(),
            control_id: "SC-7".into(),
        }],
        mapping_rationale: "boundary protection".into(),
        mapped_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// add_mapping_then_list_for_invariant_returns_one
// ---------------------------------------------------------------------------

#[tokio::test]
async fn add_mapping_then_list_for_invariant_returns_one() {
    let reg = ControlMapRegistry::new();
    reg.add_mapping(make_mapping("MAP-A", "INV-001"))
        .await
        .unwrap();
    let list = reg.list_mappings_for_invariant("INV-001").await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].mapping_id, "MAP-A");
}

// ---------------------------------------------------------------------------
// add_duplicate_mapping_id_returns_internal_error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn add_duplicate_mapping_id_returns_internal_error() {
    let reg = ControlMapRegistry::new();
    let m = make_mapping("MAP-DUP", "INV-001");
    reg.add_mapping(m.clone()).await.unwrap();
    let err = reg.add_mapping(m).await.unwrap_err();
    assert!(matches!(err, IntegrationError::Internal(m) if m.contains("exists")));
}

// ---------------------------------------------------------------------------
// list_mappings_for_unknown_invariant_returns_empty
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_mappings_for_unknown_invariant_returns_empty() {
    let reg = ControlMapRegistry::new();
    let list = reg.list_mappings_for_invariant("INV-XXX").await;
    assert!(list.is_empty());
}

// ---------------------------------------------------------------------------
// list_mappings_for_framework_filters_by_standard_kind
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_mappings_for_framework_filters_by_standard_kind() {
    let reg = ControlMapRegistry::new();
    reg.add_mapping(make_mapping_with_kind(
        "MAP-NIST",
        "INV-001",
        StandardKind::Nist80053Rev5,
    ))
    .await
    .unwrap();
    reg.add_mapping(make_mapping_with_kind(
        "MAP-CIS",
        "INV-002",
        StandardKind::CisControlsV8,
    ))
    .await
    .unwrap();

    let nist = reg
        .list_mappings_for_framework(StandardKind::Nist80053Rev5)
        .await;
    assert_eq!(nist.len(), 1);
    assert_eq!(nist[0].mapping_id, "MAP-NIST");

    let cis = reg
        .list_mappings_for_framework(StandardKind::CisControlsV8)
        .await;
    assert_eq!(cis.len(), 1);
    assert_eq!(cis[0].mapping_id, "MAP-CIS");
}

// ---------------------------------------------------------------------------
// snapshot_baseline_captures_all_current_mappings
// ---------------------------------------------------------------------------

#[tokio::test]
async fn snapshot_baseline_captures_all_current_mappings() {
    let reg = ControlMapRegistry::new();
    reg.add_mapping(make_mapping("MAP-A", "INV-001"))
        .await
        .unwrap();
    reg.add_mapping(make_mapping("MAP-B", "INV-002"))
        .await
        .unwrap();

    let baseline = reg
        .snapshot_baseline("BL-001".into(), "0.0.1".into(), "validator-1".into())
        .await
        .unwrap();
    assert_eq!(baseline.baseline_id, "BL-001");
    assert_eq!(baseline.aios_version, "0.0.1");
    assert_eq!(baseline.validator_canonical_id, "validator-1");
    assert_eq!(baseline.mappings.len(), 2);
}

// ---------------------------------------------------------------------------
// snapshot_baseline_two_in_a_row_have_distinct_ids
// ---------------------------------------------------------------------------

#[tokio::test]
async fn snapshot_baseline_two_in_a_row_have_distinct_ids() {
    let reg = ControlMapRegistry::new();
    let b1 = reg
        .snapshot_baseline("BL-AAA".into(), "0.0.1".into(), "v1".into())
        .await
        .unwrap();
    let b2 = reg
        .snapshot_baseline("BL-BBB".into(), "0.0.1".into(), "v1".into())
        .await
        .unwrap();
    assert_ne!(b1.baseline_id, b2.baseline_id);
}

// ---------------------------------------------------------------------------
// get_baseline_known_returns_some
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_baseline_known_returns_some() {
    let reg = ControlMapRegistry::new();
    reg.snapshot_baseline("BL-KNOWN".into(), "0.0.1".into(), "v1".into())
        .await
        .unwrap();
    let found = reg.get_baseline("BL-KNOWN").await;
    assert!(found.is_some());
    assert_eq!(found.unwrap().baseline_id, "BL-KNOWN");
}

// ---------------------------------------------------------------------------
// get_baseline_unknown_returns_none
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_baseline_unknown_returns_none() {
    let reg = ControlMapRegistry::new();
    assert!(reg.get_baseline("NO-SUCH-BASELINE").await.is_none());
}

// ---------------------------------------------------------------------------
// detect_drift_with_identical_mappings_returns_empty_diff
// ---------------------------------------------------------------------------

#[tokio::test]
async fn detect_drift_with_identical_mappings_returns_empty_diff() {
    let reg = ControlMapRegistry::new();
    reg.add_mapping(make_mapping("MAP-ID", "INV-001"))
        .await
        .unwrap();
    let prior = reg
        .snapshot_baseline("BL-1".into(), "0.0.1".into(), "v1".into())
        .await
        .unwrap();

    let drift = reg.detect_drift(&prior).await;
    assert!(drift.added.is_empty());
    assert!(drift.removed.is_empty());
    assert!(drift.modified.is_empty());
    assert_eq!(drift.unchanged_count, 1);
}

// ---------------------------------------------------------------------------
// detect_drift_with_added_mapping_returns_one_added
// ---------------------------------------------------------------------------

#[tokio::test]
async fn detect_drift_with_added_mapping_returns_one_added() {
    let reg = ControlMapRegistry::new();
    reg.add_mapping(make_mapping("MAP-E", "INV-001"))
        .await
        .unwrap();
    let prior = reg
        .snapshot_baseline("BL-1".into(), "0.0.1".into(), "v1".into())
        .await
        .unwrap();

    // Add another mapping after the snapshot.
    reg.add_mapping(make_mapping("MAP-F", "INV-002"))
        .await
        .unwrap();

    let drift = reg.detect_drift(&prior).await;
    assert_eq!(drift.added, vec!["MAP-F"]);
    assert!(drift.removed.is_empty());
    assert!(drift.modified.is_empty());
}

// ---------------------------------------------------------------------------
// detect_drift_with_removed_mapping_returns_one_removed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn detect_drift_with_removed_mapping_returns_one_removed() {
    let reg = ControlMapRegistry::new();
    reg.add_mapping(make_mapping("MAP-G", "INV-001"))
        .await
        .unwrap();
    let prior = reg
        .snapshot_baseline("BL-1".into(), "0.0.1".into(), "v1".into())
        .await
        .unwrap();

    // Mutation: remove MAP-G from current state.
    let mut guard = reg.mappings.write().await;
    guard.remove("MAP-G");
    drop(guard);

    let drift = reg.detect_drift(&prior).await;
    assert_eq!(drift.removed, vec!["MAP-G"]);
    assert!(drift.added.is_empty());
    assert!(drift.modified.is_empty());
}

// ---------------------------------------------------------------------------
// detect_drift_with_modified_rationale_returns_one_modified
// ---------------------------------------------------------------------------

#[tokio::test]
async fn detect_drift_with_modified_rationale_returns_one_modified() {
    let reg = ControlMapRegistry::new();
    reg.add_mapping(make_mapping("MAP-MOD", "INV-001"))
        .await
        .unwrap();
    let prior = reg
        .snapshot_baseline("BL-1".into(), "0.0.1".into(), "v1".into())
        .await
        .unwrap();

    // Mutate the rationale of the existing mapping.
    let mut guard = reg.mappings.write().await;
    if let Some(m) = guard.get_mut("MAP-MOD") {
        m.mapping_rationale = "revised rationale".into();
    }
    drop(guard);

    let drift = reg.detect_drift(&prior).await;
    assert_eq!(drift.modified, vec!["MAP-MOD"]);
    assert!(drift.added.is_empty());
    assert!(drift.removed.is_empty());
}

// ---------------------------------------------------------------------------
// detect_drift_unchanged_count_correct
// ---------------------------------------------------------------------------

#[tokio::test]
async fn detect_drift_unchanged_count_correct() {
    let reg = ControlMapRegistry::new();
    reg.add_mapping(make_mapping("MAP-STAY", "INV-001"))
        .await
        .unwrap();
    reg.add_mapping(make_mapping("MAP-LEAVE", "INV-002"))
        .await
        .unwrap();
    let prior = reg
        .snapshot_baseline("BL-1".into(), "0.0.1".into(), "v1".into())
        .await
        .unwrap();

    // Remove MAP-LEAVE and add MAP-NEW after snapshot.
    let mut guard = reg.mappings.write().await;
    guard.remove("MAP-LEAVE");
    drop(guard);
    reg.add_mapping(make_mapping("MAP-NEW", "INV-003"))
        .await
        .unwrap();

    let drift = reg.detect_drift(&prior).await;
    assert_eq!(drift.added, vec!["MAP-NEW"]);
    assert_eq!(drift.removed, vec!["MAP-LEAVE"]);
    assert!(drift.modified.is_empty());
    assert_eq!(drift.unchanged_count, 1);
}

// ---------------------------------------------------------------------------
// aios_invariant_serde_round_trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn aios_invariant_serde_round_trip() {
    let inv = AiosInvariant {
        invariant_id: "INV-007".into(),
        name: "Secrets Are Capabilities".into(),
        layer: "L4".into(),
    };
    let json = serde_json::to_string(&inv).unwrap();
    let back: AiosInvariant = serde_json::from_str(&json).unwrap();
    assert_eq!(inv, back);
}

// ---------------------------------------------------------------------------
// compliance_baseline_serde_round_trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn compliance_baseline_serde_round_trip() {
    let baseline = ComplianceBaseline {
        baseline_id: "BL-X".into(),
        aios_version: "0.1.0".into(),
        mappings: vec![make_mapping("MAP-C", "INV-003")],
        snapshot_at: Utc::now(),
        validator_canonical_id: "auditor-42".into(),
    };
    let json = serde_json::to_string(&baseline).unwrap();
    let back: ComplianceBaseline = serde_json::from_str(&json).unwrap();
    assert_eq!(baseline, back);
}
