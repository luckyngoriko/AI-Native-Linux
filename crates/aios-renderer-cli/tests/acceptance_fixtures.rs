//! T-063 S7.6 acceptance fixtures.

#![allow(
    clippy::items_after_statements,
    reason = "fixture tests keep setup next to the asserted CLI command"
)]

use std::error::Error;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use aios_action::{ActionEnvelope, ActionId, Identity, Request, Trace};
use aios_fs::{ChunkId, ChunkRef, ObjectWriteRequest, SubjectRef as FsSubjectRef};
use aios_policy::{
    ApprovalRequirement, Constraints, Decision, PolicyContext, PolicyDecision, PolicyError,
    PolicyKernel,
};
use aios_renderer_cli::{AiosCli, AiosClient, InProcessBackend};
use chrono::Utc;
use clap::Parser;
use serde_json::{json, Value};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
struct FixturePolicyKernel {
    decision: Decision,
    reason_code: &'static str,
}

impl FixturePolicyKernel {
    const fn new(decision: Decision, reason_code: &'static str) -> Self {
        Self {
            decision,
            reason_code,
        }
    }
}

impl PolicyKernel for FixturePolicyKernel {
    fn evaluate_policy<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 self,
        _envelope: &'life1 ActionEnvelope,
        context: &'life2 PolicyContext,
    ) -> Pin<Box<dyn Future<Output = Result<PolicyDecision, PolicyError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: 'async_trait,
    {
        let decision = self.decision;
        let reason_code = self.reason_code.to_owned();
        let bundle_version = context.bundle_version.clone();
        let enrichment_snapshot_id = context.enrichment.snapshot_id.clone();

        Box::pin(async move {
            Ok(PolicyDecision {
                policy_decision_id: "poldec_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
                action_id: ActionId::new(),
                request_hash: "0".repeat(64),
                bundle_version,
                enrichment_snapshot_id,
                decision,
                reason_code,
                reason_message: "scripted S7.6 acceptance fixture decision".to_owned(),
                constraints: Constraints::default(),
                approval: ApprovalRequirement::default(),
                evidence_receipt_id: String::new(),
                evaluated_at: Utc::now(),
                rules_consulted: 1,
                simulated: false,
            })
        })
    }
}

