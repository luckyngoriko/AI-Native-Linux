//! Evaluator and read-only view materialization for the AIOS-FS query DSL.

#![allow(
    clippy::module_name_repetitions,
    reason = "AIOS-FS public names mirror the spec vocabulary"
)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use aios_action::{blake3_hash, jcs_canonicalize};

use crate::error::FsError;
use crate::fs_trait::AiosFs;
use crate::lifecycle::LifecycleState;
use crate::namespace::NamespaceClass;
use crate::object::{Object, ObjectId, ObjectKind, PrivacyClass, ScopeKind};
use crate::pointer::{Pointer, PointerKind};
use crate::query::{Predicate, Query, QueryField, QueryOperator, QueryValue};
use crate::snapshot_id::SnapshotId;
use crate::version::{Version, VersionState};

/// Additive metadata enumeration surface for read-only view materialization.
///
/// This keeps the frozen [`AiosFs`] trait focused on authoritative reads/writes
/// while allowing query materialization to discover candidate object ids without
/// relying on debug formatting.
#[async_trait]
pub trait FsEnumerator: AiosFs {
    /// Return object ids visible to the local metadata enumerator.
    ///
    /// # Errors
    ///
    /// Backends may return [`FsError::Internal`] for catalog scan failures.
    async fn object_ids(&self) -> Result<Vec<ObjectId>, FsError>;

    /// Return the current head snapshot id for empty materialized views.
    fn head_snapshot_id(&self) -> SnapshotId;
}

/// Borrowed predicate evaluation context.
#[derive(Debug, Clone, Copy)]
pub struct QueryEvalContext<'a> {
    /// Object record reached through policy-aware reads.
    pub object: &'a Object,
    /// Current version record reached through the object pointer.
    pub version: &'a Version,
    /// Current pointer record.
    pub pointer: &'a Pointer,
    /// Namespace class associated with the object path/scope.
    pub namespace_class: Option<NamespaceClass>,
}

/// Materialized reference to a matched object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ObjectRef {
    /// Matched object id.
    pub object_id: ObjectId,
}

/// Read-only materialized view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct View {
    /// Snapshot used for every object read.
    pub snapshot_id: SnapshotId,
    /// Matched object references in deterministic object id order.
    pub matched: Vec<ObjectRef>,
    /// Lowercase BLAKE3 hash of the JCS canonical query AST.
    pub query_hash: String,
}

/// Runtime evaluation failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QueryEvalError {
    /// Field/operator/value types do not match.
    #[error("type mismatch evaluating {field}: operator `{op}` expects `{expected_value_type}`, got `{actual_value_type}`")]
    TypeMismatch {
        /// Dotted field.
        field: String,
        /// Operator token.
        op: String,
        /// Expected value type.
        expected_value_type: &'static str,
        /// Actual value type.
        actual_value_type: &'static str,
    },
    /// Operator is not valid for the field's runtime type.
    #[error("unsupported operator for {field}: `{op}`")]
    UnsupportedOperator {
        /// Dotted field.
        field: String,
        /// Operator token.
        op: String,
    },
    /// Timestamp literal failed RFC3339 parsing.
    #[error("invalid timestamp for {field}: {value}")]
    InvalidTimestamp {
        /// Dotted field.
        field: String,
        /// Offending timestamp literal.
        value: String,
    },
}

