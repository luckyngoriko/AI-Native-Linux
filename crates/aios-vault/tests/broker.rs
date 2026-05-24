//! T-047 integration tests for the `VaultBroker` trait and in-memory harness.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::{Duration, Utc};
use strum::IntoEnumIterator;

use aios_vault::{
    CapabilityClass, CapabilityId, CapabilityState, InMemoryVaultBroker, IssueCapabilityRequest,
    KeyAlgorithm, SubjectRef, UseCapabilityRequest, UseCapabilityResult, VaultBroker, VaultError,
    VaultOperation,
};

const SECRET_BYTES: &[u8] = b"super-secret-key-material";

fn subject(id: &str) -> SubjectRef {
    SubjectRef(id.to_owned())
}

const fn issue_request(
    class: CapabilityClass,
    issued_to: SubjectRef,
    key_material_bytes: Option<Vec<u8>>,
) -> IssueCapabilityRequest {
    IssueCapabilityRequest {
        class,
        issued_to,
        expires_at: None,
        key_algorithm: KeyAlgorithm::Aes256Gcm,
        key_material_bytes,
    }
}

async fn issue(
    broker: &InMemoryVaultBroker,
    class: CapabilityClass,
    issued_to: SubjectRef,
) -> aios_vault::VaultCapability {
    broker
        .issue_capability(issue_request(class, issued_to, Some(SECRET_BYTES.to_vec())))
        .await
        .expect("issue capability")
}

#[tokio::test]
async fn issue_capability_happy_path_returns_active_public_record() {
    let broker = InMemoryVaultBroker::new();

    let capability = broker
        .issue_capability(issue_request(
            CapabilityClass::KeyEncrypt,
            subject("family:alice"),
            None,
        ))
        .await
        .expect("issue capability");

    assert_eq!(capability.class, CapabilityClass::KeyEncrypt);
    assert_eq!(capability.issued_to, subject("family:alice"));
    assert_eq!(capability.state, CapabilityState::Active);
    assert!(capability
        .key_material_handle
        .0
        .starts_with("vault-internal:"));
}

#[tokio::test]
async fn issue_capability_accepts_each_capability_class() {
    let broker = InMemoryVaultBroker::new();

    for class in CapabilityClass::iter() {
        let capability = broker
            .issue_capability(issue_request(class, subject("family:alice"), None))
            .await
            .expect("issue capability class");

        assert_eq!(capability.class, class);
        assert_eq!(capability.state, CapabilityState::Active);
    }
}

#[tokio::test]
async fn use_capability_with_matching_encrypt_class_returns_placeholder() {
    let broker = InMemoryVaultBroker::new();
    let capability = issue(
        &broker,
        CapabilityClass::KeyEncrypt,
        subject("family:alice"),
    )
    .await;

    let result = broker
        .use_capability(UseCapabilityRequest {
            capability_id: capability.capability_id,
            operation: VaultOperation::Encrypt {
                plaintext: b"secret plaintext".to_vec(),
                aad: b"aad".to_vec(),
            },
        })
        .await
        .expect("use capability");

    match result {
        UseCapabilityResult::Encrypted {
            ciphertext,
            nonce,
            aad,
        } => {
            assert_eq!(ciphertext, b"operation_simulated".to_vec());
            assert_eq!(nonce, b"operation_simulated".to_vec());
            assert_eq!(aad, b"aad".to_vec());
        }
        other => panic!("expected encrypted result, got {other:?}"),
    }
}

#[tokio::test]
async fn use_capability_with_mismatched_class_returns_operation_class_mismatch() {
    let broker = InMemoryVaultBroker::new();
    let capability = issue(
        &broker,
        CapabilityClass::KeyEncrypt,
        subject("family:alice"),
    )
    .await;

    let error = broker
        .use_capability(UseCapabilityRequest {
            capability_id: capability.capability_id,
            operation: VaultOperation::Sign {
                message: b"payload".to_vec(),
            },
        })
        .await
        .expect_err("mismatched class must fail");

    match error {
        VaultError::OperationClassMismatch {
            capability_class,
            operation_kind,
        } => {
            assert_eq!(capability_class, CapabilityClass::KeyEncrypt);
            assert_eq!(operation_kind, "SIGN");
        }
        other => panic!("expected OperationClassMismatch, got {other:?}"),
    }
}

