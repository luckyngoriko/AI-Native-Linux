//! T-069 integration tests for the `aios.verification.v1alpha1.VerificationEngine` gRPC surface.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::items_after_statements,
    clippy::result_large_err,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use aios_action::ActionId;
use aios_verification::service::conversions::{
    primitive_result_from_proto, primitive_result_to_proto, verification_error_to_status,
    verification_intent_from_proto, verification_intent_to_proto, verification_result_from_proto,
    verification_result_to_proto,
};
use aios_verification::service::proto::verification_engine_server::VerificationEngine as _;
use aios_verification::service::proto::{ExplainResultRequest, RunVerificationRequest};
use aios_verification::service::{
    VerificationEngineClient, VerificationEngineGrpcServer, VerificationEngineService,
    SCHEMA_VERSION,
};
use aios_verification::{
    InMemoryVerificationEngine, IntentId, LocalProbe, MockLocalProbe, PrimitiveResult,
    VerificationError, VerificationIntent, VerificationPrimitive, VerificationResult,
    VerificationStatus,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::json;
use strum::EnumCount;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tonic::{Code, Request};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn fixed_time() -> TestResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339("2026-05-25T10:00:00Z")?.with_timezone(&Utc))
}

fn action_id_proto(action_id: &ActionId) -> Vec<u8> {
    action_id.as_str().as_bytes().to_vec()
}

fn make_service(probe: Arc<dyn LocalProbe>) -> VerificationEngineService {
    let engine = Arc::new(InMemoryVerificationEngine::new().with_local_probe(probe));
    VerificationEngineService::new(engine)
}

fn passing_service() -> VerificationEngineService {
    make_service(Arc::new(
        MockLocalProbe::default()
            .with_file_exists("/tmp/aios-ok", true)
            .with_process_running("nginx", true),
    ))
}

fn request_for(intent: &VerificationIntent) -> RunVerificationRequest {
    RunVerificationRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        action_id_proto: action_id_proto(&intent.action_id),
        intent: Some(verification_intent_to_proto(intent)),
        subject: "operator:goriko".to_owned(),
        simulate: true,
    }
}

fn intent(expression: &str, timeout_seconds: u32) -> VerificationIntent {
    VerificationIntent::new(ActionId::new(), expression, timeout_seconds)
}

fn primitive_result() -> PrimitiveResult {
    PrimitiveResult {
        primitive_kind: VerificationPrimitive::FileExists,
        passed: true,
        actual: json!({"exists": true}),
        expected: json!({"object_or_path": "/tmp/aios-ok"}),
        elapsed_ms: 12,
        error: None,
    }
}

