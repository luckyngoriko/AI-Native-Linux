#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Closed vocabulary for firmware update classes (S8.5 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount)]
pub enum FirmwareUpdateClass {
    CpuMicrocode,
    GpuFirmware,
    NetworkFirmware,
    StorageFirmware,
    PeripheralFirmware,
}

/// Closed vocabulary for firmware scope (S8.5 §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount)]
pub enum FirmwareScope {
    BiosUefi,
    Cpu,
    Gpu,
    NetworkAdapter,
    Storage,
    Thunderbolt,
    Tpm,
    OtherPeripheral,
}

/// Closed vocabulary for firmware update lifecycle state (S8.5 §3.3).
/// Ordered from earliest to latest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount, EnumIter)]
pub enum FirmwareUpdateState {
    Proposed,
    Verified,
    Approved,
    Staged,
    Applying,
    Applied,
    Failed,
    Reverted,
}

/// Closed vocabulary for firmware trust verification results (S8.5 §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount, EnumIter)]
pub enum FirmwareTrustResult {
    AiosPublisherSigned,
    VendorSignedThroughAiosBridge,
    OperatorLocalSigned,
    UnsignedRefused,
    RevokedKey,
    VersionRegression,
    IncompatibleScope,
    ConstitutionalRefusal,
}

/// Closed vocabulary for firmware apply strategy (S8.5 §3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount)]
pub enum FirmwareApplyStrategy {
    Atomic,
    Staged,
    Deferred,
}

/// Closed vocabulary for firmware deferral reasons (S8.5 §3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount)]
pub enum FirmwareDeferReason {
    BatteryNotPluggedIn,
    ActiveSession,
    AppliesAtNextBoot,
    PendingOperatorApproval,
    PendingRecoveryWindow,
}
