//! T-051 capability audit log and expiration lifecycle tests.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::{Duration, Utc};

use aios_vault::{
    CapabilityAuditLog, CapabilityClass, CapabilityLifecycleDriver, CapabilityState,
    InMemoryVaultBroker, IssueCapabilityRequest, KeyAlgorithm, SubjectRef, UseCapabilityRequest,
    VaultBroker, VaultError, VaultOperation,
};

const SECRET_BYTES: [u8; 32] = [0x51; 32];

fn subject(id: &str) -> SubjectRef {
    SubjectRef(id.to_owned())
}

fn issue_request(
    class: CapabilityClass,
    issued_to: SubjectRef,
    expires_at: Option<chrono::DateTime<Utc>>,
) -> IssueCapabilityRequest {
    IssueCapabilityRequest {
        class,
        issued_to,
        expires_at,
        key_algorithm: key_algorithm_for_class(class),
        key_material_bytes: Some(SECRET_BYTES.to_vec()),
    }
}

const fn key_algorithm_for_class(class: CapabilityClass) -> KeyAlgorithm {
    match class {
        CapabilityClass::KeySign
        | CapabilityClass::KeyVerify
        | CapabilityClass::BootstrapKeySign => KeyAlgorithm::Ed25519,
        CapabilityClass::MacGenerate | CapabilityClass::MacVerify => KeyAlgorithm::HmacSha256,
        CapabilityClass::KeyEncrypt
        | CapabilityClass::KeyDecrypt
        | CapabilityClass::RandomGenerate
        | CapabilityClass::SecretGet => KeyAlgorithm::Aes256Gcm,
    }
}

async fn issue(
    broker: &InMemoryVaultBroker,
    class: CapabilityClass,
    issued_to: SubjectRef,
    expires_at: Option<chrono::DateTime<Utc>>,
) -> aios_vault::VaultCapability {
    broker
        .issue_capability(issue_request(class, issued_to, expires_at))
        .await
        .expect("issue capability")
}

async fn use_mac_generate(
    broker: &InMemoryVaultBroker,
    capability_id: aios_vault::CapabilityId,
) -> Result<aios_vault::UseCapabilityResult, VaultError> {
    broker
        .use_capability(UseCapabilityRequest {
            capability_id,
            operation: VaultOperation::MacGenerate {
                message: b"payload".to_vec(),
            },
        })
        .await
}

#[tokio::test]
async fn issue_capability_with_audit_log_records_issue() {
    let audit = Arc::new(CapabilityAuditLog::new());
    let broker = InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit));
    let issued_to = subject("family:alice");

    let capability = issue(
        &broker,
        CapabilityClass::MacGenerate,
        issued_to.clone(),
        None,
    )
    .await;

    let entry = audit
        .lookup(&capability.capability_id)
        .expect("audit entry");
    assert_eq!(entry.capability_id, capability.capability_id);
    assert_eq!(entry.issued_by, issued_to);
    assert_eq!(entry.use_count, 0);
    assert_eq!(entry.last_used_at, None);
    assert_eq!(entry.last_used_op_kind, None);
}

#[tokio::test]
async fn use_capability_three_times_increments_audit_use_count() {
    let audit = Arc::new(CapabilityAuditLog::new());
    let broker = InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit));
    let capability = issue(
        &broker,
        CapabilityClass::MacGenerate,
        subject("family:alice"),
        None,
    )
    .await;

    for _ in 0..3 {
        use_mac_generate(&broker, capability.capability_id.clone())
            .await
            .expect("use capability");
    }

    let entry = audit
        .lookup(&capability.capability_id)
        .expect("audit entry");
    assert_eq!(entry.use_count, 3);
}

#[tokio::test]
async fn use_capability_records_last_used_timestamp_and_operation_kind() {
    let audit = Arc::new(CapabilityAuditLog::new());
    let broker = InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit));
    let capability = issue(
        &broker,
        CapabilityClass::MacGenerate,
        subject("family:alice"),
        None,
    )
    .await;

    use_mac_generate(&broker, capability.capability_id.clone())
        .await
        .expect("use capability");

    let entry = audit
        .lookup(&capability.capability_id)
        .expect("audit entry");
    assert!(entry.last_used_at.is_some());
    assert_eq!(entry.last_used_op_kind, Some("MAC_GENERATE".to_owned()));
}

#[tokio::test]
async fn revoke_capability_records_revoker_and_revoke_timestamp() {
    let audit = Arc::new(CapabilityAuditLog::new());
    let broker = InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit));
    let revoked_by = subject("family:operator");
    let capability = issue(
        &broker,
        CapabilityClass::MacGenerate,
        subject("family:alice"),
        None,
    )
    .await;

    broker
        .revoke_capability(&capability.capability_id, &revoked_by)
        .await
        .expect("revoke capability");

    let entry = audit
        .lookup(&capability.capability_id)
        .expect("audit entry");
    assert_eq!(entry.revoked_by, Some(revoked_by));
    assert!(entry.revoked_at.is_some());
}

#[tokio::test]
async fn run_expiration_pass_expires_past_active_capabilities_and_audits() {
    let audit = Arc::new(CapabilityAuditLog::new());
    let broker = InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit));
    let now = Utc::now();
    let capability = issue(
        &broker,
        CapabilityClass::MacGenerate,
        subject("family:alice"),
        Some(now - Duration::seconds(1)),
    )
    .await;
    let driver = CapabilityLifecycleDriver::new(Arc::new(broker.clone()), Arc::clone(&audit));

    let report = driver.run_expiration_pass(now).await.expect("run pass");

    assert_eq!(report.capabilities_inspected, 1);
    assert_eq!(report.capabilities_expired, 1);
    let listed = broker
        .list_capabilities(&subject("family:alice"))
        .await
        .expect("list capabilities");
    assert_eq!(listed[0].state, CapabilityState::Expired);
    let entry = audit
        .lookup(&capability.capability_id)
        .expect("audit entry");
    assert!(entry.expired_at.is_some());
}

