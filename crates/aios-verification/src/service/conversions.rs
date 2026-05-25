//! Rust-to-proto translations for the gRPC `VerificationEngine` surface (T-069).
//!
//! The conversion layer is the only place that knows about prost-generated
//! message shapes. The core verification model remains tonic-free.

#![allow(
    missing_docs,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::result_large_err,
    reason = "conversion function names are intentionally literal and covered by the module docs"
)]

use aios_action::ActionId;
use chrono::{DateTime, TimeZone, Utc};
use prost_types::{value::Kind, ListValue, NullValue, Timestamp, Value as ProstValue};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use tonic::Status;

use crate::service::proto;
use crate::{
    IntentId, PrimitiveResult, VerificationError, VerificationIntent, VerificationPrimitive,
    VerificationResult, VerificationStatus,
};

// ---------------------------------------------------------------------------
// Timestamp helpers
// ---------------------------------------------------------------------------

/// Convert a `chrono::DateTime<Utc>` into the prost well-known `Timestamp`.
pub fn datetime_to_proto(dt: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: dt.timestamp(),
        nanos: i32::try_from(dt.timestamp_subsec_nanos()).unwrap_or(0),
    }
}

/// Convert the prost well-known `Timestamp` back into `chrono::DateTime<Utc>`.
pub fn datetime_from_proto(ts: Timestamp) -> DateTime<Utc> {
    Utc.timestamp_opt(ts.seconds, u32::try_from(ts.nanos).unwrap_or(0))
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap_or_default())
}

fn required_datetime_from_proto(
    ts: Option<Timestamp>,
    field: &'static str,
) -> Result<DateTime<Utc>, Status> {
    ts.map(datetime_from_proto)
        .ok_or_else(|| Status::invalid_argument(format!("{field} is required")))
}

// ---------------------------------------------------------------------------
// JSON Value helpers
// ---------------------------------------------------------------------------

pub fn json_to_prost_value(value: &JsonValue) -> ProstValue {
    let kind = match value {
        JsonValue::Null => Kind::NullValue(NullValue::NullValue as i32),
        JsonValue::Bool(value) => Kind::BoolValue(*value),
        JsonValue::Number(value) => Kind::NumberValue(value.as_f64().unwrap_or_default()),
        JsonValue::String(value) => Kind::StringValue(value.clone()),
        JsonValue::Array(values) => Kind::ListValue(ListValue {
            values: values.iter().map(json_to_prost_value).collect(),
        }),
        JsonValue::Object(map) => Kind::StructValue(prost_types::Struct {
            fields: map
                .iter()
                .map(|(key, value)| (key.clone(), json_to_prost_value(value)))
                .collect(),
        }),
    };
    ProstValue { kind: Some(kind) }
}

pub fn prost_value_to_json(value: &ProstValue) -> JsonValue {
    match value.kind.as_ref() {
        Some(Kind::NullValue(_)) | None => JsonValue::Null,
        Some(Kind::NumberValue(value)) => number_to_json(*value),
        Some(Kind::StringValue(value)) => JsonValue::String(value.clone()),
        Some(Kind::BoolValue(value)) => JsonValue::Bool(*value),
        Some(Kind::StructValue(value)) => JsonValue::Object(
            value
                .fields
                .iter()
                .map(|(key, value)| (key.clone(), prost_value_to_json(value)))
                .collect::<JsonMap<String, JsonValue>>(),
        ),
        Some(Kind::ListValue(value)) => {
            JsonValue::Array(value.values.iter().map(prost_value_to_json).collect())
        }
    }
}

fn number_to_json(value: f64) -> JsonValue {
    JsonNumber::from_f64(value).map_or(JsonValue::Null, JsonValue::Number)
}

fn optional_prost_value_to_json(value: Option<&ProstValue>) -> JsonValue {
    value.map_or(JsonValue::Null, prost_value_to_json)
}

// ---------------------------------------------------------------------------
// Error -> tonic::Status
// ---------------------------------------------------------------------------

/// Map typed [`VerificationError`] values onto canonical gRPC status codes.
pub fn verification_error_to_status(err: &VerificationError) -> Status {
    match err {
        VerificationError::UnknownPrimitive(_)
        | VerificationError::IntentParseFailed(_)
        | VerificationError::InvalidIntent(_) => Status::invalid_argument(err.to_string()),
        VerificationError::TimeoutExceeded { .. } => Status::deadline_exceeded(err.to_string()),
        VerificationError::PrimitiveExecutionFailed { .. } => {
            Status::failed_precondition(err.to_string())
        }
        VerificationError::Internal(_) => Status::internal(err.to_string()),
    }
}

