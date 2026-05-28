//! L8 Hardware Evidence Emitter (S8.3 + S8.2 + S8.5 ↔ S3.1) — typed lifecycle
//! event emission into the append-only Evidence Log.
//!
//! Every graph build, drift signal, device register/deregister/lifecycle/quarantine,
//! driver binding admit/reject, capability lie, IOMMU missing, Thunderbolt
//! unauthorized, removable admission denied, AI-blocked removable, GPU device
//! register, binding grant/release, VRAM exhaustion, dmabuf peer unauthorized,
//! and firmware FSM transition produces a chained evidence receipt.
//! INV-015: NO raw signatures, NO private key material, NO raw firmware blob
//! bytes, NO raw `VkDevice` handles.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use aios_evidence::{ReceiptBuilder, ReceiptChain, RecordType, RetentionClass};

use crate::bus::BusKind;
use crate::capability_lie::LieSeverity;
use crate::device_record::HardwareDeviceRecord;
use crate::drift::DriftSignal;
use crate::driver::DriverProvenance;
use crate::driver_binding::DriverBinding;
use crate::error::HardwareError;
use crate::firmware_update::FirmwareUpdatePlan;
use crate::gpu_resource::{GpuCapabilityBinding, GpuDevice};
use crate::graph::HardwareGraph;
use crate::ids::{DeviceId, FirmwareBlobId, GpuId, HardwareGraphId};
use crate::lifecycle::DeviceLifecycleState;
use crate::removable::RemovableDevicePolicy;
use crate::trust_class::DeviceQuarantineReason;

// ---------------------------------------------------------------------------
// HardwareRecordType — 32 lifecycle event discriminators (15 S8.3 + 5 S8.2 + 12 S8.5)
// ---------------------------------------------------------------------------

/// Closed set of L8 hardware lifecycle event types.
///
/// These map to the nearest `aios_evidence::RecordType` variant at emission
/// time. The mapping is stable and one-directional.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HardwareRecordType {
    // ── S8.3 Hardware Graph (15) ──
    HardwareGraphBuilt,
    HardwareGraphDriftDetected,
    HardwareGraphFirstBoot,
    DeviceRegistered,
    DeviceDeregistered,
    DeviceLifecycleTransitioned,
    DeviceQuarantined,
    DeviceClassificationFailed,
    DriverBindingAdmitted,
    DriverBindingRejected,
    HostCapabilityLieDetected,
    IommuMissingForProtectedBus,
    ThunderboltUnauthorized,
    RemovableAdmissionDenied,
    RemovableAiBlocked,

    // ── S8.2 GPU Resource Model (5) ──
    GpuDeviceRegistered,
    GpuBindingGranted,
    GpuBindingReleased,
    GpuVramExhausted,
    DmabufPeerUnauthorized,

    // ── S8.5 Firmware Trust (12) ──
    FirmwareProposed,
    FirmwareVerified,
    FirmwareApproved,
    FirmwareStaged,
    FirmwareApplied,
    FirmwareReverted,
    FirmwareFailed,
    FirmwareUnsignedRefused,
    FirmwareSignatureInvalid,
    FirmwareVersionRegression,
    FirmwareOperatorLocalSigned,
    FirmwareConstitutionalRefusal,
}

