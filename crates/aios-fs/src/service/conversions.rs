//! Rust ↔ proto translations for the gRPC `AiosFs` surface (T-043).
//!
//! The conversion layer is the only place that knows about prost-generated
//! message shapes. The core AIOS-FS model remains tonic-free.

#![allow(
    missing_docs,
    clippy::clone_on_copy,
    clippy::missing_const_for_fn,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::ref_option,
    clippy::result_large_err,
    reason = "conversion function names are intentionally literal and covered by the module docs"
)]

use chrono::{DateTime, TimeZone, Utc};
use prost_types::{ListValue, NullValue, Struct, Timestamp, Value as ProstValue};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use tonic::Status;

use aios_action::ActionId;

use crate::chunk::{Chunk, ChunkId, ChunkRef};
use crate::error::FsError;
use crate::fs_trait::{
    FsContext, ObjectReadResult, ObjectWriteRequest, ObjectWriteResult, SnapshotSummary,
};
use crate::gc::{GcPassReport, GcReason, VersionPurgeReason};
use crate::impl_space::{ImplSpaceBinding, ImplSpaceSource, ImplSpaceTarget, IntegrityState};
use crate::lifecycle::LifecycleState;
use crate::object::{
    Object, ObjectId, ObjectKind, ObjectMetadata, PrivacyClass, ScopeBinding, ScopeKind, SubjectRef,
};
use crate::pointer::{Pointer, PointerId, PointerKind};
use crate::quarantine::{QuarantineDisposition, QuarantineReceipt, QuarantineTrigger};
use crate::query::{Predicate, Query, QueryField, QueryNamespace, QueryOperator, QueryValue};
use crate::query_eval::{ObjectRef, View};
use crate::service::proto;
use crate::snapshot_id::SnapshotId;
use crate::transaction::{ConsistencyClass, TransactionId};
use crate::version::{Version, VersionId, VersionState};

// ---------------------------------------------------------------------------
// Timestamp helpers
// ---------------------------------------------------------------------------

/// Convert a `chrono::DateTime<Utc>` into the prost well-known `Timestamp`.
#[must_use]
pub fn datetime_to_proto(dt: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: dt.timestamp(),
        nanos: i32::try_from(dt.timestamp_subsec_nanos()).unwrap_or(0),
    }
}

/// Convert the prost well-known `Timestamp` back into `chrono::DateTime<Utc>`.
#[must_use]
pub fn datetime_from_proto(ts: Timestamp) -> DateTime<Utc> {
    Utc.timestamp_opt(ts.seconds, u32::try_from(ts.nanos).unwrap_or(0))
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap_or_default())
}

fn optional_datetime_to_proto(dt: Option<DateTime<Utc>>) -> Option<Timestamp> {
    dt.map(datetime_to_proto)
}

fn optional_datetime_from_proto(ts: Option<Timestamp>) -> Option<DateTime<Utc>> {
    ts.map(datetime_from_proto)
}

// ---------------------------------------------------------------------------
// JSON <-> google.protobuf.Struct
// ---------------------------------------------------------------------------

/// Convert a `serde_json::Value` object into a protobuf `Struct`.
///
/// Non-object values collapse to an empty struct because `Struct` itself is an
/// object map; scalar query values use dedicated proto fields elsewhere.
#[must_use]
pub fn json_to_struct(value: &JsonValue) -> Struct {
    let Some(map) = value.as_object() else {
        return Struct::default();
    };

    Struct {
        fields: map
            .iter()
            .map(|(key, value)| (key.clone(), json_to_prost_value(value)))
            .collect(),
    }
}

/// Convert a protobuf `Struct` back into `serde_json::Value::Object`.
#[must_use]
pub fn struct_to_json(value: &Struct) -> JsonValue {
    JsonValue::Object(
        value
            .fields
            .iter()
            .map(|(key, value)| (key.clone(), prost_value_to_json(value)))
            .collect::<JsonMap<String, JsonValue>>(),
    )
}

fn json_to_prost_value(value: &JsonValue) -> ProstValue {
    let kind = match value {
        JsonValue::Null => prost_types::value::Kind::NullValue(NullValue::NullValue as i32),
        JsonValue::Bool(v) => prost_types::value::Kind::BoolValue(*v),
        JsonValue::Number(v) => {
            prost_types::value::Kind::NumberValue(v.as_f64().unwrap_or_default())
        }
        JsonValue::String(v) => prost_types::value::Kind::StringValue(v.clone()),
        JsonValue::Array(values) => prost_types::value::Kind::ListValue(ListValue {
            values: values.iter().map(json_to_prost_value).collect(),
        }),
        JsonValue::Object(_) => prost_types::value::Kind::StructValue(json_to_struct(value)),
    };
    ProstValue { kind: Some(kind) }
}

fn prost_value_to_json(value: &ProstValue) -> JsonValue {
    match value.kind.as_ref() {
        Some(prost_types::value::Kind::NullValue(_)) | None => JsonValue::Null,
        Some(prost_types::value::Kind::NumberValue(v)) => {
            JsonNumber::from_f64(*v).map_or(JsonValue::Null, JsonValue::Number)
        }
        Some(prost_types::value::Kind::StringValue(v)) => JsonValue::String(v.clone()),
        Some(prost_types::value::Kind::BoolValue(v)) => JsonValue::Bool(*v),
        Some(prost_types::value::Kind::StructValue(v)) => struct_to_json(v),
        Some(prost_types::value::Kind::ListValue(v)) => {
            JsonValue::Array(v.values.iter().map(prost_value_to_json).collect())
        }
    }
}

// ---------------------------------------------------------------------------
// Error -> tonic::Status
// ---------------------------------------------------------------------------

/// Map typed [`FsError`] values onto canonical gRPC status codes.
#[must_use]
pub fn fs_error_to_status(err: &FsError) -> Status {
    match err {
        FsError::ObjectNotFound(_)
        | FsError::VersionNotFound(_)
        | FsError::PointerNotFound(_)
        | FsError::ChunkUnknown(_)
        | FsError::ImplSpaceBindingNotFound(_) => Status::not_found(err.to_string()),
        FsError::InvalidPath(_)
        | FsError::WriteRequiresParent
        | FsError::InvalidTransition { .. }
        | FsError::QueryParse(_)
        | FsError::QueryEval(_) => Status::invalid_argument(err.to_string()),
        FsError::QuarantineViolation(_)
        | FsError::NamespaceMutationDenied { .. }
        | FsError::QuarantineAlreadyApplied(_)
        | FsError::QuarantineNotApplied(_)
        | FsError::VersionAlreadyPurged(_)
        | FsError::ChunkStillReferenced { .. }
        | FsError::NoPriorStablePointer(_) => Status::failed_precondition(err.to_string()),
        FsError::SnapshotStale { .. } => Status::aborted(err.to_string()),
        FsError::ImplSpaceIntegrityFailed(_) => Status::data_loss(err.to_string()),
        FsError::ImplSpaceTargetUnreachable(_) => Status::unavailable(err.to_string()),
        FsError::EvidenceEmitFailed(_) | FsError::Internal(_) => Status::internal(err.to_string()),
    }
}

