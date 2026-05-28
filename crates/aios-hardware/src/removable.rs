#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use strum_macros::EnumCount;

/// Closed vocabulary for removable device policy (S8.3 §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount)]
pub enum RemovableDevicePolicy {
    DenyDefault,
    AllowReadOnly,
    AllowMount,
    AllowReadWrite,
    RecoveryDenied,
}