impl HardwareRecordType {
    /// Map to the evidence crate's closed `RecordType`.
    #[must_use]
    pub const fn to_evidence_record_type(self) -> RecordType {
        match self {
            // ── S8.3 ──
            Self::HardwareGraphBuilt => RecordType::HardwareGraphRebuilt,
            Self::HardwareGraphDriftDetected => RecordType::HardwareGraphDriftDetected,
            Self::HardwareGraphFirstBoot => RecordType::FirstBootOperation,
            Self::DeviceRegistered => RecordType::DeviceDetected,
            Self::DeviceDeregistered => RecordType::DeviceDisconnected,
            Self::DeviceLifecycleTransitioned => RecordType::StatusTransition,
            Self::DeviceQuarantined => RecordType::DeviceQuarantined,
            Self::DeviceClassificationFailed => RecordType::CapabilityLieDetected,
            Self::DriverBindingAdmitted => RecordType::DeviceDriverBound,
            Self::DriverBindingRejected => RecordType::DeviceDriverRejected,
            Self::HostCapabilityLieDetected => RecordType::HostCapabilityLie,
            Self::IommuMissingForProtectedBus => RecordType::IommuDmaProtectionDegraded,
            Self::ThunderboltUnauthorized => RecordType::OutOfTreeDriverBlocked,
            Self::RemovableAdmissionDenied => RecordType::RemovableDeviceDenied,
            Self::RemovableAiBlocked => RecordType::AiRemovableDeviceBlocked,
            // ── S8.2 ──
            Self::GpuDeviceRegistered => RecordType::GpuDeviceEnumerated,
            Self::GpuBindingGranted => RecordType::GpuVkDeviceCreated,
            Self::GpuBindingReleased => RecordType::GpuVkDeviceDestroyed,
            Self::GpuVramExhausted => RecordType::GpuBudgetExceeded,
            Self::DmabufPeerUnauthorized => RecordType::GpuDmabufDenied,
            // ── S8.5 ──
            Self::FirmwareProposed => RecordType::FirmwareUpdateRequested,
            Self::FirmwareVerified | Self::FirmwareApproved => {
                RecordType::FirmwareVerificationPassed
            }
            Self::FirmwareStaged => RecordType::BiosUefiUpdateDeferred,
            Self::FirmwareApplied => RecordType::FirmwareApplied,
            Self::FirmwareReverted => RecordType::FirmwareRollbackPerformed,
            Self::FirmwareFailed => RecordType::FirmwareApplyFailed,
            Self::FirmwareUnsignedRefused => RecordType::FirmwareUnsignedRejected,
            Self::FirmwareSignatureInvalid => RecordType::FirmwareVerificationFailed,
            Self::FirmwareVersionRegression => RecordType::FirmwareDowngradeBlocked,
            Self::FirmwareOperatorLocalSigned => RecordType::OperatorLocalFirmwareInstalled,
            Self::FirmwareConstitutionalRefusal => RecordType::FirmwareTamperDetected,
        }
    }

    /// Retention class for this event type.
    #[must_use]
    pub const fn retention_class(self) -> RetentionClass {
        match self {
            // FOREVER: denials, tamper, constitutional barriers
            Self::HardwareGraphDriftDetected
            | Self::HostCapabilityLieDetected
            | Self::IommuMissingForProtectedBus
            | Self::ThunderboltUnauthorized
            | Self::RemovableAiBlocked
            | Self::DmabufPeerUnauthorized
            | Self::DriverBindingRejected
            | Self::FirmwareUnsignedRefused
            | Self::FirmwareSignatureInvalid
            | Self::FirmwareVersionRegression
            | Self::FirmwareOperatorLocalSigned
            | Self::FirmwareConstitutionalRefusal => RetentionClass::Forever,
            // STANDARD_24M: everything else
            _ => RetentionClass::Standard24M,
        }
    }

    /// Wire-name string for this discriminator.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            // ── S8.3 ──
            Self::HardwareGraphBuilt => "HARDWARE_GRAPH_BUILT",
            Self::HardwareGraphDriftDetected => "HARDWARE_GRAPH_DRIFT_DETECTED",
            Self::HardwareGraphFirstBoot => "HARDWARE_GRAPH_FIRST_BOOT",
            Self::DeviceRegistered => "DEVICE_REGISTERED",
            Self::DeviceDeregistered => "DEVICE_DEREGISTERED",
            Self::DeviceLifecycleTransitioned => "DEVICE_LIFECYCLE_TRANSITIONED",
            Self::DeviceQuarantined => "DEVICE_QUARANTINED",
            Self::DeviceClassificationFailed => "DEVICE_CLASSIFICATION_FAILED",
            Self::DriverBindingAdmitted => "DRIVER_BINDING_ADMITTED",
            Self::DriverBindingRejected => "DRIVER_BINDING_REJECTED",
            Self::HostCapabilityLieDetected => "HOST_CAPABILITY_LIE_DETECTED",
            Self::IommuMissingForProtectedBus => "IOMMU_MISSING_FOR_PROTECTED_BUS",
            Self::ThunderboltUnauthorized => "THUNDERBOLT_UNAUTHORIZED",
            Self::RemovableAdmissionDenied => "REMOVABLE_ADMISSION_DENIED",
            Self::RemovableAiBlocked => "REMOVABLE_AI_BLOCKED",
            // ── S8.2 ──
            Self::GpuDeviceRegistered => "GPU_DEVICE_REGISTERED",
            Self::GpuBindingGranted => "GPU_BINDING_GRANTED",
            Self::GpuBindingReleased => "GPU_BINDING_RELEASED",
            Self::GpuVramExhausted => "GPU_VRAM_EXHAUSTED",
            Self::DmabufPeerUnauthorized => "DMABUF_PEER_UNAUTHORIZED",
            // ── S8.5 ──
            Self::FirmwareProposed => "FIRMWARE_PROPOSED",
            Self::FirmwareVerified => "FIRMWARE_VERIFIED",
            Self::FirmwareApproved => "FIRMWARE_APPROVED",
            Self::FirmwareStaged => "FIRMWARE_STAGED",
            Self::FirmwareApplied => "FIRMWARE_APPLIED",
            Self::FirmwareReverted => "FIRMWARE_REVERTED",
            Self::FirmwareFailed => "FIRMWARE_FAILED",
            Self::FirmwareUnsignedRefused => "FIRMWARE_UNSIGNED_REFUSED",
            Self::FirmwareSignatureInvalid => "FIRMWARE_SIGNATURE_INVALID",
            Self::FirmwareVersionRegression => "FIRMWARE_VERSION_REGRESSION",
            Self::FirmwareOperatorLocalSigned => "FIRMWARE_OPERATOR_LOCAL_SIGNED",
            Self::FirmwareConstitutionalRefusal => "FIRMWARE_CONSTITUTIONAL_REFUSAL",
        }
    }
}

