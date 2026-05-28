use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::error::IntegrationError;
use crate::standard::StandardKind;

/// An AIOS invariant from the specification (INV-001 .. INV-024).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiosInvariant {
    /// Identifier of the form `INV-001` .. `INV-024`.
    pub invariant_id: String,
    /// Human-readable short name.
    pub name: String,
    /// Layer identifier (L0 .. L10).
    pub layer: String,
}

/// A reference into an external control framework, e.g. NIST 800-53 AC-3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlFrameworkRef {
    /// Which standard or framework.
    pub framework: StandardKind,
    /// Control family within the framework (e.g. "AC", "SC").
    pub control_family: String,
    /// Specific control / check identifier (e.g. "AC-3", "SC-7").
    pub control_id: String,
}

/// Maps one AIOS invariant to one or more external control references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlMapping {
    /// Unique mapping identifier.
    pub mapping_id: String,
    /// The AIOS-side invariant.
    pub invariant: AiosInvariant,
    /// The external control framework references.
    pub control_refs: Vec<ControlFrameworkRef>,
    /// Human-readable rationale for this mapping.
    pub mapping_rationale: String,
    /// When this mapping was established.
    pub mapped_at: DateTime<Utc>,
}

/// An immutable compliance baseline snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComplianceBaseline {
    /// Unique baseline identifier.
    pub baseline_id: String,
    /// The aios-integration version this baseline was taken against.
    pub aios_version: String,
    /// All control mappings captured at snapshot time.
    pub mappings: Vec<ControlMapping>,
    /// UTC timestamp of the snapshot.
    pub snapshot_at: DateTime<Utc>,
    /// Canonical identity of the validator that requested the snapshot.
    pub validator_canonical_id: String,
}

/// Drift report comparing a prior baseline to the current mapping state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlDriftReport {
    /// Baseline ID of the prior snapshot being compared.
    pub prior_baseline_id: String,
    /// Mapping IDs that exist now but were absent in the prior baseline.
    pub added: Vec<String>,
    /// Mapping IDs that existed in the prior baseline but are now absent.
    pub removed: Vec<String>,
    /// Mapping IDs that exist in both but differ in content.
    pub modified: Vec<String>,
    /// Count of mapping IDs that are identical across both snapshots.
    pub unchanged_count: usize,
}

/// Registry that tracks AIOS-invariant ↔ external control framework mappings
/// and produces immutable compliance baseline snapshots with drift detection.
#[derive(Debug)]
pub struct ControlMapRegistry {
    /// Active control mappings keyed by mapping_id.  Exposed as pub(crate) so
    /// integration tests can directly mutate the map for drift scenarios.
    #[allow(clippy::doc_markdown)]
    pub mappings: RwLock<HashMap<String, ControlMapping>>,
    baselines: RwLock<HashMap<String, ComplianceBaseline>>,
}

