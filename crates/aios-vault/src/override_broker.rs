//! Emergency override broker surface and in-memory implementation (S5.4).

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S5.4 override broker vocabulary"
)]
#![allow(
    clippy::significant_drop_tightening,
    reason = "the write guard is held through override state validation and mutation so consume/revoke decisions stay atomic"
)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use ulid::Ulid;

use aios_action::ActionId;

use crate::error::VaultError;
use crate::identity::{Subject, SubjectRef, SubjectType};
use crate::identity_catalog::IdentityCatalog;
use crate::override_class::{OverrideBinding, OverrideBindingState, OverrideClass};

/// Request to mint an emergency override binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantOverrideRequest {
    /// Strength tier for the override grant.
    pub class: OverrideClass,
    /// Subjects confirming the override.
    pub granted_by: Vec<SubjectRef>,
    /// Optional exact action bound to this override.
    pub target_action_id: Option<ActionId>,
    /// Hard expiry timestamp for the binding.
    pub expires_at: DateTime<Utc>,
    /// Operator justification text.
    pub reason: String,
}

/// S5.4 emergency override broker.
#[async_trait]
pub trait OverrideBroker: Send + Sync {
    /// Grant a new override binding.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::SubjectNotFound`] when any granting subject is
    /// unknown, [`VaultError::OverrideClassApproverCountMismatch`] when the
    /// class quorum is wrong, [`VaultError::AiCannotGrantOverride`] when an
    /// agent/application subject participates, or
    /// [`VaultError::OverrideRequiresHumanApprovers`] for other non-human
    /// grant subjects.
    async fn grant_override(
        &self,
        request: GrantOverrideRequest,
    ) -> Result<OverrideBinding, VaultError>;

    /// Atomically consume a live override binding.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::OverrideBindingNotFound`] when the binding is
    /// unknown, [`VaultError::OverrideAlreadyConsumed`] on replay, or
    /// [`VaultError::OverrideExpired`] when the binding's TTL has elapsed.
    async fn consume_override(
        &self,
        binding_id: &str,
        consumer: &SubjectRef,
    ) -> Result<OverrideBinding, VaultError>;

    /// Revoke an override binding in any current state.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::OverrideBindingNotFound`] when the binding is
    /// unknown.
    async fn revoke_override(
        &self,
        binding_id: &str,
        revoker: &SubjectRef,
    ) -> Result<(), VaultError>;

    /// Look up an override binding by id.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::OverrideBindingNotFound`] when the binding is
    /// unknown.
    async fn lookup_override(&self, binding_id: &str) -> Result<OverrideBinding, VaultError>;

    /// List override bindings granted by a subject.
    ///
    /// # Errors
    ///
    /// The in-memory implementation has no fallible path today, but the
    /// result remains fallible to match the broker contract.
    async fn list_overrides_for_subject(
        &self,
        subject: &SubjectRef,
    ) -> Result<Vec<OverrideBinding>, VaultError>;
}

/// HashMap-backed in-process [`OverrideBroker`].
#[derive(Debug)]
pub struct InMemoryOverrideBroker {
    overrides: RwLock<HashMap<String, OverrideBinding>>,
    catalog: Arc<IdentityCatalog>,
}

impl InMemoryOverrideBroker {
    /// Construct an empty in-memory override broker.
    #[must_use]
    pub fn new(catalog: Arc<IdentityCatalog>) -> Self {
        Self {
            overrides: RwLock::new(HashMap::new()),
            catalog,
        }
    }

    async fn lookup_granting_subjects(
        &self,
        granted_by: &[SubjectRef],
    ) -> Result<Vec<Subject>, VaultError> {
        let mut subjects = Vec::with_capacity(granted_by.len());
        for subject_ref in granted_by {
            subjects.push(self.catalog.lookup_subject(&subject_ref.0).await?);
        }
        Ok(subjects)
    }
}

#[async_trait]
impl OverrideBroker for InMemoryOverrideBroker {
    async fn grant_override(
        &self,
        request: GrantOverrideRequest,
    ) -> Result<OverrideBinding, VaultError> {
        let GrantOverrideRequest {
            class,
            granted_by,
            target_action_id,
            expires_at,
            reason: _reason,
        } = request;

        validate_approver_count(class, granted_by.len())?;
        let subjects = self.lookup_granting_subjects(&granted_by).await?;
        reject_ai_granting_subjects(&subjects)?;
        validate_human_approvers(class, &subjects)?;

        let now = Utc::now();
        let binding = OverrideBinding {
            binding_id: format!("ovr_{}", Ulid::new()),
            class,
            granted_by,
            granted_at: now,
            expires_at,
            target_action_id,
            state: OverrideBindingState::Granted,
        };

        self.overrides
            .write()
            .await
            .insert(binding.binding_id.clone(), binding.clone());

        Ok(binding)
    }

