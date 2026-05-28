use serde::{Deserialize, Serialize};

/// CVE severity ordered closed enum (Low < Medium < High < Critical).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CveSeverity {
    /// Informational / negligible impact.
    Low,
    /// Moderate impact, limited exposure.
    Medium,
    /// Significant impact, broad exposure.
    High,
    /// Severe impact, active exploitation likely.
    Critical,
}

/// CVE remediation status ordered closed enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CveStatus {
    /// Newly discovered, not yet triaged.
    Open,
    /// Actively being investigated.
    UnderReview,
    /// Remediation applied and verified.
    Patched,
    /// Service isolated while remediation is developed.
    Quarantined,
    /// CVE does not affect this deployment.
    NotApplicable,
}

/// A CVE identifier (format `CVE-YYYY-NNNN+`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CveId(pub String);
