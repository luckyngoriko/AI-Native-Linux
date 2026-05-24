//! T-052 integration tests for the `aios.vault.v1alpha1.VaultBroker` gRPC surface.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::items_after_statements,
    clippy::result_large_err,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::net::SocketAddr;
use std::sync::Arc;

use chrono::{Duration, Utc};
use prost::Message;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tonic::{Code, Request};

use aios_vault::service::proto::vault_broker_server::VaultBroker;
use aios_vault::service::proto::{
    self, CapabilityState, EncryptRequest, GrantOverrideRequest, IssueCapabilityRequest,
    KeyAlgorithm, ListCapabilitiesRequest, LookupSubjectRequest, OverrideClass,
    RegisterSubjectRequest, RevokeCapabilityRequest, RunExpirationPassRequest, SessionState,
    StartSessionRequest, SubjectProto, SubjectType, UseCapabilityRequest, VaultCapabilityClass,
    VaultOperation,
};
use aios_vault::service::{VaultBrokerClient, VaultBrokerGrpcServer, VaultBrokerService};
use aios_vault::{
    CapabilityAuditLog, CapabilityLifecycleDriver, IdentityCatalog, InMemoryOverrideBroker,
    InMemoryVaultBroker,
};

const KNOWN_AES_KEY: &[u8; 32] = b"T052-wire-leak-key-material!!!!!";

fn timestamp(offset_seconds: i64) -> prost_types::Timestamp {
    let value = Utc::now() + Duration::seconds(offset_seconds);
    prost_types::Timestamp {
        seconds: value.timestamp(),
        nanos: i32::try_from(value.timestamp_subsec_nanos()).unwrap_or(0),
    }
}

fn make_service() -> VaultBrokerService {
    let audit = Arc::new(CapabilityAuditLog::new());
    let vault = Arc::new(InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit)));
    let identity = Arc::new(IdentityCatalog::with_fixtures());
    let overrides = Arc::new(InMemoryOverrideBroker::new(Arc::clone(&identity)));
    let lifecycle = Arc::new(CapabilityLifecycleDriver::new(
        Arc::clone(&vault),
        Arc::clone(&audit),
    ));

    VaultBrokerService::new(vault, overrides, identity, audit, lifecycle)
}

fn issue_request(subject: &str, class: VaultCapabilityClass) -> IssueCapabilityRequest {
    IssueCapabilityRequest {
        class: i32::from(class),
        issued_to: subject.to_owned(),
        expires_at: Some(timestamp(3600)),
        key_algorithm: i32::from(KeyAlgorithm::Aes256Gcm),
        key_material_bytes: Some(KNOWN_AES_KEY.to_vec()),
    }
}

async fn issue_encrypt_capability(svc: &VaultBrokerService, subject: &str) -> String {
    svc.issue_capability(Request::new(issue_request(
        subject,
        VaultCapabilityClass::KeyEncrypt,
    )))
    .await
    .expect("issue capability")
    .into_inner()
    .capability
    .expect("capability")
    .capability_id
}

fn encrypt_operation(plaintext: &[u8]) -> VaultOperation {
    VaultOperation {
        operation: Some(proto::vault_operation::Operation::Enc(EncryptRequest {
            plaintext: plaintext.to_vec(),
            aad: b"grpc-aad".to_vec(),
        })),
    }
}

async fn pick_port() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind random port");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);
    addr
}