/// Evaluate a query against one object/version/pointer context.
///
/// # Errors
///
/// Returns [`QueryEvalError`] for invalid field/operator/value type pairings.
pub fn evaluate(query: &Query, ctx: &QueryEvalContext<'_>) -> Result<bool, QueryEvalError> {
    match query {
        Query::And(predicates) => {
            for predicate in predicates {
                if !evaluate_predicate(predicate, ctx)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

/// Materialize a read-only object view over the current AIOS-FS state.
///
/// Candidate discovery is supplied by [`FsEnumerator`], while every authoritative
/// object access still goes through [`AiosFs::read_object`].
///
/// # Errors
///
/// Returns [`FsError`] from snapshot/object reads or wraps query evaluation
/// failures in [`FsError::QueryEval`].
pub async fn materialize_view<'a, F>(
    query: &'a Query,
    fs: &'a F,
    snapshot: Option<&'a SnapshotId>,
) -> Result<View, FsError>
where
    F: AiosFs + FsEnumerator + ?Sized + 'a,
{
    let query_hash = hash_query(query)?;
    let object_ids = fs.object_ids().await?;
    let mut matched = Vec::new();
    let mut view_snapshot_id = snapshot.cloned();

    for object_id in object_ids {
        let read = fs.read_object(&object_id, snapshot).await?;
        if view_snapshot_id.is_none() {
            view_snapshot_id = Some(read.snapshot_id.clone());
        }

        let pointer = fs.resolve_pointer(&read.object.current_pointer_id).await?;
        let ctx = QueryEvalContext {
            object: &read.object,
            version: &read.version,
            pointer: &pointer,
            namespace_class: Some(namespace_class_for_object(&read.object)),
        };

        if evaluate(query, &ctx).map_err(|err| FsError::QueryEval(err.to_string()))? {
            matched.push(ObjectRef { object_id });
        }
    }

    matched.sort_by(|left, right| left.object_id.as_str().cmp(right.object_id.as_str()));

    Ok(View {
        snapshot_id: view_snapshot_id.unwrap_or_else(|| fs.head_snapshot_id()),
        matched,
        query_hash,
    })
}

fn evaluate_predicate(
    predicate: &Predicate,
    ctx: &QueryEvalContext<'_>,
) -> Result<bool, QueryEvalError> {
    match predicate.op {
        QueryOperator::Eq
        | QueryOperator::Neq
        | QueryOperator::Lt
        | QueryOperator::Lte
        | QueryOperator::Gt
        | QueryOperator::Gte => eval_compare(predicate, ctx),
        QueryOperator::In => eval_in(predicate, ctx),
        QueryOperator::Contains => eval_contains(predicate, ctx),
        QueryOperator::Matches => eval_matches(predicate, ctx),
    }
}

#[derive(Debug, Clone)]
enum FieldValue {
    Str(String),
    StrList(Vec<String>),
    Time(DateTime<Utc>),
    Absent,
}

impl FieldValue {
    const fn type_name(&self) -> &'static str {
        match self {
            Self::Str(_) => "string",
            Self::StrList(_) => "string-list",
            Self::Time(_) => "timestamp",
            Self::Absent => "absent",
        }
    }
}

fn eval_compare(predicate: &Predicate, ctx: &QueryEvalContext<'_>) -> Result<bool, QueryEvalError> {
    let actual = resolve_field_value(predicate.field, ctx);
    match (&actual, &predicate.rhs) {
        (FieldValue::Absent, _) => Ok(false),
        (FieldValue::Str(left), QueryValue::String(right)) => {
            Ok(compare_strings(left, right, predicate.op))
        }
        (FieldValue::Time(left), QueryValue::String(right)) => {
            let right = parse_timestamp(predicate.field, right)?;
            Ok(compare_times(*left, right, predicate.op))
        }
        (FieldValue::Time(left), QueryValue::TimeRange { start, end }) => match predicate.op {
            QueryOperator::Eq => Ok(time_in_range(*left, start, end, predicate.field)?),
            _ => Err(unsupported_operator(predicate)),
        },
        (FieldValue::StrList(_), QueryValue::String(_)) => Err(unsupported_operator(predicate)),
        (actual, rhs) => Err(QueryEvalError::TypeMismatch {
            field: predicate.field.as_dotted().to_owned(),
            op: predicate.op.as_str().to_owned(),
            expected_value_type: actual.type_name(),
            actual_value_type: rhs.type_name(),
        }),
    }
}

fn eval_in(predicate: &Predicate, ctx: &QueryEvalContext<'_>) -> Result<bool, QueryEvalError> {
    let actual = resolve_field_value(predicate.field, ctx);
    match (&actual, &predicate.rhs) {
        (FieldValue::Absent, _) => Ok(false),
        (FieldValue::Str(left), QueryValue::StringList(values)) => {
            Ok(values.iter().any(|right| right == left))
        }
        (FieldValue::StrList(left), QueryValue::StringList(values)) => Ok(values
            .iter()
            .any(|right| left.iter().any(|item| item == right))),
        (FieldValue::Time(left), QueryValue::TimeRange { start, end }) => {
            time_in_range(*left, start, end, predicate.field)
        }
        (actual, rhs) => Err(QueryEvalError::TypeMismatch {
            field: predicate.field.as_dotted().to_owned(),
            op: predicate.op.as_str().to_owned(),
            expected_value_type: actual.type_name(),
            actual_value_type: rhs.type_name(),
        }),
    }
}

fn eval_contains(
    predicate: &Predicate,
    ctx: &QueryEvalContext<'_>,
) -> Result<bool, QueryEvalError> {
    let actual = resolve_field_value(predicate.field, ctx);
    match (&actual, &predicate.rhs) {
        (FieldValue::Absent, _) => Ok(false),
        (FieldValue::Str(left), QueryValue::String(needle)) => Ok(left.contains(needle)),
        (FieldValue::StrList(left), QueryValue::String(needle)) => {
            Ok(left.iter().any(|item| item == needle))
        }
        (actual, rhs) => Err(QueryEvalError::TypeMismatch {
            field: predicate.field.as_dotted().to_owned(),
            op: predicate.op.as_str().to_owned(),
            expected_value_type: actual.type_name(),
            actual_value_type: rhs.type_name(),
        }),
    }
}

fn eval_matches(predicate: &Predicate, ctx: &QueryEvalContext<'_>) -> Result<bool, QueryEvalError> {
    let actual = resolve_field_value(predicate.field, ctx);
    match (&actual, &predicate.rhs) {
        (FieldValue::Absent, _) => Ok(false),
        (FieldValue::Str(left), QueryValue::String(pattern)) => Ok(glob_match(left, pattern)),
        (actual, rhs) => Err(QueryEvalError::TypeMismatch {
            field: predicate.field.as_dotted().to_owned(),
            op: predicate.op.as_str().to_owned(),
            expected_value_type: actual.type_name(),
            actual_value_type: rhs.type_name(),
        }),
    }
}

fn resolve_field_value(field: QueryField, ctx: &QueryEvalContext<'_>) -> FieldValue {
    match field {
        QueryField::ObjectKind => FieldValue::Str(object_kind_token(ctx.object.kind).to_owned()),
        QueryField::ObjectPrivacyClass => {
            FieldValue::Str(privacy_class_token(ctx.object.privacy_class).to_owned())
        }
        QueryField::ObjectLifecycleState => {
            FieldValue::Str(lifecycle_state_token(ctx.object.lifecycle_state).to_owned())
        }
        QueryField::ObjectMetadataName => FieldValue::Str(ctx.object.metadata.name.clone()),
        QueryField::ObjectPolicyTags => FieldValue::StrList(ctx.object.policy_tags.clone()),
        QueryField::VersionState => {
            FieldValue::Str(version_state_token(ctx.version.state).to_owned())
        }
        QueryField::VersionCreatedAt => FieldValue::Time(ctx.version.created_at),
        QueryField::VersionCreatedBy => ctx
            .version
            .created_by_action_id
            .as_ref()
            .map(|id| FieldValue::Str(id.as_str().to_owned()))
            .or_else(|| {
                ctx.version
                    .created_by_transaction_id
                    .as_ref()
                    .map(|id| FieldValue::Str(id.as_str().to_owned()))
            })
            .unwrap_or(FieldValue::Absent),
        QueryField::PointerKind => FieldValue::Str(pointer_kind_token(ctx.pointer.kind).to_owned()),
        QueryField::NamespaceClass => ctx.namespace_class.map_or(FieldValue::Absent, |class| {
            FieldValue::Str(namespace_class_token(class).to_owned())
        }),
    }
}

fn compare_strings(left: &str, right: &str, op: QueryOperator) -> bool {
    match op {
        QueryOperator::Eq => left == right,
        QueryOperator::Neq => left != right,
        QueryOperator::Lt => left < right,
        QueryOperator::Lte => left <= right,
        QueryOperator::Gt => left > right,
        QueryOperator::Gte => left >= right,
        QueryOperator::In | QueryOperator::Contains | QueryOperator::Matches => false,
    }
}

fn compare_times(left: DateTime<Utc>, right: DateTime<Utc>, op: QueryOperator) -> bool {
    match op {
        QueryOperator::Eq => left == right,
        QueryOperator::Neq => left != right,
        QueryOperator::Lt => left < right,
        QueryOperator::Lte => left <= right,
        QueryOperator::Gt => left > right,
        QueryOperator::Gte => left >= right,
        QueryOperator::In | QueryOperator::Contains | QueryOperator::Matches => false,
    }
}

fn time_in_range(
    value: DateTime<Utc>,
    start: &str,
    end: &str,
    field: QueryField,
) -> Result<bool, QueryEvalError> {
    let start = parse_timestamp(field, start)?;
    let end = parse_timestamp(field, end)?;
    Ok(start <= value && value <= end)
}

fn parse_timestamp(field: QueryField, value: &str) -> Result<DateTime<Utc>, QueryEvalError> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|_| QueryEvalError::InvalidTimestamp {
            field: field.as_dotted().to_owned(),
            value: value.to_owned(),
        })
}

