//! Identity subject and session records consumed by the vault skeleton (S5.1).

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// L4 canonical subject reference carried as an opaque string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubjectRef(
    /// Canonical subject string, e.g. `"family:alice"`.
    pub String,
);

impl fmt::Display for SubjectRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// S5.1 subject taxonomy.
///
/// The Rust shape mirrors `aios-policy::SubjectType`; T-054 reconciles the
/// cross-crate type boundary. Serialization uses the S5.1 `SubjectKind` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
pub enum SubjectType {
    /// `HUMAN_USER` — a person with credentials.
    #[serde(rename = "HUMAN_USER")]
    Human,
    /// `AI_AGENT` — an LLM-backed configured agent.
    #[serde(rename = "AI_AGENT")]
    Agent,
    /// `APPLICATION` — an L6 app instance running an action.
    #[serde(rename = "APPLICATION")]
    Application,
    /// `SERVICE` — a constitutional or system service.
    #[serde(rename = "SERVICE")]
    Service,
    /// `DEVICE` — a registered device acting on behalf of a user.
    #[serde(rename = "DEVICE")]
    Device,
    /// `WORKFLOW` — a parameterized action sequence.
    #[serde(rename = "WORKFLOW")]
    Workflow,
    /// `REMOTE_OPERATOR` — a human operating remotely under recovery/admin context.
    #[serde(rename = "REMOTE_OPERATOR")]
    RemoteOperator,
    /// `LOCAL_OPERATOR` — a human physically present at recovery/first-boot console.
    #[serde(rename = "LOCAL_OPERATOR")]
    LocalOperator,
}

/// Minimal S5.1 subject record for the vault opening slice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Subject {
    /// Stable canonical subject id.
    pub canonical_subject_id: String,
    /// Closed subject taxonomy.
    pub subject_type: SubjectType,
    /// Human-facing provisional display name.
    pub provisional_name: String,
    /// Group memberships relevant to vault authorization.
    pub groups: Vec<String>,
    /// Identity-service-bound AI classification.
    pub is_ai: bool,
    /// Subject creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Vault-facing session lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionState {
    /// Session is currently accepted.
    Active,
    /// Session is temporarily suspended.
    Suspended,
    /// Session was explicitly revoked.
    Revoked,
    /// Session has passed its expiry timestamp.
    Expired,
}

/// Minimal S5.1 session record for vault authorization checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Session {
    /// Session identifier.
    pub session_id: String,
    /// Subject bound to this session.
    pub subject_id: String,
    /// Session start timestamp.
    pub started_at: DateTime<Utc>,
    /// Hard expiry timestamp.
    pub expires_at: DateTime<Utc>,
    /// Current session state.
    pub state: SessionState,
}
