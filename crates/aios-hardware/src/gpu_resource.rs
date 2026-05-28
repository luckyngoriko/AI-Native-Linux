#![allow(
    missing_docs,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::significant_drop_tightening
)]

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use tokio::sync::RwLock;
use ulid::Ulid;

use crate::error::HardwareError;
use crate::gpu::GpuCapabilityClass;
use crate::gpu::GpuVendorKind;
use crate::ids::GpuId;

// ---------------------------------------------------------------------------
// S8.2 §4 — GpuDevice topology record
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GpuDevice {
    pub gpu_id: GpuId,
    pub vendor: GpuVendorKind,
    pub product_name: String,
    pub vram_total_bytes: u64,
    pub supported_classes: Vec<GpuCapabilityClass>,
    pub iommu_protected: bool,
    pub host_canonical_id: String,
}

// ---------------------------------------------------------------------------
// S8.2 §5 — GpuCapabilityBinding: lease between a subject and a GPU
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GpuCapabilityBinding {
    pub binding_id: String,
    pub gpu_id: GpuId,
    pub group_id: String,
    pub subject_canonical_id: String,
    pub capability_class: GpuCapabilityClass,
    pub vram_bytes_reserved: u64,
    pub vk_device_partition_id: String,
    pub bound_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// S8.2 §6 — VramAccounting: per-(gpu, group, subject, class) usage view
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct VramAccounting {
    pub gpu_id: GpuId,
    pub group_id: String,
    pub subject_canonical_id: String,
    pub capability_class: GpuCapabilityClass,
    pub bytes_used: u64,
    pub bytes_reserved: u64,
}

// ---------------------------------------------------------------------------
// S8.2 §6 — VkDevicePartition: per-group Vulkan device handle marker
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct VkDevicePartition {
    pub partition_id: String,
    pub gpu_id: GpuId,
    pub group_id: String,
    pub created_at: DateTime<Utc>,
    pub authorized_subjects: Vec<String>,
}

impl VkDevicePartition {
    pub fn new(gpu_id: GpuId, group_id: String) -> Self {
        Self {
            partition_id: Ulid::new().to_string(),
            gpu_id,
            group_id,
            created_at: Utc::now(),
            authorized_subjects: Vec::new(),
        }
    }

    pub fn authorize_subject(&mut self, subject_canonical_id: String) -> Result<(), HardwareError> {
        if !self.authorized_subjects.contains(&subject_canonical_id) {
            self.authorized_subjects.push(subject_canonical_id);
        }
        Ok(())
    }

    pub fn revoke_subject(&mut self, subject_canonical_id: &str) -> Result<(), HardwareError> {
        self.authorized_subjects
            .retain(|s| s != subject_canonical_id);
        Ok(())
    }

    pub fn is_authorized(&self, subject_canonical_id: &str) -> bool {
        self.authorized_subjects
            .contains(&subject_canonical_id.to_string())
    }
}

// ---------------------------------------------------------------------------
// S8.2 §6 — BindingRequest: input shape for request_binding
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BindingRequest {
    pub gpu_id: GpuId,
    pub group_id: String,
    pub subject_canonical_id: String,
    pub capability_class: GpuCapabilityClass,
    pub vram_bytes: u64,
    pub ttl: Option<Duration>,
}

// ---------------------------------------------------------------------------
// S8.2 §6 — GpuResourceRegistry: the central GPU resource manager
// ---------------------------------------------------------------------------

pub struct GpuResourceRegistry {
    devices: RwLock<HashMap<GpuId, GpuDevice>>,
    partitions: RwLock<HashMap<String, VkDevicePartition>>,
    bindings: RwLock<HashMap<String, GpuCapabilityBinding>>,
    accounting: RwLock<HashMap<(GpuId, String, String, GpuCapabilityClass), VramAccounting>>,
}

impl GpuResourceRegistry {
    pub fn new() -> Self {
        Self {
            devices: RwLock::new(HashMap::new()),
            partitions: RwLock::new(HashMap::new()),
            bindings: RwLock::new(HashMap::new()),
            accounting: RwLock::new(HashMap::new()),
        }
    }

    // -- device registration -------------------------------------------------

    pub async fn register_device(&self, device: GpuDevice) -> Result<(), HardwareError> {
        let mut devices = self.devices.write().await;
        if devices.contains_key(&device.gpu_id) {
            return Err(HardwareError::Internal("duplicate gpu_id".into()));
        }
        devices.insert(device.gpu_id.clone(), device);
        Ok(())
    }

    pub async fn list_devices(&self) -> Vec<GpuDevice> {
        self.devices.read().await.values().cloned().collect()
    }

    // -- partition management ------------------------------------------------

    pub async fn ensure_partition(
        &self,
        gpu_id: &GpuId,
        group_id: &str,
    ) -> Result<VkDevicePartition, HardwareError> {
        {
            let partitions = self.partitions.read().await;
            for p in partitions.values() {
                if &p.gpu_id == gpu_id && p.group_id == group_id {
                    return Ok(p.clone());
                }
            }
        }

        let partition = VkDevicePartition::new(gpu_id.clone(), group_id.to_string());
        let mut partitions = self.partitions.write().await;
        // Double-check under write lock to avoid TOCTOU
        for p in partitions.values() {
            if &p.gpu_id == gpu_id && p.group_id == group_id {
                return Ok(p.clone());
            }
        }
        partitions.insert(partition.partition_id.clone(), partition.clone());
        Ok(partition)
    }

    pub async fn list_partitions_for_group(&self, group_id: &str) -> Vec<VkDevicePartition> {
        self.partitions
            .read()
            .await
            .values()
            .filter(|p| p.group_id == group_id)
            .cloned()
            .collect()
    }

    // -- binding lifecycle ---------------------------------------------------

    pub async fn request_binding(
        &self,
        req: BindingRequest,
    ) -> Result<GpuCapabilityBinding, HardwareError> {
        // Resolve device
        let device = {
            let devices = self.devices.read().await;
            devices
                .get(&req.gpu_id)
                .cloned()
                .ok_or_else(|| HardwareError::GpuBindingInvalid {
                    gpu: req.gpu_id.clone(),
                    reason: "unknown gpu".into(),
                })?
        };

        // Check capability class membership
        if !device.supported_classes.contains(&req.capability_class) {
            return Err(HardwareError::GpuBindingInvalid {
                gpu: req.gpu_id,
                reason: "capability not supported".into(),
            });
        }

        // VRAM budget check — sum bytes_reserved across all entries for this gpu
        let current_reserved: u64 = {
            let acct = self.accounting.read().await;
            acct.iter()
                .filter(|((gpu_id, _, _, _), _)| gpu_id == &req.gpu_id)
                .map(|(_, v)| v.bytes_reserved)
                .sum()
        };

        if current_reserved + req.vram_bytes > device.vram_total_bytes {
            let available = device.vram_total_bytes.saturating_sub(current_reserved);
            return Err(HardwareError::GpuVramExhausted {
                gpu: req.gpu_id,
                requested: req.vram_bytes,
                available,
            });
        }

        // Ensure partition and authorize subject
        let partition = self.ensure_partition(&req.gpu_id, &req.group_id).await?;
        {
            let mut partitions = self.partitions.write().await;
            if let Some(p) = partitions.get_mut(&partition.partition_id) {
                p.authorize_subject(req.subject_canonical_id.clone())?;
            }
        }

        // Create binding
        let binding_id = Ulid::new().to_string();
        let now = Utc::now();
        let expires_at = req.ttl.map(|ttl| now + ttl);

        let binding = GpuCapabilityBinding {
            binding_id: binding_id.clone(),
            gpu_id: req.gpu_id.clone(),
            group_id: req.group_id.clone(),
            subject_canonical_id: req.subject_canonical_id.clone(),
            capability_class: req.capability_class,
            vram_bytes_reserved: req.vram_bytes,
            vk_device_partition_id: partition.partition_id.clone(),
            bound_at: now,
            expires_at,
        };

        // Update accounting — increment bytes_reserved, leave bytes_used = 0
        let acct_key = (
            req.gpu_id.clone(),
            req.group_id.clone(),
            req.subject_canonical_id.clone(),
            req.capability_class,
        );
        {
            let mut acct = self.accounting.write().await;
            let entry = acct.entry(acct_key).or_insert_with(|| VramAccounting {
                gpu_id: req.gpu_id.clone(),
                group_id: req.group_id.clone(),
                subject_canonical_id: req.subject_canonical_id,
                capability_class: req.capability_class,
                bytes_used: 0,
                bytes_reserved: 0,
            });
            entry.bytes_reserved += req.vram_bytes;
        }

        // Store binding
        {
            let mut bindings = self.bindings.write().await;
            bindings.insert(binding_id, binding.clone());
        }

        Ok(binding)
    }

    pub async fn release_binding(&self, binding_id: &str) -> Result<(), HardwareError> {
        let binding = {
            let bindings = self.bindings.read().await;
            bindings
                .get(binding_id)
                .cloned()
                .ok_or_else(|| HardwareError::GpuBindingInvalid {
                    gpu: GpuId("unknown_binding".into()),
                    reason: "unknown binding".into(),
                })?
        };

        // Decrement accounting bytes_reserved
        {
            let acct_key = (
                binding.gpu_id.clone(),
                binding.group_id.clone(),
                binding.subject_canonical_id.clone(),
                binding.capability_class,
            );
            let mut acct = self.accounting.write().await;
            if let Some(entry) = acct.get_mut(&acct_key) {
                entry.bytes_reserved = entry
                    .bytes_reserved
                    .saturating_sub(binding.vram_bytes_reserved);
            }
        }

        // Remove binding
        {
            let mut bindings = self.bindings.write().await;
            bindings.remove(binding_id);
        }

        // Optionally remove subject from partition if no remaining binding
        // references that subject on the same gpu+group.
        {
            let bindings = self.bindings.read().await;
            let still_bound = bindings.values().any(|b| {
                b.gpu_id == binding.gpu_id
                    && b.group_id == binding.group_id
                    && b.subject_canonical_id == binding.subject_canonical_id
            });
            if !still_bound {
                let mut partitions = self.partitions.write().await;
                if let Some(p) = partitions.get_mut(&binding.vk_device_partition_id) {
                    let _ = p.revoke_subject(&binding.subject_canonical_id);
                }
            }
        }

        Ok(())
    }

    // -- accounting queries --------------------------------------------------

    pub async fn get_accounting(&self, gpu_id: &GpuId, group_id: &str) -> Vec<VramAccounting> {
        self.accounting
            .read()
            .await
            .iter()
            .filter(|((g, grp, _, _), _)| g == gpu_id && grp == group_id)
            .map(|(_, v)| v.clone())
            .collect()
    }

    pub async fn total_vram_used(&self, gpu_id: &GpuId) -> u64 {
        self.accounting
            .read()
            .await
            .iter()
            .filter(|((g, _, _, _), _)| g == gpu_id)
            .map(|(_, v)| v.bytes_reserved)
            .sum()
    }
}

impl Default for GpuResourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