    async fn consume_override(
        &self,
        binding_id: &str,
        _consumer: &SubjectRef,
    ) -> Result<OverrideBinding, VaultError> {
        let mut overrides = self.overrides.write().await;
        let binding = overrides
            .get_mut(binding_id)
            .ok_or_else(|| VaultError::OverrideBindingNotFound(binding_id.to_owned()))?;

        match binding.state {
            OverrideBindingState::Granted => {}
            OverrideBindingState::Consumed => {
                return Err(VaultError::OverrideAlreadyConsumed);
            }
            OverrideBindingState::Expired => {
                return Err(VaultError::OverrideExpired(binding_id.to_owned()));
            }
            OverrideBindingState::Revoked => {
                return Err(VaultError::Internal(format!(
                    "override binding revoked: {binding_id}"
                )));
            }
        }

        if binding.expires_at <= Utc::now() {
            binding.state = OverrideBindingState::Expired;
            return Err(VaultError::OverrideExpired(binding_id.to_owned()));
        }

        binding.state = OverrideBindingState::Consumed;
        Ok(binding.clone())
    }

    async fn revoke_override(
        &self,
        binding_id: &str,
        _revoker: &SubjectRef,
    ) -> Result<(), VaultError> {
        let mut overrides = self.overrides.write().await;
        let binding = overrides
            .get_mut(binding_id)
            .ok_or_else(|| VaultError::OverrideBindingNotFound(binding_id.to_owned()))?;
        binding.state = OverrideBindingState::Revoked;
        Ok(())
    }

    async fn lookup_override(&self, binding_id: &str) -> Result<OverrideBinding, VaultError> {
        let mut overrides = self.overrides.write().await;
        let binding = overrides
            .get_mut(binding_id)
            .ok_or_else(|| VaultError::OverrideBindingNotFound(binding_id.to_owned()))?;
        transition_expired_binding(binding, Utc::now());
        Ok(binding.clone())
    }

    async fn list_overrides_for_subject(
        &self,
        subject: &SubjectRef,
    ) -> Result<Vec<OverrideBinding>, VaultError> {
        let now = Utc::now();
        let mut overrides = self.overrides.write().await;
        let mut bindings = Vec::new();
        for binding in overrides.values_mut() {
            transition_expired_binding(binding, now);
            if binding.granted_by.iter().any(|granter| granter == subject) {
                bindings.push(binding.clone());
            }
        }
        bindings.sort_by(|left, right| left.binding_id.cmp(&right.binding_id));
        Ok(bindings)
    }
}

fn validate_approver_count(class: OverrideClass, found: usize) -> Result<(), VaultError> {
    let expected = expected_approver_count(class);
    let found = count_to_u32(found);
    if found == expected {
        return Ok(());
    }

    Err(VaultError::OverrideClassApproverCountMismatch {
        class,
        expected,
        found,
    })
}

const fn expected_approver_count(class: OverrideClass) -> u32 {
    match class {
        OverrideClass::StrongSolo => 1,
        OverrideClass::DualHuman => 2,
        OverrideClass::TripleHuman => 3,
    }
}

fn count_to_u32(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

fn reject_ai_granting_subjects(subjects: &[Subject]) -> Result<(), VaultError> {
    if let Some(subject) = subjects.iter().find(|subject| {
        matches!(
            subject.subject_type,
            SubjectType::Agent | SubjectType::Application
        )
    }) {
        return Err(VaultError::AiCannotGrantOverride(
            subject.canonical_subject_id.clone(),
        ));
    }

    Ok(())
}

fn validate_human_approvers(class: OverrideClass, subjects: &[Subject]) -> Result<(), VaultError> {
    let found_non_human: Vec<String> = subjects
        .iter()
        .filter(|subject| subject.subject_type != SubjectType::Human)
        .map(|subject| subject.canonical_subject_id.clone())
        .collect();

    if found_non_human.is_empty() {
        return Ok(());
    }

    Err(VaultError::OverrideRequiresHumanApprovers {
        class,
        found_non_human,
    })
}

fn transition_expired_binding(binding: &mut OverrideBinding, now: DateTime<Utc>) {
    if binding.state == OverrideBindingState::Granted && binding.expires_at <= now {
        binding.state = OverrideBindingState::Expired;
    }
}
