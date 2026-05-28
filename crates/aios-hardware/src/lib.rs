#![allow(missing_docs)]
//! L8 Hardware Graph + GPU Resource Model + Firmware Trust for AIOS
//! (S8.3 + S8.2 + S8.5).
//!
//! Typed core skeleton: closed vocabularies + error enum + identifier types.
//! Trait, enumeration, classification, driver registry, drift, GPU, firmware
//! FSM, gRPC, evidence, cross-crate land in later tasks.

pub mod bus;
pub mod capability_lie;
pub mod classifier;
pub mod device;
pub mod device_record;
pub mod dmabuf;
pub mod drift;
pub mod driver;
pub mod driver_binding;
pub mod error;
pub mod firmware;
pub mod gpu;
pub mod gpu_resource;
pub mod graph;
pub mod ids;
pub mod lifecycle;
pub mod manager;
pub mod observation;
pub mod removable;
pub mod trust_class;

pub use bus::BusKind;
pub use capability_lie::{
    AdvertisedCapability, CapabilityLieDetector, CapabilityLieOutcome, LieSeverity,
    ObservedCapability,
};
pub use classifier::{classify_batch, classify_batch_into_records, DeviceClassifier};
pub use device::DeviceClass;
pub use device_record::HardwareDeviceRecord;
pub use dmabuf::{DmabufBroker, DmabufHandle, DmabufPeer, DmabufPeerSet};
pub use drift::{
    DriftDetector, DriftSignal, EvilMaidEvidenceMarker, EvilMaidRecommendedAction, GraphDiff,
    PriorGraphStore,
};
pub use driver::DriverProvenance;
pub use driver_binding::{DriverBinding, DriverBindingRegistry, DriverBlacklistEntry};
pub use error::{HardwareError, HardwareErrorCode};
pub use firmware::{
    FirmwareApplyStrategy, FirmwareDeferReason, FirmwareScope, FirmwareTrustResult,
    FirmwareUpdateClass, FirmwareUpdateState,
};
pub use gpu::{GpuCapabilityClass, GpuVendorKind};
pub use gpu_resource::{
    BindingRequest, GpuCapabilityBinding, GpuDevice, GpuResourceRegistry, VkDevicePartition,
    VramAccounting,
};
pub use graph::{HardwareGraph, HardwareGraphBuilder};
pub use ids::{DeviceId, DriverBindingId, FirmwareBlobId, GpuId, HardwareGraphId};
pub use lifecycle::DeviceLifecycleState;
pub use manager::{HardwareManager, InMemoryHardwareManager};
pub use observation::{EnumerationBatch, RawDeviceObservation};
pub use removable::RemovableDevicePolicy;
pub use trust_class::{DeviceQuarantineReason, DeviceTrustClass};

/// Crate version marker used by closure-invariant tests in T-174.
pub const DEFAULT_CODE_VERSION: &str = "aios-hardware/0.0.1-T163";
