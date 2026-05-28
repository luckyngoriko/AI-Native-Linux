#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::doc_markdown,
    clippy::significant_drop_tightening,
    clippy::wildcard_imports,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::needless_collect,
    clippy::format_collect,
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use aios_integration::*;
use chrono::Utc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_record(id: &str, score: f32) -> CveRecord {
    CveRecord {
        cve_id: CveId(id.to_string()),
        published_at: Utc::now(),
        last_modified_at: Utc::now(),
        cvss_v3_score: score,
        severity: if score >= 9.0 {
            CveSeverity::Critical
        } else if score >= 7.0 {
            CveSeverity::High
        } else if score >= 4.0 {
            CveSeverity::Medium
        } else {
            CveSeverity::Low
        },
        summary: format!("Test {id}"),
        affected_cpe_uris: vec!["cpe:2.3:*:*:*:*:*:*:*:*:*:*".into()],
    }
}

fn make_binding(
    binding_id: &str,
    cve_id: &str,
    package_id: &str,
    status: CveStatus,
) -> PackageCveBinding {
    PackageCveBinding {
        binding_id: binding_id.to_string(),
        cve_id: CveId(cve_id.to_string()),
        package_id: package_id.to_string(),
        status,
        bound_at: Utc::now(),
        matched_via_cpe: None,
        mitigated_by: None,
    }
}

// ---------------------------------------------------------------------------
// cvss_to_enforcement
// ---------------------------------------------------------------------------

#[test]
fn cvss_to_enforcement_below_4_returns_monitor_only() {
    assert_eq!(cvss_to_enforcement(0.0), CveEnforcementLevel::MonitorOnly);
    assert_eq!(cvss_to_enforcement(1.0), CveEnforcementLevel::MonitorOnly);
    assert_eq!(cvss_to_enforcement(3.9), CveEnforcementLevel::MonitorOnly);
}

#[test]
fn cvss_to_enforcement_at_4_returns_operator_notify() {
    assert_eq!(
        cvss_to_enforcement(4.0),
        CveEnforcementLevel::OperatorNotify
    );
    assert_eq!(
        cvss_to_enforcement(5.5),
        CveEnforcementLevel::OperatorNotify
    );
    assert_eq!(
        cvss_to_enforcement(6.9),
        CveEnforcementLevel::OperatorNotify
    );
}

#[test]
fn cvss_to_enforcement_at_7_returns_quarantine_candidate() {
    assert_eq!(
        cvss_to_enforcement(7.0),
        CveEnforcementLevel::QuarantineCandidate
    );
    assert_eq!(
        cvss_to_enforcement(8.0),
        CveEnforcementLevel::QuarantineCandidate
    );
    assert_eq!(
        cvss_to_enforcement(8.9),
        CveEnforcementLevel::QuarantineCandidate
    );
}

#[test]
fn cvss_to_enforcement_at_9_returns_auto_quarantine() {
    assert_eq!(
        cvss_to_enforcement(9.0),
        CveEnforcementLevel::AutoQuarantine
    );
    assert_eq!(
        cvss_to_enforcement(9.5),
        CveEnforcementLevel::AutoQuarantine
    );
}

#[test]
fn cvss_to_enforcement_at_10_returns_auto_quarantine() {
    assert_eq!(
        cvss_to_enforcement(10.0),
        CveEnforcementLevel::AutoQuarantine
    );
}

#[test]
fn cvss_to_enforcement_ordering_is_monotonic() {
    assert!(CveEnforcementLevel::MonitorOnly < CveEnforcementLevel::OperatorNotify);
    assert!(CveEnforcementLevel::OperatorNotify < CveEnforcementLevel::QuarantineCandidate);
    assert!(CveEnforcementLevel::QuarantineCandidate < CveEnforcementLevel::AutoQuarantine);
}

// ---------------------------------------------------------------------------
// is_valid_cve_id
// ---------------------------------------------------------------------------

