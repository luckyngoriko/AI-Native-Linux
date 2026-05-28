#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use strum_macros::EnumCount;

/// Closed vocabulary for device lifecycle states (S8.3 §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount)]
pub enum DeviceLifecycleState {
    Detected,
    Probed,
    Bound,
    Active,
    Suspended,
    Quarantined,
    Removed,
    Recovered,
}
