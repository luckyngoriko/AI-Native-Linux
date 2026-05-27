//! T-063 section-22 MVP CLI walk.

#![allow(
    clippy::items_after_statements,
    clippy::too_many_lines,
    reason = "the test is intentionally organized as the section-22 phase walk"
)]

use std::error::Error;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use aios_action::{ActionEnvelope, ActionId, Identity, Request, Trace};
use aios_evidence::{EvidenceReceipt, ReceiptBuilder, RecordType, RetentionClass};
use aios_fs::{ChunkId, ChunkRef, ObjectWriteRequest, SubjectRef as FsSubjectRef};
use aios_policy::{
    ApprovalRequirement, Constraints, Decision, PolicyContext, PolicyDecision, PolicyError,
    PolicyKernel,
};
use aios_renderer_cli::{
    AiosCli, AiosClient, EvidenceChainView, InProcessBackend, OutputFormat, RenderContext,
    Renderable,
};
use chrono::Utc;
use clap::Parser;
use serde_json::{json, Value};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(1);

const BOOT_STUB_NOTE: &str = "section-22 phase 1 boot is owned by L1/M9 and is not a CLI action";
const MOUNT_STUB_NOTE: &str =
    "section-22 phase 2 /aios path mounting is stubbed at L1; CLI reads the seeded system object";
const EVIDENCE_STUB_NOTE: &str =
    "InProcessBackend starts policy/runtime/fs/vault/verification/recovery/sgr; evidence gRPC is represented by a renderable stub chain";

#[derive(Debug)]
struct ScriptedPolicyKernel {
    decision: Decision,
    reason_code: &'static str,
}

impl ScriptedPolicyKernel {
    const fn allow() -> Self {
        Self {
            decision: Decision::Allow,
            reason_code: "MvpAllow",
        }
    }
}