// ---------------------------------------------------------------------------
// FirmwarePhaseRecord — FSM transition multiplexer
// ---------------------------------------------------------------------------

/// Encodes which phase of the 7-stage firmware FSM an event belongs to.
///
/// Each variant maps to the appropriate `FIRMWARE_*` `HardwareRecordType`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirmwarePhaseRecord {
    Proposed,
    Verified,
    Approved,
    Staged,
    Applied,
    Reverted,
    Failed { reason: String },
}

impl FirmwarePhaseRecord {
    /// Map to the corresponding hardware record type.
    #[must_use]
    pub const fn to_record_type(&self) -> HardwareRecordType {
        match self {
            Self::Proposed => HardwareRecordType::FirmwareProposed,
            Self::Verified => HardwareRecordType::FirmwareVerified,
            Self::Approved => HardwareRecordType::FirmwareApproved,
            Self::Staged => HardwareRecordType::FirmwareStaged,
            Self::Applied => HardwareRecordType::FirmwareApplied,
            Self::Reverted => HardwareRecordType::FirmwareReverted,
            Self::Failed { .. } => HardwareRecordType::FirmwareFailed,
        }
    }

    /// Human-readable phase label.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Verified => "verified",
            Self::Approved => "approved",
            Self::Staged => "staged",
            Self::Applied => "applied",
            Self::Reverted => "reverted",
            Self::Failed { .. } => "failed",
        }
    }
}

// ---------------------------------------------------------------------------
// EvidenceReceipt — hardware-side receipt view
// ---------------------------------------------------------------------------

/// Evidence receipt returned to L8 callers.
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
// HardwareEvidenceEmitter trait
// ---------------------------------------------------------------------------

