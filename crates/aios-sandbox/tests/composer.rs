//! Integration tests for `SandboxComposer` trait + `InMemorySandboxComposer`
//! (S3.2 §5.1 + §19.1 6-source merge algorithm).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use aios_sandbox::{
    ComposeRequest, GpuCapabilityClass, GpuPolicy, InMemorySandboxComposer, IsolationKind,
    NetworkPosture, ProfileId, ResourceLimits, SandboxComposer, SandboxError, SandboxProfile,
    SubjectRef,
};

fn make_profile(
    name: &str,
    cpu: u32,
    mem_mb: u64,
    network: NetworkPosture,
    gpu: GpuCapabilityClass,
    isolation: IsolationKind,
) -> SandboxProfile {
    SandboxProfile {
        profile_id: ProfileId::new(),
        name: name.into(),
        description: format!("Integration profile: {name}"),
        isolation_kind: isolation,
        resource_limits: ResourceLimits {
            cpu_quota_percent: cpu,
            memory_max_bytes: mem_mb * 1024 * 1024,
            io_max_bytes_per_sec: None,
            network_max_bytes_per_sec: None,
            process_max_count: None,
            file_descriptor_max: None,
            expires_at: None,
        },
        gpu_policy: GpuPolicy {
            gpu_capability_class: gpu,
            ..GpuPolicy::default_deny_all()
        },
        network_posture: network,
        syscall_allowlist: None,
        signing_authority: "test".into(),
        signature_ed25519: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Trait-object dispatch + basic lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn trait_object_compose_empty_request_returns_default() {
    let composer: Arc<dyn SandboxComposer> = Arc::new(InMemorySandboxComposer::new());
    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "test-action".into(),
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
}

#[tokio::test]
async fn trait_object_store_list_get_roundtrip() {
    let composer: Arc<dyn SandboxComposer> = Arc::new(InMemorySandboxComposer::new());
    let profile = make_profile(
        "roundtrip",
        40,
        256,
        NetworkPosture::LoopbackOnly,
        GpuCapabilityClass::GpuBasic2d,
        IsolationKind::ProcessContainer,
    );
    let id = composer.store_profile(profile).await.unwrap();
    let retrieved = composer.get_profile(&id).await.unwrap();
    assert_eq!(retrieved.name, "roundtrip");
    assert_eq!(retrieved.network_posture, NetworkPosture::LoopbackOnly);

    let all = composer.list_profiles().await.unwrap();
    assert_eq!(all.len(), 1);
}

#[tokio::test]
async fn trait_object_get_profile_not_found() {
    let composer: Arc<dyn SandboxComposer> = Arc::new(InMemorySandboxComposer::new());
    let fake_id = ProfileId::new();
    let err = composer.get_profile(&fake_id).await.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("profile not found"), "got: {msg}");
}

#[tokio::test]
async fn trait_object_compose_with_base_profile_not_found() {
    let composer: Arc<dyn SandboxComposer> = Arc::new(InMemorySandboxComposer::new());
    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "test".into(),
        base_profile_id: Some(ProfileId::new()),
        adapter_default: None,
        app_manifest: None,
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: false,
        is_ai: false,
    };
    let err = composer.compose(req).await.unwrap_err();
    assert!(matches!(err, SandboxError::ProfileNotFound(_)));
}

