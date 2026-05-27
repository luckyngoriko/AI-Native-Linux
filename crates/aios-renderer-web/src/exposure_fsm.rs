//! Exposure FSM — transition state machine enforcing INV I3 escalation rules.
//!
//! The FSM enforces the `Localhost → LanPending → LanApproved → LanActive`
//! chain with a 24 h heartbeat that auto-revokes `LanActive` when missed, and the
//! `Localhost → Public` escalation path requiring both recovery authorization and a
//! policy decision ID.
//!
//! All invalid transitions return `WebRendererError::ExposureEscalationDenied`.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::error::WebRendererError;
use crate::evidence::WebEvidenceEmitter;
use crate::exposure::{ExposureLevel, ExposureLevelLabel};

/// Twenty-four hours in seconds — INV I3 default heartbeat interval.
const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 24 * 60 * 60;

/// State machine governing `ExposureLevel` transitions (INV I3).
///
/// Holds the current exposure level, a transition history, an independent
/// heartbeat timestamp (so the guard can read it without matching on the
/// current variant), and a configurable heartbeat interval.
pub struct ExposureFsm {
    current: RwLock<ExposureLevel>,
    history: RwLock<Vec<ExposureTransition>>,
    last_heartbeat_at: RwLock<Option<DateTime<Utc>>>,
    heartbeat_interval: Duration,
    evidence_emitter: Option<Arc<dyn WebEvidenceEmitter>>,
}

/// One recorded transition in the exposure lifecycle.
#[derive(Debug, Clone)]
pub struct ExposureTransition {
    /// Label of the state before the transition.
    pub from: ExposureLevelLabel,
    /// Label of the state after the transition.
    pub to: ExposureLevelLabel,
    /// When the transition was committed.
    pub transitioned_at: DateTime<Utc>,
    /// Why the transition occurred.
    pub reason: ExposureTransitionReason,
}

/// Why a transition occurred (closed vocabulary per INV I3).
#[derive(Debug, Clone)]
pub enum ExposureTransitionReason {
    /// FSM was constructed — first entry in the history log.
    Initial,
    /// A policy decision approved the escalation (e.g. `LanPending → LanApproved`).
    PolicyApprovalGranted {
        /// S3.1 evidence receipt ID for the granting decision.
        decision_id: String,
    },
    /// An operator (human or system principal) explicitly requested the escalation.
    OperatorRequest {
        /// Canonical subject ID of the requesting principal.
        canonical_id: String,
    },
    /// The 24 h heartbeat was not received in time — automatic revocation.
    HeartbeatMissed,
    /// Recovery-mode authorization granted public-internet exposure.
    RecoveryAuthorized {
        /// Canonical subject ID of the recovery operator.
        authorized_by: String,
    },
    /// Exposure was explicitly revoked.
    Revoked {
        /// Human-readable reason for revocation.
        reason: String,
    },
}

