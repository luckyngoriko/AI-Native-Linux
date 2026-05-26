use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::SandboxError;
use crate::evidence_emit::SandboxEvidenceEmitter;
use crate::evidence_payloads::ResourceLimitExceededPayload;
use crate::isolation::IsolationKind;
use crate::resources::ResourceLimits;

/// A requested resource profile — what the sandbox instance wants to use.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceRequest {
    /// CPU quota percentage of one core (0–100).
    pub cpu_pct: u32,
    /// Memory in bytes.
    pub memory_bytes: u64,
    /// I/O throughput in bytes per second.
    pub io_bytes_per_sec: u64,
    /// Network throughput in bytes per second.
    pub network_bytes_per_sec: u64,
    /// Number of child processes.
    pub process_count: u32,
    /// Number of open file descriptors.
    pub fd_count: u32,
}

/// Currently observed resource usage (same shape as `ResourceRequest`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceUsage {
    /// Current CPU usage percentage.
    pub cpu_pct: u32,
    /// Current memory usage in bytes.
    pub memory_bytes: u64,
    /// Current I/O rate in bytes per second.
    pub io_bytes_per_sec: u64,
    /// Current network rate in bytes per second.
    pub network_bytes_per_sec: u64,
    /// Current process count.
    pub process_count: u32,
    /// Current open file descriptor count.
    pub fd_count: u32,
}

/// Remaining resource budget (signed — negative means over-budget).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceRemaining {
    /// Remaining CPU percentage (negative = over-budget).
    pub cpu_pct: i32,
    /// Remaining memory in bytes (negative = over-budget).
    pub memory_bytes: i64,
    /// Remaining I/O budget (negative = over-budget; `i64::MAX` if uncapped).
    pub io_bytes_per_sec: i64,
    /// Remaining network budget (negative = over-budget; `i64::MAX` if uncapped).
    pub network_bytes_per_sec: i64,
    /// Remaining process slots (negative = over-budget).
    pub process_count: i32,
    /// Remaining file descriptor slots (negative = over-budget).
    pub fd_count: i32,
}

/// Enforces resource limits for sandbox profiles (S3.2 `ResourceLimits`).
///
/// Gates CPU, memory, I/O, network, process count, and file descriptor usage
/// against a profile's `ResourceLimits`, producing `ResourceLimitsViolation`
/// when any cap is exceeded. Also provides syscall allowlist validation.
#[derive(Clone)]
pub struct ResourceLimitEnforcer {
    /// Optional evidence emitter for resource limit exceeded events.
    evidence_emitter: Option<Arc<SandboxEvidenceEmitter>>,
}

impl fmt::Debug for ResourceLimitEnforcer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResourceLimitEnforcer")
            .field(
                "evidence_emitter",
                &self
                    .evidence_emitter
                    .as_ref()
                    .map(|_| "<SandboxEvidenceEmitter>"),
            )
            .finish()
    }
}

impl ResourceLimitEnforcer {
    /// Create a new `ResourceLimitEnforcer` with sensible defaults.
    #[must_use]
    pub const fn new_with_defaults() -> Self {
        Self {
            evidence_emitter: None,
        }
    }

    /// Attach an evidence emitter for resource limit exceeded events.
    #[must_use]
    pub fn with_evidence_emitter(mut self, emitter: Arc<SandboxEvidenceEmitter>) -> Self {
        self.evidence_emitter = Some(emitter);
        self
    }

