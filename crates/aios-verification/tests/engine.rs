//! Integration coverage for the T-065 verification engine harness.

use std::error::Error;
use std::sync::Arc;

use aios_action::ActionId;
use aios_verification::{
    InMemoryVerificationEngine, IntentId, LocalProbe, MockLocalProbe, VerificationContext,
    VerificationEngine, VerificationError, VerificationIntent, VerificationPrimitive,
    VerificationStatus,
};
use chrono::Utc;
use serde_json::{json, Map, Value};
use strum::{EnumCount, IntoEnumIterator};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn expression(primitives: &[VerificationPrimitive]) -> TestResult<String> {
    Ok(serde_json::to_string(primitives)?)
}

fn invocation_expression(primitive: VerificationPrimitive, payload: Value) -> TestResult<String> {
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

fn intent_with(primitives: &[VerificationPrimitive]) -> TestResult<VerificationIntent> {
    Ok(VerificationIntent::new(
        ActionId::new(),
        expression(primitives)?,
        5,
    ))
}

fn intent_with_invocation(
    primitive: VerificationPrimitive,
    payload: Value,
) -> TestResult<VerificationIntent> {
    Ok(VerificationIntent::new(
        ActionId::new(),
        invocation_expression(primitive, payload)?,
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

#[test]
fn in_memory_verification_engine_new_succeeds() {
    let _engine = InMemoryVerificationEngine::new();
}

#[tokio::test]
async fn run_verification_with_simple_intent_passes() -> TestResult {
    let probe: Arc<dyn LocalProbe> =
        Arc::new(MockLocalProbe::default().with_file_exists("/tmp/aios-ok", true));
    let engine = InMemoryVerificationEngine::new().with_local_probe(probe);
    let intent = intent_with_invocation(
        VerificationPrimitive::FileExists,
        json!({"object_or_path": "/tmp/aios-ok"}),
    )?;
    let context = context_for(intent.action_id.clone());

    let result = engine.run_verification(&intent, &context).await?;

    assert_eq!(result.status, VerificationStatus::Passed);
    assert_eq!(result.intent_id, intent.intent_id);
    assert_eq!(result.action_id, intent.action_id);
    assert_eq!(result.per_primitive.len(), 1);
    assert_eq!(
        result.per_primitive[0].primitive_kind,
        VerificationPrimitive::FileExists
    );
    Ok(())
}

#[tokio::test]
async fn run_verification_with_empty_primitive_list_passes() -> TestResult {
    let engine = InMemoryVerificationEngine::new();
    let intent = intent_with(&[])?;
    let context = context_for(intent.action_id.clone());

    let result = engine.run_verification(&intent, &context).await?;

    assert_eq!(result.status, VerificationStatus::Passed);
    assert!(result.per_primitive.is_empty());
    Ok(())
}

#[tokio::test]
async fn run_verification_with_multiple_primitives_lists_all_results() -> TestResult {
    let engine = InMemoryVerificationEngine::new().with_local_probe(Arc::new(
        MockLocalProbe::default().with_file_exists("/tmp/aios-ok", true),
    ));
    let primitives = vec![
        json!({"primitive": "FILE_EXISTS", "object_or_path": "/tmp/aios-ok"}),
        json!({"primitive": "HTTP_OK", "url": "http://127.0.0.1/"}),
        json!({"primitive": "EVIDENCE_EXISTS", "receipt_id": "rcpt_01"}),
    ];
    let intent = VerificationIntent::new(ActionId::new(), serde_json::to_string(&primitives)?, 5);
    let context = context_for(intent.action_id.clone());

    let result = engine.run_verification(&intent, &context).await?;
    let observed: Vec<VerificationPrimitive> = result
        .per_primitive
        .iter()
        .map(|primitive| primitive.primitive_kind)
        .collect();

    assert_eq!(
        observed,
        [
            VerificationPrimitive::FileExists,
            VerificationPrimitive::HttpOk,
            VerificationPrimitive::EvidenceExists,
        ]
    );
    assert_eq!(result.status, VerificationStatus::ProbeError);
    assert!(result.per_primitive[0].passed);
    assert!(result.per_primitive[1].error.is_some());
    assert!(result.per_primitive[2].error.is_some());
    Ok(())
}

#[tokio::test]
async fn invalid_json_expression_returns_intent_parse_failed() {
    let engine = InMemoryVerificationEngine::new();
    let intent = VerificationIntent::new(ActionId::new(), "not-json", 5);
    let context = context_for(intent.action_id.clone());

    let error = engine.run_verification(&intent, &context).await.err();

    assert!(matches!(
        error,
        Some(VerificationError::IntentParseFailed(_))
    ));
}

#[tokio::test]
async fn unknown_primitive_variant_returns_unknown_primitive() {
    let engine = InMemoryVerificationEngine::new();
    let intent = VerificationIntent::new(ActionId::new(), r#"["NO_SUCH_PRIMITIVE"]"#, 5);
    let context = context_for(intent.action_id.clone());

    let result = engine.run_verification(&intent, &context).await;

    assert_eq!(
        result,
        Err(VerificationError::UnknownPrimitive(
            "NO_SUCH_PRIMITIVE".to_owned()
        ))
    );
}

#[tokio::test]
async fn run_verification_populates_timing_fields() -> TestResult {
    let engine = InMemoryVerificationEngine::new().with_local_probe(Arc::new(
        MockLocalProbe::default().with_process_running("nginx", true),
    ));
    let intent = intent_with_invocation(
        VerificationPrimitive::ServiceActive,
        json!({"service": "nginx"}),
    )?;
    let context = context_for(intent.action_id.clone());

    let result = engine.run_verification(&intent, &context).await?;

    assert_eq!(result.started_at, context.started_at);
    assert!(result.completed_at >= result.started_at);
    assert_eq!(
        result.duration_ms,
        u64::try_from((result.completed_at - result.started_at).num_milliseconds()).unwrap_or(0)
    );
    Ok(())
}

#[tokio::test]
async fn get_result_after_run_returns_cached_result() -> TestResult {
    let engine = InMemoryVerificationEngine::new();
    let intent = intent_with(&[])?;
    let context = context_for(intent.action_id.clone());

    let result = engine.run_verification(&intent, &context).await?;
    assert_eq!(engine.get_result(&intent.intent_id).await, Some(result));
    Ok(())
}

#[tokio::test]
async fn get_result_for_unknown_intent_id_returns_none() {
    let engine = InMemoryVerificationEngine::new();

    assert_eq!(engine.get_result(&IntentId::new()).await, None);
}

#[tokio::test]
async fn list_primitives_returns_s24_vocabulary() {
    let engine = InMemoryVerificationEngine::new();

    let primitives = engine.list_primitives().await;
    let expected: Vec<VerificationPrimitive> = VerificationPrimitive::iter().collect();

    assert_eq!(primitives.len(), VerificationPrimitive::COUNT);
    assert_eq!(primitives.len(), 36);
    assert_eq!(primitives, expected);
}

#[tokio::test]
async fn arc_dyn_verification_engine_runs_end_to_end() -> TestResult {
    let engine: Arc<dyn VerificationEngine> =
        Arc::new(InMemoryVerificationEngine::new().with_local_probe(Arc::new(
            MockLocalProbe::default().with_port_listening(9555, false),
        )));
    let intent = intent_with_invocation(
        VerificationPrimitive::PortClosed,
        json!({"host": "127.0.0.1", "port": 9555, "protocol": "tcp"}),
    )?;
    let context = context_for(intent.action_id.clone());

    let result = engine.run_verification(&intent, &context).await?;

    assert_eq!(result.status, VerificationStatus::Passed);
    assert_eq!(result.per_primitive.len(), 1);
    Ok(())
}
