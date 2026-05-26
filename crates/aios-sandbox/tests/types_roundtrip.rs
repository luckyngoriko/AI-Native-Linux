//! T-106 round-trip + invariant tests for the aios-sandbox skeleton.
//!
//! These tests anchor the constitutional shape of the core types so subsequent
//! tasks cannot silently drift the surface:
//!
//! - `IsolationKind` has exactly 5 variants (S3.2).
//! - `GpuCapabilityClass` has exactly 5 variants (S8.2 §3).
//! - `NetworkPosture` has exactly 5 variants (S3.2).
//! - `SandboxProfile` round-trips through `serde_json`.
//! - `ResourceLimits` round-trips + validates.
//! - `GpuPolicy` round-trips.
//! - `SandboxError` Display strings are non-empty for every variant.
//! - Cross-crate: `aios_action::ActionId` import compiles.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use strum::{EnumCount, IntoEnumIterator};

use aios_sandbox::{
    GpuCapabilityClass, GpuPolicy, IsolationKind, NetworkPosture, ProfileId, ResourceLimits,
    SandboxError, SandboxProfile,
};

// ---------------------------------------------------------------------------
// Enum variant counts
// ---------------------------------------------------------------------------

#[test]
fn isolation_kind_count_matches_spec() {
    assert_eq!(
        IsolationKind::COUNT,
        5,
        "S3.2: IsolationKind must have exactly 5 variants"
    );
    assert_eq!(IsolationKind::iter().count(), 5);
}

#[test]
fn gpu_capability_class_count_matches_spec() {
    assert_eq!(
        GpuCapabilityClass::COUNT,
        5,
        "S8.2 §3: GpuCapabilityClass must have exactly 5 variants"
    );
    assert_eq!(GpuCapabilityClass::iter().count(), 5);
}

#[test]
fn network_posture_count_matches_spec() {
    assert_eq!(
        NetworkPosture::COUNT,
        5,
        "S3.2: NetworkPosture must have exactly 5 variants"
    );
    assert_eq!(NetworkPosture::iter().count(), 5);
}

// ---------------------------------------------------------------------------
// serde round-trips
// ---------------------------------------------------------------------------

#[test]
fn sandbox_profile_round_trips_through_serde_json() {
    let profile = SandboxProfile {
        profile_id: ProfileId::new(),
        name: "test-profile".into(),
        description: "A test sandbox profile for round-trip verification".into(),
        isolation_kind: IsolationKind::ProcessContainer,
        resource_limits: ResourceLimits::default_strict(),
        gpu_policy: GpuPolicy::default_deny_all(),
        network_posture: NetworkPosture::LoopbackOnly,
        syscall_allowlist: Some(vec!["test-seccomp".into()]),
        signing_authority: "aios-root".into(),
        signature_ed25519: vec![0xCD; 64],
    };

    let json = serde_json::to_string(&profile).expect("serialise SandboxProfile");
    let back: SandboxProfile = serde_json::from_str(&json).expect("deserialise SandboxProfile");
    assert_eq!(profile, back, "SandboxProfile round-trip mismatch");
}

#[test]
fn resource_limits_round_trips_through_serde_json() {
    let limits = ResourceLimits {
        cpu_quota_percent: 75,
        memory_max_bytes: 1024 * 1024 * 1024,
        io_max_bytes_per_sec: Some(50 * 1024 * 1024),
        network_max_bytes_per_sec: None,
        process_max_count: Some(32),
        file_descriptor_max: None,
        expires_at: None,
    };

    let json = serde_json::to_string(&limits).expect("serialise ResourceLimits");
    let back: ResourceLimits = serde_json::from_str(&json).expect("deserialise ResourceLimits");
    assert_eq!(limits, back, "ResourceLimits round-trip mismatch");
}

