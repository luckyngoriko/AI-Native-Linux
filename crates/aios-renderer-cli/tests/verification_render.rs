//! Tests for verification renderable implementations and CLI wiring.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use aios_action::ActionId;
use aios_renderer_cli::{
    AiosCli, AiosCommand, InProcessBackend, OutputFormat, RenderContext, Renderable,
    VerificationSubcommand,
};
use aios_verification::{
    PrimitiveResult, VerificationIntent, VerificationPrimitive, VerificationResult,
    VerificationStatus,
};
use chrono::{TimeZone, Utc};
use clap::Parser;
use serde_json::json;
use strum::EnumCount;

static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(1);

fn ctx(color: bool) -> RenderContext {
    RenderContext {
        color,
        width: Some(220),
        redact_secrets: true,
        verbose: false,
        locale: "en_US.UTF-8".to_owned(),
    }
}

fn action_id() -> ActionId {
    ActionId::parse("act_01HXY8K2JPQ7N3M4R5S6T7V8W9").expect("valid action id")
}

fn fixed_intent() -> VerificationIntent {
    VerificationIntent::new(
        action_id(),
        r#"file.exists(object_or_path="/tmp/aios-ok")"#,
        5,
    )
}

fn primitive(passed: bool, error: Option<&str>) -> PrimitiveResult {
    PrimitiveResult {
        primitive_kind: VerificationPrimitive::FileExists,
        passed,
        actual: json!({"exists": passed}),
        expected: json!({"object_or_path": "/tmp/aios-ok"}),
        elapsed_ms: 12,
        error: error.map(str::to_owned),
    }
}

fn verification_result(status: VerificationStatus) -> VerificationResult {
    let intent = fixed_intent();
    let started_at = Utc
        .with_ymd_and_hms(2026, 5, 25, 10, 0, 0)
        .single()
        .expect("timestamp");

    VerificationResult {
        result_id: "vrf_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        intent_id: intent.intent_id,
        action_id: intent.action_id,
        status,
        per_primitive: vec![primitive(
            status == VerificationStatus::Passed,
            (status != VerificationStatus::Passed).then_some("missing file"),
        )],
        started_at,
        completed_at: started_at,
        duration_ms: 12,
        evidence_receipt_id: None,
    }
}

fn temp_json_file(prefix: &str, bytes: &[u8]) -> PathBuf {
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("{prefix}-{}-{id}.json", std::process::id()));
    std::fs::write(&path, bytes).expect("write json fixture");
    path
}

fn existing_marker_file() -> PathBuf {
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "aios-verification-marker-{}-{id}",
        std::process::id()
    ));
    std::fs::write(&path, b"ok").expect("write marker");
    path
}

#[test]
fn verification_result_passed_text_contains_intent_status_and_duration() {
    let result = verification_result(VerificationStatus::Passed);

    let rendered = result
        .render(OutputFormat::Text, &ctx(false))
        .expect("render result text");

    assert!(rendered.contains(result.intent_id.as_str()));
    assert!(rendered.contains("STATUS: VERIFICATION_PASSED"));
    assert!(rendered.contains("duration_ms: 12"));
    assert!(rendered.contains("FILE_EXISTS"));
}

#[test]
fn verification_result_failed_color_respects_context() {
    let result = verification_result(VerificationStatus::Failed);

    let colored = result
        .render(OutputFormat::Text, &ctx(true))
        .expect("render colored failed");
    let plain = result
        .render(OutputFormat::Text, &ctx(false))
        .expect("render plain failed");

    assert!(colored.contains("\u{1b}[31mVERIFICATION_FAILED\u{1b}[0m"));
    assert!(plain.contains("STATUS: VERIFICATION_FAILED"));
    assert!(!plain.contains("\u{1b}["));
}

#[test]
fn verification_result_tree_shows_per_primitive_hierarchy() {
    let result = verification_result(VerificationStatus::Passed);

    let rendered = result
        .render(OutputFormat::Tree, &ctx(false))
        .expect("render result tree");

    assert!(rendered.starts_with("VerificationResult vrf_01HXY8K2JPQ7N3M4R5S6T7V8W9"));
    assert!(rendered.contains("status: VERIFICATION_PASSED"));
    assert!(rendered.contains("per_primitive: count=1"));
    assert!(rendered.contains("FILE_EXISTS passed=true elapsed_ms=12"));
}

#[test]
fn verification_result_json_round_trips() {
    let result = verification_result(VerificationStatus::Passed);

    let rendered = result
        .render(OutputFormat::Json, &ctx(false))
        .expect("render result json");
    let reparsed: VerificationResult = serde_json::from_str(&rendered).expect("parse result json");

    assert_eq!(reparsed, result);
}

