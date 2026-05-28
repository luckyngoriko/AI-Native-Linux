//! Network policy controller trait and in-memory implementation (S8.1 §6).
//!
//! The [`NetworkPolicyController`] async trait is the single service interface for
//! host-wide posture management, per-subject outbound directive registry, and
//! connection evaluation. [`InMemoryNetworkPolicyController`] provides the
//! in-memory backing used by all M16 tasks.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::error::{NetworkPolicyError, NetworkPolicyErrorCode};
use crate::evidence::{NetworkEvidenceEmitter, WithEmitter};
use crate::ids::SubjectId;
use crate::outbound::OutboundDirective;
use crate::posture::NetworkPosture;
use crate::protocol::ProtocolFamily;

/// Request to evaluate whether a subject should be allowed to connect.
#[derive(Debug, Clone)]
pub struct EvaluateConnectionRequest {
    /// The subject requesting the connection.
    pub subject: SubjectId,
    /// Destination in `"host:port"` or `"ip:port"` form.
    pub destination: String,
    /// The protocol family for this connection.
    pub protocol: ProtocolFamily,
}

/// Outcome of a connection evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionDecision {
    /// Connection is allowed, keyed by the matching rule identifier.
    Allowed {
        /// Identifier of the rule that matched.
        matched_rule_id: String,
    },
    /// Connection is denied with a closed error code.
    Denied {
        /// The denial reason code.
        code: NetworkPolicyErrorCode,
    },
}

/// Receipt recorded every time the host-wide posture changes.
#[derive(Debug, Clone)]
pub struct PostureChangeReceipt {
    /// Previous posture before the change.
    pub from: NetworkPosture,
    /// New posture after the change.
    pub to: NetworkPosture,
    /// The subject who initiated the change.
    pub actor: SubjectId,
    /// Timestamp of the change (wall clock).
    pub at: DateTime<Utc>,
}

/// Service interface for network policy control (S8.1 §6).
///
/// All RPCs are async and `Send + Sync` so the trait can be held behind `Arc`.
#[async_trait]
pub trait NetworkPolicyController: Send + Sync {
    /// Return the current host-wide network posture.
    async fn current_posture(&self) -> NetworkPosture;

    /// Set a new host-wide posture, recording a [`PostureChangeReceipt`].
    async fn set_posture(
        &self,
        new: NetworkPosture,
        actor: SubjectId,
    ) -> Result<PostureChangeReceipt, NetworkPolicyError>;

    /// Look up the outbound directive for a subject. Defaults to [`OutboundDirective::DenyAll`].
    async fn subject_directive(&self, subject: &SubjectId) -> OutboundDirective;

    /// Set the outbound directive for a subject.
    async fn set_subject_directive(
        &self,
        subject: SubjectId,
        directive: OutboundDirective,
        actor: SubjectId,
    ) -> Result<(), NetworkPolicyError>;

    /// List all currently active per-subject directives.
    async fn list_directives(&self) -> Vec<(SubjectId, OutboundDirective)>;

    /// Remove the outbound directive for a subject (idempotent).
    async fn revoke_subject_directive(
        &self,
        subject: &SubjectId,
        actor: SubjectId,
    ) -> Result<(), NetworkPolicyError>;

    /// Evaluate whether a connection request should be allowed.
    ///
    /// In this stub (T-152) the decision is based solely on the subject's
    /// outbound directive. Allowlist and VPN resolution are wired in T-153/T-155/T-158.
    async fn evaluate_connection(
        &self,
        req: EvaluateConnectionRequest,
    ) -> Result<ConnectionDecision, NetworkPolicyError>;
}

// ── in-memory state ──────────────────────────────────────────────────────────

struct ControllerState {
    posture: NetworkPosture,
    directives: HashMap<SubjectId, OutboundDirective>,
    posture_history: Vec<PostureChangeReceipt>,
}