// ---------------------------------------------------------------------------
// ID helpers
// ---------------------------------------------------------------------------

pub fn action_id_to_proto(action_id: &ActionId) -> Vec<u8> {
    action_id.as_str().as_bytes().to_vec()
}

pub fn action_id_from_proto(bytes: &[u8]) -> Result<ActionId, Status> {
    if bytes.is_empty() {
        return Err(Status::invalid_argument("action_id_proto is required"));
    }
    let raw = std::str::from_utf8(bytes)
        .map_err(|err| Status::invalid_argument(format!("action_id_proto is not UTF-8: {err}")))?;
    ActionId::parse(raw)
        .map_err(|err| Status::invalid_argument(format!("invalid action id `{raw}`: {err}")))
}

pub fn intent_id_from_string(input: String) -> Result<IntentId, Status> {
    if input.is_empty() {
        return Ok(IntentId::new());
    }
    if input.starts_with(IntentId::PREFIX) {
        return Ok(IntentId(input));
    }
    Err(Status::invalid_argument(format!(
        "intent_id must start with `{}`",
        IntentId::PREFIX
    )))
}

// ---------------------------------------------------------------------------
// Enum conversions
// ---------------------------------------------------------------------------

pub const fn verification_status_to_proto(
    status: VerificationStatus,
) -> proto::VerificationStatusProto {
    match status {
        VerificationStatus::Passed => proto::VerificationStatusProto::VerificationPassed,
        VerificationStatus::Failed => proto::VerificationStatusProto::VerificationFailed,
        VerificationStatus::Timeout => proto::VerificationStatusProto::VerificationTimeout,
        VerificationStatus::ProbeError => proto::VerificationStatusProto::VerificationProbeError,
        VerificationStatus::Skipped => proto::VerificationStatusProto::VerificationSkipped,
    }
}

pub fn verification_status_from_proto(
    status: proto::VerificationStatusProto,
) -> Result<VerificationStatus, Status> {
    match status {
        proto::VerificationStatusProto::VerificationPassed => Ok(VerificationStatus::Passed),
        proto::VerificationStatusProto::VerificationFailed => Ok(VerificationStatus::Failed),
        proto::VerificationStatusProto::VerificationTimeout => Ok(VerificationStatus::Timeout),
        proto::VerificationStatusProto::VerificationProbeError => {
            Ok(VerificationStatus::ProbeError)
        }
        proto::VerificationStatusProto::VerificationSkipped => Ok(VerificationStatus::Skipped),
        proto::VerificationStatusProto::VerificationStatusUnspecified => Err(
            Status::invalid_argument("verification status is unspecified"),
        ),
    }
}

