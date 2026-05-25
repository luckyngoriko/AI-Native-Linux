//! T-073 — S2.4 §13 golden fixture acceptance coverage.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::items_after_statements,
    reason = "acceptance fixtures are compact executable spec examples"
)]

use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;

use aios_action::ActionId;
use aios_verification::{
    compile_intent, InMemoryVerificationEngine, LocalProbe, MockLocalProbe, VerificationContext,
    VerificationEngine, VerificationError, VerificationIntent, VerificationPrimitive,
    VerificationStatus,
};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn intent(expression: &str, timeout_seconds: u32) -> VerificationIntent {
    VerificationIntent::new(ActionId::new(), expression, timeout_seconds)
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
    expression: &str,
    probe: Arc<dyn LocalProbe>,
) -> Result<aios_verification::VerificationResult, VerificationError> {
    let engine = InMemoryVerificationEngine::new().with_local_probe(probe);
    let intent = intent(expression, 5);
    let context = context_for(intent.action_id.clone());
    engine.run_verification(&intent, &context).await
}

#[derive(Debug)]
struct EventualServiceProbe {
    attempts: AtomicUsize,
    pass_on_attempt: usize,
}

#[async_trait]
impl LocalProbe for EventualServiceProbe {
    async fn file_exists(&self, _path: &str) -> bool {
        false
    }

    async fn file_blake3(&self, _path: &str) -> Option<String> {
        None
    }

