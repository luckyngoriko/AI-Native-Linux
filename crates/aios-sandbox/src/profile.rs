use serde::{Deserialize, Serialize};

use crate::{GpuPolicy, IsolationKind, NetworkPosture, ResourceLimits};

/// Sandbox profile identifier — `sbx_<ULID>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProfileId(pub String);

impl ProfileId {
    /// Create a new `ProfileId` with the `sbx_` prefix and a fresh ULID.
    #[must_use]
    pub fn new() -> Self {
        Self(format!("sbx_{}", ulid::Ulid::new()))
    }
}

impl Default for ProfileId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A typed sandbox profile (S3.2).
///
/// Defines the isolation boundary, resource limits, GPU policy, and network
/// posture for a single sandboxed execution context. Profiles are content-
/// addressed once applied; mutation requires a new `ProfileId`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SandboxProfile {
    /// Content-addressed profile identifier.
    pub profile_id: ProfileId,
    /// Human-readable label (no semantics).
    pub name: String,
    /// Human-readable description of the profile's purpose.
    pub description: String,
    /// Kernel-level isolation boundary.
    pub isolation_kind: IsolationKind,
    /// CPU, memory, I/O, process, and FD limits.
    pub resource_limits: ResourceLimits,
    /// GPU access policy.
    pub gpu_policy: GpuPolicy,
    /// Network posture.
    pub network_posture: NetworkPosture,
    /// Optional syscall allow-list (seccomp filter names).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub syscall_allowlist: Option<Vec<String>>,
    /// Trusted authority that signed this profile.
    pub signing_authority: String,
    /// Ed25519 signature over the canonical profile bytes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signature_ed25519: Vec<u8>,
}

impl SandboxProfile {
    /// Create a new profile with strictest defaults (composer SC source).
    pub fn new_strict(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            profile_id: ProfileId::new(),
            name: name.into(),
            description: description.into(),
            isolation_kind: IsolationKind::NamespaceLocal,
            resource_limits: ResourceLimits::default_strict(),
            gpu_policy: GpuPolicy::default_deny_all(),
            network_posture: NetworkPosture::DenyAll,
            syscall_allowlist: None,
            signing_authority: String::new(),
            signature_ed25519: Vec::new(),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use crate::GpuCapabilityClass;

    #[test]
    fn profile_id_default_creates_sbx_prefixed_ulid() {
        let id = ProfileId::new();
        assert!(
            id.0.starts_with("sbx_"),
            "expected sbx_ prefix, got: {}",
            id.0
        );
        // ULID is 26 chars + 4-char prefix = 30
        assert_eq!(
            id.0.len(),
            30,
            "expected 30 chars (sbx_ + 26-char ULID), got: {}",
            id.0.len()
        );
    }

    #[test]
    fn profile_id_display_round_trips() {
        let id = ProfileId::new();
        let displayed = format!("{id}");
        assert_eq!(displayed, id.0);
    }

    #[test]
    fn profile_id_serde_round_trip() {
        let id = ProfileId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: ProfileId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn sandbox_profile_new_strict_has_deny_all_defaults() {
        let profile = SandboxProfile::new_strict("test", "A test profile");
        assert_eq!(profile.isolation_kind, IsolationKind::NamespaceLocal);
        assert_eq!(profile.network_posture, NetworkPosture::DenyAll);
        assert_eq!(
            profile.gpu_policy.gpu_capability_class,
            GpuCapabilityClass::GpuPassiveDisplay
        );
        assert!(profile.syscall_allowlist.is_none());
    }

    #[test]
    fn sandbox_profile_serde_round_trip() {
        let profile = SandboxProfile {
            profile_id: ProfileId::new(),
            name: "browser-sandbox".into(),
            description: "Per-origin browser isolation".into(),
            isolation_kind: IsolationKind::BrowserOriginIsolated,
            resource_limits: ResourceLimits::default_strict(),
            gpu_policy: GpuPolicy::default_deny_all(),
            network_posture: NetworkPosture::ExplicitAllowlist,
            syscall_allowlist: Some(vec!["browser-default".into()]),
            signing_authority: "aios-root".into(),
            signature_ed25519: vec![0xAB; 64],
        };
        let json = serde_json::to_string(&profile).unwrap();
        let back: SandboxProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(profile, back);
    }
}
