#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use strum_macros::EnumCount;

/// Closed vocabulary for hardware device classes (S8.3 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount)]
pub enum DeviceClass {
    Cpu,
    Memory,
    GpuIntegrated,
    GpuDiscrete,
    NetworkEthernet,
    NetworkWifi,
    NetworkBluetooth,
    StorageNvme,
    StorageSata,
    StorageMmc,
    AudioCard,
    AudioHeadset,
    UsbController,
    ThunderboltController,
    PrinterOrScanner,
    SensorOrInputDevice,
}
