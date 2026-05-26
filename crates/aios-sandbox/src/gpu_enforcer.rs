use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::composer::SubjectRef;
use crate::error::SandboxError;
use crate::gpu::{GpuCapabilityClass, GpuPolicy};

/// IOMMU availability status (S8.2).
///
/// Real IOMMU detection is deferred to M17 (`aios-hardware`).
/// This enum is a stub for the policy enforcement layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IommuStatus {
    /// IOMMU is available and active.
    Available,
    /// IOMMU is not available — GPU DMA isolation is degraded.
    Unavailable,
    /// IOMMU status cannot be determined.
    Unknown,
}

impl IommuStatus {
    /// Returns `true` if IOMMU is confirmed available.
    #[must_use]
    pub const fn is_available(self) -> bool {
        matches!(self, Self::Available)
    }
}

/// A GPU capability binding issued for a specific (group, subject) tuple (S8.2).
///
/// Represents the policy applied to a concrete sandbox instance. The `binding_id`
/// is a `gcb_<ULID>` unique identifier. Real Ed25519-signed bindings land in M17
/// (`aios-hardware`); this is a stub that carries the policy fields without a
/// cryptographic signature.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "S8.2 defines exactly 4 independent boolean fields for GPU binding properties"
)]
pub struct GpuCapabilityBinding {
    /// Unique binding identifier — `gcb_<ULID>`.
    pub binding_id: String,
    /// The GPU capability class granted by this binding.
    pub gpu_capability_class: GpuCapabilityClass,
    /// The group to which this binding applies.
    pub group_id: String,
    /// The subject to which this binding applies.
    pub subject: SubjectRef,
    /// Whether a dedicated `VkDevice` (or platform equivalent) is required.
    pub vk_device_required: bool,
    /// Whether dmabuf handles may be passed across process boundaries.
    pub dmabuf_passthrough_allowed: bool,
    /// Whether IOMMU is required for GPU DMA isolation.
    pub iommu_required: bool,
    /// True when IOMMU is required but unavailable — per S8.2 `IOMMU_UNAVAILABLE_DEGRADED`.
    pub degraded_isolation: bool,
    /// When this binding was issued.
    pub issued_at: DateTime<Utc>,
    /// When this binding expires, if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Enforces GPU policy constraints for sandbox profiles (S3.2 §`GpuPolicy` + S8.2).
///
/// Validates GPU policies, checks capability class bounds, and computes stub
/// capability bindings for (group, subject) tuples. Real Ed25519-signed bindings
/// and `VkDevice` partitioning are deferred to M17 (`aios-hardware`).
#[derive(Debug, Clone)]
pub struct GpuPolicyEnforcer {
    /// Trusted GPU authority identifier.
    pub trusted_gpu_authority: String,
    /// Stub IOMMU status — real detection lands in M17.
    default_iommu_status: IommuStatus,
}

impl GpuPolicyEnforcer {
    /// Create a new `GpuPolicyEnforcer` with the given trusted authority and IOMMU status.
    #[must_use]
    pub fn new(trusted_gpu_authority: impl Into<String>, iommu_status: IommuStatus) -> Self {
        Self {
            trusted_gpu_authority: trusted_gpu_authority.into(),
            default_iommu_status: iommu_status,
        }
    }

    /// Create a `GpuPolicyEnforcer` with sensible defaults.
    ///
    /// Uses `"aios-gpu-root"` as the trusted authority and `IommuStatus::Unknown`
    /// as the default IOMMU status (real detection lands in M17).
    #[must_use]
    pub fn new_with_defaults() -> Self {
        Self {
            trusted_gpu_authority: "aios-gpu-root".into(),
            default_iommu_status: IommuStatus::Unknown,
        }
    }

    /// Returns the current stub IOMMU status.
    #[must_use]
    pub const fn iommu_status(&self) -> IommuStatus {
        self.default_iommu_status
    }

