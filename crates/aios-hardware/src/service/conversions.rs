//! Rust ↔ proto translations for gRPC `HardwareManagerService` +
//! `GpuResourceService` + `FirmwareTrustService` (T-172).
//!
//! Owns bidirectional translation between domain types and tonic-generated proto
//! types, plus the `hardware_error_to_status` mapper.

#![allow(
    clippy::result_large_err,
    missing_docs,
    clippy::match_wildcard_for_single_variants,
    clippy::use_self,
    clippy::cast_possible_truncation,
    clippy::clone_on_copy,
    clippy::missing_errors_doc,
    clippy::too_many_lines,
    clippy::wildcard_imports
)]

use chrono::{DateTime, TimeZone, Utc};
use prost_types::Timestamp;
use tonic::Status;

use crate::bus::BusKind;
use crate::capability_lie::{CapabilityLieOutcome, LieSeverity};
use crate::device::DeviceClass;
use crate::device_record::HardwareDeviceRecord;
use crate::dmabuf::{DmabufHandle, DmabufPeer, DmabufPeerSet};
use crate::drift::DriftSignal;
use crate::driver::DriverProvenance;
use crate::driver_binding::DriverBinding;
use crate::error::HardwareError;
use crate::firmware::{
    FirmwareApplyStrategy, FirmwareScope, FirmwareTrustResult, FirmwareUpdateClass,
    FirmwareUpdateState,
};
use crate::firmware_update::{FirmwareBlob, FirmwareUpdatePlan};
use crate::gpu::{GpuCapabilityClass, GpuVendorKind};
use crate::gpu_resource::{
    BindingRequest, GpuCapabilityBinding, GpuDevice, VkDevicePartition, VramAccounting,
};
use crate::graph::HardwareGraph;
use crate::ids::{DeviceId, FirmwareBlobId, GpuId};
use crate::lifecycle::DeviceLifecycleState;
use crate::observation::RawDeviceObservation;
use crate::removable::RemovableDevicePolicy;
use crate::service::proto;
use crate::trust_class::DeviceTrustClass;
use crate::DriverBindingId;
use crate::HardwareGraphId;

// ── Timestamp helpers ──────────────────────────────────────────────────────

fn datetime_to_proto(dt: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: dt.timestamp(),
        nanos: i32::try_from(dt.timestamp_subsec_nanos()).unwrap_or(0),
    }
}

pub(crate) fn datetime_from_proto(ts: Option<Timestamp>) -> DateTime<Utc> {
    ts.map_or_else(
        || Utc.timestamp_opt(0, 0).single().unwrap_or_default(),
        |t| {
            Utc.timestamp_opt(t.seconds, u32::try_from(t.nanos).unwrap_or(0))
                .single()
                .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap_or_default())
        },
    )
}

// ── DeviceClass ↔ proto ────────────────────────────────────────────────────

const fn device_class_to_proto(c: DeviceClass) -> i32 {
    match c {
        DeviceClass::Cpu => proto::DeviceClassProto::Cpu as i32,
        DeviceClass::Memory => proto::DeviceClassProto::Memory as i32,
        DeviceClass::GpuIntegrated => proto::DeviceClassProto::GpuIntegrated as i32,
        DeviceClass::GpuDiscrete => proto::DeviceClassProto::GpuDiscrete as i32,
        DeviceClass::NetworkEthernet => proto::DeviceClassProto::NetworkEthernet as i32,
        DeviceClass::NetworkWifi => proto::DeviceClassProto::NetworkWifi as i32,
        DeviceClass::NetworkBluetooth => proto::DeviceClassProto::NetworkBluetooth as i32,
        DeviceClass::StorageNvme => proto::DeviceClassProto::StorageNvme as i32,
        DeviceClass::StorageSata => proto::DeviceClassProto::StorageSata as i32,
        DeviceClass::StorageMmc => proto::DeviceClassProto::StorageMmc as i32,
        DeviceClass::AudioCard => proto::DeviceClassProto::AudioCard as i32,
        DeviceClass::AudioHeadset => proto::DeviceClassProto::AudioHeadset as i32,
        DeviceClass::UsbController => proto::DeviceClassProto::UsbController as i32,
        DeviceClass::ThunderboltController => proto::DeviceClassProto::ThunderboltController as i32,
        DeviceClass::PrinterOrScanner => proto::DeviceClassProto::PrinterOrScanner as i32,
        DeviceClass::SensorOrInputDevice => proto::DeviceClassProto::SensorOrInputDevice as i32,
    }
}

fn device_class_from_proto(v: i32) -> Result<DeviceClass, HardwareError> {
    let p = proto::DeviceClassProto::try_from(v)
        .map_err(|_| HardwareError::Internal(format!("invalid DeviceClassProto: {v}")))?;
    match p {
        proto::DeviceClassProto::Cpu => Ok(DeviceClass::Cpu),
        proto::DeviceClassProto::Memory => Ok(DeviceClass::Memory),
        proto::DeviceClassProto::GpuIntegrated => Ok(DeviceClass::GpuIntegrated),
        proto::DeviceClassProto::GpuDiscrete => Ok(DeviceClass::GpuDiscrete),
        proto::DeviceClassProto::NetworkEthernet => Ok(DeviceClass::NetworkEthernet),
        proto::DeviceClassProto::NetworkWifi => Ok(DeviceClass::NetworkWifi),
        proto::DeviceClassProto::NetworkBluetooth => Ok(DeviceClass::NetworkBluetooth),
        proto::DeviceClassProto::StorageNvme => Ok(DeviceClass::StorageNvme),
        proto::DeviceClassProto::StorageSata => Ok(DeviceClass::StorageSata),
        proto::DeviceClassProto::StorageMmc => Ok(DeviceClass::StorageMmc),
        proto::DeviceClassProto::AudioCard => Ok(DeviceClass::AudioCard),
        proto::DeviceClassProto::AudioHeadset => Ok(DeviceClass::AudioHeadset),
        proto::DeviceClassProto::UsbController => Ok(DeviceClass::UsbController),
        proto::DeviceClassProto::ThunderboltController => Ok(DeviceClass::ThunderboltController),
        proto::DeviceClassProto::PrinterOrScanner => Ok(DeviceClass::PrinterOrScanner),
        proto::DeviceClassProto::SensorOrInputDevice => Ok(DeviceClass::SensorOrInputDevice),
        proto::DeviceClassProto::DeviceClassUnspecified => {
            Err(HardwareError::Internal("unspecified device class".into()))
        }
    }
}

