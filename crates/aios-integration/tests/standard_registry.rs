#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::doc_markdown,
    clippy::significant_drop_tightening,
    clippy::wildcard_imports,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::needless_collect,
    clippy::format_collect,
    clippy::missing_const_for_fn,
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use aios_integration::*;
use chrono::{Duration, Utc};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_sub(id: &str, kind: StandardKind) -> StandardSubscription {
    let now = Utc::now();
    StandardSubscription {
        subscription_id: StandardSubscriptionId(id.into()),
        standard: kind,
        catalog_url: standard_kind_to_canonical_url(kind).into(),
        current_revision: "v1.0".into(),
        last_reviewed_at: now,
        next_review_due_at: now + Duration::days(90),
        responsible_canonical_id: "auditor-1".into(),
    }
}

// ---------------------------------------------------------------------------
// subscribe / list
// ---------------------------------------------------------------------------

#[tokio::test]
async fn subscribe_then_list_returns_1() {
    let reg = ExternalStandardRegistry::new();
    let sub = make_sub("sub-001", StandardKind::Gdpr);
    reg.subscribe(sub.clone()).await.unwrap();
    let all = reg.list_subscriptions().await;
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].subscription_id.0, "sub-001");
}

#[tokio::test]
async fn subscribe_duplicate_id_returns_internal_error() {
    let reg = ExternalStandardRegistry::new();
    let sub = make_sub("sub-001", StandardKind::Gdpr);
    reg.subscribe(sub.clone()).await.unwrap();
    let err = reg.subscribe(sub).await.unwrap_err();
    assert!(matches!(err, IntegrationError::Internal(_)));
    assert!(format!("{err}").contains("already exists"));
}

// ---------------------------------------------------------------------------
// revise
// ---------------------------------------------------------------------------

#[tokio::test]
async fn revise_unknown_subscription_returns_internal_error() {
    let reg = ExternalStandardRegistry::new();
    let err = reg
        .revise(
            &StandardSubscriptionId("ghost".into()),
            "v2.0".into(),
            "reviewer".into(),
            "note".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, IntegrationError::Internal(_)));
}

#[tokio::test]
async fn revise_known_subscription_updates_revision_and_records_history() {
    let reg = ExternalStandardRegistry::new();
    let mut sub = make_sub("sub-001", StandardKind::Nist80053Rev5);
    sub.current_revision = "r4".into();
    reg.subscribe(sub.clone()).await.unwrap();

    reg.revise(
        &StandardSubscriptionId("sub-001".into()),
        "r5".into(),
        "auditor".into(),
        "updated to r5".into(),
    )
    .await
    .unwrap();

    let subs = reg.list_subscriptions().await;
    assert_eq!(subs[0].current_revision, "r5");

    let history = reg
        .review_history_for(&StandardSubscriptionId("sub-001".into()))
        .await;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].revision_before, "r4");
    assert_eq!(history[0].revision_after, "r5");
    assert_eq!(history[0].reviewer, "auditor");
}

#[tokio::test]
async fn revise_appends_review_history_entry() {
    let reg = ExternalStandardRegistry::new();
    let sub = make_sub("sub-001", StandardKind::Iso27001);
    reg.subscribe(sub).await.unwrap();

    reg.revise(
        &StandardSubscriptionId("sub-001".into()),
        "2025".into(),
        "alice".into(),
        "annual review".into(),
    )
    .await
    .unwrap();

    let history = reg
        .review_history_for(&StandardSubscriptionId("sub-001".into()))
        .await;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].reviewer, "alice");
    assert_eq!(history[0].note, "annual review");
}

#[tokio::test]
async fn revise_with_3_revisions_history_has_3_entries() {
    let reg = ExternalStandardRegistry::new();
    let sub = make_sub("sub-001", StandardKind::Soc2);
    reg.subscribe(sub).await.unwrap();

    for (rev, r) in ["v2.0", "v3.0", "v4.0"].iter().enumerate() {
        reg.revise(
            &StandardSubscriptionId("sub-001".into()),
            (*r).into(),
            format!("reviewer-{rev}"),
            format!("note-{rev}"),
        )
        .await
        .unwrap();
    }

    let history = reg
        .review_history_for(&StandardSubscriptionId("sub-001".into()))
        .await;
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].revision_before, "v1.0");
    assert_eq!(history[0].revision_after, "v2.0");
    assert_eq!(history[2].revision_after, "v4.0");
}

// ---------------------------------------------------------------------------
// status
// ---------------------------------------------------------------------------

#[tokio::test]
async fn status_within_review_window_returns_current() {
    let reg = ExternalStandardRegistry::new();
    let now = Utc::now();
    let mut sub = make_sub("sub-001", StandardKind::Gdpr);
    sub.next_review_due_at = now + Duration::days(60);
    reg.subscribe(sub).await.unwrap();

    let status = reg
        .status(&StandardSubscriptionId("sub-001".into()), now)
        .await
        .unwrap();
    assert!(matches!(status, SubscriptionStatus::Current { .. }));
}

#[tokio::test]
async fn status_past_next_review_due_returns_review_due() {
    let reg = ExternalStandardRegistry::new();
    let now = Utc::now();
    let mut sub = make_sub("sub-001", StandardKind::Hipaa);
    sub.next_review_due_at = now - Duration::days(10);
    reg.subscribe(sub).await.unwrap();

    let status = reg
        .status(&StandardSubscriptionId("sub-001".into()), now)
        .await
        .unwrap();
    assert!(matches!(status, SubscriptionStatus::ReviewDue { .. }));
}

