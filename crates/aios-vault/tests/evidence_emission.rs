//! T-053 integration tests for S5.2/S5.4 -> S3.1 evidence emission.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::{DateTime, Duration, TimeZone, Utc};
use ed25519_dalek::SigningKey;
use serde::de::DeserializeOwned;

use aios_action::ActionId;
use aios_evidence::{EvidenceReceipt, RecordType};
use aios_vault::{
    CapabilityClass, CapabilityExpiredPayload, CapabilityId, CapabilityIssuedPayload,
    CapabilityLifecycleDriver, CapabilityRevokedPayload, CapabilityUsedPayload,
    GrantOverrideRequest, IdentityCatalog, InMemoryOverrideBroker, InMemoryVaultBroker,
    InMemoryVaultEvidenceLog, IssueCapabilityRequest, KeyAlgorithm, OverrideBroker, OverrideClass,
    OverrideConsumedPayload, OverrideGrantedPayload, OverrideRevokedPayload, Subject, SubjectRef,
    SubjectType, VaultBroker, VaultEvidenceEmitter, VaultOperation,
};

const AES_KEY: [u8; 32] = [0xA5; 32];
const SIGNING_KEY_BYTES: [u8; 32] = *b"AIOS_SIGNING_KEY_MATERIAL_32B!!!";
const PLAINTEXT_MARKER: &str = "AIOS_SECRET_PLAINTEXT_MARKER";
const SIGN_INPUT_MARKER: &str = "AIOS_SIGN_INPUT_MARKER";

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[53_u8; 32])
}

fn subject(id: &str) -> SubjectRef {
    SubjectRef(id.to_owned())
}

fn now_fixture() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 24, 12, 0, 0)
        .single()
        .expect("fixture timestamp is valid")
}

fn evidence_fixture() -> (Arc<InMemoryVaultEvidenceLog>, Arc<VaultEvidenceEmitter>) {
    let log = Arc::new(InMemoryVaultEvidenceLog::new());
    let emitter = Arc::new(VaultEvidenceEmitter::new(
        log.clone(),
        signing_key(),
        subject("_system:service:vault-broker"),
    ));
    (log, emitter)
}

const fn issue_request(
    class: CapabilityClass,
    issued_to: SubjectRef,
    key_algorithm: KeyAlgorithm,
    key_material_bytes: Vec<u8>,
) -> IssueCapabilityRequest {
    IssueCapabilityRequest {
        class,
        issued_to,
        expires_at: None,
        key_algorithm,
        key_material_bytes: Some(key_material_bytes),
    }
}

async fn issue_encrypt_capability(broker: &InMemoryVaultBroker) -> aios_vault::VaultCapability {
    broker
        .issue_capability(issue_request(
            CapabilityClass::KeyEncrypt,
            subject("family:alice"),
            KeyAlgorithm::Aes256Gcm,
            AES_KEY.to_vec(),
        ))
        .await
        .expect("issue encrypt capability")
}

fn payload_as<T>(receipt: &EvidenceReceipt) -> T
where
    T: DeserializeOwned,
{
    serde_json::from_value(receipt.payload().clone()).expect("payload must decode")
}

fn payload_json(receipt: &EvidenceReceipt) -> String {
    serde_json::to_string(receipt.payload()).expect("payload serializes")
}

fn round_trip<T>(payload: &T)
where
    T: serde::Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(payload).expect("serialise");
    let back: T = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(&back, payload);
}

#[test]
fn capability_issued_payload_round_trips_through_serde_json() {
    round_trip(&CapabilityIssuedPayload {
        capability_id: CapabilityId::parse("cap_01HXY8K2JPQ7N3M4R5S6T7V8W9")
            .expect("capability id"),
        class: CapabilityClass::KeyEncrypt,
        issued_to: subject("family:alice"),
        issued_at: now_fixture(),
        expires_at: Some(now_fixture() + Duration::minutes(5)),
    });
}

#[test]
fn capability_used_payload_round_trips_through_serde_json() {
    round_trip(&CapabilityUsedPayload {
        capability_id: CapabilityId::parse("cap_01HXY8K2JPQ7N3M4R5S6T7V8W9")
            .expect("capability id"),
        operation_kind: "Encrypt".to_owned(),
        used_at: now_fixture(),
        subject: subject("_system:service:vault-broker"),
    });
}

