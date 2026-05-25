//! T-055 — §22 vault-mediated integration across vault, policy, and runtime.

#![allow(
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::panic,
    clippy::too_many_lines,
    clippy::unwrap_used,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;

use aios_action::{ActionEnvelope, ActionId, Identity, Request, Trace};
use aios_capability_runtime::{
    adapter_manifest::AdapterActionDeclaration, canonical_signed_manifest_bytes,
    encode_hex_signature, ActionDispatchKind, ActionLifecycleState, AdapterIOMode, AdapterManifest,
    AdapterStability, CapabilityRuntime, DispatchQueue, EvidenceEmitter, InMemoryAdapterRegistry,
    InMemoryApprovalSink, InMemoryCapabilityRuntime, InMemoryEvidenceSink, RuntimeContext,
};
use aios_evidence::{EvidenceReceipt, RecordType};
use aios_policy::{
    ApprovalRequirement, ApprovalScope, ApproverClass, Constraints, Decision, HardDenyEngine,
    HydratedSubject, InMemoryPolicyKernel, PolicyContext, PolicyDecision, PolicyError,
    PolicyKernel, SubjectHydrator,
};
use aios_vault::{
    CapabilityAuditLog, CapabilityClass, GrantOverrideRequest, HydratedSubjectSnapshot,
    IdentityCatalog, InMemoryOverrideBroker, InMemoryVaultBroker, InMemoryVaultEvidenceLog,
    IssueCapabilityRequest, KeyAlgorithm, OverrideBroker, OverrideClass, Subject, SubjectRef,
    SubjectType as VaultSubjectType, UseCapabilityRequest, UseCapabilityResult, VaultBroker,
    VaultError, VaultEvidenceEmitter, VaultOperation, VaultPolicyHydrator,
    VaultPolicyOverrideBoundary,
};

const EXTERNAL_CALL_ACTION: &str = "external.model.call";
const RAW_SECRET_ACTION: &str = "vault.raw.secret.get";
const OVERRIDE_ACTION: &str = "policy.kernel.restore";
const TRUSTED_KEY_ID: &str = "publisher:key:t055:vault:01";
const SIGNING_KEY_MARKER: &[u8; 32] = b"M6_SIGNING_KEY_MATERIAL_32B!!!!!";
const AES_KEY_MARKER: &[u8; 32] = b"M6_AES_KEY_MATERIAL_32_BYTES!!!!";

#[derive(Debug)]
enum VaultKernelScenario {
    ExternalSign {
        vault: Arc<InMemoryVaultBroker>,
        capability_id: aios_vault::CapabilityId,
        observed_signature: Arc<Mutex<Option<Vec<u8>>>>,
    },
    DenyRawSecret,
    Override {
        boundary: VaultPolicyOverrideBoundary,
    },
}

struct VaultMvpPolicyKernel {
    baseline: InMemoryPolicyKernel,
    hydrator: VaultPolicyHydrator,
    scenario: VaultKernelScenario,
}

impl std::fmt::Debug for VaultMvpPolicyKernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultMvpPolicyKernel")
            .field("scenario", &self.scenario)
            .finish_non_exhaustive()
    }
}

impl VaultMvpPolicyKernel {
    fn new(catalog: Arc<IdentityCatalog>, scenario: VaultKernelScenario) -> Arc<Self> {
        let hydrator = VaultPolicyHydrator::new(Arc::clone(&catalog));
        let baseline_hydrator: Arc<dyn SubjectHydrator + Send + Sync> =
            Arc::new(VaultPolicyHydrator::new(catalog));
        Arc::new(Self {
            baseline: InMemoryPolicyKernel::new_with_full_chain(
                baseline_hydrator,
                HardDenyEngine::new_with_defaults(),
            ),
            hydrator,
            scenario,
        })
    }
}