    /// Validate a `GpuPolicy` for internal consistency.
    ///
    /// # Checks
    ///
    /// 1. If `dmabuf_passthrough_allowed` is `true` and `iommu_required` is `false`,
    ///    the policy is rejected — cross-group dmabuf without IOMMU is a data leak vector.
    /// 2. If `gpu_capability_class` is above `GpuPassiveDisplay` and `vk_device_required`
    ///    is `false`, the policy is rejected — 3D/2D shader capability requires a `VkDevice`.
    ///
    /// # Errors
    ///
    /// Returns `SandboxError::GpuPolicyViolation` with a human-readable reason.
    pub fn validate_policy(&self, policy: &GpuPolicy) -> Result<(), SandboxError> {
        if policy.dmabuf_passthrough_allowed && !policy.iommu_required {
            return Err(SandboxError::GpuPolicyViolation(
                "dmabuf_passthrough_allowed=true requires iommu_required=true; cross-group dmabuf without IOMMU is a data leak vector per S8.2".into(),
            ));
        }

        if policy.gpu_capability_class > GpuCapabilityClass::GpuPassiveDisplay
            && !policy.vk_device_required
        {
            return Err(SandboxError::GpuPolicyViolation(format!(
                "gpu_capability_class={:?} requires vk_device_required=true; 3D/2D shader capability requires a `VkDevice` per S8.2",
                policy.gpu_capability_class
            )));
        }

        Ok(())
    }

    /// Check whether a requested `GpuCapabilityClass` is allowed under a given profile.
    ///
    /// Per S3.2: the requested class must be ≤ the profile's `gpu_capability_class`.
    /// Capability can only be tightened, never loosened.
    ///
    /// # Errors
    ///
    /// Returns `SandboxError::GpuPolicyViolation` if the requested class exceeds
    /// the profile's allowed class.
    pub fn check_capability_allowed(
        &self,
        requested: GpuCapabilityClass,
        profile: &GpuPolicy,
    ) -> Result<(), SandboxError> {
        if requested > profile.gpu_capability_class {
            return Err(SandboxError::GpuPolicyViolation(format!(
                "requested gpu_capability_class={requested:?} exceeds profile limit={:?}",
                profile.gpu_capability_class
            )));
        }
        Ok(())
    }