fn unsupported_operator(predicate: &Predicate) -> QueryEvalError {
    QueryEvalError::UnsupportedOperator {
        field: predicate.field.as_dotted().to_owned(),
        op: predicate.op.as_str().to_owned(),
    }
}

fn glob_match(value: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let Some(star) = pattern.find('*') else {
        return value == pattern;
    };
    let (prefix, suffix_with_star) = pattern.split_at(star);
    let suffix = &suffix_with_star[1..];
    value.starts_with(prefix) && value.ends_with(suffix)
}

fn hash_query(query: &Query) -> Result<String, FsError> {
    jcs_canonicalize(query)
        .map(|canonical| blake3_hash(canonical.as_bytes()))
        .map_err(|err| FsError::QueryEval(format!("query canonicalization failed: {err}")))
}

const fn namespace_class_for_object(object: &Object) -> NamespaceClass {
    match object.scope_binding.scope_kind {
        ScopeKind::System => NamespaceClass::System,
        ScopeKind::Group => NamespaceClass::Group,
        ScopeKind::User => NamespaceClass::User,
    }
}

const fn object_kind_token(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Project => "PROJECT",
        ObjectKind::Application => "APPLICATION",
        ObjectKind::File => "FILE",
        ObjectKind::Memory => "MEMORY",
        ObjectKind::Policy => "POLICY",
        ObjectKind::Model => "MODEL",
        ObjectKind::Package => "PACKAGE",
        ObjectKind::EvidenceRef => "EVIDENCE_REF",
        ObjectKind::Workspace => "WORKSPACE",
        ObjectKind::Config => "CONFIG",
    }
}