#[tokio::test]
async fn fixture_1_basic_aios_chrome_tree_maps_to_evidence_chain_cli() -> TestResult {
    let (mut client, shutdown) = allow_backend().await?;

    let output = run_cli(
        &mut client,
        &[
            "aios",
            "--no-color",
            "evidence",
            "chain",
            "act_01HXY8K2JPQ7N3M4R5S6T7V8W9",
        ],
    )
    .await?;

    assert!(output.contains("EvidenceChain"));
    assert!(output.contains("receipts"));

    shutdown.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn fixture_2_approval_prompt_request_hash_maps_to_action_submit_cli() -> TestResult {
    let (mut client, shutdown) = allow_backend().await?;
    let envelope = envelope(
        "aios.fs.write",
        "family:alice",
        false,
        json!({
            "request_hash": "DEADBEEFDEADBEEFDEADBEEFDEADBEEF",
            "question": "Allow family-assistant to delete inbox entries?",
            "approved_by": "family:alice"
        }),
    );
    let path = write_envelope_file(&envelope)?;

    let output = run_cli(
        &mut client,
        &["aios", "--no-color", "action", "submit", path_str(&path)?],
    )
    .await?;

    assert!(output.contains("Action"));
    assert!(output.contains("status:"));

    if let Err(_err) = std::fs::remove_file(&path) {}
    shutdown.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn fixture_3_list_bound_to_s2_1_view_maps_to_fs_list_cli() -> TestResult {
    let (mut client, shutdown) = allow_backend().await?;
    let written = client
        .write_object(write_request("fixture-inbox-entry", "USER"))
        .await?;

    let output = run_cli(&mut client, &["aios", "-o", "json", "fs", "list", "user"]).await?;
    let value: Value = serde_json::from_str(&output)?;

    assert!(value
        .get("matched")
        .and_then(Value::as_array)
        .is_some_and(|matched| matched.iter().any(|entry| {
            entry
                .get("object_id")
                .and_then(Value::as_str)
                .is_some_and(|id| id == written.object_id.as_ref())
        })));

    shutdown.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn fixture_4_agent_message_ai_origin_maps_to_action_submit_json() -> TestResult {
    let (mut client, shutdown) = allow_backend().await?;
    let envelope = envelope(
        "aios.proposal.note",
        "family:family-assistant",
        true,
        json!({
            "agent_canonical_id": "family:family-assistant",
            "body": "I propose deleting 12 stale invoices.",
            "reasoning_summary": "Invoices older than 90 days, no references."
        }),
    );
    let path = write_envelope_file(&envelope)?;

    let output = run_cli(
        &mut client,
        &["aios", "-o", "json", "action", "submit", path_str(&path)?],
    )
    .await?;
    let value: Value = serde_json::from_str(&output)?;

    assert_eq!(json_str(&value, "status")?, "SUCCEEDED");

    if let Err(_err) = std::fs::remove_file(&path) {}
    shutdown.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn fixture_5_surface_embed_app_surface_maps_to_fs_read_cli() -> TestResult {
    let (mut client, shutdown) = allow_backend().await?;
    let written = client
        .write_object(write_request("com-example-game-surface", "GROUP"))
        .await?;

    let output = run_cli(
        &mut client,
        &[
            "aios",
            "--no-color",
            "fs",
            "read",
            written.object_id.as_ref(),
        ],
    )
    .await?;

    assert!(output.contains("Object"));
    assert!(output.contains("com-example-game-surface"));

    shutdown.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn fixture_6_recovery_only_subtree_rejected_outside_recovery_policy_cli() -> TestResult {
    let (mut client, shutdown) =
        scripted_backend(Decision::Deny, "RecoveryNodeOutsideRecovery").await?;
    let envelope = envelope(
        "aios.ui.render",
        "_system:remote:operator-247",
        false,
        json!({
            "recovery_only": true,
            "root_kind": "CONTAINER",
            "session_recovery_mode": false
        }),
    );
    let path = write_envelope_file(&envelope)?;

    let output = run_cli(
        &mut client,
        &["aios", "-o", "json", "policy", "evaluate", path_str(&path)?],
    )
    .await?;
    let value: Value = serde_json::from_str(&output)?;

    assert_eq!(json_str(&value, "decision")?, "DENY");
    assert_eq!(
        json_str(&value, "reason_code")?,
        "RecoveryNodeOutsideRecovery"
    );

    if let Err(_err) = std::fs::remove_file(&path) {}
    shutdown.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn fixture_7_ai_subject_security_indicator_authorship_refused_policy_cli() -> TestResult {
    let (mut client, shutdown) =
        scripted_backend(Decision::Deny, "TrustBearingAuthorshipRefused").await?;
    let envelope = envelope(
        "aios.ui.render",
        "family:family-assistant",
        true,
        json!({
            "issuer_kind": "AI_AGENT",
            "attempted_kind": "SECURITY_INDICATOR",
            "subject_canonical_id": "family:alice"
        }),
    );
    let path = write_envelope_file(&envelope)?;

    let output = run_cli(
        &mut client,
        &["aios", "--no-color", "policy", "evaluate", path_str(&path)?],
    )
    .await?;

    assert!(output.contains("PolicyDecision"));
    assert!(output.contains("TrustBearingAuthorshipRefused"));

    if let Err(_err) = std::fs::remove_file(&path) {}
    shutdown.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn fixture_8_schema_tree_too_large_rejected_policy_cli() -> TestResult {
    let (mut client, shutdown) = scripted_backend(Decision::Deny, "SchemaTreeTooLarge").await?;
    let envelope = envelope(
        "aios.ui.render",
        "family:alice",
        false,
        json!({
            "root_kind": "CONTAINER",
            "node_count": 10001
        }),
    );
    let path = write_envelope_file(&envelope)?;

    let output = run_cli(
        &mut client,
        &["aios", "-o", "json", "policy", "evaluate", path_str(&path)?],
    )
    .await?;
    let value: Value = serde_json::from_str(&output)?;

    assert_eq!(json_str(&value, "decision")?, "DENY");
    assert_eq!(json_str(&value, "reason_code")?, "SchemaTreeTooLarge");

    if let Err(_err) = std::fs::remove_file(&path) {}
    shutdown.shutdown().await?;
    Ok(())
}

async fn allow_backend() -> TestResult<(AiosClient, aios_renderer_cli::ShutdownHandle)> {
    scripted_backend(Decision::Allow, "FixtureAllow").await
}

async fn scripted_backend(
    decision: Decision,
    reason_code: &'static str,
) -> TestResult<(AiosClient, aios_renderer_cli::ShutdownHandle)> {
    let kernel: Arc<dyn PolicyKernel> = Arc::new(FixturePolicyKernel::new(decision, reason_code));
    Ok(InProcessBackend::spawn_and_connect_with_policy(kernel).await?)
}

async fn run_cli(client: &mut AiosClient, args: &[&str]) -> TestResult<String> {
    let cli = AiosCli::try_parse_from(args.iter().copied())?;
    Ok(cli.execute(client).await?)
}

fn envelope(action: &str, subject: &str, is_ai: bool, target: Value) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new(subject, is_ai),
        Request::new(action, target),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn write_request(name: &str, scope_kind: &str) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: None,
        parent_version_ids: Vec::new(),
        chunks: vec![ChunkRef(ChunkId::from_hash_bytes(name.as_bytes()))],
        metadata_delta: json!({
            "name": name,
            "kind": "APPLICATION",
            "mime": "application/aios-ui",
            "scope": {
                "kind": scope_kind,
                "group_id": "family",
                "user_id": "alice"
            }
        }),
        action_id: None,
        subject: FsSubjectRef("family:alice".to_owned()),
    }
}

fn write_envelope_file(envelope: &ActionEnvelope) -> TestResult<PathBuf> {
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "aios-s76-acceptance-fixture-{}-{id}.json",
        std::process::id()
    ));
    std::fs::write(&path, serde_json::to_vec(envelope)?)?;
    Ok(path)
}

fn path_str(path: &Path) -> TestResult<&str> {
    path.to_str()
        .ok_or_else(|| test_error(format!("path is not UTF-8: {}", path.display())))
}

fn json_str<'a>(value: &'a Value, key: &str) -> TestResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| test_error(format!("missing string JSON field `{key}`")))
}

fn test_error(message: impl Into<String>) -> Box<dyn Error + Send + Sync> {
    std::io::Error::other(message.into()).into()
}