/// S8.3 + S8.2 + S8.5 ↔ S3.1 — async contract for emitting hardware
/// lifecycle events into the Evidence Log.
///
/// Twenty-six methods cover the full L8 hardware event surface. Implementations
/// are optional (`Option<Arc<dyn HardwareEvidenceEmitter>>`): when `None`, no
/// emission occurs and no error is raised.
#[async_trait]
pub trait HardwareEvidenceEmitter: Send + Sync {
    /// Emit a `HARDWARE_GRAPH_BUILT` evidence record.
    async fn emit_graph_built(
        &self,
        graph: &HardwareGraph,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `HARDWARE_GRAPH_DRIFT_DETECTED` evidence record (FOREVER).
    async fn emit_graph_drift_detected(
        &self,
        signal: &DriftSignal,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `HARDWARE_GRAPH_FIRST_BOOT` evidence record.
    async fn emit_graph_first_boot(
        &self,
        graph_id: &HardwareGraphId,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `DEVICE_REGISTERED` evidence record.
    async fn emit_device_registered(
        &self,
        record: &HardwareDeviceRecord,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `DEVICE_DEREGISTERED` evidence record.
    async fn emit_device_deregistered(
        &self,
        device_id: &DeviceId,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `DEVICE_LIFECYCLE_TRANSITIONED` evidence record.
    async fn emit_device_lifecycle_transitioned(
        &self,
        device_id: &DeviceId,
        from: DeviceLifecycleState,
        to: DeviceLifecycleState,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `DEVICE_QUARANTINED` evidence record.
    async fn emit_device_quarantined(
        &self,
        device_id: &DeviceId,
        reason: DeviceQuarantineReason,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `DEVICE_CLASSIFICATION_FAILED` evidence record.
    async fn emit_device_classification_failed(
        &self,
        observation_summary: &str,
        reason: &str,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `DRIVER_BINDING_ADMITTED` evidence record.
    async fn emit_driver_binding_admitted(
        &self,
        binding: &DriverBinding,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `DRIVER_BINDING_REJECTED` evidence record.
    async fn emit_driver_binding_rejected(
        &self,
        device: &DeviceId,
        reason: &str,
        provenance: Option<DriverProvenance>,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `HOST_CAPABILITY_LIE_DETECTED` evidence record.
    async fn emit_host_capability_lie(
        &self,
        device: &DeviceId,
        key: &str,
        advertised: &str,
        observed: &str,
        severity: LieSeverity,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit an `IOMMU_MISSING_FOR_PROTECTED_BUS` evidence record (FOREVER).
    async fn emit_iommu_missing(
        &self,
        device: &DeviceId,
        bus: BusKind,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `THUNDERBOLT_UNAUTHORIZED` evidence record (FOREVER).
    async fn emit_thunderbolt_unauthorized(
        &self,
        device: &DeviceId,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `REMOVABLE_ADMISSION_DENIED` evidence record.
    async fn emit_removable_admission_denied(
        &self,
        device: &DeviceId,
        policy: RemovableDevicePolicy,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `REMOVABLE_AI_BLOCKED` evidence record (FOREVER, INV-013).
    async fn emit_removable_ai_blocked(
        &self,
        device: &DeviceId,
        ai_subject: &str,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `GPU_DEVICE_REGISTERED` evidence record.
    async fn emit_gpu_device_registered(
        &self,
        device: &GpuDevice,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `GPU_BINDING_GRANTED` evidence record.
    async fn emit_gpu_binding_granted(
        &self,
        binding: &GpuCapabilityBinding,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `GPU_BINDING_RELEASED` evidence record.
    async fn emit_gpu_binding_released(
        &self,
        binding_id: &str,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `GPU_VRAM_EXHAUSTED` evidence record.
    async fn emit_gpu_vram_exhausted(
        &self,
        gpu_id: &GpuId,
        requested: u64,
        available: u64,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `DMABUF_PEER_UNAUTHORIZED` evidence record (FOREVER).
    async fn emit_dmabuf_peer_unauthorized(
        &self,
        handle_id: &str,
        source: &GpuId,
        target: &GpuId,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a firmware lifecycle event evidence record.
    ///
    /// Combines the update plan with a phase discriminator; `phase` encodes
    /// which `FIRMWARE_*` `HardwareRecordType` to emit.
    async fn emit_firmware_event(
        &self,
        plan: &FirmwareUpdatePlan,
        phase: FirmwarePhaseRecord,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `FIRMWARE_UNSIGNED_REFUSED` evidence record (FOREVER).
    async fn emit_firmware_unsigned_refused(
        &self,
        blob_id: &FirmwareBlobId,
        signer_fingerprint: &str,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `FIRMWARE_SIGNATURE_INVALID` evidence record (FOREVER).
    async fn emit_firmware_signature_invalid(
        &self,
        blob_id: &FirmwareBlobId,
        reason: &str,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `FIRMWARE_VERSION_REGRESSION` evidence record (FOREVER).
    async fn emit_firmware_version_regression(
        &self,
        blob_id: &FirmwareBlobId,
        attempted: &str,
        installed: &str,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `FIRMWARE_OPERATOR_LOCAL_SIGNED` evidence record (FOREVER).
    async fn emit_firmware_operator_local_signed(
        &self,
        blob_id: &FirmwareBlobId,
        operator: &str,
    ) -> Result<EvidenceReceipt, HardwareError>;

    /// Emit a `FIRMWARE_CONSTITUTIONAL_REFUSAL` evidence record (FOREVER).
    async fn emit_firmware_constitutional_refusal(
        &self,
        blob_id: &FirmwareBlobId,
        reason: &str,
    ) -> Result<EvidenceReceipt, HardwareError>;
}

// ---------------------------------------------------------------------------
// InMemoryHardwareEvidenceEmitter
// ---------------------------------------------------------------------------

/// In-memory `HardwareEvidenceEmitter` backed by a `ReceiptChain`.
///
/// Every emission seals a new receipt, appends it to the chain, and returns
/// an `EvidenceReceipt` with the chain sequence number.
pub struct InMemoryHardwareEvidenceEmitter {
    chain: Arc<RwLock<ReceiptChain>>,
    subject: String,
}

impl InMemoryHardwareEvidenceEmitter {
    /// Create an emitter that stamps receipts with the given subject
    /// canonical id (e.g. `"_system:service:hardware-manager"`).
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
    pub async fn verify_chain(&self) -> Result<(), HardwareError> {
        self.chain
            .read()
            .await
            .verify_integrity()
            .map_err(|e| HardwareError::Internal(format!("chain integrity: {e}")))
    }

    /// Shared seal-and-append helper.
    async fn seal_and_append(
        &self,
        record_type: HardwareRecordType,
        payload: serde_json::Value,
    ) -> Result<EvidenceReceipt, HardwareError> {
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
            .map_err(|e| HardwareError::Internal(format!("seal: {e}")))?;

        let mut hw_receipt = EvidenceReceipt::from_evidence_receipt(&receipt);
        hw_receipt.sequence = seq;

        chain
            .append(receipt)
            .map_err(|e| HardwareError::Internal(format!("append: {e}")))?;
        drop(chain);

        Ok(hw_receipt)
    }
}

#[async_trait]
impl HardwareEvidenceEmitter for InMemoryHardwareEvidenceEmitter {
    async fn emit_graph_built(
        &self,
        graph: &HardwareGraph,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "graph_id": graph.id.0,
            "device_count": graph.devices.len(),
            "host_canonical_id": graph.host_canonical_id,
            "signer_fingerprint": graph.signer_fingerprint,
            "built_at": graph.built_at.to_rfc3339(),
        });
        self.seal_and_append(HardwareRecordType::HardwareGraphBuilt, payload)
            .await
    }

    async fn emit_graph_drift_detected(
        &self,
        signal: &DriftSignal,
    ) -> Result<EvidenceReceipt, HardwareError> {
        match signal {
            DriftSignal::DriftDetected {
                prior,
                current,
                change,
            } => {
                let payload = serde_json::json!({
                    "prior_graph_id": prior.0,
                    "current_graph_id": current.0,
                    "added": change.added.len(),
                    "removed": change.removed.len(),
                    "modified": change.modified.len(),
                    "kept": change.kept,
                });
                self.seal_and_append(HardwareRecordType::HardwareGraphDriftDetected, payload)
                    .await
            }
            _ => Err(HardwareError::Internal(
                "emit_graph_drift_detected called with non-DriftDetected signal".into(),
            )),
        }
    }

    async fn emit_graph_first_boot(
        &self,
        graph_id: &HardwareGraphId,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "graph_id": graph_id.0,
        });
        self.seal_and_append(HardwareRecordType::HardwareGraphFirstBoot, payload)
            .await
    }

    async fn emit_device_registered(
        &self,
        record: &HardwareDeviceRecord,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "device_id": record.device_id.0,
            "class": format!("{:?}", record.class),
            "bus": format!("{:?}", record.bus),
            "vendor_name": record.vendor_name,
            "product_name": record.product_name,
            "trust_class": format!("{:?}", record.trust_class),
        });
        self.seal_and_append(HardwareRecordType::DeviceRegistered, payload)
            .await
    }

    async fn emit_device_deregistered(
        &self,
        device_id: &DeviceId,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "device_id": device_id.0,
        });
        self.seal_and_append(HardwareRecordType::DeviceDeregistered, payload)
            .await
    }

    async fn emit_device_lifecycle_transitioned(
        &self,
        device_id: &DeviceId,
        from: DeviceLifecycleState,
        to: DeviceLifecycleState,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "device_id": device_id.0,
            "from": format!("{from:?}"),
            "to": format!("{to:?}"),
        });
        self.seal_and_append(HardwareRecordType::DeviceLifecycleTransitioned, payload)
            .await
    }

    async fn emit_device_quarantined(
        &self,
        device_id: &DeviceId,
        reason: DeviceQuarantineReason,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "device_id": device_id.0,
            "reason": format!("{reason:?}"),
        });
        self.seal_and_append(HardwareRecordType::DeviceQuarantined, payload)
            .await
    }

    async fn emit_device_classification_failed(
        &self,
        observation_summary: &str,
        reason: &str,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "observation_summary": observation_summary,
            "reason": reason,
        });
        self.seal_and_append(HardwareRecordType::DeviceClassificationFailed, payload)
            .await
    }

    async fn emit_driver_binding_admitted(
        &self,
        binding: &DriverBinding,
    ) -> Result<EvidenceReceipt, HardwareError> {
        // INV-015: NO raw signature bytes in payload.
        let payload = serde_json::json!({
            "binding_id": binding.binding_id.0,
            "device_id": binding.device_id.0,
            "driver_module_name": binding.driver_module_name,
            "kernel_module_version": binding.kernel_module_version,
            "provenance": binding.provenance.label(),
            "blake3_hash": binding.blake3_hash,
            "signer_fingerprint": binding.signer_fingerprint,
            "admitted_at": binding.admitted_at.to_rfc3339(),
        });
        self.seal_and_append(HardwareRecordType::DriverBindingAdmitted, payload)
            .await
    }

    async fn emit_driver_binding_rejected(
        &self,
        device: &DeviceId,
        reason: &str,
        provenance: Option<DriverProvenance>,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let provenance_label = provenance.as_ref().map_or("unknown", |p| p.label());
        let payload = serde_json::json!({
            "device_id": device.0,
            "reason": reason,
            "provenance": provenance_label,
        });
        self.seal_and_append(HardwareRecordType::DriverBindingRejected, payload)
            .await
    }

    async fn emit_host_capability_lie(
        &self,
        device: &DeviceId,
        key: &str,
        advertised: &str,
        observed: &str,
        severity: LieSeverity,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "device_id": device.0,
            "key": key,
            "advertised": advertised,
            "observed": observed,
            "severity": format!("{severity:?}"),
        });
        self.seal_and_append(HardwareRecordType::HostCapabilityLieDetected, payload)
            .await
    }

    async fn emit_iommu_missing(
        &self,
        device: &DeviceId,
        bus: BusKind,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "device_id": device.0,
            "bus": format!("{bus:?}"),
        });
        self.seal_and_append(HardwareRecordType::IommuMissingForProtectedBus, payload)
            .await
    }

    async fn emit_thunderbolt_unauthorized(
        &self,
        device: &DeviceId,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "device_id": device.0,
        });
        self.seal_and_append(HardwareRecordType::ThunderboltUnauthorized, payload)
            .await
    }

    async fn emit_removable_admission_denied(
        &self,
        device: &DeviceId,
        policy: RemovableDevicePolicy,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "device_id": device.0,
            "policy": format!("{policy:?}"),
        });
        self.seal_and_append(HardwareRecordType::RemovableAdmissionDenied, payload)
            .await
    }

    async fn emit_removable_ai_blocked(
        &self,
        device: &DeviceId,
        ai_subject: &str,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "device_id": device.0,
            "ai_subject": ai_subject,
        });
        self.seal_and_append(HardwareRecordType::RemovableAiBlocked, payload)
            .await
    }