// ---------------------------------------------------------------------------
// ID helpers
// ---------------------------------------------------------------------------

pub(crate) fn parse_object_id(input: &str) -> Result<ObjectId, Status> {
    ObjectId::parse(input)
        .map_err(|err| Status::invalid_argument(format!("invalid object_id `{input}`: {err}")))
}

pub(crate) fn parse_version_id(input: &str) -> Result<VersionId, Status> {
    VersionId::parse(input)
        .map_err(|err| Status::invalid_argument(format!("invalid version_id `{input}`: {err}")))
}

pub(crate) fn parse_pointer_id(input: &str) -> Result<PointerId, Status> {
    PointerId::parse(input)
        .map_err(|err| Status::invalid_argument(format!("invalid pointer_id `{input}`: {err}")))
}

fn parse_transaction_id(input: &str) -> Result<TransactionId, Status> {
    TransactionId::parse(input)
        .map_err(|err| Status::invalid_argument(format!("invalid transaction_id `{input}`: {err}")))
}

fn parse_chunk_id(input: &str) -> Result<ChunkId, Status> {
    ChunkId::parse(input)
        .map_err(|err| Status::invalid_argument(format!("invalid chunk_id `{input}`: {err}")))
}

pub(crate) fn snapshot_from_string(input: &str) -> Option<SnapshotId> {
    (!input.trim().is_empty()).then(|| SnapshotId(input.to_owned()))
}

fn action_id_to_bytes(action_id: &Option<ActionId>) -> Vec<u8> {
    action_id
        .as_ref()
        .map_or_else(Vec::new, |id| id.as_str().as_bytes().to_vec())
}

fn action_id_from_bytes(bytes: &[u8]) -> Result<Option<ActionId>, Status> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let raw = std::str::from_utf8(bytes)
        .map_err(|err| Status::invalid_argument(format!("action_id_proto is not UTF-8: {err}")))?;
    ActionId::parse(raw)
        .map(Some)
        .map_err(|err| Status::invalid_argument(format!("invalid action_id_proto `{raw}`: {err}")))
}

// ---------------------------------------------------------------------------
// Enum conversions
// ---------------------------------------------------------------------------

#[must_use]
pub const fn object_kind_to_proto(kind: ObjectKind) -> proto::ObjectKindProto {
    match kind {
        ObjectKind::Project => proto::ObjectKindProto::Project,
        ObjectKind::Application => proto::ObjectKindProto::Application,
        ObjectKind::File => proto::ObjectKindProto::File,
        ObjectKind::Memory => proto::ObjectKindProto::Memory,
        ObjectKind::Policy => proto::ObjectKindProto::Policy,
        ObjectKind::Model => proto::ObjectKindProto::Model,
        ObjectKind::Package => proto::ObjectKindProto::Package,
        ObjectKind::EvidenceRef => proto::ObjectKindProto::EvidenceRef,
        ObjectKind::Workspace => proto::ObjectKindProto::Workspace,
        ObjectKind::Config => proto::ObjectKindProto::Config,
    }
}

pub fn object_kind_from_proto(kind: proto::ObjectKindProto) -> Result<ObjectKind, Status> {
    match kind {
        proto::ObjectKindProto::Project => Ok(ObjectKind::Project),
        proto::ObjectKindProto::Application => Ok(ObjectKind::Application),
        proto::ObjectKindProto::File => Ok(ObjectKind::File),
        proto::ObjectKindProto::Memory => Ok(ObjectKind::Memory),
        proto::ObjectKindProto::Policy => Ok(ObjectKind::Policy),
        proto::ObjectKindProto::Model => Ok(ObjectKind::Model),
        proto::ObjectKindProto::Package => Ok(ObjectKind::Package),
        proto::ObjectKindProto::EvidenceRef => Ok(ObjectKind::EvidenceRef),
        proto::ObjectKindProto::Workspace => Ok(ObjectKind::Workspace),
        proto::ObjectKindProto::Config => Ok(ObjectKind::Config),
        proto::ObjectKindProto::ObjectKindUnspecified => {
            Err(Status::invalid_argument("object kind is unspecified"))
        }
    }
}

#[must_use]
pub const fn privacy_class_to_proto(class: PrivacyClass) -> proto::PrivacyClassProto {
    match class {
        PrivacyClass::Public => proto::PrivacyClassProto::Public,
        PrivacyClass::Internal => proto::PrivacyClassProto::Internal,
        PrivacyClass::Sensitive => proto::PrivacyClassProto::Sensitive,
        PrivacyClass::SecretBearing => proto::PrivacyClassProto::SecretBearing,
        PrivacyClass::Classified => proto::PrivacyClassProto::Classified,
    }
}

pub fn privacy_class_from_proto(class: proto::PrivacyClassProto) -> Result<PrivacyClass, Status> {
    match class {
        proto::PrivacyClassProto::Public => Ok(PrivacyClass::Public),
        proto::PrivacyClassProto::Internal => Ok(PrivacyClass::Internal),
        proto::PrivacyClassProto::Sensitive => Ok(PrivacyClass::Sensitive),
        proto::PrivacyClassProto::SecretBearing => Ok(PrivacyClass::SecretBearing),
        proto::PrivacyClassProto::Classified => Ok(PrivacyClass::Classified),
        proto::PrivacyClassProto::PrivacyClassUnspecified => {
            Err(Status::invalid_argument("privacy class is unspecified"))
        }
    }
}

#[must_use]
pub const fn lifecycle_state_to_proto(state: LifecycleState) -> proto::LifecycleStateProto {
    match state {
        LifecycleState::Active => proto::LifecycleStateProto::Active,
        LifecycleState::Retired => proto::LifecycleStateProto::Retired,
        LifecycleState::Purged => proto::LifecycleStateProto::Purged,
    }
}

pub fn lifecycle_state_from_proto(
    state: proto::LifecycleStateProto,
) -> Result<LifecycleState, Status> {
    match state {
        proto::LifecycleStateProto::Active => Ok(LifecycleState::Active),
        proto::LifecycleStateProto::Retired => Ok(LifecycleState::Retired),
        proto::LifecycleStateProto::Purged => Ok(LifecycleState::Purged),
        proto::LifecycleStateProto::LifecycleStateUnspecified => {
            Err(Status::invalid_argument("lifecycle state is unspecified"))
        }
    }
}

#[must_use]
pub const fn version_state_to_proto(state: VersionState) -> proto::VersionStateProto {
    match state {
        VersionState::Staged => proto::VersionStateProto::Staged,
        VersionState::Verified => proto::VersionStateProto::Verified,
        VersionState::Quarantined => proto::VersionStateProto::Quarantined,
        VersionState::RetiredVersion => proto::VersionStateProto::RetiredVersion,
    }
}

