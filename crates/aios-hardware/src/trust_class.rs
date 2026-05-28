#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use strum_macros::EnumCount;

/// Closed vocabulary for device trust classification (S8.3 §3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount)]
pub enum DeviceTrustClass {
    RootSigned,
    VendorSigned,
    CommunitySigned,
    OperatorLocal,
    Untrusted,
}

/// Closed vocabulary for device quarantine reasons (S8.3 §3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount)]
pub enum DeviceQuarantineReason {
    UnsignedFirmware,
    OutOfTreeDriver,
    CapabilityLie,
    ThunderboltUnauthorized,
    RemovableDeniedByPolicy,
    DmaBypassRisk,
    DriftFromPriorBoot,
    OperatorRequested,
}