fn verification_result() -> TestResult<VerificationResult> {
    let intent = intent(r#"file.exists(object_or_path="/tmp/aios-ok")"#, 5);
    Ok(VerificationResult {
        result_id: "vrf_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
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

async fn pick_port() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind random port");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);
    addr
}

async fn spawn_server(
    svc: VerificationEngineService,
) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let addr = pick_port().await;
    let (tx, rx) = oneshot::channel::<()>();
    let server =
        tonic::transport::Server::builder().add_service(VerificationEngineGrpcServer::new(svc));
    let handle = tokio::spawn(async move {
        server
            .serve_with_shutdown(addr, async move {
                let _ = rx.await;
            })
            .await
            .expect("server task");
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    (addr, tx, handle)
}

#[derive(Debug)]
struct SlowProbe {
    sleep_ms: u64,
}

#[async_trait]
impl LocalProbe for SlowProbe {
    async fn file_exists(&self, _path: &str) -> bool {
        tokio::time::sleep(Duration::from_millis(self.sleep_ms)).await;
        true
    }

    async fn file_blake3(&self, _path: &str) -> Option<String> {
        None
    }

    async fn process_running(&self, _name: &str) -> bool {
        false
    }

    async fn port_listening(&self, _port: u16) -> bool {
        false
    }

    async fn env_var(&self, _name: &str) -> Option<String> {
        None
    }

    async fn command_exit_code(&self, _cmd: &str, _args: &[String]) -> Option<i32> {
        None
    }
}

#[tokio::test]
async fn run_verification_with_simple_primitive_returns_result_proto() -> TestResult {
    let svc = passing_service();
    let intent = intent(r#"file.exists(object_or_path="/tmp/aios-ok")"#, 5);

    let response = svc
        .run_verification(Request::new(request_for(&intent)))
        .await?
        .into_inner();

    assert_eq!(
        response.status,
        i32::from(aios_verification::service::proto::VerificationStatusProto::VerificationPassed)
    );
    assert_eq!(response.intent_id, intent.intent_id.as_str());
    assert_eq!(response.per_primitive.len(), 1);
    Ok(())
}

#[tokio::test]
async fn run_verification_with_all_combinator_passes() -> TestResult {
    let svc = passing_service();
    let intent = intent(
        r#"all[file.exists(object_or_path="/tmp/aios-ok"), service.active(service="nginx")]"#,
        5,
    );

    let response = svc
        .run_verification(Request::new(request_for(&intent)))
        .await?
        .into_inner();

    assert_eq!(
        response.status,
        i32::from(aios_verification::service::proto::VerificationStatusProto::VerificationPassed)
    );
    assert_eq!(response.per_primitive.len(), 2);
    Ok(())
}

#[tokio::test]
async fn run_verification_with_any_combinator_uses_executor_status() -> TestResult {
    let svc = passing_service();
    let intent = intent(
        r#"any[file.exists(object_or_path="/tmp/missing"), service.active(service="nginx")]"#,
        5,
    );

    let response = svc
        .run_verification(Request::new(request_for(&intent)))
        .await?
        .into_inner();

    assert_eq!(
        response.status,
        i32::from(aios_verification::service::proto::VerificationStatusProto::VerificationPassed)
    );
    assert_eq!(response.per_primitive.len(), 2);
    Ok(())
}

#[tokio::test]
async fn run_verification_timeout_maps_to_deadline_exceeded() {
    let svc = make_service(Arc::new(SlowProbe { sleep_ms: 25 }));
    let intent = intent(r#"file.exists(object_or_path="/tmp/slow")"#, 0);

    let err = svc
        .run_verification(Request::new(request_for(&intent)))
        .await
        .expect_err("timeout must map to gRPC deadline exceeded");

    assert_eq!(err.code(), Code::DeadlineExceeded);
}

#[tokio::test]
async fn run_verification_invalid_intent_maps_to_invalid_argument() {
    let svc = passing_service();
    let intent = intent("not-json", 5);

    let err = svc
        .run_verification(Request::new(request_for(&intent)))
        .await
        .expect_err("invalid grammar must map to invalid argument");

    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn get_engine_info_returns_version_and_primitive_capabilities() -> TestResult {
    let svc = passing_service();

    let response = svc.get_engine_info(Request::new(())).await?.into_inner();

    assert_eq!(response.default_schema_version, SCHEMA_VERSION);
    assert!(response
        .supported_schema_versions
        .contains(&SCHEMA_VERSION.to_owned()));
    assert_eq!(
        response.supported_primitives.len(),
        VerificationPrimitive::COUNT
    );
    assert_eq!(response.supported_primitives.len(), 36);
    assert!(response
        .supported_primitives
        .contains(&"FILE_EXISTS".to_owned()));
    assert!(!response.code_version.is_empty());
    Ok(())
}

#[tokio::test]
async fn explain_result_on_completed_verification_returns_result() -> TestResult {
    let svc = passing_service();
    let intent = intent(r#"file.exists(object_or_path="/tmp/aios-ok")"#, 5);
    let run = svc
        .run_verification(Request::new(request_for(&intent)))
        .await?
        .into_inner();

    let explained = svc
        .explain_result(Request::new(ExplainResultRequest {
            verification_id: run.result_id.clone(),
        }))
        .await?
        .into_inner()
        .result
        .expect("explain result");

    assert_eq!(explained.result_id, run.result_id);
    assert_eq!(explained.intent_id, intent.intent_id.as_str());
    Ok(())
}

#[tokio::test]
async fn explain_result_unknown_verification_maps_to_not_found() {
    let svc = passing_service();

    let err = svc
        .explain_result(Request::new(ExplainResultRequest {
            verification_id: IntentId::new().to_string(),
        }))
        .await
        .expect_err("unknown result must be not found");

    assert_eq!(err.code(), Code::NotFound);
}

#[test]
fn verification_intent_proto_roundtrips_to_rust() -> TestResult {
    let intent = intent(r#"file.exists(object_or_path="/tmp/aios-ok")"#, 5);
    let proto = verification_intent_to_proto(&intent);
    let decoded = verification_intent_from_proto(proto)?;

    assert_eq!(decoded, intent);
    Ok(())
}

#[test]
fn primitive_result_proto_roundtrips_to_rust() -> TestResult {
    let result = primitive_result();
    let proto = primitive_result_to_proto(&result);
    let decoded = primitive_result_from_proto(proto)?;

    assert_eq!(decoded, result);
    Ok(())
}

#[test]
fn verification_result_proto_roundtrips_to_rust() -> TestResult {
    let result = verification_result()?;
    let proto = verification_result_to_proto(&result);
    let decoded = verification_result_from_proto(proto)?;

    assert_eq!(decoded, result);
    Ok(())
}

#[tokio::test]
async fn tonic_in_process_channel_smoke_test() -> TestResult {
    let svc = passing_service();
    let (addr, shutdown, handle) = spawn_server(svc).await;
    let mut client = VerificationEngineClient::connect(format!("http://{addr}")).await?;
    let intent = intent(r#"file.exists(object_or_path="/tmp/aios-ok")"#, 5);

    let response = client
        .run_verification(request_for(&intent))
        .await?
        .into_inner();
    drop(client);

    assert_eq!(
        response.status,
        i32::from(aios_verification::service::proto::VerificationStatusProto::VerificationPassed)
    );

    let _ = shutdown.send(());
    let _ = handle.await;
    Ok(())
}

#[test]
fn verification_error_status_mapping_table_matches_t069_contract() {
    let cases = [
        (
            VerificationError::UnknownPrimitive("NO_SUCH".to_owned()),
            Code::InvalidArgument,
        ),
        (
            VerificationError::IntentParseFailed("bad syntax".to_owned()),
            Code::InvalidArgument,
        ),
        (
            VerificationError::InvalidIntent("missing action".to_owned()),
            Code::InvalidArgument,
        ),
        (
            VerificationError::TimeoutExceeded {
                intent_id: IntentId::new(),
                after_ms: 10,
            },
            Code::DeadlineExceeded,
        ),
        (
            VerificationError::PrimitiveExecutionFailed {
                primitive: VerificationPrimitive::FileExists,
                reason: "permission denied".to_owned(),
            },
            Code::FailedPrecondition,
        ),
        (
            VerificationError::Internal("clock moved backwards".to_owned()),
            Code::Internal,
        ),
    ];

    for (error, code) in cases {
        assert_eq!(verification_error_to_status(&error).code(), code);
    }
}
