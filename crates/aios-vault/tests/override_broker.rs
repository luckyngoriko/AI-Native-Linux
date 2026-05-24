//! T-050 integration tests for the emergency override broker.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};

use aios_action::ActionId;
use aios_vault::{
    GrantOverrideRequest, IdentityCatalog, InMemoryOverrideBroker, OverrideBindingState,
    OverrideBroker, OverrideClass, Subject, SubjectRef, SubjectType, VaultError,
};

#[tokio::test]
async fn strong_solo_grant_with_one_human_approver_succeeds() {
    let broker = broker_with_standard_subjects().await;

    let binding = broker
        .grant_override(request(
            OverrideClass::StrongSolo,
            &["family:alice"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("grant override");

    assert!(binding.binding_id.starts_with("ovr_"));
    assert_eq!(binding.class, OverrideClass::StrongSolo);
    assert_eq!(binding.granted_by, vec![subject_ref("family:alice")]);
    assert_eq!(binding.state, OverrideBindingState::Granted);
}

#[tokio::test]
async fn strong_solo_grant_with_two_approvers_returns_count_mismatch() {
    let broker = broker_with_standard_subjects().await;

    let error = broker
        .grant_override(request(
            OverrideClass::StrongSolo,
            &["family:alice", "family:bob"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect_err("two approvers must fail");

    assert_eq!(
        error,
        VaultError::OverrideClassApproverCountMismatch {
            class: OverrideClass::StrongSolo,
            expected: 1,
            found: 2,
        }
    );
}

#[tokio::test]
async fn dual_human_grant_with_two_human_approvers_succeeds() {
    let broker = broker_with_standard_subjects().await;

    let binding = broker
        .grant_override(request(
            OverrideClass::DualHuman,
            &["family:alice", "family:bob"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("grant override");

    assert_eq!(binding.class, OverrideClass::DualHuman);
    assert_eq!(
        binding.granted_by,
        vec![subject_ref("family:alice"), subject_ref("family:bob")]
    );
    assert_eq!(binding.state, OverrideBindingState::Granted);
}

#[tokio::test]
async fn dual_human_grant_with_one_approver_returns_count_mismatch() {
    let broker = broker_with_standard_subjects().await;

    let error = broker
        .grant_override(request(
            OverrideClass::DualHuman,
            &["family:alice"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect_err("single approver must fail");

    assert_eq!(
        error,
        VaultError::OverrideClassApproverCountMismatch {
            class: OverrideClass::DualHuman,
            expected: 2,
            found: 1,
        }
    );
}

#[tokio::test]
async fn dual_human_grant_with_non_human_service_returns_human_approver_error() {
    let broker = broker_with_standard_subjects().await;

    let error = broker
        .grant_override(request(
            OverrideClass::DualHuman,
            &["family:alice", "_system:daemon"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect_err("service approver must fail");

    assert_eq!(
        error,
        VaultError::OverrideRequiresHumanApprovers {
            class: OverrideClass::DualHuman,
            found_non_human: vec!["_system:daemon".to_owned()],
        }
    );
}

#[tokio::test]
async fn triple_human_grant_with_three_humans_succeeds() {
    let broker = broker_with_standard_subjects().await;

    let binding = broker
        .grant_override(request(
            OverrideClass::TripleHuman,
            &["family:alice", "family:bob", "family:carol"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("grant override");

    assert_eq!(binding.class, OverrideClass::TripleHuman);
    assert_eq!(binding.granted_by.len(), 3);
    assert_eq!(binding.state, OverrideBindingState::Granted);
}

#[tokio::test]
async fn ai_defense_rejects_agent_type_approver() {
    let broker = broker_with_standard_subjects().await;

    let error = broker
        .grant_override(request(
            OverrideClass::StrongSolo,
            &["agent:dev"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect_err("agent approver must fail");

    assert_eq!(
        error,
        VaultError::AiCannotGrantOverride("agent:dev".to_owned())
    );
}

#[tokio::test]
async fn ai_defense_rejects_application_type_approver() {
    let broker = broker_with_standard_subjects().await;

    let error = broker
        .grant_override(request(
            OverrideClass::StrongSolo,
            &["app:browser"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect_err("application approver must fail");

    assert_eq!(
        error,
        VaultError::AiCannotGrantOverride("app:browser".to_owned())
    );
}

#[tokio::test]
async fn grant_with_unknown_subject_returns_subject_not_found() {
    let broker = broker_with_standard_subjects().await;

    let error = broker
        .grant_override(request(
            OverrideClass::StrongSolo,
            &["family:missing"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect_err("unknown subject must fail");

    assert_eq!(
        error,
        VaultError::SubjectNotFound("family:missing".to_owned())
    );
}

#[tokio::test]
async fn lookup_override_returns_granted_state_for_live_binding() {
    let broker = broker_with_standard_subjects().await;
    let binding = broker
        .grant_override(request(
            OverrideClass::StrongSolo,
            &["family:alice"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("grant override");

    let looked_up = broker
        .lookup_override(&binding.binding_id)
        .await
        .expect("lookup override");

    assert_eq!(looked_up.state, OverrideBindingState::Granted);
}

#[tokio::test]
async fn consume_override_transitions_granted_to_consumed() {
    let broker = broker_with_standard_subjects().await;
    let binding = broker
        .grant_override(request(
            OverrideClass::StrongSolo,
            &["family:alice"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("grant override");

    let consumed = broker
        .consume_override(&binding.binding_id, &subject_ref("family:alice"))
        .await
        .expect("consume override");

    assert_eq!(consumed.state, OverrideBindingState::Consumed);
    let looked_up = broker
        .lookup_override(&binding.binding_id)
        .await
        .expect("lookup override");
    assert_eq!(looked_up.state, OverrideBindingState::Consumed);
}

#[tokio::test]
async fn second_consume_returns_override_already_consumed() {
    let broker = broker_with_standard_subjects().await;
    let binding = broker
        .grant_override(request(
            OverrideClass::StrongSolo,
            &["family:alice"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("grant override");

    broker
        .consume_override(&binding.binding_id, &subject_ref("family:alice"))
        .await
        .expect("first consume");
    let error = broker
        .consume_override(&binding.binding_id, &subject_ref("family:alice"))
        .await
        .expect_err("second consume must fail");

    assert_eq!(error, VaultError::OverrideAlreadyConsumed);
}

#[tokio::test]
async fn parallel_consume_override_has_exactly_one_winner() {
    let broker = Arc::new(broker_with_standard_subjects().await);
    let binding = broker
        .grant_override(request(
            OverrideClass::StrongSolo,
            &["family:alice"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("grant override");

    let mut handles = Vec::new();
    for _ in 0..10 {
        let broker = Arc::clone(&broker);
        let binding_id = binding.binding_id.clone();
        handles.push(tokio::spawn(async move {
            broker
                .consume_override(&binding_id, &subject_ref("family:alice"))
                .await
        }));
    }

    let mut winners = 0_u32;
    let mut replay_failures = 0_u32;
    for handle in handles {
        match handle.await.expect("join consume task") {
            Ok(consumed) => {
                assert_eq!(consumed.state, OverrideBindingState::Consumed);
                winners += 1;
            }
            Err(VaultError::OverrideAlreadyConsumed) => {
                replay_failures += 1;
            }
            Err(error) => panic!("unexpected consume error: {error:?}"),
        }
    }

    assert_eq!(winners, 1);
    assert_eq!(replay_failures, 9);
}

#[tokio::test]
async fn revoke_override_transitions_any_state_to_revoked_and_blocks_consume() {
    let broker = broker_with_standard_subjects().await;
    let binding = broker
        .grant_override(request(
            OverrideClass::StrongSolo,
            &["family:alice"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("grant override");
    broker
        .consume_override(&binding.binding_id, &subject_ref("family:alice"))
        .await
        .expect("consume first");

    broker
        .revoke_override(&binding.binding_id, &subject_ref("family:bob"))
        .await
        .expect("revoke override");
    let looked_up = broker
        .lookup_override(&binding.binding_id)
        .await
        .expect("lookup revoked");
    assert_eq!(looked_up.state, OverrideBindingState::Revoked);

    let result = broker
        .consume_override(&binding.binding_id, &subject_ref("family:alice"))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn past_expires_at_lookup_marks_expired_and_consume_returns_override_expired() {
    let broker = broker_with_standard_subjects().await;
    let binding = broker
        .grant_override(request(
            OverrideClass::StrongSolo,
            &["family:alice"],
            Utc::now() - Duration::seconds(1),
        ))
        .await
        .expect("grant expired override");

    let looked_up = broker
        .lookup_override(&binding.binding_id)
        .await
        .expect("lookup override");
    assert_eq!(looked_up.state, OverrideBindingState::Expired);

    let error = broker
        .consume_override(&binding.binding_id, &subject_ref("family:alice"))
        .await
        .expect_err("expired consume must fail");
    assert_eq!(error, VaultError::OverrideExpired(binding.binding_id));
}

#[tokio::test]
async fn list_overrides_for_subject_returns_matching_grants() {
    let broker = broker_with_standard_subjects().await;
    let alice_binding = broker
        .grant_override(request(
            OverrideClass::StrongSolo,
            &["family:alice"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("grant alice override");
    broker
        .grant_override(request(
            OverrideClass::StrongSolo,
            &["family:bob"],
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("grant bob override");

    let bindings = broker
        .list_overrides_for_subject(&subject_ref("family:alice"))
        .await
        .expect("list overrides");

    assert_eq!(bindings, vec![alice_binding]);
}

async fn broker_with_standard_subjects() -> InMemoryOverrideBroker {
    let catalog = Arc::new(IdentityCatalog::new());
    for subject in [
        subject("family:alice", SubjectType::Human, false, &["family"]),
        subject("family:bob", SubjectType::Human, false, &["family"]),
        subject("family:carol", SubjectType::Human, false, &["family"]),
        subject("agent:dev", SubjectType::Agent, true, &["agent"]),
        subject("app:browser", SubjectType::Application, false, &["app"]),
        subject("_system:daemon", SubjectType::Service, false, &["_system"]),
    ] {
        catalog
            .register_subject(subject)
            .await
            .expect("register fixture subject");
    }
    InMemoryOverrideBroker::new(catalog)
}

fn request(
    class: OverrideClass,
    granted_by: &[&str],
    expires_at: chrono::DateTime<Utc>,
) -> GrantOverrideRequest {
    GrantOverrideRequest {
        class,
        granted_by: granted_by
            .iter()
            .map(|subject| subject_ref(subject))
            .collect(),
        target_action_id: Some(ActionId::new()),
        expires_at,
        reason: "operator documented emergency override reason".to_owned(),
    }
}

fn subject_ref(canonical_subject_id: &str) -> SubjectRef {
    SubjectRef(canonical_subject_id.to_owned())
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
        created_at: Utc
            .with_ymd_and_hms(2026, 5, 24, 12, 0, 0)
            .single()
            .expect("fixture timestamp is valid"),
    }
}
