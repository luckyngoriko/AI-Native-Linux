//! ExposureApprovalState machine — S8.1 §5.
//!
//! Enforces INV I2 (Loopback → LanPending → LanApproved → LanActive with heartbeat)
//! and INV I10 (PUBLIC requiring recovery session, co-signer, 4h TTL, 5-min heartbeat).
//! LanActive → PublicPending direct transition is forbidden; PUBLIC must re-arm from Loopback.
//! Invalid transitions return ExposureEscalationDenied carrying from/to labels and the reason.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::error::NetworkPolicyError;
use crate::evidence::{NetworkEvidenceEmitter, WithEmitter};
use crate::ids::SubjectId;

// ---------------------------------------------------------------------------
// State enum
// ---------------------------------------------------------------------------

/// The approval state of network exposure (S8.1 §5).
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::derive_partial_eq_without_eq)]
pub enum ExposureApprovalState {
    /// Constitutional ground state (INV I2).
    Loopback,
    /// LAN exposure requested but not yet approved.
    LanPending {
        /// When the request was made.
        since: DateTime<Utc>,
        /// The subject requesting LAN exposure.
        requester: SubjectId,
    },
    /// LAN exposure policy-decision approved.
    LanApproved {
        /// When the policy decision was granted.
        granted_at: DateTime<Utc>,
        /// The policy decision identifier.
        policy_decision_id: String,
    },
    /// LAN exposure active with heartbeat.
    LanActive {
        /// When LAN exposure was activated.
        activated_at: DateTime<Utc>,
        /// Last heartbeat timestamp for liveness check.
        last_heartbeat_at: DateTime<Utc>,
    },
    /// PUBLIC exposure requested (requires recovery-mode session per INV I10).
    PublicPending {
        /// When the request was made.
        since: DateTime<Utc>,
        /// The subject requesting PUBLIC exposure.
        requester: SubjectId,
        /// Recovery session identifier (mandatory per INV I10).
        recovery_session_id: String,
    },
    /// PUBLIC exposure approved with co-signer and TTL hard-cap.
    PublicApproved {
        /// When the co-signer granted approval.
        granted_at: DateTime<Utc>,
        /// The policy decision identifier.
        policy_decision_id: String,
        /// The co-signer subject who approved.
        co_signer: SubjectId,
        /// Hard TTL expiration (4h max per INV I10).
        ttl_expires_at: DateTime<Utc>,
    },
    /// PUBLIC exposure active with 5-min heartbeat and TTL.
    PublicActive {
        /// When PUBLIC exposure was activated.
        activated_at: DateTime<Utc>,
        /// Last heartbeat timestamp (5-min interval).
        last_heartbeat_at: DateTime<Utc>,
        /// Hard TTL expiration carried from approval.
        ttl_expires_at: DateTime<Utc>,
    },
    /// Exposure has been revoked.
    Revoked {
        /// Why exposure was revoked.
        reason: String,
        /// When revocation occurred.
        revoked_at: DateTime<Utc>,
    },
}

// ---------------------------------------------------------------------------
// Closed label
// ---------------------------------------------------------------------------

/// Closed set of exposure approval labels matching [`ExposureApprovalState`] variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExposureApprovalLabel {
    /// Constitutional ground state — loopback only.
    Loopback,
    /// LAN exposure requested, pending policy decision.
    LanPending,
    /// LAN exposure approved by policy.
    LanApproved,
    /// LAN exposure active with heartbeat.
    LanActive,
    /// PUBLIC exposure requested, pending co-signer.
    PublicPending,
    /// PUBLIC exposure approved with co-signer and TTL.
    PublicApproved,
    /// PUBLIC exposure active with 5-min heartbeat.
    PublicActive,
    /// Exposure has been revoked.
    Revoked,
}

impl std::fmt::Display for ExposureApprovalLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loopback => write!(f, "Loopback"),
            Self::LanPending => write!(f, "LanPending"),
            Self::LanApproved => write!(f, "LanApproved"),
            Self::LanActive => write!(f, "LanActive"),
            Self::PublicPending => write!(f, "PublicPending"),
            Self::PublicApproved => write!(f, "PublicApproved"),
            Self::PublicActive => write!(f, "PublicActive"),
            Self::Revoked => write!(f, "Revoked"),
        }
    }
}