    /// Check whether a `ResourceRequest` fits within the given `ResourceLimits`.
    ///
    /// Returns `Ok(())` if every requested field is ≤ the corresponding limit.
    /// Optional limits (`None`) are skipped (uncapped).
    ///
    /// # Errors
    ///
    /// Returns `ResourceLimitsViolation` naming the first limit that was exceeded.
    pub fn check_usage(
        &self,
        request: &ResourceRequest,
        limits: &ResourceLimits,
    ) -> Result<(), SandboxError> {
        if request.cpu_pct > limits.cpu_quota_percent {
            self.emit_resource_limit_exceeded(
                "cpu_quota_percent",
                u64::from(request.cpu_pct),
                u64::from(limits.cpu_quota_percent),
            );
            return Err(SandboxError::ResourceLimitsViolation {
                limit: "cpu_quota_percent".into(),
                requested: u64::from(request.cpu_pct),
                max: u64::from(limits.cpu_quota_percent),
            });
        }
        if request.memory_bytes > limits.memory_max_bytes {
            self.emit_resource_limit_exceeded(
                "memory_max_bytes",
                request.memory_bytes,
                limits.memory_max_bytes,
            );
            return Err(SandboxError::ResourceLimitsViolation {
                limit: "memory_max_bytes".into(),
                requested: request.memory_bytes,
                max: limits.memory_max_bytes,
            });
        }
        if let Some(cap) = limits.io_max_bytes_per_sec {
            if request.io_bytes_per_sec > cap {
                self.emit_resource_limit_exceeded(
                    "io_max_bytes_per_sec",
                    request.io_bytes_per_sec,
                    cap,
                );
                return Err(SandboxError::ResourceLimitsViolation {
                    limit: "io_max_bytes_per_sec".into(),
                    requested: request.io_bytes_per_sec,
                    max: cap,
                });
            }
        }
        if let Some(cap) = limits.network_max_bytes_per_sec {
            if request.network_bytes_per_sec > cap {
                self.emit_resource_limit_exceeded(
                    "network_max_bytes_per_sec",
                    request.network_bytes_per_sec,
                    cap,
                );
                return Err(SandboxError::ResourceLimitsViolation {
                    limit: "network_max_bytes_per_sec".into(),
                    requested: request.network_bytes_per_sec,
                    max: cap,
                });
            }
        }
        if let Some(cap) = limits.process_max_count {
            if request.process_count > cap {
                self.emit_resource_limit_exceeded(
                    "process_max_count",
                    u64::from(request.process_count),
                    u64::from(cap),
                );
                return Err(SandboxError::ResourceLimitsViolation {
                    limit: "process_max_count".into(),
                    requested: u64::from(request.process_count),
                    max: u64::from(cap),
                });
            }
        }
        if let Some(cap) = limits.file_descriptor_max {
            if request.fd_count > cap {
                self.emit_resource_limit_exceeded(
                    "file_descriptor_max",
                    u64::from(request.fd_count),
                    u64::from(cap),
                );
                return Err(SandboxError::ResourceLimitsViolation {
                    limit: "file_descriptor_max".into(),
                    requested: u64::from(request.fd_count),
                    max: u64::from(cap),
                });
            }
        }
        Ok(())
    }

    fn emit_resource_limit_exceeded(&self, limit_kind: &str, requested: u64, max: u64) {
        if let Some(ref emitter) = self.evidence_emitter {
            let emitter = Arc::clone(emitter);
            let payload = ResourceLimitExceededPayload {
                profile_id: crate::ProfileId::new(),
                limit_kind: limit_kind.to_string(),
                requested,
                max,
                exceeded_at: chrono::Utc::now(),
            };
            tokio::spawn(async move {
                let _ = emitter.emit_resource_limit_exceeded(&payload, None).await;
            });
        }
    }

    /// Compute how much of each resource cap remains (signed).
    ///
    /// `ResourceRemaining` uses signed integers so that negative values signal
    /// over-budget usage. Optional limits map to `i64::MAX` (uncapped).
    #[must_use]
    pub fn compute_remaining(
        &self,
        usage: &ResourceUsage,
        limits: &ResourceLimits,
    ) -> ResourceRemaining {
        ResourceRemaining {
            cpu_pct: i32::try_from(limits.cpu_quota_percent).unwrap_or(i32::MAX)
                - i32::try_from(usage.cpu_pct).unwrap_or(i32::MAX),
            memory_bytes: i64::try_from(limits.memory_max_bytes).unwrap_or(i64::MAX)
                - i64::try_from(usage.memory_bytes).unwrap_or(i64::MAX),
            io_bytes_per_sec: limits.io_max_bytes_per_sec.map_or(i64::MAX, |cap| {
                i64::try_from(cap).unwrap_or(i64::MAX)
                    - i64::try_from(usage.io_bytes_per_sec).unwrap_or(i64::MAX)
            }),
            network_bytes_per_sec: limits.network_max_bytes_per_sec.map_or(i64::MAX, |cap| {
                i64::try_from(cap).unwrap_or(i64::MAX)
                    - i64::try_from(usage.network_bytes_per_sec).unwrap_or(i64::MAX)
            }),
            process_count: limits.process_max_count.map_or(i32::MAX, |cap| {
                i32::try_from(cap).unwrap_or(i32::MAX)
                    - i32::try_from(usage.process_count).unwrap_or(i32::MAX)
            }),
            fd_count: limits.file_descriptor_max.map_or(i32::MAX, |cap| {
                i32::try_from(cap).unwrap_or(i32::MAX)
                    - i32::try_from(usage.fd_count).unwrap_or(i32::MAX)
            }),
        }
    }

