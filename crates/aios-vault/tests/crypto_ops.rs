//! T-049 real crypto operation coverage for the in-memory vault broker.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use aios_vault::{
    CapabilityClass, InMemoryVaultBroker, IssueCapabilityRequest, KeyAlgorithm, SubjectRef,
    UseCapabilityRequest, UseCapabilityResult, VaultBroker, VaultError, VaultOperation,
};

const AES_KEY: [u8; 32] = [0xA5; 32];
const HMAC_KEY: [u8; 32] = [0x5A; 32];
const HKDF_IKM: [u8; 32] = [0xC3; 32];
const ED25519_SEED: [u8; 32] = [0x42; 32];

fn subject(id: &str) -> SubjectRef {
    SubjectRef(id.to_owned())
}

const fn issue_request(
    class: CapabilityClass,
    key_algorithm: KeyAlgorithm,
    key_material_bytes: Option<Vec<u8>>,
) -> IssueCapabilityRequest {
    IssueCapabilityRequest {
        class,
        issued_to: SubjectRef(String::new()),
        expires_at: None,
        key_algorithm,
        key_material_bytes,
    }
}

async fn issue(
    broker: &InMemoryVaultBroker,
    class: CapabilityClass,
    key_algorithm: KeyAlgorithm,
    key_material_bytes: Option<Vec<u8>>,
) -> aios_vault::VaultCapability {
    let mut request = issue_request(class, key_algorithm, key_material_bytes);
    request.issued_to = subject("family:alice");
    broker
        .issue_capability(request)
        .await
        .expect("issue capability")
}

async fn use_capability(
    broker: &InMemoryVaultBroker,
    capability: &aios_vault::VaultCapability,
    operation: VaultOperation,
) -> UseCapabilityResult {
    broker
        .use_capability(UseCapabilityRequest {
            capability_id: capability.capability_id.clone(),
            operation,
        })
        .await
        .expect("use capability")
}

fn encrypted(result: UseCapabilityResult) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    match result {
        UseCapabilityResult::Encrypted {
            ciphertext,
            nonce,
            aad,
        } => (ciphertext, nonce, aad),
        other => panic!("expected encrypted result, got {other:?}"),
    }
}

fn decrypted(result: UseCapabilityResult) -> Vec<u8> {
    match result {
        UseCapabilityResult::Decrypted { plaintext } => plaintext,
        other => panic!("expected decrypted result, got {other:?}"),
    }
}

fn mac_tag(result: UseCapabilityResult) -> Vec<u8> {
    match result {
        UseCapabilityResult::MacGenerated { tag } => tag,
        other => panic!("expected MAC result, got {other:?}"),
    }
}

fn mac_valid(result: UseCapabilityResult) -> bool {
    match result {
        UseCapabilityResult::MacVerified { valid } => valid,
        other => panic!("expected MAC verification result, got {other:?}"),
    }
}

fn signature(result: UseCapabilityResult) -> Vec<u8> {
    match result {
        UseCapabilityResult::Signed { signature } => signature,
        other => panic!("expected signature result, got {other:?}"),
    }
}

fn verified(result: UseCapabilityResult) -> bool {
    match result {
        UseCapabilityResult::Verified { valid } => valid,
        other => panic!("expected signature verification result, got {other:?}"),
    }
}

fn derived_handle(result: UseCapabilityResult) -> String {
    match result {
        UseCapabilityResult::KdfDerived { derived_key_handle } => derived_key_handle.0,
        other => panic!("expected KDF result, got {other:?}"),
    }
}

fn assert_crypto_error(error: VaultError) {
    match error {
        VaultError::CryptoError(message) => assert!(!message.is_empty()),
        other => panic!("expected CryptoError, got {other:?}"),
    }
}

fn assert_result_debug_does_not_contain_bytes(result: &UseCapabilityResult, bytes: &[u8]) {
    let rendered = format!("{result:?}");
    let rendered_bytes = format!("{bytes:?}");
    assert!(!rendered.contains(&rendered_bytes));
}