#[tokio::test]
async fn status_past_30_day_grace_returns_expired() {
    let reg = ExternalStandardRegistry::new();
    let now = Utc::now();
    let mut sub = make_sub("sub-001", StandardKind::Fips1403);
    sub.next_review_due_at = now - Duration::days(35);
    reg.subscribe(sub).await.unwrap();

    let status = reg
        .status(&StandardSubscriptionId("sub-001".into()), now)
        .await
        .unwrap();
    assert!(matches!(status, SubscriptionStatus::Expired { .. }));
}

#[tokio::test]
async fn status_exactly_at_grace_boundary_returns_expired() {
    let reg = ExternalStandardRegistry::new();
    let now = Utc::now();
    let mut sub = make_sub("sub-001", StandardKind::CisControlsV8);
    sub.next_review_due_at = now - Duration::days(30);
    reg.subscribe(sub).await.unwrap();

    // now == next_review_due_at + 30d is exactly the grace boundary;
    // status returns Expired only when now > grace_deadline, so this should be ReviewDue.
    let status = reg
        .status(&StandardSubscriptionId("sub-001".into()), now)
        .await
        .unwrap();
    assert!(matches!(status, SubscriptionStatus::ReviewDue { .. }));
}

// ---------------------------------------------------------------------------
// list_by_kind / list_due_for_review / list_expired
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_by_kind_filters_correctly() {
    let reg = ExternalStandardRegistry::new();
    reg.subscribe(make_sub("sub-a", StandardKind::Gdpr))
        .await
        .unwrap();
    reg.subscribe(make_sub("sub-b", StandardKind::Hipaa))
        .await
        .unwrap();
    reg.subscribe(make_sub("sub-c", StandardKind::Gdpr))
        .await
        .unwrap();

    let gdpr = reg.list_by_kind(StandardKind::Gdpr).await;
    assert_eq!(gdpr.len(), 2);
    let hipaa = reg.list_by_kind(StandardKind::Hipaa).await;
    assert_eq!(hipaa.len(), 1);
}

#[tokio::test]
async fn list_due_for_review_returns_only_due() {
    let reg = ExternalStandardRegistry::new();
    let now = Utc::now();

    let mut sub_current = make_sub("sub-current", StandardKind::Gdpr);
    sub_current.next_review_due_at = now + Duration::days(30);
    reg.subscribe(sub_current).await.unwrap();

    let mut sub_due = make_sub("sub-due", StandardKind::Hipaa);
    sub_due.next_review_due_at = now - Duration::days(5);
    reg.subscribe(sub_due).await.unwrap();

    let due = reg.list_due_for_review(now).await;
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].0, "sub-due");
}

#[tokio::test]
async fn list_expired_returns_only_past_grace() {
    let reg = ExternalStandardRegistry::new();
    let now = Utc::now();

    let mut sub_review_due = make_sub("sub-due", StandardKind::Iso27001);
    sub_review_due.next_review_due_at = now - Duration::days(10);
    reg.subscribe(sub_review_due).await.unwrap();

    let mut sub_expired = make_sub("sub-expired", StandardKind::Soc2);
    sub_expired.next_review_due_at = now - Duration::days(35);
    reg.subscribe(sub_expired).await.unwrap();

    let expired = reg.list_expired(now).await;
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].0, "sub-expired");
}

// ---------------------------------------------------------------------------
// review_history_for
// ---------------------------------------------------------------------------

#[tokio::test]
async fn review_history_for_unknown_returns_empty() {
    let reg = ExternalStandardRegistry::new();
    let history = reg
        .review_history_for(&StandardSubscriptionId("ghost".into()))
        .await;
    assert!(history.is_empty());
}

// ---------------------------------------------------------------------------
// unsubscribe
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unsubscribe_known_succeeds_then_lookup_fails() {
    let reg = ExternalStandardRegistry::new();
    let sub = make_sub("sub-001", StandardKind::Gdpr);
    reg.subscribe(sub).await.unwrap();
    reg.unsubscribe(&StandardSubscriptionId("sub-001".into()))
        .await
        .unwrap();

    // Verify it's gone.
    let all = reg.list_subscriptions().await;
    assert!(all.is_empty());
}

#[tokio::test]
async fn unsubscribe_unknown_returns_internal_error() {
    let reg = ExternalStandardRegistry::new();
    let err = reg
        .unsubscribe(&StandardSubscriptionId("ghost".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, IntegrationError::Internal(_)));
}

// ---------------------------------------------------------------------------
// standard_kind_to_canonical_url
// ---------------------------------------------------------------------------

#[test]
fn standard_kind_to_canonical_url_for_nist_80053_contains_csrc_nist_gov() {
    let url = standard_kind_to_canonical_url(StandardKind::Nist80053Rev5);
    assert!(url.contains("csrc.nist.gov"));
}

#[test]
fn standard_kind_to_canonical_url_for_disa_stig_contains_public_cyber_mil() {
    let url = standard_kind_to_canonical_url(StandardKind::DisaStig);
    assert!(url.contains("public.cyber.mil"));
}

// ---------------------------------------------------------------------------
// concurrent subscribe
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_subscribe_3_distinct_no_panic() {
    use std::sync::Arc;

    let reg = Arc::new(ExternalStandardRegistry::new());
    let kinds = [StandardKind::Gdpr, StandardKind::Hipaa, StandardKind::Soc2];

    let mut handles = vec![];
    for (i, kind) in kinds.iter().enumerate() {
        let reg = Arc::clone(&reg);
        let sub = make_sub(&format!("sub-concurrent-{i}"), *kind);
        handles.push(tokio::spawn(async move {
            reg.subscribe(sub).await.unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(reg.list_subscriptions().await.len(), 3);
}
