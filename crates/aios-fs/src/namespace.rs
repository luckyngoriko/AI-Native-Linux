//! S4.1 `/aios` namespace classifier.

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Minimum evidence grade required before a namespace mutation can claim REAL.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EvidenceGradeFloor(String);

impl EvidenceGradeFloor {
    /// Adopt a closed evidence grade string.
    #[must_use]
    pub fn new(grade: impl Into<String>) -> Self {
        Self(grade.into())
    }

    /// Borrow the evidence grade string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// AIOS namespace path string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AiosPath(String);

impl AiosPath {
    /// Adopt a path string without full validation.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    /// Borrow the raw path string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Classify the path into the closest S4.1 namespace class.
    #[must_use]
    pub fn namespace_class(&self) -> Option<NamespaceClass> {
        let path = self.as_str().trim_end_matches('/');
        let segments = parse_segments(path)?;

        match segments.as_slice() {
            ["system"] => Some(NamespaceClass::System),
            ["system", reserved, ..] => {
                system_reserved_class(reserved).or(Some(NamespaceClass::System))
            }
            ["groups"] => Some(NamespaceClass::Groups),
            ["groups", _group_id] => Some(NamespaceClass::Group),
            ["groups", _group_id, "users"] => Some(NamespaceClass::GroupUsers),
            ["groups", _group_id, "users", _user_id] => Some(NamespaceClass::User),
            ["groups", _group_id, "users", _user_id, reserved, ..] => user_reserved_class(reserved),
            ["groups", _group_id, reserved, ..] => group_reserved_class(reserved),
            _ => None,
        }
    }
}

/// Closed namespace classes from the S4.1 reserved-name catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NamespaceClass {
    /// `/aios/system`.
    System,
    /// `/aios/system/apps`.
    SystemApps,
    /// `/aios/system/agents`.
    SystemAgents,
    /// `/aios/system/policy` — recovery-only mutation.
    SystemPolicy,
    /// `/aios/system/capabilities` — recovery-only mutation.
    SystemCapabilities,
    /// `/aios/system/evidence`.
    SystemEvidence,
    /// `/aios/system/vault` — recovery-only mutation.
    SystemVault,
    /// `/aios/system/runtime`.
    SystemRuntime,
    /// `/aios/system/recovery` — recovery-only mutation.
    SystemRecovery,
    /// `/aios/system/boot`.
    SystemBoot,
    /// `/aios/system/firstboot`.
    SystemFirstboot,
    /// `/aios/system/governance`.
    SystemGovernance,
    /// `/aios/system/identity`.
    SystemIdentity,
    /// `/aios/system/kernel`.
    SystemKernel,
    /// `/aios/system/hardware`.
    SystemHardware,
    /// `/aios/system/drivers`.
    SystemDrivers,
    /// `/aios/system/firmware`.
    SystemFirmware,
    /// `/aios/system/network`.
    SystemNetwork,
    /// `/aios/system/sgr`.
    SystemSgr,
    /// `/aios/system/units`.
    SystemUnits,
    /// `/aios/system/runbooks`.
    SystemRunbooks,
    /// `/aios/system/themes`.
    SystemThemes,
    /// `/aios/system/renderers`.
    SystemRenderers,
    /// `/aios/system/web`.
    SystemWeb,
    /// `/aios/system/distribution`.
    SystemDistribution,
    /// `/aios/groups`.
    Groups,
    /// `/aios/groups/<group_id>`.
    Group,
    /// `/aios/groups/<group_id>/apps`.
    GroupApps,
    /// `/aios/groups/<group_id>/agents`.
    GroupAgents,
    /// `/aios/groups/<group_id>/users`.
    GroupUsers,
    /// `/aios/groups/<group_id>/shared`.
    GroupShared,
    /// `/aios/groups/<group_id>/projects`.
    GroupProjects,
    /// `/aios/groups/<group_id>/datasets`.
    GroupDatasets,
    /// `/aios/groups/<group_id>/inbox`.
    GroupInbox,
    /// `/aios/groups/<group_id>/policy`.
    GroupPolicy,
    /// `/aios/groups/<group_id>/evidence`.
    GroupEvidence,
    /// `/aios/groups/<group_id>/vault`.
    GroupVault,
    /// `/aios/groups/<group_id>/audit`.
    GroupAudit,
    /// `/aios/groups/<group_id>/services`.
    GroupServices,
    /// `/aios/groups/<group_id>/system`.
    GroupSystem,
    /// `/aios/groups/<group_id>/users/<user_id>`.
    User,
    /// `/aios/groups/<group_id>/users/<user_id>/home`.
    UserHome,
    /// `/aios/groups/<group_id>/users/<user_id>/agents`.
    UserAgents,
    /// `/aios/groups/<group_id>/users/<user_id>/prefs`.
    UserPrefs,
    /// `/aios/groups/<group_id>/users/<user_id>/desktop`.
    UserDesktop,
    /// `/aios/groups/<group_id>/users/<user_id>/inbox`.
    UserInbox,
    /// `/aios/groups/<group_id>/users/<user_id>/outbox`.
    UserOutbox,
    /// `/aios/groups/<group_id>/users/<user_id>/drafts`.
    UserDrafts,
    /// `/aios/groups/<group_id>/users/<user_id>/trust`.
    UserTrust,
    /// `/aios/groups/<group_id>/users/<user_id>/apps`.
    UserApps,
    /// `/aios/groups/<group_id>/users/<user_id>/runtime`.
    UserRuntime,
    /// `/aios/groups/<group_id>/users/<user_id>/exports`.
    UserExports,
}

