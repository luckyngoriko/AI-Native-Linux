use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Closed GPU capability class vocabulary (S8.2 §3).
///
/// Exactly five values. Adding or removing a class is a versioned schema change.
/// Downgrade is allowed (e.g. `GpuFull3D` → `GpuRich2D` when budget is tight);
/// upgrade is never automatic.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GpuCapabilityClass {
    /// Blit-only display; no shader operations. 16 MiB VRAM cap.
    GpuPassiveDisplay,
    /// 2D shaders only (vertex + fragment). 64 MiB VRAM cap.
    GpuBasic2d,
    /// 2D + simple 3D, no compute. 256 MiB VRAM cap.
    GpuRich2d,
    /// Full graphics pipeline including ray tracing. 25% of total VRAM.
    GpuFull3d,
    /// Compute pipelines (CUDA, HIP, Metal Performance Shaders). 50% of total VRAM.
    GpuComputeHeavy,
}

/// GPU policy attached to a sandbox profile (S3.2 §19.1 + S8.2 type-level).
///
/// Controls GPU device access, dmabuf passthrough, IOMMU requirements, and
/// per-group partitioning. The runtime safety floor can only tighten these
/// settings — never loosen them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "S3.2 §19.1 defines exactly 4 independent boolean fields"
)]
pub struct GpuPolicy {
    /// The capability class granted to this sandbox.
    pub gpu_capability_class: GpuCapabilityClass,
    /// Whether a dedicated `VkDevice` (or platform equivalent) is required.
    pub vk_device_required: bool,
    /// Whether dmabuf handles may be passed across process boundaries.
    pub dmabuf_passthrough_allowed: bool,
    /// Whether the GPU device is partitioned per group (S4.1 cross-group isolation).
    pub per_group_partitioning: bool,
    /// Whether IOMMU is required for GPU DMA isolation.
    pub iommu_required: bool,
    /// When this GPU policy expires, if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

impl GpuPolicy {
    /// Most restrictive GPU policy — no GPU access.
    #[must_use]
    pub const fn default_deny_all() -> Self {
        Self {
            gpu_capability_class: GpuCapabilityClass::GpuPassiveDisplay,
            vk_device_required: false,
            dmabuf_passthrough_allowed: false,
            per_group_partitioning: true,
            iommu_required: true,
            expires_at: None,
        }
    }

    /// Permissive GPU policy for service-class actions (full 3D, no IOMMU req).
    #[must_use]
    pub const fn default_permissive() -> Self {
        Self {
            gpu_capability_class: GpuCapabilityClass::GpuFull3d,
            vk_device_required: true,
            dmabuf_passthrough_allowed: true,
            per_group_partitioning: true,
            iommu_required: false,
            expires_at: None,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use strum::{EnumCount, IntoEnumIterator};

    #[test]
    fn gpu_capability_class_has_five_variants() {
        assert_eq!(GpuCapabilityClass::COUNT, 5);
        assert_eq!(GpuCapabilityClass::iter().count(), 5);
    }

    #[test]
    fn gpu_capability_class_serde_round_trip() {
        for variant in GpuCapabilityClass::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: GpuCapabilityClass = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "round-trip failed for {variant:?}");
        }
    }

    #[test]
    fn gpu_passive_display_is_most_restrictive() {
        assert!(GpuCapabilityClass::GpuPassiveDisplay < GpuCapabilityClass::GpuComputeHeavy);
    }

    #[test]
    fn gpu_policy_default_deny_all_is_restrictive() {
        let p = GpuPolicy::default_deny_all();
        assert_eq!(
            p.gpu_capability_class,
            GpuCapabilityClass::GpuPassiveDisplay
        );
        assert!(!p.vk_device_required);
        assert!(!p.dmabuf_passthrough_allowed);
        assert!(p.iommu_required);
    }

    #[test]
    fn gpu_policy_serde_round_trip() {
        let policy = GpuPolicy {
            gpu_capability_class: GpuCapabilityClass::GpuRich2d,
            vk_device_required: true,
            dmabuf_passthrough_allowed: false,
            per_group_partitioning: true,
            iommu_required: true,
            expires_at: None,
        };
        let json = serde_json::to_string(&policy).unwrap();
        let back: GpuPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy, back);
    }
}
