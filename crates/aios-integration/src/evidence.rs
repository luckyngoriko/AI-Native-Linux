//! L10 Integration Evidence Emitter (S11.4 ↔ S3.1).
//!
//! Typed lifecycle event emission into the append-only Evidence Log.
//!
//! Every vendor contract proposal, standard update, CVE binding,
//! lifecycle transition, vendor revocation, bridge admission, baseline
//! snapshot, and control map drift produces a chained evidence receipt.
//! INV-015: NO raw signatures, NO raw CVE feed payloads beyond the
//! record summary.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use aios_evidence::{ReceiptBuilder, ReceiptChain, RecordType, RetentionClass};

use crate::bridges::BridgeContract;
use crate::control_map::{ComplianceBaseline, ControlDriftReport};
use crate::cve::CveSeverity;
use crate::cve_feed::PackageCveBinding;
use crate::error::IntegrationError;
use crate::ids::VendorContractId;
use crate::lifecycle::IntegrationLifecycleLabel;
use crate::standard::StandardSubscription;
use crate::vendor::VendorIntegrationContract;

// ---------------------------------------------------------------------------
// IntegrationRecordType — 8 lifecycle event discriminators
// ---------------------------------------------------------------------------

/// Closed set of L10 integration lifecycle event types.
///
/// These map to the nearest `aios_evidence::RecordType` variant at emission
/// time. The mapping is stable and one-directional.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntegrationRecordType {
    /// Vendor contract enters Proposed lifecycle state (S11.4 §2 I2).
    IntegrationProposed,
    /// Standard subscription enters `ReviewDue` or has a new revision.
    StandardUpdateAvailable,
    /// CVE is bound to a package; severity from the binding.
    PackageHasKnownCve,
    /// Integration resource lifecycle state transitioned.
    IntegrationLifecycleTransitioned,
    /// Vendor contract forcibly retired (S11.4 §2 I2).
    VendorContractRevoked,
    /// External bridge package admitted into the catalog.
    BridgeAdmitted,
    /// Compliance baseline snapshot taken.
    ComplianceBaselineSnapshot,
    /// Drift detected between control map baseline and current state.
    ControlMapDriftDetected,
}

impl IntegrationRecordType {
    /// Map to the evidence crate's closed `RecordType`.
    #[must_use]
    pub const fn to_evidence_record_type(self) -> RecordType {
        match self {
            Self::IntegrationProposed
            | Self::IntegrationLifecycleTransitioned
            | Self::VendorContractRevoked
            | Self::ControlMapDriftDetected => RecordType::StatusTransition,
            Self::StandardUpdateAvailable => RecordType::PolicyDecision,
            Self::PackageHasKnownCve => RecordType::FailureObserved,
            Self::BridgeAdmitted => RecordType::ExternalBridgePackageAdmitted,
            Self::ComplianceBaselineSnapshot => RecordType::ChainCheckpoint,
        }
    }

    /// Retention class for this event type.
    #[must_use]
    pub const fn retention_class(self) -> RetentionClass {
        match self {
            Self::VendorContractRevoked
            | Self::ComplianceBaselineSnapshot
            | Self::ControlMapDriftDetected => RetentionClass::Forever,
            _ => RetentionClass::Standard24M,
        }
    }

    /// Wire-name string for this discriminator.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IntegrationProposed => "INTEGRATION_PROPOSED",
            Self::StandardUpdateAvailable => "STANDARD_UPDATE_AVAILABLE",
            Self::PackageHasKnownCve => "PACKAGE_HAS_KNOWN_CVE",
            Self::IntegrationLifecycleTransitioned => "INTEGRATION_LIFECYCLE_TRANSITIONED",
            Self::VendorContractRevoked => "VENDOR_CONTRACT_REVOKED",
            Self::BridgeAdmitted => "BRIDGE_ADMITTED",
            Self::ComplianceBaselineSnapshot => "COMPLIANCE_BASELINE_SNAPSHOT",
            Self::ControlMapDriftDetected => "CONTROL_MAP_DRIFT_DETECTED",
        }
    }
}