impl ControlMapRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            mappings: RwLock::new(HashMap::new()),
            baselines: RwLock::new(HashMap::new()),
        }
    }

    /// Adds a control mapping.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError::Internal`] when `mapping_id` already exists.
    #[allow(clippy::unused_async)]
    pub async fn add_mapping(&self, m: ControlMapping) -> Result<(), IntegrationError> {
        let mut guard = self.mappings.write().await;
        if guard.contains_key(&m.mapping_id) {
            return Err(IntegrationError::Internal(format!(
                "mapping_id {} exists",
                m.mapping_id
            )));
        }
        guard.insert(m.mapping_id.clone(), m);
        // Explicit drop to satisfy clippy::significant_drop_tightening.
        drop(guard);
        Ok(())
    }

    /// Collects every current mapping into an immutable baseline snapshot,
    /// stores it, and returns it.
    #[allow(clippy::unused_async, clippy::missing_errors_doc)]
    pub async fn snapshot_baseline(
        &self,
        baseline_id: String,
        aios_version: String,
        validator: String,
    ) -> Result<ComplianceBaseline, IntegrationError> {
        let mappings: Vec<ControlMapping> = {
            let guard = self.mappings.read().await;
            guard.values().cloned().collect()
        };

        let baseline = ComplianceBaseline {
            baseline_id: baseline_id.clone(),
            aios_version,
            mappings,
            snapshot_at: Utc::now(),
            validator_canonical_id: validator,
        };

        let mut guard = self.baselines.write().await;
        guard.insert(baseline_id, baseline.clone());
        drop(guard);
        Ok(baseline)
    }

    /// Lists all mappings that reference the given invariant ID.
    #[allow(clippy::unused_async)]
    pub async fn list_mappings_for_invariant(&self, invariant_id: &str) -> Vec<ControlMapping> {
        let guard = self.mappings.read().await;
        guard
            .values()
            .filter(|m| m.invariant.invariant_id == invariant_id)
            .cloned()
            .collect()
    }

    /// Lists all mappings that reference the given standard framework.
    #[allow(clippy::unused_async)]
    pub async fn list_mappings_for_framework(
        &self,
        framework: StandardKind,
    ) -> Vec<ControlMapping> {
        let guard = self.mappings.read().await;
        guard
            .values()
            .filter(|m| m.control_refs.iter().any(|r| r.framework == framework))
            .cloned()
            .collect()
    }

    /// Computes drift between a prior baseline and the current mapping state.
    #[allow(clippy::unused_async)]
    pub async fn detect_drift(&self, prior: &ComplianceBaseline) -> ControlDriftReport {
        let guard = self.mappings.read().await;

        let prior_map: HashMap<&str, &ControlMapping> = prior
            .mappings
            .iter()
            .map(|m| (m.mapping_id.as_str(), m))
            .collect();

        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut modified = Vec::new();
        let mut unchanged_count = 0_usize;

        for (id, current) in guard.iter() {
            if let Some(prior_mapping) = prior_map.get(id.as_str()) {
                if current == *prior_mapping {
                    unchanged_count = unchanged_count.wrapping_add(1);
                } else {
                    modified.push(id.clone());
                }
            } else {
                added.push(id.clone());
            }
        }

        for id in prior_map.keys() {
            if !guard.contains_key(*id) {
                removed.push((*id).to_string());
            }
        }

        drop(guard);

        // Stabilise ordering for deterministic test assertions.
        added.sort();
        removed.sort();
        modified.sort();

        ControlDriftReport {
            prior_baseline_id: prior.baseline_id.clone(),
            added,
            removed,
            modified,
            unchanged_count,
        }
    }

    /// Retrieves a baseline by its identifier.
    #[allow(clippy::unused_async)]
    pub async fn get_baseline(&self, baseline_id: &str) -> Option<ComplianceBaseline> {
        let guard = self.baselines.read().await;
        guard.get(baseline_id).cloned()
    }
}

impl Default for ControlMapRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_in_result,
    clippy::missing_errors_doc
)]
mod tests {
    use super::*;

    fn make_invariant(id: &str, name: &str, layer: &str) -> AiosInvariant {
        AiosInvariant {
            invariant_id: id.into(),
            name: name.into(),
            layer: layer.into(),
        }
    }

    fn make_mapping(
        id: &str,
        inv_id: &str,
        framework: StandardKind,
        rationale: &str,
    ) -> ControlMapping {
        ControlMapping {
            mapping_id: id.into(),
            invariant: make_invariant(inv_id, &format!("Invariant {inv_id}"), "L4"),
            control_refs: vec![ControlFrameworkRef {
                framework,
                control_family: "AC".into(),
                control_id: "AC-3".into(),
            }],
            mapping_rationale: rationale.into(),
            mapped_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn add_mapping_then_list_returns_one() {
        let r = ControlMapRegistry::new();
        let m = make_mapping(
            "MAP-001",
            "INV-001",
            StandardKind::Nist80053Rev5,
            "access enforcement",
        );
        r.add_mapping(m).await.unwrap();
        let list = r.list_mappings_for_invariant("INV-001").await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].mapping_id, "MAP-001");
    }

    #[tokio::test]
    async fn add_duplicate_mapping_id_returns_internal_error() {
        let r = ControlMapRegistry::new();
        let m = make_mapping("MAP-001", "INV-001", StandardKind::Nist80053Rev5, "r1");
        r.add_mapping(m.clone()).await.unwrap();
        let err = r.add_mapping(m).await.unwrap_err();
        assert!(matches!(err, IntegrationError::Internal(msg) if msg.contains("exists")));
    }

