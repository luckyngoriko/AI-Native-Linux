#![allow(missing_docs, clippy::missing_errors_doc)]

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::bus::BusKind;
use crate::error::HardwareError;
use crate::evidence::{HardwareEvidenceEmitter, WithEmitter};
use crate::ids::DeviceId;
use crate::removable_policy::RemovableDevicePolicyTable;

// ---------------------------------------------------------------------------
// IommuRequirement
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct IommuRequirement {
    pub device_id: DeviceId,
    pub bus: BusKind,
    pub iommu_required: bool,
    pub iommu_observed: bool,
}

// ---------------------------------------------------------------------------
// IommuFloorEnforcer
// ---------------------------------------------------------------------------

pub struct IommuFloorEnforcer {
    observations: RwLock<HashMap<DeviceId, IommuRequirement>>,
    emitter: Option<Arc<dyn HardwareEvidenceEmitter>>,
}

impl IommuFloorEnforcer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            observations: RwLock::new(HashMap::new()),
            emitter: None,
        }
    }

    #[must_use]
    pub const fn iommu_required_for_bus(bus: BusKind) -> bool {
        matches!(bus, BusKind::Thunderbolt | BusKind::Usb4 | BusKind::Pcie)
    }

    pub async fn record_observation(
        &self,
        device: DeviceId,
        bus: BusKind,
        observed: bool,
    ) -> Result<(), HardwareError> {
        let required = Self::iommu_required_for_bus(bus);
        let requirement = IommuRequirement {
            device_id: device.clone(),
            bus,
            iommu_required: required,
            iommu_observed: observed,
        };
        self.observations
            .write()
            .await
            .insert(device.clone(), requirement);

        if required && !observed {
            if let Some(ref e) = self.emitter {
                if let Err(emit_err) = e.emit_iommu_missing(&device, bus).await {
                    tracing::warn!(%emit_err, "Failed to emit iommu_missing evidence");
                }
            }
            Err(HardwareError::IommuMissing(device))
        } else {
            Ok(())
        }
    }

    pub async fn lookup_requirement(&self, device: &DeviceId) -> Option<IommuRequirement> {
        self.observations.read().await.get(device).cloned()
    }

    pub async fn list_requirements(&self) -> Vec<IommuRequirement> {
        self.observations.read().await.values().cloned().collect()
    }

    pub async fn quarantine_candidates(&self) -> Vec<DeviceId> {
        self.observations
            .read()
            .await
            .iter()
            .filter(|(_, req)| req.iommu_required && !req.iommu_observed)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

impl WithEmitter for IommuFloorEnforcer {
    fn with_emitter(mut self, emitter: Option<Arc<dyn HardwareEvidenceEmitter>>) -> Self {
        self.emitter = emitter;
        self
    }
}

impl Default for IommuFloorEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// evaluate_removable_admission — IOMMU floor + removable policy composition
// ---------------------------------------------------------------------------

pub async fn evaluate_removable_admission(
    removable: &RemovableDevicePolicyTable,
    iommu: &IommuFloorEnforcer,
    device: &DeviceId,
    bus: BusKind,
    requester: &str,
) -> Result<(), HardwareError> {
    // IOMMU floor check first — Thunderbolt/USB4/PCIe must have IOMMU coverage
    if IommuFloorEnforcer::iommu_required_for_bus(bus) {
        let observed = iommu
            .lookup_requirement(device)
            .await
            .is_some_and(|req| req.iommu_observed);
        if !observed {
            return Err(HardwareError::IommuMissing(device.clone()));
        }
    }
    // Delegate to removable policy
    removable.check_mount(device, requester).await
}