/// In-memory [`NetworkPolicyController`] backed by `RwLock<ControllerState>`.
///
/// Suitable for unit tests and as the default backing store until a persistent
/// controller is wired (T-161).
pub struct InMemoryNetworkPolicyController {
    state: RwLock<ControllerState>,
    emitter: RwLock<Option<Arc<dyn NetworkEvidenceEmitter>>>,
}

impl InMemoryNetworkPolicyController {
    /// Create a controller with the first-boot default posture [`NetworkPosture::LanLocal`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: RwLock::new(ControllerState {
                posture: NetworkPosture::LanLocal,
                directives: HashMap::new(),
                posture_history: Vec::new(),
            }),
            emitter: RwLock::new(None),
        }
    }

    /// Create a controller with a custom initial posture.
    #[must_use]
    pub fn new_with_posture(initial: NetworkPosture) -> Self {
        Self {
            state: RwLock::new(ControllerState {
                posture: initial,
                directives: HashMap::new(),
                posture_history: Vec::new(),
            }),
            emitter: RwLock::new(None),
        }
    }

    /// Return a clone of the full posture change history.
    pub async fn posture_history(&self) -> Vec<PostureChangeReceipt> {
        self.state.read().await.posture_history.clone()
    }
}

impl WithEmitter for InMemoryNetworkPolicyController {
    fn with_emitter(mut self, emitter: Option<Arc<dyn NetworkEvidenceEmitter>>) -> Self {
        self.emitter = RwLock::new(emitter);
        self
    }
}

impl Default for InMemoryNetworkPolicyController {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NetworkPolicyController for InMemoryNetworkPolicyController {
    async fn current_posture(&self) -> NetworkPosture {
        self.state.read().await.posture
    }

    async fn set_posture(
        &self,
        new: NetworkPosture,
        actor: SubjectId,
    ) -> Result<PostureChangeReceipt, NetworkPolicyError> {
        let mut state = self.state.write().await;
        let receipt = PostureChangeReceipt {
            from: state.posture,
            to: new,
            actor,
            at: Utc::now(),
        };
        state.posture = new;
        state.posture_history.push(receipt.clone());
        drop(state);

        if let Some(ref e) = *self.emitter.read().await {
            e.emit_posture_changed(&receipt).await?;
        }

        Ok(receipt)
    }

    async fn subject_directive(&self, subject: &SubjectId) -> OutboundDirective {
        self.state
            .read()
            .await
            .directives
            .get(subject)
            .cloned()
            .unwrap_or(OutboundDirective::DenyAll)
    }

    async fn set_subject_directive(
        &self,
        subject: SubjectId,
        directive: OutboundDirective,
        _actor: SubjectId,
    ) -> Result<(), NetworkPolicyError> {
        self.state
            .write()
            .await
            .directives
            .insert(subject, directive);
        Ok(())
    }

