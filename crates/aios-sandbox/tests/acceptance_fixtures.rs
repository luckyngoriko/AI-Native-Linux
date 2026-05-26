//! T-114 — S3.2 spec acceptance fixtures (min 8 fixtures).
//!
//! Each fixture maps to a named acceptance scenario from S3.2 and asserts
//! the constitutional contract: profile composition, GPU enforcement,
//! resource limits, syscall allowlisting, and cross-crate surface shape.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_sandbox::{
    ComposeRequest, GpuCapabilityClass, GpuPolicy, GpuPolicyEnforcer, InMemorySandboxComposer,
    IsolationKind, NetworkPosture, ProfileId, ResourceLimits, SandboxComposer, SandboxProfile,
    SubjectRef,
};

// ---------------------------------------------------------------------------
// Fixture 1: S3.2 §5.1 — Default deny when no sources provided
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fixture_1_default_deny_when_no_sources_provided() {
    let composer = InMemorySandboxComposer::new();
    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "any-action".into(),
        base_profile_id: None,
        adapter_default: None,
        app_manifest: None,
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: false,
        is_ai: false,
    };
    let result = composer.compose(req).await.unwrap();
    assert_eq!(result.profile.network_posture, NetworkPosture::DenyAll);
    assert_eq!(result.profile.isolation_kind, IsolationKind::NamespaceLocal);
    assert_eq!(result.profile.resource_limits.cpu_quota_percent, 10);
    assert_eq!(
        result.profile.gpu_policy.gpu_capability_class,
        GpuCapabilityClass::GpuPassiveDisplay
    );
}

// ---------------------------------------------------------------------------
// Fixture 2: S3.2 §5.1 — Most-restrictive-wins across six sources
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fixture_2_most_restrictive_wins_across_six_sources() {
    let composer = InMemorySandboxComposer::new();

    let adapter = SandboxProfile {
        profile_id: ProfileId::new(),
        name: "adapter".into(),
        description: "adapter".into(),
        isolation_kind: IsolationKind::NamespaceLocal,
        resource_limits: ResourceLimits {
            cpu_quota_percent: 80,
            memory_max_bytes: 1024 * 1024 * 1024,
            io_max_bytes_per_sec: None,
            network_max_bytes_per_sec: None,
            process_max_count: None,
            file_descriptor_max: None,
            expires_at: None,
        },
        gpu_policy: GpuPolicy {
            gpu_capability_class: GpuCapabilityClass::GpuComputeHeavy,
            ..GpuPolicy::default_deny_all()
        },
        network_posture: NetworkPosture::Full,
        syscall_allowlist: None,
        signing_authority: "test".into(),
        signature_ed25519: Vec::new(),
    };

    let safety = SandboxProfile {
        profile_id: ProfileId::new(),
        name: "safety".into(),
        description: "safety".into(),
        isolation_kind: IsolationKind::BrowserOriginIsolated,
        resource_limits: ResourceLimits {
            cpu_quota_percent: 5,
            memory_max_bytes: 16 * 1024 * 1024,
            io_max_bytes_per_sec: Some(1024),
            network_max_bytes_per_sec: Some(1024),
            process_max_count: Some(1),
            file_descriptor_max: Some(8),
            expires_at: None,
        },
        gpu_policy: GpuPolicy {
            gpu_capability_class: GpuCapabilityClass::GpuPassiveDisplay,
            ..GpuPolicy::default_deny_all()
        },
        network_posture: NetworkPosture::DenyAll,
        syscall_allowlist: Some(vec!["read".into(), "exit".into()]),
        signing_authority: "test".into(),
        signature_ed25519: Vec::new(),
    };

    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "any".into(),
        base_profile_id: None,
        adapter_default: Some(adapter),
        app_manifest: None,
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: Some(safety),
        recovery_mode: false,
        is_ai: false,
    };
    let result = composer.compose(req).await.unwrap();
    assert_eq!(result.profile.network_posture, NetworkPosture::DenyAll);
    assert_eq!(result.merged_sources.len(), 2);
    assert_eq!(result.profile.resource_limits.cpu_quota_percent, 5);
}

// ---------------------------------------------------------------------------
// Fixture 3: S3.2 §GPU — dmabuf passthrough without IOMMU → violation
// ---------------------------------------------------------------------------

#[test]
fn fixture_3_dmabuf_without_iommu_violation() {
    let enforcer = GpuPolicyEnforcer::new_with_defaults();
    let policy = GpuPolicy {
        dmabuf_passthrough_allowed: true,
        iommu_required: false,
        gpu_capability_class: GpuCapabilityClass::GpuBasic2d,
        vk_device_required: true,
        per_group_partitioning: true,
        expires_at: None,
    };
    let err = enforcer.validate_policy(&policy).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("dmabuf"), "got: {msg}");
}

// ---------------------------------------------------------------------------
// Fixture 4: S3.2 §ResourceLimits — CPU quota validates range
// ---------------------------------------------------------------------------