impl NamespaceClass {
    /// Return true for namespaces whose mutation surface is recovery-bound.
    #[must_use]
    pub const fn is_recovery_only_mutation(&self) -> bool {
        matches!(
            self,
            Self::SystemPolicy
                | Self::SystemCapabilities
                | Self::SystemVault
                | Self::SystemRecovery
                | Self::SystemBoot
                | Self::SystemFirstboot
                | Self::SystemGovernance
                | Self::SystemIdentity
                | Self::SystemKernel
                | Self::SystemFirmware
        )
    }

    /// Return true for namespaces where AI subjects are never mutation actors.
    #[must_use]
    pub const fn is_read_only_for_ai(&self) -> bool {
        matches!(
            self,
            Self::System
                | Self::SystemApps
                | Self::SystemAgents
                | Self::SystemPolicy
                | Self::SystemCapabilities
                | Self::SystemEvidence
                | Self::SystemVault
                | Self::SystemRuntime
                | Self::SystemRecovery
                | Self::SystemBoot
                | Self::SystemFirstboot
                | Self::SystemGovernance
                | Self::SystemIdentity
                | Self::SystemKernel
                | Self::SystemHardware
                | Self::SystemDrivers
                | Self::SystemFirmware
                | Self::SystemNetwork
                | Self::SystemSgr
                | Self::SystemUnits
                | Self::SystemRunbooks
                | Self::SystemThemes
                | Self::SystemRenderers
                | Self::SystemWeb
                | Self::SystemDistribution
                | Self::GroupSystem
        )
    }

    /// Minimum evidence grade for mutating this namespace class.
    #[must_use]
    pub fn evidence_grade_floor(&self) -> EvidenceGradeFloor {
        let grade = if self.is_read_only_for_ai()
            || matches!(
                self,
                Self::GroupPolicy | Self::GroupVault | Self::GroupAudit | Self::UserTrust
            ) {
            "E4"
        } else {
            "E3"
        };

        EvidenceGradeFloor::new(grade)
    }
}

fn parse_segments(path: &str) -> Option<Vec<&str>> {
    let rest = path.strip_prefix("/aios/")?;
    if rest.is_empty() {
        return None;
    }

    let segments: Vec<&str> = rest.split('/').collect();
    if segments
        .iter()
        .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        return None;
    }

    Some(segments)
}

fn system_reserved_class(segment: &str) -> Option<NamespaceClass> {
    match segment {
        "apps" => Some(NamespaceClass::SystemApps),
        "agents" => Some(NamespaceClass::SystemAgents),
        "policy" => Some(NamespaceClass::SystemPolicy),
        "capabilities" => Some(NamespaceClass::SystemCapabilities),
        "evidence" => Some(NamespaceClass::SystemEvidence),
        "vault" => Some(NamespaceClass::SystemVault),
        "runtime" => Some(NamespaceClass::SystemRuntime),
        "recovery" => Some(NamespaceClass::SystemRecovery),
        "boot" => Some(NamespaceClass::SystemBoot),
        "firstboot" => Some(NamespaceClass::SystemFirstboot),
        "governance" => Some(NamespaceClass::SystemGovernance),
        "identity" => Some(NamespaceClass::SystemIdentity),
        "kernel" => Some(NamespaceClass::SystemKernel),
        "hardware" => Some(NamespaceClass::SystemHardware),
        "drivers" => Some(NamespaceClass::SystemDrivers),
        "firmware" => Some(NamespaceClass::SystemFirmware),
        "network" => Some(NamespaceClass::SystemNetwork),
        "sgr" => Some(NamespaceClass::SystemSgr),
        "units" => Some(NamespaceClass::SystemUnits),
        "runbooks" => Some(NamespaceClass::SystemRunbooks),
        "themes" => Some(NamespaceClass::SystemThemes),
        "renderers" => Some(NamespaceClass::SystemRenderers),
        "web" => Some(NamespaceClass::SystemWeb),
        "distribution" => Some(NamespaceClass::SystemDistribution),
        _ => None,
    }
}

fn group_reserved_class(segment: &str) -> Option<NamespaceClass> {
    match segment {
        "apps" => Some(NamespaceClass::GroupApps),
        "agents" => Some(NamespaceClass::GroupAgents),
        "users" => Some(NamespaceClass::GroupUsers),
        "shared" => Some(NamespaceClass::GroupShared),
        "projects" => Some(NamespaceClass::GroupProjects),
        "datasets" => Some(NamespaceClass::GroupDatasets),
        "inbox" => Some(NamespaceClass::GroupInbox),
        "policy" => Some(NamespaceClass::GroupPolicy),
        "evidence" => Some(NamespaceClass::GroupEvidence),
        "vault" => Some(NamespaceClass::GroupVault),
        "audit" => Some(NamespaceClass::GroupAudit),
        "services" => Some(NamespaceClass::GroupServices),
        "system" => Some(NamespaceClass::GroupSystem),
        _ => None,
    }
}

fn user_reserved_class(segment: &str) -> Option<NamespaceClass> {
    match segment {
        "home" => Some(NamespaceClass::UserHome),
        "agents" => Some(NamespaceClass::UserAgents),
        "prefs" => Some(NamespaceClass::UserPrefs),
        "desktop" => Some(NamespaceClass::UserDesktop),
        "inbox" => Some(NamespaceClass::UserInbox),
        "outbox" => Some(NamespaceClass::UserOutbox),
        "drafts" => Some(NamespaceClass::UserDrafts),
        "trust" => Some(NamespaceClass::UserTrust),
        "apps" => Some(NamespaceClass::UserApps),
        "runtime" => Some(NamespaceClass::UserRuntime),
        "exports" => Some(NamespaceClass::UserExports),
        _ => None,
    }
}
