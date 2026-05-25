//! T-055 — S5.1 + S5.2 + S5.4 acceptance fixtures for `aios-vault`.

#![allow(
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::panic,
    clippy::too_many_lines,
    clippy::unwrap_used,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::{Duration, Utc};
use ed25519_dalek::SigningKey;
use strum::{EnumCount, IntoEnumIterator as _};

use aios_action::ActionId;
use aios_evidence::{EvidenceReceipt, RecordType};
use aios_vault::{
    CapabilityAuditLog, CapabilityClass, GrantOverrideRequest, HydratedSubjectSnapshot,
    IdentityCatalog, InMemoryOverrideBroker, InMemoryVaultBroker, InMemoryVaultEvidenceLog,
    IssueCapabilityRequest, KeyAlgorithm, OverrideBroker, OverrideClass, Subject, SubjectRef,
    SubjectType, UseCapabilityRequest, UseCapabilityResult, VaultBroker, VaultError,
    VaultEvidenceEmitter, VaultOperation, VaultSubjectHydrator,
};

const SIGNING_KEY_MARKER: &[u8; 32] = b"S52_SIGNING_KEY_MATERIAL_32B!!!!";
const AES_KEY_MARKER: &[u8; 32] = b"S52_AES_KEY_MATERIAL_32_BYTES!!!";
const HMAC_KEY_MARKER: &[u8; 32] = b"S52_HMAC_KEY_MATERIAL_32_BYTE!!!";
const HKDF_IKM_MARKER: &[u8; 32] = b"S52_HKDF_IKM_MATERIAL_32_BYTE!!!";

fn evidence_signing_key() -> SigningKey {
    SigningKey::from_bytes(&[57_u8; 32])
}

fn vault_evidence() -> (Arc<InMemoryVaultEvidenceLog>, Arc<VaultEvidenceEmitter>) {
    let log = Arc::new(InMemoryVaultEvidenceLog::new());
    let emitter = Arc::new(VaultEvidenceEmitter::new(
        log.clone(),
        evidence_signing_key(),
        SubjectRef("_system:service:vault-broker".to_owned()),
    ));
    (log, emitter)
}

fn vault_stack(
    emitter: Arc<VaultEvidenceEmitter>,
) -> (Arc<CapabilityAuditLog>, InMemoryVaultBroker) {
    let audit = Arc::new(CapabilityAuditLog::new());
    let vault = InMemoryVaultBroker::new()
        .with_audit_log(Arc::clone(&audit))
        .with_evidence_emitter(emitter);
    (audit, vault)
}