#[async_trait]
impl PolicyKernel for VaultMvpPolicyKernel {
    async fn evaluate_policy(
        &self,
        envelope: &ActionEnvelope,
        context: &PolicyContext,
    ) -> Result<PolicyDecision, PolicyError> {
        let _baseline_decision = self.baseline.evaluate_policy(envelope, context).await?;
        let subject = self
            .hydrator
            .hydrate(&envelope.identity.subject_canonical_id)
            .await?;

        match &self.scenario {
            VaultKernelScenario::ExternalSign {
                vault,
                capability_id,
                observed_signature,
            } if envelope.request.action == EXTERNAL_CALL_ACTION => {
                let result = vault
                    .use_capability(UseCapabilityRequest {
                        capability_id: capability_id.clone(),
                        operation: VaultOperation::Sign {
                            message: b"external model request canonical bytes".to_vec(),
                        },
                    })
                    .await
                    .map_err(|err| PolicyError::BundleLoad {
                        reason: err.to_string(),
                    })?;
                let UseCapabilityResult::Signed { signature } = result else {
                    return Err(PolicyError::BundleLoad {
                        reason: "vault did not return a signature".to_owned(),
                    });
                };
                *observed_signature
                    .lock()
                    .expect("signature observation mutex") = Some(signature);
                Ok(policy_decision(
                    envelope,
                    context,
                    &subject,
                    Decision::Allow,
                    "VaultBrokeredExternalCallAllow",
                    "§22 external model call signed through vault KEY_SIGN capability",
                ))
            }
            VaultKernelScenario::DenyRawSecret if envelope.request.action == RAW_SECRET_ACTION => {
                Ok(policy_decision(
                    envelope,
                    context,
                    &subject,
                    Decision::Deny,
                    "VaultRawSecretDenied",
                    "AI subject cannot request raw KEY_ENCRYPT material",
                ))
            }
            VaultKernelScenario::Override { boundary }
                if envelope.request.action == OVERRIDE_ACTION =>
            {
                let Some(grant) = boundary
                    .is_overridden(&envelope.request.action, &subject.canonical_subject_id)
                    .await
                else {
                    return Ok(policy_decision(
                        envelope,
                        context,
                        &subject,
                        Decision::Deny,
                        "DefaultDeny",
                        "would deny without active STRONG_SOLO override",
                    ));
                };
                Ok(policy_decision(
                    envelope,
                    context,
                    &subject,
                    Decision::Allow,
                    aios_policy::reason_code::EMERGENCY_OVERRIDE_RELAXED,
                    &format!(
                        "vault override receipt {} relaxed {OVERRIDE_ACTION}",
                        grant.override_id
                    ),
                ))
            }
            _ => Ok(policy_decision(
                envelope,
                context,
                &subject,
                Decision::Deny,
                "DefaultDeny",
                "no matching T-055 fixture policy",
            )),
        }
    }
}

fn policy_decision(
    envelope: &ActionEnvelope,
    context: &PolicyContext,
    subject: &HydratedSubject,
    decision: Decision,
    reason_code: &str,
    reason_message: &str,
) -> PolicyDecision {
    PolicyDecision {
        policy_decision_id: format!("poldec_t055_{}", envelope.request.action.replace('.', "_")),
        action_id: ActionId::new(),
        request_hash: envelope.request.request_hash().unwrap_or_default(),
        bundle_version: context.bundle_version.clone(),
        enrichment_snapshot_id: context.enrichment.snapshot_id.clone(),
        decision,
        reason_code: reason_code.to_owned(),
        reason_message: format!("{reason_message}; subject={}", subject.canonical_subject_id),
        constraints: Constraints::default(),
        approval: ApprovalRequirement {
            required: false,
            approval_scope: ApprovalScope::ExactRequestHash,
            ttl_seconds: 0,
            approver_classes: vec![ApproverClass::Human],
            require_human_co_signer: false,
        },
        evidence_receipt_id: "evr_t055_policy_fixture".to_owned(),
        evaluated_at: Utc::now(),
        rules_consulted: 1,
        simulated: false,
    }
}

