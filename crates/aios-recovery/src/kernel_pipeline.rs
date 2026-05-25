//! S9.3 dedicated-kernel candidate pipeline driver.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use tokio::sync::RwLock;

use crate::{
    CandidateId, CandidateState, KernelCandidate, KernelManifest, RecoveryBoundary, RecoveryError,
    RecoveryEvidenceEmitter,
};

const ROLLBACK_STACK_CAPACITY: usize = 8;

/// In-memory S9.3 lifecycle driver for dedicated-kernel candidates.
pub struct KernelPipelineDriver {
    candidates: RwLock<HashMap<CandidateId, KernelCandidate>>,
    active_id: RwLock<Option<CandidateId>>,
    trusted_authorities: HashMap<String, VerifyingKey>,
    boundary: Arc<dyn RecoveryBoundary>,
    rollback_stack: RwLock<Vec<CandidateId>>,
    evidence_emitter: Option<Arc<RecoveryEvidenceEmitter>>,
}

impl KernelPipelineDriver {
    /// Construct an empty driver bound to the supplied recovery boundary.
    #[must_use]
    pub fn new(boundary: Arc<dyn RecoveryBoundary>) -> Self {
        Self {
            candidates: RwLock::new(HashMap::new()),
            active_id: RwLock::new(None),
            trusted_authorities: HashMap::new(),
            boundary,
            rollback_stack: RwLock::new(Vec::new()),
            evidence_emitter: None,
        }
    }

    /// Construct an empty driver with evidence emission enabled.
    #[must_use]
    pub fn with_evidence_emitter(
        boundary: Arc<dyn RecoveryBoundary>,
        evidence_emitter: Arc<RecoveryEvidenceEmitter>,
    ) -> Self {
        let mut driver = Self::new(boundary);
        driver.evidence_emitter = Some(evidence_emitter);
        driver
    }

    /// Add one trusted kernel manifest signing authority.
    #[must_use]
    pub fn with_trusted_authority(mut self, name: String, key: VerifyingKey) -> Self {
        self.trusted_authorities.insert(name, key);
        self
    }

    /// Register a signed kernel manifest as a `BUILT` S9.3 candidate.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::KernelUnknownAuthority`] when no kernel trust
    /// root is configured, [`RecoveryError::KernelSignatureInvalid`] when the
    /// Ed25519 signature is malformed or rejected by every trusted key, and
    /// [`RecoveryError::Internal`] when the manifest cannot be serialized.
    pub async fn register_candidate(
        &self,
        manifest: KernelManifest,
        signature: Vec<u8>,
    ) -> Result<KernelCandidate, RecoveryError> {
        let body = canonical_manifest_body_bytes(&manifest)?;
        let signing_authority = self.verify_manifest_signature(&body, &signature)?;
        let candidate = KernelCandidate {
            candidate_id: CandidateId::new(),
            version: manifest.version.clone(),
            kernel_blake3: blake3::hash(&body).to_hex().to_string(),
            signature_ed25519: signature,
            signing_authority,
            registered_at: Utc::now(),
            state: CandidateState::Built,
            manifest,
        };

        self.candidates
            .write()
            .await
            .insert(candidate.candidate_id.clone(), candidate.clone());
        if let Some(emitter) = &self.evidence_emitter {
            emitter
                .emit_kernel_candidate_registered(&candidate, None)
                .await?;
        }
        Ok(candidate)
    }

    /// Run the current shallow verification stub and promote `BUILT` to `GATE_PASSED`.
    ///
    /// The deep six-gate verification is deferred; this driver still enforces
    /// the S9.3 FSM by applying `BUILT -> GATING -> GATE_PASSED`.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::CandidateNotFound`] for unknown ids or
    /// [`RecoveryError::InvalidCandidateTransition`] when the current state
    /// cannot enter verification.
    pub async fn verify_candidate(
        &self,
        candidate_id: &CandidateId,
    ) -> Result<KernelCandidate, RecoveryError> {
        let candidate = {
            let mut candidates = self.candidates.write().await;
            let candidate = candidates
                .get_mut(candidate_id)
                .ok_or_else(|| RecoveryError::CandidateNotFound(candidate_id.clone()))?;
            transition_candidate(candidate, CandidateState::Gating)?;
            transition_candidate(candidate, CandidateState::GatePassed)?;
            let candidate = candidate.clone();
            drop(candidates);
            candidate
        };
        if let Some(emitter) = &self.evidence_emitter {
            emitter.emit_kernel_gate_result(&candidate, None).await?;
        }
        Ok(candidate)
    }