async fn spawn_server(
    svc: VaultBrokerService,
) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let addr = pick_port().await;
    let (tx, rx) = oneshot::channel::<()>();
    let server = tonic::transport::Server::builder().add_service(VaultBrokerGrpcServer::new(svc));
    let handle = tokio::spawn(async move {
        server
            .serve_with_shutdown(addr, async move {
                let _ = rx.await;
            })
            .await
            .expect("server task");
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (addr, tx, handle)
}

#[tokio::test]
async fn issue_capability_happy_path_returns_capability_proto() {
    let svc = make_service();

    let response = svc
        .issue_capability(Request::new(issue_request(
            "family:alice",
            VaultCapabilityClass::KeyEncrypt,
        )))
        .await
        .expect("issue ok")
        .into_inner();

    let capability = response.capability.expect("capability");
    assert!(capability.capability_id.starts_with("cap_"));
    assert_eq!(capability.issued_to, "family:alice");
    assert_eq!(
        capability.class,
        i32::from(VaultCapabilityClass::KeyEncrypt)
    );
    assert_eq!(capability.state, i32::from(CapabilityState::Active));
    assert!(capability
        .key_material_handle
        .starts_with("vault-internal:cap_"));
}

#[tokio::test]
async fn issue_capability_response_never_contains_imported_key_material() {
    let svc = make_service();

    let response = svc
        .issue_capability(Request::new(issue_request(
            "family:alice",
            VaultCapabilityClass::KeyEncrypt,
        )))
        .await
        .expect("issue ok")
        .into_inner();

    let wire = response.encode_to_vec();
    let printable = String::from_utf8_lossy(&wire);
    assert!(!wire
        .windows(KNOWN_AES_KEY.len())
        .any(|w| w == KNOWN_AES_KEY));
    assert!(!printable.contains("T052-wire-leak-key-material"));
}

#[tokio::test]
async fn use_capability_encrypt_returns_ciphertext_and_nonce() {
    let svc = make_service();
    let capability_id = issue_encrypt_capability(&svc, "family:alice").await;

    let response = svc
        .use_capability(Request::new(UseCapabilityRequest {
            capability_id,
            operation: Some(encrypt_operation(b"hello vault")),
        }))
        .await
        .expect("use ok")
        .into_inner();

    let result = response
        .result
        .expect("result")
        .result
        .expect("oneof result");
    match result {
        proto::use_capability_result::Result::Encrypted(encrypted) => {
            assert!(!encrypted.ciphertext.is_empty());
            assert_eq!(encrypted.nonce.len(), 12);
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[tokio::test]
async fn use_capability_with_class_mismatch_maps_to_invalid_argument() {
    let svc = make_service();
    let capability_id = issue_encrypt_capability(&svc, "family:alice").await;

    let err = svc
        .use_capability(Request::new(UseCapabilityRequest {
            capability_id,
            operation: Some(VaultOperation {
                operation: Some(proto::vault_operation::Operation::Sign(
                    proto::SignRequest {
                        message: b"not allowed".to_vec(),
                    },
                )),
            }),
        }))
        .await
        .expect_err("class mismatch");

    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn revoke_capability_blocks_subsequent_use() {
    let svc = make_service();
    let capability_id = issue_encrypt_capability(&svc, "family:alice").await;

    svc.revoke_capability(Request::new(RevokeCapabilityRequest {
        capability_id: capability_id.clone(),
        revoked_by: "family:alice".to_owned(),
    }))
    .await
    .expect("revoke ok");

    let err = svc
        .use_capability(Request::new(UseCapabilityRequest {
            capability_id,
            operation: Some(encrypt_operation(b"after revoke")),
        }))
        .await
        .expect_err("revoked use");

    assert_eq!(err.code(), Code::FailedPrecondition);
}

#[tokio::test]
async fn list_capabilities_filters_by_subject() {
    let svc = make_service();
    let alice_capability_id = issue_encrypt_capability(&svc, "family:alice").await;
    let _agent_capability_id = issue_encrypt_capability(&svc, "agent:dev").await;

    let response = svc
        .list_capabilities(Request::new(ListCapabilitiesRequest {
            subject: "family:alice".to_owned(),
        }))
        .await
        .expect("list ok")
        .into_inner();

    let ids = response
        .capabilities
        .into_iter()
        .map(|capability| capability.capability_id)
        .collect::<Vec<_>>();
    assert_eq!(ids, vec![alice_capability_id]);
}

#[tokio::test]
async fn grant_override_strong_solo_returns_binding_proto() {
    let svc = make_service();

    let response = svc
        .grant_override(Request::new(GrantOverrideRequest {
            class: i32::from(OverrideClass::StrongSolo),
            granted_by: vec!["family:alice".to_owned()],
            target_action_id_proto: None,
            expires_at: Some(timestamp(300)),
            reason: "recovery drill".to_owned(),
        }))
        .await
        .expect("grant ok")
        .into_inner();

    let binding = response.binding.expect("binding");
    assert!(binding.binding_id.starts_with("ovr_"));
    assert_eq!(binding.granted_by, vec!["family:alice"]);
    assert_eq!(binding.class, i32::from(OverrideClass::StrongSolo));
}

#[tokio::test]
async fn grant_override_with_ai_subject_maps_to_permission_denied() {
    let svc = make_service();

    let err = svc
        .grant_override(Request::new(GrantOverrideRequest {
            class: i32::from(OverrideClass::StrongSolo),
            granted_by: vec!["agent:dev".to_owned()],
            target_action_id_proto: None,
            expires_at: Some(timestamp(300)),
            reason: "ai must not grant".to_owned(),
        }))
        .await
        .expect_err("ai denied");

    assert_eq!(err.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn consume_override_is_atomic_and_replay_fails() {
    let svc = make_service();
    let binding_id = svc
        .grant_override(Request::new(GrantOverrideRequest {
            class: i32::from(OverrideClass::StrongSolo),
            granted_by: vec!["family:alice".to_owned()],
            target_action_id_proto: None,
            expires_at: Some(timestamp(300)),
            reason: "single consume".to_owned(),
        }))
        .await
        .expect("grant ok")
        .into_inner()
        .binding
        .expect("binding")
        .binding_id;

    svc.consume_override(Request::new(proto::ConsumeOverrideRequest {
        binding_id: binding_id.clone(),
        consumer: "agent:dev".to_owned(),
    }))
    .await
    .expect("first consume ok");

    let err = svc
        .consume_override(Request::new(proto::ConsumeOverrideRequest {
            binding_id,
            consumer: "agent:dev".to_owned(),
        }))
        .await
        .expect_err("second consume denied");

    assert_eq!(err.code(), Code::FailedPrecondition);
}

#[tokio::test]
async fn register_subject_and_lookup_round_trip() {
    let svc = make_service();
    let subject = SubjectProto {
        canonical_subject_id: "family:bob-t052".to_owned(),
        subject_type: i32::from(SubjectType::HumanUser),
        provisional_name: "Bob T052".to_owned(),
        groups: vec!["family".to_owned()],
        is_ai: false,
        created_at: Some(timestamp(0)),
    };

    svc.register_subject(Request::new(RegisterSubjectRequest {
        subject: Some(subject.clone()),
    }))
    .await
    .expect("register ok");

    let looked_up = svc
        .lookup_subject(Request::new(LookupSubjectRequest {
            canonical_subject_id: subject.canonical_subject_id.clone(),
        }))
        .await
        .expect("lookup ok")
        .into_inner()
        .subject
        .expect("subject");

    assert_eq!(looked_up, subject);
}

#[tokio::test]
async fn start_session_and_lookup_round_trip() {
    let svc = make_service();

    let session = svc
        .start_session(Request::new(StartSessionRequest {
            subject_id: "family:alice".to_owned(),
            expires_at: Some(timestamp(600)),
        }))
        .await
        .expect("session start")
        .into_inner()
        .session
        .expect("session");

    let looked_up = svc
        .lookup_session(Request::new(proto::LookupSessionRequest {
            session_id: session.session_id.clone(),
        }))
        .await
        .expect("session lookup")
        .into_inner()
        .session
        .expect("session");

    assert_eq!(looked_up.session_id, session.session_id);
    assert_eq!(looked_up.subject_id, "family:alice");
    assert_eq!(looked_up.state, i32::from(SessionState::SessionActive));
}

#[tokio::test]
async fn get_audit_entry_on_issued_capability_starts_with_zero_uses() {
    let svc = make_service();
    let capability_id = issue_encrypt_capability(&svc, "family:alice").await;

    let entry = svc
        .get_audit_entry(Request::new(proto::GetAuditEntryRequest {
            capability_id: capability_id.clone(),
        }))
        .await
        .expect("audit lookup")
        .into_inner()
        .entry
        .expect("entry");

    assert_eq!(entry.capability_id, capability_id);
    assert_eq!(entry.use_count, 0);
    assert_eq!(entry.issued_by, "family:alice");
}

#[tokio::test]
async fn run_expiration_pass_transitions_past_expiry_capability() {
    let svc = make_service();
    let response = svc
        .issue_capability(Request::new(IssueCapabilityRequest {
            class: i32::from(VaultCapabilityClass::KeyEncrypt),
            issued_to: "family:alice".to_owned(),
            expires_at: Some(timestamp(-30)),
            key_algorithm: i32::from(KeyAlgorithm::Aes256Gcm),
            key_material_bytes: Some(KNOWN_AES_KEY.to_vec()),
        }))
        .await
        .expect("issue expired")
        .into_inner();
    let capability_id = response.capability.expect("capability").capability_id;

    let report = svc
        .run_expiration_pass(Request::new(RunExpirationPassRequest {
            now: Some(timestamp(0)),
        }))
        .await
        .expect("expiration pass")
        .into_inner()
        .report
        .expect("report");

    assert!(report.pass_id.starts_with("expp_"));
    assert_eq!(report.capabilities_expired, 1);

    let listed = svc
        .list_capabilities(Request::new(ListCapabilitiesRequest {
            subject: "family:alice".to_owned(),
        }))
        .await
        .expect("list")
        .into_inner();
    let capability = listed
        .capabilities
        .into_iter()
        .find(|capability| capability.capability_id == capability_id)
        .expect("listed capability");
    assert_eq!(capability.state, i32::from(CapabilityState::Expired));
}

#[tokio::test]
async fn inv_018_list_capabilities_wire_format_does_not_contain_key_bytes() {
    let svc = make_service();
    let _capability_id = issue_encrypt_capability(&svc, "family:alice").await;

    let response = svc
        .list_capabilities(Request::new(ListCapabilitiesRequest {
            subject: "family:alice".to_owned(),
        }))
        .await
        .expect("list ok")
        .into_inner();

    let wire = response.encode_to_vec();
    let printable = String::from_utf8_lossy(&wire);
    assert!(!wire
        .windows(KNOWN_AES_KEY.len())
        .any(|w| w == KNOWN_AES_KEY));
    assert!(!printable.contains("T052-wire-leak-key-material"));
}

#[tokio::test]
async fn tonic_in_process_channel_smoke_test() {
    let svc = make_service();
    let (addr, shutdown, handle) = spawn_server(svc).await;
    let mut client = VaultBrokerClient::connect(format!("http://{addr}"))
        .await
        .expect("client connect");

    let issued = client
        .issue_capability(issue_request(
            "family:alice",
            VaultCapabilityClass::KeyEncrypt,
        ))
        .await
        .expect("client issue")
        .into_inner()
        .capability
        .expect("capability");
    let used = client
        .use_capability(UseCapabilityRequest {
            capability_id: issued.capability_id,
            operation: Some(encrypt_operation(b"through client")),
        })
        .await
        .expect("client use")
        .into_inner();
    drop(client);

    assert!(matches!(
        used.result.expect("result").result.expect("oneof result"),
        proto::use_capability_result::Result::Encrypted(_)
    ));

    let _ = shutdown.send(());
    let _ = handle.await;
}
