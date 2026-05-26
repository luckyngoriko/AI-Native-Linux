use std::collections::HashSet;
use tokio::sync::RwLock;

use crate::composer::{ComposeRequest, ComposeResult, SandboxComposer};
use crate::{
    GpuCapabilityClass, GpuPolicy, IsolationKind, NetworkPosture, ProfileId, ResourceLimits,
    SandboxError, SandboxProfile,
};

/// Merge-order source names (S3.2 §5.1 + §18.1).
const SOURCE_ADAPTER_DEFAULT: &str = "adapter_default";
const SOURCE_APP_MANIFEST: &str = "app_manifest";
const SOURCE_USER_REQUEST: &str = "user_request";
const SOURCE_POLICY_REQUIRED: &str = "policy_required";
const SOURCE_GROUP_FLOOR: &str = "group_floor";
const SOURCE_RUNTIME_SAFETY_FLOOR: &str = "runtime_safety_floor";
const SOURCE_COMPOSER_DEFAULT: &str = "composer_default";
const SOURCE_BASE_PROFILE: &str = "base_profile";

// ---------------------------------------------------------------------------
// InMemorySandboxComposer
// ---------------------------------------------------------------------------

/// In-memory implementation of [`SandboxComposer`].
///
/// Holds a profile catalog behind a `RwLock` and executes the 6-source merge
/// algorithm inline. Supports pre-loading canonical fixture profiles via
/// [`with_fixtures`](Self::with_fixtures).
pub struct InMemorySandboxComposer {
    profiles: RwLock<std::collections::HashMap<ProfileId, SandboxProfile>>,
}

