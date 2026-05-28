//! AI cross-origin discipline gate (S8.1 §4.9, INV I4).
//!
//! [`AiCrossOriginGate`] enforces INV I4 — AI subjects (id prefix `"agent:"` / `"ai:"`)
//! never reach the public internet directly. Per-AI-subject [`AICrossOriginPosture`]
//! is set by HUMAN or `_system` subjects only; AI cannot set its own posture.
//! Default for unknown AI subjects is `DenyAllExternal`. Allowed external calls
//! go through one of: VaultBroker (handle must match registered broker fingerprint)
//! or OperatorMediated (`operator_approval_id` required per call). Non-AI subjects
//! bypass this gate (`Allowed { via: BypassedNonAi }`).

use std::collections::HashMap;

use tokio::sync::RwLock;

use crate::ai_cross_origin::AICrossOriginPosture;
use crate::error::NetworkPolicyError;
use crate::ids::SubjectId;

/// Request to evaluate whether an AI subject may make an external call.
#[derive(Debug, Clone)]
pub struct AiExternalCallRequest {
    /// The subject requesting the external call.
    pub subject: SubjectId,
    /// The destination endpoint (e.g. `"https://api.openai.com/v1"`).
    pub endpoint: String,
    /// Vault Broker capability handle, required for [`AICrossOriginPosture::VaultBrokeredOnly`].
    pub broker_handle: Option<String>,
    /// Operator approval identifier, required for [`AICrossOriginPosture::OperatorMediated`].
    pub operator_approval_id: Option<String>,
}

/// The gate's decision on an external call request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiExternalCallDecision {
    /// The call is allowed through a specific pathway.
    Allowed {
        /// How the call was authorised.
        via: AllowedVia,
    },
}

/// The authorisation pathway through which an external call was allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllowedVia {
    /// The subject is not an AI subject — gate does not apply.
    BypassedNonAi,
    /// Call routed through a registered Vault Broker.
    VaultBroker {
        /// The Vault Broker capability handle.
        broker_handle: String,
        /// The Ed25519 signer fingerprint of the broker authority.
        signer_fingerprint: String,
    },
    /// Call approved by an operator with a traceable approval identifier.
    OperatorMediated {
        /// The operator's canonical subject ID.
        operator: String,
        /// The approval identifier.
        approval_id: String,
    },
}

/// Classifies a subject as AI based on configurable ID prefixes.
pub struct AiSubjectClassifier {
    ai_subject_prefixes: Vec<String>,
}

impl AiSubjectClassifier {
    /// Creates a classifier with default AI prefixes `["agent:", "ai:"]`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ai_subject_prefixes: vec!["agent:".to_string(), "ai:".to_string()],
        }
    }

    /// Creates a classifier with custom AI prefixes.
    #[must_use]
    pub const fn with_prefixes(prefixes: Vec<String>) -> Self {
        Self {
            ai_subject_prefixes: prefixes,
        }
    }

    /// Returns `true` if the subject's ID starts with any AI prefix.
    #[must_use]
    pub fn is_ai(&self, subject: &SubjectId) -> bool {
        self.ai_subject_prefixes
            .iter()
            .any(|prefix| subject.0.starts_with(prefix.as_str()))
    }
}

impl Default for AiSubjectClassifier {
    fn default() -> Self {
        Self::new()
    }
}

/// The AI cross-origin discipline gate.
///
/// Holds per-AI-subject postures (read-write via [`RwLock`]) and a broker
/// authority registry (plain [`HashMap`], mutated exclusively via
/// [`Self::register_broker`] which requires `&mut self`).
pub struct AiCrossOriginGate {
    classifier: AiSubjectClassifier,
    postures: RwLock<HashMap<SubjectId, AICrossOriginPosture>>,
    broker_handle_authority: HashMap<String, String>,
}

impl AiCrossOriginGate {
    /// Creates a new gate with the given classifier, empty posture map,
    /// and empty broker authority registry.
    #[must_use]
    pub fn new(classifier: AiSubjectClassifier) -> Self {
        Self {
            classifier,
            postures: RwLock::new(HashMap::new()),
            broker_handle_authority: HashMap::new(),
        }
    }