impl ExposureFsm {
    /// Create a new FSM starting at `Localhost` with a 24 h heartbeat interval.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current: RwLock::new(ExposureLevel::Localhost),
            history: RwLock::new(Vec::new()),
            last_heartbeat_at: RwLock::new(None),
            heartbeat_interval: Duration::from_secs(DEFAULT_HEARTBEAT_INTERVAL_SECS),
            evidence_emitter: None,
        }
    }

    /// Create a new FSM with a custom heartbeat interval (for testing).
    #[must_use]
    pub fn with_heartbeat_interval(interval: Duration) -> Self {
        Self {
            current: RwLock::new(ExposureLevel::Localhost),
            history: RwLock::new(Vec::new()),
            last_heartbeat_at: RwLock::new(None),
            heartbeat_interval: interval,
            evidence_emitter: None,
        }
    }

    /// Attach an optional evidence emitter for lifecycle event emission.
    #[must_use]
    pub fn with_evidence_emitter(mut self, emitter: Arc<dyn WebEvidenceEmitter>) -> Self {
        self.evidence_emitter = Some(emitter);
        self
    }

    // ── accessors ──────────────────────────────────────────────────────

    /// Return a clone of the current exposure level.
    pub async fn current(&self) -> ExposureLevel {
        self.current.read().await.clone()
    }

    /// Return a snapshot of the full transition history.
    pub async fn history(&self) -> Vec<ExposureTransition> {
        self.history.read().await.clone()
    }

    // ── transition helpers (private) ───────────────────────────────────

    /// Record a transition and update the current level in one write.
    async fn commit(
        &self,
        from: ExposureLevelLabel,
        to: ExposureLevelLabel,
        level: ExposureLevel,
        reason: ExposureTransitionReason,
    ) {
        let now = Utc::now();
        {
            let mut h = self.history.write().await;
            h.push(ExposureTransition {
                from,
                to,
                transitioned_at: now,
                reason,
            });
        }
        *self.current.write().await = level;
    }

    /// Return `Err(ExposureEscalationDenied)` for a disallowed transition.
    fn denied(from: ExposureLevelLabel, to: ExposureLevelLabel, reason: &str) -> WebRendererError {
        WebRendererError::ExposureEscalationDenied {
            from,
            to,
            reason: reason.to_string(),
        }
    }

    // ── public transitions ─────────────────────────────────────────────

    /// Request LAN escalation (`Localhost → LanPending`).
    ///
    /// Requires the canonical ID of the approver who will authorize this
    /// request.
    ///
    /// # Errors
    ///
    /// Returns `ExposureEscalationDenied` if the current state is not
    /// `Localhost`.
    pub async fn request_lan_escalation(
        &self,
        approver_canonical_id: &str,
    ) -> Result<(), WebRendererError> {
        let current = self.current.read().await.clone();
        match current {
            ExposureLevel::Localhost => {
                let level = ExposureLevel::LanPending {
                    since: Utc::now(),
                    approver_canonical_id: approver_canonical_id.to_string(),
                };
                self.commit(
                    ExposureLevelLabel::Localhost,
                    ExposureLevelLabel::LanPending,
                    level,
                    ExposureTransitionReason::OperatorRequest {
                        canonical_id: approver_canonical_id.to_string(),
                    },
                )
                .await;
                Ok(())
            }
            other => Err(Self::denied(
                other.label(),
                ExposureLevelLabel::LanPending,
                "only Localhost can request LAN escalation",
            )),
        }
    }

    /// Apply a policy decision approving the pending LAN request
    /// (`LanPending → LanApproved`).
    ///
    /// # Errors
    ///
    /// Returns `ExposureEscalationDenied` if the current state is not
    /// `LanPending`.
    pub async fn apply_policy_decision(&self, decision_id: &str) -> Result<(), WebRendererError> {
        let current = self.current.read().await.clone();
        match current {
            ExposureLevel::LanPending { since, .. } => {
                let level = ExposureLevel::LanApproved {
                    granted_at: Utc::now(),
                    policy_decision_id: decision_id.to_string(),
                };
                // Keep `since` in scope so the borrow-checker is happy.
                let _ = since;
                self.commit(
                    ExposureLevelLabel::LanPending,
                    ExposureLevelLabel::LanApproved,
                    level,
                    ExposureTransitionReason::PolicyApprovalGranted {
                        decision_id: decision_id.to_string(),
                    },
                )
                .await;
                Ok(())
            }
            other => Err(Self::denied(
                other.label(),
                ExposureLevelLabel::LanApproved,
                "only LanPending can receive a policy decision",
            )),
        }
    }

    /// Activate LAN exposure (`LanApproved → LanActive`) and record the
    /// initial heartbeat.
    ///
    /// # Errors
    ///
    /// Returns `ExposureEscalationDenied` if the current state is not
    /// `LanApproved`.
    pub async fn activate_lan_exposure(&self) -> Result<(), WebRendererError> {
        let current = self.current.read().await.clone();
        match current {
            ExposureLevel::LanApproved { .. } => {
                let now = Utc::now();
                let level = ExposureLevel::LanActive {
                    activated_at: now,
                    last_heartbeat_at: now,
                };
                *self.last_heartbeat_at.write().await = Some(now);
                self.commit(
                    ExposureLevelLabel::LanApproved,
                    ExposureLevelLabel::LanActive,
                    level,
                    ExposureTransitionReason::OperatorRequest {
                        canonical_id: "system:activate".to_string(),
                    },
                )
                .await;
                Ok(())
            }
            other => Err(Self::denied(
                other.label(),
                ExposureLevelLabel::LanActive,
                "only LanApproved can activate LAN exposure",
            )),
        }
    }

    /// Refresh the heartbeat timestamp (`LanActive → LanActive`).
    ///
    /// This is a self-transition — no history entry is recorded.
    ///
    /// # Errors
    ///
    /// Returns `ExposureEscalationDenied` if the current state is not
    /// `LanActive`.
    pub async fn record_heartbeat(&self) -> Result<(), WebRendererError> {
        let mut current = self.current.write().await;
        match &*current {
            ExposureLevel::LanActive { activated_at, .. } => {
                let now = Utc::now();
                *current = ExposureLevel::LanActive {
                    activated_at: *activated_at,
                    last_heartbeat_at: now,
                };
                drop(current);
                *self.last_heartbeat_at.write().await = Some(now);
                Ok(())
            }
            other => {
                let label = other.label();
                drop(current);
                Err(Self::denied(
                    label,
                    ExposureLevelLabel::LanActive,
                    "heartbeat only valid while LanActive",
                ))
            }
        }
    }

    /// Revoke exposure from any revocable state.
    ///
    /// Valid from: `LanActive`, `LanPending`, `LanApproved`, `Public`.
    ///
    /// # Errors
    ///
    /// Returns `ExposureEscalationDenied` if the current state is not
    /// revocable (e.g. already `Revoked` or `Localhost`).
    pub async fn revoke(&self, reason: &str) -> Result<(), WebRendererError> {
        let current = self.current.read().await.clone();
        let from = current.label();
        match current {
            ExposureLevel::LanActive { .. }
            | ExposureLevel::LanPending { .. }
            | ExposureLevel::LanApproved { .. }
            | ExposureLevel::Public { .. } => {
                let level = ExposureLevel::Revoked {
                    reason: reason.to_string(),
                    revoked_at: Utc::now(),
                };
                self.commit(
                    from,
                    ExposureLevelLabel::Revoked,
                    level,
                    ExposureTransitionReason::Revoked {
                        reason: reason.to_string(),
                    },
                )
                .await;
                Ok(())
            }
            other => Err(Self::denied(
                other.label(),
                ExposureLevelLabel::Revoked,
                "cannot revoke from this state",
            )),
        }
    }

    /// Escalate directly to public-internet exposure (`Localhost → Public`).
    ///
    /// Requires both a recovery operator authorization and a policy decision
    /// ID, per INV I3.
    ///
    /// # Errors
    ///
    /// Returns `ExposureEscalationDenied` if the current state is not
    /// `Localhost`.
    pub async fn escalate_to_public(
        &self,
        authorized_by: &str,
        decision_id: &str,
    ) -> Result<(), WebRendererError> {
        let current = self.current.read().await.clone();
        match current {
            ExposureLevel::Localhost => {
                let level = ExposureLevel::Public {
                    granted_at: Utc::now(),
                    recovery_authorized_by: authorized_by.to_string(),
                    policy_decision_id: decision_id.to_string(),
                };
                self.commit(
                    ExposureLevelLabel::Localhost,
                    ExposureLevelLabel::Public,
                    level,
                    ExposureTransitionReason::RecoveryAuthorized {
                        authorized_by: authorized_by.to_string(),
                    },
                )
                .await;
                Ok(())
            }
            other => Err(Self::denied(
                other.label(),
                ExposureLevelLabel::Public,
                "only Localhost can escalate directly to Public",
            )),
        }
    }

    /// Re-arm after revocation (`Revoked → Localhost`).
    ///
    /// # Errors
    ///
    /// Returns `ExposureEscalationDenied` if the current state is not
    /// `Revoked`.
    pub async fn reset_to_localhost(&self) -> Result<(), WebRendererError> {
        let current = self.current.read().await.clone();
        match current {
            ExposureLevel::Revoked { .. } => {
                self.commit(
                    ExposureLevelLabel::Revoked,
                    ExposureLevelLabel::Localhost,
                    ExposureLevel::Localhost,
                    ExposureTransitionReason::Initial,
                )
                .await;
                Ok(())
            }
            other => Err(Self::denied(
                other.label(),
                ExposureLevelLabel::Localhost,
                "only Revoked can reset to Localhost",
            )),
        }
    }

    // ── heartbeat guard ────────────────────────────────────────────────

    /// Check whether the heartbeat has expired.
    ///
    /// If the current state is `LanActive` and the last heartbeat is older
    /// than `heartbeat_interval`, the FSM auto-transitions to `Revoked` with
    /// reason `HeartbeatMissed` and returns
    /// `Err(ExposureEscalationDenied)`.
    ///
    /// Otherwise returns `Ok(())`.
    ///
    /// # Errors
    ///
    /// Returns `ExposureEscalationDenied` when the heartbeat window has been
    /// exceeded and the FSM auto-revokes.
    pub async fn check_heartbeat(&self) -> Result<(), WebRendererError> {
        let current = self.current.read().await.clone();
        match current {
            ExposureLevel::LanActive {
                last_heartbeat_at, ..
            } => {
                let elapsed = Utc::now()
                    .signed_duration_since(last_heartbeat_at)
                    .to_std()
                    .unwrap_or(Duration::MAX);
                if elapsed > self.heartbeat_interval {
                    // Drop read guard before taking write lock below.
                    drop(current);
                    let level = ExposureLevel::Revoked {
                        reason: "heartbeat missed beyond 24h".to_string(),
                        revoked_at: Utc::now(),
                    };
                    self.commit(
                        ExposureLevelLabel::LanActive,
                        ExposureLevelLabel::Revoked,
                        level,
                        ExposureTransitionReason::HeartbeatMissed,
                    )
                    .await;
                    Err(Self::denied(
                        ExposureLevelLabel::LanActive,
                        ExposureLevelLabel::Revoked,
                        "heartbeat missed beyond 24h",
                    ))
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }

    // ── test helpers ───────────────────────────────────────────────────

    /// Directly set the last-heartbeat timestamp (test-only).
    ///
    /// Also updates the `LanActive` variant's `last_heartbeat_at` field if
    /// the current state is `LanActive`, so `current()` and the internal
    /// heartbeat timestamp stay in sync.
    #[doc(hidden)]
    pub async fn set_last_heartbeat_at_for_tests(&self, t: DateTime<Utc>) {
        *self.last_heartbeat_at.write().await = Some(t);
        let mut current = self.current.write().await;
        if let ExposureLevel::LanActive { activated_at, .. } = &*current {
            *current = ExposureLevel::LanActive {
                activated_at: *activated_at,
                last_heartbeat_at: t,
            };
        }
    }
}

impl Default for ExposureFsm {
    fn default() -> Self {
        Self::new()
    }
}