impl InMemorySandboxComposer {
    /// Create an empty composer with no stored profiles.
    #[must_use]
    pub fn new() -> Self {
        Self {
            profiles: RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Create a composer pre-loaded with canonical fixture profiles.
    ///
    /// Fixtures provided:
    /// - `restrictive_ai` — AI agent sandbox (`DenyAll`, `GpuPassiveDisplay`)
    /// - `balanced_human` — human operator (`HostLimited`, `GpuRich2D`)
    /// - `service_class` — system service (`ExplicitAllowlist`, `GpuFull3D`)
    /// - `recovery_only` — recovery-mode floor (`LoopbackOnly`, `ProcessContainer`)
    #[must_use]
    pub fn with_fixtures() -> Self {
        let mut profiles = std::collections::HashMap::new();

        let fi_ai = SandboxProfile {
            profile_id: ProfileId::new(),
            name: "restrictive_ai".into(),
            description: "AI agent sandbox — maximum restrictions".into(),
            isolation_kind: IsolationKind::NamespaceLocal,
            resource_limits: ResourceLimits::default_strict(),
            gpu_policy: GpuPolicy::default_deny_all(),
            network_posture: NetworkPosture::DenyAll,
            syscall_allowlist: Some(vec!["aios-ai-basic".into()]),
            signing_authority: "aios-root".into(),
            signature_ed25519: Vec::new(),
        };
        profiles.insert(fi_ai.profile_id.clone(), fi_ai);

        let fi_human = SandboxProfile {
            profile_id: ProfileId::new(),
            name: "balanced_human".into(),
            description: "Human operator sandbox — moderate access".into(),
            isolation_kind: IsolationKind::ProcessContainer,
            resource_limits: ResourceLimits {
                cpu_quota_percent: 75,
                memory_max_bytes: 512 * 1024 * 1024,
                io_max_bytes_per_sec: Some(10 * 1024 * 1024),
                network_max_bytes_per_sec: Some(10 * 1024 * 1024),
                process_max_count: Some(64),
                file_descriptor_max: Some(256),
                expires_at: None,
            },
            gpu_policy: GpuPolicy {
                gpu_capability_class: GpuCapabilityClass::GpuRich2d,
                vk_device_required: false,
                dmabuf_passthrough_allowed: true,
                per_group_partitioning: false,
                iommu_required: false,
                expires_at: None,
            },
            network_posture: NetworkPosture::HostLimited,
            syscall_allowlist: Some(vec!["aios-human-basic".into(), "aios-human-net".into()]),
            signing_authority: "aios-root".into(),
            signature_ed25519: Vec::new(),
        };
        profiles.insert(fi_human.profile_id.clone(), fi_human);

        let fi_svc = SandboxProfile {
            profile_id: ProfileId::new(),
            name: "service_class".into(),
            description: "System service sandbox — permissive within policy".into(),
            isolation_kind: IsolationKind::VmGuest,
            resource_limits: ResourceLimits::default_permissive(),
            gpu_policy: GpuPolicy {
                gpu_capability_class: GpuCapabilityClass::GpuFull3d,
                vk_device_required: true,
                dmabuf_passthrough_allowed: true,
                per_group_partitioning: true,
                iommu_required: true,
                expires_at: None,
            },
            network_posture: NetworkPosture::ExplicitAllowlist,
            syscall_allowlist: Some(vec![
                "aios-service".into(),
                "aios-service-net".into(),
                "aios-service-gpu".into(),
            ]),
            signing_authority: "aios-root".into(),
            signature_ed25519: Vec::new(),
        };
        profiles.insert(fi_svc.profile_id.clone(), fi_svc);

        let fi_rec = SandboxProfile {
            profile_id: ProfileId::new(),
            name: "recovery_only".into(),
            description: "Recovery-mode minimum floor".into(),
            isolation_kind: IsolationKind::ProcessContainer,
            resource_limits: ResourceLimits {
                cpu_quota_percent: 25,
                memory_max_bytes: 128 * 1024 * 1024,
                io_max_bytes_per_sec: Some(512 * 1024),
                network_max_bytes_per_sec: Some(512 * 1024),
                process_max_count: Some(8),
                file_descriptor_max: Some(32),
                expires_at: None,
            },
            gpu_policy: GpuPolicy::default_deny_all(),
            network_posture: NetworkPosture::LoopbackOnly,
            syscall_allowlist: Some(vec!["aios-recovery".into()]),
            signing_authority: "aios-root".into(),
            signature_ed25519: Vec::new(),
        };
        profiles.insert(fi_rec.profile_id.clone(), fi_rec);

        Self {
            profiles: RwLock::new(profiles),
        }
    }

    // ------------------------------------------------------------------
    // Merge helpers
    // ------------------------------------------------------------------

    /// Assign a numeric strictness rank to each isolation kind.
    ///
    /// Higher = more restrictive. `NoIsolation` is ranked 0 and expected to be
    /// rejected by the runtime safety floor before reaching production.
    const fn isolation_strictness(k: IsolationKind) -> u8 {
        match k {
            IsolationKind::NoIsolation => 0,
            IsolationKind::NamespaceLocal => 1,
            IsolationKind::ProcessContainer => 2,
            IsolationKind::VmGuest => 3,
            IsolationKind::BrowserOriginIsolated => 4,
        }
    }

    /// Merge two `Option<u64>` fields — `None` means uncapped (infinity);
    /// `Some(x)` means capped at x. The smaller cap (more restrictive) wins.
    fn merge_option_u64_min(a: Option<u64>, b: Option<u64>) -> Option<u64> {
        match (a, b) {
            (None, None) => None,
            (Some(x), None) => Some(x),
            (None, Some(y)) => Some(y),
            (Some(x), Some(y)) => Some(x.min(y)),
        }
    }

    /// Merge two `Option<u32>` fields — same semantics as [`merge_option_u64_min`].
    fn merge_option_u32_min(a: Option<u32>, b: Option<u32>) -> Option<u32> {
        match (a, b) {
            (None, None) => None,
            (Some(x), None) => Some(x),
            (None, Some(y)) => Some(y),
            (Some(x), Some(y)) => Some(x.min(y)),
        }
    }

    /// Merge two `ResourceLimits` taking the most restrictive value per field.
    fn merge_resource_limits(a: &ResourceLimits, b: &ResourceLimits) -> ResourceLimits {
        ResourceLimits {
            cpu_quota_percent: a.cpu_quota_percent.min(b.cpu_quota_percent),
            memory_max_bytes: a.memory_max_bytes.min(b.memory_max_bytes),
            io_max_bytes_per_sec: Self::merge_option_u64_min(
                a.io_max_bytes_per_sec,
                b.io_max_bytes_per_sec,
            ),
            network_max_bytes_per_sec: Self::merge_option_u64_min(
                a.network_max_bytes_per_sec,
                b.network_max_bytes_per_sec,
            ),
            process_max_count: Self::merge_option_u32_min(a.process_max_count, b.process_max_count),
            file_descriptor_max: Self::merge_option_u32_min(
                a.file_descriptor_max,
                b.file_descriptor_max,
            ),
            expires_at: match (a.expires_at, b.expires_at) {
                (None, None) => None,
                (Some(t), None) | (None, Some(t)) => Some(t),
                (Some(t1), Some(t2)) => Some(t1.min(t2)),
            },
        }
    }

    /// Merge two `GpuPolicy` structs taking the most restrictive value per field.
    ///
    /// Boolean merge semantics (most-restrictive-wins):
    /// - `vk_device_required` — OR (any requiring it tightens)
    /// - `dmabuf_passthrough_allowed` — AND (any forbidding it tightens)
    /// - `per_group_partitioning` — OR (any requiring it tightens)
    /// - `iommu_required` — OR (any requiring it tightens)
    fn merge_gpu_policy(a: &GpuPolicy, b: &GpuPolicy) -> GpuPolicy {
        GpuPolicy {
            gpu_capability_class: a.gpu_capability_class.min(b.gpu_capability_class),
            vk_device_required: a.vk_device_required || b.vk_device_required,
            dmabuf_passthrough_allowed: a.dmabuf_passthrough_allowed
                && b.dmabuf_passthrough_allowed,
            per_group_partitioning: a.per_group_partitioning || b.per_group_partitioning,
            iommu_required: a.iommu_required || b.iommu_required,
            expires_at: match (a.expires_at, b.expires_at) {
                (None, None) => None,
                (Some(t), None) | (None, Some(t)) => Some(t),
                (Some(t1), Some(t2)) => Some(t1.min(t2)),
            },
        }
    }

    /// Merge two `Option<Vec<String>>` syscall allow-lists by set intersection.
    fn merge_syscall_allowlist(
        a: Option<&Vec<String>>,
        b: Option<&Vec<String>>,
    ) -> Option<Vec<String>> {
        match (a, b) {
            (None, None) => None,
            (Some(list), None) | (None, Some(list)) => Some(list.clone()),
            (Some(list_a), Some(list_b)) => {
                let set_b: HashSet<&String> = list_b.iter().collect();
                let mut intersection: Vec<String> = list_a
                    .iter()
                    .filter(|s| set_b.contains(s))
                    .cloned()
                    .collect();
                intersection.sort();
                Some(intersection)
            }
        }
    }

    /// Merge two `SandboxProfile` values per-field, taking the most restrictive
    /// value for each field.
    fn merge_profiles(base: &SandboxProfile, other: &SandboxProfile) -> SandboxProfile {
        let isolation_a = Self::isolation_strictness(base.isolation_kind);
        let isolation_b = Self::isolation_strictness(other.isolation_kind);
        let merged_isolation = if isolation_a >= isolation_b {
            base.isolation_kind
        } else {
            other.isolation_kind
        };

        SandboxProfile {
            profile_id: base.profile_id.clone(),
            name: if other.name.is_empty() {
                base.name.clone()
            } else {
                other.name.clone()
            },
            description: if other.description.is_empty() {
                base.description.clone()
            } else {
                other.description.clone()
            },
            isolation_kind: merged_isolation,
            resource_limits: Self::merge_resource_limits(
                &base.resource_limits,
                &other.resource_limits,
            ),
            gpu_policy: Self::merge_gpu_policy(&base.gpu_policy, &other.gpu_policy),
            network_posture: base.network_posture.min(other.network_posture),
            syscall_allowlist: Self::merge_syscall_allowlist(
                base.syscall_allowlist.as_ref(),
                other.syscall_allowlist.as_ref(),
            ),
            signing_authority: base.signing_authority.clone(),
            signature_ed25519: Vec::new(),
        }
    }

    // ------------------------------------------------------------------
    // Post-processing rules (S3.2 §5.4)
    // ------------------------------------------------------------------

    /// Apply recovery-mode post-processing rules (S3.2 §5.4).
    ///
    /// Forces `network_posture` to at least `LoopbackOnly` and `isolation_kind`
    /// to at least `ProcessContainer`. Returns `true` if any field was tightened.
    fn apply_recovery_rules(profile: &mut SandboxProfile) -> bool {
        // Force network to LoopbackOnly or stricter
        let net_enforced = if profile.network_posture > NetworkPosture::LoopbackOnly {
            profile.network_posture = NetworkPosture::LoopbackOnly;
            true
        } else {
            false
        };

        // Force isolation to ProcessContainer or stricter
        let iso_enforced = if Self::isolation_strictness(profile.isolation_kind)
            < Self::isolation_strictness(IsolationKind::ProcessContainer)
        {
            profile.isolation_kind = IsolationKind::ProcessContainer;
            true
        } else {
            false
        };

        net_enforced || iso_enforced
    }

    /// Apply AI-mode post-processing rules (S3.2 §5.4).
    ///
    /// Forces `network_posture` to `LoopbackOnly` or stricter and
    /// `gpu_capability_class` to at most `GpuBasic2d`. Returns `true` if any
    /// field was tightened.
    fn apply_ai_rules(profile: &mut SandboxProfile) -> bool {
        // Force network to LoopbackOnly or stricter
        let net_enforced = if profile.network_posture > NetworkPosture::LoopbackOnly {
            profile.network_posture = NetworkPosture::LoopbackOnly;
            true
        } else {
            false
        };

        // Force GPU to GpuBasic2d or stricter (max = GpuBasic2d)
        let gpu_enforced =
            if profile.gpu_policy.gpu_capability_class > GpuCapabilityClass::GpuBasic2d {
                profile.gpu_policy.gpu_capability_class = GpuCapabilityClass::GpuBasic2d;
                true
            } else {
                false
            };

        net_enforced || gpu_enforced
    }
}

impl Default for InMemorySandboxComposer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SandboxComposer impl
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl SandboxComposer for InMemorySandboxComposer {
    async fn compose(&self, request: ComposeRequest) -> Result<ComposeResult, SandboxError> {
        let mut merged_sources: Vec<String> = Vec::new();

        // Determine the starting profile
        let mut merged = if let Some(ref base_id) = request.base_profile_id {
            let base = self.get_profile(base_id).await?;
            merged_sources.push(format!("{SOURCE_BASE_PROFILE}:{base_id}"));
            base
        } else if let Some(ref adapter_default) = request.adapter_default {
            merged_sources.push(SOURCE_ADAPTER_DEFAULT.to_string());
            adapter_default.clone()
        } else {
            merged_sources.push(SOURCE_COMPOSER_DEFAULT.to_string());
            SandboxProfile::new_strict("composer-default", "Default strict composer profile")
        };

        // Merge source 2: app_manifest
        if let Some(ref app) = request.app_manifest {
            merged = Self::merge_profiles(&merged, app);
            merged_sources.push(SOURCE_APP_MANIFEST.to_string());
        }

        // Merge source 3: user_request
        if let Some(ref user) = request.user_request {
            merged = Self::merge_profiles(&merged, user);
            merged_sources.push(SOURCE_USER_REQUEST.to_string());
        }

        // Merge source 4: policy_required
        if let Some(ref policy) = request.policy_required {
            merged = Self::merge_profiles(&merged, policy);
            merged_sources.push(SOURCE_POLICY_REQUIRED.to_string());
        }

        // Merge source 5: group_floor
        if let Some(ref group) = request.group_floor {
            merged = Self::merge_profiles(&merged, group);
            merged_sources.push(SOURCE_GROUP_FLOOR.to_string());
        }

        // Merge source 6: runtime_safety_floor (unconditional floor)
        if let Some(ref safety) = request.runtime_safety_floor {
            merged = Self::merge_profiles(&merged, safety);
            merged_sources.push(SOURCE_RUNTIME_SAFETY_FLOOR.to_string());
        }

        // Post-processing: recovery mode
        let recovery_mode_enforced = if request.recovery_mode {
            Self::apply_recovery_rules(&mut merged)
        } else {
            false
        };

        // Post-processing: AI mode
        let ai_mode_enforced = if request.is_ai {
            Self::apply_ai_rules(&mut merged)
        } else {
            false
        };

        // The merged profile gets a fresh identity
        merged.profile_id = ProfileId::new();

        Ok(ComposeResult {
            profile: merged,
            merged_sources,
            recovery_mode_enforced,
            ai_mode_enforced,
        })
    }

    async fn store_profile(&self, profile: SandboxProfile) -> Result<ProfileId, SandboxError> {
        let id = profile.profile_id.clone();
        self.profiles.write().await.insert(id.clone(), profile);
        Ok(id)
    }

    async fn get_profile(&self, profile_id: &ProfileId) -> Result<SandboxProfile, SandboxError> {
        let profiles = self.profiles.read().await;
        profiles
            .get(profile_id)
            .cloned()
            .ok_or_else(|| SandboxError::ProfileNotFound(profile_id.clone()))
    }

    async fn list_profiles(&self) -> Result<Vec<SandboxProfile>, SandboxError> {
        let profiles = self.profiles.read().await;
        Ok(profiles.values().cloned().collect())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use crate::composer::SubjectRef;

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
            description: format!("Test profile: {name}"),
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

    #[test]
    fn merge_network_posture_takes_strictest() {
        let a = make_profile(
            "a",
            50,
            256,
            NetworkPosture::Full,
            GpuCapabilityClass::GpuFull3d,
            IsolationKind::NamespaceLocal,
        );
        let b = make_profile(
            "b",
            25,
            128,
            NetworkPosture::DenyAll,
            GpuCapabilityClass::GpuPassiveDisplay,
            IsolationKind::ProcessContainer,
        );
        let merged = InMemorySandboxComposer::merge_profiles(&a, &b);
        assert_eq!(merged.network_posture, NetworkPosture::DenyAll);
    }

    #[test]
    fn merge_resource_limits_takes_min_cpu() {
        let a = make_profile(
            "a",
            80,
            512,
            NetworkPosture::Full,
            GpuCapabilityClass::GpuFull3d,
            IsolationKind::NamespaceLocal,
        );
        let b = make_profile(
            "b",
            20,
            1024,
            NetworkPosture::Full,
            GpuCapabilityClass::GpuFull3d,
            IsolationKind::NamespaceLocal,
        );
        let merged = InMemorySandboxComposer::merge_profiles(&a, &b);
        assert_eq!(merged.resource_limits.cpu_quota_percent, 20);
        assert_eq!(merged.resource_limits.memory_max_bytes, 512 * 1024 * 1024);
    }

    #[test]
    fn merge_gpu_capability_takes_strictest() {
        let a = make_profile(
            "a",
            50,
            256,
            NetworkPosture::Full,
            GpuCapabilityClass::GpuComputeHeavy,
            IsolationKind::NamespaceLocal,
        );
        let b = make_profile(
            "b",
            50,
            256,
            NetworkPosture::Full,
            GpuCapabilityClass::GpuPassiveDisplay,
            IsolationKind::NamespaceLocal,
        );
        let merged = InMemorySandboxComposer::merge_profiles(&a, &b);
        assert_eq!(
            merged.gpu_policy.gpu_capability_class,
            GpuCapabilityClass::GpuPassiveDisplay
        );
    }

    #[test]
    fn merge_gpu_bools_most_restrictive() {
        let a = GpuPolicy {
            gpu_capability_class: GpuCapabilityClass::GpuFull3d,
            vk_device_required: false,
            dmabuf_passthrough_allowed: true,
            per_group_partitioning: false,
            iommu_required: false,
            expires_at: None,
        };
        let b = GpuPolicy {
            gpu_capability_class: GpuCapabilityClass::GpuRich2d,
            vk_device_required: true,
            dmabuf_passthrough_allowed: false,
            per_group_partitioning: true,
            iommu_required: true,
            expires_at: None,
        };
        let merged = InMemorySandboxComposer::merge_gpu_policy(&a, &b);
        assert!(merged.vk_device_required);
        assert!(!merged.dmabuf_passthrough_allowed);
        assert!(merged.per_group_partitioning);
        assert!(merged.iommu_required);
    }

    #[test]
    fn merge_syscall_allowlist_intersection() {
        let a = Some(vec!["read".into(), "write".into(), "open".into()]);
        let b = Some(vec!["write".into(), "open".into(), "close".into()]);
        let result = InMemorySandboxComposer::merge_syscall_allowlist(a.as_ref(), b.as_ref());
        assert_eq!(result, Some(vec!["open".into(), "write".into()]));
    }

    #[test]
    fn merge_syscall_allowlist_none_preserves_other() {
        let a = Some(vec!["read".into(), "write".into()]);
        let result = InMemorySandboxComposer::merge_syscall_allowlist(a.as_ref(), None);
        assert_eq!(result, a);
    }

    #[test]
    fn merge_isolation_takes_strictest() {
        let a = make_profile(
            "a",
            50,
            256,
            NetworkPosture::Full,
            GpuCapabilityClass::GpuFull3d,
            IsolationKind::NamespaceLocal,
        );
        let b = make_profile(
            "b",
            50,
            256,
            NetworkPosture::Full,
            GpuCapabilityClass::GpuFull3d,
            IsolationKind::VmGuest,
        );
        let merged = InMemorySandboxComposer::merge_profiles(&a, &b);
        assert_eq!(merged.isolation_kind, IsolationKind::VmGuest);
    }

    #[test]
    fn merge_option_u64_none_is_uncapped() {
        assert_eq!(
            InMemorySandboxComposer::merge_option_u64_min(None, None),
            None
        );
        assert_eq!(
            InMemorySandboxComposer::merge_option_u64_min(Some(100), None),
            Some(100)
        );
        assert_eq!(
            InMemorySandboxComposer::merge_option_u64_min(None, Some(200)),
            Some(200)
        );
        assert_eq!(
            InMemorySandboxComposer::merge_option_u64_min(Some(100), Some(200)),
            Some(100)
        );
    }

    #[test]
    fn recovery_rules_force_network_loopback() {
        let mut profile = make_profile(
            "test",
            50,
            256,
            NetworkPosture::Full,
            GpuCapabilityClass::GpuFull3d,
            IsolationKind::ProcessContainer,
        );
        let enforced = InMemorySandboxComposer::apply_recovery_rules(&mut profile);
        assert!(enforced);
        assert_eq!(profile.network_posture, NetworkPosture::LoopbackOnly);
    }

    #[test]
    fn recovery_rules_force_isolation_process_container() {
        let mut profile = make_profile(
            "test",
            50,
            256,
            NetworkPosture::LoopbackOnly,
            GpuCapabilityClass::GpuPassiveDisplay,
            IsolationKind::NamespaceLocal,
        );
        let enforced = InMemorySandboxComposer::apply_recovery_rules(&mut profile);
        assert!(enforced);
        assert_eq!(profile.isolation_kind, IsolationKind::ProcessContainer);
    }

    #[test]
    fn recovery_rules_noop_when_already_strict() {
        let mut profile = make_profile(
            "test",
            50,
            256,
            NetworkPosture::DenyAll,
            GpuCapabilityClass::GpuPassiveDisplay,
            IsolationKind::VmGuest,
        );
        let enforced = InMemorySandboxComposer::apply_recovery_rules(&mut profile);
        assert!(!enforced);
        assert_eq!(profile.network_posture, NetworkPosture::DenyAll);
        assert_eq!(profile.isolation_kind, IsolationKind::VmGuest);
    }

    #[test]
    fn ai_rules_force_network_loopback() {
        let mut profile = make_profile(
            "test",
            50,
            256,
            NetworkPosture::HostLimited,
            GpuCapabilityClass::GpuPassiveDisplay,
            IsolationKind::NamespaceLocal,
        );
        let enforced = InMemorySandboxComposer::apply_ai_rules(&mut profile);
        assert!(enforced);
        assert_eq!(profile.network_posture, NetworkPosture::LoopbackOnly);
    }

    #[test]
    fn ai_rules_force_gpu_basic2d_max() {
        let mut profile = make_profile(
            "test",
            50,
            256,
            NetworkPosture::LoopbackOnly,
            GpuCapabilityClass::GpuComputeHeavy,
            IsolationKind::NamespaceLocal,
        );
        let enforced = InMemorySandboxComposer::apply_ai_rules(&mut profile);
        assert!(enforced);
        assert_eq!(
            profile.gpu_policy.gpu_capability_class,
            GpuCapabilityClass::GpuBasic2d
        );
    }

    #[test]
    fn ai_rules_noop_when_already_strict() {
        let mut profile = make_profile(
            "test",
            50,
            256,
            NetworkPosture::DenyAll,
            GpuCapabilityClass::GpuPassiveDisplay,
            IsolationKind::NamespaceLocal,
        );
        let enforced = InMemorySandboxComposer::apply_ai_rules(&mut profile);
        assert!(!enforced);
        assert_eq!(profile.network_posture, NetworkPosture::DenyAll);
        assert_eq!(
            profile.gpu_policy.gpu_capability_class,
            GpuCapabilityClass::GpuPassiveDisplay
        );
    }

    #[test]
    fn store_and_get_profile_roundtrip() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let composer = InMemorySandboxComposer::new();
            let profile = make_profile(
                "stored",
                30,
                128,
                NetworkPosture::LoopbackOnly,
                GpuCapabilityClass::GpuBasic2d,
                IsolationKind::ProcessContainer,
            );
            let id = composer.store_profile(profile.clone()).await.unwrap();
            let retrieved = composer.get_profile(&id).await.unwrap();
            assert_eq!(retrieved.name, "stored");
            assert_eq!(retrieved.network_posture, NetworkPosture::LoopbackOnly);
        });
    }