impl PolicyKernel for ScriptedPolicyKernel {
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
                reason_message: "scripted T-063 MVP policy decision".to_owned(),
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
async fn section_22_mvp_walk_runs_through_aios_cli_text_and_json() -> TestResult {
    assert!(BOOT_STUB_NOTE.contains("M9"));
    assert!(MOUNT_STUB_NOTE.contains("stubbed"));
    assert!(EVIDENCE_STUB_NOTE.contains("evidence gRPC"));

    let policy: Arc<dyn PolicyKernel> = Arc::new(ScriptedPolicyKernel::allow());
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect_with_policy(policy).await?;
    assert_eq!(shutdown.service_count(), 10);

    let system_object = client
        .write_object(write_request(
            "aios-system-namespace",
            "SYSTEM",
            None,
            "family:alice",
        ))
        .await?;
    let mount_text = run_cli(
        &mut client,
        &[
            "aios",
            "--no-color",
            "fs",
            "read",
            system_object.object_id.as_ref(),
        ],
    )
    .await?;
    assert_text_section(&mount_text, "Object");
    assert!(mount_text.contains("aios-system-namespace"));

    let mount_json = run_cli(
        &mut client,
        &[
            "aios",
            "-o",
            "json",
            "fs",
            "read",
            system_object.object_id.as_ref(),
        ],
    )
    .await?;
    let mount_value: Value = serde_json::from_str(&mount_json)?;
    assert_eq!(
        json_str(&mount_value, "object_id")?,
        system_object.object_id.as_ref()
    );

    let envelope = mvp_write_envelope();
    let envelope_file = write_envelope_file(&envelope)?;
    let submit_text = run_cli(
        &mut client,
        &[
            "aios",
            "--no-color",
            "action",
            "submit",
            path_str(&envelope_file)?,
        ],
    )
    .await?;
    assert_text_section(&submit_text, "Action");
    assert!(submit_text.contains("status:"));

    let submit_json = run_cli(
        &mut client,
        &[
            "aios",
            "-o",
            "json",
            "action",
            "submit",
            path_str(&envelope_file)?,
        ],
    )
    .await?;
    let action_value: Value = serde_json::from_str(&submit_json)?;
    let action_id = json_str(&action_value, "action_id")?.to_owned();
    assert_eq!(json_str(&action_value, "status")?, "SUCCEEDED");

    let status_text = run_cli(
        &mut client,
        &["aios", "--no-color", "action", "status", &action_id],
    )
    .await?;
    assert_text_section(&status_text, "Action");
    assert!(status_text.contains("Succeeded"), "{status_text}");

    let status_json = run_cli(
        &mut client,
        &["aios", "-o", "json", "action", "status", &action_id],
    )
    .await?;
    let status_value: Value = serde_json::from_str(&status_json)?;
    assert_eq!(json_str(&status_value, "action_id")?, action_id);

    let object_write = client
        .write_object(write_request(
            "section-22-user-journal",
            "USER",
            Some(ActionId::parse(&action_id)?),
            "family:alice",
        ))
        .await?;
    let object_text = run_cli(
        &mut client,
        &[
            "aios",
            "--no-color",
            "fs",
            "read",
            object_write.object_id.as_ref(),
        ],
    )
    .await?;
    assert_text_section(&object_text, "Object");
    assert!(object_text.contains("section-22-user-journal"));

    let object_json = run_cli(
        &mut client,
        &[
            "aios",
            "-o",
            "json",
            "fs",
            "read",
            object_write.object_id.as_ref(),
        ],
    )
    .await?;
    let object_value: Value = serde_json::from_str(&object_json)?;
    assert_eq!(
        json_str(&object_value, "object_id")?,
        object_write.object_id.as_ref()
    );

    let versions_text = run_cli(
        &mut client,
        &[
            "aios",
            "--no-color",
            "fs",
            "list-versions",
            object_write.object_id.as_ref(),
        ],
    )
    .await?;
    assert_text_section(&versions_text, "FsVersions");
    assert!(versions_text.contains(object_write.version_id.as_ref()));

    let versions_json = run_cli(
        &mut client,
        &[
            "aios",
            "-o",
            "json",
            "fs",
            "list-versions",
            object_write.object_id.as_ref(),
        ],
    )
    .await?;
    let versions_value: Value = serde_json::from_str(&versions_json)?;
    assert!(versions_value.as_array().is_some_and(|versions| {
        versions.iter().any(|version| {
            version
                .get("version_id")
                .and_then(Value::as_str)
                .is_some_and(|id| id == object_write.version_id.as_ref())
        })
    }));

    let list_text = run_cli(&mut client, &["aios", "--no-color", "fs", "list", "user"]).await?;
    assert_text_section(&list_text, "FsList");
    assert!(list_text.contains(object_write.object_id.as_ref()));

    let list_json = run_cli(&mut client, &["aios", "-o", "json", "fs", "list", "user"]).await?;
    let list_value: Value = serde_json::from_str(&list_json)?;
    assert!(list_value
        .get("matched")
        .and_then(Value::as_array)
        .is_some_and(|matched| matched.iter().any(|entry| {
            entry
                .get("object_id")
                .and_then(Value::as_str)
                .is_some_and(|id| id == object_write.object_id.as_ref())
        })));

    let policy_text = run_cli(
        &mut client,
        &[
            "aios",
            "--no-color",
            "policy",
            "evaluate",
            path_str(&envelope_file)?,
        ],
    )
    .await?;
    assert_text_section(&policy_text, "PolicyDecision");
    assert!(policy_text.contains("Allow"));

    let policy_json = run_cli(
        &mut client,
        &[
            "aios",
            "-o",
            "json",
            "policy",
            "evaluate",
            path_str(&envelope_file)?,
        ],
    )
    .await?;
    let policy_value: Value = serde_json::from_str(&policy_json)?;
    assert_eq!(json_str(&policy_value, "decision")?, "ALLOW");

    let issued_json = run_cli(
        &mut client,
        &[
            "aios",
            "-o",
            "json",
            "vault",
            "issue",
            "key-sign",
            "family:alice",
        ],
    )
    .await?;
    let issued_value: Value = serde_json::from_str(&issued_json)?;
    assert_eq!(json_str(&issued_value, "issued_to")?, "family:alice");

    let vault_text = run_cli(
        &mut client,
        &[
            "aios",
            "--no-color",
            "vault",
            "list-capabilities",
            "family:alice",
        ],
    )
    .await?;
    assert_text_section(&vault_text, "VaultCapabilities");
    assert!(vault_text.contains("family:alice"));
    assert!(vault_text.contains("<vault-handle>"));

    let vault_json = run_cli(
        &mut client,
        &[
            "aios",
            "-o",
            "json",
            "vault",
            "list-capabilities",
            "family:alice",
        ],
    )
    .await?;
    let vault_value: Value = serde_json::from_str(&vault_json)?;
    assert!(vault_value
        .get("capabilities")
        .and_then(Value::as_array)
        .is_some_and(|capabilities| !capabilities.is_empty()));

    let evidence_text = run_cli(
        &mut client,
        &["aios", "--no-color", "evidence", "chain", &action_id],
    )
    .await?;
    assert_text_section(&evidence_text, "EvidenceChain");

    let evidence_json = run_cli(
        &mut client,
        &["aios", "-o", "json", "evidence", "chain", &action_id],
    )
    .await?;
    let evidence_value: Value = serde_json::from_str(&evidence_json)?;
    assert!(evidence_value.get("receipts").is_some());

    let get_cli =
        AiosCli::try_parse_from(["aios", "evidence", "get", "evr_01HXY8K2JPQ7N3M4R5S6T7V8W9"])?;
    let get_error = match get_cli.execute(&mut client).await {
        Ok(output) => {
            return Err(test_error(format!(
                "evidence get unexpectedly succeeded: {output}"
            )))
        }
        Err(err) => err,
    };
    assert!(get_error
        .to_string()
        .contains("evidence endpoint is not configured"));

    let stub_chain = expected_record_type_stub_chain(&action_id)?;
    let mut ctx = RenderContext::new_pipe_defaults();
    ctx.width = Some(100);
    let stub_text = stub_chain.render(OutputFormat::Text, &ctx)?;
    assert_text_section(&stub_text, "EvidenceChain");
    for record_type in expected_record_types() {
        assert!(stub_text.contains(record_type), "{record_type} missing");
    }
    let stub_json = stub_chain.render(OutputFormat::Json, &ctx)?;
    let stub_value: Value = serde_json::from_str(&stub_json)?;
    assert_eq!(
        stub_value
            .get("receipts")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        expected_record_types().len()
    );

    if let Err(_err) = std::fs::remove_file(&envelope_file) {}
    shutdown.shutdown().await?;
    Ok(())
}

async fn run_cli(client: &mut AiosClient, args: &[&str]) -> TestResult<String> {
    let cli = AiosCli::try_parse_from(args.iter().copied())?;
    Ok(cli.execute(client).await?)
}

fn mvp_write_envelope() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("family:alice", false),
        Request::new(
            "aios.fs.write",
            json!({
                "path": "/aios/groups/family/users/alice/journal.txt",
                "content": "section-22 MVP runnable proof",
                "capability_subject": "family:alice"
            }),
        ),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn write_request(
    name: &str,
    scope_kind: &str,
    action_id: Option<ActionId>,
    subject: &str,
) -> ObjectWriteRequest {
    ObjectWriteRequest {
        object_id: None,
        parent_version_ids: Vec::new(),
        chunks: vec![ChunkRef(ChunkId::from_hash_bytes(name.as_bytes()))],
        metadata_delta: json!({
            "name": name,
            "kind": "FILE",
            "mime": "text/plain",
            "scope": {
                "kind": scope_kind,
                "group_id": "family",
                "user_id": "alice"
            }
        }),
        action_id,
        subject: FsSubjectRef(subject.to_owned()),
    }
}

fn write_envelope_file(envelope: &ActionEnvelope) -> TestResult<PathBuf> {
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "aios-mvp-full-golden-path-{}-{id}.json",
        std::process::id()
    ));
    std::fs::write(&path, serde_json::to_vec(envelope)?)?;
    Ok(path)
}

