//! Cross-crate compatibility shims for `aios-vault` consumers.
//!
//! The concrete integration code lives here so closed milestone crates such as
//! `aios-policy` do not gain a dependency on vault and do not need API changes.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names include the integration boundary they adapt"
)]

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::VaultError;
use crate::hydrator::{HydratedSubjectSnapshot, VaultSubjectHydrator};
use crate::identity::{Subject, SubjectType};
use crate::identity_catalog::IdentityCatalog;
use crate::override_broker::InMemoryOverrideBroker;
use crate::override_class::{OverrideBinding, OverrideBindingState, OverrideClass};

impl From<Subject> for HydratedSubjectSnapshot {
    fn from(subject: Subject) -> Self {
        Self {
            canonical_subject_id: subject.canonical_subject_id,
            subject_type: subject.subject_type,
            groups: subject.groups,
            capabilities: vec![],
            session_class: session_class_for_subject_type(subject.subject_type).to_owned(),
            recovery_mode: recovery_mode_for_subject_type(subject.subject_type),
            is_ai: subject.is_ai,
        }
    }
}

impl From<HydratedSubjectSnapshot> for aios_policy::HydratedSubject {
    fn from(snapshot: HydratedSubjectSnapshot) -> Self {
        Self {
            canonical_subject_id: snapshot.canonical_subject_id,
            subject_type: policy_subject_type(snapshot.subject_type),
            groups: snapshot.groups,
            capabilities: snapshot.capabilities,
            session_class: snapshot.session_class,
            recovery_mode: snapshot.recovery_mode,
            is_ai: snapshot.is_ai,
        }
    }
}

impl From<OverrideBinding> for aios_policy::override_boundary::EmergencyOverride {
    fn from(binding: OverrideBinding) -> Self {
        Self {
            override_id: binding.binding_id,
            // Vault supports quorum grants; policy's T-025 receipt stores one
            // string, so the shim preserves every grantor in grant order.
            granted_by_subject_id: binding
                .granted_by
                .iter()
                .map(|subject| subject.0.as_str())
                .collect::<Vec<_>>()
                .join(","),
            granted_at: binding.granted_at,
            expires_at: binding.expires_at,
            scope: aios_policy::override_boundary::OverrideScope {
                // Vault bindings are action-bound, not policy-rule-bound.
                // Empty rule id is policy's broad "all scoped rules" shape.
                rule_id: String::new(),
                // `target_action_id = None` is a vault wildcard. The policy
                // receipt has no wildcard field, so the persisted conversion
                // uses an empty action while `VaultPolicyOverrideBoundary`
                // applies the wildcard during lookup.
                action: binding
                    .target_action_id
                    .as_ref()
                    .map_or_else(String::new, ToString::to_string),
                subjects: vec![],
            },
            reason: format!(
                "vault override binding class={}",
                override_class_token(binding.class)
            ),
            revoked: binding.state != OverrideBindingState::Granted,
        }
    }
}

/// Vault-backed adapter for `aios_policy::SubjectHydrator`.
#[derive(Debug, Clone)]
pub struct VaultPolicyHydrator {
    catalog: Arc<IdentityCatalog>,
}

impl VaultPolicyHydrator {
    /// Construct a policy hydrator over a shared vault identity catalog.
    #[must_use]
    pub const fn new(catalog: Arc<IdentityCatalog>) -> Self {
        Self { catalog }
    }

    /// Return the shared identity catalog handle.
    #[must_use]
    pub fn catalog(&self) -> Arc<IdentityCatalog> {
        Arc::clone(&self.catalog)
    }

    fn vault_hydrator(&self) -> VaultSubjectHydrator {
        VaultSubjectHydrator::new(Arc::clone(&self.catalog))
    }
}

#[async_trait]
impl aios_policy::SubjectHydrator for VaultPolicyHydrator {
    async fn hydrate(
        &self,
        provisional: &str,
    ) -> Result<aios_policy::HydratedSubject, aios_policy::PolicyError> {
        let hydrator = self.vault_hydrator();

        match hydrator.hydrate_by_session(provisional).await {
            Ok(snapshot) => return Ok(snapshot.into()),
            Err(err) if should_try_canonical_lookup(&err) => {}
            Err(_) => return Err(aios_policy::PolicyError::SubjectUnauthenticated),
        }

        hydrator
            .hydrate_by_canonical_id(provisional)
            .await
            .map(Into::into)
            .map_err(|_| aios_policy::PolicyError::SubjectUnauthenticated)
    }
}