pub const fn verification_primitive_to_proto(
    primitive: VerificationPrimitive,
) -> proto::VerificationPrimitiveProto {
    match primitive {
        VerificationPrimitive::ServiceActive => proto::VerificationPrimitiveProto::ServiceActive,
        VerificationPrimitive::ServiceInactive => {
            proto::VerificationPrimitiveProto::ServiceInactive
        }
        VerificationPrimitive::PackageInstalled => {
            proto::VerificationPrimitiveProto::PackageInstalled
        }
        VerificationPrimitive::PortOpen => proto::VerificationPrimitiveProto::PortOpen,
        VerificationPrimitive::PortClosed => proto::VerificationPrimitiveProto::PortClosed,
        VerificationPrimitive::HttpOk => proto::VerificationPrimitiveProto::HttpOk,
        VerificationPrimitive::FileExists => proto::VerificationPrimitiveProto::FileExists,
        VerificationPrimitive::FileHash => proto::VerificationPrimitiveProto::FileHash,
        VerificationPrimitive::RepoExists => proto::VerificationPrimitiveProto::RepoExists,
        VerificationPrimitive::AiosfsPointer => proto::VerificationPrimitiveProto::AiosfsPointer,
        VerificationPrimitive::PolicyDecision => proto::VerificationPrimitiveProto::PolicyDecision,
        VerificationPrimitive::EvidenceExists => proto::VerificationPrimitiveProto::EvidenceExists,
        VerificationPrimitive::NetworkSubjectOutboundClass => {
            proto::VerificationPrimitiveProto::NetworkSubjectOutboundClass
        }
        VerificationPrimitive::NetworkActiveExposureClass => {
            proto::VerificationPrimitiveProto::NetworkActiveExposureClass
        }
        VerificationPrimitive::NetworkExternalModelCallBrokeredOnly => {
            proto::VerificationPrimitiveProto::NetworkExternalModelCallBrokeredOnly
        }
        VerificationPrimitive::DnsResolverBackend => {
            proto::VerificationPrimitiveProto::DnsResolverBackend
        }
        VerificationPrimitive::VpnTunnelActive => {
            proto::VerificationPrimitiveProto::VpnTunnelActive
        }
        VerificationPrimitive::MdnsPosture => proto::VerificationPrimitiveProto::MdnsPosture,
        VerificationPrimitive::AiosfsPathInNamespace => {
            proto::VerificationPrimitiveProto::AiosfsPathInNamespace
        }
        VerificationPrimitive::SurfaceInZone => proto::VerificationPrimitiveProto::SurfaceInZone,
        VerificationPrimitive::TreeContainsKind => {
            proto::VerificationPrimitiveProto::TreeContainsKind
        }
        VerificationPrimitive::TreeMaxDepth => proto::VerificationPrimitiveProto::TreeMaxDepth,
        VerificationPrimitive::ThemeSatisfiesInvariants => {
            proto::VerificationPrimitiveProto::ThemeSatisfiesInvariants
        }
        VerificationPrimitive::ThemeConstitutionalIconsIntact => {
            proto::VerificationPrimitiveProto::ThemeConstitutionalIconsIntact
        }
        VerificationPrimitive::GpuBindingClass => {
            proto::VerificationPrimitiveProto::GpuBindingClass
        }
        VerificationPrimitive::WebRendererBoundTo => {
            proto::VerificationPrimitiveProto::WebRendererBoundTo
        }
        VerificationPrimitive::WebChromeZIndexAtLeast => {
            proto::VerificationPrimitiveProto::WebChromeZIndexAtLeast
        }
        VerificationPrimitive::AiosfsPathOwnerResolved => {
            proto::VerificationPrimitiveProto::AiosfsPathOwnerResolved
        }
        VerificationPrimitive::AiosfsPathRecoveryTreatmentSet => {
            proto::VerificationPrimitiveProto::AiosfsPathRecoveryTreatmentSet
        }
        VerificationPrimitive::NamespaceCatalogVersion => {
            proto::VerificationPrimitiveProto::NamespaceCatalogVersion
        }
        VerificationPrimitive::StatusIndicatorVisible => {
            proto::VerificationPrimitiveProto::StatusIndicatorVisible
        }
        VerificationPrimitive::SubjectSessionFlagState => {
            proto::VerificationPrimitiveProto::SubjectSessionFlagState
        }
        VerificationPrimitive::FilesystemRootIntact => {
            proto::VerificationPrimitiveProto::FilesystemRootIntact
        }
        VerificationPrimitive::SpecConsumesTable => {
            proto::VerificationPrimitiveProto::SpecConsumesTable
        }
        VerificationPrimitive::ApprovalBindingState => {
            proto::VerificationPrimitiveProto::ApprovalBindingState
        }
        VerificationPrimitive::SecretPatternMatch => {
            proto::VerificationPrimitiveProto::SecretPatternMatch
        }
    }
}

