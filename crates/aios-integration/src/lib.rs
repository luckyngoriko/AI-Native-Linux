//! L10 System Integration Layer for AIOS (S11.4).
//!
//! Typed core skeleton: closed vocabularies + error enum + identifier types.
//! Vendor registry, standards subscriptions, CVE shape, system composition,
//! orchestrator binary, gRPC, evidence, cross-crate land in later tasks.
/// System composition graph types (S11.4 §2 I5).
pub mod composition;
/// CVE severity, status, and identifier types.
pub mod cve;
/// CVE feed integration shape — typed framework for CVE ingestion and package binding.
pub mod cve_feed;
/// Integration-layer structured error catalogue.
pub mod error;
/// Identifier newtypes for integration resources.
pub mod ids;
/// 6-state integration lifecycle FSM (S11.4 §2 I1).
pub mod lifecycle;
/// Compliance standard taxonomy and subscription types.
pub mod standard;
/// Compliance standard subscription registry (S11.4 §2 I4).
pub mod standard_registry;
/// Vendor contract types (S11.4 §2 I2).
pub mod vendor;
/// Ed25519-signed vendor contract registry (S11.4 §2 I2).
pub mod vendor_registry;

pub use composition::{ComposedService, ServiceComposition, ServiceDependency};
pub use cve::{CveId, CveSeverity, CveStatus};
pub use cve_feed::{
    cvss_to_enforcement, is_valid_cve_id, CveEnforcementLevel, CveFeedShape, CveRecord,
    PackageCveBinding,
};
pub use error::{IntegrationError, IntegrationErrorCode};
pub use ids::{ComposedSystemId, IntegrationId, StandardSubscriptionId, VendorContractId};
pub use lifecycle::{IntegrationLifecycleLabel, IntegrationLifecycleState};
pub use standard::{StandardKind, StandardSubscription};
pub use standard_registry::{
    standard_kind_to_canonical_url, ExternalStandardRegistry, StandardReviewRecord,
    SubscriptionStatus,
};
pub use vendor::{VendorIntegrationContract, VendorKind, VendorTrustClass};
pub use vendor_registry::VendorIntegrationRegistry;

/// Crate version marker used by closure-invariant tests in T-186.
pub const DEFAULT_CODE_VERSION: &str = "aios-integration/0.0.1-T175";