pub fn version_state_from_proto(state: proto::VersionStateProto) -> Result<VersionState, Status> {
    match state {
        proto::VersionStateProto::Staged => Ok(VersionState::Staged),
        proto::VersionStateProto::Verified => Ok(VersionState::Verified),
        proto::VersionStateProto::Quarantined => Ok(VersionState::Quarantined),
        proto::VersionStateProto::RetiredVersion => Ok(VersionState::RetiredVersion),
        proto::VersionStateProto::VersionStateUnspecified => {
            Err(Status::invalid_argument("version state is unspecified"))
        }
    }
}

#[must_use]
pub const fn pointer_kind_to_proto(kind: PointerKind) -> proto::PointerKindProto {
    match kind {
        PointerKind::Current => proto::PointerKindProto::Current,
        PointerKind::Stable => proto::PointerKindProto::Stable,
        PointerKind::Candidate => proto::PointerKindProto::Candidate,
        PointerKind::Rollback => proto::PointerKindProto::Rollback,
        PointerKind::Quarantine => proto::PointerKindProto::Quarantine,
    }
}

pub fn pointer_kind_from_proto(kind: proto::PointerKindProto) -> Result<PointerKind, Status> {
    match kind {
        proto::PointerKindProto::Current => Ok(PointerKind::Current),
        proto::PointerKindProto::Stable => Ok(PointerKind::Stable),
        proto::PointerKindProto::Candidate => Ok(PointerKind::Candidate),
        proto::PointerKindProto::Rollback => Ok(PointerKind::Rollback),
        proto::PointerKindProto::Quarantine => Ok(PointerKind::Quarantine),
        proto::PointerKindProto::PointerKindUnspecified => {
            Err(Status::invalid_argument("pointer kind is unspecified"))
        }
    }
}

#[must_use]
pub const fn consistency_class_to_proto(
    consistency: ConsistencyClass,
) -> proto::ConsistencyClassProto {
    match consistency {
        ConsistencyClass::Snapshot => proto::ConsistencyClassProto::Snapshot,
        ConsistencyClass::Linearizable => proto::ConsistencyClassProto::Linearizable,
        ConsistencyClass::Eventual => proto::ConsistencyClassProto::Eventual,
    }
}

pub fn consistency_class_from_proto(
    consistency: proto::ConsistencyClassProto,
) -> Result<ConsistencyClass, Status> {
    match consistency {
        proto::ConsistencyClassProto::Snapshot
        | proto::ConsistencyClassProto::ConsistencyClassUnspecified => {
            Ok(ConsistencyClass::Snapshot)
        }
        proto::ConsistencyClassProto::Linearizable => Ok(ConsistencyClass::Linearizable),
        proto::ConsistencyClassProto::Eventual => Ok(ConsistencyClass::Eventual),
    }
}

#[must_use]
pub const fn scope_kind_to_proto(kind: ScopeKind) -> proto::ScopeKindProto {
    match kind {
        ScopeKind::System => proto::ScopeKindProto::System,
        ScopeKind::Group => proto::ScopeKindProto::Group,
        ScopeKind::User => proto::ScopeKindProto::User,
    }
}

pub fn scope_kind_from_proto(kind: proto::ScopeKindProto) -> Result<ScopeKind, Status> {
    match kind {
        proto::ScopeKindProto::System => Ok(ScopeKind::System),
        proto::ScopeKindProto::Group => Ok(ScopeKind::Group),
        proto::ScopeKindProto::User => Ok(ScopeKind::User),
        proto::ScopeKindProto::ScopeKindUnspecified => {
            Err(Status::invalid_argument("scope kind is unspecified"))
        }
    }
}

#[must_use]
pub const fn quarantine_trigger_to_proto(
    trigger: QuarantineTrigger,
) -> proto::QuarantineTriggerProto {
    match trigger {
        QuarantineTrigger::ValidationFailure => proto::QuarantineTriggerProto::ValidationFailure,
        QuarantineTrigger::IntegrityFailure => proto::QuarantineTriggerProto::IntegrityFailure,
        QuarantineTrigger::PolicyViolation => proto::QuarantineTriggerProto::PolicyViolation,
        QuarantineTrigger::AttestationFailure => proto::QuarantineTriggerProto::AttestationFailure,
        QuarantineTrigger::OperatorManual => proto::QuarantineTriggerProto::OperatorManual,
    }
}

pub fn quarantine_trigger_from_proto(
    trigger: proto::QuarantineTriggerProto,
) -> Result<QuarantineTrigger, Status> {
    match trigger {
        proto::QuarantineTriggerProto::ValidationFailure => {
            Ok(QuarantineTrigger::ValidationFailure)
        }
        proto::QuarantineTriggerProto::IntegrityFailure => Ok(QuarantineTrigger::IntegrityFailure),
        proto::QuarantineTriggerProto::PolicyViolation => Ok(QuarantineTrigger::PolicyViolation),
        proto::QuarantineTriggerProto::AttestationFailure => {
            Ok(QuarantineTrigger::AttestationFailure)
        }
        proto::QuarantineTriggerProto::OperatorManual => Ok(QuarantineTrigger::OperatorManual),
        proto::QuarantineTriggerProto::QuarantineTriggerUnspecified => Err(
            Status::invalid_argument("quarantine trigger is unspecified"),
        ),
    }
}

#[must_use]
pub const fn quarantine_disposition_to_proto(
    disposition: QuarantineDisposition,
) -> proto::QuarantineDispositionProto {
    match disposition {
        QuarantineDisposition::Released => proto::QuarantineDispositionProto::Released,
        QuarantineDisposition::Purged => proto::QuarantineDispositionProto::PurgedDisposition,
    }
}

pub fn quarantine_disposition_from_proto(
    disposition: proto::QuarantineDispositionProto,
) -> Result<QuarantineDisposition, Status> {
    match disposition {
        proto::QuarantineDispositionProto::Released => Ok(QuarantineDisposition::Released),
        proto::QuarantineDispositionProto::PurgedDisposition => Ok(QuarantineDisposition::Purged),
        proto::QuarantineDispositionProto::QuarantineDispositionUnspecified => Err(
            Status::invalid_argument("quarantine disposition is unspecified"),
        ),
    }
}

#[must_use]
pub const fn version_purge_reason_to_proto(
    reason: VersionPurgeReason,
) -> proto::VersionPurgeReasonProto {
    match reason {
        VersionPurgeReason::Retired => proto::VersionPurgeReasonProto::RetiredPurge,
        VersionPurgeReason::Quarantined => proto::VersionPurgeReasonProto::QuarantinedPurge,
        VersionPurgeReason::OperatorRequested => {
            proto::VersionPurgeReasonProto::OperatorRequestedPurge
        }
    }
}