pub fn verification_primitive_from_proto(
    primitive: proto::VerificationPrimitiveProto,
) -> Result<VerificationPrimitive, Status> {
    match primitive {
        proto::VerificationPrimitiveProto::ServiceActive => {
            Ok(VerificationPrimitive::ServiceActive)
        }
        proto::VerificationPrimitiveProto::ServiceInactive => {
            Ok(VerificationPrimitive::ServiceInactive)
        }
        proto::VerificationPrimitiveProto::PackageInstalled => {
            Ok(VerificationPrimitive::PackageInstalled)
        }
        proto::VerificationPrimitiveProto::PortOpen => Ok(VerificationPrimitive::PortOpen),
        proto::VerificationPrimitiveProto::PortClosed => Ok(VerificationPrimitive::PortClosed),
        proto::VerificationPrimitiveProto::HttpOk => Ok(VerificationPrimitive::HttpOk),
        proto::VerificationPrimitiveProto::FileExists => Ok(VerificationPrimitive::FileExists),
        proto::VerificationPrimitiveProto::FileHash => Ok(VerificationPrimitive::FileHash),
        proto::VerificationPrimitiveProto::RepoExists => Ok(VerificationPrimitive::RepoExists),
        proto::VerificationPrimitiveProto::AiosfsPointer => {
            Ok(VerificationPrimitive::AiosfsPointer)
        }
        proto::VerificationPrimitiveProto::PolicyDecision => {
            Ok(VerificationPrimitive::PolicyDecision)
        }
        proto::VerificationPrimitiveProto::EvidenceExists => {
            Ok(VerificationPrimitive::EvidenceExists)
        }
        proto::VerificationPrimitiveProto::NetworkSubjectOutboundClass => {
            Ok(VerificationPrimitive::NetworkSubjectOutboundClass)
        }
        proto::VerificationPrimitiveProto::NetworkActiveExposureClass => {
            Ok(VerificationPrimitive::NetworkActiveExposureClass)
        }
        proto::VerificationPrimitiveProto::NetworkExternalModelCallBrokeredOnly => {
            Ok(VerificationPrimitive::NetworkExternalModelCallBrokeredOnly)
        }
        proto::VerificationPrimitiveProto::DnsResolverBackend => {
            Ok(VerificationPrimitive::DnsResolverBackend)
        }
        proto::VerificationPrimitiveProto::VpnTunnelActive => {
            Ok(VerificationPrimitive::VpnTunnelActive)
        }
        proto::VerificationPrimitiveProto::MdnsPosture => Ok(VerificationPrimitive::MdnsPosture),
        proto::VerificationPrimitiveProto::AiosfsPathInNamespace => {
            Ok(VerificationPrimitive::AiosfsPathInNamespace)
        }
        proto::VerificationPrimitiveProto::SurfaceInZone => {
            Ok(VerificationPrimitive::SurfaceInZone)
        }
        proto::VerificationPrimitiveProto::TreeContainsKind => {
            Ok(VerificationPrimitive::TreeContainsKind)
        }
        proto::VerificationPrimitiveProto::TreeMaxDepth => Ok(VerificationPrimitive::TreeMaxDepth),
        proto::VerificationPrimitiveProto::ThemeSatisfiesInvariants => {
            Ok(VerificationPrimitive::ThemeSatisfiesInvariants)
        }
        proto::VerificationPrimitiveProto::ThemeConstitutionalIconsIntact => {
            Ok(VerificationPrimitive::ThemeConstitutionalIconsIntact)
        }
        proto::VerificationPrimitiveProto::GpuBindingClass => {
            Ok(VerificationPrimitive::GpuBindingClass)
        }
        proto::VerificationPrimitiveProto::WebRendererBoundTo => {
            Ok(VerificationPrimitive::WebRendererBoundTo)
        }
        proto::VerificationPrimitiveProto::WebChromeZIndexAtLeast => {
            Ok(VerificationPrimitive::WebChromeZIndexAtLeast)
        }
        proto::VerificationPrimitiveProto::AiosfsPathOwnerResolved => {
            Ok(VerificationPrimitive::AiosfsPathOwnerResolved)
        }
        proto::VerificationPrimitiveProto::AiosfsPathRecoveryTreatmentSet => {
            Ok(VerificationPrimitive::AiosfsPathRecoveryTreatmentSet)
        }
        proto::VerificationPrimitiveProto::NamespaceCatalogVersion => {
            Ok(VerificationPrimitive::NamespaceCatalogVersion)
        }
        proto::VerificationPrimitiveProto::StatusIndicatorVisible => {
            Ok(VerificationPrimitive::StatusIndicatorVisible)
        }
        proto::VerificationPrimitiveProto::SubjectSessionFlagState => {
            Ok(VerificationPrimitive::SubjectSessionFlagState)
        }
        proto::VerificationPrimitiveProto::FilesystemRootIntact => {
            Ok(VerificationPrimitive::FilesystemRootIntact)
        }
        proto::VerificationPrimitiveProto::SpecConsumesTable => {
            Ok(VerificationPrimitive::SpecConsumesTable)
        }
        proto::VerificationPrimitiveProto::ApprovalBindingState => {
            Ok(VerificationPrimitive::ApprovalBindingState)
        }
        proto::VerificationPrimitiveProto::SecretPatternMatch => {
            Ok(VerificationPrimitive::SecretPatternMatch)
        }
        proto::VerificationPrimitiveProto::VerificationPrimitiveUnspecified => Err(
            Status::invalid_argument("verification primitive is unspecified"),
        ),
    }
}

// ---------------------------------------------------------------------------
// Struct conversions
// ---------------------------------------------------------------------------