    /// Validate a syscall against an optional allowlist.
    ///
    /// If `allowlist` is `None`, all syscalls are permitted (no restriction).
    /// If `allowlist` is `Some`, the syscall must appear in the list.
    ///
    /// # Errors
    ///
    /// Returns `SyscallNotAllowed` when the syscall is not in the allowlist.
    pub fn validate_syscall(
        &self,
        syscall: &str,
        allowlist: Option<&[String]>,
    ) -> Result<(), SandboxError> {
        allowlist.map_or(Ok(()), |list| {
            if list.iter().any(|s| s == syscall) {
                Ok(())
            } else {
                Err(SandboxError::SyscallNotAllowed {
                    syscall: syscall.to_string(),
                    isolation_kind: IsolationKind::NamespaceLocal,
                })
            }
        })
    }

    /// Enforce CPU quota — clamp the requested percentage to the limit.
    ///
    /// Returns the clamped value (≤ limit). Rejects invalid requests
    /// where `requested_pct` is 0 or exceeds 100.
    ///
    /// # Errors
    ///
    /// Returns `ResourceLimitsViolation` if `requested_pct` is 0 or > 100.
    pub fn enforce_cpu_quota(&self, requested_pct: u32, limit: u32) -> Result<u32, SandboxError> {
        if requested_pct == 0 {
            return Err(SandboxError::ResourceLimitsViolation {
                limit: "cpu_quota_percent".into(),
                requested: 0,
                max: u64::from(limit),
            });
        }
        if requested_pct > 100 {
            return Err(SandboxError::ResourceLimitsViolation {
                limit: "cpu_quota_percent".into(),
                requested: u64::from(requested_pct),
                max: 100,
            });
        }
        Ok(requested_pct.min(limit))
    }
}

/// Syscall allowlist enforcement per isolation kind (S3.2).
///
/// Holds a canonical set of allowed Linux syscalls for each `IsolationKind`.
/// The allowlist is consulted at sandbox entry to reject unsupported syscalls.
#[derive(Debug, Clone)]
pub struct SyscallEnforcement {
    /// The isolation kind this enforcement applies to.
    pub isolation_kind: IsolationKind,
    /// Syscall names allowed under this isolation kind.
    pub allowlist: Vec<String>,
}

impl SyscallEnforcement {
    /// Build a canonical syscall allowlist for the given `IsolationKind`.
    ///
    /// Returns `None` for `NoIsolation` (no restriction — use host allowlist).
    #[must_use]
    pub fn canonical(kind: IsolationKind) -> Option<Self> {
        let allowlist = match kind {
            IsolationKind::NoIsolation => return None,
            IsolationKind::ProcessContainer => process_container_allowlist(),
            IsolationKind::NamespaceLocal => namespace_local_allowlist(),
            IsolationKind::VmGuest => vm_guest_allowlist(),
            IsolationKind::BrowserOriginIsolated => browser_origin_allowlist(),
        };
        Some(Self {
            isolation_kind: kind,
            allowlist,
        })
    }