    async fn emit_gpu_device_registered(
        &self,
        device: &GpuDevice,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let classes: Vec<String> = device
            .supported_classes
            .iter()
            .map(|c| format!("{c:?}"))
            .collect();
        let payload = serde_json::json!({
            "gpu_id": device.gpu_id.0,
            "vendor": format!("{:?}", device.vendor),
            "product_name": device.product_name,
            "vram_total_bytes": device.vram_total_bytes,
            "supported_classes": classes,
            "iommu_protected": device.iommu_protected,
        });
        self.seal_and_append(HardwareRecordType::GpuDeviceRegistered, payload)
            .await
    }

    async fn emit_gpu_binding_granted(
        &self,
        binding: &GpuCapabilityBinding,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "binding_id": binding.binding_id,
            "gpu_id": binding.gpu_id.0,
            "group_id": binding.group_id,
            "subject_canonical_id": binding.subject_canonical_id,
            "capability_class": format!("{:?}", binding.capability_class),
            "vram_bytes_reserved": binding.vram_bytes_reserved,
            // INV-015: partition_id only — NO raw VkDevice handles
            "vk_device_partition_id": binding.vk_device_partition_id,
            "bound_at": binding.bound_at.to_rfc3339(),
        });
        self.seal_and_append(HardwareRecordType::GpuBindingGranted, payload)
            .await
    }

    async fn emit_gpu_binding_released(
        &self,
        binding_id: &str,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "binding_id": binding_id,
        });
        self.seal_and_append(HardwareRecordType::GpuBindingReleased, payload)
            .await
    }

    async fn emit_gpu_vram_exhausted(
        &self,
        gpu_id: &GpuId,
        requested: u64,
        available: u64,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "gpu_id": gpu_id.0,
            "requested_bytes": requested,
            "available_bytes": available,
        });
        self.seal_and_append(HardwareRecordType::GpuVramExhausted, payload)
            .await
    }

    async fn emit_dmabuf_peer_unauthorized(
        &self,
        handle_id: &str,
        source: &GpuId,
        target: &GpuId,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "handle_id": handle_id,
            "source_gpu": source.0,
            "target_gpu": target.0,
        });
        self.seal_and_append(HardwareRecordType::DmabufPeerUnauthorized, payload)
            .await
    }

    async fn emit_firmware_event(
        &self,
        plan: &FirmwareUpdatePlan,
        phase: FirmwarePhaseRecord,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let record_type = phase.to_record_type();
        // INV-015: blob_id + version only — NO raw firmware blob bytes.
        let mut payload = serde_json::json!({
            "blob_id": plan.blob.blob_id.0,
            "update_class": format!("{:?}", plan.blob.update_class),
            "scope": format!("{:?}", plan.blob.scope),
            "vendor_name": plan.blob.vendor_name,
            "version": plan.blob.version,
            // signer_fingerprint is OK (public key digest), no raw signature
            "signer_fingerprint": plan.blob.signer_fingerprint,
            "phase": phase.label(),
            "current_state": format!("{:?}", plan.current_state),
        });
        if let FirmwarePhaseRecord::Failed { ref reason } = phase {
            payload["failure_reason"] = serde_json::Value::String(reason.clone());
        }
        self.seal_and_append(record_type, payload).await
    }

    async fn emit_firmware_unsigned_refused(
        &self,
        blob_id: &FirmwareBlobId,
        signer_fingerprint: &str,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "blob_id": blob_id.0,
            "signer_fingerprint": signer_fingerprint,
        });
        self.seal_and_append(HardwareRecordType::FirmwareUnsignedRefused, payload)
            .await
    }

    async fn emit_firmware_signature_invalid(
        &self,
        blob_id: &FirmwareBlobId,
        reason: &str,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "blob_id": blob_id.0,
            "reason": reason,
        });
        self.seal_and_append(HardwareRecordType::FirmwareSignatureInvalid, payload)
            .await
    }

    async fn emit_firmware_version_regression(
        &self,
        blob_id: &FirmwareBlobId,
        attempted: &str,
        installed: &str,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "blob_id": blob_id.0,
            "attempted_version": attempted,
            "installed_version": installed,
        });
        self.seal_and_append(HardwareRecordType::FirmwareVersionRegression, payload)
            .await
    }

    async fn emit_firmware_operator_local_signed(
        &self,
        blob_id: &FirmwareBlobId,
        operator: &str,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "blob_id": blob_id.0,
            "operator": operator,
        });
        self.seal_and_append(HardwareRecordType::FirmwareOperatorLocalSigned, payload)
            .await
    }

    async fn emit_firmware_constitutional_refusal(
        &self,
        blob_id: &FirmwareBlobId,
        reason: &str,
    ) -> Result<EvidenceReceipt, HardwareError> {
        let payload = serde_json::json!({
            "blob_id": blob_id.0,
            "reason": reason,
        });
        self.seal_and_append(HardwareRecordType::FirmwareConstitutionalRefusal, payload)
            .await
    }
}

// ---------------------------------------------------------------------------
// Optional-emitter wiring helpers
// ---------------------------------------------------------------------------

/// Trait for types that accept an optional emitter.
///
/// Implemented on the 9 hardware subsystems so that the evidence half
/// can be wired independently at construction time.
pub trait WithEmitter {
    /// Attach an optional `HardwareEvidenceEmitter`.
    ///
    /// When `None` is passed, the subsystem operates without evidence
    /// emission — existing callers and tests are unaffected.
    #[must_use]
    fn with_emitter(self, emitter: Option<Arc<dyn HardwareEvidenceEmitter>>) -> Self;
}
