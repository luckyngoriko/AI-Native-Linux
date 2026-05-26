//! Integration tests for `GpuPolicyEnforcer` — GPU policy enforcement
//! (S3.2 §`GpuPolicy` + S8.2).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_sandbox::{
    GpuCapabilityBinding, GpuCapabilityClass, GpuPolicy, GpuPolicyEnforcer, IommuStatus, SubjectRef,
};
use std::sync::Arc;

const fn valid_gpu_policy() -> GpuPolicy {
    GpuPolicy {
        gpu_capability_class: GpuCapabilityClass::GpuRich2d,
        vk_device_required: true,
        dmabuf_passthrough_allowed: false,
        per_group_partitioning: true,
        iommu_required: true,
        expires_at: None,
    }
}

const fn valid_passive_policy() -> GpuPolicy {
    GpuPolicy {
        gpu_capability_class: GpuCapabilityClass::GpuPassiveDisplay,
        vk_device_required: false,
        dmabuf_passthrough_allowed: false,
        per_group_partitioning: true,
        iommu_required: true,
        expires_at: None,
    }
}

// === 1. new_with_defaults succeeds ===

#[test]
fn gpu_policy_enforcer_new_with_defaults_succeeds() {
    let enforcer = GpuPolicyEnforcer::new_with_defaults();
    assert_eq!(enforcer.trusted_gpu_authority, "aios-gpu-root");
    assert_eq!(enforcer.iommu_status(), IommuStatus::Unknown);
}

// === 2. validate_policy on valid policy → Ok ===

#[test]
fn validate_policy_on_valid_policy_returns_ok() {
    let enforcer = GpuPolicyEnforcer::new_with_defaults();
    assert!(enforcer.validate_policy(&valid_gpu_policy()).is_ok());
}

// === 3. dmabuf_passthrough_allowed=true + iommu_required=false → GpuPolicyViolation ===

#[test]
fn validate_policy_dmabuf_without_iommu_rejected() {
    let enforcer = GpuPolicyEnforcer::new_with_defaults();
    let policy = GpuPolicy {
        dmabuf_passthrough_allowed: true,
        iommu_required: false,
        ..valid_gpu_policy()
    };
    let err = enforcer.validate_policy(&policy).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("dmabuf_passthrough_allowed"),
        "expected dmabuf mention in error, got: {msg}"
    );
    assert!(
        msg.contains("IOMMU"),
        "expected IOMMU mention in error, got: {msg}"
    );
}

// === 4. gpu_capability_class > GpuPassiveDisplay + vk_device_required=false → GpuPolicyViolation ===

#[test]
fn validate_policy_gpu_class_above_passive_without_vk_device_rejected() {
    let enforcer = GpuPolicyEnforcer::new_with_defaults();

    for variant in &[
        GpuCapabilityClass::GpuBasic2d,
        GpuCapabilityClass::GpuRich2d,
        GpuCapabilityClass::GpuFull3d,
        GpuCapabilityClass::GpuComputeHeavy,
    ] {
        let policy = GpuPolicy {
            gpu_capability_class: *variant,
            vk_device_required: false,
            ..valid_gpu_policy()
        };
        let err = enforcer.validate_policy(&policy).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("vk_device_required"),
            "variant {variant:?}: expected vk_device_required mention, got: {msg}"
        );
    }
}

// === 5. check_capability_allowed with requested ≤ profile.gpu_capability_class → Ok ===

#[test]
fn check_capability_allowed_within_profile_returns_ok() {
    let enforcer = GpuPolicyEnforcer::new_with_defaults();
    let profile = valid_gpu_policy(); // GpuRich2d

    assert!(enforcer
        .check_capability_allowed(GpuCapabilityClass::GpuPassiveDisplay, &profile)
        .is_ok());
    assert!(enforcer
        .check_capability_allowed(GpuCapabilityClass::GpuBasic2d, &profile)
        .is_ok());
    assert!(enforcer
        .check_capability_allowed(GpuCapabilityClass::GpuRich2d, &profile)
        .is_ok());
}

// === 6. check_capability_allowed with requested > profile.gpu_capability_class → GpuPolicyViolation ===

#[test]
fn check_capability_allowed_exceeds_profile_returns_violation() {
    let enforcer = GpuPolicyEnforcer::new_with_defaults();
    let profile = valid_gpu_policy(); // GpuRich2d

    let err = enforcer
        .check_capability_allowed(GpuCapabilityClass::GpuFull3d, &profile)
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("exceeds"), "got: {msg}");

    let err = enforcer
        .check_capability_allowed(GpuCapabilityClass::GpuComputeHeavy, &profile)
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("exceeds"), "got: {msg}");
}