fn subject(
    canonical_subject_id: &str,
    subject_type: SubjectType,
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

async fn catalog_with(subjects: &[Subject]) -> Arc<IdentityCatalog> {
    let catalog = Arc::new(IdentityCatalog::new());
    for subject in subjects {
        catalog
            .register_subject(subject.clone())
            .await
            .expect("register fixture subject");
    }
    catalog
}

async fn issue_capability(
    vault: &InMemoryVaultBroker,
    class: CapabilityClass,
    issued_to: &str,
    key_algorithm: KeyAlgorithm,
    key_material_bytes: Vec<u8>,
) -> aios_vault::VaultCapability {
    vault
        .issue_capability(IssueCapabilityRequest {
            class,
            issued_to: SubjectRef(issued_to.to_owned()),
            expires_at: Some(Utc::now() + Duration::minutes(10)),
            key_algorithm,
            key_material_bytes: Some(key_material_bytes),
        })
        .await
        .expect("issue capability")
}

fn payload_json(receipt: &EvidenceReceipt) -> String {
    serde_json::to_string(receipt.payload()).expect("payload json")
}

#[test]
fn s51_subject_kind_closed_enum_has_eight_values_and_local_operator_maps_to_recovery() {
    assert_eq!(SubjectType::iter().count(), SubjectType::COUNT);
    assert_eq!(SubjectType::COUNT, 8);

    let snapshot = HydratedSubjectSnapshot::from(subject(
        "_system:local:operator-247",
        SubjectType::LocalOperator,
        false,
        &["_system"],
    ));

    assert_eq!(snapshot.canonical_subject_id, "_system:local:operator-247");
    assert_eq!(snapshot.session_class, "RECOVERY");
    assert!(snapshot.recovery_mode);
    assert!(!snapshot.is_ai);
}

#[tokio::test]
async fn s51_personal_household_authentication_fixture_hydrates_human_and_ai_sessions() {
    let alice = subject("family:alice", SubjectType::Human, false, &["family"]);
    let assistant = subject(
        "family:family-assistant",
        SubjectType::Agent,
        true,
        &["family"],
    );
    let catalog = catalog_with(&[alice, assistant]).await;
    let alice_session = catalog
        .start_session("family:alice", Utc::now() + Duration::hours(1))
        .await
        .expect("start alice session");
    let assistant_session = catalog
        .start_session("family:family-assistant", Utc::now() + Duration::hours(1))
        .await
        .expect("start assistant session");
    let hydrator = VaultSubjectHydrator::new(catalog);

    let alice_snapshot = hydrator
        .hydrate_by_session(&alice_session.session_id)
        .await
        .expect("hydrate alice");
    let assistant_snapshot = hydrator
        .hydrate_by_session(&assistant_session.session_id)
        .await
        .expect("hydrate assistant");

    assert_eq!(alice_snapshot.session_class, "INTERACTIVE");
    assert!(!alice_snapshot.is_ai);
    assert_eq!(assistant_snapshot.session_class, "SERVICE");
    assert!(assistant_snapshot.is_ai);
}

#[tokio::test]
async fn s51_capability_binding_scope_filters_by_subject_reference() {
    let (_log, emitter) = vault_evidence();
    let (_audit, vault) = vault_stack(emitter);
    let capability = issue_capability(
        &vault,
        CapabilityClass::KeySign,
        "family:alice",
        KeyAlgorithm::Ed25519,
        SIGNING_KEY_MARKER.to_vec(),
    )
    .await;

    let alice = vault
        .list_capabilities(&SubjectRef("family:alice".to_owned()))
        .await
        .expect("list alice capabilities");
    let homelab_alice = vault
        .list_capabilities(&SubjectRef("homelab:alice".to_owned()))
        .await
        .expect("list switched-group capabilities");

    assert_eq!(alice.len(), 1);
    assert_eq!(alice[0].capability_id, capability.capability_id);
    assert!(homelab_alice.is_empty());
}

#[tokio::test]
async fn s52_fixture_ai_agent_signs_document_via_key_sign_without_key_leak() {
    let (log, emitter) = vault_evidence();
    let (_audit, vault) = vault_stack(emitter);
    let capability = issue_capability(
        &vault,
        CapabilityClass::KeySign,
        "family:family-assistant",
        KeyAlgorithm::Ed25519,
        SIGNING_KEY_MARKER.to_vec(),
    )
    .await;

    let result = vault
        .use_capability(UseCapabilityRequest {
            capability_id: capability.capability_id,
            operation: VaultOperation::Sign {
                message: b"document bytes".to_vec(),
            },
        })
        .await
        .expect("sign document");
    let UseCapabilityResult::Signed { signature } = result else {
        panic!("expected signed output");
    };

    assert_eq!(signature.len(), 64);
    let receipts = log.receipts().await;
    assert!(receipts
        .iter()
        .any(|receipt| receipt.record_type() == RecordType::VaultOperation));
    for receipt in receipts {
        let json = payload_json(&receipt);
        assert!(!json.contains("S52_SIGNING_KEY_MATERIAL"));
        assert!(!json.contains("document bytes"));
    }
}

#[tokio::test]
async fn s52_fixture_ai_agent_secret_get_rejected_without_raw_bytes() {
    let (log, emitter) = vault_evidence();
    let (_audit, vault) = vault_stack(emitter);
    let capability = issue_capability(
        &vault,
        CapabilityClass::SecretGet,
        "family:family-assistant",
        KeyAlgorithm::Aes256Gcm,
        AES_KEY_MARKER.to_vec(),
    )
    .await;
    let operation = VaultOperation::SecretGet {
        co_signer_approval_id: "appr_fixture".to_owned(),
    };

    let error = vault
        .use_capability(UseCapabilityRequest {
            capability_id: capability.capability_id,
            operation: operation.clone(),
        })
        .await
        .expect_err("SECRET_GET must reject");

    assert_eq!(error, VaultError::OperationUnsupportedInT049(operation));
    for receipt in log.receipts().await {
        assert!(!payload_json(&receipt).contains("S52_AES_KEY_MATERIAL"));
    }
}

#[tokio::test]
async fn s52_fixture_kdf_derive_returns_opaque_handle_not_raw_derived_bytes() {
    let (_log, emitter) = vault_evidence();
    let (_audit, vault) = vault_stack(emitter);
    let capability = issue_capability(
        &vault,
        CapabilityClass::KeyEncrypt,
        "family:alice",
        KeyAlgorithm::HkdfSha256,
        HKDF_IKM_MARKER.to_vec(),
    )
    .await;

    let result = vault
        .use_capability(UseCapabilityRequest {
            capability_id: capability.capability_id,
            operation: VaultOperation::KdfDerive {
                info: b"bundle-signing".to_vec(),
                length: 32,
            },
        })
        .await
        .expect("derive key");
    let UseCapabilityResult::KdfDerived { derived_key_handle } = result else {
        panic!("expected opaque derived-key handle");
    };

    assert_eq!(derived_key_handle.to_string(), "<vault-handle>");
    let json = serde_json::to_string(&derived_key_handle).expect("serialize handle");
    assert!(!json.contains("S52_HKDF_IKM_MATERIAL"));
}

#[tokio::test]
async fn s52_fixture_revoke_invalidates_capability_before_next_use() {
    let (log, emitter) = vault_evidence();
    let (_audit, vault) = vault_stack(emitter);
    let capability = issue_capability(
        &vault,
        CapabilityClass::MacGenerate,
        "family:alice",
        KeyAlgorithm::HmacSha256,
        HMAC_KEY_MARKER.to_vec(),
    )
    .await;

    vault
        .revoke_capability(
            &capability.capability_id,
            &SubjectRef("family:alice".to_owned()),
        )
        .await
        .expect("revoke capability");
    let err = vault
        .use_capability(UseCapabilityRequest {
            capability_id: capability.capability_id.clone(),
            operation: VaultOperation::MacGenerate {
                message: b"payload".to_vec(),
            },
        })
        .await
        .expect_err("revoked capability must fail");

    assert_eq!(err, VaultError::CapabilityRevoked(capability.capability_id));
    assert!(log
        .receipts()
        .await
        .iter()
        .any(|receipt| receipt.record_type() == RecordType::VaultCapabilityRevoked));
}

#[tokio::test]
async fn s54_fixture_strong_solo_override_grants_consumes_and_replay_fails() {
    let (_log, emitter) = vault_evidence();
    let alice = subject("family:alice", SubjectType::Human, false, &["family"]);
    let catalog = catalog_with(&[alice]).await;
    let broker = InMemoryOverrideBroker::new(catalog).with_evidence_emitter(emitter);
    let binding = broker
        .grant_override(GrantOverrideRequest {
            class: OverrideClass::StrongSolo,
            granted_by: vec![SubjectRef("family:alice".to_owned())],
            target_action_id: Some(ActionId::new()),
            expires_at: Utc::now() + Duration::minutes(5),
            reason: "recovery operator unbricks policy db".to_owned(),
        })
        .await
        .expect("grant strong solo");

    let consumed = broker
        .consume_override(&binding.binding_id, &SubjectRef("family:alice".to_owned()))
        .await
        .expect("consume override");
    let replay = broker
        .consume_override(&binding.binding_id, &SubjectRef("family:alice".to_owned()))
        .await
        .expect_err("replay must fail");

    assert_eq!(consumed.binding_id, binding.binding_id);
    assert_eq!(replay, VaultError::OverrideAlreadyConsumed);
}

#[tokio::test]
async fn s54_fixture_dual_human_requires_exactly_two_human_approvers() {
    let (_log, emitter) = vault_evidence();
    let catalog = catalog_with(&[
        subject("family:alice", SubjectType::Human, false, &["family"]),
        subject("family:bob", SubjectType::Human, false, &["family"]),
    ])
    .await;
    let broker = InMemoryOverrideBroker::new(catalog).with_evidence_emitter(emitter);

    let binding = broker
        .grant_override(GrantOverrideRequest {
            class: OverrideClass::DualHuman,
            granted_by: vec![
                SubjectRef("family:alice".to_owned()),
                SubjectRef("family:bob".to_owned()),
            ],
            target_action_id: Some(ActionId::new()),
            expires_at: Utc::now() + Duration::minutes(5),
            reason: "two admins delete old encrypted backup".to_owned(),
        })
        .await
        .expect("grant dual human");
    let too_few = broker
        .grant_override(GrantOverrideRequest {
            class: OverrideClass::DualHuman,
            granted_by: vec![SubjectRef("family:alice".to_owned())],
            target_action_id: Some(ActionId::new()),
            expires_at: Utc::now() + Duration::minutes(5),
            reason: "insufficient quorum".to_owned(),
        })
        .await
        .expect_err("dual human requires two approvers");

    assert_eq!(binding.class, OverrideClass::DualHuman);
    assert!(matches!(
        too_few,
        VaultError::OverrideClassApproverCountMismatch { .. }
    ));
}

#[tokio::test]
async fn s54_fixture_triple_human_requires_three_humans_and_rejects_service_participant() {
    let (_log, emitter) = vault_evidence();
    let catalog = catalog_with(&[
        subject("family:alice", SubjectType::Human, false, &["family"]),
        subject("family:bob", SubjectType::Human, false, &["family"]),
        subject("family:carol", SubjectType::Human, false, &["family"]),
        subject("_system:daemon", SubjectType::Service, false, &["_system"]),
    ])
    .await;
    let broker = InMemoryOverrideBroker::new(catalog).with_evidence_emitter(emitter);

    let binding = broker
        .grant_override(GrantOverrideRequest {
            class: OverrideClass::TripleHuman,
            granted_by: vec![
                SubjectRef("family:alice".to_owned()),
                SubjectRef("family:bob".to_owned()),
                SubjectRef("family:carol".to_owned()),
            ],
            target_action_id: Some(ActionId::new()),
            expires_at: Utc::now() + Duration::minutes(5),
            reason: "maximum quorum incident drill".to_owned(),
        })
        .await
        .expect("grant triple human");
    let non_human = broker
        .grant_override(GrantOverrideRequest {
            class: OverrideClass::TripleHuman,
            granted_by: vec![
                SubjectRef("family:alice".to_owned()),
                SubjectRef("family:bob".to_owned()),
                SubjectRef("_system:daemon".to_owned()),
            ],
            target_action_id: Some(ActionId::new()),
            expires_at: Utc::now() + Duration::minutes(5),
            reason: "service cannot participate".to_owned(),
        })
        .await
        .expect_err("service is not a human approver");

    assert_eq!(binding.granted_by.len(), 3);
    assert!(matches!(
        non_human,
        VaultError::OverrideRequiresHumanApprovers { .. }
    ));
}