#[tokio::test]
async fn use_capability_on_revoked_capability_returns_capability_revoked() {
    let broker = InMemoryVaultBroker::new();
    let capability = issue(&broker, CapabilityClass::KeySign, subject("family:alice")).await;

    broker
        .revoke_capability(&capability.capability_id, &subject("family:operator"))
        .await
        .expect("revoke capability");

    let error = broker
        .use_capability(UseCapabilityRequest {
            capability_id: capability.capability_id.clone(),
            operation: VaultOperation::Sign {
                message: b"payload".to_vec(),
            },
        })
        .await
        .expect_err("revoked capability must fail");

    assert_eq!(
        error,
        VaultError::CapabilityRevoked(capability.capability_id)
    );
}

#[tokio::test]
async fn use_capability_on_expired_capability_returns_capability_expired() {
    let broker = InMemoryVaultBroker::new();
    let capability = broker
        .issue_capability(IssueCapabilityRequest {
            class: CapabilityClass::KeySign,
            issued_to: subject("family:alice"),
            expires_at: Some(Utc::now() - Duration::seconds(1)),
            key_algorithm: KeyAlgorithm::Ed25519,
            key_material_bytes: Some(SECRET_BYTES.to_vec()),
        })
        .await
        .expect("issue expired capability");

    let error = broker
        .use_capability(UseCapabilityRequest {
            capability_id: capability.capability_id.clone(),
            operation: VaultOperation::Sign {
                message: b"payload".to_vec(),
            },
        })
        .await
        .expect_err("expired capability must fail");

    assert_eq!(
        error,
        VaultError::CapabilityExpired(capability.capability_id)
    );
}

#[tokio::test]
async fn use_capability_on_unknown_capability_returns_capability_not_found() {
    let broker = InMemoryVaultBroker::new();
    let capability_id = CapabilityId::new();

    let error = broker
        .use_capability(UseCapabilityRequest {
            capability_id: capability_id.clone(),
            operation: VaultOperation::Sign {
                message: b"payload".to_vec(),
            },
        })
        .await
        .expect_err("unknown capability must fail");

    assert_eq!(error, VaultError::CapabilityNotFound(capability_id));
}

#[tokio::test]
async fn list_capabilities_filters_by_subject() {
    let broker = InMemoryVaultBroker::new();
    issue(&broker, CapabilityClass::KeySign, subject("family:alice")).await;
    issue(
        &broker,
        CapabilityClass::MacGenerate,
        subject("family:alice"),
    )
    .await;
    issue(&broker, CapabilityClass::KeyVerify, subject("family:bob")).await;

    let listed = broker
        .list_capabilities(&subject("family:alice"))
        .await
        .expect("list capabilities");

    assert_eq!(listed.len(), 2);
    assert!(listed
        .iter()
        .all(|capability| capability.issued_to == subject("family:alice")));
}

#[tokio::test]
async fn revoke_capability_transitions_active_to_revoked() {
    let broker = InMemoryVaultBroker::new();
    let capability = issue(&broker, CapabilityClass::KeySign, subject("family:alice")).await;

    broker
        .revoke_capability(&capability.capability_id, &subject("family:operator"))
        .await
        .expect("revoke capability");

    let listed = broker
        .list_capabilities(&subject("family:alice"))
        .await
        .expect("list capabilities");
    let revoked = listed
        .iter()
        .find(|listed| listed.capability_id == capability.capability_id)
        .expect("revoked capability present");

    assert_eq!(revoked.state, CapabilityState::Revoked);
}

