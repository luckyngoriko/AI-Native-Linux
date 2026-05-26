//! Integration tests for `ResourceLimitEnforcer` (S3.2 `ResourceLimits` enforcement).
//!
//! Covers `check_usage`, `compute_remaining`, `validate_syscall`, `enforce_cpu_quota`,
//! `SyscallEnforcement` canonical allowlists, and type round-trips.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests use panicking assertions as the canonical failure signal"
)]

use aios_sandbox::{
    IsolationKind, ResourceLimitEnforcer, ResourceLimits, ResourceRemaining, ResourceRequest,
    ResourceUsage, SandboxError, SyscallEnforcement,
};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

const fn strict_limits() -> ResourceLimits {
    ResourceLimits::default_strict()
}

const fn small_request() -> ResourceRequest {
    ResourceRequest {
        cpu_pct: 5,
        memory_bytes: 32 * 1024 * 1024,
        io_bytes_per_sec: 512 * 1024,
        network_bytes_per_sec: 512 * 1024,
        process_count: 2,
        fd_count: 32,
    }
}

const fn permissive_limits() -> ResourceLimits {
    ResourceLimits {
        cpu_quota_percent: 100,
        memory_max_bytes: 4 * 1024 * 1024 * 1024,
        io_max_bytes_per_sec: None,
        network_max_bytes_per_sec: None,
        process_max_count: Some(64),
        file_descriptor_max: Some(256),
        expires_at: None,
    }
}

// ---------------------------------------------------------------------------
// 1. ResourceLimitEnforcer::new_with_defaults() succeeds.
// ---------------------------------------------------------------------------

#[test]
fn new_with_defaults_succeeds() {
    let enforcer = ResourceLimitEnforcer::new_with_defaults();
    // Verify the enforcer can be used immediately.
    assert!(enforcer
        .check_usage(&small_request(), &permissive_limits())
        .is_ok());
}

// ---------------------------------------------------------------------------
// 2. check_usage within limits → Ok.
// ---------------------------------------------------------------------------

#[test]
fn check_usage_within_limits_returns_ok() {
    let enforcer = ResourceLimitEnforcer::new_with_defaults();
    assert!(enforcer
        .check_usage(&small_request(), &permissive_limits())
        .is_ok());
}

// ---------------------------------------------------------------------------
// 3. check_usage cpu exceeds → ResourceLimitsViolation.
// ---------------------------------------------------------------------------

#[test]
fn check_usage_cpu_exceeds_returns_resource_limits_violation() {
    let enforcer = ResourceLimitEnforcer::new_with_defaults();
    let req = ResourceRequest {
        cpu_pct: 200,
        ..small_request()
    };
    let limits = ResourceLimits {
        cpu_quota_percent: 50,
        ..strict_limits()
    };
    let err = enforcer.check_usage(&req, &limits).unwrap_err();
    assert!(matches!(err, SandboxError::ResourceLimitsViolation { .. }));
    let msg = format!("{err}");
    assert!(msg.contains("cpu_quota_percent"), "got: {msg}");
}

// ---------------------------------------------------------------------------
// 4. check_usage memory exceeds → ResourceLimitsViolation.
// ---------------------------------------------------------------------------

#[test]
fn check_usage_memory_exceeds_returns_violation() {
    let enforcer = ResourceLimitEnforcer::new_with_defaults();
    let req = ResourceRequest {
        memory_bytes: 16 * 1024 * 1024 * 1024,
        ..small_request()
    };
    let limits = ResourceLimits {
        memory_max_bytes: 128 * 1024 * 1024,
        ..strict_limits()
    };
    let err = enforcer.check_usage(&req, &limits).unwrap_err();
    assert!(matches!(err, SandboxError::ResourceLimitsViolation { .. }));
    let msg = format!("{err}");
    assert!(msg.contains("memory_max_bytes"), "got: {msg}");
}

// ---------------------------------------------------------------------------
// 5. check_usage with None io/network limits → skipped.
// ---------------------------------------------------------------------------

#[test]
fn check_usage_none_io_network_limits_skipped() {
    let enforcer = ResourceLimitEnforcer::new_with_defaults();
    let req = ResourceRequest {
        io_bytes_per_sec: 1_000_000_000,
        network_bytes_per_sec: 1_000_000_000,
        ..small_request()
    };
    let limits = ResourceLimits {
        io_max_bytes_per_sec: None,
        network_max_bytes_per_sec: None,
        ..permissive_limits()
    };
    assert!(enforcer.check_usage(&req, &limits).is_ok());
}