// === 7. compute_capability_binding returns GpuCapabilityBinding with correct fields ===

#[test]
fn compute_capability_binding_returns_correct_fields() {
    let enforcer = GpuPolicyEnforcer::new_with_defaults();
    let profile = valid_gpu_policy();
    let subject = SubjectRef::new("agent:dev");

    let binding = enforcer
        .compute_capability_binding(&profile, "group-42", &subject)
        .unwrap();

    assert!(binding.binding_id.starts_with("gcb_"));
    assert_eq!(binding.binding_id.len(), 30);
    assert_eq!(binding.gpu_capability_class, GpuCapabilityClass::GpuRich2d);
    assert_eq!(binding.group_id, "group-42");
    assert_eq!(binding.subject, subject);
    assert!(binding.vk_device_required);
    assert!(!binding.dmabuf_passthrough_allowed);
    assert!(binding.iommu_required);
    assert!(binding.degraded_isolation); // IommuStatus::Unknown != Available
    assert!(binding.expires_at.is_none());
}

// === 8. GpuCapabilityBinding round-trips through serde_json ===

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
        issued_at: chrono::Utc::now(),
        expires_at: None,
    };
    let json = serde_json::to_string_pretty(&binding).unwrap();
    let back: GpuCapabilityBinding = serde_json::from_str(&json).unwrap();
    assert_eq!(binding, back);

    // Verify JSON contains expected field names.
    assert!(json.contains("\"binding_id\""));
    assert!(json.contains("\"gpu_capability_class\""));
    assert!(json.contains("\"degraded_isolation\""));
    assert!(json.contains("\"group_id\""));
    assert!(json.contains("\"subject\""));
}

// === 9. IommuStatus 3-variant round-trips ===

#[test]
fn iommu_status_three_variant_round_trip() {
    let variants = [
        (IommuStatus::Available, "AVAILABLE"),
        (IommuStatus::Unavailable, "UNAVAILABLE"),
        (IommuStatus::Unknown, "UNKNOWN"),
    ];

    for (variant, wire_name) in &variants {
        let json = serde_json::to_string(variant).unwrap();
        assert!(
            json.contains(wire_name),
            "expected wire name {wire_name} for {variant:?}, got: {json}"
        );
        let back: IommuStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(*variant, back, "round-trip failed for {variant:?}");
    }

    // Invalid wire value should fail.
    let err = serde_json::from_str::<IommuStatus>("\"BOGUS\"").unwrap_err();
    assert!(
        format!("{err}").contains("BOGUS"),
        "expected BOGUS in error"
    );
}

// === 10. degraded_isolation=true when IOMMU unavailable ===

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

// === 11. degraded_isolation=false when IOMMU available ===

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

// === 12. GpuCapabilityClass each variant pairs correctly with VkDevice requirement ===

#[test]
fn gpu_capability_class_each_variant_pairs_correctly_with_vk_device_requirement() {
    use strum::IntoEnumIterator;

    let enforcer = GpuPolicyEnforcer::new_with_defaults();

    for variant in GpuCapabilityClass::iter() {
        let needs_vk_device = variant > GpuCapabilityClass::GpuPassiveDisplay;
        let policy = GpuPolicy {
            gpu_capability_class: variant,
            vk_device_required: needs_vk_device,
            dmabuf_passthrough_allowed: false,
            per_group_partitioning: true,
            iommu_required: true,
            expires_at: None,
        };
        assert!(
            enforcer.validate_policy(&policy).is_ok(),
            "variant {variant:?} with vk_device_required={needs_vk_device} should validate"
        );

        // The opposite pairing should fail for non-passive variants.
        if needs_vk_device {
            let bad_policy = GpuPolicy {
                vk_device_required: false,
                ..policy.clone()
            };
            assert!(
                enforcer.validate_policy(&bad_policy).is_err(),
                "variant {variant:?} without vk_device_required should be rejected"
            );
        }
    }
}