#[tokio::test]
async fn revoke_capability_on_unknown_returns_capability_not_found() {
    let broker = InMemoryVaultBroker::new();
    let capability_id = CapabilityId::new();

    let error = broker
        .revoke_capability(&capability_id, &subject("family:operator"))
        .await
        .expect_err("unknown capability must fail");

    assert_eq!(error, VaultError::CapabilityNotFound(capability_id));
}

#[tokio::test]
async fn vault_capability_serialize_omits_key_bytes() {
    let broker = InMemoryVaultBroker::new();
    let capability = issue(
        &broker,
        CapabilityClass::KeyEncrypt,
        subject("family:alice"),
    )
    .await;

    let json = serde_json::to_string(&capability).expect("serialize capability");

    assert!(!json.contains("super-secret-key-material"));
    assert!(!json.contains("key_material_bytes"));
    assert!(!json.contains("\"bytes\""));
}

#[tokio::test]
async fn use_capability_result_omits_key_bytes() {
    let broker = InMemoryVaultBroker::new();
    let capability = issue(
        &broker,
        CapabilityClass::KeyEncrypt,
        subject("family:alice"),
    )
    .await;
    let plaintext = b"plaintext-that-must-not-appear".to_vec();
    let plaintext_debug = format!("{plaintext:?}");

    let result = broker
        .use_capability(UseCapabilityRequest {
            capability_id: capability.capability_id,
            operation: VaultOperation::Encrypt {
                plaintext,
                aad: b"aad".to_vec(),
            },
        })
        .await
        .expect("use capability");
    let rendered = format!("{result:?}");

    assert!(!rendered.contains("key_material"));
    assert!(!rendered.contains("super-secret-key-material"));
    assert!(!rendered.contains(&plaintext_debug));
}

#[tokio::test]
async fn list_capabilities_result_omits_key_bytes() {
    let broker = InMemoryVaultBroker::new();
    issue(
        &broker,
        CapabilityClass::KeyEncrypt,
        subject("family:alice"),
    )
    .await;

    let listed = broker
        .list_capabilities(&subject("family:alice"))
        .await
        .expect("list capabilities");
    let json = serde_json::to_string(&listed).expect("serialize capability list");

    assert!(!json.contains("super-secret-key-material"));
    assert!(!json.contains("key_material_bytes"));
    assert!(!json.contains("\"bytes\""));
}

#[tokio::test]
async fn arc_dyn_vault_broker_dispatch_compiles_and_works() {
    let broker: Arc<dyn VaultBroker> = Arc::new(InMemoryVaultBroker::new());
    let capability = broker
        .issue_capability(issue_request(
            CapabilityClass::MacGenerate,
            subject("family:alice"),
            Some(SECRET_BYTES.to_vec()),
        ))
        .await
        .expect("issue via trait object");

    let result = broker
        .use_capability(UseCapabilityRequest {
            capability_id: capability.capability_id,
            operation: VaultOperation::MacGenerate {
                message: b"payload".to_vec(),
            },
        })
        .await
        .expect("use via trait object");

    assert_eq!(
        result,
        UseCapabilityResult::MacGenerated {
            tag: b"operation_simulated".to_vec()
        }
    );
}

#[tokio::test]
async fn secret_get_operation_is_unsupported_in_t047_without_reveal() {
    let broker = InMemoryVaultBroker::new();
    let capability = issue(&broker, CapabilityClass::SecretGet, subject("family:alice")).await;
    let operation = VaultOperation::SecretGet {
        co_signer_approval_id: "approval:human-cosigner".to_owned(),
    };

    let error = broker
        .use_capability(UseCapabilityRequest {
            capability_id: capability.capability_id,
            operation: operation.clone(),
        })
        .await
        .expect_err("raw reveal is not in T-047");

    assert_eq!(error, VaultError::OperationUnsupportedInT047(operation));
}
