//! Object lifecycle vocabulary — S1.3 §4.2 / §5.

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// AIOS-FS object lifecycle state.
///
/// The active S1.3 lifecycle is `ACTIVE -> RETIRED -> PURGED`. `RETIRED` is a
/// logical removal state: reads remain available for audit, while writes and
/// pointer moves are denied. `PURGED` records scheduled or completed physical
/// erase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LifecycleState {
    /// `ACTIVE` — default; reads and writes allowed per policy.
    Active,
    /// `RETIRED` — logical removal; reads allowed for audit, writes denied.
    Retired,
    /// `PURGED` — physical erase scheduled or completed.
    Purged,
}

impl LifecycleState {
    /// Returns `true` for states where normal mutation is terminal.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Retired | Self::Purged)
    }
}