    async fn list_directives(&self) -> Vec<(SubjectId, OutboundDirective)> {
        self.state
            .read()
            .await
            .directives
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    async fn revoke_subject_directive(
        &self,
        subject: &SubjectId,
        _actor: SubjectId,
    ) -> Result<(), NetworkPolicyError> {
        self.state.write().await.directives.remove(subject);
        Ok(())
    }

    async fn evaluate_connection(
        &self,
        req: EvaluateConnectionRequest,
    ) -> Result<ConnectionDecision, NetworkPolicyError> {
        let directive = self.subject_directive(&req.subject).await;
        Ok(match directive {
            OutboundDirective::AllowLoopbackOnly => {
                if is_loopback(&req.destination) {
                    ConnectionDecision::Allowed {
                        matched_rule_id: "allow-loopback".into(),
                    }
                } else {
                    ConnectionDecision::Denied {
                        code: NetworkPolicyErrorCode::DefaultDeny,
                    }
                }
            }
            OutboundDirective::AllowInternet => ConnectionDecision::Allowed {
                matched_rule_id: "allow-internet".into(),
            },
            OutboundDirective::DenyAll
            | OutboundDirective::AllowListOnly { .. }
            | OutboundDirective::AllowVpnOnly { .. } => ConnectionDecision::Denied {
                code: NetworkPolicyErrorCode::DefaultDeny,
            },
        })
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn is_loopback(destination: &str) -> bool {
    let host = if destination.starts_with('[') {
        destination
            .split(']')
            .next()
            .map_or(destination, |s| &s[1..])
    } else {
        destination.split(':').next().unwrap_or(destination)
    };
    matches!(host, "127.0.0.1" | "localhost" | "::1") || destination == "::1"
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    // ── construction ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn new_controller_starts_at_lan_local() {
        let ctrl = InMemoryNetworkPolicyController::new();
        assert_eq!(ctrl.current_posture().await, NetworkPosture::LanLocal);
    }

    #[tokio::test]
    async fn new_with_posture_loopback_only_starts_at_loopback_only() {
        let ctrl = InMemoryNetworkPolicyController::new_with_posture(NetworkPosture::LoopbackOnly);
        assert_eq!(ctrl.current_posture().await, NetworkPosture::LoopbackOnly);
    }

    // ── posture ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn set_posture_records_receipt() {
        let ctrl = InMemoryNetworkPolicyController::new();
        let receipt = ctrl
            .set_posture(NetworkPosture::Airgap, SubjectId("human:op".into()))
            .await
            .unwrap();
        assert_eq!(receipt.from, NetworkPosture::LanLocal);
        assert_eq!(receipt.to, NetworkPosture::Airgap);
        assert_eq!(receipt.actor, SubjectId("human:op".into()));
        assert_eq!(ctrl.current_posture().await, NetworkPosture::Airgap);
    }

    #[tokio::test]
    async fn posture_history_after_3_changes_returns_3_receipts() {
        let ctrl = InMemoryNetworkPolicyController::new();
        let actor = SubjectId("human:op".into());
        ctrl.set_posture(NetworkPosture::Airgap, actor.clone())
            .await
            .unwrap();
        ctrl.set_posture(NetworkPosture::LoopbackOnly, actor.clone())
            .await
            .unwrap();
        ctrl.set_posture(NetworkPosture::LanLocal, actor.clone())
            .await
            .unwrap();
        let history = ctrl.posture_history().await;
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].to, NetworkPosture::Airgap);
        assert_eq!(history[1].to, NetworkPosture::LoopbackOnly);
        assert_eq!(history[2].to, NetworkPosture::LanLocal);
    }

    // ── subject directives ────────────────────────────────────────────────────

    #[tokio::test]
    async fn subject_directive_with_no_rule_returns_deny_all() {
        let ctrl = InMemoryNetworkPolicyController::new();
        assert_eq!(
            ctrl.subject_directive(&SubjectId("unknown:subject".into()))
                .await,
            OutboundDirective::DenyAll
        );
    }

    #[tokio::test]
    async fn set_subject_directive_then_get_round_trip() {
        let ctrl = InMemoryNetworkPolicyController::new();
        let subj = SubjectId("human:lucky".into());
        ctrl.set_subject_directive(
            subj.clone(),
            OutboundDirective::AllowInternet,
            SubjectId("human:op".into()),
        )
        .await
        .unwrap();
        assert_eq!(
            ctrl.subject_directive(&subj).await,
            OutboundDirective::AllowInternet
        );
    }

    #[tokio::test]
    async fn list_directives_after_2_sets_returns_2() {
        let ctrl = InMemoryNetworkPolicyController::new();
        let actor = SubjectId("human:op".into());
        ctrl.set_subject_directive(
            SubjectId("human:a".into()),
            OutboundDirective::AllowInternet,
            actor.clone(),
        )
        .await
        .unwrap();
        ctrl.set_subject_directive(
            SubjectId("agent:b".into()),
            OutboundDirective::AllowLoopbackOnly,
            actor,
        )
        .await
        .unwrap();
        let dirs = ctrl.list_directives().await;
        assert_eq!(dirs.len(), 2);
    }