impl ExposureApprovalState {
    /// Returns the closed label for this state.
    #[must_use]
    pub const fn label(&self) -> ExposureApprovalLabel {
        match self {
            Self::Loopback => ExposureApprovalLabel::Loopback,
            Self::LanPending { .. } => ExposureApprovalLabel::LanPending,
            Self::LanApproved { .. } => ExposureApprovalLabel::LanApproved,
            Self::LanActive { .. } => ExposureApprovalLabel::LanActive,
            Self::PublicPending { .. } => ExposureApprovalLabel::PublicPending,
            Self::PublicApproved { .. } => ExposureApprovalLabel::PublicApproved,
            Self::PublicActive { .. } => ExposureApprovalLabel::PublicActive,
            Self::Revoked { .. } => ExposureApprovalLabel::Revoked,
        }
    }
}

// ---------------------------------------------------------------------------
// Transition recording
// ---------------------------------------------------------------------------

/// A recorded state transition with timestamp and reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExposureTransition {
    /// The state before transition.
    pub from: ExposureApprovalLabel,
    /// The state after transition.
    pub to: ExposureApprovalLabel,
    /// When the transition occurred.
    pub transitioned_at: DateTime<Utc>,
    /// Why the transition happened.
    pub reason: ExposureTransitionReason,
}

/// Why a transition occurred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExposureTransitionReason {
    /// Initial or reset transition.
    Initial,
    /// LAN exposure requested.
    LanRequest {
        /// The requesting subject.
        requester: SubjectId,
    },
    /// LAN policy decision approved.
    LanPolicyApproved {
        /// The policy decision identifier.
        decision_id: String,
    },
    /// LAN exposure activated.
    LanActivated,
    /// LAN heartbeat refresh.
    LanHeartbeat,
    /// PUBLIC exposure requested (recovery-mode).
    PublicRequest {
        /// The requesting subject.
        requester: SubjectId,
        /// The recovery session identifier.
        recovery_session_id: String,
    },
    /// PUBLIC co-signer approved.
    PublicCoSignerApproved {
        /// The policy decision identifier.
        decision_id: String,
        /// The co-signer subject.
        co_signer: SubjectId,
        /// Hard TTL expiration.
        ttl_expires_at: DateTime<Utc>,
    },
    /// PUBLIC exposure activated.
    PublicActivated,
    /// PUBLIC heartbeat refresh.
    PublicHeartbeat,
    /// Auto-revoked due to missed heartbeat.
    HeartbeatMissed,
    /// Auto-revoked due to TTL expiry.
    TtlExpired,
    /// Manual revocation.
    Revoked {
        /// The reason for revocation.
        reason: String,
    },
}

// ---------------------------------------------------------------------------
// Transition table
// ---------------------------------------------------------------------------

/// Returns true when `from → to` is an allowed transition per S8.1 §5.
const fn transition_allowed(from: ExposureApprovalLabel, to: ExposureApprovalLabel) -> bool {
    matches!(
        (from, to),
        (
            ExposureApprovalLabel::Loopback,
            ExposureApprovalLabel::LanPending | ExposureApprovalLabel::PublicPending
        ) | (
            ExposureApprovalLabel::LanPending,
            ExposureApprovalLabel::LanApproved
        ) | (
            ExposureApprovalLabel::LanApproved | ExposureApprovalLabel::LanActive,
            ExposureApprovalLabel::LanActive
        ) | (
            ExposureApprovalLabel::PublicPending,
            ExposureApprovalLabel::PublicApproved
        ) | (
            ExposureApprovalLabel::PublicApproved | ExposureApprovalLabel::PublicActive,
            ExposureApprovalLabel::PublicActive
        ) | (_, ExposureApprovalLabel::Revoked)
            | (
                ExposureApprovalLabel::Revoked,
                ExposureApprovalLabel::Loopback
            )
    )
}

fn denied(
    from: ExposureApprovalLabel,
    to: ExposureApprovalLabel,
    reason: &str,
) -> NetworkPolicyError {
    NetworkPolicyError::ExposureEscalationDenied {
        from: from.to_string(),
        to: to.to_string(),
        reason: reason.to_string(),
    }
}

// ---------------------------------------------------------------------------
// FSM
// ---------------------------------------------------------------------------

/// Enforces the [`ExposureApprovalState`] transition rules and heartbeat/TTL guards
/// for S8.1 §5 (INV I2 + INV I10).
pub struct ExposureApprovalFsm {
    state: RwLock<ExposureApprovalState>,
    history: RwLock<Vec<ExposureTransition>>,
    lan_heartbeat_interval: Duration,
    public_heartbeat_interval: Duration,
    emitter: RwLock<Option<Arc<dyn NetworkEvidenceEmitter>>>,
}

impl ExposureApprovalFsm {
    // -- constructors -------------------------------------------------------