/// Compatibility lookup shim over vault override bindings.
#[derive(Debug, Clone)]
pub struct VaultPolicyOverrideBoundary {
    broker: Arc<InMemoryOverrideBroker>,
}

impl VaultPolicyOverrideBoundary {
    /// Construct a policy-shaped override lookup boundary over a vault broker.
    #[must_use]
    pub const fn new(broker: Arc<InMemoryOverrideBroker>) -> Self {
        Self { broker }
    }

    /// Return the wrapped vault broker handle.
    #[must_use]
    pub fn broker(&self) -> Arc<InMemoryOverrideBroker> {
        Arc::clone(&self.broker)
    }

    /// Look up an active vault override for `action`.
    ///
    /// The `subject_canonical_id` parameter mirrors
    /// `aios_policy::OverrideBoundary::is_overridden`. Vault
    /// `OverrideBinding` records do not carry a target-subject list today, so
    /// the shim matches only the bound action id and active TTL/state.
    #[must_use]
    pub async fn is_overridden(
        &self,
        action: &str,
        subject_canonical_id: &str,
    ) -> Option<aios_policy::override_boundary::EmergencyOverride> {
        self.is_overridden_at(action, subject_canonical_id, Utc::now())
            .await
    }

    /// Deterministic-clock form of [`Self::is_overridden`].
    #[must_use]
    pub async fn is_overridden_at(
        &self,
        action: &str,
        _subject_canonical_id: &str,
        now: DateTime<Utc>,
    ) -> Option<aios_policy::override_boundary::EmergencyOverride> {
        self.broker
            .list_overrides()
            .await
            .into_iter()
            .find(|binding| binding_matches_action(binding, action, now))
            .map(Into::into)
    }
}

fn should_try_canonical_lookup(err: &VaultError) -> bool {
    matches!(err, VaultError::Internal(message) if message.starts_with("session not found:"))
}

const fn policy_subject_type(subject_type: SubjectType) -> aios_policy::SubjectType {
    match subject_type {
        SubjectType::Human => aios_policy::SubjectType::Human,
        SubjectType::Agent => aios_policy::SubjectType::Agent,
        SubjectType::Application => aios_policy::SubjectType::Application,
        SubjectType::Service => aios_policy::SubjectType::Service,
        SubjectType::Device => aios_policy::SubjectType::Device,
        SubjectType::Workflow => aios_policy::SubjectType::Workflow,
        // `aios-policy` has no distinct LocalOperator variant in T-025.
        // Vault local operators cross the policy boundary as recovery/admin
        // operators while `recovery_mode` keeps the local recovery context.
        SubjectType::RemoteOperator | SubjectType::LocalOperator => {
            aios_policy::SubjectType::RemoteOperator
        }
    }
}

const fn session_class_for_subject_type(subject_type: SubjectType) -> &'static str {
    match subject_type {
        SubjectType::Human | SubjectType::Device => "INTERACTIVE",
        SubjectType::Agent
        | SubjectType::Application
        | SubjectType::Service
        | SubjectType::Workflow => "SERVICE",
        SubjectType::RemoteOperator | SubjectType::LocalOperator => "RECOVERY",
    }
}

const fn recovery_mode_for_subject_type(subject_type: SubjectType) -> bool {
    matches!(
        subject_type,
        SubjectType::RemoteOperator | SubjectType::LocalOperator
    )
}

fn binding_matches_action(binding: &OverrideBinding, action: &str, now: DateTime<Utc>) -> bool {
    binding.state == OverrideBindingState::Granted
        && binding.expires_at > now
        && binding
            .target_action_id
            .as_ref()
            .is_none_or(|target_action_id| target_action_id.as_str() == action)
}

const fn override_class_token(class: OverrideClass) -> &'static str {
    match class {
        OverrideClass::StrongSolo => "STRONG_SOLO",
        OverrideClass::DualHuman => "DUAL_HUMAN",
        OverrideClass::TripleHuman => "TRIPLE_HUMAN",
    }
}