fn expected_record_type_stub_chain(action_id: &str) -> TestResult<EvidenceChainView> {
    let action = ActionId::parse(action_id)?;
    let mut previous: Option<EvidenceReceipt> = None;
    let mut receipts = Vec::new();

    for record_type in [
        RecordType::ActionReceived,
        RecordType::PolicyDecision,
        RecordType::RoutingDecision,
        RecordType::ExecutionStarted,
        RecordType::ExecutionCompleted,
        RecordType::VerificationResult,
    ] {
        let receipt = ReceiptBuilder::new(
            record_type,
            RetentionClass::Standard24M,
            "_system:service:capability-runtime",
        )
        .with_action_id(action.clone())
        .seal(previous.as_ref())?;
        previous = Some(receipt.clone());
        receipts.push(receipt);
    }

    Ok(EvidenceChainView::new(receipts))
}

const fn expected_record_types() -> [&'static str; 6] {
    [
        "ACTION_RECEIVED",
        "POLICY_DECISION",
        "ROUTING_DECISION",
        "EXECUTION_STARTED",
        "EXECUTION_COMPLETED",
        "VERIFICATION_RESULT",
    ]
}

fn assert_text_section(output: &str, section: &str) {
    assert!(output.contains(section), "missing {section} in {output}");
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
