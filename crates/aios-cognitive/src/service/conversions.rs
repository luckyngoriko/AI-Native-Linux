//! Rust↔proto conversions for the gRPC `CognitiveCore` surface (T-101).
//!
//! Also contains `CognitiveError` → `tonic::Status` mapping per S13.1 §19.

#![allow(
    missing_docs,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::result_large_err,
    clippy::too_many_lines,
    reason = "conversion function names are intentionally literal and covered by tests"
)]

use std::collections::BTreeMap;

use chrono::{DateTime, TimeZone, Utc};
use prost_types::{
    value::Kind as ProstValueKind, ListValue, NullValue, Struct, Timestamp, Value as ProstValue,
};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use tonic::{Code, Status};

use crate::error::CognitiveError;
use crate::service::proto;
use crate::service::SCHEMA_VERSION;

// ---------------------------------------------------------------------------
// Timestamp helpers
// ---------------------------------------------------------------------------

pub fn datetime_to_proto(dt: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: dt.timestamp(),
        nanos: i32::try_from(dt.timestamp_subsec_nanos()).unwrap_or(0),
    }
}

pub fn datetime_from_proto(ts: Timestamp) -> DateTime<Utc> {
    Utc.timestamp_opt(ts.seconds, u32::try_from(ts.nanos).unwrap_or(0))
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap_or_default())
}

// ---------------------------------------------------------------------------
// serde_json Value ↔ prost Struct helpers
// ---------------------------------------------------------------------------

#[allow(clippy::cast_precision_loss)]
pub fn json_to_prost_value(value: &JsonValue) -> ProstValue {
    let kind = match value {
        JsonValue::Null => ProstValueKind::NullValue(NullValue::NullValue as i32),
        JsonValue::Bool(b) => ProstValueKind::BoolValue(*b),
        JsonValue::Number(n) => n.as_i64().map_or_else(
            || {
                n.as_f64().map_or_else(
                    || ProstValueKind::StringValue(n.to_string()),
                    ProstValueKind::NumberValue,
                )
            },
            |i| ProstValueKind::NumberValue(i as f64),
        ),
        JsonValue::String(s) => ProstValueKind::StringValue(s.clone()),
        JsonValue::Array(arr) => {
            let values = arr.iter().map(json_to_prost_value).collect();
            ProstValueKind::ListValue(ListValue { values })
        }
        JsonValue::Object(map) => {
            let fields: BTreeMap<String, ProstValue> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_prost_value(v)))
                .collect();
            ProstValueKind::StructValue(Struct { fields })
        }
    };
    ProstValue { kind: Some(kind) }
}

pub fn prost_value_to_json(value: &ProstValue) -> JsonValue {
    match &value.kind {
        Some(ProstValueKind::NullValue(_)) | None => JsonValue::Null,
        Some(ProstValueKind::BoolValue(b)) => JsonValue::Bool(*b),
        Some(ProstValueKind::NumberValue(n)) => {
            JsonValue::Number(JsonNumber::from_f64(*n).unwrap_or_else(|| JsonNumber::from(0)))
        }
        Some(ProstValueKind::StringValue(s)) => JsonValue::String(s.clone()),
        Some(ProstValueKind::ListValue(lv)) => {
            JsonValue::Array(lv.values.iter().map(prost_value_to_json).collect())
        }
        Some(ProstValueKind::StructValue(sv)) => {
            let mut map = JsonMap::new();
            for (k, v) in &sv.fields {
                map.insert(k.clone(), prost_value_to_json(v));
            }
            JsonValue::Object(map)
        }
    }
}

pub fn json_to_prost_struct(value: &JsonValue) -> Struct {
    match json_to_prost_value(value).kind {
        Some(ProstValueKind::StructValue(s)) => s,
        _ => Struct {
            fields: BTreeMap::new(),
        },
    }
}

pub fn prost_struct_to_json(s: &Struct) -> JsonValue {
    let mut map = JsonMap::new();
    for (k, v) in &s.fields {
        map.insert(k.clone(), prost_value_to_json(v));
    }
    JsonValue::Object(map)
}

// ---------------------------------------------------------------------------
// Schema version validation
// ---------------------------------------------------------------------------