pub fn version_purge_reason_from_proto(
    reason: proto::VersionPurgeReasonProto,
) -> Result<VersionPurgeReason, Status> {
    match reason {
        proto::VersionPurgeReasonProto::RetiredPurge => Ok(VersionPurgeReason::Retired),
        proto::VersionPurgeReasonProto::QuarantinedPurge => Ok(VersionPurgeReason::Quarantined),
        proto::VersionPurgeReasonProto::OperatorRequestedPurge => {
            Ok(VersionPurgeReason::OperatorRequested)
        }
        proto::VersionPurgeReasonProto::VersionPurgeReasonUnspecified => Err(
            Status::invalid_argument("version purge reason is unspecified"),
        ),
    }
}

#[must_use]
pub const fn integrity_state_to_proto(state: IntegrityState) -> proto::IntegrityStateProto {
    match state {
        IntegrityState::Verified => proto::IntegrityStateProto::VerifiedIntegrity,
        IntegrityState::Stale => proto::IntegrityStateProto::Stale,
        IntegrityState::IntegrityFailed => proto::IntegrityStateProto::IntegrityFailed,
        IntegrityState::Unknown => proto::IntegrityStateProto::UnknownIntegrity,
    }
}

pub fn integrity_state_from_proto(
    state: proto::IntegrityStateProto,
) -> Result<IntegrityState, Status> {
    match state {
        proto::IntegrityStateProto::VerifiedIntegrity => Ok(IntegrityState::Verified),
        proto::IntegrityStateProto::Stale => Ok(IntegrityState::Stale),
        proto::IntegrityStateProto::IntegrityFailed => Ok(IntegrityState::IntegrityFailed),
        proto::IntegrityStateProto::UnknownIntegrity => Ok(IntegrityState::Unknown),
        proto::IntegrityStateProto::IntegrityStateUnspecified => {
            Err(Status::invalid_argument("integrity state is unspecified"))
        }
    }
}

// ---------------------------------------------------------------------------
// Core records
// ---------------------------------------------------------------------------

#[must_use]
pub fn scope_binding_to_proto(binding: &ScopeBinding) -> proto::ScopeBindingProto {
    proto::ScopeBindingProto {
        scope_kind: i32::from(scope_kind_to_proto(binding.scope_kind)),
        group_id: binding.group_id.clone().unwrap_or_default(),
        user_id: binding.user_id.clone().unwrap_or_default(),
    }
}

pub fn scope_binding_from_proto(p: &proto::ScopeBindingProto) -> Result<ScopeBinding, Status> {
    Ok(ScopeBinding {
        scope_kind: scope_kind_from_proto(proto::ScopeKindProto::try_from(p.scope_kind).map_err(
            |_| Status::invalid_argument(format!("unknown scope kind value {}", p.scope_kind)),
        )?)?,
        group_id: (!p.group_id.is_empty()).then(|| p.group_id.clone()),
        user_id: (!p.user_id.is_empty()).then(|| p.user_id.clone()),
    })
}

#[must_use]
pub fn object_metadata_to_proto(metadata: &ObjectMetadata) -> proto::ObjectMetadataProto {
    proto::ObjectMetadataProto {
        name: metadata.name.clone(),
        labels: metadata.labels.clone(),
        mime: metadata.mime.clone(),
        extra: Some(json_to_struct(&metadata.extra)),
    }
}

pub fn object_metadata_from_proto(metadata: &proto::ObjectMetadataProto) -> ObjectMetadata {
    ObjectMetadata {
        name: metadata.name.clone(),
        labels: metadata.labels.clone(),
        mime: metadata.mime.clone(),
        extra: metadata
            .extra
            .as_ref()
            .map_or_else(|| JsonValue::Object(JsonMap::new()), struct_to_json),
    }
}

#[must_use]
pub fn object_to_proto(object: &Object) -> proto::ObjectProto {
    proto::ObjectProto {
        object_id: object.object_id.to_string(),
        kind: i32::from(object_kind_to_proto(object.kind)),
        created_at: Some(datetime_to_proto(object.created_at)),
        created_by: object.created_by.0.clone(),
        current_pointer_id: object.current_pointer_id.to_string(),
        metadata: Some(object_metadata_to_proto(&object.metadata)),
        policy_tags: object.policy_tags.clone(),
        privacy_class: i32::from(privacy_class_to_proto(object.privacy_class)),
        lifecycle_state: i32::from(lifecycle_state_to_proto(object.lifecycle_state)),
        retired_at: optional_datetime_to_proto(object.retired_at),
        purge_at: optional_datetime_to_proto(object.purge_at),
        index_hints: object.index_hints.clone(),
        scope_binding: Some(scope_binding_to_proto(&object.scope_binding)),
    }
}

pub fn object_from_proto(p: &proto::ObjectProto) -> Result<Object, Status> {
    Ok(Object {
        object_id: parse_object_id(&p.object_id)?,
        kind: object_kind_from_proto(proto::ObjectKindProto::try_from(p.kind).map_err(|_| {
            Status::invalid_argument(format!("unknown object kind value {}", p.kind))
        })?)?,
        created_at: datetime_from_proto(p.created_at.clone().unwrap_or_default()),
        created_by: SubjectRef(p.created_by.clone()),
        current_pointer_id: parse_pointer_id(&p.current_pointer_id)?,
        metadata: p.metadata.as_ref().map_or_else(
            || ObjectMetadata {
                name: String::new(),
                labels: Vec::new(),
                mime: String::new(),
                extra: JsonValue::Object(JsonMap::new()),
            },
            object_metadata_from_proto,
        ),
        policy_tags: p.policy_tags.clone(),
        privacy_class: privacy_class_from_proto(
            proto::PrivacyClassProto::try_from(p.privacy_class).map_err(|_| {
                Status::invalid_argument(format!("unknown privacy class value {}", p.privacy_class))
            })?,
        )?,
        lifecycle_state: lifecycle_state_from_proto(
            proto::LifecycleStateProto::try_from(p.lifecycle_state).map_err(|_| {
                Status::invalid_argument(format!(
                    "unknown lifecycle state value {}",
                    p.lifecycle_state
                ))
            })?,
        )?,
        retired_at: optional_datetime_from_proto(p.retired_at.clone()),
        purge_at: optional_datetime_from_proto(p.purge_at.clone()),
        index_hints: p.index_hints.clone(),
        scope_binding: p.scope_binding.as_ref().map_or_else(
            || {
                Ok(ScopeBinding {
                    scope_kind: ScopeKind::System,
                    group_id: None,
                    user_id: None,
                })
            },
            scope_binding_from_proto,
        )?,
    })
}

#[must_use]
pub fn version_to_proto(version: &Version) -> proto::VersionProto {
    proto::VersionProto {
        version_id: version.version_id.to_string(),
        object_id: version.object_id.to_string(),
        parent_version_ids: version
            .parent_version_ids
            .iter()
            .map(ToString::to_string)
            .collect(),
        chunk_refs: version
            .chunk_refs
            .iter()
            .map(|chunk_ref| chunk_ref.0.to_string())
            .collect(),
        content_hash: version.content_hash.clone(),
        metadata_delta: Some(json_to_struct(&version.metadata_delta)),
        created_by_action_id_proto: action_id_to_bytes(&version.created_by_action_id),
        created_by_transaction_id: version
            .created_by_transaction_id
            .as_ref()
            .map_or_else(String::new, ToString::to_string),
        created_at: Some(datetime_to_proto(version.created_at)),
        state: i32::from(version_state_to_proto(version.state)),
        quarantined_at: optional_datetime_to_proto(version.quarantined_at),
        quarantine_reason: version.quarantine_reason.clone().unwrap_or_default(),
    }
}