    async fn process_running(&self, name: &str) -> bool {
        assert_eq!(name, "docker");
        self.attempts.fetch_add(1, Ordering::SeqCst) + 1 >= self.pass_on_attempt
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

#[derive(Debug)]
struct SlowServiceProbe;

#[async_trait]
impl LocalProbe for SlowServiceProbe {
    async fn file_exists(&self, _path: &str) -> bool {
        false
    }

    async fn file_blake3(&self, _path: &str) -> Option<String> {
        None
    }

    async fn process_running(&self, _name: &str) -> bool {
        tokio::time::sleep(Duration::from_millis(80)).await;
        true
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
async fn fixture_13_1_service_active_passes() -> TestResult {
    let result = run_with_probe(
        "service_active(service=nginx)",
        Arc::new(MockLocalProbe::default().with_process_running("nginx", true)),
    )
    .await?;

    assert_eq!(result.status, VerificationStatus::Passed);
    assert_eq!(
        result.per_primitive[0].primitive_kind,
        VerificationPrimitive::ServiceActive
    );
    assert_eq!(
        result.per_primitive[0]
            .actual
            .get("running")
            .and_then(Value::as_bool),
        Some(true)
    );
    Ok(())
}

#[tokio::test]
async fn fixture_13_2_http_probe_error_is_distinct_from_failed_verdict() -> TestResult {
    let result = run_with_probe(
        r#"http_ok(url="http://localhost/")"#,
        Arc::new(MockLocalProbe::default()),
    )
    .await?;

    assert_eq!(result.status, VerificationStatus::ProbeError);
    assert_ne!(result.status, VerificationStatus::Failed);
    assert!(result.per_primitive[0]
        .error
        .as_deref()
        .is_some_and(|error| error.contains("deferred to M16")));
    Ok(())
}

#[tokio::test]
async fn fixture_13_3_all_short_circuits_after_first_failed_load_bearing_probe() -> TestResult {
    let result = run_with_probe(
        r#"all[service_active(service=nginx), file_exists(path="/tmp/missing"), evidence_exists(receipt_id="evr_abc")]"#,
        Arc::new(
            MockLocalProbe::default()
                .with_process_running("nginx", true)
                .with_file_exists("/tmp/missing", false),
        ),
    )
    .await?;

    assert_eq!(result.status, VerificationStatus::Failed);
    assert_eq!(result.per_primitive.len(), 3);
    assert_eq!(
        result.per_primitive[2].primitive_kind,
        VerificationPrimitive::EvidenceExists
    );
    assert_eq!(result.per_primitive[2].elapsed_ms, 0);
    assert!(result.per_primitive[2]
        .error
        .as_deref()
        .is_some_and(|error| error.contains("SHORT_CIRCUITED")));
    Ok(())
}

#[tokio::test]
async fn fixture_13_4_eventually_succeeds_within_window() -> TestResult {
    let probe = Arc::new(EventualServiceProbe {
        attempts: AtomicUsize::new(0),
        pass_on_attempt: 3,
    });
    let result = run_with_probe(
        "eventually(service_active(service=docker), max_duration=1s, interval=10ms)",
        probe.clone(),
    )
    .await?;

    assert_eq!(result.status, VerificationStatus::Passed);
    assert_eq!(probe.attempts.load(Ordering::SeqCst), 3);
    assert!(result.per_primitive[0]
        .actual
        .get("retry_count")
        .and_then(Value::as_u64)
        .is_some_and(|count| count >= 2));
    Ok(())
}

#[tokio::test]
async fn fixture_13_5_privacy_skip_status_is_reachable_but_policy_gate_is_later() -> TestResult {
    let result = run_with_probe(
        r#"file_hash(path="/classified/object", expected_hash="00")"#,
        Arc::new(MockLocalProbe::default()),
    )
    .await?;

    assert_eq!(result.status, VerificationStatus::ProbeError);
    assert_ne!(VerificationStatus::Skipped, result.status);
    assert_eq!(
        VerificationStatus::Skipped.as_wire_str(),
        "VERIFICATION_SKIPPED"
    );
    assert!(result.per_primitive[0]
        .error
        .as_deref()
        .is_some_and(|error| error.contains("could not hash")));
    Ok(())
}

#[tokio::test]
async fn fixture_13_6_composition_depth_exceeded_is_rejected_at_submission() {
    let intent = intent(
        r#"all[all[all[all[all[all[all[all[file_exists(path="/tmp/a"), file_exists(path="/tmp/b")], file_exists(path="/tmp/c")], file_exists(path="/tmp/d")], file_exists(path="/tmp/e")], file_exists(path="/tmp/f")], file_exists(path="/tmp/g")], file_exists(path="/tmp/h")], file_exists(path="/tmp/i")]"#,
        5,
    );

    let error = compile_intent(&intent).expect_err("depth 9 rejected");

    assert!(matches!(error, VerificationError::IntentParseFailed(_)));
    assert!(error
        .to_string()
        .contains("composition depth exceeds S2.4 limit of 8"));
}

#[tokio::test]
async fn fixture_13_7_property_append_only_maps_to_current_deferred_evidence_probe() -> TestResult {
    let result = run_with_probe(
        r#"evidence_exists(receipt_id="seg_append_only_fixture")"#,
        Arc::new(MockLocalProbe::default()),
    )
    .await?;

    assert_eq!(result.status, VerificationStatus::ProbeError);
    assert_eq!(
        result.per_primitive[0].primitive_kind,
        VerificationPrimitive::EvidenceExists
    );
    assert!(result.per_primitive[0]
        .error
        .as_deref()
        .is_some_and(|error| error.contains("deferred to M16")));
    Ok(())
}

#[tokio::test]
async fn fixture_13_8_property_tamper_detected_maps_to_current_deferred_chain_probe() -> TestResult
{
    let result = run_with_probe(
        r#"policy_decision(policy_decision_id="poldec_tamper", expected_decision=DENY)"#,
        Arc::new(MockLocalProbe::default()),
    )
    .await?;

    assert_eq!(result.status, VerificationStatus::ProbeError);
    assert_eq!(
        result.per_primitive[0].primitive_kind,
        VerificationPrimitive::PolicyDecision
    );
    assert!(result.per_primitive[0]
        .error
        .as_deref()
        .is_some_and(|error| error.contains("deferred to M16")));
    Ok(())
}

#[tokio::test]
async fn acceptance_timeout_status_is_a_first_class_result() {
    let engine = InMemoryVerificationEngine::new().with_local_probe(Arc::new(SlowServiceProbe));
    let intent = intent("service_active(service=slow)", 0);
    let context = context_for(intent.action_id.clone());

    let result = engine
        .run_verification(&intent, &context)
        .await
        .expect("verification result");

    assert_eq!(result.status, VerificationStatus::Timeout);
    assert_eq!(
        result.per_primitive[0]
            .error
            .as_deref()
            .map(|error| error.contains("TIMEOUT")),
        Some(true)
    );
}