pub fn verification_intent_to_proto(intent: &VerificationIntent) -> proto::VerificationIntentProto {
    proto::VerificationIntentProto {
        intent_id: intent.intent_id.to_string(),
        action_id_proto: action_id_to_proto(&intent.action_id),
        expression: intent.expression.clone(),
        expression_hash: intent.expression_hash.clone(),
        created_at: Some(datetime_to_proto(intent.created_at)),
        timeout_seconds: intent.timeout_seconds,
    }
}

pub fn verification_intent_from_proto(
    intent: proto::VerificationIntentProto,
) -> Result<VerificationIntent, Status> {
    let intent_id = intent_id_from_string(intent.intent_id)?;
    let action_id = action_id_from_proto(&intent.action_id_proto)?;
    if intent.expression.trim().is_empty() {
        return Err(Status::invalid_argument("expression is required"));
    }
    let expected_hash = blake3::hash(intent.expression.as_bytes())
        .to_hex()
        .to_string();
    let expression_hash = if intent.expression_hash.is_empty() {
        expected_hash.clone()
    } else {
        intent.expression_hash
    };
    if expression_hash != expected_hash {
        return Err(Status::invalid_argument(
            "expression_hash does not match expression",
        ));
    }

    Ok(VerificationIntent {
        intent_id,
        action_id,
        expression: intent.expression,
        expression_hash,
        created_at: intent.created_at.map_or_else(Utc::now, datetime_from_proto),
        timeout_seconds: intent.timeout_seconds,
    })
}

pub fn primitive_result_to_proto(result: &PrimitiveResult) -> proto::PrimitiveResultProto {
    proto::PrimitiveResultProto {
        primitive_kind: i32::from(verification_primitive_to_proto(result.primitive_kind)),
        passed: result.passed,
        actual: Some(json_to_prost_value(&result.actual)),
        expected: Some(json_to_prost_value(&result.expected)),
        elapsed_ms: result.elapsed_ms,
        error: result.error.clone(),
    }
}

pub fn primitive_result_from_proto(
    result: proto::PrimitiveResultProto,
) -> Result<PrimitiveResult, Status> {
    let primitive_kind = proto::VerificationPrimitiveProto::try_from(result.primitive_kind)
        .map_err(|_| {
            Status::invalid_argument(format!("unknown primitive kind {}", result.primitive_kind))
        })
        .and_then(verification_primitive_from_proto)?;

    Ok(PrimitiveResult {
        primitive_kind,
        passed: result.passed,
        actual: optional_prost_value_to_json(result.actual.as_ref()),
        expected: optional_prost_value_to_json(result.expected.as_ref()),
        elapsed_ms: result.elapsed_ms,
        error: result.error,
    })
}

pub fn verification_result_to_proto(result: &VerificationResult) -> proto::VerificationResultProto {
    proto::VerificationResultProto {
        result_id: result.result_id.clone(),
        intent_id: result.intent_id.to_string(),
        action_id_proto: action_id_to_proto(&result.action_id),
        status: i32::from(verification_status_to_proto(result.status)),
        per_primitive: result
            .per_primitive
            .iter()
            .map(primitive_result_to_proto)
            .collect(),
        started_at: Some(datetime_to_proto(result.started_at)),
        completed_at: Some(datetime_to_proto(result.completed_at)),
        duration_ms: result.duration_ms,
        evidence_receipt_id: result.evidence_receipt_id.clone(),
    }
}

pub fn verification_result_from_proto(
    result: proto::VerificationResultProto,
) -> Result<VerificationResult, Status> {
    if result.result_id.is_empty() {
        return Err(Status::invalid_argument("result_id is required"));
    }
    let status = proto::VerificationStatusProto::try_from(result.status)
        .map_err(|_| Status::invalid_argument(format!("unknown status {}", result.status)))
        .and_then(verification_status_from_proto)?;

    Ok(VerificationResult {
        result_id: result.result_id,
        intent_id: intent_id_from_string(result.intent_id)?,
        action_id: action_id_from_proto(&result.action_id_proto)?,
        status,
        per_primitive: result
            .per_primitive
            .into_iter()
            .map(primitive_result_from_proto)
            .collect::<Result<Vec<_>, _>>()?,
        started_at: required_datetime_from_proto(result.started_at, "started_at")?,
        completed_at: required_datetime_from_proto(result.completed_at, "completed_at")?,
        duration_ms: result.duration_ms,
        evidence_receipt_id: result.evidence_receipt_id,
    })
}