#[test]
fn capability_revoked_payload_round_trips_through_serde_json() {
    round_trip(&CapabilityRevokedPayload {
        capability_id: CapabilityId::parse("cap_01HXY8K2JPQ7N3M4R5S6T7V8W9")
            .expect("capability id"),
        revoked_by: subject("family:operator"),
        reason: "admin_request".to_owned(),
        revoked_at: now_fixture(),
    });
}

#[test]
fn capability_expired_payload_round_trips_through_serde_json() {
    round_trip(&CapabilityExpiredPayload {
        capability_id: CapabilityId::parse("cap_01HXY8K2JPQ7N3M4R5S6T7V8W9")
            .expect("capability id"),
        expired_at: now_fixture(),
    });
}

#[test]
fn override_granted_payload_round_trips_through_serde_json() {
    round_trip(&OverrideGrantedPayload {
        binding_id: "ovr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        class: OverrideClass::DualHuman,
        granted_by: vec![subject("family:alice"), subject("family:bob")],
        target_action_id: Some(ActionId::new()),
        granted_at: now_fixture(),
        expires_at: now_fixture() + Duration::minutes(5),
    });
}

#[test]
fn override_consumed_payload_round_trips_through_serde_json() {
    round_trip(&OverrideConsumedPayload {
        binding_id: "ovr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        consumer: subject("family:alice"),
        consumed_at: now_fixture(),
    });
}

#[test]
fn override_revoked_payload_round_trips_through_serde_json() {
    round_trip(&OverrideRevokedPayload {
        binding_id: "ovr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        revoker: subject("family:bob"),
        revoked_at: now_fixture(),
    });
}

#[tokio::test]
async fn issue_capability_emits_vault_capability_issued_with_correct_payload() {
    let (log, emitter) = evidence_fixture();
    let broker = InMemoryVaultBroker::new().with_evidence_emitter(emitter);

    let capability = issue_encrypt_capability(&broker).await;
    let receipts = log.receipts().await;

    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].record_type(), RecordType::VaultCapabilityIssued);
    let payload: CapabilityIssuedPayload = payload_as(&receipts[0]);
    assert_eq!(payload.capability_id, capability.capability_id);
    assert_eq!(payload.class, CapabilityClass::KeyEncrypt);
    assert_eq!(payload.issued_to, subject("family:alice"));
    assert_eq!(payload.expires_at, None);
    let json = payload_json(&receipts[0]);
    assert!(!json.contains("vault-internal:"));
    assert!(!json.contains("key_material_handle"));
}

#[tokio::test]
async fn use_capability_encrypt_emits_vault_operation_without_plaintext() {
    let (log, emitter) = evidence_fixture();
    let broker = InMemoryVaultBroker::new().with_evidence_emitter(emitter);
    let capability = issue_encrypt_capability(&broker).await;

    broker
        .use_capability(aios_vault::UseCapabilityRequest {
            capability_id: capability.capability_id.clone(),
            operation: VaultOperation::Encrypt {
                plaintext: PLAINTEXT_MARKER.as_bytes().to_vec(),
                aad: b"aad".to_vec(),
            },
        })
        .await
        .expect("use capability");
    let receipts = log.receipts().await;

    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[1].record_type(), RecordType::VaultOperation);
    let payload: CapabilityUsedPayload = payload_as(&receipts[1]);
    assert_eq!(payload.capability_id, capability.capability_id);
    assert_eq!(payload.operation_kind, "Encrypt");
    assert!(!payload_json(&receipts[1]).contains(PLAINTEXT_MARKER));
}

#[tokio::test]
async fn use_capability_sign_emits_vault_operation_without_key_or_message_bytes() {
    let (log, emitter) = evidence_fixture();
    let broker = InMemoryVaultBroker::new().with_evidence_emitter(emitter);
    let capability = broker
        .issue_capability(issue_request(
            CapabilityClass::KeySign,
            subject("family:alice"),
            KeyAlgorithm::Ed25519,
            SIGNING_KEY_BYTES.to_vec(),
        ))
        .await
        .expect("issue signing capability");

    broker
        .use_capability(aios_vault::UseCapabilityRequest {
            capability_id: capability.capability_id.clone(),
            operation: VaultOperation::Sign {
                message: SIGN_INPUT_MARKER.as_bytes().to_vec(),
            },
        })
        .await
        .expect("use signing capability");
    let receipts = log.receipts().await;

    assert_eq!(receipts[1].record_type(), RecordType::VaultOperation);
    let payload: CapabilityUsedPayload = payload_as(&receipts[1]);
    assert_eq!(payload.operation_kind, "Sign");
    let json = payload_json(&receipts[1]);
    assert!(!json.contains("AIOS_SIGNING_KEY_MATERIAL_32B"));
    assert!(!json.contains(SIGN_INPUT_MARKER));
}