#[test]
fn fixture_4_cpu_quota_validates_0_100_range() {
    // cpu_quota_percent > 100 is rejected
    let limits = ResourceLimits {
        cpu_quota_percent: 101,
        ..ResourceLimits::default_strict()
    };
    assert!(limits.validate().is_err());

    // memory_max_bytes == 0 is rejected
    let limits = ResourceLimits {
        memory_max_bytes: 0,
        ..ResourceLimits::default_strict()
    };
    assert!(limits.validate().is_err());

    // Valid values pass
    let limits = ResourceLimits {
        cpu_quota_percent: 50,
        ..ResourceLimits::default_strict()
    };
    assert!(limits.validate().is_ok());
}

// ---------------------------------------------------------------------------
// Fixture 5: S3.2 §Isolation — Canonical allowlist per isolation kind
// ---------------------------------------------------------------------------

#[test]
fn fixture_5_canonical_allowlist_per_isolation_kind() {
    use aios_sandbox::SyscallEnforcement;

    let enforcement = SyscallEnforcement::canonical(IsolationKind::ProcessContainer)
        .expect("ProcessContainer has canonical allowlist");
    assert!(enforcement.allowlist.len() >= 50);
    assert!(enforcement.validate("read").is_ok());
    assert!(enforcement.validate("write").is_ok());
    assert!(enforcement.validate("futex").is_ok());
    assert!(enforcement.validate("mount").is_err());
}

// ---------------------------------------------------------------------------
// Fixture 6: S3.2 §19.1 — Base profile from fixture catalog
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fixture_6_base_profile_from_fixture_catalog() {
    let composer = InMemorySandboxComposer::with_fixtures();
    let profiles = composer.list_profiles().await.unwrap();
    assert!(
        profiles.len() >= 3,
        "expected at least 3 fixture profiles, got {}",
        profiles.len()
    );

    let names: Vec<_> = profiles.iter().map(|p| p.name.as_str()).collect();
    assert!(
        names.contains(&"balanced_human"),
        "balanced_human fixture missing"
    );
    assert!(
        names.contains(&"restrictive_ai"),
        "restrictive_ai fixture missing"
    );
    assert!(
        names.contains(&"recovery_only"),
        "recovery_only fixture missing"
    );
}

// ---------------------------------------------------------------------------
// Fixture 7: S3.2 §GPU — GPU capability class PartialOrd ordering
// ---------------------------------------------------------------------------

#[test]
fn fixture_7_gpu_capability_class_partial_ord_ordering() {
    assert!(GpuCapabilityClass::GpuPassiveDisplay < GpuCapabilityClass::GpuBasic2d);
    assert!(GpuCapabilityClass::GpuBasic2d < GpuCapabilityClass::GpuRich2d);
    assert!(GpuCapabilityClass::GpuRich2d < GpuCapabilityClass::GpuFull3d);
    assert!(GpuCapabilityClass::GpuFull3d < GpuCapabilityClass::GpuComputeHeavy);
}

// ---------------------------------------------------------------------------
// Fixture 8: S3.2 §ResourceLimits — None limits are uncapped
// ---------------------------------------------------------------------------

#[test]
fn fixture_8_none_limits_are_uncapped() {
    let limits = ResourceLimits::default_permissive();
    assert_eq!(limits.io_max_bytes_per_sec, None);
    assert_eq!(limits.network_max_bytes_per_sec, None);

    let limits = ResourceLimits::default_strict();
    assert!(limits.io_max_bytes_per_sec.is_some());
    assert!(limits.process_max_count.is_some());
}

// ---------------------------------------------------------------------------
// Fixture 9: S3.2 §19.1 — Store + get + list round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fixture_9_store_get_list_round_trip() {
    let composer = InMemorySandboxComposer::new();
    let profile = SandboxProfile {
        profile_id: ProfileId::new(),
        name: "stored-profile".into(),
        description: "Stored test profile".into(),
        isolation_kind: IsolationKind::ProcessContainer,
        resource_limits: ResourceLimits::default_strict(),
        gpu_policy: GpuPolicy::default_deny_all(),
        network_posture: NetworkPosture::LoopbackOnly,
        syscall_allowlist: None,
        signing_authority: "test".into(),
        signature_ed25519: Vec::new(),
    };
    let id = composer.store_profile(profile).await.unwrap();
    let retrieved = composer.get_profile(&id).await.unwrap();
    assert_eq!(retrieved.name, "stored-profile");
    assert_eq!(composer.list_profiles().await.unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Fixture 10: S3.2 §GPU — VkDevice required above PassiveDisplay
// ---------------------------------------------------------------------------

#[test]
fn fixture_10_vk_device_required_above_passive_display() {
    let enforcer = GpuPolicyEnforcer::new_with_defaults();
    // GpuPassiveDisplay without vk_device_required → OK
    let passive = GpuPolicy {
        gpu_capability_class: GpuCapabilityClass::GpuPassiveDisplay,
        vk_device_required: false,
        dmabuf_passthrough_allowed: false,
        per_group_partitioning: true,
        iommu_required: true,
        expires_at: None,
    };
    assert!(enforcer.validate_policy(&passive).is_ok());

    // GpuFull3d without vk_device_required → violation
    let full3d = GpuPolicy {
        gpu_capability_class: GpuCapabilityClass::GpuFull3d,
        vk_device_required: false,
        dmabuf_passthrough_allowed: false,
        per_group_partitioning: true,
        iommu_required: true,
        expires_at: None,
    };
    assert!(enforcer.validate_policy(&full3d).is_err());
}