const fn privacy_class_token(class: PrivacyClass) -> &'static str {
    match class {
        PrivacyClass::Public => "PUBLIC",
        PrivacyClass::Internal => "INTERNAL",
        PrivacyClass::Sensitive => "SENSITIVE",
        PrivacyClass::SecretBearing => "SECRET_BEARING",
        PrivacyClass::Classified => "CLASSIFIED",
    }
}

const fn lifecycle_state_token(state: LifecycleState) -> &'static str {
    match state {
        LifecycleState::Active => "ACTIVE",
        LifecycleState::Retired => "RETIRED",
        LifecycleState::Purged => "PURGED",
    }
}

const fn version_state_token(state: VersionState) -> &'static str {
    match state {
        VersionState::Staged => "STAGED",
        VersionState::Verified => "VERIFIED",
        VersionState::Quarantined => "QUARANTINED",
        VersionState::RetiredVersion => "RETIRED_VERSION",
    }
}

const fn pointer_kind_token(kind: PointerKind) -> &'static str {
    match kind {
        PointerKind::Current => "CURRENT",
        PointerKind::Stable => "STABLE",
        PointerKind::Candidate => "CANDIDATE",
        PointerKind::Rollback => "ROLLBACK",
        PointerKind::Quarantine => "QUARANTINE",
    }
}

