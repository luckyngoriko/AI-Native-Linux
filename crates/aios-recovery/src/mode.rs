//! S9.1 recovery mode and active-state vocabulary.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Closed S9.1 boot-time mode classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecoveryMode {
    /// `NORMAL` - full AIOS stack running.
    Normal,
    /// `RECOVERY` - recovery boot path active with degraded L4 only.
    Recovery,
    /// `DEGRADED` - abnormal normal-mode, not recovery privileges.
    Degraded,
    /// `FIRST_BOOT` - first-boot installer active.
    FirstBoot,
}

/// Current recovery-related state observed by the boot/recovery layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryState {
    /// S9.1 closed mode for the host.
    pub mode: RecoveryMode,
    /// UTC timestamp when recovery mode was entered, if active.
    pub entered_at: Option<DateTime<Utc>>,
    /// UTC timestamp at which an exit/reboot is planned.
    pub exit_planned_at: Option<DateTime<Utc>>,
    /// Human-readable entry reason or diagnostic detail.
    pub reason: Option<String>,
    /// Optional S5.4 override binding id authorising this recovery context.
    pub operator_grant: Option<String>,
}