#[test]
fn is_valid_cve_id_for_well_formed_succeeds() {
    assert!(is_valid_cve_id("CVE-2024-12345"));
    assert!(is_valid_cve_id("CVE-1999-0001"));
    assert!(is_valid_cve_id("CVE-2025-999999"));
}

#[test]
fn is_valid_cve_id_for_missing_prefix_fails() {
    assert!(!is_valid_cve_id("CV-2024-12345"));
    assert!(!is_valid_cve_id("cve-2024-12345"));
    assert!(!is_valid_cve_id("2024-12345"));
}

#[test]
fn is_valid_cve_id_for_short_suffix_fails() {
    assert!(!is_valid_cve_id("CVE-2024-123"));
    assert!(!is_valid_cve_id("CVE-2024-1"));
    assert!(!is_valid_cve_id("CVE-2024-"));
}

#[test]
fn is_valid_cve_id_for_bad_year_fails() {
    assert!(!is_valid_cve_id("CVE-20x4-12345"));
    assert!(!is_valid_cve_id("CVE-202-12345"));
    assert!(!is_valid_cve_id("CVE-20245-12345"));
}

#[test]
fn is_valid_cve_id_for_empty_suffix_fails() {
    assert!(!is_valid_cve_id("CVE-2024-"));
    assert!(!is_valid_cve_id("CVE-"));
    assert!(!is_valid_cve_id(""));
}

// ---------------------------------------------------------------------------
// CveFeedShape — record ingestion
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ingest_record_with_valid_cvss_succeeds() {
    let shape = CveFeedShape::new();
    let record = make_record("CVE-2024-12345", 7.5);
    shape.ingest_record(record.clone()).await.unwrap();
    let got = shape.get_record(&record.cve_id).await.unwrap();
    assert_eq!(got.cve_id, record.cve_id);
    assert_eq!(got.cvss_v3_score, 7.5);
}

#[tokio::test]
async fn ingest_record_with_cvss_above_10_returns_config_invalid() {
    let shape = CveFeedShape::new();
    let record = make_record("CVE-2024-99999", 10.1);
    let err = shape.ingest_record(record).await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("CVSS") || msg.contains("range"), "got: {msg}");
}

#[tokio::test]
async fn ingest_record_with_invalid_id_format_returns_config_invalid() {
    let shape = CveFeedShape::new();
    let record = make_record("NOT-A-CVE", 5.0);
    let err = shape.ingest_record(record).await.unwrap_err();
    let msg = err.to_string();
    assert!(!msg.is_empty());
}

#[tokio::test]
async fn ingest_record_replaces_prior_for_same_id() {
    let shape = CveFeedShape::new();
    let id = CveId("CVE-2024-88888".into());

    let r1 = make_record("CVE-2024-88888", 3.0);
    shape.ingest_record(r1).await.unwrap();

    let r2 = make_record("CVE-2024-88888", 9.5);
    shape.ingest_record(r2).await.unwrap();

    let got = shape.get_record(&id).await.unwrap();
    assert_eq!(got.cvss_v3_score, 9.5);
}

#[tokio::test]
async fn ingest_record_with_cvss_below_0_returns_config_invalid() {
    let shape = CveFeedShape::new();
    let record = make_record("CVE-2024-00001", -0.1);
    let err = shape.ingest_record(record).await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("CVSS") || msg.contains("range"), "got: {msg}");
}

// ---------------------------------------------------------------------------
// CveFeedShape — list_records / get_record
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_records_returns_all_ingested() {
    let shape = CveFeedShape::new();
    for i in 0..5 {
        let id = format!("CVE-2024-{i:05}");
        shape
            .ingest_record(make_record(&id, (i as f32) + 1.0))
            .await
            .unwrap();
    }
    let records = shape.list_records().await;
    assert_eq!(records.len(), 5);
}

#[tokio::test]
async fn get_record_for_unknown_cve_returns_none() {
    let shape = CveFeedShape::new();
    assert!(shape
        .get_record(&CveId("CVE-9999-99999".into()))
        .await
        .is_none());
}

