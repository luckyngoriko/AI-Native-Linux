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