// ---------------------------------------------------------------------------
// 6-source merge order (S3.2 §5.1 + §18.1)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn all_six_sources_merged_in_spec_order() {
    let composer = InMemorySandboxComposer::new();

    let adapter = make_profile(
        "adapter",
        80,
        1024,
        NetworkPosture::Full,
        GpuCapabilityClass::GpuComputeHeavy,
        IsolationKind::NamespaceLocal,
    );
    let app = make_profile(
        "app",
        60,
        512,
        NetworkPosture::HostLimited,
        GpuCapabilityClass::GpuFull3d,
        IsolationKind::ProcessContainer,
    );
    let user = make_profile(
        "user",
        40,
        256,
        NetworkPosture::ExplicitAllowlist,
        GpuCapabilityClass::GpuRich2d,
        IsolationKind::ProcessContainer,
    );
    let policy = make_profile(
        "policy",
        20,
        128,
        NetworkPosture::LoopbackOnly,
        GpuCapabilityClass::GpuBasic2d,
        IsolationKind::VmGuest,
    );
    let group = make_profile(
        "group",
        10,
        64,
        NetworkPosture::DenyAll,
        GpuCapabilityClass::GpuBasic2d,
        IsolationKind::VmGuest,
    );
    let safety = make_profile(
        "safety",
        5,
        32,
        NetworkPosture::DenyAll,
        GpuCapabilityClass::GpuPassiveDisplay,
        IsolationKind::BrowserOriginIsolated,
    );

    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "full-merge".into(),
        base_profile_id: None,
        adapter_default: Some(adapter),
        app_manifest: Some(app),
        user_request: Some(user),
        policy_required: Some(policy),
        group_floor: Some(group),
        runtime_safety_floor: Some(safety),
        recovery_mode: false,
        is_ai: false,
    };

    let result = composer.compose(req).await.unwrap();
    // Safety floor wins: 5% CPU, 32MB mem, DenyAll, PassiveDisplay, BrowserOriginIsolated
    assert_eq!(result.profile.resource_limits.cpu_quota_percent, 5);
    assert_eq!(
        result.profile.resource_limits.memory_max_bytes,
        32 * 1024 * 1024
    );
    assert_eq!(result.profile.network_posture, NetworkPosture::DenyAll);
    assert_eq!(
        result.profile.gpu_policy.gpu_capability_class,
        GpuCapabilityClass::GpuPassiveDisplay
    );
    assert_eq!(
        result.profile.isolation_kind,
        IsolationKind::BrowserOriginIsolated
    );
    assert_eq!(result.merged_sources.len(), 6);
}

#[tokio::test]
async fn group_floor_tightens_beyond_policy() {
    let composer = InMemorySandboxComposer::new();

    let adapter = make_profile(
        "adapter",
        80,
        1024,
        NetworkPosture::HostLimited,
        GpuCapabilityClass::GpuFull3d,
        IsolationKind::NamespaceLocal,
    );
    let group = make_profile(
        "group",
        5,
        64,
        NetworkPosture::DenyAll,
        GpuCapabilityClass::GpuPassiveDisplay,
        IsolationKind::VmGuest,
    );

    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "group-tightens".into(),
        base_profile_id: None,
        adapter_default: Some(adapter),
        app_manifest: None,
        user_request: None,
        policy_required: None,
        group_floor: Some(group),
        runtime_safety_floor: None,
        recovery_mode: false,
        is_ai: false,
    };

    let result = composer.compose(req).await.unwrap();
    assert_eq!(result.profile.resource_limits.cpu_quota_percent, 5);
    assert_eq!(result.profile.network_posture, NetworkPosture::DenyAll);
    assert_eq!(result.profile.isolation_kind, IsolationKind::VmGuest);
}

#[tokio::test]
async fn adapter_default_only_is_starting_point() {
    let composer = InMemorySandboxComposer::new();

    let adapter = make_profile(
        "adapter",
        70,
        512,
        NetworkPosture::ExplicitAllowlist,
        GpuCapabilityClass::GpuRich2d,
        IsolationKind::ProcessContainer,
    );

    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "adapter-only".into(),
        base_profile_id: None,
        adapter_default: Some(adapter.clone()),
        app_manifest: None,
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: false,
        is_ai: false,
    };

    let result = composer.compose(req).await.unwrap();
    assert_eq!(result.profile.network_posture, adapter.network_posture);
    assert_eq!(result.profile.isolation_kind, adapter.isolation_kind);
    assert_eq!(result.profile.resource_limits.cpu_quota_percent, 70);
}

#[tokio::test]
async fn runtime_safety_floor_wins_over_every_other_source() {
    let composer = InMemorySandboxComposer::new();

    let adapter = make_profile(
        "adapter",
        100,
        4096,
        NetworkPosture::Full,
        GpuCapabilityClass::GpuComputeHeavy,
        IsolationKind::NoIsolation,
    );
    let app = make_profile(
        "app",
        90,
        2048,
        NetworkPosture::Full,
        GpuCapabilityClass::GpuComputeHeavy,
        IsolationKind::NoIsolation,
    );
    let safety = make_profile(
        "safety",
        1,
        16,
        NetworkPosture::DenyAll,
        GpuCapabilityClass::GpuPassiveDisplay,
        IsolationKind::BrowserOriginIsolated,
    );

    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "safety-absolute".into(),
        base_profile_id: None,
        adapter_default: Some(adapter),
        app_manifest: Some(app),
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: Some(safety),
        recovery_mode: false,
        is_ai: false,
    };

    let result = composer.compose(req).await.unwrap();
    assert_eq!(result.profile.resource_limits.cpu_quota_percent, 1);
    assert_eq!(result.profile.network_posture, NetworkPosture::DenyAll);
    assert_eq!(
        result.profile.isolation_kind,
        IsolationKind::BrowserOriginIsolated
    );
    assert!(result
        .merged_sources
        .iter()
        .any(|s| s == "runtime_safety_floor"));
}

