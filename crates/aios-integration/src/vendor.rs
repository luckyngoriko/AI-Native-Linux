use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::VendorContractId;

/// Closed taxonomy of vendor types that AIOS may integrate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VendorKind {
    /// Linux package repository (e.g. apt, dnf, zypper mirror).
    PackageRepository,
    /// Application store or marketplace.
    ApplicationStore,
    /// OCI container registry.
    OciRegistry,
    /// CVE vulnerability feed provider.
    CveFeed,
    /// Compliance / audit provider.
    ComplianceProvider,
    /// Metrics / telemetry exporter target.
    MetricsExporter,
    /// External identity provider (OIDC / SAML).
    IdentityProvider,
    /// Vendor with another certified integration type.
    OtherCertified,
}

impl VendorKind {
    /// Returns the canonical label for this vendor kind (used in contract signing).
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::PackageRepository => "package_repository",
            Self::ApplicationStore => "application_store",
            Self::OciRegistry => "oci_registry",
            Self::CveFeed => "cve_feed",
            Self::ComplianceProvider => "compliance_provider",
            Self::MetricsExporter => "metrics_exporter",
            Self::IdentityProvider => "identity_provider",
            Self::OtherCertified => "other_certified",
        }
    }
}

/// Trust classification for admitted vendor contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VendorTrustClass {
    /// Formally certified AIOS partner.
    AiosCertifiedPartner,
    /// Community-vetted integration.
    CommunityVerified,
    /// Operator-authorised on a per-instance basis.
    OperatorAuthorised,
    /// Explicitly blocked; do not admit.
    BlacklistedDoNotAdmit,
}

impl VendorTrustClass {
    /// Returns the canonical label for this trust class (used in contract signing).
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::AiosCertifiedPartner => "aios_certified_partner",
            Self::CommunityVerified => "community_verified",
            Self::OperatorAuthorised => "operator_authorised",
            Self::BlacklistedDoNotAdmit => "blacklisted_do_not_admit",
        }
    }
}

/// A signed vendor integration contract (S11.4 §2, invariant I2).
///
/// Signature verification lands in T-176; T-175 only defines the shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorIntegrationContract {
    /// Unique contract identifier.
    pub contract_id: VendorContractId,
    /// Human-readable vendor name.
    pub vendor_name: String,
    /// Class of the vendor.
    pub vendor_kind: VendorKind,
    /// Trust classification.
    pub trust_class: VendorTrustClass,
    /// Canonical identity of the vendor contact.
    pub contact_canonical_id: String,
    /// Key rotation cadence in days.
    pub rotation_cadence_days: u32,
    /// URL to the breach-response playbook.
    pub breach_playbook_url: String,
    /// Fingerprint of the signing key.
    pub signer_fingerprint: String,
    /// Ed25519 signature over the canonical contract bytes.
    pub signature: Vec<u8>,
    /// When the contract was admitted into the registry.
    pub admitted_at: DateTime<Utc>,
}