    /// Validate a syscall against this enforcement's allowlist.
    ///
    /// # Errors
    ///
    /// Returns `SyscallNotAllowed` when the syscall is not in the allowlist.
    pub fn validate(&self, syscall: &str) -> Result<(), SandboxError> {
        if self.allowlist.iter().any(|s| s == syscall) {
            Ok(())
        } else {
            Err(SandboxError::SyscallNotAllowed {
                syscall: syscall.to_string(),
                isolation_kind: self.isolation_kind,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Canonical allowlists per isolation kind (Linux seccomp BPF minimum sets)
// ---------------------------------------------------------------------------

fn process_container_allowlist() -> Vec<String> {
    syscalls(&[
        "read",
        "write",
        "open",
        "close",
        "stat",
        "fstat",
        "lstat",
        "poll",
        "lseek",
        "mmap",
        "mprotect",
        "munmap",
        "brk",
        "rt_sigaction",
        "rt_sigprocmask",
        "rt_sigreturn",
        "ioctl",
        "pread64",
        "pwrite64",
        "readv",
        "writev",
        "access",
        "pipe",
        "select",
        "sched_yield",
        "mremap",
        "msync",
        "madvise",
        "mincore",
        "dup",
        "dup2",
        "pause",
        "nanosleep",
        "getpid",
        "socket",
        "connect",
        "accept",
        "sendto",
        "recvfrom",
        "sendmsg",
        "recvmsg",
        "bind",
        "listen",
        "getsockname",
        "socketpair",
        "setsockopt",
        "clone",
        "fork",
        "execve",
        "exit",
        "wait4",
        "kill",
        "fcntl",
        "fsync",
        "fdatasync",
        "truncate",
        "ftruncate",
        "getdents",
        "getcwd",
        "chdir",
        "rename",
        "mkdir",
        "unlink",
        "readlink",
        "chmod",
        "fchmod",
        "umask",
        "gettimeofday",
        "getrlimit",
        "getuid",
        "getgid",
        "setuid",
        "setgid",
        "getgroups",
        "gettid",
        "prctl",
        "arch_prctl",
        "futex",
        "epoll_create",
        "epoll_ctl",
        "epoll_wait",
        "clock_gettime",
        "clock_getres",
    ])
}

fn namespace_local_allowlist() -> Vec<String> {
    // Broad set — namespace isolation still permits most syscalls.
    let mut base = process_container_allowlist();
    let extra = syscalls(&[
        "msgget",
        "msgsnd",
        "msgrcv",
        "msgctl",
        "semget",
        "semop",
        "semctl",
        "shmget",
        "shmat",
        "shmdt",
        "shmctl",
        "ptrace",
        "sched_setaffinity",
        "sched_getaffinity",
        "set_tid_address",
        "set_robust_list",
        "get_robust_list",
        "timerfd_create",
        "timerfd_settime",
        "timerfd_gettime",
        "signalfd",
        "memfd_create",
        "eventfd",
        "capget",
    ]);
    base.extend(extra);
    base
}

fn vm_guest_allowlist() -> Vec<String> {
    // Broad set — VM guest has a full kernel, only truly dangerous syscalls excluded.
    let mut base = namespace_local_allowlist();
    let extra = syscalls(&[
        "uname",
        "sysinfo",
        "times",
        "getrusage",
        "getrlimit",
        "setrlimit",
        "getegid",
        "geteuid",
        "getgid",
        "getgroups",
        "getpgid",
        "getpgrp",
        "getppid",
        "getresgid",
        "getresuid",
        "getsid",
        "getuid",
        "setfsgid",
        "setfsuid",
        "setpgid",
        "setregid",
        "setresgid",
        "setresuid",
        "setreuid",
        "setsid",
        "setgroups",
        "personality",
        "sigaltstack",
        "statfs",
        "fstatfs",
        "statx",
        "sync",
        "syncfs",
        "link",
        "symlink",
        "lchown",
        "fchown",
        "chown",
        "renameat",
        "renameat2",
        "mkdirat",
        "mknod",
        "mknodat",
        "newfstatat",
        "openat",
        "openat2",
        "readlinkat",
        "faccessat",
        "faccessat2",
        "fchmodat",
        "fchownat",
        "linkat",
        "unlinkat",
        "symlinkat",
        "mkfifo",
        "mkfifoat",
        "utimensat",
        "futimesat",
        "inotify_init",
        "inotify_init1",
        "inotify_add_watch",
        "inotify_rm_watch",
        "fanotify_init",
        "fanotify_mark",
        "perf_event_open",
        "bpf",
        "seccomp",
        "rseq",
    ]);
    base.extend(extra);
    base
}

fn browser_origin_allowlist() -> Vec<String> {
    // Minimal set for browser origin isolation.
    syscalls(&[
        "read",
        "write",
        "mmap",
        "munmap",
        "mprotect",
        "brk",
        "close",
        "futex",
        "exit",
        "rt_sigreturn",
        "sigaltstack",
        "getpid",
        "gettid",
        "sched_yield",
        "nanosleep",
        "clock_gettime",
        "clock_getres",
        "gettimeofday",
        "prctl",
        "arch_prctl",
        "set_robust_list",
        "set_tid_address",
    ])
}

fn syscalls(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| (*s).to_string()).collect()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    fn strict_limits() -> ResourceLimits {
        ResourceLimits::default_strict()
    }

    fn small_request() -> ResourceRequest {
        ResourceRequest {
            cpu_pct: 5,
            memory_bytes: 32 * 1024 * 1024, // 32 MiB
            io_bytes_per_sec: 512 * 1024,   // 512 KiB/s
            network_bytes_per_sec: 512 * 1024,
            process_count: 2,
            fd_count: 32,
        }
    }

    // --- ResourceLimitEnforcer ---

    #[test]
    fn new_with_defaults_succeeds() {
        let enforcer = ResourceLimitEnforcer::new_with_defaults();
        // Struct is zero-sized; construction should not panic.
        let _ = enforcer;
    }

    #[test]
    fn check_usage_within_limits_returns_ok() {
        let enforcer = ResourceLimitEnforcer::new_with_defaults();
        assert!(enforcer
            .check_usage(&small_request(), &strict_limits())
            .is_ok());
    }

    #[test]
    fn check_usage_cpu_exceeds_returns_violation_with_correct_limit_name() {
        let enforcer = ResourceLimitEnforcer::new_with_defaults();
        let req = ResourceRequest {
            cpu_pct: 50,
            ..small_request()
        };
        let limits = ResourceLimits {
            cpu_quota_percent: 25,
            ..strict_limits()
        };
        let err = enforcer.check_usage(&req, &limits).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("cpu_quota_percent"), "got: {msg}");
    }

    #[test]
    fn check_usage_memory_exceeds_returns_violation() {
        let enforcer = ResourceLimitEnforcer::new_with_defaults();
        let req = ResourceRequest {
            memory_bytes: 2 * 1024 * 1024 * 1024,
            ..small_request()
        };
        let limits = ResourceLimits {
            memory_max_bytes: 128 * 1024 * 1024,
            ..strict_limits()
        };
        let err = enforcer.check_usage(&req, &limits).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("memory_max_bytes"), "got: {msg}");
    }

    #[test]
    fn check_usage_none_limits_skipped() {
        let enforcer = ResourceLimitEnforcer::new_with_defaults();
        let req = ResourceRequest {
            io_bytes_per_sec: 1_000_000_000,
            network_bytes_per_sec: 1_000_000_000,
            ..small_request()
        };
        let limits = ResourceLimits {
            io_max_bytes_per_sec: None,
            network_max_bytes_per_sec: None,
            cpu_quota_percent: 100,
            memory_max_bytes: 4 * 1024 * 1024 * 1024,
            ..strict_limits()
        };
        assert!(enforcer.check_usage(&req, &limits).is_ok());
    }

    // --- compute_remaining ---

    #[test]
    fn compute_remaining_returns_signed_values() {
        let enforcer = ResourceLimitEnforcer::new_with_defaults();
        let usage = ResourceUsage {
            cpu_pct: 2,
            memory_bytes: 10 * 1024 * 1024,
            io_bytes_per_sec: 100 * 1024,
            network_bytes_per_sec: 100 * 1024,
            process_count: 1,
            fd_count: 8,
        };
        let remaining = enforcer.compute_remaining(&usage, &strict_limits());
        assert!(remaining.cpu_pct > 0, "should have CPU headroom");
        assert!(remaining.memory_bytes > 0, "should have memory headroom");
    }

    #[test]
    fn compute_remaining_negative_means_over_budget() {
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

    // --- validate_syscall ---

    #[test]
    fn validate_syscall_none_allowlist_ok_for_any_syscall() {
        let enforcer = ResourceLimitEnforcer::new_with_defaults();
        assert!(enforcer.validate_syscall("read", None).is_ok());
        assert!(enforcer.validate_syscall("malicious_syscall", None).is_ok());
    }

    #[test]
    fn validate_syscall_in_allowlist_returns_ok() {
        let enforcer = ResourceLimitEnforcer::new_with_defaults();
        let allowlist = vec!["read".to_string(), "write".to_string(), "exit".to_string()];
        assert!(enforcer.validate_syscall("read", Some(&allowlist)).is_ok());
    }

    #[test]
    fn validate_syscall_not_in_allowlist_returns_syscall_not_allowed() {
        let enforcer = ResourceLimitEnforcer::new_with_defaults();
        let allowlist = vec!["read".to_string(), "write".to_string()];
        let err = enforcer
            .validate_syscall("mount", Some(&allowlist))
            .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("mount"), "got: {msg}");
    }

    // --- enforce_cpu_quota ---

    #[test]
    fn enforce_cpu_quota_clamps_requested_to_limit() {
        let enforcer = ResourceLimitEnforcer::new_with_defaults();
        let result = enforcer.enforce_cpu_quota(80, 50).unwrap();
        assert_eq!(result, 50);
    }

    #[test]
    fn enforce_cpu_quota_returns_ok_when_requested_within_limit() {
        let enforcer = ResourceLimitEnforcer::new_with_defaults();
        let result = enforcer.enforce_cpu_quota(30, 100).unwrap();
        assert_eq!(result, 30);
    }

    #[test]
    fn enforce_cpu_quota_rejects_zero_requested() {
        let enforcer = ResourceLimitEnforcer::new_with_defaults();
        let err = enforcer.enforce_cpu_quota(0, 100).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("cpu_quota_percent"), "got: {msg}");
    }

    #[test]
    fn enforce_cpu_quota_rejects_over_100() {
        let enforcer = ResourceLimitEnforcer::new_with_defaults();
        let err = enforcer.enforce_cpu_quota(101, 100).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("cpu_quota_percent"), "got: {msg}");
    }