#[tokio::test]
async fn key_encrypt_round_trip_decrypts_original_plaintext() {
    let broker = InMemoryVaultBroker::new();
    let encrypt_cap = issue(
        &broker,
        CapabilityClass::KeyEncrypt,
        KeyAlgorithm::Aes256Gcm,
        Some(AES_KEY.to_vec()),
    )
    .await;
    let decrypt_cap = issue(
        &broker,
        CapabilityClass::KeyDecrypt,
        KeyAlgorithm::Aes256Gcm,
        Some(AES_KEY.to_vec()),
    )
    .await;
    let plaintext = b"secret plaintext".to_vec();

    let encrypted_result = use_capability(
        &broker,
        &encrypt_cap,
        VaultOperation::Encrypt {
            plaintext: plaintext.clone(),
            aad: b"aad".to_vec(),
        },
    )
    .await;
    assert_result_debug_does_not_contain_bytes(&encrypted_result, &plaintext);
    let (ciphertext, nonce, aad) = encrypted(encrypted_result);

    assert_eq!(nonce.len(), 12);
    assert_eq!(aad, b"aad".to_vec());
    assert_eq!(
        decrypted(
            use_capability(
                &broker,
                &decrypt_cap,
                VaultOperation::Decrypt {
                    ciphertext,
                    aad: b"aad".to_vec(),
                },
            )
            .await,
        ),
        plaintext
    );
}

#[tokio::test]
async fn key_encrypt_decrypt_rejects_tampered_aad() {
    let broker = InMemoryVaultBroker::new();
    let encrypt_cap = issue(
        &broker,
        CapabilityClass::KeyEncrypt,
        KeyAlgorithm::Aes256Gcm,
        Some(AES_KEY.to_vec()),
    )
    .await;
    let decrypt_cap = issue(
        &broker,
        CapabilityClass::KeyDecrypt,
        KeyAlgorithm::Aes256Gcm,
        Some(AES_KEY.to_vec()),
    )
    .await;
    let plaintext = b"aad protected plaintext".to_vec();
    let encrypted_result = use_capability(
        &broker,
        &encrypt_cap,
        VaultOperation::Encrypt {
            plaintext: plaintext.clone(),
            aad: b"correct aad".to_vec(),
        },
    )
    .await;
    assert_result_debug_does_not_contain_bytes(&encrypted_result, &plaintext);
    let (ciphertext, _nonce, _aad) = encrypted(encrypted_result);

    let error = broker
        .use_capability(UseCapabilityRequest {
            capability_id: decrypt_cap.capability_id,
            operation: VaultOperation::Decrypt {
                ciphertext,
                aad: b"tampered aad".to_vec(),
            },
        })
        .await
        .expect_err("tampered AAD must fail");

    assert_crypto_error(error);
}

#[tokio::test]
async fn key_encrypt_decrypt_rejects_wrong_nonce_prefix() {
    let broker = InMemoryVaultBroker::new();
    let encrypt_cap = issue(
        &broker,
        CapabilityClass::KeyEncrypt,
        KeyAlgorithm::Aes256Gcm,
        Some(AES_KEY.to_vec()),
    )
    .await;
    let decrypt_cap = issue(
        &broker,
        CapabilityClass::KeyDecrypt,
        KeyAlgorithm::Aes256Gcm,
        Some(AES_KEY.to_vec()),
    )
    .await;
    let plaintext = b"nonce protected plaintext".to_vec();
    let encrypted_result = use_capability(
        &broker,
        &encrypt_cap,
        VaultOperation::Encrypt {
            plaintext: plaintext.clone(),
            aad: b"aad".to_vec(),
        },
    )
    .await;
    assert_result_debug_does_not_contain_bytes(&encrypted_result, &plaintext);
    let (mut ciphertext, nonce, _aad) = encrypted(encrypted_result);
    assert_eq!(nonce.len(), 12);
    ciphertext[0] ^= 0xFF;

    let error = broker
        .use_capability(UseCapabilityRequest {
            capability_id: decrypt_cap.capability_id,
            operation: VaultOperation::Decrypt {
                ciphertext,
                aad: b"aad".to_vec(),
            },
        })
        .await
        .expect_err("wrong nonce must fail");

    assert_crypto_error(error);
}

