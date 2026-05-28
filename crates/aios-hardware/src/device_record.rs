#![allow(missing_docs, clippy::missing_panics_doc)]

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::bus::BusKind;
use crate::device::DeviceClass;
use crate::driver::DriverProvenance;
use crate::ids::DeviceId;
use crate::lifecycle::DeviceLifecycleState;
use crate::trust_class::DeviceTrustClass;

/// A single hardware device record — the unit of enumeration.
#[derive(Debug, Clone, Serialize)]
pub struct HardwareDeviceRecord {
    pub device_id: DeviceId,
    pub class: DeviceClass,
    pub bus: BusKind,
    pub vendor_id: u16,
    pub product_id: u16,
    pub vendor_name: String,
    pub product_name: String,
    pub trust_class: DeviceTrustClass,
    pub lifecycle: DeviceLifecycleState,
    pub driver_provenance: Option<DriverProvenance>,
    pub firmware_version: Option<String>,
    pub removable: bool,
    pub iommu_protected: bool,
    pub probed_at: DateTime<Utc>,
}

impl HardwareDeviceRecord {
    /// Deterministic JSON byte form used as input to BLAKE3 hashing during
    /// graph snapshot content-addressing.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        // Struct field serialization order is determined by the declaration
        // order above, which is stable for a given compiled binary.  Together
        // with BTreeMap-ordering at the graph level this gives us the
        // determinism required by T-167 cross-boot drift detection.
        #[allow(clippy::expect_used)]
        serde_json::to_vec(self)
            .expect("HardwareDeviceRecord canonical serialization is infallible")
    }
}
