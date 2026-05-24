//! Typed AST for the AIOS-FS query predicate DSL.

#![allow(
    clippy::module_name_repetitions,
    reason = "AIOS-FS public names mirror the spec vocabulary"
)]

use serde::{Deserialize, Serialize};

/// Parsed query expression.
///
/// T-040 implements the S2.1 predicate core: AND-only conjunction, no OR, no NOT,
/// no grouping parentheses, and a closed field/operator vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    /// Every predicate must evaluate to true.
    And(Vec<Predicate>),
}

/// One closed-vocabulary predicate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Predicate {
    /// Namespace owning the field.
    pub namespace: QueryNamespace,
    /// Queryable field.
    pub field: QueryField,
    /// Closed operator.
    pub op: QueryOperator,
    /// Right-hand side typed literal.
    pub rhs: QueryValue,
}

/// Closed query namespaces for the T-040 predicate core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryNamespace {
    /// Object record fields.
    Object,
    /// Version record fields.
    Version,
    /// Pointer record fields.
    Pointer,
    /// Chunk record fields.
    Chunk,
    /// Namespace classifier fields.
    Namespace,
}

impl QueryNamespace {
    /// Canonical lowercase namespace token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::Version => "version",
            Self::Pointer => "pointer",
            Self::Chunk => "chunk",
            Self::Namespace => "namespace",
        }
    }

    /// Parse a namespace token.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "object" => Some(Self::Object),
            "version" => Some(Self::Version),
            "pointer" => Some(Self::Pointer),
            "chunk" => Some(Self::Chunk),
            "namespace" => Some(Self::Namespace),
            _ => None,
        }
    }
}

/// Closed queryable fields for the T-040 predicate core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryField {
    /// `object.kind`
    ObjectKind,
    /// `object.privacy_class`
    ObjectPrivacyClass,
    /// `object.lifecycle_state`
    ObjectLifecycleState,
    /// `object.metadata.name`
    ObjectMetadataName,
    /// `object.policy_tags`
    ObjectPolicyTags,
    /// `version.state`
    VersionState,
    /// `version.created_at`
    VersionCreatedAt,
    /// `version.created_by`
    VersionCreatedBy,
    /// `pointer.kind`
    PointerKind,
    /// `namespace.class`
    NamespaceClass,
}

impl QueryField {
    /// Resolve a namespace/subpath pair to a closed field.
    #[must_use]
    pub fn resolve(namespace: QueryNamespace, subpath: &str) -> Option<Self> {
        match (namespace, subpath) {
            (QueryNamespace::Object, "kind") => Some(Self::ObjectKind),
            (QueryNamespace::Object, "privacy_class") => Some(Self::ObjectPrivacyClass),
            (QueryNamespace::Object, "lifecycle_state") => Some(Self::ObjectLifecycleState),
            (QueryNamespace::Object, "metadata.name") => Some(Self::ObjectMetadataName),
            (QueryNamespace::Object, "policy_tags") => Some(Self::ObjectPolicyTags),
            (QueryNamespace::Version, "state") => Some(Self::VersionState),
            (QueryNamespace::Version, "created_at") => Some(Self::VersionCreatedAt),
            (QueryNamespace::Version, "created_by") => Some(Self::VersionCreatedBy),
            (QueryNamespace::Pointer, "kind") => Some(Self::PointerKind),
            (QueryNamespace::Namespace, "class") => Some(Self::NamespaceClass),
            _ => None,
        }
    }

    /// Owning namespace.
    #[must_use]
    pub const fn namespace(self) -> QueryNamespace {
        match self {
            Self::ObjectKind
            | Self::ObjectPrivacyClass
            | Self::ObjectLifecycleState
            | Self::ObjectMetadataName
            | Self::ObjectPolicyTags => QueryNamespace::Object,
            Self::VersionState | Self::VersionCreatedAt | Self::VersionCreatedBy => {
                QueryNamespace::Version
            }
            Self::PointerKind => QueryNamespace::Pointer,
            Self::NamespaceClass => QueryNamespace::Namespace,
        }
    }

    /// Canonical dotted field form.
    #[must_use]
    pub const fn as_dotted(self) -> &'static str {
        match self {
            Self::ObjectKind => "object.kind",
            Self::ObjectPrivacyClass => "object.privacy_class",
            Self::ObjectLifecycleState => "object.lifecycle_state",
            Self::ObjectMetadataName => "object.metadata.name",
            Self::ObjectPolicyTags => "object.policy_tags",
            Self::VersionState => "version.state",
            Self::VersionCreatedAt => "version.created_at",
            Self::VersionCreatedBy => "version.created_by",
            Self::PointerKind => "pointer.kind",
            Self::NamespaceClass => "namespace.class",
        }
    }
}

/// Closed query operator vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryOperator {
    /// `=`
    Eq,
    /// `!=`
    Neq,
    /// `<`
    Lt,
    /// `<=`
    Lte,
    /// `>`
    Gt,
    /// `>=`
    Gte,
    /// `in`
    In,
    /// `contains`
    Contains,
    /// `matches`
    Matches,
}

impl QueryOperator {
    /// Canonical operator token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Neq => "!=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::In => "in",
            Self::Contains => "contains",
            Self::Matches => "matches",
        }
    }
}

/// Typed literal value on the right-hand side of a predicate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum QueryValue {
    /// String or closed-enum token.
    String(String),
    /// Signed integer.
    Int(i64),
    /// Boolean literal.
    Bool(bool),
    /// Homogeneous string list for `in`.
    StringList(Vec<String>),
    /// Inclusive timestamp range for `created_at`.
    TimeRange {
        /// RFC3339 start timestamp.
        start: String,
        /// RFC3339 end timestamp.
        end: String,
    },
}

impl QueryValue {
    /// Short type name for diagnostics.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::String(_) => "string",
            Self::Int(_) => "int",
            Self::Bool(_) => "bool",
            Self::StringList(_) => "string-list",
            Self::TimeRange { .. } => "time-range",
        }
    }
}