#[tokio::test]
async fn mac_generate_and_verify_accepts_same_message_and_tag() {
    let broker = InMemoryVaultBroker::new();
    let generate_cap = issue(
        &broker,
        CapabilityClass::MacGenerate,
        KeyAlgorithm::HmacSha256,
        Some(HMAC_KEY.to_vec()),
    )
    .await;
    let verify_cap = issue(
        &broker,
        CapabilityClass::MacVerify,
        KeyAlgorithm::HmacSha256,
        Some(HMAC_KEY.to_vec()),
    )
    .await;
    let message = b"message".to_vec();
    let tag = mac_tag(
        use_capability(
            &broker,
            &generate_cap,
            VaultOperation::MacGenerate {
                message: message.clone(),
            },
        )
        .await,
    );

    assert_eq!(tag.len(), 32);
    assert!(mac_valid(
        use_capability(
            &broker,
            &verify_cap,
            VaultOperation::MacVerify { message, tag },
        )
        .await
    ));
}

#[tokio::test]
async fn mac_verify_returns_false_for_wrong_message() {
    let broker = InMemoryVaultBroker::new();
    let generate_cap = issue(
        &broker,
        CapabilityClass::MacGenerate,
        KeyAlgorithm::HmacSha256,
        Some(HMAC_KEY.to_vec()),
    )
    .await;
    let verify_cap = issue(
        &broker,
        CapabilityClass::MacVerify,
        KeyAlgorithm::HmacSha256,
        Some(HMAC_KEY.to_vec()),
    )
    .await;
    let tag = mac_tag(
        use_capability(
            &broker,
            &generate_cap,
            VaultOperation::MacGenerate {
                message: b"message".to_vec(),
            },
        )
        .await,
    );

    assert!(!mac_valid(
        use_capability(
            &broker,
            &verify_cap,
            VaultOperation::MacVerify {
                message: b"wrong message".to_vec(),
                tag,
            },
        )
        .await
    ));
}

#[tokio::test]
async fn mac_verify_rejects_wrong_tag_length_as_crypto_error() {
    let broker = InMemoryVaultBroker::new();
    let verify_cap = issue(
        &broker,
        CapabilityClass::MacVerify,
        KeyAlgorithm::HmacSha256,
        Some(HMAC_KEY.to_vec()),
    )
    .await;

    let error = broker
        .use_capability(UseCapabilityRequest {
            capability_id: verify_cap.capability_id,
            operation: VaultOperation::MacVerify {
                message: b"message".to_vec(),
                tag: vec![0xAA; 31],
            },
        })
        .await
        .expect_err("wrong tag length must fail");

    assert_crypto_error(error);
}

#[tokio::test]
async fn kdf_derive_is_deterministic_for_same_info_and_length() {
    let broker = InMemoryVaultBroker::new();
    let capability = issue(
        &broker,
        CapabilityClass::KeyEncrypt,
        KeyAlgorithm::HkdfSha256,
        Some(HKDF_IKM.to_vec()),
    )
    .await;

    let first = derived_handle(
        use_capability(
            &broker,
            &capability,
            VaultOperation::KdfDerive {
                info: b"aios:test".to_vec(),
                length: 32,
            },
        )
        .await,
    );
    let second = derived_handle(
        use_capability(
            &broker,
            &capability,
            VaultOperation::KdfDerive {
                info: b"aios:test".to_vec(),
                length: 32,
            },
        )
        .await,
    );

    assert_eq!(first, second);
    assert!(first.starts_with("vault-derived:hkdf-sha256:"));
}

#[tokio::test]
async fn kdf_derive_rejects_zero_length_to_avoid_empty_key_handles() {
    let broker = InMemoryVaultBroker::new();
    let capability = issue(
        &broker,
        CapabilityClass::KeyEncrypt,
        KeyAlgorithm::HkdfSha256,
        Some(HKDF_IKM.to_vec()),
    )
    .await;

    let error = broker
        .use_capability(UseCapabilityRequest {
            capability_id: capability.capability_id,
            operation: VaultOperation::KdfDerive {
                info: b"aios:test".to_vec(),
                length: 0,
            },
        })
        .await
        .expect_err("zero-length KDF output must fail");

    assert_crypto_error(error);
}