pub fn version_from_proto(p: &proto::VersionProto) -> Result<Version, Status> {
    Ok(Version {
        version_id: parse_version_id(&p.version_id)?,
        object_id: parse_object_id(&p.object_id)?,
        parent_version_ids: p
            .parent_version_ids
            .iter()
            .map(|id| parse_version_id(id))
            .collect::<Result<Vec<_>, _>>()?,
        chunk_refs: p
            .chunk_refs
            .iter()
            .map(|id| parse_chunk_id(id).map(ChunkRef))
            .collect::<Result<Vec<_>, _>>()?,
        content_hash: p.content_hash.clone(),
        metadata_delta: p
            .metadata_delta
            .as_ref()
            .map_or_else(|| JsonValue::Object(JsonMap::new()), struct_to_json),
        created_by_action_id: action_id_from_bytes(&p.created_by_action_id_proto)?,
        created_by_transaction_id: (!p.created_by_transaction_id.is_empty())
            .then(|| parse_transaction_id(&p.created_by_transaction_id))
            .transpose()?,
        created_at: datetime_from_proto(p.created_at.clone().unwrap_or_default()),
        state: version_state_from_proto(proto::VersionStateProto::try_from(p.state).map_err(
            |_| Status::invalid_argument(format!("unknown version state value {}", p.state)),
        )?)?,
        quarantined_at: optional_datetime_from_proto(p.quarantined_at.clone()),
        quarantine_reason: (!p.quarantine_reason.is_empty()).then(|| p.quarantine_reason.clone()),
    })
}

#[must_use]
pub fn chunk_to_proto(chunk: &Chunk) -> proto::ChunkProto {
    proto::ChunkProto {
        chunk_id: chunk.chunk_id.to_string(),
        size_bytes: chunk.size_bytes,
        ref_count: chunk.ref_count,
        created_at: Some(datetime_to_proto(chunk.created_at)),
    }
}

pub fn chunk_from_proto(p: &proto::ChunkProto) -> Result<Chunk, Status> {
    Ok(Chunk {
        chunk_id: parse_chunk_id(&p.chunk_id)?,
        size_bytes: p.size_bytes,
        ref_count: p.ref_count,
        created_at: datetime_from_proto(p.created_at.clone().unwrap_or_default()),
    })
}

#[must_use]
pub fn pointer_to_proto(pointer: &Pointer) -> proto::PointerProto {
    proto::PointerProto {
        pointer_id: pointer.pointer_id.to_string(),
        object_id: pointer.object_id.to_string(),
        kind: i32::from(pointer_kind_to_proto(pointer.kind)),
        current_version_id: pointer.current_version_id.to_string(),
        last_promoted_at: Some(datetime_to_proto(pointer.last_promoted_at)),
        last_promoted_by_transaction_id: pointer.last_promoted_by_transaction_id.to_string(),
    }
}

pub fn pointer_from_proto(p: &proto::PointerProto) -> Result<Pointer, Status> {
    Ok(Pointer {
        pointer_id: parse_pointer_id(&p.pointer_id)?,
        object_id: parse_object_id(&p.object_id)?,
        kind: pointer_kind_from_proto(proto::PointerKindProto::try_from(p.kind).map_err(
            |_| Status::invalid_argument(format!("unknown pointer kind value {}", p.kind)),
        )?)?,
        current_version_id: parse_version_id(&p.current_version_id)?,
        last_promoted_at: datetime_from_proto(p.last_promoted_at.clone().unwrap_or_default()),
        last_promoted_by_transaction_id: parse_transaction_id(&p.last_promoted_by_transaction_id)?,
    })
}

#[must_use]
pub fn snapshot_summary_to_proto(summary: &SnapshotSummary) -> proto::SnapshotSummaryProto {
    proto::SnapshotSummaryProto {
        snapshot_id: summary.snapshot_id.to_string(),
        at: Some(datetime_to_proto(summary.at)),
        object_count: summary.object_count,
        pointer_count: summary.pointer_count,
    }
}

pub fn snapshot_summary_from_proto(p: &proto::SnapshotSummaryProto) -> SnapshotSummary {
    SnapshotSummary {
        snapshot_id: SnapshotId(p.snapshot_id.clone()),
        at: datetime_from_proto(p.at.clone().unwrap_or_default()),
        object_count: p.object_count,
        pointer_count: p.pointer_count,
    }
}

#[must_use]
pub fn object_read_result_to_proto(read: &ObjectReadResult) -> proto::ObjectReadResultProto {
    proto::ObjectReadResultProto {
        object: Some(object_to_proto(&read.object)),
        version: Some(version_to_proto(&read.version)),
        chunks: read.chunks.iter().map(chunk_to_proto).collect(),
        snapshot_id: read.snapshot_id.to_string(),
    }
}

#[must_use]
pub fn object_write_result_to_proto(result: &ObjectWriteResult) -> proto::ObjectWriteResultProto {
    proto::ObjectWriteResultProto {
        object_id: result.object_id.to_string(),
        version_id: result.version_id.to_string(),
        transaction_id: result.transaction_id.to_string(),
        snapshot_id_after: result.snapshot_id_after.to_string(),
    }
}

pub fn object_write_request_from_proto(
    p: &proto::ObjectWriteRequestProto,
) -> Result<(ObjectWriteRequest, FsContext), Status> {
    let action_id = action_id_from_bytes(&p.action_id_proto)?;
    let consistency_class = consistency_class_from_proto(
        proto::ConsistencyClassProto::try_from(p.consistency_class).map_err(|_| {
            Status::invalid_argument(format!(
                "unknown consistency class value {}",
                p.consistency_class
            ))
        })?,
    )?;
    let subject = SubjectRef(p.subject.clone());
    let write = ObjectWriteRequest {
        object_id: (!p.object_id.is_empty())
            .then(|| parse_object_id(&p.object_id))
            .transpose()?,
        parent_version_ids: p
            .parent_version_ids
            .iter()
            .map(|id| parse_version_id(id))
            .collect::<Result<Vec<_>, _>>()?,
        chunks: p
            .chunk_refs
            .iter()
            .map(|id| parse_chunk_id(id).map(ChunkRef))
            .collect::<Result<Vec<_>, _>>()?,
        metadata_delta: p
            .metadata_delta
            .as_ref()
            .map_or_else(|| JsonValue::Object(JsonMap::new()), struct_to_json),
        action_id: action_id.clone(),
        subject: subject.clone(),
    };
    let context = FsContext {
        subject,
        action_id,
        expected_snapshot_id: snapshot_from_string(&p.expected_snapshot_id),
        consistency_class,
    };
    Ok((write, context))
}