// ── BusKind ↔ proto ──────────────────────────────────────────────────────

const fn bus_kind_to_proto(b: BusKind) -> i32 {
    match b {
        BusKind::Pci => proto::BusKindProto::Pci as i32,
        BusKind::Pcie => proto::BusKindProto::Pcie as i32,
        BusKind::Usb2 => proto::BusKindProto::Usb2 as i32,
        BusKind::Usb3 => proto::BusKindProto::Usb3 as i32,
        BusKind::Usb4 => proto::BusKindProto::Usb4 as i32,
        BusKind::Thunderbolt => proto::BusKindProto::Thunderbolt as i32,
        BusKind::Nvme => proto::BusKindProto::Nvme as i32,
        BusKind::I2c => proto::BusKindProto::I2c as i32,
    }
}

pub(crate) fn bus_kind_from_proto(v: i32) -> Result<BusKind, HardwareError> {
    let p = proto::BusKindProto::try_from(v)
        .map_err(|_| HardwareError::Internal(format!("invalid BusKindProto: {v}")))?;
    match p {
        proto::BusKindProto::Pci => Ok(BusKind::Pci),
        proto::BusKindProto::Pcie => Ok(BusKind::Pcie),
        proto::BusKindProto::Usb2 => Ok(BusKind::Usb2),
        proto::BusKindProto::Usb3 => Ok(BusKind::Usb3),
        proto::BusKindProto::Usb4 => Ok(BusKind::Usb4),
        proto::BusKindProto::Thunderbolt => Ok(BusKind::Thunderbolt),
        proto::BusKindProto::Nvme => Ok(BusKind::Nvme),
        proto::BusKindProto::I2c => Ok(BusKind::I2c),
        proto::BusKindProto::BusKindUnspecified => {
            Err(HardwareError::Internal("unspecified bus kind".into()))
        }
    }
}

// ── DeviceTrustClass ↔ proto ─────────────────────────────────────────────

const fn trust_class_to_proto(t: DeviceTrustClass) -> i32 {
    match t {
        DeviceTrustClass::RootSigned => proto::DeviceTrustClassProto::RootSigned as i32,
        DeviceTrustClass::VendorSigned => proto::DeviceTrustClassProto::VendorSigned as i32,
        DeviceTrustClass::CommunitySigned => proto::DeviceTrustClassProto::CommunitySigned as i32,
        DeviceTrustClass::OperatorLocal => proto::DeviceTrustClassProto::OperatorLocal as i32,
        DeviceTrustClass::Untrusted => proto::DeviceTrustClassProto::Untrusted as i32,
    }
}

// ── DeviceLifecycleState ↔ proto ──────────────────────────────────────────

const fn lifecycle_to_proto(s: DeviceLifecycleState) -> i32 {
    match s {
        DeviceLifecycleState::Detected => proto::DeviceLifecycleStateProto::Detected as i32,
        DeviceLifecycleState::Probed => proto::DeviceLifecycleStateProto::Probed as i32,
        DeviceLifecycleState::Bound => proto::DeviceLifecycleStateProto::Bound as i32,
        DeviceLifecycleState::Active => proto::DeviceLifecycleStateProto::Active as i32,
        DeviceLifecycleState::Suspended => proto::DeviceLifecycleStateProto::Suspended as i32,
        DeviceLifecycleState::Quarantined => proto::DeviceLifecycleStateProto::Quarantined as i32,
        DeviceLifecycleState::Removed => proto::DeviceLifecycleStateProto::Removed as i32,
        DeviceLifecycleState::Recovered => proto::DeviceLifecycleStateProto::Recovered as i32,
    }
}

pub(crate) fn lifecycle_from_proto(v: i32) -> Result<DeviceLifecycleState, HardwareError> {
    let p = proto::DeviceLifecycleStateProto::try_from(v)
        .map_err(|_| HardwareError::Internal(format!("invalid lifecycle state: {v}")))?;
    match p {
        proto::DeviceLifecycleStateProto::Detected => Ok(DeviceLifecycleState::Detected),
        proto::DeviceLifecycleStateProto::Probed => Ok(DeviceLifecycleState::Probed),
        proto::DeviceLifecycleStateProto::Bound => Ok(DeviceLifecycleState::Bound),
        proto::DeviceLifecycleStateProto::Active => Ok(DeviceLifecycleState::Active),
        proto::DeviceLifecycleStateProto::Suspended => Ok(DeviceLifecycleState::Suspended),
        proto::DeviceLifecycleStateProto::Quarantined => Ok(DeviceLifecycleState::Quarantined),
        proto::DeviceLifecycleStateProto::Removed => Ok(DeviceLifecycleState::Removed),
        proto::DeviceLifecycleStateProto::Recovered => Ok(DeviceLifecycleState::Recovered),
        proto::DeviceLifecycleStateProto::DeviceLifecycleStateUnspecified => Err(
            HardwareError::Internal("unspecified lifecycle state".into()),
        ),
    }
}

