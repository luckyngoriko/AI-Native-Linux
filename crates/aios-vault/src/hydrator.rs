//! Vault-backed subject hydration snapshot for the future policy integration.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S5.1 subject hydration vocabulary"
)]

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::VaultError;
use crate::identity::{SessionState, Subject, SubjectType};
use crate::identity_catalog::IdentityCatalog;

/// Hydrated subject shape mirrored locally until T-054 wires `aios-policy`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct HydratedSubjectSnapshot {
    /// Stable canonical subject id.
    pub canonical_subject_id: String,
    /// Closed subject taxonomy.
    pub subject_type: SubjectType,
    /// Group memberships used by policy subject matchers.
    pub groups: Vec<String>,
    /// Vault-granted capability ids active for this subject.
    pub capabilities: Vec<String>,
    /// S5.1 session class name.
    pub session_class: String,
    /// Whether this subject is operating under recovery mode.
    pub recovery_mode: bool,
    /// Identity-service-bound AI classification.
    pub is_ai: bool,
}

/// Subject hydrator backed by an [`IdentityCatalog`].
#[derive(Debug, Clone)]
pub struct VaultSubjectHydrator {
    catalog: Arc<IdentityCatalog>,
}

impl VaultSubjectHydrator {
    /// Construct a hydrator over a shared identity catalog.
    #[must_use]
    pub const fn new(catalog: Arc<IdentityCatalog>) -> Self {
        Self { catalog }
    }

    /// Hydrate a subject through an active session id.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::SessionExpired`] for expired sessions,
    /// [`VaultError::SubjectNotFound`] when the session's subject is missing,
    /// or [`VaultError::Internal`] for non-active session states.
    pub async fn hydrate_by_session(
        &self,
        session_id: &str,
    ) -> Result<HydratedSubjectSnapshot, VaultError> {
        let session = self.catalog.lookup_session(session_id).await?;
        match session.state {
            SessionState::Active => {}
            SessionState::Expired => return Err(VaultError::SessionExpired(session_id.to_owned())),
            SessionState::Suspended | SessionState::Revoked => {
                return Err(VaultError::Internal(format!(
                    "session not active: {session_id}"
                )));
            }
        }

        let subject = self.catalog.lookup_subject(&session.subject_id).await?;
        self.snapshot_for_subject(&subject).await
    }

    /// Hydrate a subject directly from its canonical id.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::SubjectNotFound`] when the canonical id is not
    /// registered.
    pub async fn hydrate_by_canonical_id(
        &self,
        canonical_id: &str,
    ) -> Result<HydratedSubjectSnapshot, VaultError> {
        let subject = self.catalog.lookup_subject(canonical_id).await?;
        self.snapshot_for_subject(&subject).await
    }

    async fn snapshot_for_subject(
        &self,
        subject: &Subject,
    ) -> Result<HydratedSubjectSnapshot, VaultError> {
        Ok(HydratedSubjectSnapshot {
            canonical_subject_id: subject.canonical_subject_id.clone(),
            subject_type: subject.subject_type,
            groups: self
                .catalog
                .groups_for_subject(&subject.canonical_subject_id)
                .await,
            capabilities: vec![],
            session_class: session_class_for_subject_type(subject.subject_type).to_owned(),
            recovery_mode: recovery_mode_for_subject_type(subject.subject_type),
            is_ai: subject.is_ai,
        })
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