    #[tokio::test]
    async fn revoke_subject_directive_then_get_returns_deny_all() {
        let ctrl = InMemoryNetworkPolicyController::new();
        let subj = SubjectId("human:tmp".into());
        let actor = SubjectId("human:op".into());
        ctrl.set_subject_directive(
            subj.clone(),
            OutboundDirective::AllowInternet,
            actor.clone(),
        )
        .await
        .unwrap();
        ctrl.revoke_subject_directive(&subj, actor).await.unwrap();
        assert_eq!(
            ctrl.subject_directive(&subj).await,
            OutboundDirective::DenyAll
        );
    }

    #[tokio::test]
    async fn revoke_unknown_subject_is_idempotent_ok() {
        let ctrl = InMemoryNetworkPolicyController::new();
        let result = ctrl
            .revoke_subject_directive(
                &SubjectId("never:existed".into()),
                SubjectId("human:op".into()),
            )
            .await;
        assert!(result.is_ok());
    }

    // ── connection evaluation (T-152 stub) ────────────────────────────────────

    #[tokio::test]
    async fn evaluate_connection_with_deny_all_subject_returns_denied_default_deny() {
        let ctrl = InMemoryNetworkPolicyController::new();
        let req = EvaluateConnectionRequest {
            subject: SubjectId("unknown:subj".into()),
            destination: "example.com:443".into(),
            protocol: ProtocolFamily::Tcp,
        };
        let decision = ctrl.evaluate_connection(req).await.unwrap();
        assert_eq!(
            decision,
            ConnectionDecision::Denied {
                code: NetworkPolicyErrorCode::DefaultDeny
            }
        );
    }

    #[tokio::test]
    async fn evaluate_connection_with_allow_loopback_only_on_loopback_returns_allowed() {
        let ctrl = InMemoryNetworkPolicyController::new();
        let subj = SubjectId("service:loopback-svc".into());
        let actor = SubjectId("human:op".into());
        ctrl.set_subject_directive(subj.clone(), OutboundDirective::AllowLoopbackOnly, actor)
            .await
            .unwrap();

        let req = EvaluateConnectionRequest {
            subject: subj,
            destination: "127.0.0.1:8080".into(),
            protocol: ProtocolFamily::Tcp,
        };
        let decision = ctrl.evaluate_connection(req).await.unwrap();
        assert_eq!(
            decision,
            ConnectionDecision::Allowed {
                matched_rule_id: "allow-loopback".into()
            }
        );
    }

    #[tokio::test]
    async fn evaluate_connection_with_allow_loopback_only_non_loopback_returns_denied() {
        let ctrl = InMemoryNetworkPolicyController::new();
        let subj = SubjectId("service:loopback-svc".into());
        let actor = SubjectId("human:op".into());
        ctrl.set_subject_directive(subj.clone(), OutboundDirective::AllowLoopbackOnly, actor)
            .await
            .unwrap();

        let req = EvaluateConnectionRequest {
            subject: subj,
            destination: "192.168.1.1:443".into(),
            protocol: ProtocolFamily::Tcp,
        };
        let decision = ctrl.evaluate_connection(req).await.unwrap();
        assert_eq!(
            decision,
            ConnectionDecision::Denied {
                code: NetworkPolicyErrorCode::DefaultDeny
            }
        );
    }

    #[tokio::test]
    async fn evaluate_connection_loopback_localhost_returns_allowed() {
        let ctrl = InMemoryNetworkPolicyController::new();
        let subj = SubjectId("service:lb".into());
        ctrl.set_subject_directive(
            subj.clone(),
            OutboundDirective::AllowLoopbackOnly,
            SubjectId("human:op".into()),
        )
        .await
        .unwrap();

        let req = EvaluateConnectionRequest {
            subject: subj,
            destination: "localhost:3000".into(),
            protocol: ProtocolFamily::Tcp,
        };
        let decision = ctrl.evaluate_connection(req).await.unwrap();
        assert_eq!(
            decision,
            ConnectionDecision::Allowed {
                matched_rule_id: "allow-loopback".into()
            }
        );
    }

