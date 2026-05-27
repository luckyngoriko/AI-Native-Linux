//! Exposure level FSM marker types (S7.5 §I3).
//!
//! Six closed `ExposureLevel` variants enforce the INV I3 lifecycle:
//! `Localhost` → `LanPending` → `LanApproved` → `LanActive` (heartbeat) or
//! `Public` (recovery-authorized). `Revoked` is terminal.
//!
//! FSM transitions are not defined here — they land in T-144.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Exposure level — 6-variant FSM marker per INV I3.
///
/// Every Web renderer surface starts at `Localhost`. Escalation to any wider
/// scope requires explicit evidence and policy authorization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExposureLevel {
    /// Default — binds only to `127.0.0.1` / `[::1]`. No evidence required.
    Localhost,
    /// LAN exposure has been requested and is awaiting operator approval.
    LanPending {
        /// When the LAN exposure request was submitted.
        since: DateTime<Utc>,
        /// Canonical subject ID of the approver gate.
        approver_canonical_id: String,
    },
    /// LAN exposure has been approved but is not yet active (pending service
    /// restart or bind).
    LanApproved {
        /// When the LAN approval was granted.
        granted_at: DateTime<Utc>,
        /// S3.1 evidence receipt ID for the `WEB_EXPOSURE_GRANTED` event.
        policy_decision_id: String,
    },
    /// LAN exposure is active. Requires periodic heartbeat (INV I3
    /// §`STANDARD_24M` — every 30 s) to remain in this state.
    LanActive {
        /// When LAN exposure was activated.
        activated_at: DateTime<Utc>,
        /// Timestamp of the last successful heartbeat.
        last_heartbeat_at: DateTime<Utc>,
    },
    /// Public internet exposure — requires recovery-mode authorization (INV I3
    /// § Public requires recovery-mode authorization).
    Public {
        /// When public exposure was granted.
        granted_at: DateTime<Utc>,
        /// The recovery operator who authorized the public exposure.
        recovery_authorized_by: String,
        /// S3.1 evidence receipt ID for the authorization event.
        policy_decision_id: String,
    },
    /// Exposure has been revoked — terminal state.
    Revoked {
        /// Human-readable reason for revocation.
        reason: String,
        /// When the revocation occurred.
        revoked_at: DateTime<Utc>,
    },
}

/// Closed labels for `ExposureLevel` used in error messages and evidence
/// emission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExposureLevelLabel {
    /// `Localhost`
    Localhost,
    /// `LanPending`
    LanPending,
    /// `LanApproved`
    LanApproved,
    /// `LanActive`
    LanActive,
    /// `Public`
    Public,
    /// `Revoked`
    Revoked,
}

impl std::fmt::Display for ExposureLevelLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Localhost => write!(f, "Localhost"),
            Self::LanPending => write!(f, "LanPending"),
            Self::LanApproved => write!(f, "LanApproved"),
            Self::LanActive => write!(f, "LanActive"),
            Self::Public => write!(f, "Public"),
            Self::Revoked => write!(f, "Revoked"),
        }
    }
}

impl ExposureLevel {
    /// Return the label for this exposure level variant.
    #[must_use]
    pub const fn label(&self) -> ExposureLevelLabel {
        match self {
            Self::Localhost => ExposureLevelLabel::Localhost,
            Self::LanPending { .. } => ExposureLevelLabel::LanPending,
            Self::LanApproved { .. } => ExposureLevelLabel::LanApproved,
            Self::LanActive { .. } => ExposureLevelLabel::LanActive,
            Self::Public { .. } => ExposureLevelLabel::Public,
            Self::Revoked { .. } => ExposureLevelLabel::Revoked,
        }
    }
}