    /// Sets the AI cross-origin posture for `subject`.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkPolicyError::Internal`] if:
    /// - `setter` is itself an AI subject (AI cannot set its own posture).
    /// - `subject` is not an AI subject (postures only apply to AI).
    pub async fn set_posture(
        &self,
        subject: SubjectId,
        posture: AICrossOriginPosture,
        setter: SubjectId,
    ) -> Result<(), NetworkPolicyError> {
        if self.classifier.is_ai(&setter) {
            return Err(NetworkPolicyError::Internal(
                "AI cannot set its own posture".into(),
            ));
        }
        if !self.classifier.is_ai(&subject) {
            return Err(NetworkPolicyError::Internal(
                "posture target must be AI subject".into(),
            ));
        }
        let mut guard = self.postures.write().await;
        guard.insert(subject, posture);
        drop(guard);
        Ok(())
    }

    /// Returns the current posture for `subject`, defaulting to
    /// [`AICrossOriginPosture::DenyAllExternal`] when no posture has been
    /// explicitly set (INV I4 + INV I1 default-deny).
    pub async fn get_posture(&self, subject: &SubjectId) -> AICrossOriginPosture {
        let guard = self.postures.read().await;
        let posture = guard
            .get(subject)
            .cloned()
            .unwrap_or(AICrossOriginPosture::DenyAllExternal);
        drop(guard);
        posture
    }

    /// Registers a Vault Broker handle with its Ed25519 signer fingerprint.
    pub fn register_broker(&mut self, broker_handle: String, signer_fingerprint: String) {
        self.broker_handle_authority
            .insert(broker_handle, signer_fingerprint);
    }

    /// Evaluates whether `req.subject` may make an external call to
    /// `req.endpoint` under its current AI cross-origin posture.
    ///
    /// Non-AI subjects bypass this gate immediately with
    /// [`AllowedVia::BypassedNonAi`].
    ///
    /// # Errors
    ///
    /// Returns [`NetworkPolicyError::AiDirectInternetDenied`] when an AI
    /// subject attempts a call that is not permitted under its posture.
    pub async fn evaluate_external_call(
        &self,
        req: AiExternalCallRequest,
    ) -> Result<AiExternalCallDecision, NetworkPolicyError> {
        if !self.classifier.is_ai(&req.subject) {
            return Ok(AiExternalCallDecision::Allowed {
                via: AllowedVia::BypassedNonAi,
            });
        }

        let posture = self.get_posture(&req.subject).await;

        match posture {
            AICrossOriginPosture::DenyAllExternal => {
                Err(NetworkPolicyError::AiDirectInternetDenied {
                    subject: req.subject,
                    attempted_endpoint: req.endpoint,
                })
            }
            AICrossOriginPosture::VaultBrokeredOnly { broker_handle } => {
                match req.broker_handle.as_deref() {
                    Some(h) if h == broker_handle.as_str() => {}
                    _ => {
                        return Err(NetworkPolicyError::AiDirectInternetDenied {
                            subject: req.subject,
                            attempted_endpoint: req.endpoint,
                        });
                    }
                }
                let fingerprint = self.broker_handle_authority.get(&broker_handle).cloned();
                match fingerprint {
                    Some(signer_fingerprint) => Ok(AiExternalCallDecision::Allowed {
                        via: AllowedVia::VaultBroker {
                            broker_handle,
                            signer_fingerprint,
                        },
                    }),
                    None => Err(NetworkPolicyError::AiDirectInternetDenied {
                        subject: req.subject,
                        attempted_endpoint: req.endpoint,
                    }),
                }
            }
            AICrossOriginPosture::OperatorMediated {
                operator_canonical_id,
            } => match req.operator_approval_id {
                Some(approval_id) => Ok(AiExternalCallDecision::Allowed {
                    via: AllowedVia::OperatorMediated {
                        operator: operator_canonical_id,
                        approval_id,
                    },
                }),
                None => Err(NetworkPolicyError::AiDirectInternetDenied {
                    subject: req.subject,
                    attempted_endpoint: req.endpoint,
                }),
            },
        }
    }
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

    fn agent_subject() -> SubjectId {
        SubjectId("agent:planner-7".into())
    }

    fn human_subject() -> SubjectId {
        SubjectId("human:lucky".into())
    }

    #[test]
    fn classifier_default_recognises_agent_and_ai_prefixes() {
        let c = AiSubjectClassifier::new();
        assert!(c.is_ai(&SubjectId("agent:foo".into())));
        assert!(c.is_ai(&SubjectId("ai:bar".into())));
    }

    #[test]
    fn classifier_with_custom_prefixes() {
        let c = AiSubjectClassifier::with_prefixes(vec!["bot:".into()]);
        assert!(c.is_ai(&SubjectId("bot:hal".into())));
        assert!(!c.is_ai(&SubjectId("agent:x".into())));
    }

