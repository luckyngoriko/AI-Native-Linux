#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Closed vocabulary for driver provenance (S8.3 §3.3).
/// Ordered from most-trusted to least-trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount, EnumIter)]
pub enum DriverProvenance {
    AiosVerified,
    SignedKernelModule,
    DistroProvided,
    OutOfTreeBlacklisted,
    OperatorLocalSigned,
}