fn subject(
    canonical_subject_id: &str,
    subject_type: VaultSubjectType,
    is_ai: bool,
    groups: &[&str],
) -> Subject {
    Subject {
        canonical_subject_id: canonical_subject_id.to_owned(),
        subject_type,
        provisional_name: canonical_subject_id.to_owned(),
        groups: groups.iter().map(|group| (*group).to_owned()).collect(),
        is_ai,
        created_at: Utc::now(),
    }
}

async fn register_subjects(subjects: &[Subject]) -> Arc<IdentityCatalog> {
    let catalog = Arc::new(IdentityCatalog::new());
    for subject in subjects {
        catalog
            .register_subject(subject.clone())
            .await
            .expect("register fixture subject");
    }
    catalog
}

fn policy_subject_from(subject: Subject) -> HydratedSubject {
    let snapshot = HydratedSubjectSnapshot::from(subject);
    snapshot.into()
}

fn runtime_context(subject: HydratedSubject) -> RuntimeContext {
    RuntimeContext::from_subject(subject, "polb_t055_v1", "0.1.0-T055")
}

fn envelope(
    subject_id: &str,
    is_ai: bool,
    action: &str,
    target: serde_json::Value,
) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new(subject_id, is_ai),
        Request::new(action, target),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn runtime_signing_key() -> SigningKey {
    SigningKey::from_bytes(&[55_u8; 32])
}

fn vault_signing_key() -> SigningKey {
    SigningKey::from_bytes(&[56_u8; 32])
}

fn runtime_evidence() -> (Arc<InMemoryEvidenceSink>, Arc<EvidenceEmitter>) {
    let sink = Arc::new(InMemoryEvidenceSink::new(runtime_signing_key()));
    let emitter = Arc::new(EvidenceEmitter::new(sink.clone()));
    (sink, emitter)
}

fn vault_evidence() -> (Arc<InMemoryVaultEvidenceLog>, Arc<VaultEvidenceEmitter>) {
    let log = Arc::new(InMemoryVaultEvidenceLog::new());
    let emitter = Arc::new(VaultEvidenceEmitter::new(
        log.clone(),
        vault_signing_key(),
        SubjectRef("_system:service:vault-broker".to_owned()),
    ));
    (log, emitter)
}

fn unsigned_manifest(actions: &[&str]) -> AdapterManifest {
    let now = Utc::now();
    AdapterManifest {
        adapter_id: "adapter:aios:vault-mvp:1.0.0".into(),
        adapter_version: "1.0.0".into(),
        vendor: "aios".into(),
        name: "vault-mediated-mvp-adapter".into(),
        declared_stability: AdapterStability::Stable,
        io_mode: AdapterIOMode::TypedParametersOnly,
        dispatch_kind: ActionDispatchKind::SubprocessFork,
        declared_actions: actions
            .iter()
            .map(|action| AdapterActionDeclaration {
                action_kind: (*action).to_owned(),
                target_schema: serde_json::json!({"type": "object"}),
                response_schema: serde_json::json!({"type": "object"}),
                rollback_strategy: "NONE".into(),
                timeout_seconds: 30,
                template_string: None,
                template_substitution_variables: vec![],
            })
            .collect(),
        declared_invariants_supported: vec!["INV-002".into(), "INV-018".into()],
        default_adapter_timeout_seconds: 60,
        default_sandbox_profile_id: "vault-mvp-default".into(),
        adapter_signature: String::new(),
        signing_key_id: TRUSTED_KEY_ID.to_string(),
        manifest_created_at: now,
        manifest_expires_at: now + Duration::days(365),
    }
}

fn sign_manifest(manifest: &mut AdapterManifest, signing_key: &SigningKey) {
    let body = canonical_signed_manifest_bytes(manifest).expect("canonical manifest");
    let signature = signing_key.sign(&body);
    manifest.adapter_signature = encode_hex_signature(&signature.to_bytes());
}

