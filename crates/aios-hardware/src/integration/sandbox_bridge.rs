//! Sandbox bridge: `GpuCapabilityBinding` → `aios_sandbox::GpuPolicy`.
//!
//! Maps a hardware-layer GPU capability binding into a sandbox `GpuPolicy` floor so
//! the sandbox runtime can enforce GPU access constraints without re-deriving policy
//! from the hardware layer.
//!
//! ## Spec deviation
//!
//! The hardware crate's `GpuCapabilityClass` vocabulary (S8.2 §3: `RenderOnly`,
//! `ComputeOnly`, `RenderAndCompute`, `VideoEncode`, `VideoDecode`) differs from the
//! sandbox crate's vocabulary (S3.2 §19.1: `GpuPassiveDisplay`, `GpuBasic2d`,
//! `GpuRich2d`, `GpuFull3d`, `GpuComputeHeavy`).  The bridge maps between them:
//!
//! - `RenderOnly` → `GpuFull3d` (full graphics, no compute)
//! - `ComputeOnly` → `GpuComputeHeavy` (compute pipelines)
//! - `RenderAndCompute` → `GpuFull3d` (full graphics — compute handled separately)
//! - `VideoEncode` / `VideoDecode` → `GpuRich2d` (2D + media)
//!
//! This is a best-effort semantic mapping; a future revision should unify the
//! vocabularies at the spec level.

use aios_sandbox::GpuPolicy;

use crate::gpu::GpuCapabilityClass;
use crate::gpu_resource::GpuCapabilityBinding;

/// Convert a hardware-layer `GpuCapabilityBinding` into an `aios_sandbox::GpuPolicy`.
///
/// The policy floor is derived from the binding's capability class.
/// `per_group_partitioning` is always true (AIOS S4.1 invariant).
/// `iommu_required` is always true (S8.2 IOMMU-DMA protection floor).
///
/// The hardware `GpuCapabilityBinding` carries `gpu_id`, `vram_bytes_reserved`, and
/// per-subject/per-group metadata that `aios_sandbox::GpuPolicy` does not expose.
/// The sandbox policy is a **floor** — the runtime may only tighten these settings,
/// never loosen them.
#[must_use]
pub const fn gpu_binding_to_sandbox_constraint(binding: &GpuCapabilityBinding) -> GpuPolicy {
    let sandbox_class = map_capability_class(binding.capability_class);

    let (vk_device_required, dmabuf_passthrough_allowed) = match binding.capability_class {
        GpuCapabilityClass::RenderOnly
        | GpuCapabilityClass::RenderAndCompute
        | GpuCapabilityClass::VideoEncode
        | GpuCapabilityClass::VideoDecode => (true, true),
        GpuCapabilityClass::ComputeOnly => (true, false),
    };

    GpuPolicy {
        gpu_capability_class: sandbox_class,
        vk_device_required,
        dmabuf_passthrough_allowed,
        per_group_partitioning: true,
        iommu_required: true,
        expires_at: binding.expires_at,
    }
}

/// Map hardware `GpuCapabilityClass` (S8.2 §3) to sandbox `GpuCapabilityClass`
/// (S3.2 §19.1).
const fn map_capability_class(cls: GpuCapabilityClass) -> aios_sandbox::GpuCapabilityClass {
    match cls {
        GpuCapabilityClass::RenderOnly | GpuCapabilityClass::RenderAndCompute => {
            aios_sandbox::GpuCapabilityClass::GpuFull3d
        }
        GpuCapabilityClass::ComputeOnly => aios_sandbox::GpuCapabilityClass::GpuComputeHeavy,
        GpuCapabilityClass::VideoEncode | GpuCapabilityClass::VideoDecode => {
            aios_sandbox::GpuCapabilityClass::GpuRich2d
        }
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
    use crate::ids::GpuId;

    fn make_binding(class: GpuCapabilityClass) -> GpuCapabilityBinding {
        GpuCapabilityBinding {
            binding_id: "gcb_test".into(),
            gpu_id: GpuId("gpu_test".into()),
            group_id: "group:test".into(),
            subject_canonical_id: "human:lucky".into(),
            capability_class: class,
            vram_bytes_reserved: 256 * 1024 * 1024,
            vk_device_partition_id: "vkdp_01".into(),
            bound_at: chrono::Utc::now(),
            expires_at: None,
        }
    }

    #[test]
    fn render_only_binding_maps_to_full_3d_policy() {
        let binding = make_binding(GpuCapabilityClass::RenderOnly);
        let policy = gpu_binding_to_sandbox_constraint(&binding);
        assert_eq!(
            policy.gpu_capability_class,
            aios_sandbox::GpuCapabilityClass::GpuFull3d
        );
        assert!(policy.vk_device_required);
    }

    #[test]
    fn compute_only_binding_maps_to_compute_heavy_policy() {
        let binding = make_binding(GpuCapabilityClass::ComputeOnly);
        let policy = gpu_binding_to_sandbox_constraint(&binding);
        assert_eq!(
            policy.gpu_capability_class,
            aios_sandbox::GpuCapabilityClass::GpuComputeHeavy
        );
        assert!(policy.vk_device_required);
        assert!(!policy.dmabuf_passthrough_allowed);
    }

    #[test]
    fn all_five_capability_classes_map_without_panic() {
        let classes = [
            GpuCapabilityClass::RenderOnly,
            GpuCapabilityClass::ComputeOnly,
            GpuCapabilityClass::RenderAndCompute,
            GpuCapabilityClass::VideoEncode,
            GpuCapabilityClass::VideoDecode,
        ];
        for cls in &classes {
            let binding = make_binding(*cls);
            let policy = gpu_binding_to_sandbox_constraint(&binding);
            assert!(policy.per_group_partitioning);
            assert!(policy.iommu_required);
        }
    }

    #[test]
    fn expires_at_is_preserved() {
        use chrono::{TimeZone, Utc};
        let mut binding = make_binding(GpuCapabilityClass::RenderOnly);
        let expiry = Utc.timestamp_opt(9_999_999_999, 0).unwrap();
        binding.expires_at = Some(expiry);
        let policy = gpu_binding_to_sandbox_constraint(&binding);
        assert_eq!(policy.expires_at, Some(expiry));
    }
}
