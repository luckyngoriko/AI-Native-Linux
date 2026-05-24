//! Object record types — S1.3 §4.1, §5, and S4.1 §12.2.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

use crate::id::{fresh_prefixed_ulid, validate_prefixed_ulid};
use crate::lifecycle::LifecycleState;
use crate::pointer::PointerId;

/// Stable logical object identifier: `"obj_<ULID>"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectId(String);

impl ObjectId {
    /// Canonical object identifier prefix.
    pub const PREFIX: &'static str = "obj_";

    /// Mint a fresh object id.
    #[must_use]
    pub fn new() -> Self {
        Self(fresh_prefixed_ulid(Self::PREFIX))
    }

    /// Validate and adopt an externally supplied object id.
    ///
    /// # Errors
    ///
    /// Returns a string error when the prefix is not `obj_` or the body is not
    /// a valid ULID.
    pub fn parse(input: &str) -> Result<Self, String> {
        validate_prefixed_ulid(input, Self::PREFIX).map(Self)
    }

    /// Borrow the canonical string form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ObjectId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ObjectId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// L4 canonical subject reference carried as an opaque string until L4 lands.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubjectRef(
    /// Canonical subject string, e.g. `"family:alice"` or `"_system:recovery:lucky"`.
    pub String,
);

/// Object kind vocabulary from S1.3 §5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ObjectKind {
    /// `PROJECT` — task or work project.
    Project,
    /// `APPLICATION` — application object.
    Application,
    /// `FILE` — file-like object.
    File,
    /// `MEMORY` — cognitive memory object.
    Memory,
    /// `POLICY` — policy package object.
    Policy,
    /// `MODEL` — model artifact.
    Model,
    /// `PACKAGE` — software package.
    Package,
    /// `EVIDENCE_REF` — reference into the L9 evidence log.
    EvidenceRef,
    /// `WORKSPACE` — workspace object.
    Workspace,
    /// `CONFIG` — configuration object.
    Config,
}

/// Privacy classification imported from S1.2 §5.
///
/// Stub note: S1.2 owns the canonical runtime lattice. AIOS-FS carries this
/// type-level placeholder so S2.3 and S1.2 can consume the object field.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PrivacyClass {
    /// `PUBLIC` — no sensitive information.
    Public,
    /// `INTERNAL` — organisation or project context.
    Internal,
    /// `SENSITIVE` — identifiable user data; default when unspecified.
    Sensitive,
    /// `SECRET_BEARING` — references to vault material or credential hints.
    SecretBearing,
    /// `CLASSIFIED` — operator-marked classified material.
    Classified,
}

/// S4.1 namespace scope kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScopeKind {
    /// `SYSTEM` — top-level `/aios/system` scope.
    System,
    /// `GROUP` — `/aios/groups/<group_id>` scope.
    Group,
    /// `USER` — `/aios/groups/<group_id>/users/<user_id>` scope.
    User,
}

/// Immutable S4.1 scope binding stamped onto every object at creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ScopeBinding {
    /// Scope kind derived from the creation path.
    pub scope_kind: ScopeKind,
    /// Group id for group and user scopes; `None` for system scope.
    pub group_id: Option<String>,
    /// User id for user scope; `None` for system and group scopes.
    pub user_id: Option<String>,
}

/// Queryable object metadata from S1.3 §5.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ObjectMetadata {
    /// Human-facing object name.
    pub name: String,
    /// Searchable object labels.
    pub labels: Vec<String>,
    /// MIME type or empty string when not applicable.
    pub mime: String,
    /// Free-form metadata that is not authoritative security state.
    pub extra: serde_json::Value,
}

/// Constructor input for [`Object::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectInit {
    /// Object id to assign.
    pub object_id: ObjectId,
    /// Object kind.
    pub kind: ObjectKind,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Creator subject.
    pub created_by: SubjectRef,
    /// Current pointer id.
    pub current_pointer_id: PointerId,
    /// Queryable metadata.
    pub metadata: ObjectMetadata,
    /// Initial privacy classification.
    pub privacy_class: PrivacyClass,
    /// Immutable namespace scope binding.
    pub scope_binding: ScopeBinding,
}

/// Stable logical AIOS-FS object record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Object {
    /// `"obj_<ULID>"` stable logical identity.
    pub object_id: ObjectId,
    /// Closed object kind.
    pub kind: ObjectKind,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// L4 subject string of the creator.
    pub created_by: SubjectRef,
    /// Mutable pointer that represents the object's current view.
    pub current_pointer_id: PointerId,
    /// Queryable, non-authoritative metadata.
    pub metadata: ObjectMetadata,
    /// Policy tags consumed by the Policy Kernel.
    pub policy_tags: Vec<String>,
    /// Privacy class consumed by routing and policy decisions.
    pub privacy_class: PrivacyClass,
    /// Lifecycle state.
    pub lifecycle_state: LifecycleState,
    /// Logical retirement timestamp.
    pub retired_at: Option<DateTime<Utc>>,
    /// Scheduled purge timestamp.
    pub purge_at: Option<DateTime<Utc>>,
    /// Search/index hints, e.g. `"fulltext"` or `"semantic"`.
    pub index_hints: Vec<String>,
    /// Immutable namespace scope binding per S4.1 §12.2.
    pub scope_binding: ScopeBinding,
}

impl Object {
    /// Construct an active object with empty policy tags and index hints.
    #[must_use]
    pub fn new(init: ObjectInit) -> Self {
        Self {
            object_id: init.object_id,
            kind: init.kind,
            created_at: init.created_at,
            created_by: init.created_by,
            current_pointer_id: init.current_pointer_id,
            metadata: init.metadata,
            policy_tags: Vec::new(),
            privacy_class: init.privacy_class,
            lifecycle_state: LifecycleState::Active,
            retired_at: None,
            purge_at: None,
            index_hints: Vec::new(),
            scope_binding: init.scope_binding,
        }
    }

    /// Replace policy tags on a newly constructed object.
    #[must_use]
    pub fn with_policy_tags(mut self, policy_tags: Vec<String>) -> Self {
        self.policy_tags = policy_tags;
        self
    }

    /// Replace index hints on a newly constructed object.
    #[must_use]
    pub fn with_index_hints(mut self, index_hints: Vec<String>) -> Self {
        self.index_hints = index_hints;
        self
    }
}