// ---------------------------------------------------------------------------
// Query / view
// ---------------------------------------------------------------------------

#[must_use]
pub const fn query_namespace_to_proto(namespace: QueryNamespace) -> proto::QueryNamespaceProto {
    match namespace {
        QueryNamespace::Object => proto::QueryNamespaceProto::Object,
        QueryNamespace::Version => proto::QueryNamespaceProto::Version,
        QueryNamespace::Pointer => proto::QueryNamespaceProto::Pointer,
        QueryNamespace::Chunk => proto::QueryNamespaceProto::Chunk,
        QueryNamespace::Namespace => proto::QueryNamespaceProto::Namespace,
    }
}

pub fn query_namespace_from_proto(
    namespace: proto::QueryNamespaceProto,
) -> Result<QueryNamespace, Status> {
    match namespace {
        proto::QueryNamespaceProto::Object => Ok(QueryNamespace::Object),
        proto::QueryNamespaceProto::Version => Ok(QueryNamespace::Version),
        proto::QueryNamespaceProto::Pointer => Ok(QueryNamespace::Pointer),
        proto::QueryNamespaceProto::Chunk => Ok(QueryNamespace::Chunk),
        proto::QueryNamespaceProto::Namespace => Ok(QueryNamespace::Namespace),
        proto::QueryNamespaceProto::QueryNamespaceUnspecified => {
            Err(Status::invalid_argument("query namespace is unspecified"))
        }
    }
}

#[must_use]
pub const fn query_field_to_proto(field: QueryField) -> proto::QueryFieldProto {
    match field {
        QueryField::ObjectKind => proto::QueryFieldProto::ObjectKind,
        QueryField::ObjectPrivacyClass => proto::QueryFieldProto::ObjectPrivacyClass,
        QueryField::ObjectLifecycleState => proto::QueryFieldProto::ObjectLifecycleState,
        QueryField::ObjectMetadataName => proto::QueryFieldProto::ObjectMetadataName,
        QueryField::ObjectPolicyTags => proto::QueryFieldProto::ObjectPolicyTags,
        QueryField::VersionState => proto::QueryFieldProto::VersionState,
        QueryField::VersionCreatedAt => proto::QueryFieldProto::VersionCreatedAt,
        QueryField::VersionCreatedBy => proto::QueryFieldProto::VersionCreatedBy,
        QueryField::PointerKind => proto::QueryFieldProto::PointerKind,
        QueryField::NamespaceClass => proto::QueryFieldProto::NamespaceClass,
    }
}

pub fn query_field_from_proto(field: proto::QueryFieldProto) -> Result<QueryField, Status> {
    match field {
        proto::QueryFieldProto::ObjectKind => Ok(QueryField::ObjectKind),
        proto::QueryFieldProto::ObjectPrivacyClass => Ok(QueryField::ObjectPrivacyClass),
        proto::QueryFieldProto::ObjectLifecycleState => Ok(QueryField::ObjectLifecycleState),
        proto::QueryFieldProto::ObjectMetadataName => Ok(QueryField::ObjectMetadataName),
        proto::QueryFieldProto::ObjectPolicyTags => Ok(QueryField::ObjectPolicyTags),
        proto::QueryFieldProto::VersionState => Ok(QueryField::VersionState),
        proto::QueryFieldProto::VersionCreatedAt => Ok(QueryField::VersionCreatedAt),
        proto::QueryFieldProto::VersionCreatedBy => Ok(QueryField::VersionCreatedBy),
        proto::QueryFieldProto::PointerKind => Ok(QueryField::PointerKind),
        proto::QueryFieldProto::NamespaceClass => Ok(QueryField::NamespaceClass),
        proto::QueryFieldProto::QueryFieldUnspecified => {
            Err(Status::invalid_argument("query field is unspecified"))
        }
    }
}

#[must_use]
pub const fn query_operator_to_proto(op: QueryOperator) -> proto::QueryOperatorProto {
    match op {
        QueryOperator::Eq => proto::QueryOperatorProto::Eq,
        QueryOperator::Neq => proto::QueryOperatorProto::Neq,
        QueryOperator::Lt => proto::QueryOperatorProto::Lt,
        QueryOperator::Lte => proto::QueryOperatorProto::Lte,
        QueryOperator::Gt => proto::QueryOperatorProto::Gt,
        QueryOperator::Gte => proto::QueryOperatorProto::Gte,
        QueryOperator::In => proto::QueryOperatorProto::In,
        QueryOperator::Contains => proto::QueryOperatorProto::Contains,
        QueryOperator::Matches => proto::QueryOperatorProto::Matches,
    }
}

pub fn query_operator_from_proto(op: proto::QueryOperatorProto) -> Result<QueryOperator, Status> {
    match op {
        proto::QueryOperatorProto::Eq => Ok(QueryOperator::Eq),
        proto::QueryOperatorProto::Neq => Ok(QueryOperator::Neq),
        proto::QueryOperatorProto::Lt => Ok(QueryOperator::Lt),
        proto::QueryOperatorProto::Lte => Ok(QueryOperator::Lte),
        proto::QueryOperatorProto::Gt => Ok(QueryOperator::Gt),
        proto::QueryOperatorProto::Gte => Ok(QueryOperator::Gte),
        proto::QueryOperatorProto::In => Ok(QueryOperator::In),
        proto::QueryOperatorProto::Contains => Ok(QueryOperator::Contains),
        proto::QueryOperatorProto::Matches => Ok(QueryOperator::Matches),
        proto::QueryOperatorProto::QueryOperatorUnspecified => {
            Err(Status::invalid_argument("query operator is unspecified"))
        }
    }
}

#[must_use]
pub fn query_value_to_proto(value: &QueryValue) -> proto::QueryValueProto {
    let value = match value {
        QueryValue::String(value) => proto::query_value_proto::Value::StringValue(value.clone()),
        QueryValue::Int(value) => proto::query_value_proto::Value::IntValue(*value),
        QueryValue::Bool(value) => proto::query_value_proto::Value::BoolValue(*value),
        QueryValue::StringList(values) => {
            proto::query_value_proto::Value::StringList(proto::StringListProto {
                values: values.clone(),
            })
        }
        QueryValue::TimeRange { start, end } => {
            proto::query_value_proto::Value::TimeRange(proto::TimeRangeProto {
                start: start.clone(),
                end: end.clone(),
            })
        }
    };
    proto::QueryValueProto { value: Some(value) }
}

pub fn query_value_from_proto(value: &proto::QueryValueProto) -> Result<QueryValue, Status> {
    match value.value.as_ref() {
        Some(proto::query_value_proto::Value::StringValue(value)) => {
            Ok(QueryValue::String(value.clone()))
        }
        Some(proto::query_value_proto::Value::IntValue(value)) => Ok(QueryValue::Int(*value)),
        Some(proto::query_value_proto::Value::BoolValue(value)) => Ok(QueryValue::Bool(*value)),
        Some(proto::query_value_proto::Value::StringList(values)) => {
            Ok(QueryValue::StringList(values.values.clone()))
        }
        Some(proto::query_value_proto::Value::TimeRange(range)) => Ok(QueryValue::TimeRange {
            start: range.start.clone(),
            end: range.end.clone(),
        }),
        None => Err(Status::invalid_argument("query value is required")),
    }
}