// ---------------------------------------------------------------------------
// Syscall allowlist intersection across sources
// ---------------------------------------------------------------------------

#[tokio::test]
async fn syscall_allowlist_intersection_across_sources() {
    let composer = InMemorySandboxComposer::new();

    let mut adapter = make_profile(
        "adapter",
        50,
        256,
        NetworkPosture::LoopbackOnly,
        GpuCapabilityClass::GpuBasic2d,
        IsolationKind::ProcessContainer,
    );
    adapter.syscall_allowlist = Some(vec![
        "read".into(),
        "write".into(),
        "open".into(),
        "close".into(),
    ]);

    let mut policy = make_profile(
        "policy",
        50,
        256,
        NetworkPosture::LoopbackOnly,
        GpuCapabilityClass::GpuBasic2d,
        IsolationKind::ProcessContainer,
    );
    policy.syscall_allowlist = Some(vec!["write".into(), "open".into(), "mmap".into()]);

    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "syscall-intersect".into(),
        base_profile_id: None,
        adapter_default: Some(adapter),
        app_manifest: None,
        user_request: None,
        policy_required: Some(policy),
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: false,
        is_ai: false,
    };

    let result = composer.compose(req).await.unwrap();
    // Intersection of {read,write,open,close} ∩ {write,open,mmap} = {open,write}
    assert_eq!(
        result.profile.syscall_allowlist,
        Some(vec!["open".into(), "write".into()])
    );
}

// ---------------------------------------------------------------------------
// Recovery + AI mode simultaneously
// ---------------------------------------------------------------------------

#[tokio::test]
async fn recovery_and_ai_both_enforced() {
    let composer = InMemorySandboxComposer::new();

    let adapter = make_profile(
        "adapter",
        80,
        1024,
        NetworkPosture::Full,
        GpuCapabilityClass::GpuComputeHeavy,
        IsolationKind::NamespaceLocal,
    );

    let req = ComposeRequest {
        subject: SubjectRef::new("ai-agent"),
        action_kind: "recovery-ai".into(),
        base_profile_id: None,
        adapter_default: Some(adapter),
        app_manifest: None,
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: true,
        is_ai: true,
    };

    let result = composer.compose(req).await.unwrap();
    assert!(result.recovery_mode_enforced);
    assert!(result.ai_mode_enforced);
    assert_eq!(result.profile.network_posture, NetworkPosture::LoopbackOnly);
    assert_eq!(
        result.profile.isolation_kind,
        IsolationKind::ProcessContainer
    );
    assert_eq!(
        result.profile.gpu_policy.gpu_capability_class,
        GpuCapabilityClass::GpuBasic2d
    );
}

// ---------------------------------------------------------------------------
// GpuPolicy boolean merge across sources
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gpu_policy_boolean_merge_across_sources() {
    let composer = InMemorySandboxComposer::new();

    let a = SandboxProfile {
        profile_id: ProfileId::new(),
        name: "a".into(),
        description: "a".into(),
        isolation_kind: IsolationKind::NamespaceLocal,
        resource_limits: ResourceLimits::default_strict(),
        gpu_policy: GpuPolicy {
            gpu_capability_class: GpuCapabilityClass::GpuFull3d,
            vk_device_required: false,
            dmabuf_passthrough_allowed: true,
            per_group_partitioning: false,
            iommu_required: false,
            expires_at: None,
        },
        network_posture: NetworkPosture::LoopbackOnly,
        syscall_allowlist: None,
        signing_authority: "test".into(),
        signature_ed25519: Vec::new(),
    };

    let b = SandboxProfile {
        profile_id: ProfileId::new(),
        name: "b".into(),
        description: "b".into(),
        isolation_kind: IsolationKind::NamespaceLocal,
        resource_limits: ResourceLimits::default_strict(),
        gpu_policy: GpuPolicy {
            gpu_capability_class: GpuCapabilityClass::GpuBasic2d,
            vk_device_required: true,
            dmabuf_passthrough_allowed: false,
            per_group_partitioning: true,
            iommu_required: true,
            expires_at: None,
        },
        network_posture: NetworkPosture::LoopbackOnly,
        syscall_allowlist: None,
        signing_authority: "test".into(),
        signature_ed25519: Vec::new(),
    };

    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "gpu-bool-merge".into(),
        base_profile_id: None,
        adapter_default: Some(a),
        app_manifest: Some(b),
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: false,
        is_ai: false,
    };

    let result = composer.compose(req).await.unwrap();
    assert!(result.profile.gpu_policy.vk_device_required);
    assert!(!result.profile.gpu_policy.dmabuf_passthrough_allowed);
    assert!(result.profile.gpu_policy.per_group_partitioning);
    assert!(result.profile.gpu_policy.iommu_required);
}

