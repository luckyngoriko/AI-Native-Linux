#![allow(missing_docs)]
//! L8 Hardware Graph + GPU Resource Model + Firmware Trust for AIOS
//! (S8.3 + S8.2 + S8.5).
//!
//! Typed core skeleton: closed vocabularies + error enum + identifier types.
//! Trait, enumeration, classification, driver registry, drift, GPU, firmware
//! FSM, gRPC, evidence, cross-crate land in later tasks.

pub mod bus;
pub mod device;
pub mod driver;
pub mod error;
pub mod firmware;
pub mod gpu;
pub mod ids;
pub mod lifecycle;
pub mod removable;
pub mod trust_class;

pub use bus::BusKind;
pub use device::DeviceClass;
pub use driver::DriverProvenance;
pub use error::{HardwareError, HardwareErrorCode};
pub use firmware::{
    FirmwareApplyStrategy, FirmwareDeferReason, FirmwareScope, FirmwareTrustResult,
    FirmwareUpdateClass, FirmwareUpdateState,
};
pub use gpu::{GpuCapabilityClass, GpuVendorKind};
pub use ids::{DeviceId, DriverBindingId, FirmwareBlobId, GpuId, HardwareGraphId};
pub use lifecycle::DeviceLifecycleState;
pub use removable::RemovableDevicePolicy;
pub use trust_class::{DeviceQuarantineReason, DeviceTrustClass};

/// Crate version marker used by closure-invariant tests in T-174.
pub const DEFAULT_CODE_VERSION: &str = "aios-hardware/0.0.1-T163";