// ── DriverProvenance ↔ proto ─────────────────────────────────────────────

const fn provenance_to_proto(p: DriverProvenance) -> i32 {
    match p {
        DriverProvenance::AiosVerified => proto::DriverProvenanceProto::AiosVerified as i32,
        DriverProvenance::SignedKernelModule => {
            proto::DriverProvenanceProto::SignedKernelModule as i32
        }
        DriverProvenance::DistroProvided => proto::DriverProvenanceProto::DistroProvided as i32,
        DriverProvenance::OutOfTreeBlacklisted => {
            proto::DriverProvenanceProto::OutOfTreeBlacklisted as i32
        }
        DriverProvenance::OperatorLocalSigned => {
            proto::DriverProvenanceProto::OperatorLocalSigned as i32
        }
    }
}

fn provenance_from_proto(v: i32) -> Result<DriverProvenance, HardwareError> {
    let p = proto::DriverProvenanceProto::try_from(v)
        .map_err(|_| HardwareError::Internal(format!("invalid DriverProvenanceProto: {v}")))?;
    match p {
        proto::DriverProvenanceProto::AiosVerified => Ok(DriverProvenance::AiosVerified),
        proto::DriverProvenanceProto::SignedKernelModule => {
            Ok(DriverProvenance::SignedKernelModule)
        }
        proto::DriverProvenanceProto::DistroProvided => Ok(DriverProvenance::DistroProvided),
        proto::DriverProvenanceProto::OutOfTreeBlacklisted => {
            Ok(DriverProvenance::OutOfTreeBlacklisted)
        }
        proto::DriverProvenanceProto::OperatorLocalSigned => {
            Ok(DriverProvenance::OperatorLocalSigned)
        }
        proto::DriverProvenanceProto::DriverProvenanceUnspecified => Err(HardwareError::Internal(
            "unspecified driver provenance".into(),
        )),
    }
}

// ── RemovableDevicePolicy ↔ proto ─────────────────────────────────────────

#[allow(dead_code)]
const fn removable_policy_to_proto(p: RemovableDevicePolicy) -> i32 {
    match p {
        RemovableDevicePolicy::DenyDefault => proto::RemovableDevicePolicyProto::DenyDefault as i32,
        RemovableDevicePolicy::AllowReadOnly => {
            proto::RemovableDevicePolicyProto::AllowReadOnly as i32
        }
        RemovableDevicePolicy::AllowMount => proto::RemovableDevicePolicyProto::AllowMount as i32,
        RemovableDevicePolicy::AllowReadWrite => {
            proto::RemovableDevicePolicyProto::AllowReadWrite as i32
        }
        RemovableDevicePolicy::RecoveryDenied => {
            proto::RemovableDevicePolicyProto::RecoveryDenied as i32
        }
    }
}

pub(crate) fn removable_policy_from_proto(v: i32) -> Result<RemovableDevicePolicy, Status> {
    let p = proto::RemovableDevicePolicyProto::try_from(v).map_err(|_| {
        Status::invalid_argument(format!("invalid RemovableDevicePolicyProto: {v}"))
    })?;
    match p {
        proto::RemovableDevicePolicyProto::DenyDefault => Ok(RemovableDevicePolicy::DenyDefault),
        proto::RemovableDevicePolicyProto::AllowReadOnly => {
            Ok(RemovableDevicePolicy::AllowReadOnly)
        }
        proto::RemovableDevicePolicyProto::AllowMount => Ok(RemovableDevicePolicy::AllowMount),
        proto::RemovableDevicePolicyProto::AllowReadWrite => {
            Ok(RemovableDevicePolicy::AllowReadWrite)
        }
        proto::RemovableDevicePolicyProto::RecoveryDenied => {
            Ok(RemovableDevicePolicy::RecoveryDenied)
        }
        proto::RemovableDevicePolicyProto::RemovableDevicePolicyUnspecified => {
            Err(Status::invalid_argument("unspecified removable policy"))
        }
    }
}

// ── LieSeverity ↔ proto ───────────────────────────────────────────────────

const fn lie_severity_to_proto(s: LieSeverity) -> i32 {
    match s {
        LieSeverity::Soft => proto::LieSeverityProto::Soft as i32,
        LieSeverity::Hard => proto::LieSeverityProto::Hard as i32,
        LieSeverity::Constitutional => proto::LieSeverityProto::Constitutional as i32,
    }
}

// ── GpuVendorKind ↔ proto ─────────────────────────────────────────────────

const fn gpu_vendor_to_proto(v: GpuVendorKind) -> i32 {
    match v {
        GpuVendorKind::Amd => proto::GpuVendorKindProto::Amd as i32,
        GpuVendorKind::Intel => proto::GpuVendorKindProto::Intel as i32,
        GpuVendorKind::Nvidia => proto::GpuVendorKindProto::Nvidia as i32,
        GpuVendorKind::Arm => proto::GpuVendorKindProto::Arm as i32,
        GpuVendorKind::Apple => proto::GpuVendorKindProto::Apple as i32,
        GpuVendorKind::Other => proto::GpuVendorKindProto::Other as i32,
    }
}

// ── GpuCapabilityClass ↔ proto ────────────────────────────────────────────

const fn gpu_capability_to_proto(c: GpuCapabilityClass) -> i32 {
    match c {
        GpuCapabilityClass::RenderOnly => proto::GpuCapabilityClassProto::RenderOnly as i32,
        GpuCapabilityClass::ComputeOnly => proto::GpuCapabilityClassProto::ComputeOnly as i32,
        GpuCapabilityClass::RenderAndCompute => {
            proto::GpuCapabilityClassProto::RenderAndCompute as i32
        }
        GpuCapabilityClass::VideoEncode => proto::GpuCapabilityClassProto::VideoEncode as i32,
        GpuCapabilityClass::VideoDecode => proto::GpuCapabilityClassProto::VideoDecode as i32,
    }
}