    // --- SyscallEnforcement canonical allowlists ---

    #[test]
    fn syscall_enforcement_process_container_has_allowlist() {
        let enforcement = SyscallEnforcement::canonical(IsolationKind::ProcessContainer)
            .expect("ProcessContainer should have an allowlist");
        assert!(
            enforcement.allowlist.len() >= 80,
            "expected >=80 syscalls, got {}",
            enforcement.allowlist.len()
        );
        assert!(enforcement.validate("read").is_ok());
        assert!(enforcement.validate("write").is_ok());
        assert!(enforcement.validate("exit").is_ok());
    }

    #[test]
    fn syscall_enforcement_process_container_rejects_forbidden_syscall() {
        let enforcement = SyscallEnforcement::canonical(IsolationKind::ProcessContainer)
            .expect("ProcessContainer should have an allowlist");
        let err = enforcement.validate("mount").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("mount"), "got: {msg}");
    }

    #[test]
    fn syscall_enforcement_no_isolation_returns_none() {
        assert!(SyscallEnforcement::canonical(IsolationKind::NoIsolation).is_none());
    }

    #[test]
    fn syscall_enforcement_every_isolation_kind_has_expected_shape() {
        use strum::IntoEnumIterator;
        for kind in IsolationKind::iter() {
            match SyscallEnforcement::canonical(kind) {
                None => {
                    assert_eq!(
                        kind,
                        IsolationKind::NoIsolation,
                        "only NoIsolation should return None"
                    );
                }
                Some(enforcement) => {
                    assert_eq!(enforcement.isolation_kind, kind);
                    assert!(
                        !enforcement.allowlist.is_empty(),
                        "allowlist for {kind:?} should not be empty"
                    );
                }
            }
        }
    }

    // --- ResourceRequest + ResourceUsage + ResourceRemaining round-trip ---

    #[test]
    fn resource_request_serde_round_trip() {
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
    }

    #[test]
    fn resource_usage_serde_round_trip() {
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
    }

    #[test]
    fn resource_remaining_serde_round_trip() {
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

    // --- concurrent ---

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_check_usage_from_3_tasks_no_panic() {
        use std::sync::Arc;
        let enforcer = Arc::new(ResourceLimitEnforcer::new_with_defaults());
        let limits = ResourceLimits {
            cpu_quota_percent: 100,
            memory_max_bytes: 4 * 1024 * 1024 * 1024,
            io_max_bytes_per_sec: None,
            network_max_bytes_per_sec: None,
            process_max_count: Some(64),
            file_descriptor_max: Some(256),
            expires_at: None,
        };

        let e1 = Arc::clone(&enforcer);
        let l1 = limits.clone();
        let t1 = tokio::task::spawn(async move {
            let req = ResourceRequest {
                cpu_pct: 25,
                ..small_request()
            };
            e1.check_usage(&req, &l1).unwrap();
        });

        let e2 = Arc::clone(&enforcer);
        let l2 = limits.clone();
        let t2 = tokio::task::spawn(async move {
            let req = ResourceRequest {
                cpu_pct: 50,
                ..small_request()
            };
            e2.check_usage(&req, &l2).unwrap();
        });

        let e3 = Arc::clone(&enforcer);
        let l3 = limits.clone();
        let t3 = tokio::task::spawn(async move {
            let req = ResourceRequest {
                cpu_pct: 75,
                ..small_request()
            };
            e3.check_usage(&req, &l3).unwrap();
        });

        let (r1, r2, r3) = tokio::join!(t1, t2, t3);
        r1.unwrap();
        r2.unwrap();
        r3.unwrap();
    }
}
