//! T-048 integration tests for the identity catalog and vault subject hydrator.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};

use aios_vault::{
    HydratedSubjectSnapshot, IdentityCatalog, SessionState, Subject, SubjectType, VaultError,
    VaultSubjectHydrator,
};

#[tokio::test]
async fn register_subject_and_lookup_subject_round_trip() {
    let catalog = IdentityCatalog::new();
    let subject = subject("family:alice", SubjectType::Human, false, &["family"]);

    catalog
        .register_subject(subject.clone())
        .await
        .expect("register subject");

    let looked_up = catalog
        .lookup_subject("family:alice")
        .await
        .expect("lookup subject");
    assert_eq!(looked_up, subject);
}

#[tokio::test]
async fn duplicate_canonical_id_returns_subject_already_registered() {
    let catalog = IdentityCatalog::new();
    let subject = subject("family:alice", SubjectType::Human, false, &["family"]);

    catalog
        .register_subject(subject.clone())
        .await
        .expect("first registration");
    let error = catalog
        .register_subject(subject)
        .await
        .expect_err("duplicate registration must fail");

    assert_eq!(
        error,
        VaultError::SubjectAlreadyRegistered("family:alice".to_owned())
    );
}

#[tokio::test]
async fn lookup_subject_unknown_returns_subject_not_found() {
    let catalog = IdentityCatalog::new();

    let error = catalog
        .lookup_subject("family:missing")
        .await
        .expect_err("unknown subject must fail");

    assert_eq!(
        error,
        VaultError::SubjectNotFound("family:missing".to_owned())
    );
}

#[tokio::test]
async fn start_session_and_lookup_session_round_trip() {
    let catalog = catalog_with_subject(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["family"],
    ))
    .await;

    let session = catalog
        .start_session("family:alice", Utc::now() + Duration::hours(1))
        .await
        .expect("start session");
    let looked_up = catalog
        .lookup_session(&session.session_id)
        .await
        .expect("lookup session");

    assert_eq!(looked_up, session);
    assert_eq!(looked_up.state, SessionState::Active);
}

#[tokio::test]
async fn start_session_rejects_an_already_active_subject_session() {
    let catalog = catalog_with_subject(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["family"],
    ))
    .await;

    catalog
        .start_session("family:alice", Utc::now() + Duration::hours(1))
        .await
        .expect("start first session");
    let error = catalog
        .start_session("family:alice", Utc::now() + Duration::hours(1))
        .await
        .expect_err("second active session must fail");

    assert_eq!(
        error,
        VaultError::SessionAlreadyActive("family:alice".to_owned())
    );
}

#[tokio::test]
async fn lookup_session_marks_past_session_expired() {
    let catalog = catalog_with_subject(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["family"],
    ))
    .await;

    let session = catalog
        .start_session("family:alice", Utc::now() - Duration::seconds(1))
        .await
        .expect("start past session");
    let looked_up = catalog
        .lookup_session(&session.session_id)
        .await
        .expect("lookup session");

    assert_eq!(looked_up.state, SessionState::Expired);
}

#[tokio::test]
async fn suspend_session_transitions_active_to_suspended() {
    let catalog = catalog_with_subject(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["family"],
    ))
    .await;
    let session = catalog
        .start_session("family:alice", Utc::now() + Duration::hours(1))
        .await
        .expect("start session");

    catalog
        .suspend_session(&session.session_id)
        .await
        .expect("suspend session");
    let looked_up = catalog
        .lookup_session(&session.session_id)
        .await
        .expect("lookup suspended session");

    assert_eq!(looked_up.state, SessionState::Suspended);
}

#[tokio::test]
async fn revoke_session_transitions_any_session_to_revoked() {
    let catalog = catalog_with_subject(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["family"],
    ))
    .await;
    let session = catalog
        .start_session("family:alice", Utc::now() + Duration::hours(1))
        .await
        .expect("start session");

    catalog
        .suspend_session(&session.session_id)
        .await
        .expect("suspend session");
    catalog
        .revoke_session(&session.session_id)
        .await
        .expect("revoke session");
    let looked_up = catalog
        .lookup_session(&session.session_id)
        .await
        .expect("lookup revoked session");

    assert_eq!(looked_up.state, SessionState::Revoked);
}

#[tokio::test]
async fn add_to_group_and_remove_from_group_lifecycle() {
    let catalog = catalog_with_subject(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["family"],
    ))
    .await;

    catalog
        .add_to_group("family:alice", "homelab")
        .await
        .expect("add group");
    let snapshot = VaultSubjectHydrator::new(Arc::new(catalog))
        .hydrate_by_canonical_id("family:alice")
        .await
        .expect("hydrate subject");
    assert_eq!(
        snapshot.groups,
        vec!["family".to_owned(), "homelab".to_owned()]
    );
}