#[tokio::test]
async fn revoke_capability_emits_vault_capability_revoked() {
    let (log, emitter) = evidence_fixture();
    let broker = InMemoryVaultBroker::new().with_evidence_emitter(emitter);
    let capability = issue_encrypt_capability(&broker).await;
    let revoker = subject("family:operator");

    broker
        .revoke_capability(&capability.capability_id, &revoker)
        .await
        .expect("revoke capability");
    let receipts = log.receipts().await;

    assert_eq!(receipts.len(), 2);
    assert_eq!(
        receipts[1].record_type(),
        RecordType::VaultCapabilityRevoked
    );
    let payload: CapabilityRevokedPayload = payload_as(&receipts[1]);
    assert_eq!(payload.capability_id, capability.capability_id);
    assert_eq!(payload.revoked_by, revoker);
    assert_eq!(payload.reason, "admin_request");
}

#[tokio::test]
async fn expire_capability_emits_expiration_payload_as_vault_operation() {
    let (log, emitter) = evidence_fixture();
    let audit = Arc::new(aios_vault::CapabilityAuditLog::new());
    let broker = Arc::new(
        InMemoryVaultBroker::new()
            .with_audit_log(Arc::clone(&audit))
            .with_evidence_emitter(Arc::clone(&emitter)),
    );
    let capability = issue_encrypt_capability(&broker).await;
    let driver =
        CapabilityLifecycleDriver::with_evidence_emitter(Arc::clone(&broker), audit, emitter);

    driver
        .expire_capability(&capability.capability_id)
        .await
        .expect("expire capability");
    let receipts = log.receipts().await;

    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[1].record_type(), RecordType::VaultOperation);
    let payload: CapabilityExpiredPayload = payload_as(&receipts[1]);
    assert_eq!(payload.capability_id, capability.capability_id);
}

#[tokio::test]
async fn run_expiration_pass_emits_one_expiration_receipt_per_transition() {
    let (log, emitter) = evidence_fixture();
    let audit = Arc::new(aios_vault::CapabilityAuditLog::new());
    let broker = Arc::new(
        InMemoryVaultBroker::new()
            .with_audit_log(Arc::clone(&audit))
            .with_evidence_emitter(Arc::clone(&emitter)),
    );
    let now = Utc::now();
    let capability = broker
        .issue_capability(IssueCapabilityRequest {
            class: CapabilityClass::KeyEncrypt,
            issued_to: subject("family:alice"),
            expires_at: Some(now - Duration::seconds(1)),
            key_algorithm: KeyAlgorithm::Aes256Gcm,
            key_material_bytes: Some(AES_KEY.to_vec()),
        })
        .await
        .expect("issue expiring capability");
    let driver =
        CapabilityLifecycleDriver::with_evidence_emitter(Arc::clone(&broker), audit, emitter);

    let report = driver.run_expiration_pass(now).await.expect("run pass");
    let receipts = log.receipts().await;

    assert_eq!(report.capabilities_expired, 1);
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[1].record_type(), RecordType::VaultOperation);
    let payload: CapabilityExpiredPayload = payload_as(&receipts[1]);
    assert_eq!(payload.capability_id, capability.capability_id);
}

