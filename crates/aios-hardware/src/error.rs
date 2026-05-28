#![allow(missing_docs)]

use strum_macros::EnumCount;

use crate::firmware::FirmwareScope;
use crate::ids::{DeviceId, FirmwareBlobId, GpuId, HardwareGraphId};
use crate::removable::RemovableDevicePolicy;

/// Closed error code catalogue for pattern matching (19 codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumCount)]
pub enum HardwareErrorCode {
    DeviceNotFound,
    ClassificationFailed,
    DriverBindingFailed,
    DriftFromPriorBoot,
    CapabilityLie,
    ThunderboltUnauthorized,
    IommuMissing,
    RemovableDenied,
    GpuVramExhausted,
    GpuBindingInvalid,
    DmabufPeerUnauthorized,
    FirmwareUnsigned,
    FirmwareSignatureInvalid,
    FirmwareVersionRegression,
    FirmwareScopeMismatch,
    FirmwareRefusedConstitutional,
    FirmwareApplyFailed,
    GraphSnapshotSignatureInvalid,
    Internal,
}

/// Typed hardware error catalogue covering S8.3 + S8.2 + S8.5 invariants.
#[derive(Debug, thiserror::Error)]
pub enum HardwareError {
    #[error("device not found: {0:?}")]
    DeviceNotFound(DeviceId),

    #[error("classification failed for device {device:?}: {reason}")]
    ClassificationFailed { device: DeviceId, reason: String },

    #[error("driver binding failed for device {device:?}: {reason}")]
    DriverBindingFailed { device: DeviceId, reason: String },

    #[error("hardware graph drift: prior {prior_graph_id:?} vs current {current_graph_id:?}, changed: {changed_devices:?}")]
    DriftFromPriorBoot {
        prior_graph_id: HardwareGraphId,
        current_graph_id: HardwareGraphId,
        changed_devices: Vec<DeviceId>,
    },

    #[error("capability lie: device {device:?} advertised {advertised} but observed {observed}")]
    CapabilityLie {
        device: DeviceId,
        advertised: String,
        observed: String,
    },

    #[error("thunderbolt unauthorized: {0:?}")]
    ThunderboltUnauthorized(DeviceId),

    #[error("IOMMU missing for device: {0:?}")]
    IommuMissing(DeviceId),

    #[error("removable device denied: {device:?} by policy {policy:?}")]
    RemovableDenied {
        device: DeviceId,
        policy: RemovableDevicePolicy,
    },

    #[error("GPU VRAM exhausted: {gpu:?} requested {requested} but only {available} available")]
    GpuVramExhausted {
        gpu: GpuId,
        requested: u64,
        available: u64,
    },

    #[error("GPU binding invalid: {gpu:?}: {reason}")]
    GpuBindingInvalid { gpu: GpuId, reason: String },

    #[error("dmabuf peer unauthorized: src {src:?} -> dst {target:?}")]
    DmabufPeerUnauthorized { src: GpuId, target: GpuId },

    #[error("firmware unsigned: {0:?}")]
    FirmwareUnsigned(FirmwareBlobId),

    #[error("firmware signature invalid: {blob:?}: {reason}")]
    FirmwareSignatureInvalid {
        blob: FirmwareBlobId,
        reason: String,
    },

    #[error(
        "firmware version regression: {blob:?} attempted {attempted} but {installed} is installed"
    )]
    FirmwareVersionRegression {
        blob: FirmwareBlobId,
        attempted: String,
        installed: String,
    },

    #[error(
        "firmware scope mismatch: {blob:?} expected {expected:?} but advertised {advertised:?}"
    )]
    FirmwareScopeMismatch {
        blob: FirmwareBlobId,
        expected: FirmwareScope,
        advertised: FirmwareScope,
    },

    #[error("firmware refused on constitutional grounds: {blob:?}: {reason}")]
    FirmwareRefusedConstitutional {
        blob: FirmwareBlobId,
        reason: String,
    },

    #[error("firmware apply failed: {blob:?}: {reason}")]
    FirmwareApplyFailed {
        blob: FirmwareBlobId,
        reason: String,
    },

    #[error("graph snapshot signature invalid: {0:?}")]
    GraphSnapshotSignatureInvalid(HardwareGraphId),

    #[error("internal hardware error: {0}")]
    Internal(String),
}

impl HardwareError {
    #[must_use]
    pub const fn code(&self) -> HardwareErrorCode {
        match self {
            Self::DeviceNotFound(_) => HardwareErrorCode::DeviceNotFound,
            Self::ClassificationFailed { .. } => HardwareErrorCode::ClassificationFailed,
            Self::DriverBindingFailed { .. } => HardwareErrorCode::DriverBindingFailed,
            Self::DriftFromPriorBoot { .. } => HardwareErrorCode::DriftFromPriorBoot,
            Self::CapabilityLie { .. } => HardwareErrorCode::CapabilityLie,
            Self::ThunderboltUnauthorized(_) => HardwareErrorCode::ThunderboltUnauthorized,
            Self::IommuMissing(_) => HardwareErrorCode::IommuMissing,
            Self::RemovableDenied { .. } => HardwareErrorCode::RemovableDenied,
            Self::GpuVramExhausted { .. } => HardwareErrorCode::GpuVramExhausted,
            Self::GpuBindingInvalid { .. } => HardwareErrorCode::GpuBindingInvalid,
            Self::DmabufPeerUnauthorized { .. } => HardwareErrorCode::DmabufPeerUnauthorized,
            Self::FirmwareUnsigned(_) => HardwareErrorCode::FirmwareUnsigned,
            Self::FirmwareSignatureInvalid { .. } => HardwareErrorCode::FirmwareSignatureInvalid,
            Self::FirmwareVersionRegression { .. } => HardwareErrorCode::FirmwareVersionRegression,
            Self::FirmwareScopeMismatch { .. } => HardwareErrorCode::FirmwareScopeMismatch,
            Self::FirmwareRefusedConstitutional { .. } => {
                HardwareErrorCode::FirmwareRefusedConstitutional
            }
            Self::FirmwareApplyFailed { .. } => HardwareErrorCode::FirmwareApplyFailed,
            Self::GraphSnapshotSignatureInvalid(_) => {
                HardwareErrorCode::GraphSnapshotSignatureInvalid
            }
            Self::Internal(_) => HardwareErrorCode::Internal,
        }
    }
}
