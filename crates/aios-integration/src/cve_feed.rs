use std::collections::HashMap;
use std::sync::RwLock;

use chrono::{DateTime, Utc};

use crate::cve::{CveId, CveSeverity, CveStatus};
use crate::error::IntegrationError;

// ---------------------------------------------------------------------------
// CveRecord
// ---------------------------------------------------------------------------

/// A CVE record ingested from a feed source (NVD, GitHub Advisory, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct CveRecord {
    /// CVE identifier (e.g. `CVE-2024-12345`).
    pub cve_id: CveId,
    /// UTC timestamp when the CVE was first published.
    pub published_at: DateTime<Utc>,
    /// UTC timestamp of the most recent modification.
    pub last_modified_at: DateTime<Utc>,
    /// CVSS v3 base score; must be in `0.0..=10.0`.
    pub cvss_v3_score: f32,
    /// Severity classification derived from CVSS score.
    pub severity: CveSeverity,
    /// Human-readable description of the vulnerability.
    pub summary: String,
    /// CPE 2.3 URIs affected by this CVE.
    pub affected_cpe_uris: Vec<String>,
}

// ---------------------------------------------------------------------------
// PackageCveBinding
// ---------------------------------------------------------------------------

/// Links a CVE identifier to an opaque AIOS package identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCveBinding {
    /// Opaque binding identifier.
    pub binding_id: String,
    /// CVE identifier this binding references.
    pub cve_id: CveId,
    /// Opaque AIOS package id.
    pub package_id: String,
    /// Current remediation status for this binding.
    pub status: CveStatus,
    /// UTC timestamp when this binding was created.
    pub bound_at: DateTime<Utc>,
    /// CPE 2.3 URI that matched this binding, if any.
    pub matched_via_cpe: Option<String>,
    /// Human-readable mitigation note (e.g. "patched in version X.Y.Z").
    pub mitigated_by: Option<String>,
}

// ---------------------------------------------------------------------------
// CveEnforcementLevel
// ---------------------------------------------------------------------------

/// Enforcement action derived from CVSS v3 score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CveEnforcementLevel {
    /// Notify but do not block (CVSS < 4.0).
    MonitorOnly,
    /// Emit warning evidence (4.0 ≤ CVSS < 7.0).
    OperatorNotify,
    /// Propose quarantine, await operator approval (7.0 ≤ CVSS < 9.0).
    QuarantineCandidate,
    /// Quarantine immediately, FOREVER evidence (CVSS ≥ 9.0).
    AutoQuarantine,
}

/// Map a CVSS v3 base score to the corresponding enforcement level.
#[must_use]
pub const fn cvss_to_enforcement(score: f32) -> CveEnforcementLevel {
    if score >= 9.0 {
        CveEnforcementLevel::AutoQuarantine
    } else if score >= 7.0 {
        CveEnforcementLevel::QuarantineCandidate
    } else if score >= 4.0 {
        CveEnforcementLevel::OperatorNotify
    } else {
        CveEnforcementLevel::MonitorOnly
    }
}

// ---------------------------------------------------------------------------
// CVE id validation (dependency-free)
// ---------------------------------------------------------------------------

