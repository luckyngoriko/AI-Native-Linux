//! Emergency override class and binding records (S5.4).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

use aios_action::ActionId;

use crate::identity::SubjectRef;

/// S5.4 override strength vocabulary exposed as the T-046 override class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OverrideClass {
    /// `STRONG_SOLO` — recovery-boot solo path only.
    StrongSolo,
    /// `DUAL_HUMAN` — default non-recovery two-human quorum.
    DualHuman,
    /// `TRIPLE_HUMAN` — deepest non-constitutional hard-deny quorum.
    TripleHuman,
}

/// S5.4 override binding lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OverrideBindingState {
    /// Binding has been granted and is live.
    Granted,
    /// Binding has been consumed by the bound action.
    Consumed,
    /// Binding was revoked before consumption.
    Revoked,
    /// Binding expired before consumption.
    Expired,
}

/// S5.4 override binding record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct OverrideBinding {
    /// `ovr_<ULID>` binding identifier.
    pub binding_id: String,
    /// Strength/class used to issue the binding.
    pub class: OverrideClass,
    /// Subjects that granted the binding.
    pub granted_by: Vec<SubjectRef>,
    /// Grant timestamp.
    pub granted_at: DateTime<Utc>,
    /// Hard expiry timestamp.
    pub expires_at: DateTime<Utc>,
    /// Optional exact action bound to this override.
    pub target_action_id: Option<ActionId>,
    /// Current binding state.
    pub state: OverrideBindingState,
}
