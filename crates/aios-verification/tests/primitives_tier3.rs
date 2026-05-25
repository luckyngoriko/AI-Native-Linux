//! Tier-3 deferred primitive coverage for T-066.

use std::error::Error;

use aios_action::ActionId;
use aios_verification::primitives::tier3;
use aios_verification::{
    InMemoryVerificationEngine, VerificationContext, VerificationEngine, VerificationIntent,
    VerificationPrimitive, VerificationStatus,
};
use chrono::Utc;
use serde_json::{json, Map, Value};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn expression(primitive: VerificationPrimitive, payload: Value) -> TestResult<String> {
    let mut object = match payload {
        Value::Object(object) => object,
        _ => Map::new(),
    };
    object.insert(
        "primitive".to_owned(),
        Value::String(primitive.as_wire_str().to_owned()),
    );
    Ok(serde_json::to_string(&vec![Value::Object(object)])?)
}

fn intent_with(primitive: VerificationPrimitive, payload: Value) -> TestResult<VerificationIntent> {
    Ok(VerificationIntent::new(
        ActionId::new(),
        expression(primitive, payload)?,
        5,
    ))
}

fn context_for(action_id: ActionId) -> VerificationContext {
    VerificationContext {
        subject: "operator:goriko".to_owned(),
        action_id,
        started_at: Utc::now(),
        timeout_seconds: 5,
        dry_run: true,
    }
}

#[tokio::test]
async fn http_ok_returns_probe_error_deferred_to_m16() -> TestResult {
    let engine = InMemoryVerificationEngine::new();
    let intent = intent_with(
        VerificationPrimitive::HttpOk,
        json!({"url": "http://127.0.0.1/", "expected_status_min": 200, "expected_status_max": 299}),
    )?;
    let context = context_for(intent.action_id.clone());

    let result = engine.run_verification(&intent, &context).await?;

    assert_eq!(result.status, VerificationStatus::ProbeError);
    assert!(!result.per_primitive[0].passed);
    assert!(result.per_primitive[0]
        .error
        .as_deref()
        .is_some_and(|err| err.contains("not yet implemented in M8")));
    Ok(())
}

#[tokio::test]
async fn dns_resolver_backend_returns_probe_error_deferred_to_m16() -> TestResult {
    let engine = InMemoryVerificationEngine::new();
    let intent = intent_with(
        VerificationPrimitive::DnsResolverBackend,
        json!({"host_id": "host_local", "expected_transport": 1}),
    )?;
    let context = context_for(intent.action_id.clone());

    let result = engine.run_verification(&intent, &context).await?;

    assert_eq!(result.status, VerificationStatus::ProbeError);
    assert!(!result.per_primitive[0].passed);
    Ok(())
}

#[test]
fn all_tier3_primitives_are_deferred_with_probe_error() {
    for primitive in tier3::deferred_primitives() {
        let result = tier3::deferred_result(*primitive, &json!({"probe": "test"}));
        assert_eq!(result.primitive_kind, *primitive);
        assert!(!result.passed);
        assert!(result.error.as_deref().is_some_and(|err| {
            err.contains("PROBE_ERROR") && err.contains("not yet implemented in M8")
        }));
    }
}
