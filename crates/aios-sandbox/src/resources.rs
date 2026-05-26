use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::SandboxError;

/// Resource limits for a sandbox profile (S3.2).
///
/// CPU, memory, I/O, process count, and file descriptor caps. The composer
/// merges resource limits by taking the minimum across all six sources.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResourceLimits {
    /// CPU quota as percentage of one core (0–100, where 100 = 1 full core).
    pub cpu_quota_percent: u32,
    /// Maximum memory in bytes.
    pub memory_max_bytes: u64,
    /// Maximum I/O throughput in bytes per second, if capped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub io_max_bytes_per_sec: Option<u64>,
    /// Maximum network throughput in bytes per second, if capped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_max_bytes_per_sec: Option<u64>,
    /// Maximum number of child processes, if capped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_max_count: Option<u32>,
    /// Maximum number of open file descriptors, if capped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_descriptor_max: Option<u32>,
    /// When these limits expire, if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

impl ResourceLimits {
    /// Most restrictive defaults — used as the composer default source (SC).
    #[must_use]
    pub const fn default_strict() -> Self {
        Self {
            cpu_quota_percent: 10,
            memory_max_bytes: 64 * 1024 * 1024,           // 64 MiB
            io_max_bytes_per_sec: Some(1024 * 1024),      // 1 MiB/s
            network_max_bytes_per_sec: Some(1024 * 1024), // 1 MiB/s
            process_max_count: Some(4),
            file_descriptor_max: Some(64),
            expires_at: None,
        }
    }

    /// Relaxed defaults for service-class actions (e.g. system daemons).
    #[must_use]
    pub const fn default_permissive() -> Self {
        Self {
            cpu_quota_percent: 100,
            memory_max_bytes: 2 * 1024 * 1024 * 1024, // 2 GiB
            io_max_bytes_per_sec: None,               // uncapped
            network_max_bytes_per_sec: None,          // uncapped
            process_max_count: Some(256),
            file_descriptor_max: Some(1024),
            expires_at: None,
        }
    }

    /// Validate resource limits for sanity.
    ///
    /// Returns `Ok(())` if all limits are within acceptable ranges,
    /// or a `SandboxError` describing the first violation found.
    ///
    /// # Errors
    ///
    /// Returns `ResourceLimitsViolation` if `cpu_quota_percent` exceeds 100
    /// or `memory_max_bytes` is zero.
    pub fn validate(&self) -> Result<(), SandboxError> {
        if self.cpu_quota_percent > 100 {
            return Err(SandboxError::ResourceLimitsViolation {
                limit: "cpu_quota_percent".into(),
                requested: u64::from(self.cpu_quota_percent),
                max: 100,
            });
        }
        if self.memory_max_bytes == 0 {
            return Err(SandboxError::ResourceLimitsViolation {
                limit: "memory_max_bytes".into(),
                requested: 0,
                max: u64::MAX,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    #[test]
    fn default_strict_returns_sensible_restrictive_values() {
        let limits = ResourceLimits::default_strict();
        assert_eq!(limits.cpu_quota_percent, 10);
        assert_eq!(limits.memory_max_bytes, 64 * 1024 * 1024);
        assert!(limits.io_max_bytes_per_sec.is_some());
        assert!(limits.process_max_count.expect("strict has process cap") <= 8);
    }

    #[test]
    fn default_permissive_returns_relaxed_values() {
        let limits = ResourceLimits::default_permissive();
        assert_eq!(limits.cpu_quota_percent, 100);
        assert!(limits.memory_max_bytes > 64 * 1024 * 1024);
        assert!(limits.io_max_bytes_per_sec.is_none());
    }

    #[test]
    fn validate_rejects_cpu_quota_over_100() {
        let mut limits = ResourceLimits::default_strict();
        limits.cpu_quota_percent = 101;
        let err = limits.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("cpu_quota_percent"),
            "expected cpu_quota_percent in: {msg}"
        );
    }

    #[test]
    fn validate_rejects_memory_max_zero() {
        let mut limits = ResourceLimits::default_strict();
        limits.memory_max_bytes = 0;
        let err = limits.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("memory_max_bytes") || msg.contains("ResourceLimitsViolation"),
            "expected memory_max_bytes violation in: {msg}"
        );
    }

    #[test]
    fn validate_accepts_sensible_values() {
        let limits = ResourceLimits::default_strict();
        assert!(limits.validate().is_ok());
    }

    #[test]
    fn resource_limits_serde_round_trip() {
        let limits = ResourceLimits {
            cpu_quota_percent: 50,
            memory_max_bytes: 512 * 1024 * 1024,
            io_max_bytes_per_sec: Some(10 * 1024 * 1024),
            network_max_bytes_per_sec: None,
            process_max_count: Some(16),
            file_descriptor_max: None,
            expires_at: None,
        };
        let json = serde_json::to_string(&limits).unwrap();
        let back: ResourceLimits = serde_json::from_str(&json).unwrap();
        assert_eq!(limits, back);
    }
}