pub fn validate_schema_version(schema_version: &str) -> Result<(), Status> {
    if schema_version.is_empty() || schema_version == SCHEMA_VERSION {
        return Ok(());
    }
    Err(Status::failed_precondition(format!(
        "unsupported schema_version `{schema_version}`"
    )))
}

// ---------------------------------------------------------------------------
// CognitiveError → tonic::Status mapping (per S13.1 §19.2)
// ---------------------------------------------------------------------------

pub fn cognitive_error_to_status(err: &CognitiveError) -> Status {
    match err {
        CognitiveError::IntentParseFailed(m) => {
            Status::new(Code::InvalidArgument, format!("intent parse failed: {m}"))
        }
        CognitiveError::NoMatchingCapability(m) => {
            Status::new(Code::NotFound, format!("no matching capability: {m}"))
        }
        CognitiveError::TranslationRefused(m) => Status::new(
            Code::FailedPrecondition,
            format!("translation refused: {m}"),
        ),
        CognitiveError::AmbiguousIntent(m) => {
            Status::new(Code::InvalidArgument, format!("ambiguous intent: {m}"))
        }
        CognitiveError::LatencyPrivacyConflict(m) => Status::new(
            Code::FailedPrecondition,
            format!("latency/privacy conflict: {m}"),
        ),
        CognitiveError::NoRouteAvailable(m) => {
            Status::new(Code::Unavailable, format!("no route available: {m}"))
        }
        CognitiveError::CircuitBreakerOpen(m) => {
            let mut status = Status::new(
                Code::FailedPrecondition,
                format!("circuit breaker open: {m}"),
            );
            // Extract retry_after_ms from the error message if present.
            if let Some(rest) = m.strip_prefix("backend ") {
                if let Some(idx) = rest.find(": circuit open, retry_after_ms=") {
                    let ms_str = &rest[idx + ": circuit open, retry_after_ms=".len()..];
                    if let Ok(ms) = ms_str.parse::<u64>() {
                        status.metadata_mut().insert(
                            "retry_after_ms",
                            tonic::metadata::MetadataValue::try_from(&format!("{ms}"))
                                .unwrap_or_else(|_| {
                                    tonic::metadata::MetadataValue::from_static("0")
                                }),
                        );
                    }
                }
            }
            status
        }
        CognitiveError::ModelResponseInvalid(m) => {
            Status::new(Code::Internal, format!("model response invalid: {m}"))
        }
        CognitiveError::Internal(m) => Status::new(Code::Internal, format!("internal: {m}")),
        CognitiveError::ExternalBackendBlocked { posture } => Status::new(
            Code::PermissionDenied,
            format!("external backend blocked by posture {posture:?}"),
        ),
        CognitiveError::VaultCredentialMissing(model_id) => Status::new(
            Code::PermissionDenied,
            format!("vault credential missing for model {model_id}"),
        ),
        CognitiveError::EvidenceEmitFailed(msg) => {
            Status::new(Code::Internal, format!("evidence emit failed: {msg}"))
        }
        CognitiveError::TranslationFailed(msg) => {
            Status::new(Code::Internal, format!("translation failed: {msg}"))
        }
    }
}

// ---------------------------------------------------------------------------
// CognitiveError → proto CognitiveErrorCode mapping
// ---------------------------------------------------------------------------

pub const fn cognitive_error_to_proto_code(err: &CognitiveError) -> i32 {
    let code = match err {
        CognitiveError::IntentParseFailed(_) | CognitiveError::AmbiguousIntent(_) => {
            proto::CognitiveErrorCode::IntentAmbiguous
        }
        CognitiveError::NoMatchingCapability(_)
        | CognitiveError::ModelResponseInvalid(_)
        | CognitiveError::TranslationRefused(_)
        | CognitiveError::LatencyPrivacyConflict(_) => {
            proto::CognitiveErrorCode::ProposalDraftFailed
        }
        CognitiveError::NoRouteAvailable(_)
        | CognitiveError::CircuitBreakerOpen(_)
        | CognitiveError::Internal(_)
        | CognitiveError::EvidenceEmitFailed(_)
        | CognitiveError::TranslationFailed(_) => proto::CognitiveErrorCode::ModelUnavailable,
        CognitiveError::ExternalBackendBlocked { .. }
        | CognitiveError::VaultCredentialMissing(_) => {
            proto::CognitiveErrorCode::ExternalModelCallRejected
        }
    };
    code as i32
}