#[tokio::test]
async fn sign_and_verify_round_trip_accepts_signature() {
    let broker = InMemoryVaultBroker::new();
    let sign_cap = issue(
        &broker,
        CapabilityClass::KeySign,
        KeyAlgorithm::Ed25519,
        Some(ED25519_SEED.to_vec()),
    )
    .await;
    let verify_cap = issue(
        &broker,
        CapabilityClass::KeyVerify,
        KeyAlgorithm::Ed25519,
        Some(ED25519_SEED.to_vec()),
    )
    .await;
    let message = b"signed message".to_vec();

    let signed_result = use_capability(
        &broker,
        &sign_cap,
        VaultOperation::Sign {
            message: message.clone(),
        },
    )
    .await;
    assert_result_debug_does_not_contain_bytes(&signed_result, &ED25519_SEED);
    let signature = signature(signed_result);

    assert_eq!(signature.len(), 64);
    assert!(verified(
        use_capability(
            &broker,
            &verify_cap,
            VaultOperation::Verify { message, signature },
        )
        .await
    ));
}

#[tokio::test]
async fn verify_returns_false_for_wrong_message() {
    let broker = InMemoryVaultBroker::new();
    let sign_cap = issue(
        &broker,
        CapabilityClass::KeySign,
        KeyAlgorithm::Ed25519,
        Some(ED25519_SEED.to_vec()),
    )
    .await;
    let verify_cap = issue(
        &broker,
        CapabilityClass::KeyVerify,
        KeyAlgorithm::Ed25519,
        Some(ED25519_SEED.to_vec()),
    )
    .await;
    let signed_result = use_capability(
        &broker,
        &sign_cap,
        VaultOperation::Sign {
            message: b"signed message".to_vec(),
        },
    )
    .await;
    assert_result_debug_does_not_contain_bytes(&signed_result, &ED25519_SEED);
    let signature = signature(signed_result);

    assert!(!verified(
        use_capability(
            &broker,
            &verify_cap,
            VaultOperation::Verify {
                message: b"wrong message".to_vec(),
                signature,
            },
        )
        .await
    ));
}

#[tokio::test]
async fn verify_returns_false_for_wrong_signature() {
    let broker = InMemoryVaultBroker::new();
    let sign_cap = issue(
        &broker,
        CapabilityClass::KeySign,
        KeyAlgorithm::Ed25519,
        Some(ED25519_SEED.to_vec()),
    )
    .await;
    let verify_cap = issue(
        &broker,
        CapabilityClass::KeyVerify,
        KeyAlgorithm::Ed25519,
        Some(ED25519_SEED.to_vec()),
    )
    .await;
    let signed_result = use_capability(
        &broker,
        &sign_cap,
        VaultOperation::Sign {
            message: b"signed message".to_vec(),
        },
    )
    .await;
    assert_result_debug_does_not_contain_bytes(&signed_result, &ED25519_SEED);
    let mut signature = signature(signed_result);
    signature[0] ^= 0xFF;

    assert!(!verified(
        use_capability(
            &broker,
            &verify_cap,
            VaultOperation::Verify {
                message: b"signed message".to_vec(),
                signature,
            },
        )
        .await
    ));
}

#[tokio::test]
async fn fresh_key_generation_produces_distinct_capabilities_and_keys() {
    let broker = InMemoryVaultBroker::new();
    let first_cap = issue(
        &broker,
        CapabilityClass::MacGenerate,
        KeyAlgorithm::HmacSha256,
        None,
    )
    .await;
    let second_cap = issue(
        &broker,
        CapabilityClass::MacGenerate,
        KeyAlgorithm::HmacSha256,
        None,
    )
    .await;

    let first_tag = mac_tag(
        use_capability(
            &broker,
            &first_cap,
            VaultOperation::MacGenerate {
                message: b"same message".to_vec(),
            },
        )
        .await,
    );
    let second_tag = mac_tag(
        use_capability(
            &broker,
            &second_cap,
            VaultOperation::MacGenerate {
                message: b"same message".to_vec(),
            },
        )
        .await,
    );

    assert_ne!(first_cap.capability_id, second_cap.capability_id);
    assert_ne!(first_tag, second_tag);
}

#[tokio::test]
async fn key_encrypt_debug_result_does_not_contain_input_plaintext() {
    let broker = InMemoryVaultBroker::new();
    let capability = issue(
        &broker,
        CapabilityClass::KeyEncrypt,
        KeyAlgorithm::Aes256Gcm,
        Some(AES_KEY.to_vec()),
    )
    .await;
    let plaintext = b"plaintext-that-must-not-render".to_vec();

    let result = use_capability(
        &broker,
        &capability,
        VaultOperation::Encrypt {
            plaintext: plaintext.clone(),
            aad: b"aad".to_vec(),
        },
    )
    .await;

    assert_result_debug_does_not_contain_bytes(&result, &plaintext);
}

