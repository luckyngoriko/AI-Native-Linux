//! T-054 cross-crate integration tests for `aios-vault` ↔ `aios-policy`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::{Duration, Utc};

use aios_action::{ActionEnvelope, ActionId, Identity, Request, Trace};
use aios_policy::override_boundary::{OverrideRequest, OverrideScope};
use aios_policy::{
    Decision, EnrichmentSnapshot, HardDenyEngine, HydratedSubject, InMemoryPolicyKernel,
    PolicyContext, PolicyError, PolicyKernel, SubjectHydrator,
};
use aios_vault::{
    GrantOverrideRequest, HydratedSubjectSnapshot, IdentityCatalog, InMemoryOverrideBroker,
    OverrideBinding, OverrideBindingState, OverrideBroker, OverrideClass, Subject, SubjectRef,
    SubjectType, VaultPolicyHydrator, VaultPolicyOverrideBoundary,
};

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
        groups: groups.iter().map(|g| (*g).to_owned()).collect(),
        is_ai,
        created_at: Utc::now(),
    }
}

fn placeholder_policy_subject() -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: "placeholder".to_owned(),
        subject_type: aios_policy::SubjectType::Human,
        groups: vec![],
        capabilities: vec![],
        session_class: "INTERNAL".to_owned(),
        recovery_mode: false,
        is_ai: false,
    }
}

fn policy_context(subject: HydratedSubject) -> PolicyContext {
    PolicyContext::new(
        subject,
        EnrichmentSnapshot {
            snapshot_id: "snap_t054".to_owned(),
            ..Default::default()
        },
        "polb_t054_v1",
        "code_t054",
    )
}

fn envelope(subject_canonical_id: &str, action: &str) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new(subject_canonical_id, false),
        Request::new(action, serde_json::json!({"service": "nginx"})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

async fn catalog_with(subject: Subject) -> Arc<IdentityCatalog> {
    let catalog = Arc::new(IdentityCatalog::new());
    catalog
        .register_subject(subject)
        .await
        .expect("register subject");
    catalog
}

#[test]
fn hydrated_subject_snapshot_converts_to_policy_subject_field_for_field() {
    let snapshot = HydratedSubjectSnapshot::from(subject(
        "agent:dev",
        SubjectType::Agent,
        true,
        &["agent", "cognitive-core"],
    ));

    let policy_subject = aios_policy::HydratedSubject::from(snapshot);

    assert_eq!(
        policy_subject,
        HydratedSubject {
            canonical_subject_id: "agent:dev".to_owned(),
            subject_type: aios_policy::SubjectType::Agent,
            groups: vec!["agent".to_owned(), "cognitive-core".to_owned()],
            capabilities: vec![],
            session_class: "SERVICE".to_owned(),
            recovery_mode: false,
            is_ai: true,
        }
    );
}

#[test]
fn local_operator_maps_to_policy_remote_operator_with_recovery_mode() {
    let snapshot = HydratedSubjectSnapshot::from(subject(
        "operator:root",
        SubjectType::LocalOperator,
        false,
        &["operator"],
    ));

    let policy_subject = aios_policy::HydratedSubject::from(snapshot);

    assert_eq!(
        policy_subject.subject_type,
        aios_policy::SubjectType::RemoteOperator
    );
    assert!(policy_subject.recovery_mode);
    assert_eq!(policy_subject.session_class, "RECOVERY");
}

#[test]
fn override_binding_converts_to_policy_emergency_override() {
    let action_id = ActionId::new();
    let granted_at = Utc::now();
    let expires_at = granted_at + Duration::hours(1);
    let binding = OverrideBinding {
        binding_id: "ovr_01HX0000000000000000000000".to_owned(),
        class: OverrideClass::DualHuman,
        granted_by: vec![
            SubjectRef("family:alice".to_owned()),
            SubjectRef("family:bob".to_owned()),
        ],
        granted_at,
        expires_at,
        target_action_id: Some(action_id.clone()),
        state: OverrideBindingState::Granted,
    };

    let converted = aios_policy::override_boundary::EmergencyOverride::from(binding);

    assert_eq!(converted.override_id, "ovr_01HX0000000000000000000000");
    assert_eq!(converted.granted_by_subject_id, "family:alice,family:bob");
    assert_eq!(converted.granted_at, granted_at);
    assert_eq!(converted.expires_at, expires_at);
    assert_eq!(converted.scope.rule_id, "");
    assert_eq!(converted.scope.action, action_id.as_str());
    assert!(converted.scope.subjects.is_empty());
    assert_eq!(converted.reason, "vault override binding class=DUAL_HUMAN");
    assert!(!converted.revoked);
}

#[test]
fn non_granted_override_binding_converts_to_revoked_policy_override() {
    let binding = OverrideBinding {
        binding_id: "ovr_01HX1111111111111111111111".to_owned(),
        class: OverrideClass::StrongSolo,
        granted_by: vec![SubjectRef("family:alice".to_owned())],
        granted_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(1),
        target_action_id: None,
        state: OverrideBindingState::Consumed,
    };

    let converted = aios_policy::override_boundary::EmergencyOverride::from(binding);

    assert!(converted.revoked);
    assert_eq!(converted.scope.action, "");
}

#[tokio::test]
async fn vault_policy_hydrator_returns_policy_subject_with_all_fields_populated() {
    let catalog = catalog_with(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["operators", "family"],
    ))
    .await;
    let hydrator = VaultPolicyHydrator::new(catalog);

    let hydrated = hydrator
        .hydrate("family:alice")
        .await
        .expect("vault subject must hydrate");

    assert_eq!(
        hydrated,
        HydratedSubject {
            canonical_subject_id: "family:alice".to_owned(),
            subject_type: aios_policy::SubjectType::Human,
            groups: vec!["family".to_owned(), "operators".to_owned()],
            capabilities: vec![],
            session_class: "INTERACTIVE".to_owned(),
            recovery_mode: false,
            is_ai: false,
        }
    );
}

