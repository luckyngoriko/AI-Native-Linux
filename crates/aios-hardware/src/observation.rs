#![allow(missing_docs)]

use chrono::{DateTime, Utc};

use crate::bus::BusKind;

/// A raw device observation provided by the caller (e.g. a bus scanner).
///
/// The `class_hint` encoding differs by bus:
/// - PCI/PCIe: 24-bit class code (base[23:16] + sub[15:8] + prog-if[7:0]).
/// - USB:      bDeviceClass[15:8] + bDeviceSubClass[7:0] (protocol ignored).
/// - Other:    bus-specific, see S8.3 §3.1.
#[derive(Debug, Clone)]
pub struct RawDeviceObservation {
    pub bus: BusKind,
    pub bus_address: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub class_hint: u32,
    pub vendor_name: Option<String>,
    pub product_name: Option<String>,
    pub removable_hint: bool,
    pub iommu_protected_hint: bool,
    pub firmware_version_hint: Option<String>,
}

/// A batch of observations collected during a single enumeration pass.
#[derive(Debug, Clone)]
pub struct EnumerationBatch {
    pub host_canonical_id: String,
    pub observations: Vec<RawDeviceObservation>,
    pub observed_at: DateTime<Utc>,
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn raw_device_observation_construction_and_field_access() {
        let obs = RawDeviceObservation {
            bus: BusKind::Pcie,
            bus_address: "0000:00:02.0".into(),
            vendor_id: 0x8086,
            product_id: 0x9A49,
            class_hint: 0x03_00_00,
            vendor_name: Some("Intel".into()),
            product_name: Some("Iris Xe".into()),
            removable_hint: false,
            iommu_protected_hint: true,
            firmware_version_hint: None,
        };
        assert_eq!(obs.bus, BusKind::Pcie);
        assert_eq!(obs.vendor_id, 0x8086);
        assert_eq!(obs.product_id, 0x9A49);
        assert_eq!(obs.class_hint, 0x03_00_00);
    }

    #[test]
    fn enumeration_batch_holds_observations() {
        let batch = EnumerationBatch {
            host_canonical_id: "host-01".into(),
            observations: vec![RawDeviceObservation {
                bus: BusKind::Pci,
                bus_address: "0000:00:00.0".into(),
                vendor_id: 0x8086,
                product_id: 0x0000,
                class_hint: 0x06_00_00,
                vendor_name: None,
                product_name: None,
                removable_hint: false,
                iommu_protected_hint: false,
                firmware_version_hint: None,
            }],
            observed_at: chrono::DateTime::from_timestamp(1_700_000_000, 0)
                .expect("valid fixed unix timestamp"),
        };
        assert_eq!(batch.host_canonical_id, "host-01");
        assert_eq!(batch.observations.len(), 1);
        assert_eq!(batch.observations[0].bus, BusKind::Pci);
    }
}