    #[tokio::test]
    async fn evaluate_connection_loopback_bracketed_ipv6_returns_allowed() {
        let ctrl = InMemoryNetworkPolicyController::new();
        let subj = SubjectId("service:lb6".into());
        ctrl.set_subject_directive(
            subj.clone(),
            OutboundDirective::AllowLoopbackOnly,
            SubjectId("human:op".into()),
        )
        .await
        .unwrap();

        let req = EvaluateConnectionRequest {
            subject: subj,
            destination: "[::1]:9090".into(),
            protocol: ProtocolFamily::Tcp,
        };
        let decision = ctrl.evaluate_connection(req).await.unwrap();
        assert_eq!(
            decision,
            ConnectionDecision::Allowed {
                matched_rule_id: "allow-loopback".into()
            }
        );
    }

    #[tokio::test]
    async fn evaluate_connection_with_allow_internet_returns_allowed_with_rule_id() {
        let ctrl = InMemoryNetworkPolicyController::new();
        let subj = SubjectId("human:lucky".into());
        ctrl.set_subject_directive(
            subj.clone(),
            OutboundDirective::AllowInternet,
            SubjectId("human:op".into()),
        )
        .await
        .unwrap();

        let req = EvaluateConnectionRequest {
            subject: subj,
            destination: "example.com:443".into(),
            protocol: ProtocolFamily::Tcp,
        };
        let decision = ctrl.evaluate_connection(req).await.unwrap();
        assert_eq!(
            decision,
            ConnectionDecision::Allowed {
                matched_rule_id: "allow-internet".into()
            }
        );
    }

    #[tokio::test]
    async fn evaluate_connection_with_allow_list_only_returns_denied_in_t152_stub() {
        let ctrl = InMemoryNetworkPolicyController::new();
        let subj = SubjectId("service:allowlisted".into());
        ctrl.set_subject_directive(
            subj.clone(),
            OutboundDirective::AllowListOnly {
                allowlist_id: "wl-01".into(),
            },
            SubjectId("human:op".into()),
        )
        .await
        .unwrap();

        let req = EvaluateConnectionRequest {
            subject: subj,
            destination: "example.com:443".into(),
            protocol: ProtocolFamily::Tcp,
        };
        let decision = ctrl.evaluate_connection(req).await.unwrap();
        assert_eq!(
            decision,
            ConnectionDecision::Denied {
                code: NetworkPolicyErrorCode::DefaultDeny
            }
        );
    }

    #[tokio::test]
    async fn evaluate_connection_with_allow_vpn_only_returns_denied_in_t152_stub() {
        let ctrl = InMemoryNetworkPolicyController::new();
        let subj = SubjectId("service:vpn-client".into());
        ctrl.set_subject_directive(
            subj.clone(),
            OutboundDirective::AllowVpnOnly {
                tunnel_id: "vpn-01".into(),
            },
            SubjectId("human:op".into()),
        )
        .await
        .unwrap();

        let req = EvaluateConnectionRequest {
            subject: subj,
            destination: "10.0.0.1:443".into(),
            protocol: ProtocolFamily::Udp,
        };
        let decision = ctrl.evaluate_connection(req).await.unwrap();
        assert_eq!(
            decision,
            ConnectionDecision::Denied {
                code: NetworkPolicyErrorCode::DefaultDeny
            }
        );
    }

    // ── concurrency ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn concurrent_set_directive_3_subjects_no_panic() {
        use std::sync::Arc;

        let ctrl = Arc::new(InMemoryNetworkPolicyController::new());
        let actor = SubjectId("human:op".into());

        let mut handles = Vec::new();
        for i in 0..3 {
            let ctrl = Arc::clone(&ctrl);
            let actor = actor.clone();
            handles.push(tokio::spawn(async move {
                ctrl.set_subject_directive(
                    SubjectId(format!("subject:{i}")),
                    OutboundDirective::AllowInternet,
                    actor,
                )
                .await
                .unwrap();
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        let dirs = ctrl.list_directives().await;
        assert_eq!(dirs.len(), 3);
    }
}