/// Returns `true` when `id` matches the pattern `CVE-YYYY-N+` (year 4 digits,
/// suffix at least 4 digits). Uses only `str` primitives — no regex.
#[must_use]
pub fn is_valid_cve_id(id: &str) -> bool {
    if !id.starts_with("CVE-") {
        return false;
    }
    let rest = &id[4..];
    let Some(dash_pos) = rest.find('-') else {
        return false;
    };
    let year = &rest[..dash_pos];
    let suffix = &rest[dash_pos + 1..];
    year.len() == 4
        && year.chars().all(|c| c.is_ascii_digit())
        && suffix.len() >= 4
        && suffix.chars().all(|c| c.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// Poison helper
// ---------------------------------------------------------------------------

fn lock_poisoned() -> IntegrationError {
    IntegrationError::Internal("lock poisoned".into())
}

// ---------------------------------------------------------------------------
// CveFeedShape
// ---------------------------------------------------------------------------

/// In-memory typed framework for CVE ingestion and package binding.
///
/// Stores CVE records, package-to-CVE bindings, and indexes for
/// efficient enforcement-level lookups.
pub struct CveFeedShape {
    records: RwLock<HashMap<CveId, CveRecord>>,
    bindings: RwLock<HashMap<String, PackageCveBinding>>,
    by_package: RwLock<HashMap<String, Vec<String>>>,
}

#[allow(clippy::unused_async)]
impl CveFeedShape {
    /// Create an empty `CveFeedShape`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
            bindings: RwLock::new(HashMap::new()),
            by_package: RwLock::new(HashMap::new()),
        }
    }

    // -- record ingestion ---------------------------------------------------

    /// Ingest (or revise) a CVE record.
    ///
    /// Validates the CVSS score (`0.0..=10.0`) and CVE id format. Replaces
    /// any prior record for the same `cve_id` — idempotent revision.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError::ConfigInvalid`] if the CVSS score is
    /// outside `0.0..=10.0` or the CVE id format is invalid.
    pub async fn ingest_record(&self, record: CveRecord) -> Result<(), IntegrationError> {
        if record.cvss_v3_score < 0.0 || record.cvss_v3_score > 10.0 {
            return Err(IntegrationError::ConfigInvalid(
                "CVSS score out of range".into(),
            ));
        }
        if !is_valid_cve_id(&record.cve_id.0) {
            return Err(IntegrationError::ConfigInvalid(
                "invalid CVE id format".into(),
            ));
        }
        let mut records = self.records.write().map_err(|_| lock_poisoned())?;
        records.insert(record.cve_id.clone(), record);
        drop(records);
        Ok(())
    }

    /// Return a clone of the record for `cve_id`, if present.
    pub async fn get_record(&self, cve_id: &CveId) -> Option<CveRecord> {
        let records = self.records.read().ok()?;
        let result = records.get(cve_id).cloned();
        drop(records);
        result
    }

    /// List all ingested CVE records.
    pub async fn list_records(&self) -> Vec<CveRecord> {
        let Ok(records) = self.records.read() else {
            return Vec::new();
        };
        let result = records.values().cloned().collect();
        drop(records);
        result
    }

    // -- package binding ----------------------------------------------------

    /// Bind a CVE record to an AIOS package.
    ///
    /// The CVE record must already be ingested. On success the binding is
    /// stored and the per-package index is updated.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError::Internal`] if the CVE id is not found
    /// in the ingested records.
    pub async fn bind_to_package(
        &self,
        binding: PackageCveBinding,
    ) -> Result<(), IntegrationError> {
        {
            let records = self.records.read().map_err(|_| lock_poisoned())?;
            if !records.contains_key(&binding.cve_id) {
                return Err(IntegrationError::Internal("unknown CVE id".into()));
            }
        }
        let binding_id = binding.binding_id.clone();
        let package_id = binding.package_id.clone();
        {
            let mut bindings = self.bindings.write().map_err(|_| lock_poisoned())?;
            bindings.insert(binding_id.clone(), binding);
        }
        {
            let mut by_pkg = self.by_package.write().map_err(|_| lock_poisoned())?;
            by_pkg.entry(package_id).or_default().push(binding_id);
        }
        Ok(())
    }

    /// Remove a binding by id.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError`] only on lock poison; missing bindings
    /// are silently ignored.
    pub async fn unbind(&self, binding_id: &str) -> Result<(), IntegrationError> {
        let removed = {
            let mut bindings = self.bindings.write().map_err(|_| lock_poisoned())?;
            bindings.remove(binding_id)
        };
        if let Some(ref binding) = removed {
            let mut by_pkg = self.by_package.write().map_err(|_| lock_poisoned())?;
            if let Some(ids) = by_pkg.get_mut(&binding.package_id) {
                ids.retain(|id| id != binding_id);
            }
        }
        Ok(())
    }

    /// List all bindings.
    pub async fn list_bindings(&self) -> Vec<PackageCveBinding> {
        let Ok(bindings) = self.bindings.read() else {
            return Vec::new();
        };
        let result = bindings.values().cloned().collect();
        drop(bindings);
        result
    }

    /// List all bindings for a given package.
    pub async fn list_bindings_for_package(&self, package_id: &str) -> Vec<PackageCveBinding> {
        let binding_ids = {
            let Ok(by_pkg) = self.by_package.read() else {
                return Vec::new();
            };
            let ids = by_pkg.get(package_id).cloned().unwrap_or_default();
            drop(by_pkg);
            ids
        };
        let Ok(bindings) = self.bindings.read() else {
            return Vec::new();
        };
        let result: Vec<PackageCveBinding> = binding_ids
            .iter()
            .filter_map(|id| bindings.get(id).cloned())
            .collect();
        drop(bindings);
        result
    }

    /// Update the status of an existing binding.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError::Internal`] if the binding id is unknown.
    pub async fn update_binding_status(
        &self,
        binding_id: &str,
        new_status: CveStatus,
    ) -> Result<(), IntegrationError> {
        let mut bindings = self.bindings.write().map_err(|_| lock_poisoned())?;
        let binding = bindings
            .get_mut(binding_id)
            .ok_or_else(|| IntegrationError::Internal("unknown binding id".into()))?;
        binding.status = new_status;
        drop(bindings);
        Ok(())
    }

    // -- enforcement --------------------------------------------------------

    /// Derive the enforcement level for a CVE from its CVSS score.
    pub async fn enforcement_level_for(&self, cve_id: &CveId) -> Option<CveEnforcementLevel> {
        let records = self.records.read().ok()?;
        let record = records.get(cve_id)?;
        let level = cvss_to_enforcement(record.cvss_v3_score);
        drop(records);
        Some(level)
    }

    /// Return the set of package ids that have at least one binding whose
    /// enforcement level is ≥ `level`.
    pub async fn list_packages_at_or_above(&self, level: CveEnforcementLevel) -> Vec<String> {
        let Ok(records) = self.records.read() else {
            return Vec::new();
        };
        let Ok(bindings) = self.bindings.read() else {
            return Vec::new();
        };
        let mut packages: Vec<String> = bindings
            .values()
            .filter(|b| {
                records
                    .get(&b.cve_id)
                    .is_some_and(|r| cvss_to_enforcement(r.cvss_v3_score) >= level)
            })
            .map(|b| b.package_id.clone())
            .collect();
        drop(bindings);
        drop(records);
        packages.sort();
        packages.dedup();
        packages
    }
}

impl Default for CveFeedShape {
    fn default() -> Self {
        Self::new()
    }
}