fn gpu_capability_from_proto(v: i32) -> Result<GpuCapabilityClass, HardwareError> {
    let p = proto::GpuCapabilityClassProto::try_from(v)
        .map_err(|_| HardwareError::Internal(format!("invalid GpuCapabilityClassProto: {v}")))?;
    match p {
        proto::GpuCapabilityClassProto::RenderOnly => Ok(GpuCapabilityClass::RenderOnly),
        proto::GpuCapabilityClassProto::ComputeOnly => Ok(GpuCapabilityClass::ComputeOnly),
        proto::GpuCapabilityClassProto::RenderAndCompute => {
            Ok(GpuCapabilityClass::RenderAndCompute)
        }
        proto::GpuCapabilityClassProto::VideoEncode => Ok(GpuCapabilityClass::VideoEncode),
        proto::GpuCapabilityClassProto::VideoDecode => Ok(GpuCapabilityClass::VideoDecode),
        proto::GpuCapabilityClassProto::GpuCapabilityClassUnspecified => {
            Err(HardwareError::Internal("unspecified gpu capability".into()))
        }
    }
}

// ── FirmwareUpdateState ↔ proto ───────────────────────────────────────────

const fn firmware_state_to_proto(s: FirmwareUpdateState) -> i32 {
    match s {
        FirmwareUpdateState::Proposed => proto::FirmwareUpdateStateProto::Proposed as i32,
        FirmwareUpdateState::Verified => proto::FirmwareUpdateStateProto::Verified as i32,
        FirmwareUpdateState::Approved => proto::FirmwareUpdateStateProto::Approved as i32,
        FirmwareUpdateState::Staged => proto::FirmwareUpdateStateProto::Staged as i32,
        FirmwareUpdateState::Applying => proto::FirmwareUpdateStateProto::Applying as i32,
        FirmwareUpdateState::Applied => proto::FirmwareUpdateStateProto::Applied as i32,
        FirmwareUpdateState::Failed => proto::FirmwareUpdateStateProto::Failed as i32,
        FirmwareUpdateState::Reverted => proto::FirmwareUpdateStateProto::Reverted as i32,
    }
}

// ── FirmwareTrustResult ↔ proto ───────────────────────────────────────────

pub(crate) const fn trust_result_to_proto(r: FirmwareTrustResult) -> i32 {
    match r {
        FirmwareTrustResult::AiosPublisherSigned => {
            proto::FirmwareTrustResultProto::AiosPublisherSigned as i32
        }
        FirmwareTrustResult::VendorSignedThroughAiosBridge => {
            proto::FirmwareTrustResultProto::VendorSignedThroughAiosBridge as i32
        }
        FirmwareTrustResult::OperatorLocalSigned => {
            proto::FirmwareTrustResultProto::OperatorLocalSignedResult as i32
        }
        FirmwareTrustResult::UnsignedRefused => {
            proto::FirmwareTrustResultProto::UnsignedRefused as i32
        }
        FirmwareTrustResult::RevokedKey => proto::FirmwareTrustResultProto::RevokedKey as i32,
        FirmwareTrustResult::VersionRegression => {
            proto::FirmwareTrustResultProto::VersionRegression as i32
        }
        FirmwareTrustResult::IncompatibleScope => {
            proto::FirmwareTrustResultProto::IncompatibleScope as i32
        }
        FirmwareTrustResult::ConstitutionalRefusal => {
            proto::FirmwareTrustResultProto::ConstitutionalRefusal as i32
        }
    }
}

// ── FirmwareScope ↔ proto ─────────────────────────────────────────────────

const fn firmware_scope_to_proto(s: FirmwareScope) -> i32 {
    match s {
        FirmwareScope::BiosUefi => proto::FirmwareScopeProto::BiosUefi as i32,
        FirmwareScope::Cpu => proto::FirmwareScopeProto::CpuScope as i32,
        FirmwareScope::Gpu => proto::FirmwareScopeProto::GpuScope as i32,
        FirmwareScope::NetworkAdapter => proto::FirmwareScopeProto::NetworkAdapter as i32,
        FirmwareScope::Storage => proto::FirmwareScopeProto::StorageScope as i32,
        FirmwareScope::Thunderbolt => proto::FirmwareScopeProto::ThunderboltScope as i32,
        FirmwareScope::Tpm => proto::FirmwareScopeProto::Tpm as i32,
        FirmwareScope::OtherPeripheral => proto::FirmwareScopeProto::OtherPeripheral as i32,
    }
}

// ── FirmwareApplyStrategy ↔ proto ─────────────────────────────────────────

const fn apply_strategy_to_proto(s: FirmwareApplyStrategy) -> i32 {
    match s {
        FirmwareApplyStrategy::Atomic => proto::FirmwareApplyStrategyProto::Atomic as i32,
        FirmwareApplyStrategy::Staged => proto::FirmwareApplyStrategyProto::StagedApply as i32,
        FirmwareApplyStrategy::Deferred => proto::FirmwareApplyStrategyProto::Deferred as i32,
    }
}

pub(crate) fn apply_strategy_from_proto(v: i32) -> Result<FirmwareApplyStrategy, HardwareError> {
    let p = proto::FirmwareApplyStrategyProto::try_from(v)
        .map_err(|_| HardwareError::Internal(format!("invalid FirmwareApplyStrategyProto: {v}")))?;
    match p {
        proto::FirmwareApplyStrategyProto::Atomic => Ok(FirmwareApplyStrategy::Atomic),
        proto::FirmwareApplyStrategyProto::StagedApply => Ok(FirmwareApplyStrategy::Staged),
        proto::FirmwareApplyStrategyProto::Deferred => Ok(FirmwareApplyStrategy::Deferred),
        proto::FirmwareApplyStrategyProto::FirmwareApplyStrategyUnspecified => {
            Err(HardwareError::Internal("unspecified apply strategy".into()))
        }
    }
}