#[test]
fn gpu_policy_round_trips_through_serde_json() {
    let policy = GpuPolicy {
        gpu_capability_class: GpuCapabilityClass::GpuFull3d,
        vk_device_required: true,
        dmabuf_passthrough_allowed: false,
        per_group_partitioning: true,
        iommu_required: false,
        expires_at: None,
    };

    let json = serde_json::to_string(&policy).expect("serialise GpuPolicy");
    let back: GpuPolicy = serde_json::from_str(&json).expect("deserialise GpuPolicy");
    assert_eq!(policy, back, "GpuPolicy round-trip mismatch");
}

// ---------------------------------------------------------------------------
// ResourceLimits defaults + validation
// ---------------------------------------------------------------------------

#[test]
fn resource_limits_default_strict_returns_sensible_restrictive_values() {
    let limits = ResourceLimits::default_strict();
    assert!(limits.cpu_quota_percent <= 25, "strict cpu should be ≤25%");
    assert!(
        limits.memory_max_bytes <= 128 * 1024 * 1024,
        "strict memory should be ≤128 MiB"
    );
    assert!(
        limits.io_max_bytes_per_sec.is_some(),
        "strict I/O should be capped"
    );
    assert!(
        limits.process_max_count.is_some(),
        "strict process count should be capped"
    );
}

#[test]
fn resource_limits_default_permissive_returns_relaxed_values() {
    let limits = ResourceLimits::default_permissive();
    assert!(
        limits.cpu_quota_percent >= 50,
        "permissive cpu should be ≥50%"
    );
    assert!(
        limits.memory_max_bytes >= 512 * 1024 * 1024,
        "permissive memory should be ≥512 MiB"
    );
    assert!(
        limits.io_max_bytes_per_sec.is_none(),
        "permissive I/O should be uncapped"
    );
}

#[test]
fn resource_limits_validate_rejects_cpu_quota_over_100() {
    let limits = ResourceLimits {
        cpu_quota_percent: 101,
        ..ResourceLimits::default_strict()
    };
    let err = limits.validate().unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("cpu_quota_percent"),
        "error should mention cpu_quota_percent: {msg}"
    );
}

#[test]
fn resource_limits_validate_rejects_memory_max_zero() {
    let limits = ResourceLimits {
        memory_max_bytes: 0,
        ..ResourceLimits::default_strict()
    };
    let err = limits.validate().unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("memory_max_bytes") || msg.contains("ResourceLimitsViolation"),
        "error should mention memory_max_bytes: {msg}"
    );
}

// ---------------------------------------------------------------------------
// SandboxError Display
// ---------------------------------------------------------------------------

#[test]
fn sandbox_error_display_strings_present_and_non_empty() {
    let profile_id = ProfileId::new();
    let errors: &[SandboxError] = &[
        SandboxError::ProfileNotFound(profile_id),
        SandboxError::InvalidProfile("field X missing".into()),
        SandboxError::ManifestSignatureInvalid,
        SandboxError::ManifestUnknownAuthority("unknown-ca".into()),
        SandboxError::ResourceLimitsViolation {
            limit: "cpu_quota_percent".into(),
            requested: 200,
            max: 100,
        },
        SandboxError::GpuPolicyViolation("compute denied by policy".into()),
        SandboxError::IsolationKindNotSupported {
            kind: IsolationKind::VmGuest,
            reason: "KVM not available".into(),
        },
        SandboxError::Internal("assertion failed".into()),
    ];

    for err in errors {
        let msg = format!("{err}");
        assert!(
            !msg.is_empty(),
            "SandboxError variant must have non-empty Display"
        );
    }
}

// ---------------------------------------------------------------------------
// Cross-crate import smoke test
// ---------------------------------------------------------------------------

#[test]
fn cross_crate_aios_action_action_id_import_compiles() {
    // Verify that the aios-action dependency is accessible from aios-sandbox
    // tests. This anchors the workspace dependency edge.
    let id = aios_action::ActionId::new();
    assert!(!format!("{id}").is_empty());
}