#[tokio::test]
async fn vault_policy_hydrator_unknown_subject_returns_subject_unauthenticated() {
    let hydrator = VaultPolicyHydrator::new(Arc::new(IdentityCatalog::new()));

    let err = hydrator
        .hydrate("family:ghost")
        .await
        .expect_err("unknown subject must fail closed");

    assert_eq!(err, PolicyError::SubjectUnauthenticated);
}

#[tokio::test]
async fn vault_policy_hydrator_expired_session_returns_subject_unauthenticated() {
    let catalog = catalog_with(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["family"],
    ))
    .await;
    let session = catalog
        .start_session("family:alice", Utc::now() - Duration::seconds(1))
        .await
        .expect("start expired session fixture");
    let hydrator = VaultPolicyHydrator::new(catalog);

    let err = hydrator
        .hydrate(&session.session_id)
        .await
        .expect_err("expired session must fail closed");

    assert_eq!(err, PolicyError::SubjectUnauthenticated);
}

#[tokio::test]
async fn kernel_full_chain_uses_vault_hydrated_subject_on_allow_path() {
    let catalog = catalog_with(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["family"],
    ))
    .await;
    let hydrator: Arc<dyn SubjectHydrator + Send + Sync> =
        Arc::new(VaultPolicyHydrator::new(catalog));
    let boundary = Arc::new(aios_policy::OverrideBoundary::new());
    let grant = boundary
        .request_override(OverrideRequest {
            granted_by_subject: HydratedSubject {
                canonical_subject_id: "family:alice".to_owned(),
                subject_type: aios_policy::SubjectType::Human,
                groups: vec!["family".to_owned()],
                capabilities: vec![],
                session_class: "INTERACTIVE".to_owned(),
                recovery_mode: false,
                is_ai: false,
            },
            scope: OverrideScope {
                rule_id: "deny_service_restart".to_owned(),
                action: "service.restart".to_owned(),
                subjects: vec!["family:alice".to_owned()],
            },
            reason: "t054 vault hydration allow path".to_owned(),
            ttl_seconds: 3600,
            attempted_hard_deny: None,
        })
        .expect("grant override");
    let kernel =
        InMemoryPolicyKernel::new_with_full_chain(hydrator, HardDenyEngine::new_with_defaults())
            .with_override_boundary(boundary);
    let ctx = policy_context(placeholder_policy_subject());
    let env = envelope("family:alice", "service.restart");

    let decision = kernel
        .evaluate_policy(&env, &ctx)
        .await
        .expect("policy evaluation");

    assert_eq!(decision.decision, Decision::Allow);
    assert_eq!(
        decision.reason_code,
        aios_policy::reason_code::EMERGENCY_OVERRIDE_RELAXED
    );
    assert!(
        decision.reason_message.contains(&grant.override_id),
        "allow path must reference the active override id"
    );
}

#[tokio::test]
async fn vault_policy_override_boundary_returns_active_converted_override() {
    let catalog = catalog_with(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["family"],
    ))
    .await;
    let broker = Arc::new(InMemoryOverrideBroker::new(catalog));
    let action_id = ActionId::new();
    let binding = broker
        .grant_override(GrantOverrideRequest {
            class: OverrideClass::StrongSolo,
            granted_by: vec![SubjectRef("family:alice".to_owned())],
            target_action_id: Some(action_id.clone()),
            expires_at: Utc::now() + Duration::hours(1),
            reason: "incident response".to_owned(),
        })
        .await
        .expect("grant vault override");
    let boundary = VaultPolicyOverrideBoundary::new(broker);

    let converted = boundary
        .is_overridden(action_id.as_str(), "family:bob")
        .await
        .expect("active override must match");

    assert_eq!(converted.override_id, binding.binding_id);
    assert_eq!(converted.scope.action, action_id.as_str());
    assert_eq!(converted.granted_by_subject_id, "family:alice");
    assert!(!converted.revoked);
}

#[tokio::test]
async fn vault_policy_override_boundary_returns_none_for_missing_override() {
    let catalog = catalog_with(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["family"],
    ))
    .await;
    let broker = Arc::new(InMemoryOverrideBroker::new(catalog));
    let boundary = VaultPolicyOverrideBoundary::new(broker);

    assert!(boundary
        .is_overridden(ActionId::new().as_str(), "family:alice")
        .await
        .is_none());
}

#[tokio::test]
async fn vault_policy_override_boundary_returns_none_for_expired_override() {
    let catalog = catalog_with(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["family"],
    ))
    .await;
    let broker = Arc::new(InMemoryOverrideBroker::new(catalog));
    let action_id = ActionId::new();
    broker
        .grant_override(GrantOverrideRequest {
            class: OverrideClass::StrongSolo,
            granted_by: vec![SubjectRef("family:alice".to_owned())],
            target_action_id: Some(action_id.clone()),
            expires_at: Utc::now() - Duration::seconds(1),
            reason: "expired".to_owned(),
        })
        .await
        .expect("grant expired vault override fixture");
    let boundary = VaultPolicyOverrideBoundary::new(broker);

    assert!(boundary
        .is_overridden(action_id.as_str(), "family:alice")
        .await
        .is_none());
}