// ── FirmwareUpdateClass ↔ proto ───────────────────────────────────────────

const fn firmware_update_class_to_proto(c: FirmwareUpdateClass) -> i32 {
    match c {
        FirmwareUpdateClass::CpuMicrocode => proto::FirmwareUpdateClassProto::CpuMicrocode as i32,
        FirmwareUpdateClass::GpuFirmware => proto::FirmwareUpdateClassProto::GpuFirmware as i32,
        FirmwareUpdateClass::NetworkFirmware => {
            proto::FirmwareUpdateClassProto::NetworkFirmware as i32
        }
        FirmwareUpdateClass::StorageFirmware => {
            proto::FirmwareUpdateClassProto::StorageFirmware as i32
        }
        FirmwareUpdateClass::PeripheralFirmware => {
            proto::FirmwareUpdateClassProto::PeripheralFirmware as i32
        }
    }
}

// ── HardwareDeviceRecord ↔ proto ──────────────────────────────────────────

pub(crate) fn device_record_to_proto(r: &HardwareDeviceRecord) -> proto::HardwareDeviceRecordProto {
    proto::HardwareDeviceRecordProto {
        device_id: r.device_id.0.clone(),
        class: device_class_to_proto(r.class),
        bus: bus_kind_to_proto(r.bus),
        vendor_id: u32::from(r.vendor_id),
        product_id: u32::from(r.product_id),
        vendor_name: r.vendor_name.clone(),
        product_name: r.product_name.clone(),
        trust_class: trust_class_to_proto(r.trust_class),
        lifecycle: lifecycle_to_proto(r.lifecycle),
        driver_provenance: r.driver_provenance.map(provenance_to_proto),
        firmware_version: r.firmware_version.clone(),
        removable: r.removable,
        iommu_protected: r.iommu_protected,
        probed_at: Some(datetime_to_proto(r.probed_at)),
    }
}

pub(crate) fn device_record_from_proto(
    p: &proto::HardwareDeviceRecordProto,
) -> Result<HardwareDeviceRecord, HardwareError> {
    Ok(HardwareDeviceRecord {
        device_id: DeviceId(p.device_id.clone()),
        class: device_class_from_proto(p.class)?,
        bus: bus_kind_from_proto(p.bus)?,
        vendor_id: p.vendor_id as u16,
        product_id: p.product_id as u16,
        vendor_name: p.vendor_name.clone(),
        product_name: p.product_name.clone(),
        trust_class: DeviceTrustClass::Untrusted,
        lifecycle: lifecycle_from_proto(p.lifecycle)?,
        driver_provenance: p.driver_provenance.map(|v| {
            let _ = proto::DriverProvenanceProto::try_from(v);
            DriverProvenance::OperatorLocalSigned
        }),
        firmware_version: p.firmware_version.clone(),
        removable: p.removable,
        iommu_protected: p.iommu_protected,
        probed_at: datetime_from_proto(p.probed_at),
    })
}

// ── RawDeviceObservation ↔ proto ──────────────────────────────────────────

pub(crate) fn observation_from_proto(p: &proto::RawDeviceObservationProto) -> RawDeviceObservation {
    RawDeviceObservation {
        bus: bus_kind_from_proto(p.bus).unwrap_or(BusKind::Pci),
        bus_address: p.bus_address.clone(),
        vendor_id: p.vendor_id as u16,
        product_id: p.product_id as u16,
        class_hint: p.class_hint,
        vendor_name: p.vendor_name.clone(),
        product_name: p.product_name.clone(),
        removable_hint: p.removable_hint,
        iommu_protected_hint: p.iommu_protected_hint,
        firmware_version_hint: p.firmware_version_hint.clone(),
    }
}

// ── DriverBinding ↔ proto ─────────────────────────────────────────────────

pub(crate) fn driver_binding_from_proto(
    p: &proto::DriverBindingProto,
) -> Result<DriverBinding, HardwareError> {
    Ok(DriverBinding {
        binding_id: DriverBindingId(p.binding_id.clone()),
        device_id: DeviceId(p.device_id.clone()),
        driver_module_name: p.driver_module_name.clone(),
        kernel_module_version: p.kernel_module_version.clone(),
        provenance: provenance_from_proto(p.provenance)?,
        blake3_hash: p.blake3_hash.clone(),
        signer_fingerprint: p.signer_fingerprint.clone(),
        signature: p.signature.clone(),
        admitted_at: datetime_from_proto(p.admitted_at),
    })
}

pub(crate) fn driver_binding_to_proto(b: &DriverBinding) -> proto::DriverBindingProto {
    proto::DriverBindingProto {
        binding_id: b.binding_id.0.clone(),
        device_id: b.device_id.0.clone(),
        driver_module_name: b.driver_module_name.clone(),
        kernel_module_version: b.kernel_module_version.clone(),
        provenance: provenance_to_proto(b.provenance),
        blake3_hash: b.blake3_hash.clone(),
        signer_fingerprint: b.signer_fingerprint.clone(),
        signature: b.signature.clone(),
        admitted_at: Some(datetime_to_proto(b.admitted_at)),
    }
}

// ── HardwareGraph ↔ proto ─────────────────────────────────────────────────