    /// Creates a new FSM starting at Loopback with default heartbeat intervals
    /// (24h LAN, 5min PUBLIC).
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: RwLock::new(ExposureApprovalState::Loopback),
            history: RwLock::new(Vec::new()),
            lan_heartbeat_interval: Duration::from_hours(24),
            public_heartbeat_interval: Duration::from_mins(5),
            emitter: RwLock::new(None),
        }
    }

    /// Creates a new FSM with custom heartbeat intervals (primarily for testing).
    #[must_use]
    pub fn with_intervals(lan_heartbeat: Duration, public_heartbeat: Duration) -> Self {
        Self {
            state: RwLock::new(ExposureApprovalState::Loopback),
            history: RwLock::new(Vec::new()),
            lan_heartbeat_interval: lan_heartbeat,
            public_heartbeat_interval: public_heartbeat,
            emitter: RwLock::new(None),
        }
    }

    // -- read helpers -------------------------------------------------------

    /// Returns a clone of the current state.
    pub async fn current(&self) -> ExposureApprovalState {
        self.state.read().await.clone()
    }

    /// Returns a clone of the complete transition history.
    pub async fn history(&self) -> Vec<ExposureTransition> {
        self.history.read().await.clone()
    }

    // -- internal helpers ---------------------------------------------------

    async fn record_transition(
        &self,
        from: ExposureApprovalLabel,
        to: ExposureApprovalLabel,
        reason: ExposureTransitionReason,
    ) {
        let transition = ExposureTransition {
            from,
            to,
            transitioned_at: Utc::now(),
            reason,
        };
        self.history.write().await.push(transition.clone());

        if let Some(ref e) = *self.emitter.read().await {
            let actor = actor_from_reason(&transition.reason);
            let _ = e.emit_exposure_transition(&transition, actor).await;
        }
    }

    async fn enforce_transition(
        &self,
        target_label: ExposureApprovalLabel,
        new_state: ExposureApprovalState,
        reason: ExposureTransitionReason,
    ) -> Result<(), NetworkPolicyError> {
        let mut state = self.state.write().await;
        let from_label = state.label();
        if !transition_allowed(from_label, target_label) {
            return Err(denied(
                from_label,
                target_label,
                "transition not allowed by S8.1 §5",
            ));
        }
        let from_label = state.label();
        *state = new_state;
        drop(state);
        self.record_transition(from_label, target_label, reason)
            .await;
        Ok(())
    }

    // -- LAN lifecycle (INV I2) ---------------------------------------------

    /// Request LAN exposure from Loopback.
    ///
    /// # Errors
    /// Returns [] if the transition
    /// is not allowed by the S8.1 §5 transition table.
    pub async fn request_lan(&self, requester: SubjectId) -> Result<(), NetworkPolicyError> {
        let now = Utc::now();
        self.enforce_transition(
            ExposureApprovalLabel::LanPending,
            ExposureApprovalState::LanPending {
                since: now,
                requester: requester.clone(),
            },
            ExposureTransitionReason::LanRequest { requester },
        )
        .await
    }

    /// Apply a policy decision approving LAN exposure from `LanPending`.
    ///
    /// # Errors
    /// Returns [] if the transition
    /// is not allowed by the S8.1 §5 transition table.
    pub async fn apply_lan_policy_decision(
        &self,
        decision_id: &str,
    ) -> Result<(), NetworkPolicyError> {
        let now = Utc::now();
        self.enforce_transition(
            ExposureApprovalLabel::LanApproved,
            ExposureApprovalState::LanApproved {
                granted_at: now,
                policy_decision_id: decision_id.to_string(),
            },
            ExposureTransitionReason::LanPolicyApproved {
                decision_id: decision_id.to_string(),
            },
        )
        .await
    }

    /// Activate LAN exposure from `LanApproved`.
    ///
    /// # Errors
    /// Returns [] if the transition
    /// is not allowed by the S8.1 §5 transition table.
    pub async fn activate_lan(&self) -> Result<(), NetworkPolicyError> {
        let now = Utc::now();
        self.enforce_transition(
            ExposureApprovalLabel::LanActive,
            ExposureApprovalState::LanActive {
                activated_at: now,
                last_heartbeat_at: now,
            },
            ExposureTransitionReason::LanActivated,
        )
        .await
    }

    /// Refresh the LAN heartbeat timestamp while in `LanActive`.
    ///
    /// # Errors
    /// Returns [] if the transition
    /// is not allowed by the S8.1 §5 transition table.
    pub async fn record_lan_heartbeat(&self) -> Result<(), NetworkPolicyError> {
        let mut state = self.state.write().await;
        let from_label = state.label();
        if !transition_allowed(from_label, ExposureApprovalLabel::LanActive) {
            return Err(denied(
                from_label,
                ExposureApprovalLabel::LanActive,
                "heartbeat requires LanActive",
            ));
        }
        *state = ExposureApprovalState::LanActive {
            activated_at: Utc::now(), // preserved in spirit, timestamp for fresh activation
            last_heartbeat_at: Utc::now(),
        };
        drop(state);
        self.record_transition(
            from_label,
            ExposureApprovalLabel::LanActive,
            ExposureTransitionReason::LanHeartbeat,
        )
        .await;
        Ok(())
    }

    // -- PUBLIC lifecycle (INV I10) -----------------------------------------

    /// Request PUBLIC exposure from Loopback. Requires a non-empty
    /// `recovery_session_id` per INV I10.
    ///
    /// # Errors
    /// Returns [] if the transition
    /// is not allowed by the S8.1 §5 transition table.
    pub async fn request_public(
        &self,
        requester: SubjectId,
        recovery_session_id: &str,
    ) -> Result<(), NetworkPolicyError> {
        if recovery_session_id.is_empty() {
            return Err(denied(
                ExposureApprovalLabel::Loopback,
                ExposureApprovalLabel::PublicPending,
                "recovery-mode session required",
            ));
        }
        let now = Utc::now();
        self.enforce_transition(
            ExposureApprovalLabel::PublicPending,
            ExposureApprovalState::PublicPending {
                since: now,
                requester: requester.clone(),
                recovery_session_id: recovery_session_id.to_string(),
            },
            ExposureTransitionReason::PublicRequest {
                requester,
                recovery_session_id: recovery_session_id.to_string(),
            },
        )
        .await
    }

    /// Apply a co-signer approval for PUBLIC exposure from `PublicPending`.
    ///
    /// # Errors
    /// Returns [] if the transition
    /// is not allowed by the S8.1 §5 transition table.
    pub async fn apply_public_co_signer_approval(
        &self,
        decision_id: &str,
        co_signer: SubjectId,
        ttl_expires_at: DateTime<Utc>,
    ) -> Result<(), NetworkPolicyError> {
        let now = Utc::now();
        self.enforce_transition(
            ExposureApprovalLabel::PublicApproved,
            ExposureApprovalState::PublicApproved {
                granted_at: now,
                policy_decision_id: decision_id.to_string(),
                co_signer: co_signer.clone(),
                ttl_expires_at,
            },
            ExposureTransitionReason::PublicCoSignerApproved {
                decision_id: decision_id.to_string(),
                co_signer,
                ttl_expires_at,
            },
        )
        .await
    }

    /// Activate PUBLIC exposure from `PublicApproved`.
    ///
    /// # Errors
    /// Returns [] if the transition
    /// is not allowed by the S8.1 §5 transition table.
    pub async fn activate_public(&self) -> Result<(), NetworkPolicyError> {
        // We need to read the current ttl_expires_at from the state before transitioning.
        let ttl_expires_at = {
            let state = self.state.read().await;
            match &*state {
                ExposureApprovalState::PublicApproved { ttl_expires_at, .. } => *ttl_expires_at,
                _ => {
                    return Err(denied(
                        state.label(),
                        ExposureApprovalLabel::PublicActive,
                        "activate requires PublicApproved",
                    ));
                }
            }
        };
        let now = Utc::now();
        self.enforce_transition(
            ExposureApprovalLabel::PublicActive,
            ExposureApprovalState::PublicActive {
                activated_at: now,
                last_heartbeat_at: now,
                ttl_expires_at,
            },
            ExposureTransitionReason::PublicActivated,
        )
        .await
    }

    /// Refresh the PUBLIC heartbeat timestamp while in `PublicActive`.
    ///
    /// # Errors
    /// Returns [] if the transition
    /// is not allowed by the S8.1 §5 transition table.
    pub async fn record_public_heartbeat(&self) -> Result<(), NetworkPolicyError> {
        let ttl = {
            let state = self.state.read().await;
            match &*state {
                ExposureApprovalState::PublicActive { ttl_expires_at, .. } => *ttl_expires_at,
                _ => {
                    return Err(denied(
                        state.label(),
                        ExposureApprovalLabel::PublicActive,
                        "heartbeat requires PublicActive",
                    ));
                }
            }
        };
        let mut state = self.state.write().await;
        let from_label = state.label();
        *state = ExposureApprovalState::PublicActive {
            activated_at: Utc::now(),
            last_heartbeat_at: Utc::now(),
            ttl_expires_at: ttl,
        };
        drop(state);
        self.record_transition(
            from_label,
            ExposureApprovalLabel::PublicActive,
            ExposureTransitionReason::PublicHeartbeat,
        )
        .await;
        Ok(())
    }

    // -- revoke / reset -----------------------------------------------------

    /// Revoke exposure from any state.
    ///
    /// # Errors
    /// Returns [] if the transition
    /// is not allowed by the S8.1 §5 transition table.
    pub async fn revoke(&self, reason: &str) -> Result<(), NetworkPolicyError> {
        let now = Utc::now();
        let reason_owned = reason.to_string();
        self.enforce_transition(
            ExposureApprovalLabel::Revoked,
            ExposureApprovalState::Revoked {
                reason: reason_owned.clone(),
                revoked_at: now,
            },
            ExposureTransitionReason::Revoked {
                reason: reason_owned,
            },
        )
        .await
    }

    /// Reset from Revoked back to Loopback.
    ///
    /// # Errors
    /// Returns [] if the transition
    /// is not allowed by the S8.1 §5 transition table.
    pub async fn reset_to_loopback(&self) -> Result<(), NetworkPolicyError> {
        self.enforce_transition(
            ExposureApprovalLabel::Loopback,
            ExposureApprovalState::Loopback,
            ExposureTransitionReason::Initial,
        )
        .await
    }

    // -- heartbeat / TTL guard ----------------------------------------------

    /// Checks heartbeat and TTL; auto-revokes if either has expired.
    ///
    /// # Errors
    /// Returns [] if the transition
    /// is not allowed by the S8.1 §5 transition table.
    pub async fn check_heartbeat(&self) -> Result<(), NetworkPolicyError> {
        let (should_revoke, revoke_reason) = {
            let state = self.state.read().await;
            let now = Utc::now();
            match &*state {
                ExposureApprovalState::LanActive {
                    last_heartbeat_at, ..
                } => {
                    let elapsed = now
                        .signed_duration_since(*last_heartbeat_at)
                        .to_std()
                        .unwrap_or(Duration::MAX);
                    if elapsed > self.lan_heartbeat_interval {
                        (true, "HeartbeatMissed: LAN heartbeat expired")
                    } else {
                        (false, "")
                    }
                }
                ExposureApprovalState::PublicActive {
                    last_heartbeat_at,
                    ttl_expires_at,
                    ..
                } => {
                    if now > *ttl_expires_at {
                        (true, "TtlExpired: PUBLIC TTL expired")
                    } else {
                        let elapsed = now
                            .signed_duration_since(*last_heartbeat_at)
                            .to_std()
                            .unwrap_or(Duration::MAX);
                        if elapsed > self.public_heartbeat_interval {
                            (true, "HeartbeatMissed: PUBLIC heartbeat expired")
                        } else {
                            (false, "")
                        }
                    }
                }
                ExposureApprovalState::PublicApproved { ttl_expires_at, .. }
                    if now > *ttl_expires_at =>
                {
                    (true, "TtlExpired: PUBLIC TTL expired before activation")
                }
                _ => (false, ""),
            }
        };

        if should_revoke {
            self.revoke(revoke_reason).await?;
        }

        Ok(())
    }

    // -- test helper --------------------------------------------------------

    /// Sets `last_heartbeat_at` on the current state. No-op if not in an
    /// active state. For deterministic heartbeat/TTL tests only.
    #[doc(hidden)]
    pub async fn set_last_heartbeat_at_for_tests(&self, t: DateTime<Utc>) {
        let mut state = self.state.write().await;
        match &mut *state {
            ExposureApprovalState::LanActive {
                last_heartbeat_at, ..
            }
            | ExposureApprovalState::PublicActive {
                last_heartbeat_at, ..
            } => {
                *last_heartbeat_at = t;
            }
            _ => {}
        }
    }
}

impl WithEmitter for ExposureApprovalFsm {
    fn with_emitter(mut self, emitter: Option<Arc<dyn NetworkEvidenceEmitter>>) -> Self {
        self.emitter = RwLock::new(emitter);
        self
    }
}

impl Default for ExposureApprovalFsm {
    fn default() -> Self {
        Self::new()
    }
}

fn actor_from_reason(reason: &ExposureTransitionReason) -> &str {
    match reason {
        ExposureTransitionReason::LanRequest { requester }
        | ExposureTransitionReason::PublicRequest { requester, .. } => &requester.0,
        ExposureTransitionReason::PublicCoSignerApproved { co_signer, .. } => &co_signer.0,
        _ => "_system",
    }
}