    /// Compute a `GpuCapabilityBinding` for a specific (group, subject) tuple.
    ///
    /// The binding represents the GPU policy applied to a concrete sandbox instance.
    /// Per S8.2 IOMMU rule: when `iommu_required=true` and the environment does not
    /// report IOMMU as available, the binding is issued with `degraded_isolation=true`.
    ///
    /// This is a **stub** — real Ed25519-signed bindings and `VkDevice` partitioning
    /// land in M17 (`aios-hardware`).
    ///
    /// # Errors
    ///
    /// Returns `SandboxError::GpuPolicyViolation` if the profile fails validation.
    pub fn compute_capability_binding(
        &self,
        profile: &GpuPolicy,
        group_id: &str,
        subject: &SubjectRef,
    ) -> Result<GpuCapabilityBinding, SandboxError> {
        self.validate_policy(profile)?;

        let degraded_isolation =
            profile.iommu_required && !self.default_iommu_status.is_available();

        Ok(GpuCapabilityBinding {
            binding_id: format!("gcb_{}", ulid::Ulid::new()),
            gpu_capability_class: profile.gpu_capability_class,
            group_id: group_id.to_string(),
            subject: subject.clone(),
            vk_device_required: profile.vk_device_required,
            dmabuf_passthrough_allowed: profile.dmabuf_passthrough_allowed,
            iommu_required: profile.iommu_required,
            degraded_isolation,
            issued_at: Utc::now(),
            expires_at: profile.expires_at,
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    // --- helpers ---

    fn valid_gpu_policy() -> GpuPolicy {
        GpuPolicy {
            gpu_capability_class: GpuCapabilityClass::GpuRich2d,
            vk_device_required: true,
            dmabuf_passthrough_allowed: false,
            per_group_partitioning: true,
            iommu_required: true,
            expires_at: None,
        }
    }

    fn valid_gpu_policy_passive() -> GpuPolicy {
        GpuPolicy {
            gpu_capability_class: GpuCapabilityClass::GpuPassiveDisplay,
            vk_device_required: false,
            dmabuf_passthrough_allowed: false,
            per_group_partitioning: true,
            iommu_required: true,
            expires_at: None,
        }
    }

    // --- GpuPolicyEnforcer ---

    #[test]
    fn new_with_defaults_succeeds() {
        let enforcer = GpuPolicyEnforcer::new_with_defaults();
        assert_eq!(enforcer.trusted_gpu_authority, "aios-gpu-root");
        assert_eq!(enforcer.iommu_status(), IommuStatus::Unknown);
    }

    #[test]
    fn new_with_custom_authority_and_iommu_available() {
        let enforcer = GpuPolicyEnforcer::new("custom-authority", IommuStatus::Available);
        assert_eq!(enforcer.trusted_gpu_authority, "custom-authority");
        assert!(enforcer.iommu_status().is_available());
    }

    // --- validate_policy ---

    #[test]
    fn validate_policy_on_valid_policy_returns_ok() {
        let enforcer = GpuPolicyEnforcer::new_with_defaults();
        assert!(enforcer.validate_policy(&valid_gpu_policy()).is_ok());
    }

    #[test]
    fn validate_policy_passive_display_no_vk_device_is_valid() {
        let enforcer = GpuPolicyEnforcer::new_with_defaults();
        assert!(enforcer
            .validate_policy(&valid_gpu_policy_passive())
            .is_ok());
    }

    #[test]
    fn validate_policy_dmabuf_passthrough_without_iommu_rejected() {
        let enforcer = GpuPolicyEnforcer::new_with_defaults();
        let policy = GpuPolicy {
            dmabuf_passthrough_allowed: true,
            iommu_required: false,
            ..valid_gpu_policy()
        };
        let err = enforcer.validate_policy(&policy).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("dmabuf"),
            "expected dmabuf mention, got: {msg}"
        );
    }

    #[test]
    fn validate_policy_3d_capability_without_vk_device_rejected() {
        let enforcer = GpuPolicyEnforcer::new_with_defaults();
        let policy = GpuPolicy {
            gpu_capability_class: GpuCapabilityClass::GpuFull3d,
            vk_device_required: false,
            ..valid_gpu_policy()
        };
        let err = enforcer.validate_policy(&policy).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("GpuFull3d"),
            "expected GpuFull3d mention, got: {msg}"
        );
    }

    #[test]
    fn validate_policy_both_rules_pass_together() {
        let enforcer = GpuPolicyEnforcer::new_with_defaults();
        // dmabuf=true + iommu=true is fine; class=GpuBasic2d + vk=true is fine
        let policy = GpuPolicy {
            gpu_capability_class: GpuCapabilityClass::GpuBasic2d,
            vk_device_required: true,
            dmabuf_passthrough_allowed: true,
            iommu_required: true,
            ..valid_gpu_policy()
        };
        assert!(enforcer.validate_policy(&policy).is_ok());
    }

    // --- check_capability_allowed ---

    #[test]
    fn check_capability_allowed_within_profile_returns_ok() {
        let enforcer = GpuPolicyEnforcer::new_with_defaults();
        let profile = valid_gpu_policy(); // GpuRich2d
        assert!(enforcer
            .check_capability_allowed(GpuCapabilityClass::GpuBasic2d, &profile)
            .is_ok());
        assert!(enforcer
            .check_capability_allowed(GpuCapabilityClass::GpuRich2d, &profile)
            .is_ok());
    }

    #[test]
    fn check_capability_allowed_exceeds_profile_returns_violation() {
        let enforcer = GpuPolicyEnforcer::new_with_defaults();
        let profile = valid_gpu_policy(); // GpuRich2d
        let err = enforcer
            .check_capability_allowed(GpuCapabilityClass::GpuComputeHeavy, &profile)
            .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("exceeds"), "got: {msg}");
    }

    #[test]
    fn check_capability_allowed_passive_display_allows_only_passive() {
        let enforcer = GpuPolicyEnforcer::new_with_defaults();
        let profile = valid_gpu_policy_passive(); // GpuPassiveDisplay
        assert!(enforcer
            .check_capability_allowed(GpuCapabilityClass::GpuPassiveDisplay, &profile)
            .is_ok());
        assert!(enforcer
            .check_capability_allowed(GpuCapabilityClass::GpuBasic2d, &profile)
            .is_err());
    }

    // --- compute_capability_binding ---

