//! Network bridge: `HardwareGraph` → `NetworkPostureHint`.
//!
//! Provides a lightweight typed shape converter from the hardware graph's device
//! inventory to network posture metadata.  The `NetworkPolicyController` can use
//! these hints to recommend exposure tightening (e.g. when a Thunderbolt device
//! just appeared).  This bridge is **read-only metadata** — it does not mutate
//! network policy.

use serde::{Deserialize, Serialize};

use crate::bus::BusKind;
use crate::device::DeviceClass;
use crate::graph::HardwareGraph;

/// Lightweight posture hint derived from a [`HardwareGraph`] snapshot.
///
/// Pure metadata — no policy mutation.  The network layer can consume this to
/// tighten exposure posture when new risky devices appear.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkPostureHint {
    /// At least one Thunderbolt/USB4 device is present in the graph.
    pub has_thunderbolt: bool,
    /// At least one `WiFi` adapter is present.
    pub has_wifi: bool,
    /// At least one Ethernet adapter is present.
    pub has_ethernet: bool,
    /// At least one discrete GPU is present.
    pub has_discrete_gpu: bool,
    /// Total number of devices in the snapshot.
    pub device_count: usize,
    /// Number of devices with IOMMU DMA protection active.
    pub iommu_protected_count: usize,
}

impl NetworkPostureHint {
    /// Returns `true` if any posture-relevant risk signal is present.
    ///
    /// Currently: Thunderbolt devices are the primary network posture risk signal.
    #[must_use]
    pub const fn has_risk_signal(&self) -> bool {
        self.has_thunderbolt
    }
}

/// Build a `NetworkPostureHint` from a hardware graph snapshot.
///
/// Iterates every device in the graph and aggregates bus-class metadata.
#[must_use]
pub fn graph_summary(graph: &HardwareGraph) -> NetworkPostureHint {
    let mut has_thunderbolt = false;
    let mut has_wifi = false;
    let mut has_ethernet = false;
    let mut has_discrete_gpu = false;
    let mut iommu_protected_count = 0_usize;

    for record in graph.devices.values() {
        // Thunderbolt / USB4 bus devices signal risk.
        if matches!(record.bus, BusKind::Thunderbolt | BusKind::Usb4) {
            has_thunderbolt = true;
        }

        // Class-based posture signals.
        match record.class {
            DeviceClass::GpuDiscrete => has_discrete_gpu = true,
            DeviceClass::NetworkWifi => has_wifi = true,
            DeviceClass::NetworkEthernet => has_ethernet = true,
            _ => {}
        }

        if record.iommu_protected {
            iommu_protected_count += 1;
        }
    }

    NetworkPostureHint {
        has_thunderbolt,
        has_wifi,
        has_ethernet,
        has_discrete_gpu,
        device_count: graph.devices.len(),
        iommu_protected_count,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test"
)]
mod tests {
    use super::*;
    use crate::device::DeviceClass;
    use crate::device_record::HardwareDeviceRecord;
    use crate::graph::HardwareGraphBuilder;
    use crate::lifecycle::DeviceLifecycleState;
    use crate::trust_class::DeviceTrustClass;
    use chrono::Utc;
    use ed25519_dalek::SigningKey;

    fn make_record(
        id: &str,
        class: DeviceClass,
        bus: BusKind,
        iommu_protected: bool,
    ) -> HardwareDeviceRecord {
        HardwareDeviceRecord {
            device_id: crate::ids::DeviceId(id.into()),
            class,
            bus,
            vendor_id: 0x1234,
            product_id: 0x5678,
            vendor_name: "TestVendor".into(),
            product_name: "TestProduct".into(),
            trust_class: DeviceTrustClass::Untrusted,
            lifecycle: DeviceLifecycleState::Active,
            driver_provenance: None,
            firmware_version: None,
            removable: false,
            iommu_protected,
            probed_at: Utc::now(),
        }
    }

    #[test]
    fn empty_graph_has_no_risk_signals() {
        let sk = SigningKey::generate(&mut rand_core::OsRng);
        let graph = HardwareGraphBuilder::new("test-host")
            .build_and_sign(&sk, "test_fp")
            .unwrap();
        let hint = graph_summary(&graph);
        assert!(!hint.has_thunderbolt);
        assert!(!hint.has_wifi);
        assert!(!hint.has_ethernet);
        assert!(!hint.has_discrete_gpu);
        assert_eq!(hint.device_count, 0);
        assert_eq!(hint.iommu_protected_count, 0);
        assert!(!hint.has_risk_signal());
    }

    #[test]
    fn thunderbolt_device_signals_risk() {
        let sk = SigningKey::generate(&mut rand_core::OsRng);
        let mut builder = HardwareGraphBuilder::new("test-host");
        builder
            .add_device(make_record(
                "tb_001",
                DeviceClass::ThunderboltController,
                BusKind::Thunderbolt,
                false,
            ))
            .unwrap();
        let graph = builder.build_and_sign(&sk, "test_fp").unwrap();
        let hint = graph_summary(&graph);
        assert!(hint.has_thunderbolt);
        assert!(hint.has_risk_signal());
        assert_eq!(hint.device_count, 1);
    }

    #[test]
    fn iommu_protected_count_tracks_protected_devices() {
        let sk = SigningKey::generate(&mut rand_core::OsRng);
        let mut builder = HardwareGraphBuilder::new("test-host");
        builder
            .add_device(make_record(
                "a",
                DeviceClass::NetworkEthernet,
                BusKind::Pcie,
                true,
            ))
            .unwrap();
        builder
            .add_device(make_record(
                "b",
                DeviceClass::NetworkWifi,
                BusKind::Usb3,
                false,
            ))
            .unwrap();
        let graph = builder.build_and_sign(&sk, "test_fp").unwrap();
        let hint = graph_summary(&graph);
        assert!(hint.has_ethernet);
        assert!(hint.has_wifi);
        assert_eq!(hint.device_count, 2);
        assert_eq!(hint.iommu_protected_count, 1);
    }

    #[test]
    fn discrete_gpu_is_detected() {
        let sk = SigningKey::generate(&mut rand_core::OsRng);
        let mut builder = HardwareGraphBuilder::new("test-host");
        builder
            .add_device(make_record(
                "gpu",
                DeviceClass::GpuDiscrete,
                BusKind::Pcie,
                true,
            ))
            .unwrap();
        let graph = builder.build_and_sign(&sk, "test_fp").unwrap();
        let hint = graph_summary(&graph);
        assert!(hint.has_discrete_gpu);
        assert_eq!(hint.device_count, 1);
    }
}
