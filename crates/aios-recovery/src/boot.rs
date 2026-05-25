//! S9.2 first-boot flow typed core.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};
use ulid::Ulid;

/// First-boot session id with canonical `boot_<ULID>` wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BootId(String);

impl BootId {
    /// Canonical prefix including the trailing underscore.
    pub const PREFIX: &'static str = "boot_";

    /// Mint a fresh first-boot id.
    #[must_use]
    pub fn new() -> Self {
        Self(format!("{}{}", Self::PREFIX, Ulid::new()))
    }

    /// Borrow the canonical `boot_<ULID>` string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for BootId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for BootId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for BootId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Coarse boot phase vocabulary used by the S9 bootstrap skeleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BootPhase {
    /// Machine has started from a cold boot path.
    Cold,
    /// Bootstrap services are preparing the host.
    Bootstrap,
    /// S9.2 first-boot installer is active.
    FirstBoot,
    /// Normal AIOS boot/runtime phase.
    Normal,
    /// S9.1 recovery boot path is active.
    Recovery,
}

/// Closed S9.2 first-boot stage vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FirstBootPhase {
    /// `STAGE_INSTALLER_MEDIA_VERIFIED`.
    StageInstallerMediaVerified,
    /// `STAGE_DISK_PARTITIONED`.
    StageDiskPartitioned,
    /// `STAGE_KERNEL_INSTALLED`.
    StageKernelInstalled,
    /// `STAGE_AIOS_FS_INITIALIZED`.
    StageAiosFsInitialized,
    /// `STAGE_VAULT_ROOT_GENERATED`.
    StageVaultRootGenerated,
    /// `STAGE_INVARIANT_BUNDLE_LOADED`.
    StageInvariantBundleLoaded,
    /// `STAGE_POLICY_BUNDLE_LOADED`.
    StagePolicyBundleLoaded,
    /// `STAGE_IDENTITY_BUNDLE_LOADED`.
    StageIdentityBundleLoaded,
    /// `STAGE_RECOVERY_OPERATOR_REGISTRATION`.
    StageRecoveryOperatorRegistration,
    /// `STAGE_AI_PROVIDER_CONFIGURATION`.
    StageAiProviderConfiguration,
    /// `STAGE_FIRST_GROUP_REGISTRATION`.
    StageFirstGroupRegistration,
    /// `STAGE_FIRST_USER_REGISTRATION`.
    StageFirstUserRegistration,
    /// `STAGE_RUNTIME_SERVICES_STARTED`.
    StageRuntimeServicesStarted,
    /// `STAGE_FIRST_BOOT_COMPLETE`.
    StageFirstBootComplete,
    /// `STAGE_FAILED_REQUIRES_RECOVERY`.
    StageFailedRequiresRecovery,
}

/// First-boot execution status for a single first-boot context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FirstBootStatus {
    /// First-boot has not started.
    NotStarted,
    /// First-boot is currently running.
    InProgress,
    /// First-boot completed and wrote the marker.
    Completed,
    /// First-boot failed and requires recovery.
    Failed,
    /// First-boot was intentionally skipped by boot decision logic.
    Skipped,
}

/// Runtime context for one S9.2 first-boot session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirstBootContext {
    /// Unique first-boot session id.
    pub boot_id: BootId,
    /// UTC timestamp when first-boot started.
    pub started_at: DateTime<Utc>,
    /// UTC timestamp when first-boot completed or failed.
    pub completed_at: Option<DateTime<Utc>>,
    /// Current first-boot status.
    pub status: FirstBootStatus,
    /// First-boot stages already performed.
    pub performed_phases: Vec<FirstBootPhase>,
}