#[must_use]
pub fn predicate_to_proto(predicate: &Predicate) -> proto::PredicateProto {
    proto::PredicateProto {
        namespace: i32::from(query_namespace_to_proto(predicate.namespace)),
        field: i32::from(query_field_to_proto(predicate.field)),
        op: i32::from(query_operator_to_proto(predicate.op)),
        rhs: Some(query_value_to_proto(&predicate.rhs)),
    }
}

pub fn predicate_from_proto(p: &proto::PredicateProto) -> Result<Predicate, Status> {
    Ok(Predicate {
        namespace: query_namespace_from_proto(
            proto::QueryNamespaceProto::try_from(p.namespace).map_err(|_| {
                Status::invalid_argument(format!("unknown query namespace value {}", p.namespace))
            })?,
        )?,
        field: query_field_from_proto(proto::QueryFieldProto::try_from(p.field).map_err(
            |_| Status::invalid_argument(format!("unknown query field value {}", p.field)),
        )?)?,
        op: query_operator_from_proto(proto::QueryOperatorProto::try_from(p.op).map_err(
            |_| Status::invalid_argument(format!("unknown query operator value {}", p.op)),
        )?)?,
        rhs: query_value_from_proto(
            p.rhs
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("predicate rhs is required"))?,
        )?,
    })
}

#[must_use]
pub fn query_to_proto(query: &Query) -> proto::QueryProto {
    match query {
        Query::And(predicates) => proto::QueryProto {
            source: String::new(),
            predicates: predicates.iter().map(predicate_to_proto).collect(),
        },
    }
}