pub(crate) fn graph_to_proto(g: &HardwareGraph) -> proto::HardwareGraphProto {
    let devices: Vec<proto::HardwareDeviceRecordProto> =
        g.devices.values().map(device_record_to_proto).collect();
    proto::HardwareGraphProto {
        graph_id: g.id.0.clone(),
        devices,
        built_at: Some(datetime_to_proto(g.built_at)),
        host_canonical_id: g.host_canonical_id.clone(),
        signer_fingerprint: g.signer_fingerprint.clone(),
        signature: g.signature.clone(),
    }
}

pub(crate) fn graph_from_proto(
    p: &proto::HardwareGraphProto,
) -> Result<HardwareGraph, HardwareError> {
    let mut device_map = std::collections::BTreeMap::new();
    for d in &p.devices {
        let rec = device_record_from_proto(d)?;
        device_map.insert(rec.device_id.clone(), rec);
    }
    Ok(HardwareGraph {
        id: HardwareGraphId(p.graph_id.clone()),
        devices: device_map,
        built_at: datetime_from_proto(p.built_at),
        host_canonical_id: p.host_canonical_id.clone(),
        signer_fingerprint: p.signer_fingerprint.clone(),
        signature: p.signature.clone(),
    })
}

// ── DriftSignal ↔ proto ───────────────────────────────────────────────────

pub(crate) fn drift_signal_to_proto(s: &DriftSignal) -> proto::DriftSignalProto {
    match s {
        DriftSignal::NoDrift => proto::DriftSignalProto {
            signal: proto::drift_signal_proto::Signal::NoDrift as i32,
            prior_graph_id: None,
            current_graph_id: None,
            diff: None,
        },
        DriftSignal::FirstBoot { current } => proto::DriftSignalProto {
            signal: proto::drift_signal_proto::Signal::FirstBoot as i32,
            prior_graph_id: None,
            current_graph_id: Some(current.0.clone()),
            diff: None,
        },
        DriftSignal::DriftDetected {
            prior,
            current,
            change,
        } => proto::DriftSignalProto {
            signal: proto::drift_signal_proto::Signal::DriftDetected as i32,
            prior_graph_id: Some(prior.0.clone()),
            current_graph_id: Some(current.0.clone()),
            diff: Some(proto::GraphDiffProto {
                added: change.added.iter().map(|id| id.0.clone()).collect(),
                removed: change.removed.iter().map(|id| id.0.clone()).collect(),
                modified: change.modified.iter().map(|id| id.0.clone()).collect(),
                kept: change.kept as u32,
            }),
        },
    }
}

// ── CapabilityLieOutcome → proto ──────────────────────────────────────────

pub(crate) fn lie_outcome_to_proto(o: &CapabilityLieOutcome) -> proto::CapabilityLieOutcomeProto {
    match o {
        CapabilityLieOutcome::Match => proto::CapabilityLieOutcomeProto {
            outcome: proto::capability_lie_outcome_proto::Outcome::Match as i32,
            device_id: None,
            key: None,
            advertised: None,
            observed: None,
            severity: None,
        },
        CapabilityLieOutcome::Lie {
            device,
            key,
            advertised,
            observed,
            severity,
        } => proto::CapabilityLieOutcomeProto {
            outcome: proto::capability_lie_outcome_proto::Outcome::Lie as i32,
            device_id: Some(device.0.clone()),
            key: Some(key.clone()),
            advertised: Some(advertised.clone()),
            observed: Some(observed.clone()),
            severity: Some(lie_severity_to_proto(*severity)),
        },
    }
}

// ── GpuDevice ↔ proto ─────────────────────────────────────────────────────

pub(crate) fn gpu_device_to_proto(d: &GpuDevice) -> proto::GpuDeviceProto {
    proto::GpuDeviceProto {
        gpu_id: d.gpu_id.0.clone(),
        vendor: gpu_vendor_to_proto(d.vendor),
        product_name: d.product_name.clone(),
        vram_total_bytes: d.vram_total_bytes,
        supported_classes: d
            .supported_classes
            .iter()
            .map(|c| gpu_capability_to_proto(*c))
            .collect(),
        iommu_protected: d.iommu_protected,
        host_canonical_id: d.host_canonical_id.clone(),
    }
}

pub(crate) fn gpu_device_from_proto(p: &proto::GpuDeviceProto) -> Result<GpuDevice, HardwareError> {
    let classes: Result<Vec<GpuCapabilityClass>, HardwareError> = p
        .supported_classes
        .iter()
        .map(|v| gpu_capability_from_proto(*v))
        .collect();
    Ok(GpuDevice {
        gpu_id: GpuId(p.gpu_id.clone()),
        vendor: {
            let vp = proto::GpuVendorKindProto::try_from(p.vendor)
                .unwrap_or(proto::GpuVendorKindProto::Other);
            match vp {
                proto::GpuVendorKindProto::Amd => GpuVendorKind::Amd,
                proto::GpuVendorKindProto::Intel => GpuVendorKind::Intel,
                proto::GpuVendorKindProto::Nvidia => GpuVendorKind::Nvidia,
                proto::GpuVendorKindProto::Arm => GpuVendorKind::Arm,
                proto::GpuVendorKindProto::Apple => GpuVendorKind::Apple,
                _ => GpuVendorKind::Other,
            }
        },
        product_name: p.product_name.clone(),
        vram_total_bytes: p.vram_total_bytes,
        supported_classes: classes?,
        iommu_protected: p.iommu_protected,
        host_canonical_id: p.host_canonical_id.clone(),
    })
}

// ── BindingRequest from proto ─────────────────────────────────────────────