#[tokio::test]
async fn grant_override_emits_override_granted_with_all_subjects() {
    let (log, emitter) = evidence_fixture();
    let broker = broker_with_standard_subjects()
        .await
        .with_evidence_emitter(emitter);

    let binding = broker
        .grant_override(override_request(
            OverrideClass::DualHuman,
            &["family:alice", "family:bob"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("grant override");
    let receipts = log.receipts().await;

    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].record_type(), RecordType::OverrideGranted);
    let payload: OverrideGrantedPayload = payload_as(&receipts[0]);
    assert_eq!(payload.binding_id, binding.binding_id);
    assert_eq!(
        payload.granted_by,
        vec![subject("family:alice"), subject("family:bob")]
    );
    assert_eq!(payload.target_action_id, binding.target_action_id);
}

#[tokio::test]
async fn consume_override_emits_override_consumed() {
    let (log, emitter) = evidence_fixture();
    let broker = broker_with_standard_subjects()
        .await
        .with_evidence_emitter(emitter);
    let binding = broker
        .grant_override(override_request(
            OverrideClass::StrongSolo,
            &["family:alice"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("grant override");
    let consumer = subject("family:alice");

    broker
        .consume_override(&binding.binding_id, &consumer)
        .await
        .expect("consume override");
    let receipts = log.receipts().await;

    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[1].record_type(), RecordType::OverrideConsumed);
    let payload: OverrideConsumedPayload = payload_as(&receipts[1]);
    assert_eq!(payload.binding_id, binding.binding_id);
    assert_eq!(payload.consumer, consumer);
}

#[tokio::test]
async fn revoke_override_emits_override_revoked() {
    let (log, emitter) = evidence_fixture();
    let broker = broker_with_standard_subjects()
        .await
        .with_evidence_emitter(emitter);
    let binding = broker
        .grant_override(override_request(
            OverrideClass::StrongSolo,
            &["family:alice"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("grant override");
    let revoker = subject("family:bob");

    broker
        .revoke_override(&binding.binding_id, &revoker)
        .await
        .expect("revoke override");
    let receipts = log.receipts().await;

    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[1].record_type(), RecordType::OverrideRevoked);
    let payload: OverrideRevokedPayload = payload_as(&receipts[1]);
    assert_eq!(payload.binding_id, binding.binding_id);
    assert_eq!(payload.revoker, revoker);
}

#[tokio::test]
async fn blake3_chain_links_second_receipt_to_first_receipt_hash() {
    let (log, emitter) = evidence_fixture();
    let broker = InMemoryVaultBroker::new().with_evidence_emitter(emitter);
    let capability = issue_encrypt_capability(&broker).await;

    broker
        .use_capability(aios_vault::UseCapabilityRequest {
            capability_id: capability.capability_id,
            operation: VaultOperation::Encrypt {
                plaintext: b"payload".to_vec(),
                aad: b"aad".to_vec(),
            },
        })
        .await
        .expect("use capability");
    let receipts = log.receipts().await;

    assert_eq!(
        receipts[1].previous_receipt_hash(),
        Some(receipts[0].link_hash().expect("first link hash").as_str())
    );
    log.verify_integrity().await.expect("chain verifies");
}

#[tokio::test]
async fn emitted_payloads_do_not_leak_plaintext_key_bytes_or_internal_handles() {
    let (log, emitter) = evidence_fixture();
    let broker = InMemoryVaultBroker::new().with_evidence_emitter(emitter);
    let encrypt_cap = issue_encrypt_capability(&broker).await;
    let sign_cap = broker
        .issue_capability(issue_request(
            CapabilityClass::KeySign,
            subject("family:alice"),
            KeyAlgorithm::Ed25519,
            SIGNING_KEY_BYTES.to_vec(),
        ))
        .await
        .expect("issue signing capability");

    broker
        .use_capability(aios_vault::UseCapabilityRequest {
            capability_id: encrypt_cap.capability_id.clone(),
            operation: VaultOperation::Encrypt {
                plaintext: PLAINTEXT_MARKER.as_bytes().to_vec(),
                aad: b"aad".to_vec(),
            },
        })
        .await
        .expect("encrypt");
    broker
        .use_capability(aios_vault::UseCapabilityRequest {
            capability_id: sign_cap.capability_id,
            operation: VaultOperation::Sign {
                message: SIGN_INPUT_MARKER.as_bytes().to_vec(),
            },
        })
        .await
        .expect("sign");
    broker
        .revoke_capability(&encrypt_cap.capability_id, &subject("family:operator"))
        .await
        .expect("revoke");

    for receipt in log.receipts().await {
        let json = payload_json(&receipt);
        assert!(!json.contains(PLAINTEXT_MARKER));
        assert!(!json.contains(SIGN_INPUT_MARKER));
        assert!(!json.contains("AIOS_SIGNING_KEY_MATERIAL_32B"));
        assert!(!json.contains("vault-internal:"));
    }
}

#[tokio::test]
async fn no_emitter_configured_preserves_existing_success_paths() {
    let broker = InMemoryVaultBroker::new();
    let capability = issue_encrypt_capability(&broker).await;

    broker
        .use_capability(aios_vault::UseCapabilityRequest {
            capability_id: capability.capability_id.clone(),
            operation: VaultOperation::Encrypt {
                plaintext: b"payload".to_vec(),
                aad: b"aad".to_vec(),
            },
        })
        .await
        .expect("use capability without emitter");
    broker
        .revoke_capability(&capability.capability_id, &subject("family:operator"))
        .await
        .expect("revoke without emitter");

    let override_broker = broker_with_standard_subjects().await;
    let binding = override_broker
        .grant_override(override_request(
            OverrideClass::StrongSolo,
            &["family:alice"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("grant without emitter");
    override_broker
        .consume_override(&binding.binding_id, &subject("family:alice"))
        .await
        .expect("consume without emitter");
}

#[tokio::test]
async fn emitted_receipts_verify_with_emitter_ed25519_key() {
    let (log, emitter) = evidence_fixture();
    let verifying_key = emitter.verifying_key();
    let broker = InMemoryVaultBroker::new().with_evidence_emitter(emitter);
    let capability = issue_encrypt_capability(&broker).await;

    broker
        .use_capability(aios_vault::UseCapabilityRequest {
            capability_id: capability.capability_id,
            operation: VaultOperation::Encrypt {
                plaintext: b"payload".to_vec(),
                aad: b"aad".to_vec(),
            },
        })
        .await
        .expect("use capability");

    for receipt in log.receipts().await {
        assert!(receipt.is_signed());
        receipt
            .verify_signature(&verifying_key)
            .expect("signature verifies");
    }
}

#[tokio::test]
async fn concurrent_issue_use_revoke_operations_keep_receipt_chain_coherent() {
    let (log, emitter) = evidence_fixture();
    let broker = Arc::new(InMemoryVaultBroker::new().with_evidence_emitter(emitter));

    let mut tasks = Vec::new();
    for i in 0..5 {
        let broker = Arc::clone(&broker);
        tasks.push(tokio::spawn(async move {
            let capability = issue_encrypt_capability(&broker).await;
            broker
                .use_capability(aios_vault::UseCapabilityRequest {
                    capability_id: capability.capability_id.clone(),
                    operation: VaultOperation::Encrypt {
                        plaintext: format!("{PLAINTEXT_MARKER}_{i}").into_bytes(),
                        aad: b"aad".to_vec(),
                    },
                })
                .await
                .expect("use capability");
            broker
                .revoke_capability(&capability.capability_id, &subject("family:operator"))
                .await
                .expect("revoke capability");
        }));
    }

    for task in tasks {
        task.await.expect("join task");
    }

    let receipts = log.receipts().await;
    assert_eq!(receipts.len(), 15);
    log.verify_integrity().await.expect("chain verifies");
    for receipt in receipts {
        receipt
            .verify_signature(&signing_key().verifying_key())
            .expect("signature verifies");
    }
}

async fn broker_with_standard_subjects() -> InMemoryOverrideBroker {
    let catalog = Arc::new(IdentityCatalog::new());
    for subject in [
        identity_subject("family:alice", SubjectType::Human, false, &["family"]),
        identity_subject("family:bob", SubjectType::Human, false, &["family"]),
        identity_subject("family:carol", SubjectType::Human, false, &["family"]),
    ] {
        catalog
            .register_subject(subject)
            .await
            .expect("register fixture subject");
    }
    InMemoryOverrideBroker::new(catalog)
}

fn override_request(
    class: OverrideClass,
    granted_by: &[&str],
    expires_at: DateTime<Utc>,
) -> GrantOverrideRequest {
    GrantOverrideRequest {
        class,
        granted_by: granted_by.iter().map(|id| subject(id)).collect(),
        target_action_id: Some(ActionId::new()),
        expires_at,
        reason: "operator documented emergency override reason".to_owned(),
    }
}

fn identity_subject(
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
        created_at: now_fixture(),
    }
}