#[test]
fn verification_status_variants_render_expected_colors() {
    let cases = [
        (VerificationStatus::Passed, "32", "VERIFICATION_PASSED"),
        (VerificationStatus::Failed, "31", "VERIFICATION_FAILED"),
        (VerificationStatus::Timeout, "33", "VERIFICATION_TIMEOUT"),
        (
            VerificationStatus::ProbeError,
            "35",
            "VERIFICATION_PROBE_ERROR",
        ),
        (VerificationStatus::Skipped, "90", "VERIFICATION_SKIPPED"),
    ];

    for (status, color, label) in cases {
        let rendered = status
            .render(OutputFormat::Text, &ctx(true))
            .expect("render status");
        assert!(
            rendered.contains(&format!("\u{1b}[{color}m{label}\u{1b}[0m")),
            "{rendered}"
        );
    }
}

#[test]
fn primitive_result_passed_renders_green_check_marker() {
    let rendered = primitive(true, None)
        .render(OutputFormat::Text, &ctx(true))
        .expect("render primitive");

    assert!(rendered.contains("\u{1b}[32m✓\u{1b}[0m"));
    assert!(rendered.contains("FILE_EXISTS"));
    assert!(rendered.contains("elapsed_ms: 12"));
}

#[test]
fn primitive_result_failed_renders_red_x_marker_and_error() {
    let rendered = primitive(false, Some("missing file"))
        .render(OutputFormat::Text, &ctx(true))
        .expect("render primitive");

    assert!(rendered.contains("\u{1b}[31mX\u{1b}[0m"));
    assert!(rendered.contains("error: missing file"));
}

#[test]
fn verification_primitive_representative_variants_render_wire_names() {
    for (primitive, expected) in [
        (VerificationPrimitive::ServiceActive, "SERVICE_ACTIVE"),
        (VerificationPrimitive::FileExists, "FILE_EXISTS"),
        (
            VerificationPrimitive::NetworkExternalModelCallBrokeredOnly,
            "NETWORK_EXTERNAL_MODEL_CALL_BROKERED_ONLY",
        ),
    ] {
        for format in [
            OutputFormat::Text,
            OutputFormat::Json,
            OutputFormat::Tree,
            OutputFormat::Table,
        ] {
            let rendered = primitive
                .render(format, &ctx(false))
                .expect("render primitive");
            assert!(rendered.contains(expected), "{rendered}");
        }
    }
}

#[test]
fn verification_intent_display_truncates_expression_hash() {
    let intent = fixed_intent();
    let full_hash = intent.expression_hash.clone();

    let rendered = intent
        .render(OutputFormat::Text, &ctx(false))
        .expect("render intent");

    assert!(rendered.contains(&full_hash[..12]));
    assert!(rendered.contains("..."));
    assert!(!rendered.contains(&full_hash));
}

#[tokio::test]
async fn cli_verify_run_renders_verification_result() {
    let marker = existing_marker_file();
    let expression = format!(
        "file.exists(object_or_path=\"{}\")",
        marker.to_str().expect("utf8 marker path")
    );
    let intent = VerificationIntent::new(action_id(), expression, 5);
    let path = temp_json_file(
        "aios-cli-verification-intent",
        &serde_json::to_vec(&intent).expect("serialize intent"),
    );
    let cli = AiosCli::try_parse_from([
        "aios",
        "--no-color",
        "verify",
        "run",
        path.to_str().expect("utf8 intent path"),
    ])
    .expect("parse verify run");
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let output = cli.execute(&mut client).await.expect("execute verify run");

    assert!(output.contains("VerificationResult"));
    assert!(output.contains("STATUS: VERIFICATION_PASSED"));
    assert!(output.contains("FILE_EXISTS"));

    shutdown.shutdown().await.expect("shutdown");
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(marker);
}

#[tokio::test]
async fn cli_verify_list_primitives_renders_36_entries() {
    let cli = AiosCli::try_parse_from(["aios", "--no-color", "verify", "list-primitives"])
        .expect("parse verify list");
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let output = cli.execute(&mut client).await.expect("execute verify list");

    assert!(output.contains("VerificationPrimitives"));
    assert!(output.contains("primitives: 36"));
    assert!(output.contains("SERVICE_ACTIVE"));
    assert!(output.contains("SECRET_PATTERN_MATCH"));

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn in_process_backend_spawns_recovery_and_sgr_services() {
    let (_client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    assert_eq!(shutdown.service_count(), 9);

    shutdown.shutdown().await.expect("shutdown");
}

#[test]
fn parser_verify_run_accepts_intent_file_and_action_override() {
    let cli = AiosCli::try_parse_from([
        "aios",
        "verify",
        "run",
        "intent.json",
        "--action-id",
        "act_01HXY8K2JPQ7N3M4R5S6T7V8W9",
    ])
    .expect("parse verify run");

    match cli.command {
        AiosCommand::Verify {
            subcommand:
                VerificationSubcommand::Run {
                    intent_file,
                    action_id,
                },
        } => {
            assert_eq!(intent_file, PathBuf::from("intent.json"));
            assert_eq!(action_id.as_deref(), Some("act_01HXY8K2JPQ7N3M4R5S6T7V8W9"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn verification_primitive_count_is_36() {
    assert_eq!(VerificationPrimitive::COUNT, 36);
}