pub(crate) fn binding_request_from_proto(
    p: &proto::BindingRequestProto,
) -> Result<BindingRequest, HardwareError> {
    Ok(BindingRequest {
        gpu_id: GpuId(p.gpu_id.clone()),
        group_id: p.group_id.clone(),
        subject_canonical_id: p.subject_canonical_id.clone(),
        capability_class: gpu_capability_from_proto(p.capability_class)?,
        vram_bytes: p.vram_bytes,
        ttl: p
            .ttl_seconds
            .map(|s| std::time::Duration::from_secs(u64::from(s)))
            .map(chrono::Duration::from_std)
            .transpose()
            .map_err(|e| HardwareError::Internal(format!("invalid ttl: {e}")))?,
    })
}

// ── GpuCapabilityBinding → proto ──────────────────────────────────────────

pub(crate) fn binding_to_proto(b: &GpuCapabilityBinding) -> proto::GpuCapabilityBindingProto {
    proto::GpuCapabilityBindingProto {
        binding_id: b.binding_id.clone(),
        gpu_id: b.gpu_id.0.clone(),
        group_id: b.group_id.clone(),
        subject_canonical_id: b.subject_canonical_id.clone(),
        capability_class: gpu_capability_to_proto(b.capability_class),
        vram_bytes_reserved: b.vram_bytes_reserved,
        vk_device_partition_id: b.vk_device_partition_id.clone(),
        bound_at: Some(datetime_to_proto(b.bound_at)),
        expires_at: b.expires_at.map(datetime_to_proto),
    }
}

// ── VkDevicePartition → proto ─────────────────────────────────────────────

pub(crate) fn partition_to_proto(p: &VkDevicePartition) -> proto::VkDevicePartitionProto {
    proto::VkDevicePartitionProto {
        partition_id: p.partition_id.clone(),
        gpu_id: p.gpu_id.0.clone(),
        group_id: p.group_id.clone(),
        created_at: Some(datetime_to_proto(p.created_at)),
        authorized_subjects: p.authorized_subjects.clone(),
    }
}

// ── VramAccounting → proto ────────────────────────────────────────────────

pub(crate) fn accounting_to_proto(a: &VramAccounting) -> proto::VramAccountingProto {
    proto::VramAccountingProto {
        gpu_id: a.gpu_id.0.clone(),
        group_id: a.group_id.clone(),
        subject_canonical_id: a.subject_canonical_id.clone(),
        capability_class: gpu_capability_to_proto(a.capability_class),
        bytes_used: a.bytes_used,
        bytes_reserved: a.bytes_reserved,
    }
}

// ── DmabufHandle ↔ proto ─────────────────────────────────────────────────

pub(crate) fn dmabuf_handle_from_proto(p: &proto::DmabufHandleProto) -> DmabufHandle {
    DmabufHandle {
        handle_id: p.handle_id.clone(),
        source_gpu: GpuId(p.source_gpu.clone()),
        source_group: p.source_group.clone(),
        source_subject: p.source_subject.clone(),
        size_bytes: p.size_bytes,
        format_code: p.format_code,
        created_at: datetime_from_proto(p.created_at),
    }
}

// ── DmabufPeerSet from proto ─────────────────────────────────────────────

pub(crate) fn dmabuf_peer_set_from_proto(p: &proto::DmabufPeerSetProto) -> DmabufPeerSet {
    DmabufPeerSet {
        handle_id: p.handle_id.clone(),
        authorized_peers: p
            .authorized_peers
            .iter()
            .map(|peer| DmabufPeer {
                target_gpu: GpuId(peer.target_gpu.clone()),
                target_group: peer.target_group.clone(),
                target_subject: peer.target_subject.clone(),
            })
            .collect(),
        policy_decision_id: p.policy_decision_id.clone(),
    }
}

// ── FirmwareBlob ↔ proto ──────────────────────────────────────────────────

pub(crate) fn firmware_blob_from_proto(
    p: &proto::FirmwareBlobProto,
) -> Result<FirmwareBlob, HardwareError> {
    let uc = proto::FirmwareUpdateClassProto::try_from(p.update_class)
        .map_err(|_| HardwareError::Internal("invalid firmware update class".into()))?;
    let update_class = match uc {
        proto::FirmwareUpdateClassProto::CpuMicrocode => FirmwareUpdateClass::CpuMicrocode,
        proto::FirmwareUpdateClassProto::GpuFirmware => FirmwareUpdateClass::GpuFirmware,
        proto::FirmwareUpdateClassProto::NetworkFirmware => FirmwareUpdateClass::NetworkFirmware,
        proto::FirmwareUpdateClassProto::StorageFirmware => FirmwareUpdateClass::StorageFirmware,
        proto::FirmwareUpdateClassProto::PeripheralFirmware => {
            FirmwareUpdateClass::PeripheralFirmware
        }
        _ => {
            return Err(HardwareError::Internal(
                "unspecified firmware update class".into(),
            ))
        }
    };
    let sc = proto::FirmwareScopeProto::try_from(p.scope)
        .map_err(|_| HardwareError::Internal("invalid firmware scope".into()))?;
    let scope = match sc {
        proto::FirmwareScopeProto::BiosUefi => FirmwareScope::BiosUefi,
        proto::FirmwareScopeProto::CpuScope => FirmwareScope::Cpu,
        proto::FirmwareScopeProto::GpuScope => FirmwareScope::Gpu,
        proto::FirmwareScopeProto::NetworkAdapter => FirmwareScope::NetworkAdapter,
        proto::FirmwareScopeProto::StorageScope => FirmwareScope::Storage,
        proto::FirmwareScopeProto::ThunderboltScope => FirmwareScope::Thunderbolt,
        proto::FirmwareScopeProto::Tpm => FirmwareScope::Tpm,
        proto::FirmwareScopeProto::OtherPeripheral => FirmwareScope::OtherPeripheral,
        _ => return Err(HardwareError::Internal("unspecified firmware scope".into())),
    };
    Ok(FirmwareBlob {
        blob_id: FirmwareBlobId(p.blob_id.clone()),
        update_class,
        scope,
        target_device: p.target_device.clone().map(DeviceId),
        vendor_name: p.vendor_name.clone(),
        version: p.version.clone(),
        blake3_hash: p.blake3_hash.clone(),
        signature: p.signature.clone(),
        signer_fingerprint: p.signer_fingerprint.clone(),
        published_at: datetime_from_proto(p.published_at),
    })
}