// ---------------------------------------------------------------------------
// 6. compute_remaining returns signed values; negative means over-budget.
// ---------------------------------------------------------------------------

#[test]
fn compute_remaining_returns_signed_negative_means_over_budget() {
    let enforcer = ResourceLimitEnforcer::new_with_defaults();
    let usage = ResourceUsage {
        cpu_pct: strict_limits().cpu_quota_percent + 5,
        memory_bytes: strict_limits().memory_max_bytes + 1,
        io_bytes_per_sec: 0,
        network_bytes_per_sec: 0,
        process_count: 0,
        fd_count: 0,
    };
    let remaining = enforcer.compute_remaining(&usage, &strict_limits());
    assert!(remaining.cpu_pct < 0, "CPU should be over budget");
    assert!(remaining.memory_bytes < 0, "memory should be over budget");
}

#[test]
fn compute_remaining_with_headroom_returns_positive_values() {
    let enforcer = ResourceLimitEnforcer::new_with_defaults();
    let usage = ResourceUsage {
        cpu_pct: 2,
        memory_bytes: 10 * 1024 * 1024,
        io_bytes_per_sec: 0,
        network_bytes_per_sec: 0,
        process_count: 1,
        fd_count: 4,
    };
    let remaining = enforcer.compute_remaining(&usage, &strict_limits());
    assert!(remaining.cpu_pct > 0);
    assert!(remaining.memory_bytes > 0);
}

// ---------------------------------------------------------------------------
// 7. validate_syscall with None allowlist → Ok for any syscall.
// ---------------------------------------------------------------------------

#[test]
fn validate_syscall_none_allowlist_ok_for_any_syscall() {
    let enforcer = ResourceLimitEnforcer::new_with_defaults();
    assert!(enforcer.validate_syscall("read", None).is_ok());
    assert!(enforcer.validate_syscall("malicious_syscall", None).is_ok());
    assert!(enforcer
        .validate_syscall("kernel_module_load", None)
        .is_ok());
}

// ---------------------------------------------------------------------------
// 8. validate_syscall with allowlist + syscall in list → Ok.
// ---------------------------------------------------------------------------

#[test]
fn validate_syscall_in_allowlist_returns_ok() {
    let enforcer = ResourceLimitEnforcer::new_with_defaults();
    let allowlist: Vec<String> = vec!["read".into(), "write".into(), "exit".into()];
    assert!(enforcer.validate_syscall("read", Some(&allowlist)).is_ok());
    assert!(enforcer.validate_syscall("exit", Some(&allowlist)).is_ok());
}

// ---------------------------------------------------------------------------
// 9. validate_syscall with allowlist + syscall NOT in list → SyscallNotAllowed.
// ---------------------------------------------------------------------------

#[test]
fn validate_syscall_not_in_allowlist_returns_syscall_not_allowed() {
    let enforcer = ResourceLimitEnforcer::new_with_defaults();
    let allowlist: Vec<String> = vec!["read".into(), "write".into()];
    let err = enforcer
        .validate_syscall("mount", Some(&allowlist))
        .unwrap_err();
    assert!(matches!(err, SandboxError::SyscallNotAllowed { .. }));
    let msg = format!("{err}");
    assert!(msg.contains("mount"), "got: {msg}");
}

// ---------------------------------------------------------------------------
// 10. enforce_cpu_quota clamps requested to limit.
// ---------------------------------------------------------------------------

#[test]
fn enforce_cpu_quota_clamps_requested_to_limit() {
    let enforcer = ResourceLimitEnforcer::new_with_defaults();
    assert_eq!(enforcer.enforce_cpu_quota(80, 50).unwrap(), 50);
    assert_eq!(enforcer.enforce_cpu_quota(100, 30).unwrap(), 30);
}

// ---------------------------------------------------------------------------
// 11. enforce_cpu_quota returns error if requested ≤ 0 or > 100.
// ---------------------------------------------------------------------------

#[test]
fn enforce_cpu_quota_rejects_zero_and_over_100() {
    let enforcer = ResourceLimitEnforcer::new_with_defaults();
    assert!(enforcer.enforce_cpu_quota(0, 100).is_err());
    assert!(enforcer.enforce_cpu_quota(101, 100).is_err());
    assert!(enforcer.enforce_cpu_quota(200, 100).is_err());
}