#[tokio::test]
async fn duplicate_group_membership_returns_group_membership_unchanged() {
    let catalog = catalog_with_subject(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["family"],
    ))
    .await;

    let error = catalog
        .add_to_group("family:alice", "family")
        .await
        .expect_err("duplicate membership must fail");

    assert_eq!(error, VaultError::GroupMembershipUnchanged);
}

#[tokio::test]
async fn hydrate_by_session_returns_full_subject_snapshot() {
    let catalog = catalog_with_subject(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["family"],
    ))
    .await;
    catalog
        .add_to_group("family:alice", "homelab")
        .await
        .expect("add group");
    let session = catalog
        .start_session("family:alice", Utc::now() + Duration::hours(1))
        .await
        .expect("start session");
    let hydrator = VaultSubjectHydrator::new(Arc::new(catalog));

    let snapshot = hydrator
        .hydrate_by_session(&session.session_id)
        .await
        .expect("hydrate by session");

    assert_eq!(snapshot.canonical_subject_id, "family:alice");
    assert_eq!(snapshot.subject_type, SubjectType::Human);
    assert_eq!(
        snapshot.groups,
        vec!["family".to_owned(), "homelab".to_owned()]
    );
    assert!(snapshot.capabilities.is_empty());
    assert_eq!(snapshot.session_class, "INTERACTIVE");
    assert!(!snapshot.recovery_mode);
    assert!(!snapshot.is_ai);
}

#[tokio::test]
async fn hydrate_by_canonical_id_works_without_session() {
    let catalog = catalog_with_subject(subject(
        "_system:kwin",
        SubjectType::Service,
        false,
        &["_system"],
    ))
    .await;
    let hydrator = VaultSubjectHydrator::new(Arc::new(catalog));

    let snapshot = hydrator
        .hydrate_by_canonical_id("_system:kwin")
        .await
        .expect("hydrate by canonical id");

    assert_eq!(snapshot.canonical_subject_id, "_system:kwin");
    assert_eq!(snapshot.subject_type, SubjectType::Service);
    assert_eq!(snapshot.session_class, "SERVICE");
    assert!(!snapshot.recovery_mode);
}

#[tokio::test]
async fn hydrate_of_agent_subject_sets_is_ai_true() {
    let catalog =
        catalog_with_subject(subject("agent:dev", SubjectType::Agent, true, &["agent"])).await;
    let hydrator = VaultSubjectHydrator::new(Arc::new(catalog));

    let snapshot = hydrator
        .hydrate_by_canonical_id("agent:dev")
        .await
        .expect("hydrate agent");

    assert!(snapshot.is_ai);
}

#[tokio::test]
async fn hydrate_of_human_subject_sets_is_ai_false() {
    let catalog = catalog_with_subject(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["family"],
    ))
    .await;
    let hydrator = VaultSubjectHydrator::new(Arc::new(catalog));

    let snapshot = hydrator
        .hydrate_by_canonical_id("family:alice")
        .await
        .expect("hydrate human");

    assert!(!snapshot.is_ai);
}

#[tokio::test]
async fn with_fixtures_loads_five_canonical_subjects() {
    let catalog = IdentityCatalog::with_fixtures();

    let expected = [
        "family:alice",
        "agent:dev",
        "app:browser",
        "_system:kwin",
        "operator:root",
    ];
    for canonical_id in expected {
        catalog
            .lookup_subject(canonical_id)
            .await
            .expect("fixture subject exists");
    }
}

#[test]
fn hydrated_subject_snapshot_round_trips_through_serde_json() {
    let snapshot = HydratedSubjectSnapshot {
        canonical_subject_id: "family:alice".to_owned(),
        subject_type: SubjectType::Human,
        groups: vec!["family".to_owned()],
        capabilities: vec![],
        session_class: "INTERACTIVE".to_owned(),
        recovery_mode: false,
        is_ai: false,
    };

    let json = serde_json::to_string(&snapshot).expect("serialize snapshot");
    let back: HydratedSubjectSnapshot = serde_json::from_str(&json).expect("deserialize snapshot");

    assert_eq!(snapshot, back);
}

#[tokio::test]
async fn remove_from_group_updates_hydrated_groups() {
    let catalog = catalog_with_subject(subject(
        "family:alice",
        SubjectType::Human,
        false,
        &["family", "homelab"],
    ))
    .await;

    catalog
        .remove_from_group("family:alice", "homelab")
        .await
        .expect("remove group");
    let snapshot = VaultSubjectHydrator::new(Arc::new(catalog))
        .hydrate_by_canonical_id("family:alice")
        .await
        .expect("hydrate subject");

    assert_eq!(snapshot.groups, vec!["family".to_owned()]);
}

async fn catalog_with_subject(subject: Subject) -> IdentityCatalog {
    let catalog = IdentityCatalog::new();
    catalog
        .register_subject(subject)
        .await
        .expect("register subject");
    catalog
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
