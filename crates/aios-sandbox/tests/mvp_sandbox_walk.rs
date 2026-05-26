//! T-114 — §22 sandbox walk scenarios composed with M3-M11 stack.
//!
//! Six end-to-end scenarios exercising sandbox composition, GPU policy
//! enforcement, resource limit enforcement, and syscall validation through
//! the full M3-M11 cross-crate surface.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use aios_capability_runtime::RuntimeSandboxComposer;
use aios_sandbox::{
    GpuCapabilityClass, GpuPolicy, GpuPolicyEnforcer, InMemorySandboxComposer, IsolationKind,
    NetworkPosture, ResourceLimitEnforcer, ResourceLimits, ResourceRequest, SandboxComposer,
    SandboxRuntimeAdapter, SubjectRef,
};

fn permissive_profile(name: &str) -> aios_sandbox::SandboxProfile {
    aios_sandbox::SandboxProfile {
        profile_id: aios_sandbox::ProfileId::new(),
        name: name.into(),
        description: format!("Permissive profile: {name}"),
        isolation_kind: IsolationKind::NamespaceLocal,
        resource_limits: ResourceLimits {
            cpu_quota_percent: 100,
            memory_max_bytes: 2 * 1024 * 1024 * 1024,
            io_max_bytes_per_sec: None,
            network_max_bytes_per_sec: None,
            process_max_count: Some(64),
            file_descriptor_max: Some(256),
            expires_at: None,
        },
        gpu_policy: GpuPolicy {
            gpu_capability_class: GpuCapabilityClass::GpuComputeHeavy,
            vk_device_required: true,
            dmabuf_passthrough_allowed: false,
            per_group_partitioning: true,
            iommu_required: true,
            expires_at: None,
        },
        network_posture: NetworkPosture::Full,
        syscall_allowlist: None,
        signing_authority: "test-authority".into(),
        signature_ed25519: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Scenario 1: AI submits action → restrictive sandbox composed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_1_ai_submit_action_restrictive_sandbox() {
    let composer = InMemorySandboxComposer::new();
    let adapter = permissive_profile("adapter");

    let req = aios_sandbox::ComposeRequest {
        subject: SubjectRef::new("ai-agent-7"),
        action_kind: "file.read".into(),
        base_profile_id: None,
        adapter_default: Some(adapter),
        app_manifest: None,
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: false,
        is_ai: true,
    };
    let result = composer.compose(req).await.unwrap();
    assert!(result.ai_mode_enforced);
    // AI mode forces network ≤ LoopbackOnly
    assert_eq!(result.profile.network_posture, NetworkPosture::LoopbackOnly);
    // AI mode forces GPU ≤ GpuBasic2d
    assert_eq!(
        result.profile.gpu_policy.gpu_capability_class,
        GpuCapabilityClass::GpuBasic2d
    );
}

// ---------------------------------------------------------------------------
// Scenario 2: Human submits action → permissive sandbox
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_2_human_submit_action_permissive_sandbox() {
    let composer = InMemorySandboxComposer::new();
    let adapter = permissive_profile("adapter");

    let req = aios_sandbox::ComposeRequest {
        subject: SubjectRef::new("human:lucky"),
        action_kind: "package.install".into(),
        base_profile_id: None,
        adapter_default: Some(adapter),
        app_manifest: None,
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: false,
        is_ai: false,
    };
    let result = composer.compose(req).await.unwrap();
    assert!(!result.ai_mode_enforced);
    // Human gets the adapter's full profile without AI downgrade
    assert_eq!(result.profile.network_posture, NetworkPosture::Full);
    assert_eq!(
        result.profile.gpu_policy.gpu_capability_class,
        GpuCapabilityClass::GpuComputeHeavy
    );
}

// ---------------------------------------------------------------------------
// Scenario 3: Recovery mode forces restrictive sandbox
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_3_recovery_mode_forces_restrictive_sandbox() {
    let composer = InMemorySandboxComposer::new();
    let adapter = permissive_profile("adapter");

    let req = aios_sandbox::ComposeRequest {
        subject: SubjectRef::new("operator-root"),
        action_kind: "recovery.restore".into(),
        base_profile_id: None,
        adapter_default: Some(adapter),
        app_manifest: None,
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: true,
        is_ai: false,
    };
    let result = composer.compose(req).await.unwrap();
    assert!(result.recovery_mode_enforced);
    // Recovery forces network ≥ LoopbackOnly
    assert!(
        result.profile.network_posture >= NetworkPosture::LoopbackOnly,
        "network should be at least LoopbackOnly, got {:?}",
        result.profile.network_posture
    );
    // Recovery forces isolation ≥ ProcessContainer
    // IsolationKind has no PartialOrd — verify it's not the weakest variant
    assert_ne!(
        result.profile.isolation_kind,
        IsolationKind::NoIsolation,
        "recovery mode must not allow NoIsolation"
    );
    assert_ne!(
        result.profile.isolation_kind,
        IsolationKind::NamespaceLocal,
        "recovery mode must not allow NamespaceLocal"
    );
}

// ---------------------------------------------------------------------------
// Scenario 4: GPU policy violation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_4_gpu_policy_violation() {
    let enforcer = GpuPolicyEnforcer::new_with_defaults();
    let restrictive_profile = GpuPolicy {
        gpu_capability_class: GpuCapabilityClass::GpuPassiveDisplay,
        vk_device_required: false,
        dmabuf_passthrough_allowed: false,
        per_group_partitioning: true,
        iommu_required: true,
        expires_at: None,
    };

    // Request GpuFull3D — exceeds GpuPassiveDisplay in the profile
    let err = enforcer
        .check_capability_allowed(GpuCapabilityClass::GpuFull3d, &restrictive_profile)
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("exceeds"), "got: {msg}");
}

// ---------------------------------------------------------------------------
// Scenario 5: Resource limit exceeded
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_5_resource_limit_exceeded() {
    let enforcer = ResourceLimitEnforcer::new_with_defaults();
    let limits = ResourceLimits::default_strict();

    let request = ResourceRequest {
        cpu_pct: 5,
        memory_bytes: 512 * 1024 * 1024, // 512 MiB exceeds strict default (64 MiB)
        io_bytes_per_sec: 0,
        network_bytes_per_sec: 0,
        process_count: 0,
        fd_count: 0,
    };
    let err = enforcer.check_usage(&request, &limits).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("memory_max_bytes"), "got: {msg}");
}

// ---------------------------------------------------------------------------
// Scenario 6: Syscall not in allowlist
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_6_syscall_not_in_allowlist() {
    let enforcer = ResourceLimitEnforcer::new_with_defaults();
    let allowlist: Vec<String> = vec!["read".into(), "write".into(), "exit".into()];

    // "mount" is not in the allowlist
    let err = enforcer
        .validate_syscall("mount", Some(&allowlist))
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("mount"), "got: {msg}");
}

// ---------------------------------------------------------------------------
// Cross-crate: SandboxRuntimeAdapter with full composer stack
// ---------------------------------------------------------------------------

#[tokio::test]
async fn runtime_adapter_sandbox_walk_with_full_fixture_composer() {
    let composer = Arc::new(InMemorySandboxComposer::with_fixtures());
    let adapter = SandboxRuntimeAdapter::new(composer);

    let summary = adapter
        .compose_for_action("package.install", "human-operator", false, false)
        .await
        .expect("runtime adapter compose with fixtures");

    assert!(!summary.profile_id.is_empty());
    assert!(!summary.isolation_kind.is_empty());
    assert!(!summary.network_posture.is_empty());
    assert!(!summary.gpu_capability_class.is_empty());
}