    #[test]
    fn classifier_does_not_recognise_human_subject() {
        let c = AiSubjectClassifier::new();
        assert!(!c.is_ai(&SubjectId("human:alice".into())));
        assert!(!c.is_ai(&SubjectId("_system:kernel".into())));
    }

    // -- set_posture / get_posture ---------------------------------------

    #[tokio::test]
    async fn set_posture_by_human_subject_succeeds() {
        let gate = AiCrossOriginGate::new(AiSubjectClassifier::new());
        let subj = agent_subject();
        gate.set_posture(
            subj.clone(),
            AICrossOriginPosture::DenyAllExternal,
            human_subject(),
        )
        .await
        .unwrap();
        assert_eq!(
            gate.get_posture(&subj).await,
            AICrossOriginPosture::DenyAllExternal
        );
    }

    #[tokio::test]
    async fn set_posture_by_ai_subject_returns_internal_error() {
        let gate = AiCrossOriginGate::new(AiSubjectClassifier::new());
        let err = gate
            .set_posture(
                agent_subject(),
                AICrossOriginPosture::DenyAllExternal,
                agent_subject(),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, NetworkPolicyError::Internal(_)));
    }

    #[tokio::test]
    async fn set_posture_for_human_target_returns_internal_error() {
        let gate = AiCrossOriginGate::new(AiSubjectClassifier::new());
        let err = gate
            .set_posture(
                human_subject(),
                AICrossOriginPosture::DenyAllExternal,
                human_subject(),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, NetworkPolicyError::Internal(_)));
    }

    #[tokio::test]
    async fn get_posture_for_unknown_ai_subject_returns_deny_all_external() {
        let gate = AiCrossOriginGate::new(AiSubjectClassifier::new());
        assert_eq!(
            gate.get_posture(&agent_subject()).await,
            AICrossOriginPosture::DenyAllExternal
        );
    }

    // -- evaluate_external_call -------------------------------------------

    #[tokio::test]
    async fn evaluate_external_call_non_ai_subject_returns_allowed_bypassed_non_ai() {
        let gate = AiCrossOriginGate::new(AiSubjectClassifier::new());
        let decision = gate
            .evaluate_external_call(AiExternalCallRequest {
                subject: human_subject(),
                endpoint: "https://example.com".into(),
                broker_handle: None,
                operator_approval_id: None,
            })
            .await
            .unwrap();
        assert_eq!(
            decision,
            AiExternalCallDecision::Allowed {
                via: AllowedVia::BypassedNonAi
            }
        );
    }

    #[tokio::test]
    async fn evaluate_external_call_ai_with_deny_all_external_returns_ai_direct_internet_denied() {
        let gate = AiCrossOriginGate::new(AiSubjectClassifier::new());
        let err = gate
            .evaluate_external_call(AiExternalCallRequest {
                subject: agent_subject(),
                endpoint: "https://evil.com".into(),
                broker_handle: None,
                operator_approval_id: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            NetworkPolicyError::AiDirectInternetDenied { .. }
        ));
    }

    #[tokio::test]
    async fn evaluate_external_call_ai_with_vault_brokered_only_matching_handle_succeeds() {
        let mut gate = AiCrossOriginGate::new(AiSubjectClassifier::new());
        let subj = agent_subject();
        let handle = "vault://broker-1".to_string();
        let fingerprint = "abc123def".to_string();
        gate.register_broker(handle.clone(), fingerprint.clone());
        gate.set_posture(
            subj.clone(),
            AICrossOriginPosture::VaultBrokeredOnly {
                broker_handle: handle.clone(),
            },
            human_subject(),
        )
        .await
        .unwrap();

        let decision = gate
            .evaluate_external_call(AiExternalCallRequest {
                subject: subj,
                endpoint: "https://api.openai.com/v1".into(),
                broker_handle: Some(handle.clone()),
                operator_approval_id: None,
            })
            .await
            .unwrap();

        assert_eq!(
            decision,
            AiExternalCallDecision::Allowed {
                via: AllowedVia::VaultBroker {
                    broker_handle: handle,
                    signer_fingerprint: fingerprint,
                }
            }
        );
    }

    #[tokio::test]
    async fn evaluate_external_call_ai_with_vault_brokered_only_missing_handle_returns_ai_direct_internet_denied(
    ) {
        let gate = AiCrossOriginGate::new(AiSubjectClassifier::new());
        let subj = agent_subject();
        gate.set_posture(
            subj.clone(),
            AICrossOriginPosture::VaultBrokeredOnly {
                broker_handle: "vault://expect-broker".into(),
            },
            human_subject(),
        )
        .await
        .unwrap();
        let err = gate
            .evaluate_external_call(AiExternalCallRequest {
                subject: subj,
                endpoint: "https://api.openai.com/v1".into(),
                broker_handle: None,
                operator_approval_id: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            NetworkPolicyError::AiDirectInternetDenied { .. }
        ));
    }

    #[tokio::test]
    async fn evaluate_external_call_ai_with_vault_brokered_only_unknown_authority_returns_ai_direct_internet_denied(
    ) {
        let gate = AiCrossOriginGate::new(AiSubjectClassifier::new());
        let subj = agent_subject();
        let handle = "vault://unregistered".to_string();
        gate.set_posture(
            subj.clone(),
            AICrossOriginPosture::VaultBrokeredOnly {
                broker_handle: handle.clone(),
            },
            human_subject(),
        )
        .await
        .unwrap();

        let err = gate
            .evaluate_external_call(AiExternalCallRequest {
                subject: subj,
                endpoint: "https://api.openai.com/v1".into(),
                broker_handle: Some(handle),
                operator_approval_id: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            NetworkPolicyError::AiDirectInternetDenied { .. }
        ));
    }

    #[tokio::test]
    async fn evaluate_external_call_ai_with_operator_mediated_with_approval_succeeds() {
        let gate = AiCrossOriginGate::new(AiSubjectClassifier::new());
        let subj = agent_subject();
        let operator_id = "human:admin".to_string();
        gate.set_posture(
            subj.clone(),
            AICrossOriginPosture::OperatorMediated {
                operator_canonical_id: operator_id.clone(),
            },
            human_subject(),
        )
        .await
        .unwrap();

        let decision = gate
            .evaluate_external_call(AiExternalCallRequest {
                subject: subj,
                endpoint: "https://api.openai.com/v1".into(),
                broker_handle: None,
                operator_approval_id: Some("approval-42".into()),
            })
            .await
            .unwrap();

        assert_eq!(
            decision,
            AiExternalCallDecision::Allowed {
                via: AllowedVia::OperatorMediated {
                    operator: operator_id,
                    approval_id: "approval-42".into(),
                }
            }
        );
    }

    #[tokio::test]
    async fn evaluate_external_call_ai_with_operator_mediated_no_approval_returns_ai_direct_internet_denied(
    ) {
        let gate = AiCrossOriginGate::new(AiSubjectClassifier::new());
        let subj = agent_subject();
        gate.set_posture(
            subj.clone(),
            AICrossOriginPosture::OperatorMediated {
                operator_canonical_id: "human:admin".into(),
            },
            human_subject(),
        )
        .await
        .unwrap();

        let err = gate
            .evaluate_external_call(AiExternalCallRequest {
                subject: subj,
                endpoint: "https://api.openai.com/v1".into(),
                broker_handle: None,
                operator_approval_id: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            NetworkPolicyError::AiDirectInternetDenied { .. }
        ));
    }

    #[tokio::test]
    async fn register_broker_then_evaluate_call_uses_signer_fingerprint() {
        let mut gate = AiCrossOriginGate::new(AiSubjectClassifier::new());
        let subj = agent_subject();
        let handle = "vault://main".to_string();
        gate.register_broker(handle.clone(), "fp-main".into());
        gate.set_posture(
            subj.clone(),
            AICrossOriginPosture::VaultBrokeredOnly {
                broker_handle: handle.clone(),
            },
            human_subject(),
        )
        .await
        .unwrap();

        let decision = gate
            .evaluate_external_call(AiExternalCallRequest {
                subject: subj,
                endpoint: "https://api.anthropic.com/v1".into(),
                broker_handle: Some(handle.clone()),
                operator_approval_id: None,
            })
            .await
            .unwrap();

        assert_eq!(
            decision,
            AiExternalCallDecision::Allowed {
                via: AllowedVia::VaultBroker {
                    broker_handle: handle,
                    signer_fingerprint: "fp-main".into(),
                }
            }
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_set_5_postures_no_panic() {
        let gate = std::sync::Arc::new(AiCrossOriginGate::new(AiSubjectClassifier::new()));
        let human = human_subject();
        let mut handles = Vec::new();

        for i in 0..5 {
            let g = gate.clone();
            let h = human.clone();
            handles.push(tokio::spawn(async move {
                g.set_posture(
                    SubjectId(format!("agent:worker-{i}")),
                    AICrossOriginPosture::DenyAllExternal,
                    h,
                )
                .await
            }));
        }

        for h in handles {
            let _ = h.await.unwrap();
        }
    }
}