// ---------------------------------------------------------------------------
// 12. SyscallEnforcement canonical allowlists for IsolationKind::ProcessContainer.
// ---------------------------------------------------------------------------

#[test]
fn syscall_enforcement_process_container_canonical_allowlist() {
    let enforcement = SyscallEnforcement::canonical(IsolationKind::ProcessContainer)
        .expect("ProcessContainer should have a canonical allowlist");
    assert_eq!(enforcement.isolation_kind, IsolationKind::ProcessContainer);
    assert!(
        enforcement.allowlist.len() >= 50,
        "expected >=50 syscalls, got {}",
        enforcement.allowlist.len()
    );
    // Core syscalls must be present.
    assert!(enforcement.validate("read").is_ok());
    assert!(enforcement.validate("write").is_ok());
    assert!(enforcement.validate("exit").is_ok());
    assert!(enforcement.validate("mmap").is_ok());
    assert!(enforcement.validate("futex").is_ok());
}

#[test]
fn syscall_enforcement_process_container_rejects_forbidden_syscall() {
    let enforcement = SyscallEnforcement::canonical(IsolationKind::ProcessContainer)
        .expect("ProcessContainer should have a canonical allowlist");
    let err = enforcement.validate("mount").unwrap_err();
    assert!(matches!(err, SandboxError::SyscallNotAllowed { .. }));
}

// ---------------------------------------------------------------------------
// 13. ResourceRequest + ResourceUsage + ResourceRemaining round-trip serde.
// ---------------------------------------------------------------------------

#[test]
fn resource_request_usage_remaining_serde_round_trip() {
    let req = ResourceRequest {
        cpu_pct: 75,
        memory_bytes: 256 * 1024 * 1024,
        io_bytes_per_sec: 10 * 1024 * 1024,
        network_bytes_per_sec: 5 * 1024 * 1024,
        process_count: 8,
        fd_count: 128,
    };
    let json = serde_json::to_string(&req).unwrap();
    let back: ResourceRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(req, back);

    let usage = ResourceUsage {
        cpu_pct: 30,
        memory_bytes: 128 * 1024 * 1024,
        io_bytes_per_sec: 1_000_000,
        network_bytes_per_sec: 500_000,
        process_count: 4,
        fd_count: 64,
    };
    let json = serde_json::to_string(&usage).unwrap();
    let back: ResourceUsage = serde_json::from_str(&json).unwrap();
    assert_eq!(usage, back);

    let rem = ResourceRemaining {
        cpu_pct: 50,
        memory_bytes: 128 * 1024 * 1024,
        io_bytes_per_sec: -1,
        network_bytes_per_sec: i64::MAX,
        process_count: 3,
        fd_count: -5,
    };
    let json = serde_json::to_string(&rem).unwrap();
    let back: ResourceRemaining = serde_json::from_str(&json).unwrap();
    assert_eq!(rem, back);
}

// ---------------------------------------------------------------------------
// 14. Concurrent check_usage from 3 tasks → no panic.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_check_usage_from_3_tasks_no_panic() {
    use std::sync::Arc;
    let enforcer = Arc::new(ResourceLimitEnforcer::new_with_defaults());
    let limits = permissive_limits();

    let e1 = Arc::clone(&enforcer);
    let l1 = limits.clone();
    let t1 = tokio::task::spawn(async move {
        e1.check_usage(&small_request(), &l1).unwrap();
    });

    let e2 = Arc::clone(&enforcer);
    let l2 = limits.clone();
    let t2 = tokio::task::spawn(async move {
        e2.check_usage(
            &ResourceRequest {
                cpu_pct: 50,
                ..small_request()
            },
            &l2,
        )
        .unwrap();
    });

    let e3 = Arc::clone(&enforcer);
    let l3 = limits.clone();
    let t3 = tokio::task::spawn(async move {
        e3.check_usage(
            &ResourceRequest {
                cpu_pct: 75,
                ..small_request()
            },
            &l3,
        )
        .unwrap();
    });

    let (r1, r2, r3) = tokio::join!(t1, t2, t3);
    r1.unwrap();
    r2.unwrap();
    r3.unwrap();
}