async fn registry_for(actions: &[&str]) -> Arc<InMemoryAdapterRegistry> {
    let signing_key = SigningKey::generate(&mut OsRng);
    let mut trusted = HashMap::new();
    trusted.insert(TRUSTED_KEY_ID.to_owned(), signing_key.verifying_key());
    let registry = InMemoryAdapterRegistry::new(trusted);
    let mut manifest = unsigned_manifest(actions);
    sign_manifest(&mut manifest, &signing_key);
    registry
        .register(manifest, Utc::now())
        .await
        .expect("register adapter manifest");
    Arc::new(registry)
}

async fn runtime_with_kernel(
    kernel: Arc<dyn PolicyKernel>,
    actions: &[&str],
) -> (Arc<InMemoryEvidenceSink>, InMemoryCapabilityRuntime) {
    let (sink, emitter) = runtime_evidence();
    let runtime = InMemoryCapabilityRuntime::new()
        .with_policy_kernel(kernel)
        .with_adapter_registry(registry_for(actions).await)
        .with_dispatch_queue(Arc::new(DispatchQueue::new_with_defaults()))
        .with_evidence_emitter(emitter)
        .with_approval_sink(Arc::new(InMemoryApprovalSink::new()));
    (sink, runtime)
}

fn vault_with_audit_and_emitter(
    emitter: Arc<VaultEvidenceEmitter>,
) -> (Arc<CapabilityAuditLog>, Arc<InMemoryVaultBroker>) {
    let audit = Arc::new(CapabilityAuditLog::new());
    let vault = Arc::new(
        InMemoryVaultBroker::new()
            .with_audit_log(Arc::clone(&audit))
            .with_evidence_emitter(emitter),
    );
    (audit, vault)
}

async fn issue_capability(
    vault: &InMemoryVaultBroker,
    class: CapabilityClass,
    subject: &str,
    algorithm: KeyAlgorithm,
    key_material_bytes: Vec<u8>,
) -> aios_vault::VaultCapability {
    vault
        .issue_capability(IssueCapabilityRequest {
            class,
            issued_to: SubjectRef(subject.to_owned()),
            expires_at: Some(Utc::now() + Duration::minutes(10)),
            key_algorithm: algorithm,
            key_material_bytes: Some(key_material_bytes),
        })
        .await
        .expect("issue capability")
}

fn payload_json(receipt: &EvidenceReceipt) -> String {
    serde_json::to_string(receipt.payload()).expect("payload json")
}

#[tokio::test]
async fn vault_brokered_external_call_signs_without_revealing_key_and_completes_action() {
    let (vault_log, vault_emitter) = vault_evidence();
    let (_audit, vault) = vault_with_audit_and_emitter(vault_emitter);
    let agent = subject(
        "family:family-assistant",
        VaultSubjectType::Agent,
        true,
        &["family"],
    );
    let catalog = register_subjects(std::slice::from_ref(&agent)).await;
    let capability = issue_capability(
        &vault,
        CapabilityClass::KeySign,
        "family:family-assistant",
        KeyAlgorithm::Ed25519,
        SIGNING_KEY_MARKER.to_vec(),
    )
    .await;
    let observed_signature = Arc::new(Mutex::new(None));
    let kernel = VaultMvpPolicyKernel::new(
        catalog,
        VaultKernelScenario::ExternalSign {
            vault: Arc::clone(&vault),
            capability_id: capability.capability_id.clone(),
            observed_signature: Arc::clone(&observed_signature),
        },
    );
    let (runtime_sink, runtime) = runtime_with_kernel(kernel, &[EXTERNAL_CALL_ACTION]).await;
    let env = envelope(
        "family:family-assistant",
        true,
        EXTERNAL_CALL_ACTION,
        serde_json::json!({
            "model": "external://fixture-model",
            "capability_id": capability.capability_id.as_str(),
            "requires": "KEY_SIGN"
        }),
    );

    let ctx = runtime
        .submit_action(&env, &runtime_context(policy_subject_from(agent)))
        .await
        .expect("submit action");

    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);
    let signature = observed_signature
        .lock()
        .expect("signature observation mutex")
        .clone()
        .expect("adapter observed signature");
    assert_eq!(signature.len(), 64);
    assert!(!format!("{signature:?}").contains("M6_SIGNING_KEY_MATERIAL"));

    let vault_receipts = vault_log.receipts().await;
    assert!(
        vault_receipts.iter().any(
            |receipt| receipt.record_type() == RecordType::VaultOperation
                && payload_json(receipt).contains("\"operation_kind\":\"Sign\"")
        ),
        "CAPABILITY_USED is represented by VAULT_OPERATION in the current S3.1 Rust enum"
    );
    for receipt in &vault_receipts {
        assert!(!payload_json(receipt).contains("M6_SIGNING_KEY_MATERIAL"));
    }

    let runtime_receipts = runtime_sink.receipts().await;
    assert!(runtime_receipts
        .iter()
        .any(|receipt| receipt.record_type() == RecordType::ExecutionCompleted));
    runtime_sink
        .verify_integrity()
        .await
        .expect("runtime evidence chain verifies");
    vault_log
        .verify_integrity()
        .await
        .expect("vault evidence chain verifies");
}