// === 13. Concurrent compute_capability_binding from 3 tasks → no race ===

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_compute_capability_binding_from_3_tasks_no_race() {
    let enforcer = Arc::new(GpuPolicyEnforcer::new_with_defaults());
    let profile = valid_gpu_policy();
    let subject = SubjectRef::new("agent:dev");

    let e1 = Arc::clone(&enforcer);
    let p1 = profile.clone();
    let s1 = subject.clone();
    let t1 =
        tokio::task::spawn(async move { e1.compute_capability_binding(&p1, "g1", &s1).unwrap() });

    let e2 = Arc::clone(&enforcer);
    let p2 = profile.clone();
    let s2 = subject.clone();
    let t2 =
        tokio::task::spawn(async move { e2.compute_capability_binding(&p2, "g2", &s2).unwrap() });

    let e3 = Arc::clone(&enforcer);
    let p3 = profile.clone();
    let s3 = subject.clone();
    let t3 =
        tokio::task::spawn(async move { e3.compute_capability_binding(&p3, "g3", &s3).unwrap() });

    let (r1, r2, r3) = tokio::join!(t1, t2, t3);
    let b1 = r1.unwrap();
    let b2 = r2.unwrap();
    let b3 = r3.unwrap();

    // All three bindings have distinct IDs.
    assert_ne!(b1.binding_id, b2.binding_id);
    assert_ne!(b1.binding_id, b3.binding_id);
    assert_ne!(b2.binding_id, b3.binding_id);

    // Each binding is tagged with the correct group.
    assert_eq!(b1.group_id, "g1");
    assert_eq!(b2.group_id, "g2");
    assert_eq!(b3.group_id, "g3");
}

// === 14. INV-009 cross-group surface pixel-readback isolation stub ===

/// INV-009 (S7.1 §I4 referenced from S8.2): cross-group surface pixel-readback
/// isolation. This is a STUB test — real per-group `VkDevice` partitioning and
/// surface-readback isolation is deferred to M17 (`aios-hardware`). The current
/// binding carries `per_group_partitioning` from the profile but does not
/// enforce GPU-side memory isolation between groups.
#[test]
fn inv_009_cross_group_surface_pixel_readback_isolation_stub() {
    // STUB: M17 will wire real cross-group GPU memory isolation.
    // For now, verify that bindings for distinct groups carry the same
    // capability class (no per-group GPU partitioning enforcement).

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

    // Both share the same capability class — cross-group surface isolation
    // is NOT enforced here (that's M17 work).
    assert_eq!(
        binding_a.gpu_capability_class,
        binding_b.gpu_capability_class
    );
    assert_ne!(binding_a.group_id, binding_b.group_id);

    // degraded_isolation flags the IOMMU gap but doesn't block cross-group access.
    // Real S7.1 §I4 pixel-readback isolation lands in M17.
}

// === 15. validate_policy rejects both rules simultaneously ===

#[test]
fn validate_policy_rejects_both_rules_simultaneously() {
    let enforcer = GpuPolicyEnforcer::new_with_defaults();
    let policy = GpuPolicy {
        gpu_capability_class: GpuCapabilityClass::GpuFull3d,
        vk_device_required: false,
        dmabuf_passthrough_allowed: true,
        iommu_required: false,
        ..valid_gpu_policy()
    };
    // First rule (dmabuf without IOMMU) triggers first.
    let err = enforcer.validate_policy(&policy).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("dmabuf") || msg.contains("vk_device_required"),
        "expected one of the violation messages, got: {msg}"
    );
}

// === 16. compute_capability_binding rejects invalid profile ===

#[test]
fn compute_capability_binding_rejects_invalid_profile() {
    let enforcer = GpuPolicyEnforcer::new_with_defaults();
    let invalid_policy = GpuPolicy {
        dmabuf_passthrough_allowed: true,
        iommu_required: false,
        ..valid_gpu_policy()
    };
    let subject = SubjectRef::new("agent:dev");

    let err = enforcer
        .compute_capability_binding(&invalid_policy, "g", &subject)
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("dmabuf"), "got: {msg}");
}

// === 17. passive display with vk_device_required=false is valid ===

#[test]
fn passive_display_without_vk_device_is_valid() {
    let enforcer = GpuPolicyEnforcer::new_with_defaults();
    assert!(enforcer.validate_policy(&valid_passive_policy()).is_ok());

    let subject = SubjectRef::new("agent:dev");
    let binding = enforcer
        .compute_capability_binding(&valid_passive_policy(), "g", &subject)
        .unwrap();
    assert_eq!(
        binding.gpu_capability_class,
        GpuCapabilityClass::GpuPassiveDisplay
    );
    assert!(!binding.vk_device_required);
}

// === 18. compute_capability_binding preserves expires_at from profile ===

#[test]
fn compute_capability_binding_preserves_expires_at_from_profile() {
    use chrono::{TimeZone, Utc};
    let enforcer = GpuPolicyEnforcer::new_with_defaults();
    let expiry = Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap();
    let policy = GpuPolicy {
        expires_at: Some(expiry),
        ..valid_gpu_policy()
    };
    let subject = SubjectRef::new("agent:dev");

    let binding = enforcer
        .compute_capability_binding(&policy, "g", &subject)
        .unwrap();
    assert_eq!(binding.expires_at, Some(expiry));
}