    /// Promote a `GATE_PASSED` candidate to slot A.
    ///
    /// When the manifest marks the installation as recovery-only, S9.3
    /// promotion is admitted only while the S9.1 boundary reports recovery
    /// mode active. Any displaced active candidate is pushed onto the bounded
    /// rollback stack.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::RecoveryNotActive`] when a recovery-required
    /// promotion is attempted outside recovery, [`RecoveryError::CandidateNotFound`]
    /// for unknown ids, or [`RecoveryError::InvalidCandidateTransition`] when
    /// the candidate is not currently `GATE_PASSED`.
    pub async fn activate_candidate(
        &self,
        candidate_id: &CandidateId,
    ) -> Result<KernelCandidate, RecoveryError> {
        let requires_recovery_install = {
            let candidates = self.candidates.read().await;
            let candidate = candidates
                .get(candidate_id)
                .ok_or_else(|| RecoveryError::CandidateNotFound(candidate_id.clone()))?;
            let requires_recovery_install = candidate.manifest.requires_recovery_install;
            drop(candidates);
            requires_recovery_install
        };

        if requires_recovery_install && !self.boundary.is_recovery_active().await {
            return Err(RecoveryError::RecoveryNotActive);
        }

        let candidate = {
            let mut candidates = self.candidates.write().await;
            let mut active_id = self.active_id.write().await;
            let previous_active = active_id.clone();
            let candidate = candidates
                .get_mut(candidate_id)
                .ok_or_else(|| RecoveryError::CandidateNotFound(candidate_id.clone()))?;
            transition_candidate(candidate, CandidateState::APromoted)?;

            if let Some(previous_id) = previous_active.filter(|id| id != candidate_id) {
                let mut rollback_stack = self.rollback_stack.write().await;
                rollback_stack.push(previous_id);
                trim_rollback_stack(&mut rollback_stack);
                drop(rollback_stack);
            }
            *active_id = Some(candidate_id.clone());
            let candidate = candidate.clone();
            drop(active_id);
            drop(candidates);
            candidate
        };
        if let Some(emitter) = &self.evidence_emitter {
            emitter.emit_kernel_activated(&candidate, None).await?;
        }
        Ok(candidate)
    }

    /// Roll back the current active candidate and restore the prior active id.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::CandidateNotFound`] for unknown ids,
    /// [`RecoveryError::InvalidCandidateTransition`] when the candidate is not
    /// active in a rollback-capable state, or [`RecoveryError::Internal`] when
    /// there is no previous active candidate in the bounded rollback stack.
    pub async fn rollback_candidate(
        &self,
        candidate_id: &CandidateId,
    ) -> Result<KernelCandidate, RecoveryError> {
        let (candidate, previous_candidate_id) = {
            let mut candidates = self.candidates.write().await;
            let mut active_id = self.active_id.write().await;

            let candidate = candidates
                .get(candidate_id)
                .ok_or_else(|| RecoveryError::CandidateNotFound(candidate_id.clone()))?;
            if active_id.as_ref() != Some(candidate_id) {
                return Err(RecoveryError::InvalidCandidateTransition {
                    from: candidate.state,
                    to: CandidateState::Rollback,
                });
            }
            ensure_transition(candidate.state, CandidateState::Rollback)?;

            let previous_id = {
                let mut rollback_stack = self.rollback_stack.write().await;
                let previous_id = rollback_stack.pop().ok_or_else(|| {
                    RecoveryError::Internal(
                        "rollback requires previous active candidate".to_owned(),
                    )
                })?;
                drop(rollback_stack);
                previous_id
            };
            if !candidates.contains_key(&previous_id) {
                return Err(RecoveryError::Internal(format!(
                    "rollback stack points to missing candidate {previous_id}"
                )));
            }

            let candidate = candidates
                .get_mut(candidate_id)
                .ok_or_else(|| RecoveryError::CandidateNotFound(candidate_id.clone()))?;
            transition_candidate(candidate, CandidateState::Rollback)?;
            *active_id = Some(previous_id.clone());
            let candidate = candidate.clone();
            drop(active_id);
            drop(candidates);
            (candidate, previous_id)
        };
        if let Some(emitter) = &self.evidence_emitter {
            emitter
                .emit_kernel_rolled_back_to_previous(
                    &candidate,
                    previous_candidate_id,
                    "previous active restored",
                    None,
                )
                .await?;
        }
        Ok(candidate)
    }