// ---------------------------------------------------------------------------
// EvidenceReceipt — integration-side receipt view
// ---------------------------------------------------------------------------

/// Evidence receipt returned to L10 callers.
///
/// Carries the record identity, content hash, and chain sequence number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceReceipt {
    /// Evidence receipt id (`evr_<ULID>`).
    pub record_id: String,
    /// BLAKE3-256 content hash (64 lowercase hex chars).
    pub hash: String,
    /// 0-based sequence position in the emitter's chain.
    pub sequence: u64,
}

impl EvidenceReceipt {
    fn from_evidence_receipt(r: &aios_evidence::EvidenceReceipt) -> Self {
        Self {
            record_id: r.receipt_id().as_str().to_owned(),
            hash: r.content_hash().to_owned(),
            sequence: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// IntegrationEvidenceEmitter trait
// ---------------------------------------------------------------------------

/// S11.4 ↔ S3.1 — async contract for emitting integration lifecycle
/// events into the Evidence Log.
///
/// Eight methods cover the full L10 integration event surface. Implementations
/// are optional (`Option<Arc<dyn IntegrationEvidenceEmitter>>`): when `None`,
/// no emission occurs and no error is raised.
#[async_trait]
pub trait IntegrationEvidenceEmitter: Send + Sync {
    /// Emit an `INTEGRATION_PROPOSED` evidence record when a
    /// `VendorIntegrationContract` enters the Proposed lifecycle state.
    async fn emit_integration_proposed(
        &self,
        contract: &VendorIntegrationContract,
    ) -> Result<EvidenceReceipt, IntegrationError>;

    /// Emit a `STANDARD_UPDATE_AVAILABLE` evidence record when a
    /// standard subscription enters `ReviewDue` or has a new revision.
    async fn emit_standard_update_available(
        &self,
        subscription: &StandardSubscription,
        new_revision: &str,
    ) -> Result<EvidenceReceipt, IntegrationError>;

    /// Emit a `PACKAGE_HAS_KNOWN_CVE` evidence record when a CVE is
    /// bound to a package.
    async fn emit_package_has_known_cve(
        &self,
        binding: &PackageCveBinding,
        severity: CveSeverity,
    ) -> Result<EvidenceReceipt, IntegrationError>;

    /// Emit an `INTEGRATION_LIFECYCLE_TRANSITIONED` evidence record
    /// when an integration resource transitions lifecycle state.
    async fn emit_lifecycle_transitioned(
        &self,
        contract_id: &VendorContractId,
        from: IntegrationLifecycleLabel,
        to: IntegrationLifecycleLabel,
    ) -> Result<EvidenceReceipt, IntegrationError>;

    /// Emit a `VENDOR_CONTRACT_REVOKED` evidence record (FOREVER) when
    /// a vendor contract is forcibly retired.
    async fn emit_vendor_revoked(
        &self,
        contract_id: &VendorContractId,
        reason: &str,
    ) -> Result<EvidenceReceipt, IntegrationError>;

    /// Emit a `BRIDGE_ADMITTED` evidence record when an external bridge
    /// contract is admitted into the catalog.
    async fn emit_bridge_admitted(
        &self,
        bridge: &BridgeContract,
    ) -> Result<EvidenceReceipt, IntegrationError>;

    /// Emit a `COMPLIANCE_BASELINE_SNAPSHOT` evidence record (FOREVER)
    /// when a compliance baseline is captured.
    async fn emit_baseline_snapshot(
        &self,
        baseline: &ComplianceBaseline,
    ) -> Result<EvidenceReceipt, IntegrationError>;

    /// Emit a `CONTROL_MAP_DRIFT_DETECTED` evidence record (FOREVER)
    /// when drift is detected between a baseline and the current mapping.
    async fn emit_control_drift(
        &self,
        report: &ControlDriftReport,
    ) -> Result<EvidenceReceipt, IntegrationError>;
}

// ---------------------------------------------------------------------------
// InMemoryIntegrationEvidenceEmitter
// ---------------------------------------------------------------------------

/// In-memory `IntegrationEvidenceEmitter` backed by a `ReceiptChain`.
///
/// Every emission seals a new receipt, appends it to the chain, and returns
/// an `EvidenceReceipt` with the chain sequence number.
pub struct InMemoryIntegrationEvidenceEmitter {
    chain: Arc<RwLock<ReceiptChain>>,
    subject: String,
}

impl InMemoryIntegrationEvidenceEmitter {
    /// Create an emitter that stamps receipts with the given subject
    /// canonical id (e.g. `"_system:service:integration-manager"`).
    #[must_use]
    pub fn new(subject: impl Into<String>) -> Self {
        Self {
            chain: Arc::new(RwLock::new(ReceiptChain::new())),
            subject: subject.into(),
        }
    }

    /// Return the number of receipts currently in the chain (test seam).
    pub async fn receipt_count(&self) -> usize {
        self.chain.read().await.len()
    }

    /// Return the payload at the given 0-based index (test seam).
    #[must_use]
    pub async fn get_payload(&self, index: usize) -> Option<serde_json::Value> {
        let chain = self.chain.read().await;
        chain.receipts().get(index).map(|r| r.payload().clone())
    }

    /// Verify the full hash-chain integrity (test seam).
    ///
    /// # Errors
    ///
    /// Returns `Internal` wrapping the underlying chain error.
    pub async fn verify_chain(&self) -> Result<(), IntegrationError> {
        self.chain
            .read()
            .await
            .verify_integrity()
            .map_err(|e| IntegrationError::Internal(format!("chain integrity: {e}")))
    }

    /// Shared seal-and-append helper.
    async fn seal_and_append(
        &self,
        record_type: IntegrationRecordType,
        payload: serde_json::Value,
    ) -> Result<EvidenceReceipt, IntegrationError> {
        let mut chain = self.chain.write().await;
        let prev = chain.receipts().last();
        let seq = chain.len() as u64;

        let builder = ReceiptBuilder::new(
            record_type.to_evidence_record_type(),
            record_type.retention_class(),
            &self.subject,
        )
        .with_payload(payload);

        let receipt = builder
            .seal(prev)
            .map_err(|e| IntegrationError::Internal(format!("seal: {e}")))?;

        let mut ir = EvidenceReceipt::from_evidence_receipt(&receipt);
        ir.sequence = seq;

        chain
            .append(receipt)
            .map_err(|e| IntegrationError::Internal(format!("append: {e}")))?;
        drop(chain);

        Ok(ir)
    }
}

#[async_trait]
impl IntegrationEvidenceEmitter for InMemoryIntegrationEvidenceEmitter {
    async fn emit_integration_proposed(
        &self,
        contract: &VendorIntegrationContract,
    ) -> Result<EvidenceReceipt, IntegrationError> {
        // INV-015: NO raw signature bytes in payload.
        let payload = serde_json::json!({
            "contract_id": contract.contract_id.0,
            "vendor_name": contract.vendor_name,
            "vendor_kind": contract.vendor_kind.label(),
            "trust_class": contract.trust_class.label(),
            "contact_canonical_id": contract.contact_canonical_id,
            "rotation_cadence_days": contract.rotation_cadence_days,
            "signer_fingerprint": contract.signer_fingerprint,
            "admitted_at": contract.admitted_at.to_rfc3339(),
        });
        self.seal_and_append(IntegrationRecordType::IntegrationProposed, payload)
            .await
    }

    async fn emit_standard_update_available(
        &self,
        subscription: &StandardSubscription,
        new_revision: &str,
    ) -> Result<EvidenceReceipt, IntegrationError> {
        let payload = serde_json::json!({
            "subscription_id": subscription.subscription_id.0,
            "standard": format!("{:?}", subscription.standard),
            "current_revision": subscription.current_revision,
            "new_revision": new_revision,
            "next_review_due_at": subscription.next_review_due_at.to_rfc3339(),
            "responsible_canonical_id": subscription.responsible_canonical_id,
        });
        self.seal_and_append(IntegrationRecordType::StandardUpdateAvailable, payload)
            .await
    }

    async fn emit_package_has_known_cve(
        &self,
        binding: &PackageCveBinding,
        severity: CveSeverity,
    ) -> Result<EvidenceReceipt, IntegrationError> {
        // INV-015: NO raw CVE feed payloads beyond the record summary.
        let payload = serde_json::json!({
            "binding_id": binding.binding_id,
            "cve_id": binding.cve_id.0,
            "package_id": binding.package_id,
            "severity": format!("{severity:?}"),
            "status": format!("{:?}", binding.status),
            "bound_at": binding.bound_at.to_rfc3339(),
        });
        self.seal_and_append(IntegrationRecordType::PackageHasKnownCve, payload)
            .await
    }

    async fn emit_lifecycle_transitioned(
        &self,
        contract_id: &VendorContractId,
        from: IntegrationLifecycleLabel,
        to: IntegrationLifecycleLabel,
    ) -> Result<EvidenceReceipt, IntegrationError> {
        let payload = serde_json::json!({
            "contract_id": contract_id.0,
            "from": from.label(),
            "to": to.label(),
        });
        self.seal_and_append(
            IntegrationRecordType::IntegrationLifecycleTransitioned,
            payload,
        )
        .await
    }

    async fn emit_vendor_revoked(
        &self,
        contract_id: &VendorContractId,
        reason: &str,
    ) -> Result<EvidenceReceipt, IntegrationError> {
        let payload = serde_json::json!({
            "contract_id": contract_id.0,
            "reason": reason,
        });
        self.seal_and_append(IntegrationRecordType::VendorContractRevoked, payload)
            .await
    }

    async fn emit_bridge_admitted(
        &self,
        bridge: &BridgeContract,
    ) -> Result<EvidenceReceipt, IntegrationError> {
        let payload = serde_json::json!({
            "bridge_id": bridge.bridge_id,
            "kind": bridge.kind.label(),
            "manifest_format": bridge.translation_rules.source_manifest_format,
            "trust_class": bridge.vendor_contract.trust_class.label(),
            "signer_fingerprint": bridge.vendor_contract.signer_fingerprint,
            "admitted_at": bridge.admitted_at.to_rfc3339(),
        });
        self.seal_and_append(IntegrationRecordType::BridgeAdmitted, payload)
            .await
    }

    async fn emit_baseline_snapshot(
        &self,
        baseline: &ComplianceBaseline,
    ) -> Result<EvidenceReceipt, IntegrationError> {
        let payload = serde_json::json!({
            "baseline_id": baseline.baseline_id,
            "aios_version": baseline.aios_version,
            "mapping_count": baseline.mappings.len(),
            "snapshot_at": baseline.snapshot_at.to_rfc3339(),
            "validator_canonical_id": baseline.validator_canonical_id,
        });
        self.seal_and_append(IntegrationRecordType::ComplianceBaselineSnapshot, payload)
            .await
    }

    async fn emit_control_drift(
        &self,
        report: &ControlDriftReport,
    ) -> Result<EvidenceReceipt, IntegrationError> {
        let payload = serde_json::json!({
            "prior_baseline_id": report.prior_baseline_id,
            "added_count": report.added.len(),
            "removed_count": report.removed.len(),
            "modified_count": report.modified.len(),
            "unchanged_count": report.unchanged_count,
        });
        self.seal_and_append(IntegrationRecordType::ControlMapDriftDetected, payload)
            .await
    }
}

// ---------------------------------------------------------------------------
// Optional-emitter wiring helpers
// ---------------------------------------------------------------------------

/// Trait for types that accept an optional integration evidence emitter.
///
/// Implemented on the 5 integration subsystems so that the evidence half
/// can be wired independently at construction time.
pub trait WithIntegrationEmitter {
    /// Attach an optional `IntegrationEvidenceEmitter`.
    ///
    /// When `None` is passed, the subsystem operates without evidence
    /// emission — existing callers and tests are unaffected.
    #[must_use]
    fn with_emitter(self, emitter: Arc<dyn IntegrationEvidenceEmitter>) -> Self;
}
