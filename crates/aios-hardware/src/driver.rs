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

impl DriverProvenance {
    /// Human-readable label used in canonical byte construction for Ed25519
    /// signature verification (`DriverBindingRegistry::admit_binding`).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AiosVerified => "aios-verified",
            Self::SignedKernelModule => "signed-kernel-module",
            Self::DistroProvided => "distro-provided",
            Self::OutOfTreeBlacklisted => "out-of-tree-blacklisted",
            Self::OperatorLocalSigned => "operator-local-signed",
        }
    }
}