const fn namespace_class_token(class: NamespaceClass) -> &'static str {
    match class {
        NamespaceClass::System => "SYSTEM",
        NamespaceClass::SystemApps => "SYSTEM_APPS",
        NamespaceClass::SystemAgents => "SYSTEM_AGENTS",
        NamespaceClass::SystemPolicy => "SYSTEM_POLICY",
        NamespaceClass::SystemCapabilities => "SYSTEM_CAPABILITIES",
        NamespaceClass::SystemEvidence => "SYSTEM_EVIDENCE",
        NamespaceClass::SystemVault => "SYSTEM_VAULT",
        NamespaceClass::SystemRuntime => "SYSTEM_RUNTIME",
        NamespaceClass::SystemRecovery => "SYSTEM_RECOVERY",
        NamespaceClass::SystemBoot => "SYSTEM_BOOT",
        NamespaceClass::SystemFirstboot => "SYSTEM_FIRSTBOOT",
        NamespaceClass::SystemGovernance => "SYSTEM_GOVERNANCE",
        NamespaceClass::SystemIdentity => "SYSTEM_IDENTITY",
        NamespaceClass::SystemKernel => "SYSTEM_KERNEL",
        NamespaceClass::SystemHardware => "SYSTEM_HARDWARE",
        NamespaceClass::SystemDrivers => "SYSTEM_DRIVERS",
        NamespaceClass::SystemFirmware => "SYSTEM_FIRMWARE",
        NamespaceClass::SystemNetwork => "SYSTEM_NETWORK",
        NamespaceClass::SystemSgr => "SYSTEM_SGR",
        NamespaceClass::SystemUnits => "SYSTEM_UNITS",
        NamespaceClass::SystemRunbooks => "SYSTEM_RUNBOOKS",
        NamespaceClass::SystemThemes => "SYSTEM_THEMES",
        NamespaceClass::SystemRenderers => "SYSTEM_RENDERERS",
        NamespaceClass::SystemWeb => "SYSTEM_WEB",
        NamespaceClass::SystemDistribution => "SYSTEM_DISTRIBUTION",
        NamespaceClass::Groups => "GROUPS",
        NamespaceClass::Group => "GROUP",
        NamespaceClass::GroupApps => "GROUP_APPS",
        NamespaceClass::GroupAgents => "GROUP_AGENTS",
        NamespaceClass::GroupUsers => "GROUP_USERS",
        NamespaceClass::GroupShared => "GROUP_SHARED",
        NamespaceClass::GroupProjects => "GROUP_PROJECTS",
        NamespaceClass::GroupDatasets => "GROUP_DATASETS",
        NamespaceClass::GroupInbox => "GROUP_INBOX",
        NamespaceClass::GroupPolicy => "GROUP_POLICY",
        NamespaceClass::GroupEvidence => "GROUP_EVIDENCE",
        NamespaceClass::GroupVault => "GROUP_VAULT",
        NamespaceClass::GroupAudit => "GROUP_AUDIT",
        NamespaceClass::GroupServices => "GROUP_SERVICES",
        NamespaceClass::GroupSystem => "GROUP_SYSTEM",
        NamespaceClass::User => "USER",
        NamespaceClass::UserHome => "USER_HOME",
        NamespaceClass::UserAgents => "USER_AGENTS",
        NamespaceClass::UserPrefs => "USER_PREFS",
        NamespaceClass::UserDesktop => "USER_DESKTOP",
        NamespaceClass::UserInbox => "USER_INBOX",
        NamespaceClass::UserOutbox => "USER_OUTBOX",
        NamespaceClass::UserDrafts => "USER_DRAFTS",
        NamespaceClass::UserTrust => "USER_TRUST",
        NamespaceClass::UserApps => "USER_APPS",
        NamespaceClass::UserRuntime => "USER_RUNTIME",
        NamespaceClass::UserExports => "USER_EXPORTS",
    }
}