    #[tokio::test]
    async fn list_mappings_for_unknown_invariant_returns_empty() {
        let r = ControlMapRegistry::new();
        let list = r.list_mappings_for_invariant("NONEXISTENT").await;
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn list_mappings_for_framework_filters_by_standard_kind() {
        let r = ControlMapRegistry::new();
        r.add_mapping(make_mapping(
            "MAP-NIST",
            "INV-001",
            StandardKind::Nist80053Rev5,
            "nist",
        ))
        .await
        .unwrap();
        r.add_mapping(make_mapping(
            "MAP-CIS",
            "INV-002",
            StandardKind::CisControlsV8,
            "cis",
        ))
        .await
        .unwrap();

        let nist = r
            .list_mappings_for_framework(StandardKind::Nist80053Rev5)
            .await;
        assert_eq!(nist.len(), 1);
        assert_eq!(nist[0].mapping_id, "MAP-NIST");

        let cis = r
            .list_mappings_for_framework(StandardKind::CisControlsV8)
            .await;
        assert_eq!(cis.len(), 1);
        assert_eq!(cis[0].mapping_id, "MAP-CIS");
    }

    #[tokio::test]
    async fn snapshot_baseline_captures_all_current_mappings() {
        let r = ControlMapRegistry::new();
        r.add_mapping(make_mapping(
            "MAP-A",
            "INV-001",
            StandardKind::Nist80053Rev5,
            "a",
        ))
        .await
        .unwrap();
        r.add_mapping(make_mapping(
            "MAP-B",
            "INV-002",
            StandardKind::CisControlsV8,
            "b",
        ))
        .await
        .unwrap();

        let baseline = r
            .snapshot_baseline("BL-001".into(), "0.0.1".into(), "v1".into())
            .await
            .unwrap();
        assert_eq!(baseline.baseline_id, "BL-001");
        assert_eq!(baseline.aios_version, "0.0.1");
        assert_eq!(baseline.validator_canonical_id, "v1");
        assert_eq!(baseline.mappings.len(), 2);
    }

    #[tokio::test]
    async fn snapshot_baseline_two_in_a_row_have_distinct_ids() {
        let r = ControlMapRegistry::new();
        let b1 = r
            .snapshot_baseline("BL-A".into(), "0.0.1".into(), "v1".into())
            .await
            .unwrap();
        let b2 = r
            .snapshot_baseline("BL-B".into(), "0.0.1".into(), "v1".into())
            .await
            .unwrap();
        assert_ne!(b1.baseline_id, b2.baseline_id);
    }

    #[tokio::test]
    async fn get_baseline_known_returns_some() {
        let r = ControlMapRegistry::new();
        r.snapshot_baseline("BL-X".into(), "0.0.1".into(), "v1".into())
            .await
            .unwrap();
        let found = r.get_baseline("BL-X").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().baseline_id, "BL-X");
    }

    #[tokio::test]
    async fn get_baseline_unknown_returns_none() {
        let r = ControlMapRegistry::new();
        assert!(r.get_baseline("NO-SUCH").await.is_none());
    }

    #[tokio::test]
    async fn detect_drift_with_identical_mappings_returns_empty_diff() {
        let r = ControlMapRegistry::new();
        r.add_mapping(make_mapping(
            "MAP-A",
            "INV-001",
            StandardKind::Nist80053Rev5,
            "rationale",
        ))
        .await
        .unwrap();

        let _baseline = r
            .snapshot_baseline("BL-1".into(), "0.0.1".into(), "v1".into())
            .await
            .unwrap();
        // Re-open the baseline to avoid holding the RwLock across async.
        let prior = r.get_baseline("BL-1").await.unwrap();

        let drift = r.detect_drift(&prior).await;
        assert!(drift.added.is_empty());
        assert!(drift.removed.is_empty());
        assert!(drift.modified.is_empty());
        assert_eq!(drift.unchanged_count, 1);
    }

    #[tokio::test]
    async fn detect_drift_with_added_mapping_returns_one_added() {
        let r = ControlMapRegistry::new();
        r.add_mapping(make_mapping(
            "MAP-A",
            "INV-001",
            StandardKind::Nist80053Rev5,
            "a",
        ))
        .await
        .unwrap();
        let prior = r
            .snapshot_baseline("BL-1".into(), "0.0.1".into(), "v1".into())
            .await
            .unwrap();

        // Add another mapping after the snapshot
        r.add_mapping(make_mapping(
            "MAP-B",
            "INV-002",
            StandardKind::CisControlsV8,
            "b",
        ))
        .await
        .unwrap();

        let drift = r.detect_drift(&prior).await;
        assert_eq!(drift.added, vec!["MAP-B"]);
        assert!(drift.removed.is_empty());
        assert!(drift.modified.is_empty());
    }