    /// Retire an inactive candidate with an operator-supplied reason.
    ///
    /// The reason is accepted for the future evidence path; T-077 stores only
    /// the terminal state because evidence emission is deferred to T-080.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::CandidateNotFound`] for unknown ids or
    /// [`RecoveryError::InvalidCandidateTransition`] when the candidate is the
    /// current active slot A image or its state cannot transition to `RETIRED`.
    pub async fn retire_candidate(
        &self,
        candidate_id: &CandidateId,
        _reason: &str,
    ) -> Result<(), RecoveryError> {
        {
            let mut candidates = self.candidates.write().await;
            let active_id = self.active_id.read().await;
            let candidate = candidates
                .get_mut(candidate_id)
                .ok_or_else(|| RecoveryError::CandidateNotFound(candidate_id.clone()))?;

            if active_id.as_ref() == Some(candidate_id) {
                return Err(RecoveryError::InvalidCandidateTransition {
                    from: candidate.state,
                    to: CandidateState::Retired,
                });
            }
            transition_candidate(candidate, CandidateState::Retired)?;
            drop(active_id);
            drop(candidates);
        }
        Ok(())
    }

    /// Return a snapshot of all registered candidates.
    pub async fn list_candidates(&self) -> Vec<KernelCandidate> {
        self.candidates.read().await.values().cloned().collect()
    }

    /// Return the current active candidate, if any.
    pub async fn get_active(&self) -> Option<KernelCandidate> {
        let candidates = self.candidates.read().await;
        let active_id = self.active_id.read().await.clone()?;
        candidates.get(&active_id).cloned()
    }

    fn verify_manifest_signature(
        &self,
        body: &[u8],
        signature: &[u8],
    ) -> Result<String, RecoveryError> {
        if self.trusted_authorities.is_empty() {
            return Err(RecoveryError::KernelUnknownAuthority(
                "no trusted kernel authorities configured".to_owned(),
            ));
        }
        let sig_bytes: [u8; 64] = signature
            .try_into()
            .map_err(|_| RecoveryError::KernelSignatureInvalid)?;
        let signature = Signature::from_bytes(&sig_bytes);

        self.trusted_authorities
            .iter()
            .find_map(|(name, key)| key.verify(body, &signature).is_ok().then(|| name.clone()))
            .ok_or(RecoveryError::KernelSignatureInvalid)
    }
}

fn transition_candidate(
    candidate: &mut KernelCandidate,
    to: CandidateState,
) -> Result<(), RecoveryError> {
    ensure_transition(candidate.state, to)?;
    candidate.state = to;
    Ok(())
}

const fn ensure_transition(from: CandidateState, to: CandidateState) -> Result<(), RecoveryError> {
    if transition_allowed(from, to) {
        Ok(())
    } else {
        Err(RecoveryError::InvalidCandidateTransition { from, to })
    }
}

const fn transition_allowed(from: CandidateState, to: CandidateState) -> bool {
    matches!(
        (from, to),
        (CandidateState::Building, CandidateState::Built)
            | (CandidateState::Built, CandidateState::Gating)
            | (
                CandidateState::Gating,
                CandidateState::GatePassed | CandidateState::GateFailed,
            )
            | (
                CandidateState::GatePassed,
                CandidateState::APromoted | CandidateState::Retired,
            )
            | (
                CandidateState::APromoted,
                CandidateState::BDemotedToA | CandidateState::Rollback | CandidateState::Retired,
            )
            | (
                CandidateState::BDemotedToA,
                CandidateState::Rollback | CandidateState::Retired,
            )
            | (
                CandidateState::Rollback | CandidateState::GateFailed,
                CandidateState::Retired,
            )
    )
}

fn trim_rollback_stack(stack: &mut Vec<CandidateId>) {
    let overflow = stack.len().saturating_sub(ROLLBACK_STACK_CAPACITY);
    if overflow > 0 {
        stack.drain(0..overflow);
    }
}

fn canonical_manifest_body_bytes(manifest: &KernelManifest) -> Result<Vec<u8>, RecoveryError> {
    serde_json::to_vec(manifest)
        .map_err(|err| RecoveryError::Internal(format!("kernel manifest serialise: {err}")))
}
