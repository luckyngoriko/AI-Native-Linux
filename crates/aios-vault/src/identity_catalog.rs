//! In-memory identity catalog for S5.1 subject, session, and membership state.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S5.1 identity catalog vocabulary"
)]

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use ulid::Ulid;

use crate::error::VaultError;
use crate::identity::{Session, SessionState, Subject, SubjectType};

/// HashMap-backed identity catalog used by the vault opening slice.
#[derive(Debug)]
pub struct IdentityCatalog {
    subjects: RwLock<HashMap<String, Subject>>,
    sessions: RwLock<HashMap<String, Session>>,
    group_membership: RwLock<HashMap<String, Vec<String>>>,
}

impl Default for IdentityCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl IdentityCatalog {
    /// Construct an empty identity catalog.
    #[must_use]
    pub fn new() -> Self {
        Self {
            subjects: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
            group_membership: RwLock::new(HashMap::new()),
        }
    }

    /// Construct a catalog pre-loaded with the canonical T-048 fixture subjects.
    #[must_use]
    pub fn with_fixtures() -> Self {
        let now = Utc::now();
        let fixtures = [
            Subject {
                canonical_subject_id: "family:alice".to_owned(),
                subject_type: SubjectType::Human,
                provisional_name: "Alice".to_owned(),
                groups: vec!["family".to_owned()],
                is_ai: false,
                created_at: now,
            },
            Subject {
                canonical_subject_id: "agent:dev".to_owned(),
                subject_type: SubjectType::Agent,
                provisional_name: "Developer Agent".to_owned(),
                groups: vec!["agent".to_owned()],
                is_ai: true,
                created_at: now,
            },
            Subject {
                canonical_subject_id: "app:browser".to_owned(),
                subject_type: SubjectType::Application,
                provisional_name: "Browser".to_owned(),
                groups: vec!["app".to_owned()],
                is_ai: false,
                created_at: now,
            },
            Subject {
                canonical_subject_id: "_system:kwin".to_owned(),
                subject_type: SubjectType::Service,
                provisional_name: "KWin".to_owned(),
                groups: vec!["_system".to_owned()],
                is_ai: false,
                created_at: now,
            },
            Subject {
                canonical_subject_id: "operator:root".to_owned(),
                subject_type: SubjectType::LocalOperator,
                provisional_name: "Root Operator".to_owned(),
                groups: vec!["operator".to_owned()],
                is_ai: false,
                created_at: now,
            },
        ];

        let mut subjects = HashMap::new();
        let mut group_membership: HashMap<String, Vec<String>> = HashMap::new();
        for subject in fixtures {
            index_memberships(
                &mut group_membership,
                &subject.canonical_subject_id,
                &subject.groups,
            );
            subjects.insert(subject.canonical_subject_id.clone(), subject);
        }

        Self {
            subjects: RwLock::new(subjects),
            sessions: RwLock::new(HashMap::new()),
            group_membership: RwLock::new(group_membership),
        }
    }

    /// Register a new subject.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::SubjectAlreadyRegistered`] when the canonical id
    /// is already present.
    pub async fn register_subject(&self, subject: Subject) -> Result<(), VaultError> {
        let canonical_subject_id = subject.canonical_subject_id.clone();
        let groups = subject.groups.clone();

        {
            let mut subjects = self.subjects.write().await;
            if subjects.contains_key(&canonical_subject_id) {
                return Err(VaultError::SubjectAlreadyRegistered(canonical_subject_id));
            }
            subjects.insert(canonical_subject_id.clone(), subject);
        }

        let mut group_membership = self.group_membership.write().await;
        index_memberships(&mut group_membership, &canonical_subject_id, &groups);

        Ok(())
    }

    /// Look up a subject by canonical id.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::SubjectNotFound`] when no such subject is present.
    pub async fn lookup_subject(&self, canonical_id: &str) -> Result<Subject, VaultError> {
        let subjects = self.subjects.read().await;
        subjects
            .get(canonical_id)
            .cloned()
            .ok_or_else(|| VaultError::SubjectNotFound(canonical_id.to_owned()))
    }

    /// Start a new active session for an existing subject.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::SubjectNotFound`] when the subject is unknown, or
    /// [`VaultError::SessionAlreadyActive`] when the subject already has an
    /// unexpired active session.
    pub async fn start_session(
        &self,
        subject_id: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<Session, VaultError> {
        {
            let subjects = self.subjects.read().await;
            if !subjects.contains_key(subject_id) {
                return Err(VaultError::SubjectNotFound(subject_id.to_owned()));
            }
        }

        let now = Utc::now();
        let mut sessions = self.sessions.write().await;
        expire_past_sessions_for_subject(&mut sessions, subject_id, now);
        if sessions.values().any(|session| {
            session.subject_id == subject_id
                && session.state == SessionState::Active
                && session.expires_at >= now
        }) {
            return Err(VaultError::SessionAlreadyActive(subject_id.to_owned()));
        }

        let session = Session {
            session_id: format!("sess_{}", Ulid::new()),
            subject_id: subject_id.to_owned(),
            started_at: now,
            expires_at,
            state: SessionState::Active,
        };
        sessions.insert(session.session_id.clone(), session.clone());
        drop(sessions);

        Ok(session)
    }

    /// Look up a session and transition it to `Expired` when its expiry is past.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Internal`] when no such session is present. A
    /// dedicated session-not-found variant is outside the T-048 error scope.
    pub async fn lookup_session(&self, session_id: &str) -> Result<Session, VaultError> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| VaultError::Internal(format!("session not found: {session_id}")))?;