    #[test]
    fn compute_capability_binding_returns_correct_fields() {
        let enforcer = GpuPolicyEnforcer::new_with_defaults();
        let profile = valid_gpu_policy();
        let subject = SubjectRef::new("agent:dev");
        let group_id = "group-42";

        let binding = enforcer
            .compute_capability_binding(&profile, group_id, &subject)
            .unwrap();

        assert!(binding.binding_id.starts_with("gcb_"));
        assert_eq!(binding.binding_id.len(), 30); // gcb_ + 26-char ULID
        assert_eq!(binding.gpu_capability_class, GpuCapabilityClass::GpuRich2d);
        assert_eq!(binding.group_id, "group-42");
        assert_eq!(binding.subject.0, "agent:dev");
        assert!(binding.vk_device_required);
        assert!(!binding.dmabuf_passthrough_allowed);
        assert!(binding.iommu_required);
        assert!(binding.degraded_isolation); // Unknown != Available
        assert!(binding.expires_at.is_none());
    }

    #[test]
    fn compute_capability_binding_distinct_ids_per_call() {
        let enforcer = GpuPolicyEnforcer::new_with_defaults();
        let profile = valid_gpu_policy();
        let subject = SubjectRef::new("agent:dev");

        let b1 = enforcer
            .compute_capability_binding(&profile, "g1", &subject)
            .unwrap();
        let b2 = enforcer
            .compute_capability_binding(&profile, "g2", &subject)
            .unwrap();

        assert_ne!(b1.binding_id, b2.binding_id);
    }

    // --- IommuStatus ---

    #[test]
    fn iommu_status_three_variant_round_trip() {
        for variant in &[
            IommuStatus::Available,
            IommuStatus::Unavailable,
            IommuStatus::Unknown,
        ] {
            let json = serde_json::to_string(variant).unwrap();
            let back: IommuStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*variant, back, "round-trip failed for {variant:?}");
        }
    }

    #[test]
    fn iommu_status_is_available_logic() {
        assert!(IommuStatus::Available.is_available());
        assert!(!IommuStatus::Unavailable.is_available());
        assert!(!IommuStatus::Unknown.is_available());
    }

    // --- degraded_isolation ---

    #[test]
    fn degraded_isolation_true_when_iommu_unavailable() {
        let enforcer = GpuPolicyEnforcer::new("auth", IommuStatus::Unavailable);
        let profile = valid_gpu_policy(); // iommu_required=true
        let subject = SubjectRef::new("agent:dev");

        let binding = enforcer
            .compute_capability_binding(&profile, "g", &subject)
            .unwrap();
        assert!(binding.degraded_isolation);
    }

    #[test]
    fn degraded_isolation_true_when_iommu_unknown() {
        let enforcer = GpuPolicyEnforcer::new("auth", IommuStatus::Unknown);
        let profile = valid_gpu_policy(); // iommu_required=true
        let subject = SubjectRef::new("agent:dev");

        let binding = enforcer
            .compute_capability_binding(&profile, "g", &subject)
            .unwrap();
        assert!(binding.degraded_isolation);
    }

    #[test]
    fn degraded_isolation_false_when_iommu_available() {
        let enforcer = GpuPolicyEnforcer::new("auth", IommuStatus::Available);
        let profile = valid_gpu_policy(); // iommu_required=true
        let subject = SubjectRef::new("agent:dev");

        let binding = enforcer
            .compute_capability_binding(&profile, "g", &subject)
            .unwrap();
        assert!(!binding.degraded_isolation);
    }

    #[test]
    fn degraded_isolation_false_when_iommu_not_required() {
        let enforcer = GpuPolicyEnforcer::new("auth", IommuStatus::Unavailable);
        let profile = GpuPolicy {
            iommu_required: false,
            ..valid_gpu_policy_passive()
        };
        let subject = SubjectRef::new("agent:dev");

        let binding = enforcer
            .compute_capability_binding(&profile, "g", &subject)
            .unwrap();
        assert!(!binding.degraded_isolation);
    }

    // --- GpuCapabilityBinding serde ---

    #[test]
    fn gpu_capability_binding_serde_round_trip() {
        let binding = GpuCapabilityBinding {
            binding_id: "gcb_01JQZYX80W3YQH7K4N5R8T9F2X".into(),
            gpu_capability_class: GpuCapabilityClass::GpuFull3d,
            group_id: "group-alpha".into(),
            subject: SubjectRef::new("app:browser"),
            vk_device_required: true,
            dmabuf_passthrough_allowed: false,
            iommu_required: true,
            degraded_isolation: false,
            issued_at: Utc::now(),
            expires_at: None,
        };
        let json = serde_json::to_string(&binding).unwrap();
        let back: GpuCapabilityBinding = serde_json::from_str(&json).unwrap();
        assert_eq!(binding, back);
    }