// ---------------------------------------------------------------------------
// CveFeedShape — package binding
// ---------------------------------------------------------------------------

#[tokio::test]
async fn bind_to_package_with_known_cve_succeeds() {
    let shape = CveFeedShape::new();
    shape
        .ingest_record(make_record("CVE-2024-11111", 5.0))
        .await
        .unwrap();

    let binding = make_binding("B-001", "CVE-2024-11111", "PKG-A", CveStatus::Open);
    shape.bind_to_package(binding).await.unwrap();

    let bindings = shape.list_bindings().await;
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].binding_id, "B-001");
}

#[tokio::test]
async fn bind_to_package_with_unknown_cve_returns_internal_error() {
    let shape = CveFeedShape::new();
    let binding = make_binding("B-002", "CVE-2024-22222", "PKG-B", CveStatus::Open);
    let err = shape.bind_to_package(binding).await.unwrap_err();
    let msg = err.to_string();
    assert!(!msg.is_empty());
}

#[tokio::test]
async fn list_bindings_for_package_returns_all_matching() {
    let shape = CveFeedShape::new();

    shape
        .ingest_record(make_record("CVE-2024-33331", 3.0))
        .await
        .unwrap();
    shape
        .ingest_record(make_record("CVE-2024-33332", 4.0))
        .await
        .unwrap();

    shape
        .bind_to_package(make_binding(
            "B-A1",
            "CVE-2024-33331",
            "PKG-A",
            CveStatus::Open,
        ))
        .await
        .unwrap();
    shape
        .bind_to_package(make_binding(
            "B-A2",
            "CVE-2024-33332",
            "PKG-A",
            CveStatus::UnderReview,
        ))
        .await
        .unwrap();
    shape
        .bind_to_package(make_binding(
            "B-B1",
            "CVE-2024-33331",
            "PKG-B",
            CveStatus::Open,
        ))
        .await
        .unwrap();

    let pkg_a = shape.list_bindings_for_package("PKG-A").await;
    assert_eq!(pkg_a.len(), 2);

    let pkg_b = shape.list_bindings_for_package("PKG-B").await;
    assert_eq!(pkg_b.len(), 1);
}

#[tokio::test]
async fn list_bindings_returns_all() {
    let shape = CveFeedShape::new();
    shape
        .ingest_record(make_record("CVE-2024-55555", 5.0))
        .await
        .unwrap();

    shape
        .bind_to_package(make_binding(
            "B-1",
            "CVE-2024-55555",
            "PKG-1",
            CveStatus::Open,
        ))
        .await
        .unwrap();
    shape
        .bind_to_package(make_binding(
            "B-2",
            "CVE-2024-55555",
            "PKG-2",
            CveStatus::Open,
        ))
        .await
        .unwrap();

    let all = shape.list_bindings().await;
    assert_eq!(all.len(), 2);
}

// ---------------------------------------------------------------------------
// CveFeedShape — enforcement
// ---------------------------------------------------------------------------

#[tokio::test]
async fn enforcement_level_for_high_cvss_returns_auto_quarantine() {
    let shape = CveFeedShape::new();
    let id = CveId("CVE-2024-99999".into());
    shape
        .ingest_record(make_record("CVE-2024-99999", 9.5))
        .await
        .unwrap();

    let level = shape.enforcement_level_for(&id).await.unwrap();
    assert_eq!(level, CveEnforcementLevel::AutoQuarantine);
}

#[tokio::test]
async fn enforcement_level_for_unknown_cve_returns_none() {
    let shape = CveFeedShape::new();
    assert!(shape
        .enforcement_level_for(&CveId("CVE-9999-99999".into()))
        .await
        .is_none());
}