    #[tokio::test]
    async fn detect_drift_with_removed_mapping_returns_one_removed() {
        let r = ControlMapRegistry::new();
        r.add_mapping(make_mapping(
            "MAP-A",
            "INV-001",
            StandardKind::Nist80053Rev5,
            "a",
        ))
        .await
        .unwrap();
        r.add_mapping(make_mapping(
            "MAP-B",
            "INV-002",
            StandardKind::CisControlsV8,
            "b",
        ))
        .await
        .unwrap();
        let prior = r
            .snapshot_baseline("BL-1".into(), "0.0.1".into(), "v1".into())
            .await
            .unwrap();

        // Remove MAP-B from current state
        let mut guard = r.mappings.write().await;
        guard.remove("MAP-B");
        drop(guard);

        let drift = r.detect_drift(&prior).await;
        assert_eq!(drift.removed, vec!["MAP-B"]);
        assert!(drift.added.is_empty());
        assert!(drift.modified.is_empty());
    }

    #[tokio::test]
    async fn detect_drift_with_modified_rationale_returns_one_modified() {
        let r = ControlMapRegistry::new();
        r.add_mapping(make_mapping(
            "MAP-A",
            "INV-001",
            StandardKind::Nist80053Rev5,
            "original rationale",
        ))
        .await
        .unwrap();
        let prior = r
            .snapshot_baseline("BL-1".into(), "0.0.1".into(), "v1".into())
            .await
            .unwrap();

        // Mutate the mapping's rationale
        let mut guard = r.mappings.write().await;
        if let Some(m) = guard.get_mut("MAP-A") {
            m.mapping_rationale = "updated rationale".into();
        }
        drop(guard);

        let drift = r.detect_drift(&prior).await;
        assert_eq!(drift.modified, vec!["MAP-A"]);
        assert!(drift.added.is_empty());
        assert!(drift.removed.is_empty());
    }

    #[tokio::test]
    async fn detect_drift_unchanged_count_correct() {
        let r = ControlMapRegistry::new();
        r.add_mapping(make_mapping(
            "MAP-UNCHANGED",
            "INV-001",
            StandardKind::Nist80053Rev5,
            "same",
        ))
        .await
        .unwrap();
        r.add_mapping(make_mapping(
            "MAP-ADDED",
            "INV-002",
            StandardKind::CisControlsV8,
            "new",
        ))
        .await
        .unwrap();
        let prior_snapshot = r
            .snapshot_baseline("BL-1".into(), "0.0.1".into(), "v1".into())
            .await
            .unwrap();

        // Remove MAP-ADDED and add MAP-THIRD after the snapshot
        {
            let mut guard = r.mappings.write().await;
            guard.remove("MAP-ADDED");
        }
        r.add_mapping(make_mapping(
            "MAP-THIRD",
            "INV-003",
            StandardKind::Nist80053Rev5,
            "third",
        ))
        .await
        .unwrap();

        let drift = r.detect_drift(&prior_snapshot).await;
        assert_eq!(drift.added, vec!["MAP-THIRD"]);
        assert_eq!(drift.removed, vec!["MAP-ADDED"]);
        assert!(drift.modified.is_empty());
        assert_eq!(drift.unchanged_count, 1);
    }

    #[tokio::test]
    async fn aios_invariant_serde_round_trip() {
        let inv = make_invariant("INV-007", "Secrets Are Capabilities", "L4");
        let json = serde_json::to_string(&inv).unwrap();
        let back: AiosInvariant = serde_json::from_str(&json).unwrap();
        assert_eq!(inv, back);
    }

    #[tokio::test]
    async fn compliance_baseline_serde_round_trip() {
        let baseline = ComplianceBaseline {
            baseline_id: "BL-X".into(),
            aios_version: "0.1.0".into(),
            mappings: vec![make_mapping(
                "MAP-C",
                "INV-003",
                StandardKind::Nist80053Rev5,
                "rationale",
            )],
            snapshot_at: Utc::now(),
            validator_canonical_id: "auditor-42".into(),
        };
        let json = serde_json::to_string(&baseline).unwrap();
        let back: ComplianceBaseline = serde_json::from_str(&json).unwrap();
        assert_eq!(baseline, back);
    }
}
