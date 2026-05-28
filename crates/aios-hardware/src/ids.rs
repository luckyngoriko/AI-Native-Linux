#![allow(missing_docs)]

use serde::{Deserialize, Serialize};

/// Canonical bus+vendor+product device identifier (e.g., `pci:8086:9a49`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(pub String);

/// GPU identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GpuId(pub String);

/// Hardware graph snapshot identifier (format `hwgraph_<hex32>`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HardwareGraphId(pub String);

/// Firmware blob identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FirmwareBlobId(pub String);

/// Driver binding identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DriverBindingId(pub String);
