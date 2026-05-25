//! Tier-2 local-probe primitive coverage for T-066.

use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aios_action::ActionId;
use aios_verification::primitives::tier2::command_exit_code_eq;
use aios_verification::{
    InMemoryVerificationEngine, LocalProbe, MockLocalProbe, StdLocalProbe, VerificationContext,
    VerificationEngine, VerificationIntent, VerificationPrimitive, VerificationStatus,
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

async fn run_with_probe(
    primitive: VerificationPrimitive,
    payload: Value,
    probe: Arc<dyn LocalProbe>,
) -> TestResult<aios_verification::VerificationResult> {
    let engine = InMemoryVerificationEngine::new().with_local_probe(probe);
    let intent = intent_with(primitive, payload)?;
    let context = context_for(intent.action_id.clone());

    Ok(engine.run_verification(&intent, &context).await?)
}

#[tokio::test]
async fn file_exists_passes_with_mock_probe_true() -> TestResult {
    let result = run_with_probe(
        VerificationPrimitive::FileExists,
        json!({"object_or_path": "/tmp/aios-ok"}),
        Arc::new(MockLocalProbe::default().with_file_exists("/tmp/aios-ok", true)),
    )
    .await?;

    assert_eq!(result.status, VerificationStatus::Passed);
    assert!(result.per_primitive[0].passed);
    assert_eq!(result.per_primitive[0].actual, json!({"exists": true}));
    Ok(())
}

#[tokio::test]
async fn file_exists_fails_with_mock_probe_false() -> TestResult {
    let result = run_with_probe(
        VerificationPrimitive::FileExists,
        json!({"object_or_path": "/tmp/aios-missing"}),
        Arc::new(MockLocalProbe::default().with_file_exists("/tmp/aios-missing", false)),
    )
    .await?;

    assert_eq!(result.status, VerificationStatus::Failed);
    assert!(!result.per_primitive[0].passed);
    assert_eq!(result.per_primitive[0].actual, json!({"exists": false}));
    Ok(())
}

#[tokio::test]
async fn service_active_uses_process_running_mock() -> TestResult {
    let result = run_with_probe(
        VerificationPrimitive::ServiceActive,
        json!({"service": "nginx"}),
        Arc::new(MockLocalProbe::default().with_process_running("nginx", true)),
    )
    .await?;

    assert_eq!(result.status, VerificationStatus::Passed);
    assert_eq!(result.per_primitive[0].actual, json!({"running": true}));
    Ok(())
}

#[tokio::test]
async fn port_open_uses_port_listening_mock() -> TestResult {
    let result = run_with_probe(
        VerificationPrimitive::PortOpen,
        json!({"host": "127.0.0.1", "port": 8930, "protocol": "tcp"}),
        Arc::new(MockLocalProbe::default().with_port_listening(8930, true)),
    )
    .await?;

    assert_eq!(result.status, VerificationStatus::Passed);
    assert_eq!(result.per_primitive[0].actual, json!({"listening": true}));
    Ok(())
}

#[tokio::test]
async fn command_exit_code_zero_passes() -> TestResult {
    let probe: Arc<dyn LocalProbe> =
        Arc::new(MockLocalProbe::default().with_command_exit_code("true", Vec::new(), Some(0)));

    let verdict = command_exit_code_eq(
        probe.as_ref(),
        &json!({"cmd": "true", "args": [], "expected_exit_code": 0}),
    )
    .await;

    assert!(verdict.passed);
    assert_eq!(verdict.actual, json!({"exit_code": 0}));
    assert_eq!(verdict.error, None);
    Ok(())
}

#[tokio::test]
async fn command_exit_code_nonzero_fails() -> TestResult {
    let probe: Arc<dyn LocalProbe> =
        Arc::new(MockLocalProbe::default().with_command_exit_code("false", Vec::new(), Some(1)));

    let verdict = command_exit_code_eq(
        probe.as_ref(),
        &json!({"cmd": "false", "args": [], "expected_exit_code": 0}),
    )
    .await;

    assert!(!verdict.passed);
    assert_eq!(verdict.actual, json!({"exit_code": 1}));
    assert_eq!(verdict.error, None);
    Ok(())
}

#[tokio::test]
async fn std_local_probe_file_exists_smoke_test_with_temp_file() -> TestResult {
    let path = temp_path("aios-verification-tier2-smoke");
    std::fs::write(&path, b"aios")?;

    let probe = StdLocalProbe;
    let exists = probe.file_exists(path_to_str(&path)?).await;

    std::fs::remove_file(&path)?;
    assert!(exists);
    Ok(())
}

#[tokio::test]
async fn mock_local_probe_can_be_seeded_from_response_map() -> TestResult {
    let mut responses = HashMap::new();
    responses.insert(
        aios_verification::primitives::MockProbeKey::FileExists("/tmp/map".to_owned()),
        aios_verification::primitives::MockProbeValue::Bool(true),
    );
    let probe = MockLocalProbe { responses };

    assert!(probe.file_exists("/tmp/map").await);
    Ok(())
}

fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{}_{}", label, ulid::Ulid::new()))
}

fn path_to_str(path: &Path) -> TestResult<&str> {
    path.to_str()
        .ok_or_else(|| "temporary path is not valid UTF-8".into())
}