#[tokio::test]
async fn list_packages_at_or_above_quarantine_candidate_returns_correct_set() {
    let shape = CveFeedShape::new();

    // PKG-A: CVSS 7.0 — QuarantineCandidate
    shape
        .ingest_record(make_record("CVE-2024-70001", 7.0))
        .await
        .unwrap();
    shape
        .bind_to_package(make_binding(
            "B-A",
            "CVE-2024-70001",
            "PKG-A",
            CveStatus::Open,
        ))
        .await
        .unwrap();

    // PKG-B: CVSS 9.5 — AutoQuarantine
    shape
        .ingest_record(make_record("CVE-2024-90001", 9.5))
        .await
        .unwrap();
    shape
        .bind_to_package(make_binding(
            "B-B",
            "CVE-2024-90001",
            "PKG-B",
            CveStatus::Open,
        ))
        .await
        .unwrap();

    // PKG-C: CVSS 3.0 — MonitorOnly (below threshold)
    shape
        .ingest_record(make_record("CVE-2024-30001", 3.0))
        .await
        .unwrap();
    shape
        .bind_to_package(make_binding(
            "B-C",
            "CVE-2024-30001",
            "PKG-C",
            CveStatus::Open,
        ))
        .await
        .unwrap();

    let packages = shape
        .list_packages_at_or_above(CveEnforcementLevel::QuarantineCandidate)
        .await;
    assert_eq!(packages.len(), 2);
    assert!(packages.contains(&"PKG-A".to_string()));
    assert!(packages.contains(&"PKG-B".to_string()));
    assert!(!packages.contains(&"PKG-C".to_string()));
}

// ---------------------------------------------------------------------------
// CveFeedShape — update_binding_status
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_binding_status_to_patched_then_lookup_returns_patched() {
    let shape = CveFeedShape::new();
    shape
        .ingest_record(make_record("CVE-2024-44444", 6.0))
        .await
        .unwrap();

    let binding = make_binding("B-PATCH", "CVE-2024-44444", "PKG-X", CveStatus::Open);
    shape.bind_to_package(binding).await.unwrap();

    shape
        .update_binding_status("B-PATCH", CveStatus::Patched)
        .await
        .unwrap();

    let bindings = shape.list_bindings_for_package("PKG-X").await;
    assert_eq!(bindings[0].status, CveStatus::Patched);
}

#[tokio::test]
async fn update_binding_status_for_unknown_id_returns_internal_error() {
    let shape = CveFeedShape::new();
    let err = shape
        .update_binding_status("NONEXISTENT", CveStatus::Patched)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(!msg.is_empty());
}

// ---------------------------------------------------------------------------
// CveFeedShape — unbind
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unbind_then_list_for_package_excludes_it() {
    let shape = CveFeedShape::new();
    shape
        .ingest_record(make_record("CVE-2024-66666", 4.5))
        .await
        .unwrap();

    let binding = make_binding("B-REM", "CVE-2024-66666", "PKG-REM", CveStatus::Open);
    shape.bind_to_package(binding).await.unwrap();

    assert_eq!(shape.list_bindings_for_package("PKG-REM").await.len(), 1);

    shape.unbind("B-REM").await.unwrap();

    assert!(shape.list_bindings_for_package("PKG-REM").await.is_empty());
}

#[tokio::test]
async fn unbind_nonexistent_id_is_noop() {
    let shape = CveFeedShape::new();
    shape.unbind("NONEXISTENT").await.unwrap();
}

// ---------------------------------------------------------------------------
// CveFeedShape — concurrent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_ingest_5_distinct_records_no_panic() {
    use std::sync::Arc;

    let shape = Arc::new(CveFeedShape::new());
    let mut handles = Vec::new();

    for i in 0..5 {
        let shape = Arc::clone(&shape);
        handles.push(tokio::spawn(async move {
            let id = format!("CVE-2025-{i:05}");
            let score = (i as f32).mul_add(2.0, 1.0);
            let record = make_record(&id, score);
            shape.ingest_record(record).await.unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let records = shape.list_records().await;
    assert_eq!(records.len(), 5);
}