// ── FirmwareUpdatePlan → proto ────────────────────────────────────────────

pub(crate) fn firmware_plan_to_proto(plan: &FirmwareUpdatePlan) -> proto::FirmwareUpdatePlanProto {
    proto::FirmwareUpdatePlanProto {
        blob: Some(proto::FirmwareBlobProto {
            blob_id: plan.blob.blob_id.0.clone(),
            update_class: firmware_update_class_to_proto(plan.blob.update_class),
            scope: firmware_scope_to_proto(plan.blob.scope),
            target_device: plan.blob.target_device.as_ref().map(|d| d.0.clone()),
            vendor_name: plan.blob.vendor_name.clone(),
            version: plan.blob.version.clone(),
            blake3_hash: plan.blob.blake3_hash.clone(),
            signature: plan.blob.signature.clone(),
            signer_fingerprint: plan.blob.signer_fingerprint.clone(),
            published_at: Some(datetime_to_proto(plan.blob.published_at)),
        }),
        current_state: firmware_state_to_proto(plan.current_state),
        apply_strategy: apply_strategy_to_proto(plan.apply_strategy),
        trust_result: plan.trust_result.map(trust_result_to_proto),
        history: plan
            .history
            .iter()
            .map(|e| proto::FirmwareStageEntryProto {
                state: firmware_state_to_proto(e.state),
                transitioned_at: Some(datetime_to_proto(e.transitioned_at)),
                note: e.note.clone(),
            })
            .collect(),
        installed_version_before: plan.installed_version_before.clone(),
    }
}

// ── hardware_error_to_status ──────────────────────────────────────────────

/// Map a [`HardwareError`] to a [`tonic::Status`] for gRPC responses.
#[must_use]
pub fn hardware_error_to_status(err: &HardwareError) -> Status {
    match err {
        HardwareError::DeviceNotFound(id) => {
            Status::not_found(format!("device not found: {id:?}"))
        }
        HardwareError::ClassificationFailed { device, reason } => {
            Status::invalid_argument(format!("classification failed for {device:?}: {reason}"))
        }
        HardwareError::DriverBindingFailed { device, reason } => {
            Status::permission_denied(format!("driver binding failed for {device:?}: {reason}"))
        }
        HardwareError::DriftFromPriorBoot { prior_graph_id, current_graph_id, changed_devices } => {
            Status::failed_precondition(format!(
                "hardware graph drift: prior {prior_graph_id:?} vs current {current_graph_id:?}, changed: {changed_devices:?}"
            ))
        }
        HardwareError::CapabilityLie { device, advertised, observed } => {
            Status::permission_denied(format!(
                "capability lie: {device:?} advertised {advertised} but observed {observed}"
            ))
        }
        HardwareError::ThunderboltUnauthorized(id) => {
            Status::permission_denied(format!("thunderbolt unauthorized: {id:?}"))
        }
        HardwareError::IommuMissing(id) => {
            Status::failed_precondition(format!("IOMMU missing for device: {id:?}"))
        }
        HardwareError::RemovableDenied { device, policy } => {
            Status::permission_denied(format!(
                "removable device denied: {device:?} by policy {policy:?}"
            ))
        }
        HardwareError::GpuVramExhausted { gpu, requested, available } => {
            Status::resource_exhausted(format!(
                "GPU VRAM exhausted: {gpu:?} requested {requested} but only {available} available"
            ))
        }
        HardwareError::GpuBindingInvalid { gpu, reason } => {
            Status::invalid_argument(format!("GPU binding invalid: {gpu:?}: {reason}"))
        }
        HardwareError::DmabufPeerUnauthorized { src, target } => {
            Status::permission_denied(format!(
                "dmabuf peer unauthorized: src {src:?} -> dst {target:?}"
            ))
        }
        HardwareError::FirmwareUnsigned(id) => {
            Status::permission_denied(format!("firmware unsigned: {id:?}"))
        }
        HardwareError::FirmwareSignatureInvalid { blob, reason } => {
            Status::permission_denied(format!("firmware signature invalid: {blob:?}: {reason}"))
        }
        HardwareError::FirmwareVersionRegression { blob, attempted, installed } => {
            Status::failed_precondition(format!(
                "firmware version regression: {blob:?} attempted {attempted} but {installed} installed"
            ))
        }
        HardwareError::FirmwareScopeMismatch { blob, expected, advertised } => {
            Status::invalid_argument(format!(
                "firmware scope mismatch: {blob:?} expected {expected:?} but advertised {advertised:?}"
            ))
        }
        HardwareError::FirmwareRefusedConstitutional { blob, reason } => {
            Status::failed_precondition(format!(
                "firmware refused on constitutional grounds: {blob:?}: {reason}"
            ))
        }
        HardwareError::FirmwareApplyFailed { blob, reason } => {
            Status::internal(format!("firmware apply failed: {blob:?}: {reason}"))
        }
        HardwareError::GraphSnapshotSignatureInvalid(id) => {
            Status::permission_denied(format!("graph snapshot signature invalid: {id:?}"))
        }
        HardwareError::Internal(msg) => {
            Status::internal(format!("internal hardware error: {msg}"))
        }
    }
}