#[tokio::test]
async fn sign_result_and_capability_serialization_do_not_leak_key_material() {
    let broker = InMemoryVaultBroker::new();
    let capability = issue(
        &broker,
        CapabilityClass::KeySign,
        KeyAlgorithm::Ed25519,
        Some(ED25519_SEED.to_vec()),
    )
    .await;
    let result = use_capability(
        &broker,
        &capability,
        VaultOperation::Sign {
            message: b"payload".to_vec(),
        },
    )
    .await;

    assert_result_debug_does_not_contain_bytes(&result, &ED25519_SEED);
    let serialized = serde_json::to_string(&capability).expect("serialize capability");
    assert!(!serialized.contains(&format!("{ED25519_SEED:?}")));
    assert!(!serialized.contains("key_material_bytes"));
    assert!(!serialized.contains("\"bytes\""));
}

#[tokio::test]
async fn algorithm_mismatch_is_rejected_at_issue_time() {
    let broker = InMemoryVaultBroker::new();

    let error = broker
        .issue_capability(IssueCapabilityRequest {
            class: CapabilityClass::KeyEncrypt,
            issued_to: subject("family:alice"),
            expires_at: None,
            key_algorithm: KeyAlgorithm::Ed25519,
            key_material_bytes: None,
        })
        .await
        .expect_err("algorithm mismatch must fail");

    assert_eq!(
        error,
        VaultError::KeyAlgorithmMismatch {
            expected: KeyAlgorithm::Aes256Gcm,
            found: KeyAlgorithm::Ed25519,
        }
    );
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "single coverage sweep intentionally touches all nine capability classes"
)]
async fn every_capability_class_is_exercised_at_least_once() {
    let broker = InMemoryVaultBroker::new();
    let encrypt_cap = issue(
        &broker,
        CapabilityClass::KeyEncrypt,
        KeyAlgorithm::Aes256Gcm,
        Some(AES_KEY.to_vec()),
    )
    .await;
    let decrypt_cap = issue(
        &broker,
        CapabilityClass::KeyDecrypt,
        KeyAlgorithm::Aes256Gcm,
        Some(AES_KEY.to_vec()),
    )
    .await;
    let encrypted_result = use_capability(
        &broker,
        &encrypt_cap,
        VaultOperation::Encrypt {
            plaintext: b"class coverage".to_vec(),
            aad: Vec::new(),
        },
    )
    .await;
    let (ciphertext, _nonce, _aad) = encrypted(encrypted_result);
    let _ = use_capability(
        &broker,
        &decrypt_cap,
        VaultOperation::Decrypt {
            ciphertext,
            aad: Vec::new(),
        },
    )
    .await;

    let mac_generate_cap = issue(
        &broker,
        CapabilityClass::MacGenerate,
        KeyAlgorithm::HmacSha256,
        Some(HMAC_KEY.to_vec()),
    )
    .await;
    let tag = mac_tag(
        use_capability(
            &broker,
            &mac_generate_cap,
            VaultOperation::MacGenerate {
                message: b"class coverage".to_vec(),
            },
        )
        .await,
    );
    let mac_verify_cap = issue(
        &broker,
        CapabilityClass::MacVerify,
        KeyAlgorithm::HmacSha256,
        Some(HMAC_KEY.to_vec()),
    )
    .await;
    let _ = use_capability(
        &broker,
        &mac_verify_cap,
        VaultOperation::MacVerify {
            message: b"class coverage".to_vec(),
            tag,
        },
    )
    .await;

    let sign_cap = issue(
        &broker,
        CapabilityClass::KeySign,
        KeyAlgorithm::Ed25519,
        Some(ED25519_SEED.to_vec()),
    )
    .await;
    let signed = use_capability(
        &broker,
        &sign_cap,
        VaultOperation::Sign {
            message: b"class coverage".to_vec(),
        },
    )
    .await;
    let sig = signature(signed);
    let verify_cap = issue(
        &broker,
        CapabilityClass::KeyVerify,
        KeyAlgorithm::Ed25519,
        Some(ED25519_SEED.to_vec()),
    )
    .await;
    let _ = use_capability(
        &broker,
        &verify_cap,
        VaultOperation::Verify {
            message: b"class coverage".to_vec(),
            signature: sig,
        },
    )
    .await;

    let random_cap = issue(
        &broker,
        CapabilityClass::RandomGenerate,
        KeyAlgorithm::Aes256Gcm,
        None,
    )
    .await;
    let random = use_capability(
        &broker,
        &random_cap,
        VaultOperation::RandomGenerate { byte_count: 16 },
    )
    .await;
    match random {
        UseCapabilityResult::RandomGenerated { random_bytes } => assert_eq!(random_bytes.len(), 16),
        other => panic!("expected random result, got {other:?}"),
    }

    let secret_cap = issue(
        &broker,
        CapabilityClass::SecretGet,
        KeyAlgorithm::Aes256Gcm,
        Some(AES_KEY.to_vec()),
    )
    .await;
    let secret_operation = VaultOperation::SecretGet {
        co_signer_approval_id: "approval:human-cosigner".to_owned(),
    };
    let secret_error = broker
        .use_capability(UseCapabilityRequest {
            capability_id: secret_cap.capability_id,
            operation: secret_operation.clone(),
        })
        .await
        .expect_err("SECRET_GET remains unsupported");
    assert_eq!(
        secret_error,
        VaultError::OperationUnsupportedInT049(secret_operation)
    );

    let bootstrap_cap = issue(
        &broker,
        CapabilityClass::BootstrapKeySign,
        KeyAlgorithm::Ed25519,
        Some(ED25519_SEED.to_vec()),
    )
    .await;
    let bootstrap_result = use_capability(
        &broker,
        &bootstrap_cap,
        VaultOperation::Sign {
            message: b"firstboot marker".to_vec(),
        },
    )
    .await;
    assert_result_debug_does_not_contain_bytes(&bootstrap_result, &ED25519_SEED);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_key_encrypt_use_on_same_capability_succeeds() {
    let broker = Arc::new(InMemoryVaultBroker::new());
    let capability = issue(
        &broker,
        CapabilityClass::KeyEncrypt,
        KeyAlgorithm::Aes256Gcm,
        Some(AES_KEY.to_vec()),
    )
    .await;
    let first_broker = Arc::clone(&broker);
    let first_capability = capability.clone();
    let second_broker = Arc::clone(&broker);
    let second_capability = capability.clone();

    let first = tokio::spawn(async move {
        use_capability(
            &first_broker,
            &first_capability,
            VaultOperation::Encrypt {
                plaintext: b"first plaintext".to_vec(),
                aad: b"aad".to_vec(),
            },
        )
        .await
    });
    let second = tokio::spawn(async move {
        use_capability(
            &second_broker,
            &second_capability,
            VaultOperation::Encrypt {
                plaintext: b"second plaintext".to_vec(),
                aad: b"aad".to_vec(),
            },
        )
        .await
    });

    let first_result = first.await.expect("first task joins");
    let second_result = second.await.expect("second task joins");
    let (first_ciphertext, first_nonce, _aad) = encrypted(first_result);
    let (second_ciphertext, second_nonce, _aad) = encrypted(second_result);

    assert_eq!(first_nonce.len(), 12);
    assert_eq!(second_nonce.len(), 12);
    assert_ne!(first_ciphertext, second_ciphertext);
}

#[tokio::test]
async fn issue_use_revoke_then_use_after_revoke_returns_capability_revoked() {
    let broker = InMemoryVaultBroker::new();
    let capability = issue(
        &broker,
        CapabilityClass::RandomGenerate,
        KeyAlgorithm::Aes256Gcm,
        None,
    )
    .await;

    let _ = use_capability(
        &broker,
        &capability,
        VaultOperation::RandomGenerate { byte_count: 8 },
    )
    .await;
    broker
        .revoke_capability(&capability.capability_id, &subject("family:operator"))
        .await
        .expect("revoke capability");

    let error = broker
        .use_capability(UseCapabilityRequest {
            capability_id: capability.capability_id.clone(),
            operation: VaultOperation::RandomGenerate { byte_count: 8 },
        })
        .await
        .expect_err("revoked capability must fail");

    assert_eq!(
        error,
        VaultError::CapabilityRevoked(capability.capability_id)
    );
}