    // --- GpuCapabilityClass ↔ `VkDevice` pairing ---

    #[test]
    fn gpu_capability_class_each_variant_pairs_correctly_with_vk_device() {
        let enforcer = GpuPolicyEnforcer::new_with_defaults();
        for variant in GpuCapabilityClass::iter() {
            let policy = GpuPolicy {
                gpu_capability_class: variant,
                vk_device_required: variant > GpuCapabilityClass::GpuPassiveDisplay,
                dmabuf_passthrough_allowed: false,
                per_group_partitioning: true,
                iommu_required: true,
                expires_at: None,
            };
            assert!(
                enforcer.validate_policy(&policy).is_ok(),
                "variant {variant:?} with correct vk_device_required should validate"
            );
        }
    }

    // --- concurrency ---

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_compute_capability_binding_from_3_tasks_no_race() {
        use std::sync::Arc;
        let enforcer = Arc::new(GpuPolicyEnforcer::new_with_defaults());
        let profile = valid_gpu_policy();
        let subject = SubjectRef::new("agent:dev");

        let e1 = Arc::clone(&enforcer);
        let p1 = profile.clone();
        let s1 = subject.clone();
        let t1 =
            tokio::task::spawn(
                async move { e1.compute_capability_binding(&p1, "g1", &s1).unwrap() },
            );

        let e2 = Arc::clone(&enforcer);
        let p2 = profile.clone();
        let s2 = subject.clone();
        let t2 =
            tokio::task::spawn(
                async move { e2.compute_capability_binding(&p2, "g2", &s2).unwrap() },
            );

        let e3 = Arc::clone(&enforcer);
        let p3 = profile.clone();
        let s3 = subject.clone();
        let t3 =
            tokio::task::spawn(
                async move { e3.compute_capability_binding(&p3, "g3", &s3).unwrap() },
            );

        let (r1, r2, r3) = tokio::join!(t1, t2, t3);
        let b1 = r1.unwrap();
        let b2 = r2.unwrap();
        let b3 = r3.unwrap();

        assert_ne!(b1.binding_id, b2.binding_id);
        assert_ne!(b1.binding_id, b3.binding_id);
        assert_ne!(b2.binding_id, b3.binding_id);
        assert_eq!(b1.group_id, "g1");
        assert_eq!(b2.group_id, "g2");
        assert_eq!(b3.group_id, "g3");
    }

    // --- INV-009 cross-group surface pixel-readback isolation ---

    /// INV-009 (S7.1 §I4 referenced from S8.2): cross-group surface pixel-readback
    /// isolation. This test documents the invariant and asserts the STUB status —
    /// real enforcement of per-group `VkDevice` partitioning and surface-readback
    /// isolation is deferred to M17 (`aios-hardware`). The current binding carries
    /// the `per_group_partitioning` flag from the profile but does not enforce
    /// GPU-side memory isolation between groups.
    #[test]
    fn inv_009_cross_group_surface_pixel_readback_isolation_stub() {
        let enforcer = GpuPolicyEnforcer::new_with_defaults();
        let profile = GpuPolicy {
            per_group_partitioning: true,
            ..valid_gpu_policy()
        };
        let subject_a = SubjectRef::new("group-a:app");
        let subject_b = SubjectRef::new("group-b:app");

        let binding_a = enforcer
            .compute_capability_binding(&profile, "group-a", &subject_a)
            .unwrap();
        let binding_b = enforcer
            .compute_capability_binding(&profile, "group-b", &subject_b)
            .unwrap();

        // Both bindings share the same capability class — cross-group
        // surface isolation is NOT enforced at this layer (M17 work).
        assert_eq!(
            binding_a.gpu_capability_class,
            binding_b.gpu_capability_class
        );
        assert_ne!(binding_a.group_id, binding_b.group_id);

        // STUB: degraded_isolation flag warns but does not block.
        // Real enforcement of per-group `VkDevice` partitioning and
        // S7.1 §I4 pixel-readback isolation lands in M17.
    }
}
