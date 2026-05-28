#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use strum_macros::EnumCount;

/// Closed vocabulary for hardware bus types (S8.3 §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount)]
pub enum BusKind {
    Pci,
    Pcie,
    Usb2,
    Usb3,
    Usb4,
    Thunderbolt,
    Nvme,
    I2c,
}

impl BusKind {
    /// Short label used in device-id construction (e.g. `pci:8086:9a49@0000:00:02.0`).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pci => "pci",
            Self::Pcie => "pcie",
            Self::Usb2 | Self::Usb3 | Self::Usb4 => "usb",
            Self::Thunderbolt => "thunderbolt",
            Self::Nvme => "nvme",
            Self::I2c => "i2c",
        }
    }
}
