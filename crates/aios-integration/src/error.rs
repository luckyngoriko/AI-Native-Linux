use chrono::{DateTime, Utc};
use std::fmt;

use crate::ids::{StandardSubscriptionId, VendorContractId};
use crate::lifecycle::IntegrationLifecycleLabel;

/// Closed error code catalogue for the integration layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntegrationErrorCode {
    /// Invalid lifecycle state transition attempted.
    LifecycleInvalidTransition,
    /// Vendor contract Ed25519 signature verification failed.
    VendorContractSignatureInvalid,
    /// Vendor is blacklisted and must not be admitted.
    VendorBlacklisted,
    /// Standards subscription review window expired.
    StandardSubscriptionExpired,
    /// CVE feed endpoint is unreachable.
    CveFeedUnreachable,
    /// Directed cycle detected in the service composition graph.
    CompositionCycleDetected,
    /// A required composed service is missing from the graph.
    ComposedServiceMissing,
    /// The orchestrator binary failed to boot a stage.
    OrchestratorBootFailed,
    /// Integration configuration is invalid.
    ConfigInvalid,
    /// Unspecified internal error.
    Internal,
}

/// Structured error type for the integration layer (S11.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrationError {
    /// Lifecycle transition prevented by guard conditions.
    LifecycleInvalidTransition {
        /// Source state label.
        from: IntegrationLifecycleLabel,
        /// Target state label.
        to: IntegrationLifecycleLabel,
        /// Human-readable reason the transition was denied.
        reason: String,
    },
    /// Vendor contract Ed25519 signature verification failed.
    VendorContractSignatureInvalid {
        /// The contract whose signature is invalid.
        contract_id: VendorContractId,
        /// Reason for the verification failure.
        reason: String,
    },
    /// Vendor is blacklisted.
    VendorBlacklisted {
        /// The blacklisted contract identifier.
        contract_id: VendorContractId,
    },
    /// Standards subscription review window expired.
    StandardSubscriptionExpired {
        /// The expired subscription identifier.
        subscription_id: StandardSubscriptionId,
        /// UTC timestamp when the subscription expired.
        expired_at: DateTime<Utc>,
    },
    /// CVE feed endpoint is unreachable.
    CveFeedUnreachable(String),
    /// Directed cycle detected in the service composition graph.
    CompositionCycleDetected {
        /// The cycle as an ordered list of service IDs.
        cycle: Vec<String>,
    },
    /// A required composed service is missing from the graph.
    ComposedServiceMissing {
        /// The missing service ID.
        service_id: String,
        /// The service that requires it.
        required_by: String,
    },
    /// The orchestrator binary failed to boot a stage.
    OrchestratorBootFailed {
        /// The boot stage that failed.
        stage: String,
        /// Human-readable reason for the failure.
        reason: String,
    },
    /// Integration configuration is invalid.
    ConfigInvalid(String),
    /// Unspecified internal error.
    Internal(String),
}

impl IntegrationError {
    /// Returns the error code for this error variant.
    #[must_use]
    pub const fn code(&self) -> IntegrationErrorCode {
        match self {
            Self::LifecycleInvalidTransition { .. } => {
                IntegrationErrorCode::LifecycleInvalidTransition
            }
            Self::VendorContractSignatureInvalid { .. } => {
                IntegrationErrorCode::VendorContractSignatureInvalid
            }
            Self::VendorBlacklisted { .. } => IntegrationErrorCode::VendorBlacklisted,
            Self::StandardSubscriptionExpired { .. } => {
                IntegrationErrorCode::StandardSubscriptionExpired
            }
            Self::CveFeedUnreachable(_) => IntegrationErrorCode::CveFeedUnreachable,
            Self::CompositionCycleDetected { .. } => IntegrationErrorCode::CompositionCycleDetected,
            Self::ComposedServiceMissing { .. } => IntegrationErrorCode::ComposedServiceMissing,
            Self::OrchestratorBootFailed { .. } => IntegrationErrorCode::OrchestratorBootFailed,
            Self::ConfigInvalid(_) => IntegrationErrorCode::ConfigInvalid,
            Self::Internal(_) => IntegrationErrorCode::Internal,
        }
    }
}

impl fmt::Display for IntegrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LifecycleInvalidTransition { from, to, reason } => {
                write!(
                    f,
                    "lifecycle invalid transition from {from:?} to {to:?}: {reason}"
                )
            }
            Self::VendorContractSignatureInvalid {
                contract_id,
                reason,
            } => {
                write!(
                    f,
                    "vendor contract signature invalid for {contract_id:?}: {reason}"
                )
            }
            Self::VendorBlacklisted { contract_id } => {
                write!(f, "vendor {contract_id:?} is blacklisted")
            }
            Self::StandardSubscriptionExpired {
                subscription_id,
                expired_at,
            } => {
                write!(
                    f,
                    "standard subscription {subscription_id:?} expired at {expired_at}"
                )
            }
            Self::CveFeedUnreachable(msg) => {
                write!(f, "CVE feed unreachable: {msg}")
            }
            Self::CompositionCycleDetected { cycle } => {
                write!(f, "composition cycle detected: {cycle:?}")
            }
            Self::ComposedServiceMissing {
                service_id,
                required_by,
            } => {
                write!(
                    f,
                    "composed service {service_id} missing (required by {required_by})"
                )
            }
            Self::OrchestratorBootFailed { stage, reason } => {
                write!(f, "orchestrator boot failed at stage {stage}: {reason}")
            }
            Self::ConfigInvalid(msg) => {
                write!(f, "config invalid: {msg}")
            }
            Self::Internal(msg) => {
                write!(f, "internal error: {msg}")
            }
        }
    }
}

impl std::error::Error for IntegrationError {}