#[tokio::test]
async fn ai_cannot_read_raw_secret_policy_denies_and_vault_returns_only_encrypted_output() {
    let (_vault_log, vault_emitter) = vault_evidence();
    let (_audit, vault) = vault_with_audit_and_emitter(vault_emitter);
    let agent = subject(
        "family:family-assistant",
        VaultSubjectType::Agent,
        true,
        &["family"],
    );
    let catalog = register_subjects(std::slice::from_ref(&agent)).await;
    let kernel = VaultMvpPolicyKernel::new(catalog, VaultKernelScenario::DenyRawSecret);
    let (_runtime_sink, runtime) = runtime_with_kernel(kernel, &[RAW_SECRET_ACTION]).await;

    let denied = runtime
        .submit_action(
            &envelope(
                "family:family-assistant",
                true,
                RAW_SECRET_ACTION,
                serde_json::json!({"requested_material": "KEY_ENCRYPT"}),
            ),
            &runtime_context(policy_subject_from(agent)),
        )
        .await
        .expect("submit raw-secret action");
    assert_eq!(denied.status, ActionLifecycleState::PolicyDenied);

    let encrypt_capability = issue_capability(
        &vault,
        CapabilityClass::KeyEncrypt,
        "family:family-assistant",
        KeyAlgorithm::Aes256Gcm,
        AES_KEY_MARKER.to_vec(),
    )
    .await;
    let encrypted = vault
        .use_capability(UseCapabilityRequest {
            capability_id: encrypt_capability.capability_id,
            operation: VaultOperation::Encrypt {
                plaintext: b"payload for external service".to_vec(),
                aad: b"m6".to_vec(),
            },
        })
        .await
        .expect("encrypt through vault");
    let UseCapabilityResult::Encrypted {
        ciphertext,
        nonce,
        aad,
    } = encrypted
    else {
        panic!("expected encrypted output");
    };
    assert_ne!(ciphertext, AES_KEY_MARKER.to_vec());
    assert_eq!(nonce.len(), 12);
    assert_eq!(aad, b"m6".to_vec());

    let secret_capability = issue_capability(
        &vault,
        CapabilityClass::SecretGet,
        "family:family-assistant",
        KeyAlgorithm::Aes256Gcm,
        AES_KEY_MARKER.to_vec(),
    )
    .await;
    let operation = VaultOperation::SecretGet {
        co_signer_approval_id: "approval:ai-must-not-reveal".to_owned(),
    };
    let err = vault
        .use_capability(UseCapabilityRequest {
            capability_id: secret_capability.capability_id,
            operation: operation.clone(),
        })
        .await
        .expect_err("SECRET_GET raw reveal must fail closed");
    assert_eq!(err, VaultError::OperationUnsupportedInT049(operation));
}

