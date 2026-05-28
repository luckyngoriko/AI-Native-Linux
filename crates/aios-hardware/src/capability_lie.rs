#![allow(
    missing_docs,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::significant_drop_tightening,
    clippy::missing_panics_doc
)]

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::error::HardwareError;
use crate::ids::DeviceId;

// ---------------------------------------------------------------------------
// S8.3 §7 — AdvertisedCapability: what a device claims it supports
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvertisedCapability {
    pub device_id: DeviceId,
    pub key: String,
    pub advertised_value: String,
}

// ---------------------------------------------------------------------------
// S8.3 §7 — ObservedCapability: runtime probe result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservedCapability {
    pub device_id: DeviceId,
    pub key: String,
    pub observed_value: String,
    pub observed_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// S8.3 §7 — LieSeverity: 3-tier classification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LieSeverity {
    Soft,
    Hard,
    Constitutional,
}

// ---------------------------------------------------------------------------
// S8.3 §7 — CapabilityLieOutcome: the result of comparing ad vs obs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CapabilityLieOutcome {
    Match,
    Lie {
        device: DeviceId,
        key: String,
        advertised: String,
        observed: String,
        severity: LieSeverity,
    },
}

// ---------------------------------------------------------------------------
// S8.3 §7 — CapabilityLieDetector: advertised-vs-observed registry
// ---------------------------------------------------------------------------

pub struct CapabilityLieDetector {
    catalogue: RwLock<HashMap<(DeviceId, String), AdvertisedCapability>>,
    severity_table: HashMap<String, LieSeverity>,
}

impl CapabilityLieDetector {
    pub fn new() -> Self {
        let mut severity_table = HashMap::new();
        severity_table.insert("iommu".to_string(), LieSeverity::Hard);
        severity_table.insert("driver_provenance".to_string(), LieSeverity::Constitutional);
        severity_table.insert("tpm_pcr_count".to_string(), LieSeverity::Hard);
        severity_table.insert("firmware_version".to_string(), LieSeverity::Soft);
        severity_table.insert("gpu.max_vram_bytes".to_string(), LieSeverity::Hard);

        Self {
            catalogue: RwLock::new(HashMap::new()),
            severity_table,
        }
    }

    pub async fn advertise(&self, cap: AdvertisedCapability) -> Result<(), HardwareError> {
        let key = (cap.device_id.clone(), cap.key.clone());
        let mut catalogue = self.catalogue.write().await;
        catalogue.insert(key, cap);
        Ok(())
    }

    pub async fn observe(
        &self,
        obs: ObservedCapability,
    ) -> Result<CapabilityLieOutcome, HardwareError> {
        let key = (obs.device_id.clone(), obs.key.clone());
        let catalogue = self.catalogue.read().await;
        let Some(advertised) = catalogue.get(&key) else {
            return Ok(CapabilityLieOutcome::Match);
        };

        if advertised.advertised_value == obs.observed_value {
            return Ok(CapabilityLieOutcome::Match);
        }

        let severity = self
            .severity_table
            .get(&obs.key)
            .copied()
            .unwrap_or(LieSeverity::Hard);

        Ok(CapabilityLieOutcome::Lie {
            device: obs.device_id,
            key: obs.key,
            advertised: advertised.advertised_value.clone(),
            observed: obs.observed_value,
            severity,
        })
    }

    pub async fn list_advertised(&self) -> Vec<AdvertisedCapability> {
        self.catalogue.read().await.values().cloned().collect()
    }
}

impl Default for CapabilityLieDetector {
    fn default() -> Self {
        Self::new()
    }
}