pub fn query_from_proto(query: &proto::QueryProto) -> Result<Query, Status> {
    if !query.source.trim().is_empty() {
        return crate::query_parser::parse(&query.source)
            .map_err(|err| fs_error_to_status(&FsError::QueryParse(err.to_string())));
    }
    Ok(Query::And(
        query
            .predicates
            .iter()
            .map(predicate_from_proto)
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

#[must_use]
pub fn object_ref_to_proto(reference: &ObjectRef) -> proto::ObjectRefProto {
    proto::ObjectRefProto {
        object_id: reference.object_id.to_string(),
    }
}

pub fn object_ref_from_proto(reference: &proto::ObjectRefProto) -> Result<ObjectRef, Status> {
    Ok(ObjectRef {
        object_id: parse_object_id(&reference.object_id)?,
    })
}

#[must_use]
pub fn view_to_proto(view: &View) -> proto::ViewProto {
    proto::ViewProto {
        snapshot_id: view.snapshot_id.to_string(),
        matched: view.matched.iter().map(object_ref_to_proto).collect(),
        query_hash: view.query_hash.clone(),
    }
}

pub fn view_from_proto(view: &proto::ViewProto) -> Result<View, Status> {
    Ok(View {
        snapshot_id: SnapshotId(view.snapshot_id.clone()),
        matched: view
            .matched
            .iter()
            .map(object_ref_from_proto)
            .collect::<Result<Vec<_>, _>>()?,
        query_hash: view.query_hash.clone(),
    })
}

// ---------------------------------------------------------------------------
// Quarantine / GC
// ---------------------------------------------------------------------------

#[must_use]
pub fn quarantine_receipt_to_proto(receipt: &QuarantineReceipt) -> proto::QuarantineReceiptProto {
    proto::QuarantineReceiptProto {
        quarantine_id: receipt.quarantine_id.clone(),
        version_id: receipt.version_id.to_string(),
        transitioned_at: Some(datetime_to_proto(receipt.transitioned_at)),
        trigger: receipt
            .trigger
            .map_or(0, |trigger| i32::from(quarantine_trigger_to_proto(trigger))),
        disposition: receipt.disposition.map_or(0, |disposition| {
            i32::from(quarantine_disposition_to_proto(disposition))
        }),
        reason: receipt.reason.clone(),
    }
}

pub fn quarantine_receipt_from_proto(
    receipt: &proto::QuarantineReceiptProto,
) -> Result<QuarantineReceipt, Status> {
    let trigger = proto::QuarantineTriggerProto::try_from(receipt.trigger)
        .ok()
        .filter(|trigger| *trigger != proto::QuarantineTriggerProto::QuarantineTriggerUnspecified)
        .map(quarantine_trigger_from_proto)
        .transpose()?;
    let disposition = proto::QuarantineDispositionProto::try_from(receipt.disposition)
        .ok()
        .filter(|disposition| {
            *disposition != proto::QuarantineDispositionProto::QuarantineDispositionUnspecified
        })
        .map(quarantine_disposition_from_proto)
        .transpose()?;
    Ok(QuarantineReceipt {
        quarantine_id: receipt.quarantine_id.clone(),
        version_id: parse_version_id(&receipt.version_id)?,
        transitioned_at: datetime_from_proto(receipt.transitioned_at.clone().unwrap_or_default()),
        trigger,
        disposition,
        reason: receipt.reason.clone(),
    })
}

#[must_use]
pub fn gc_reason_to_proto(reason: &GcReason) -> proto::GcReasonProto {
    let reason = match reason {
        GcReason::OrphanChunkReclaimed { chunk_id } => {
            proto::gc_reason_proto::Reason::OrphanChunkReclaimed(chunk_id.to_string())
        }
        GcReason::VersionPurged { version_id, reason } => {
            proto::gc_reason_proto::Reason::VersionPurged(proto::VersionPurgedReasonProto {
                version_id: version_id.to_string(),
                reason: i32::from(version_purge_reason_to_proto(*reason)),
            })
        }
    };
    proto::GcReasonProto {
        reason: Some(reason),
    }
}

pub fn gc_reason_from_proto(reason: &proto::GcReasonProto) -> Result<GcReason, Status> {
    match reason.reason.as_ref() {
        Some(proto::gc_reason_proto::Reason::OrphanChunkReclaimed(chunk_id)) => {
            Ok(GcReason::OrphanChunkReclaimed {
                chunk_id: parse_chunk_id(chunk_id)?,
            })
        }
        Some(proto::gc_reason_proto::Reason::VersionPurged(purged)) => {
            Ok(GcReason::VersionPurged {
                version_id: parse_version_id(&purged.version_id)?,
                reason: version_purge_reason_from_proto(
                    proto::VersionPurgeReasonProto::try_from(purged.reason).map_err(|_| {
                        Status::invalid_argument(format!(
                            "unknown version purge reason value {}",
                            purged.reason
                        ))
                    })?,
                )?,
            })
        }
        None => Err(Status::invalid_argument("gc reason is required")),
    }
}

#[must_use]
pub fn gc_pass_report_to_proto(report: &GcPassReport) -> proto::GcPassReportProto {
    proto::GcPassReportProto {
        pass_id: report.pass_id.clone(),
        started_at: Some(datetime_to_proto(report.started_at)),
        completed_at: Some(datetime_to_proto(report.completed_at)),
        chunks_inspected: report.chunks_inspected,
        chunks_reclaimed: report.chunks_reclaimed,
        versions_inspected: report.versions_inspected,
        versions_purged: report.versions_purged,
        reasons: report.reasons.iter().map(gc_reason_to_proto).collect(),
    }
}

pub fn gc_pass_report_from_proto(
    report: &proto::GcPassReportProto,
) -> Result<GcPassReport, Status> {
    Ok(GcPassReport {
        pass_id: report.pass_id.clone(),
        started_at: datetime_from_proto(report.started_at.clone().unwrap_or_default()),
        completed_at: datetime_from_proto(report.completed_at.clone().unwrap_or_default()),
        chunks_inspected: report.chunks_inspected,
        chunks_reclaimed: report.chunks_reclaimed,
        versions_inspected: report.versions_inspected,
        versions_purged: report.versions_purged,
        reasons: report
            .reasons
            .iter()
            .map(gc_reason_from_proto)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

// ---------------------------------------------------------------------------
// Implementation space
// ---------------------------------------------------------------------------

#[must_use]
pub fn impl_space_target_to_proto(target: &ImplSpaceTarget) -> proto::ImplSpaceTargetProto {
    let target = match target {
        ImplSpaceTarget::LocalFile { path } => {
            proto::impl_space_target_proto::Target::LocalFile(proto::LocalFileTargetProto {
                path: path.clone(),
            })
        }
        ImplSpaceTarget::EncryptedBlob {
            blob_id,
            key_capability_id,
        } => {
            proto::impl_space_target_proto::Target::EncryptedBlob(proto::EncryptedBlobTargetProto {
                blob_id: blob_id.clone(),
                key_capability_id: key_capability_id.clone(),
            })
        }
        ImplSpaceTarget::RemoteBlob { url, etag } => {
            proto::impl_space_target_proto::Target::RemoteBlob(proto::RemoteBlobTargetProto {
                url: url.clone(),
                etag: etag.clone().unwrap_or_default(),
            })
        }
        ImplSpaceTarget::AiosFsManaged { handle } => {
            proto::impl_space_target_proto::Target::AiosFsManaged(proto::AiosFsManagedTargetProto {
                handle: handle.clone(),
            })
        }
    };
    proto::ImplSpaceTargetProto {
        target: Some(target),
    }
}

pub fn impl_space_target_from_proto(
    target: &proto::ImplSpaceTargetProto,
) -> Result<ImplSpaceTarget, Status> {
    match target.target.as_ref() {
        Some(proto::impl_space_target_proto::Target::LocalFile(target)) => {
            Ok(ImplSpaceTarget::LocalFile {
                path: target.path.clone(),
            })
        }
        Some(proto::impl_space_target_proto::Target::EncryptedBlob(target)) => {
            Ok(ImplSpaceTarget::EncryptedBlob {
                blob_id: target.blob_id.clone(),
                key_capability_id: target.key_capability_id.clone(),
            })
        }
        Some(proto::impl_space_target_proto::Target::RemoteBlob(target)) => {
            Ok(ImplSpaceTarget::RemoteBlob {
                url: target.url.clone(),
                etag: (!target.etag.is_empty()).then(|| target.etag.clone()),
            })
        }
        Some(proto::impl_space_target_proto::Target::AiosFsManaged(target)) => {
            Ok(ImplSpaceTarget::AiosFsManaged {
                handle: target.handle.clone(),
            })
        }
        None => Err(Status::invalid_argument(
            "impl-space target oneof value is required",
        )),
    }
}

#[must_use]
pub fn impl_space_source_to_proto(source: &ImplSpaceSource) -> proto::ImplSpaceSourceProto {
    let source = match source {
        ImplSpaceSource::Object(id) => {
            proto::impl_space_source_proto::Source::ObjectId(id.to_string())
        }
        ImplSpaceSource::Chunk(id) => {
            proto::impl_space_source_proto::Source::ChunkId(id.to_string())
        }
        ImplSpaceSource::Version(id) => {
            proto::impl_space_source_proto::Source::VersionId(id.to_string())
        }
    };
    proto::ImplSpaceSourceProto {
        source: Some(source),
    }
}

pub fn impl_space_source_from_proto(
    source: &proto::ImplSpaceSourceProto,
) -> Result<ImplSpaceSource, Status> {
    match source.source.as_ref() {
        Some(proto::impl_space_source_proto::Source::ObjectId(id)) => {
            Ok(ImplSpaceSource::Object(parse_object_id(id)?))
        }
        Some(proto::impl_space_source_proto::Source::ChunkId(id)) => {
            Ok(ImplSpaceSource::Chunk(parse_chunk_id(id)?))
        }
        Some(proto::impl_space_source_proto::Source::VersionId(id)) => {
            Ok(ImplSpaceSource::Version(parse_version_id(id)?))
        }
        None => Err(Status::invalid_argument(
            "impl-space source oneof value is required",
        )),
    }
}

#[must_use]
pub fn impl_space_binding_to_proto(binding: &ImplSpaceBinding) -> proto::ImplSpaceBindingProto {
    proto::ImplSpaceBindingProto {
        binding_id: binding.binding_id.clone(),
        object_or_chunk_id: Some(impl_space_source_to_proto(&binding.object_or_chunk_id)),
        target: Some(impl_space_target_to_proto(&binding.target)),
        created_at: Some(datetime_to_proto(binding.created_at)),
        created_by: binding.created_by.0.clone(),
        last_verified_at: optional_datetime_to_proto(binding.last_verified_at),
        integrity_state: i32::from(integrity_state_to_proto(binding.integrity_state)),
    }
}

pub fn impl_space_binding_from_proto(
    binding: &proto::ImplSpaceBindingProto,
) -> Result<ImplSpaceBinding, Status> {
    Ok(ImplSpaceBinding {
        binding_id: binding.binding_id.clone(),
        object_or_chunk_id: impl_space_source_from_proto(
            binding
                .object_or_chunk_id
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("impl-space source is required"))?,
        )?,
        target: impl_space_target_from_proto(
            binding
                .target
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("impl-space target is required"))?,
        )?,
        created_at: datetime_from_proto(binding.created_at.clone().unwrap_or_default()),
        created_by: SubjectRef(binding.created_by.clone()),
        last_verified_at: optional_datetime_from_proto(binding.last_verified_at.clone()),
        integrity_state: integrity_state_from_proto(
            proto::IntegrityStateProto::try_from(binding.integrity_state).map_err(|_| {
                Status::invalid_argument(format!(
                    "unknown integrity state value {}",
                    binding.integrity_state
                ))
            })?,
        )?,
    })
}