// ---------------------------------------------------------------------------
// ResourceLimits with None (uncapped) values merging
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resource_limits_none_uncapped_merging() {
    let composer = InMemorySandboxComposer::new();

    let uncapped = SandboxProfile {
        profile_id: ProfileId::new(),
        name: "uncapped".into(),
        description: "uncapped".into(),
        isolation_kind: IsolationKind::NamespaceLocal,
        resource_limits: ResourceLimits {
            cpu_quota_percent: 100,
            memory_max_bytes: 1024 * 1024 * 1024,
            io_max_bytes_per_sec: None,
            network_max_bytes_per_sec: None,
            process_max_count: None,
            file_descriptor_max: None,
            expires_at: None,
        },
        gpu_policy: GpuPolicy::default_deny_all(),
        network_posture: NetworkPosture::LoopbackOnly,
        syscall_allowlist: None,
        signing_authority: "test".into(),
        signature_ed25519: Vec::new(),
    };

    let capped = SandboxProfile {
        profile_id: ProfileId::new(),
        name: "capped".into(),
        description: "capped".into(),
        isolation_kind: IsolationKind::NamespaceLocal,
        resource_limits: ResourceLimits {
            cpu_quota_percent: 50,
            memory_max_bytes: 128 * 1024 * 1024,
            io_max_bytes_per_sec: Some(1024 * 1024),
            network_max_bytes_per_sec: Some(512 * 1024),
            process_max_count: Some(16),
            file_descriptor_max: Some(64),
            expires_at: None,
        },
        gpu_policy: GpuPolicy::default_deny_all(),
        network_posture: NetworkPosture::LoopbackOnly,
        syscall_allowlist: None,
        signing_authority: "test".into(),
        signature_ed25519: Vec::new(),
    };

    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "limits-merge".into(),
        base_profile_id: None,
        adapter_default: Some(uncapped),
        app_manifest: Some(capped),
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: false,
        is_ai: false,
    };

    let result = composer.compose(req).await.unwrap();
    assert_eq!(result.profile.resource_limits.cpu_quota_percent, 50);
    assert_eq!(
        result.profile.resource_limits.memory_max_bytes,
        128 * 1024 * 1024
    );
    assert_eq!(
        result.profile.resource_limits.io_max_bytes_per_sec,
        Some(1024 * 1024)
    );
    assert_eq!(
        result.profile.resource_limits.network_max_bytes_per_sec,
        Some(512 * 1024)
    );
    assert_eq!(result.profile.resource_limits.process_max_count, Some(16));
    assert_eq!(result.profile.resource_limits.file_descriptor_max, Some(64));
}

// ---------------------------------------------------------------------------
// Base profile from fixture catalog
// ---------------------------------------------------------------------------

#[tokio::test]
async fn base_profile_from_fixture_catalog_is_starting_point() {
    let composer = InMemorySandboxComposer::with_fixtures();
    let profiles = composer.list_profiles().await.unwrap();
    let balanced = profiles
        .iter()
        .find(|p| p.name == "balanced_human")
        .unwrap();

    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "base-fixture".into(),
        base_profile_id: Some(balanced.profile_id.clone()),
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
    assert_eq!(result.profile.network_posture, NetworkPosture::HostLimited);
    assert_eq!(
        result.profile.isolation_kind,
        IsolationKind::ProcessContainer
    );
    assert_eq!(result.profile.resource_limits.cpu_quota_percent, 75);
    assert!(result
        .merged_sources
        .iter()
        .any(|s| s.starts_with("base_profile")));
}

// ---------------------------------------------------------------------------
// Result has fresh ProfileId on every compose
// ---------------------------------------------------------------------------

#[tokio::test]
async fn compose_result_always_has_fresh_profile_id() {
    let composer = InMemorySandboxComposer::new();
    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "idempotent".into(),
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

    let r1 = composer.compose(req.clone()).await.unwrap();
    let r2 = composer.compose(req).await.unwrap();
    assert_ne!(r1.profile.profile_id, r2.profile.profile_id);
    // Non-id fields should match since same inputs
    assert_eq!(r1.profile.network_posture, r2.profile.network_posture);
}