    #[test]
    fn get_profile_not_found() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let composer = InMemorySandboxComposer::new();
            let fake_id = ProfileId::new();
            let err = composer.get_profile(&fake_id).await.unwrap_err();
            let msg = format!("{err}");
            assert!(msg.contains("profile not found"), "got: {msg}");
        });
    }

    #[test]
    fn list_profiles_returns_all_stored() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let composer = InMemorySandboxComposer::with_fixtures();
            let profiles = composer.list_profiles().await.unwrap();
            assert_eq!(profiles.len(), 4);
        });
    }

    #[test]
    fn compose_empty_request_returns_composer_default() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let composer = InMemorySandboxComposer::new();
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
            assert!(result
                .merged_sources
                .contains(&SOURCE_COMPOSER_DEFAULT.to_string()));
        });
    }

    #[test]
    fn compose_adapter_default_is_starting_point() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let composer = InMemorySandboxComposer::new();
            let adapter = make_profile(
                "adapter",
                60,
                256,
                NetworkPosture::HostLimited,
                GpuCapabilityClass::GpuRich2d,
                IsolationKind::ProcessContainer,
            );
            let req = ComposeRequest {
                subject: SubjectRef::new("test"),
                action_kind: "test-action".into(),
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
            assert_eq!(result.profile.network_posture, NetworkPosture::HostLimited);
            assert_eq!(
                result.profile.isolation_kind,
                IsolationKind::ProcessContainer
            );
            assert!(result
                .merged_sources
                .contains(&SOURCE_ADAPTER_DEFAULT.to_string()));
        });
    }

    #[test]
    fn compose_app_manifest_tightens_adapter() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let composer = InMemorySandboxComposer::new();
            let adapter = make_profile(
                "adapter",
                60,
                512,
                NetworkPosture::HostLimited,
                GpuCapabilityClass::GpuRich2d,
                IsolationKind::ProcessContainer,
            );
            let app = make_profile(
                "app",
                30,
                128,
                NetworkPosture::DenyAll,
                GpuCapabilityClass::GpuPassiveDisplay,
                IsolationKind::VmGuest,
            );
            let req = ComposeRequest {
                subject: SubjectRef::new("test"),
                action_kind: "test-action".into(),
                base_profile_id: None,
                adapter_default: Some(adapter),
                app_manifest: Some(app),
                user_request: None,
                policy_required: None,
                group_floor: None,
                runtime_safety_floor: None,
                recovery_mode: false,
                is_ai: false,
            };
            let result = composer.compose(req).await.unwrap();
            // Network tightened from HostLimited to DenyAll
            assert_eq!(result.profile.network_posture, NetworkPosture::DenyAll);
            // GPU tightened from Rich2d to PassiveDisplay
            assert_eq!(
                result.profile.gpu_policy.gpu_capability_class,
                GpuCapabilityClass::GpuPassiveDisplay
            );
            // Isolation tightened from ProcessContainer to VmGuest
            assert_eq!(result.profile.isolation_kind, IsolationKind::VmGuest);
            // CPU tightened from 60 to 30
            assert_eq!(result.profile.resource_limits.cpu_quota_percent, 30);
            // Sources track both adapter_default and app_manifest
            assert!(result
                .merged_sources
                .contains(&SOURCE_ADAPTER_DEFAULT.to_string()));
            assert!(result
                .merged_sources
                .contains(&SOURCE_APP_MANIFEST.to_string()));
        });
    }

    #[test]
    fn compose_runtime_safety_floor_wins_unconditionally() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let composer = InMemorySandboxComposer::new();
            let adapter = make_profile(
                "adapter",
                80,
                1024,
                NetworkPosture::Full,
                GpuCapabilityClass::GpuComputeHeavy,
                IsolationKind::NamespaceLocal,
            );
            let safety = make_profile(
                "safety",
                10,
                32,
                NetworkPosture::DenyAll,
                GpuCapabilityClass::GpuPassiveDisplay,
                IsolationKind::VmGuest,
            );
            let req = ComposeRequest {
                subject: SubjectRef::new("test"),
                action_kind: "test-action".into(),
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
            assert_eq!(result.profile.resource_limits.cpu_quota_percent, 10);
            assert_eq!(result.profile.isolation_kind, IsolationKind::VmGuest);
            assert!(result
                .merged_sources
                .contains(&SOURCE_RUNTIME_SAFETY_FLOOR.to_string()));
        });
    }

    #[test]
    fn compose_recovery_mode_enforces_rules() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let composer = InMemorySandboxComposer::new();
            let adapter = make_profile(
                "adapter",
                60,
                256,
                NetworkPosture::Full,
                GpuCapabilityClass::GpuFull3d,
                IsolationKind::NamespaceLocal,
            );
            let req = ComposeRequest {
                subject: SubjectRef::new("test"),
                action_kind: "test-action".into(),
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
            assert_eq!(result.profile.network_posture, NetworkPosture::LoopbackOnly);
            assert_eq!(
                result.profile.isolation_kind,
                IsolationKind::ProcessContainer
            );
        });
    }

    #[test]
    fn compose_ai_mode_enforces_rules() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let composer = InMemorySandboxComposer::new();
            let adapter = make_profile(
                "adapter",
                60,
                256,
                NetworkPosture::ExplicitAllowlist,
                GpuCapabilityClass::GpuComputeHeavy,
                IsolationKind::NamespaceLocal,
            );
            let req = ComposeRequest {
                subject: SubjectRef::new("ai-agent"),
                action_kind: "ai-action".into(),
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
            assert_eq!(result.profile.network_posture, NetworkPosture::LoopbackOnly);
            assert_eq!(
                result.profile.gpu_policy.gpu_capability_class,
                GpuCapabilityClass::GpuBasic2d
            );
        });
    }

    #[test]
    fn compose_with_base_profile_id_from_catalog() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let composer = InMemorySandboxComposer::with_fixtures();
            let profiles = composer.list_profiles().await.unwrap();
            // Find the service_class profile
            let svc = profiles
                .iter()
                .find(|p| p.name == "service_class")
                .expect("service_class fixture missing");
            let svc_id = svc.profile_id.clone();

            let req = ComposeRequest {
                subject: SubjectRef::new("test"),
                action_kind: "test-action".into(),
                base_profile_id: Some(svc_id),
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
            assert_eq!(
                result.profile.network_posture,
                NetworkPosture::ExplicitAllowlist
            );
            assert!(result
                .merged_sources
                .iter()
                .any(|s| s.starts_with(SOURCE_BASE_PROFILE)));
        });
    }

    #[test]
    fn compose_subject_ref_display() {
        let subject = SubjectRef::new("human:lucky");
        assert_eq!(format!("{subject}"), "human:lucky");
    }

    #[test]
    fn compose_merged_sources_tracks_every_contributor() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let composer = InMemorySandboxComposer::new();
            let adapter = make_profile(
                "adapter",
                50,
                256,
                NetworkPosture::HostLimited,
                GpuCapabilityClass::GpuRich2d,
                IsolationKind::ProcessContainer,
            );
            let app = make_profile(
                "app",
                30,
                128,
                NetworkPosture::DenyAll,
                GpuCapabilityClass::GpuPassiveDisplay,
                IsolationKind::ProcessContainer,
            );
            let user = make_profile(
                "user",
                20,
                64,
                NetworkPosture::DenyAll,
                GpuCapabilityClass::GpuPassiveDisplay,
                IsolationKind::ProcessContainer,
            );
            let safety = make_profile(
                "safety",
                5,
                16,
                NetworkPosture::DenyAll,
                GpuCapabilityClass::GpuPassiveDisplay,
                IsolationKind::VmGuest,
            );
            let req = ComposeRequest {
                subject: SubjectRef::new("test"),
                action_kind: "test-action".into(),
                base_profile_id: None,
                adapter_default: Some(adapter),
                app_manifest: Some(app),
                user_request: Some(user),
                policy_required: None,
                group_floor: None,
                runtime_safety_floor: Some(safety),
                recovery_mode: false,
                is_ai: false,
            };
            let result = composer.compose(req).await.unwrap();
            assert!(result
                .merged_sources
                .contains(&SOURCE_ADAPTER_DEFAULT.to_string()));
            assert!(result
                .merged_sources
                .contains(&SOURCE_APP_MANIFEST.to_string()));
            assert!(result
                .merged_sources
                .contains(&SOURCE_USER_REQUEST.to_string()));
            assert!(result
                .merged_sources
                .contains(&SOURCE_RUNTIME_SAFETY_FLOOR.to_string()));
            // Final profile has the tightest values from the safety floor
            assert_eq!(result.profile.resource_limits.cpu_quota_percent, 5);
        });
    }

    #[test]
    fn with_fixtures_has_four_profiles() {
        let composer = InMemorySandboxComposer::with_fixtures();
        let profiles = composer.profiles.blocking_read();
        let count = profiles.len();
        let names: Vec<String> = profiles.values().map(|p| p.name.clone()).collect();
        drop(profiles);
        assert_eq!(count, 4);
        assert!(names.iter().any(|n| n == "restrictive_ai"));
        assert!(names.iter().any(|n| n == "balanced_human"));
        assert!(names.iter().any(|n| n == "service_class"));
        assert!(names.iter().any(|n| n == "recovery_only"));
    }

    #[test]
    fn isolation_strictness_ordering() {
        assert!(
            InMemorySandboxComposer::isolation_strictness(IsolationKind::NoIsolation)
                < InMemorySandboxComposer::isolation_strictness(IsolationKind::NamespaceLocal)
        );
        assert!(
            InMemorySandboxComposer::isolation_strictness(IsolationKind::NamespaceLocal)
                < InMemorySandboxComposer::isolation_strictness(IsolationKind::ProcessContainer)
        );
        assert!(
            InMemorySandboxComposer::isolation_strictness(IsolationKind::ProcessContainer)
                < InMemorySandboxComposer::isolation_strictness(IsolationKind::VmGuest)
        );
        assert!(
            InMemorySandboxComposer::isolation_strictness(IsolationKind::VmGuest)
                < InMemorySandboxComposer::isolation_strictness(
                    IsolationKind::BrowserOriginIsolated
                )
        );
    }
}