#[tokio::test]
async fn emergency_override_strong_solo_allows_otherwise_denied_action_with_receipt_chain() {
    let (vault_log, vault_emitter) = vault_evidence();
    let catalog = register_subjects(&[subject(
        "family:alice",
        VaultSubjectType::Human,
        false,
        &["family"],
    )])
    .await;
    let override_broker = Arc::new(
        InMemoryOverrideBroker::new(Arc::clone(&catalog)).with_evidence_emitter(vault_emitter),
    );
    let binding = override_broker
        .grant_override(GrantOverrideRequest {
            class: OverrideClass::StrongSolo,
            granted_by: vec![SubjectRef("family:alice".to_owned())],
            target_action_id: None,
            expires_at: Utc::now() + Duration::minutes(5),
            reason: "restore policy db during recovery drill".to_owned(),
        })
        .await
        .expect("grant override");
    let boundary = VaultPolicyOverrideBoundary::new(Arc::clone(&override_broker));
    let kernel = VaultMvpPolicyKernel::new(catalog, VaultKernelScenario::Override { boundary });
    let (runtime_sink, runtime) = runtime_with_kernel(kernel, &[OVERRIDE_ACTION]).await;
    let alice = subject("family:alice", VaultSubjectType::Human, false, &["family"]);

    let ctx = runtime
        .submit_action(
            &envelope(
                "family:alice",
                false,
                OVERRIDE_ACTION,
                serde_json::json!({"target": "policy-db"}),
            ),
            &runtime_context(policy_subject_from(alice)),
        )
        .await
        .expect("submit override action");
    assert_eq!(ctx.status, ActionLifecycleState::Succeeded);

    override_broker
        .consume_override(&binding.binding_id, &SubjectRef("family:alice".to_owned()))
        .await
        .expect("consume override");

    let runtime_receipts = runtime_sink.receipts().await;
    let policy_receipt = runtime_receipts
        .iter()
        .find(|receipt| receipt.record_type() == RecordType::PolicyDecision)
        .expect("policy decision receipt");
    assert!(
        payload_json(policy_receipt).contains(aios_policy::reason_code::EMERGENCY_OVERRIDE_RELAXED)
    );
    assert!(runtime_receipts
        .iter()
        .any(|receipt| receipt.record_type() == RecordType::ExecutionCompleted));

    let vault_receipts = vault_log.receipts().await;
    assert!(vault_receipts
        .iter()
        .any(|receipt| receipt.record_type() == RecordType::OverrideGranted));
    assert!(vault_receipts
        .iter()
        .any(|receipt| receipt.record_type() == RecordType::OverrideConsumed));
    assert!(vault_receipts
        .iter()
        .any(|receipt| payload_json(receipt).contains(&binding.binding_id)));
    vault_log
        .verify_integrity()
        .await
        .expect("override evidence chain verifies");
}

#[tokio::test]
async fn anti_replay_rejects_second_consume_of_same_override_binding() {
    let (_vault_log, vault_emitter) = vault_evidence();
    let catalog = register_subjects(&[subject(
        "family:alice",
        VaultSubjectType::Human,
        false,
        &["family"],
    )])
    .await;
    let override_broker = InMemoryOverrideBroker::new(catalog).with_evidence_emitter(vault_emitter);
    let binding = override_broker
        .grant_override(GrantOverrideRequest {
            class: OverrideClass::StrongSolo,
            granted_by: vec![SubjectRef("family:alice".to_owned())],
            target_action_id: None,
            expires_at: Utc::now() + Duration::minutes(5),
            reason: "single-use binding".to_owned(),
        })
        .await
        .expect("grant override");

    override_broker
        .consume_override(&binding.binding_id, &SubjectRef("family:alice".to_owned()))
        .await
        .expect("first consume succeeds");
    let err = override_broker
        .consume_override(&binding.binding_id, &SubjectRef("family:alice".to_owned()))
        .await
        .expect_err("second consume must fail");

    assert_eq!(err, VaultError::OverrideAlreadyConsumed);
}
