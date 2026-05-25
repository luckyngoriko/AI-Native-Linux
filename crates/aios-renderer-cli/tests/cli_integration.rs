//! T-062 integration coverage for the `aios` clap command tree.

#![allow(
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::panic,
    clippy::unwrap_used,
    reason = "integration tests use panic-on-failure assertions"
)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_fs::{ChunkId, ChunkRef, ObjectWriteRequest, SubjectRef as FsSubjectRef};
use aios_renderer_cli::{
    ActionSubcommand, AiosCli, AiosCommand, FsSubcommand, InProcessBackend, OutputFormat,
    PolicySubcommand,
};
use clap::{error::ErrorKind, Parser};

static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(1);

fn clean_envelope() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("human:lucky", false),
        Request::new("service.status", serde_json::json!({"service": "nginx"})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn envelope_file() -> PathBuf {
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "aios-cli-envelope-{}-{id}.json",
        std::process::id()
    ));
    std::fs::write(
        &path,
        serde_json::to_vec(&clean_envelope()).expect("serialize envelope"),
    )
    .expect("write envelope fixture");
    path
}

fn chunk_ref(bytes: &[u8]) -> ChunkRef {
    ChunkRef(ChunkId::from_hash_bytes(bytes))
}

fn write_request(name: &str) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: None,
        parent_version_ids: Vec::new(),
        chunks: vec![chunk_ref(name.as_bytes())],
        metadata_delta: serde_json::json!({
            "name": name,
            "mime": "text/plain"
        }),
        action_id: None,
        subject: FsSubjectRef("family:alice".to_owned()),
    }
}

#[test]
fn parser_action_submit_accepts_envelope_file() {
    let cli = AiosCli::try_parse_from(["aios", "action", "submit", "envelope.json"])
        .expect("parse action submit");

    match cli.command {
        AiosCommand::Action {
            subcommand: ActionSubcommand::Submit { envelope_json_file },
        } => assert_eq!(envelope_json_file, PathBuf::from("envelope.json")),
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parser_fs_read_accepts_object_id() {
    let cli = AiosCli::try_parse_from(["aios", "fs", "read", "obj_01HXY8K2JPQ7N3M4R5S6T7V8W9"])
        .expect("parse fs read");

    match cli.command {
        AiosCommand::Fs {
            subcommand: FsSubcommand::Read { object_id },
        } => assert_eq!(object_id, "obj_01HXY8K2JPQ7N3M4R5S6T7V8W9"),
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parser_policy_evaluate_accepts_json_format() {
    let cli =
        AiosCli::try_parse_from(["aios", "-o", "json", "policy", "evaluate", "envelope.json"])
            .expect("parse policy evaluate");

    assert_eq!(cli.output_format().expect("format"), OutputFormat::Json);
    match cli.command {
        AiosCommand::Policy {
            subcommand: PolicySubcommand::Evaluate { envelope_json_file },
        } => assert_eq!(envelope_json_file, PathBuf::from("envelope.json")),
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parser_invalid_subcommand_returns_clap_error() {
    let err = AiosCli::try_parse_from(["aios", "invalid"]).expect_err("invalid subcommand");

    assert_eq!(err.kind(), ErrorKind::InvalidSubcommand);
}

#[tokio::test]
async fn execute_action_submit_renders_action_context() {
    let path = envelope_file();
    let cli = AiosCli::try_parse_from([
        "aios",
        "--no-color",
        "action",
        "submit",
        path.to_str().expect("utf8 path"),
    ])
    .expect("parse action submit");
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let output = cli
        .execute(&mut client)
        .await
        .expect("execute action submit");

    assert!(output.contains("Action"));
    assert!(output.contains("action_id: act_"));
    assert!(output.contains("status:"));

    shutdown.shutdown().await.expect("shutdown");
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn execute_fs_read_renders_object() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");
    let written = client
        .write_object(write_request("cli-read-object"))
        .await
        .expect("write object fixture");
    let cli = AiosCli::try_parse_from([
        "aios",
        "--no-color",
        "fs",
        "read",
        written.object_id.as_ref(),
    ])
    .expect("parse fs read");

    let output = cli.execute(&mut client).await.expect("execute fs read");

    assert!(output.contains("Object"));
    assert!(output.contains(written.object_id.as_ref()));
    assert!(output.contains("cli-read-object"));

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn execute_policy_evaluate_renders_policy_decision() {
    let path = envelope_file();
    let cli = AiosCli::try_parse_from([
        "aios",
        "--no-color",
        "policy",
        "evaluate",
        path.to_str().expect("utf8 path"),
    ])
    .expect("parse policy evaluate");
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let output = cli.execute(&mut client).await.expect("execute policy");

    assert!(output.contains("PolicyDecision"));
    assert!(output.contains("decision:"));

    shutdown.shutdown().await.expect("shutdown");
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn execute_vault_list_capabilities_renders_list() {
    let cli = AiosCli::try_parse_from([
        "aios",
        "--no-color",
        "vault",
        "list-capabilities",
        "family:alice",
    ])
    .expect("parse vault list");
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let output = cli.execute(&mut client).await.expect("execute vault list");

    assert!(output.contains("VaultCapabilities"));
    assert!(output.contains("family:alice"));
    assert!(output.contains("<vault-handle>"));

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn execute_evidence_chain_renders_chain_view() {
    let cli = AiosCli::try_parse_from([
        "aios",
        "--no-color",
        "evidence",
        "chain",
        "act_01HXY8K2JPQ7N3M4R5S6T7V8W9",
    ])
    .expect("parse evidence chain");
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let output = cli
        .execute(&mut client)
        .await
        .expect("execute evidence chain");

    assert!(output.contains("EvidenceChain"));
    assert!(output.contains("receipts"));

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn no_color_flag_disables_ansi_codes() {
    let path = envelope_file();
    let cli = AiosCli::try_parse_from([
        "aios",
        "--no-color",
        "action",
        "submit",
        path.to_str().expect("utf8 path"),
    ])
    .expect("parse action submit");
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let output = cli
        .execute(&mut client)
        .await
        .expect("execute action submit");

    assert!(!output.contains('\u{1b}'));

    shutdown.shutdown().await.expect("shutdown");
    let _ = std::fs::remove_file(path);
}
