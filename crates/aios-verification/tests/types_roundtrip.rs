//! Round-trip coverage for the T-064 S2.4 typed core skeleton.

use std::error::Error;

use aios_action::ActionId;
use aios_verification::{
    IntentId, PrimitiveResult, VerificationError, VerificationIntent, VerificationPrimitive,
    VerificationResult, VerificationStatus,
};
use chrono::{DateTime, Utc};
use serde_json::json;
use strum::EnumCount;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn fixed_time() -> TestResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339("2026-05-25T10:00:00Z")?.with_timezone(&Utc))
}

fn primitive_result() -> PrimitiveResult {
    PrimitiveResult {
        primitive_kind: VerificationPrimitive::FileExists,
        passed: true,
        actual: json!({ "size_bytes": 42 }),
        expected: json!({ "object_or_path": "/aios/system" }),
        elapsed_ms: 12,
        error: None,
    }
}

fn verification_intent() -> VerificationIntent {
    VerificationIntent::new(
        ActionId::new(),
        "all[file.exists('/aios/system'),evidence.exists('evr_sample')]",
        5,
    )
}

fn verification_result() -> TestResult<VerificationResult> {
    let intent = verification_intent();
    Ok(VerificationResult {
        result_id: "vr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        intent_id: intent.intent_id,
        action_id: intent.action_id,
        status: VerificationStatus::Passed,
        per_primitive: vec![primitive_result()],
        started_at: fixed_time()?,
        completed_at: fixed_time()?,
        duration_ms: 12,
        evidence_receipt_id: Some("evr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned()),
    })
}

#[test]
fn verification_status_count_matches_s24() {
    assert_eq!(VerificationStatus::COUNT, 5);
}

#[test]
fn verification_primitive_count_matches_s24() {
    assert_eq!(VerificationPrimitive::COUNT, 36);
}

#[test]
fn verification_status_roundtrips_with_spec_wire_name() -> TestResult {
    let encoded = serde_json::to_string(&VerificationStatus::ProbeError)?;
    assert_eq!(encoded, "\"VERIFICATION_PROBE_ERROR\"");

    let decoded: VerificationStatus = serde_json::from_str(&encoded)?;
    assert_eq!(decoded, VerificationStatus::ProbeError);
    Ok(())
}

#[test]
fn verification_intent_roundtrips_through_json() -> TestResult {
    let intent = verification_intent();
    let encoded = serde_json::to_string(&intent)?;
    let decoded: VerificationIntent = serde_json::from_str(&encoded)?;
    assert_eq!(decoded, intent);
    Ok(())
}

#[test]
fn verification_result_roundtrips_through_json() -> TestResult {
    let result = verification_result()?;
    let encoded = serde_json::to_string(&result)?;
    let decoded: VerificationResult = serde_json::from_str(&encoded)?;
    assert_eq!(decoded, result);
    Ok(())
}

#[test]
fn primitive_result_roundtrips_through_json() -> TestResult {
    let result = primitive_result();
    let encoded = serde_json::to_string(&result)?;
    let decoded: PrimitiveResult = serde_json::from_str(&encoded)?;
    assert_eq!(decoded, result);
    Ok(())
}

#[test]
fn intent_id_new_uses_s24_prefix() {
    let id = IntentId::new();
    assert!(id.as_str().starts_with("vrfi_"));
}

#[test]
fn intent_hash_is_blake3_hex_of_supplied_expression() {
    let intent = verification_intent();
    let expected = blake3::hash(intent.expression.as_bytes())
        .to_hex()
        .to_string();
    assert_eq!(intent.expression_hash, expected);
}

#[test]
fn verification_error_display_strings_are_present() {
    let errors = [
        VerificationError::UnknownPrimitive("bogus".to_owned()),
        VerificationError::InvalidIntent("missing action_id".to_owned()),
        VerificationError::TimeoutExceeded {
            intent_id: IntentId::new(),
            after_ms: 500,
        },
        VerificationError::PrimitiveExecutionFailed {
            primitive: VerificationPrimitive::FileExists,
            reason: "permission denied".to_owned(),
        },
        VerificationError::IntentParseFailed("bad expression".to_owned()),
        VerificationError::Internal("clock drift".to_owned()),
    ];

    for error in errors {
        assert!(!error.to_string().is_empty());
    }
}

#[test]
fn action_id_cross_crate_import_and_use_compiles() {
    let action_id = ActionId::new();
    let intent = VerificationIntent::new(action_id.clone(), "service.active('nginx')", 1);
    assert_eq!(intent.action_id, action_id);
}

#[test]
fn pass_result_with_all_primitives_passed_keeps_pass_status() -> TestResult {
    let result = verification_result()?;
    assert_eq!(result.status, VerificationStatus::Passed);
    assert!(result
        .per_primitive
        .iter()
        .all(|primitive| primitive.passed));
    Ok(())
}