#[tokio::test]
async fn run_expiration_pass_does_not_double_count_already_expired_capability() {
    let audit = Arc::new(CapabilityAuditLog::new());
    let broker = InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit));
    let now = Utc::now();
    let capability = issue(
        &broker,
        CapabilityClass::MacGenerate,
        subject("family:alice"),
        Some(now - Duration::seconds(1)),
    )
    .await;
    let driver = CapabilityLifecycleDriver::new(Arc::new(broker.clone()), Arc::clone(&audit));
    driver
        .expire_capability(&capability.capability_id)
        .await
        .expect("expire capability");

    let report = driver.run_expiration_pass(now).await.expect("run pass");

    assert_eq!(report.capabilities_inspected, 1);
    assert_eq!(report.capabilities_expired, 0);
}

#[tokio::test]
async fn run_expiration_pass_leaves_future_expiration_active() {
    let audit = Arc::new(CapabilityAuditLog::new());
    let broker = InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit));
    let now = Utc::now();
    issue(
        &broker,
        CapabilityClass::MacGenerate,
        subject("family:alice"),
        Some(now + Duration::seconds(60)),
    )
    .await;
    let driver = CapabilityLifecycleDriver::new(Arc::new(broker.clone()), Arc::clone(&audit));

    let report = driver.run_expiration_pass(now).await.expect("run pass");

    assert_eq!(report.capabilities_inspected, 1);
    assert_eq!(report.capabilities_expired, 0);
    let listed = broker
        .list_capabilities(&subject("family:alice"))
        .await
        .expect("list capabilities");
    assert_eq!(listed[0].state, CapabilityState::Active);
}

#[tokio::test]
async fn use_capability_lazily_expires_past_active_capability_and_audits() {
    let audit = Arc::new(CapabilityAuditLog::new());
    let broker = InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit));
    let capability = issue(
        &broker,
        CapabilityClass::MacGenerate,
        subject("family:alice"),
        Some(Utc::now() - Duration::seconds(1)),
    )
    .await;

    let error = use_mac_generate(&broker, capability.capability_id.clone())
        .await
        .expect_err("expired capability must fail");

    assert_eq!(
        error,
        VaultError::CapabilityExpired(capability.capability_id.clone())
    );
    let listed = broker
        .list_capabilities(&subject("family:alice"))
        .await
        .expect("list capabilities");
    assert_eq!(listed[0].state, CapabilityState::Expired);
    let entry = audit
        .lookup(&capability.capability_id)
        .expect("audit entry");
    assert!(entry.expired_at.is_some());
}

#[tokio::test]
async fn expire_capability_transitions_active_to_expired() {
    let audit = Arc::new(CapabilityAuditLog::new());
    let broker = InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit));
    let capability = issue(
        &broker,
        CapabilityClass::MacGenerate,
        subject("family:alice"),
        None,
    )
    .await;
    let driver = CapabilityLifecycleDriver::new(Arc::new(broker.clone()), Arc::clone(&audit));

    driver
        .expire_capability(&capability.capability_id)
        .await
        .expect("expire capability");

    let listed = broker
        .list_capabilities(&subject("family:alice"))
        .await
        .expect("list capabilities");
    assert_eq!(listed[0].state, CapabilityState::Expired);
    let entry = audit
        .lookup(&capability.capability_id)
        .expect("audit entry");
    assert!(entry.expired_at.is_some());
}

#[tokio::test]
async fn expire_capability_on_already_expired_returns_invalid_transition() {
    let audit = Arc::new(CapabilityAuditLog::new());
    let broker = InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit));
    let capability = issue(
        &broker,
        CapabilityClass::MacGenerate,
        subject("family:alice"),
        None,
    )
    .await;
    let driver = CapabilityLifecycleDriver::new(Arc::new(broker), audit);
    driver
        .expire_capability(&capability.capability_id)
        .await
        .expect("first expiration succeeds");

    let error = driver
        .expire_capability(&capability.capability_id)
        .await
        .expect_err("second expiration fails");

    assert_eq!(
        error,
        VaultError::InvalidTransition {
            from: CapabilityState::Expired,
            to: CapabilityState::Expired
        }
    );
}

#[tokio::test]
async fn audit_log_list_all_returns_all_entries() {
    let audit = Arc::new(CapabilityAuditLog::new());
    let broker = InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit));

    issue(
        &broker,
        CapabilityClass::MacGenerate,
        subject("family:alice"),
        None,
    )
    .await;
    issue(
        &broker,
        CapabilityClass::KeySign,
        subject("family:bob"),
        None,
    )
    .await;

    assert_eq!(audit.list_all().len(), 2);
}

#[tokio::test]
async fn broker_without_audit_log_preserves_existing_behavior() {
    let broker = InMemoryVaultBroker::new();
    let capability = issue(
        &broker,
        CapabilityClass::MacGenerate,
        subject("family:alice"),
        None,
    )
    .await;

    use_mac_generate(&broker, capability.capability_id.clone())
        .await
        .expect("use capability");
    broker
        .revoke_capability(&capability.capability_id, &subject("family:operator"))
        .await
        .expect("revoke capability");

    let listed = broker
        .list_capabilities(&subject("family:alice"))
        .await
        .expect("list capabilities");
    assert_eq!(listed[0].state, CapabilityState::Revoked);
}
