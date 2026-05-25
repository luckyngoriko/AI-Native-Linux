//! S9.3 dedicated-kernel candidate typed core.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};
use ulid::Ulid;

/// Dedicated-kernel candidate id with canonical `kc_<ULID>` wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CandidateId(String);

impl CandidateId {
    /// Canonical prefix including the trailing underscore.
    pub const PREFIX: &'static str = "kc_";

    /// Mint a fresh kernel-candidate id.
    #[must_use]
    pub fn new() -> Self {
        Self(format!("{}{}", Self::PREFIX, Ulid::new()))
    }

    /// Borrow the canonical `kc_<ULID>` string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for CandidateId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CandidateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for CandidateId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Closed S9.3 kernel image state FSM, exposed as the candidate state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CandidateState {
    /// `BUILDING` - the kernel build action is executing.
    Building,
    /// `BUILT` - image bytes exist and have a BLAKE3 hash.
    Built,
    /// `GATING` - the six S9.3 gates are running or queued.
    Gating,
    /// `GATE_PASSED` - all gates passed; eligible for recovery-mode promotion.
    GatePassed,
    /// `GATE_FAILED` - at least one gate failed.
    GateFailed,
    /// `A_PROMOTED` - image is installed at slot A.
    #[serde(rename = "A_PROMOTED")]
    APromoted,
    /// `B_DEMOTED_TO_A` - validated dedicated image has demoted the previous A.
    #[serde(rename = "B_DEMOTED_TO_A")]
    BDemotedToA,
    /// `ROLLBACK` - bootloader rollback was performed.
    Rollback,
    /// `RETIRED` - image is archived and no longer bootable.
    Retired,
}

impl CandidateState {
    /// Return the exact S9.3 wire token.
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::Building => "BUILDING",
            Self::Built => "BUILT",
            Self::Gating => "GATING",
            Self::GatePassed => "GATE_PASSED",
            Self::GateFailed => "GATE_FAILED",
            Self::APromoted => "A_PROMOTED",
            Self::BDemotedToA => "B_DEMOTED_TO_A",
            Self::Rollback => "ROLLBACK",
            Self::Retired => "RETIRED",
        }
    }
}

impl fmt::Display for CandidateState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

/// S9.3 manifest metadata attached to a kernel candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelManifest {
    /// Kernel candidate version string.
    pub version: String,
    /// Minimum AIOS version required by this candidate.
    pub min_aios_version: String,
    /// Whether installation/promotion requires recovery mode.
    pub requires_recovery_install: bool,
    /// Optional S2.4 verification intent expression or id.
    pub verification_intent: Option<String>,
    /// Operator/pipeline tags attached to this manifest.
    pub tags: Vec<String>,
}

/// Registered S9.3 dedicated-kernel candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelCandidate {
    /// Candidate identifier.
    pub candidate_id: CandidateId,
    /// Kernel candidate version string.
    pub version: String,
    /// Full lower-hex BLAKE3 hash of the kernel image.
    pub kernel_blake3: String,
    /// Ed25519 signature over the candidate metadata/image binding.
    pub signature_ed25519: Vec<u8>,
    /// Signing authority identifier.
    pub signing_authority: String,
    /// UTC registration timestamp.
    pub registered_at: DateTime<Utc>,
    /// Current candidate state.
    pub state: CandidateState,
    /// Candidate manifest.
    pub manifest: KernelManifest,
}