        transition_expired_session(session, Utc::now());
        let session = session.clone();
        drop(sessions);

        Ok(session)
    }

    /// Suspend an active session.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Internal`] when the session is unknown or not
    /// currently active.
    pub async fn suspend_session(&self, session_id: &str) -> Result<(), VaultError> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| VaultError::Internal(format!("session not found: {session_id}")))?;
        transition_expired_session(session, Utc::now());

        if session.state != SessionState::Active {
            return Err(VaultError::Internal(format!(
                "session not active: {session_id}"
            )));
        }

        session.state = SessionState::Suspended;
        drop(sessions);

        Ok(())
    }

    /// Revoke a session regardless of its current lifecycle state.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Internal`] when the session is unknown.
    pub async fn revoke_session(&self, session_id: &str) -> Result<(), VaultError> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| VaultError::Internal(format!("session not found: {session_id}")))?;
        session.state = SessionState::Revoked;
        drop(sessions);

        Ok(())
    }

    /// Add a subject to a group.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::SubjectNotFound`] when the subject is unknown, or
    /// [`VaultError::GroupMembershipUnchanged`] when the subject is already in
    /// the group.
    pub async fn add_to_group(&self, subject_id: &str, group_id: &str) -> Result<(), VaultError> {
        {
            let mut subjects = self.subjects.write().await;
            let subject = subjects
                .get_mut(subject_id)
                .ok_or_else(|| VaultError::SubjectNotFound(subject_id.to_owned()))?;
            if subject.groups.iter().any(|group| group == group_id) {
                return Err(VaultError::GroupMembershipUnchanged);
            }
            subject.groups.push(group_id.to_owned());
            drop(subjects);
        }

        let mut group_membership = self.group_membership.write().await;
        let members = group_membership.entry(group_id.to_owned()).or_default();
        if !members.iter().any(|member| member == subject_id) {
            members.push(subject_id.to_owned());
        }
        drop(group_membership);

        Ok(())
    }

    /// Remove a subject from a group.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::SubjectNotFound`] when the subject is unknown, or
    /// [`VaultError::GroupMembershipUnchanged`] when the subject is not in the
    /// group.
    pub async fn remove_from_group(
        &self,
        subject_id: &str,
        group_id: &str,
    ) -> Result<(), VaultError> {
        {
            let mut subjects = self.subjects.write().await;
            let subject = subjects
                .get_mut(subject_id)
                .ok_or_else(|| VaultError::SubjectNotFound(subject_id.to_owned()))?;
            let Some(position) = subject.groups.iter().position(|group| group == group_id) else {
                return Err(VaultError::GroupMembershipUnchanged);
            };
            subject.groups.remove(position);
            drop(subjects);
        }

        let mut group_membership = self.group_membership.write().await;
        if let Some(members) = group_membership.get_mut(group_id) {
            members.retain(|member| member != subject_id);
        }
        drop(group_membership);

        Ok(())
    }

    pub(crate) async fn groups_for_subject(&self, subject_id: &str) -> Vec<String> {
        let group_membership = self.group_membership.read().await;
        let mut groups: Vec<String> = group_membership
            .iter()
            .filter(|(_group_id, members)| members.iter().any(|member| member == subject_id))
            .map(|(group_id, _members)| group_id.clone())
            .collect();
        drop(group_membership);
        groups.sort();
        groups
    }
}

fn index_memberships(
    group_membership: &mut HashMap<String, Vec<String>>,
    subject_id: &str,
    groups: &[String],
) {
    for group_id in groups {
        let members = group_membership.entry(group_id.clone()).or_default();
        if !members.iter().any(|member| member == subject_id) {
            members.push(subject_id.to_owned());
        }
    }
}

fn expire_past_sessions_for_subject(
    sessions: &mut HashMap<String, Session>,
    subject_id: &str,
    now: DateTime<Utc>,
) {
    for session in sessions
        .values_mut()
        .filter(|session| session.subject_id == subject_id)
    {
        transition_expired_session(session, now);
    }
}

fn transition_expired_session(session: &mut Session, now: DateTime<Utc>) {
    if matches!(
        session.state,
        SessionState::Active | SessionState::Suspended
    ) && session.expires_at < now
    {
        session.state = SessionState::Expired;
    }
}
