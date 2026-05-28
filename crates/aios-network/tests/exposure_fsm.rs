#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::time::Duration;

use aios_network::{
    ExposureApprovalFsm, ExposureApprovalLabel, ExposureApprovalState, ExposureTransitionReason,
    SubjectId,
};
use chrono::{Duration as ChronoDuration, Utc};

fn subject(id: &str) -> SubjectId {
    SubjectId(id.to_string())
}

// ---------------------------------------------------------------------------
// Basics
// ---------------------------------------------------------------------------

#[tokio::test]
async fn new_fsm_starts_at_loopback_no_history() {
    let fsm = ExposureApprovalFsm::new();
    assert_eq!(fsm.current().await.label(), ExposureApprovalLabel::Loopback);
    assert!(fsm.history().await.is_empty());
}

// ---------------------------------------------------------------------------
// LAN lifecycle (INV I2)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn request_lan_from_loopback_transitions_to_lan_pending() {
    let fsm = ExposureApprovalFsm::new();
    fsm.request_lan(subject("human:lucky")).await.unwrap();
    assert_eq!(
        fsm.current().await.label(),
        ExposureApprovalLabel::LanPending
    );
    assert_eq!(fsm.history().await.len(), 1);
}

#[tokio::test]
async fn apply_lan_policy_decision_from_pending_transitions_to_lan_approved() {
    let fsm = ExposureApprovalFsm::new();
    fsm.request_lan(subject("human:lucky")).await.unwrap();
    fsm.apply_lan_policy_decision("dec-42").await.unwrap();
    assert_eq!(
        fsm.current().await.label(),
        ExposureApprovalLabel::LanApproved
    );
    assert_eq!(fsm.history().await.len(), 2);
}

#[tokio::test]
async fn activate_lan_from_approved_transitions_to_lan_active_with_initial_heartbeat() {
    let fsm = ExposureApprovalFsm::new();
    fsm.request_lan(subject("human:lucky")).await.unwrap();
    fsm.apply_lan_policy_decision("dec-42").await.unwrap();
    fsm.activate_lan().await.unwrap();
    assert_eq!(
        fsm.current().await.label(),
        ExposureApprovalLabel::LanActive
    );
    assert_eq!(fsm.history().await.len(), 3);
}

#[tokio::test]
async fn record_lan_heartbeat_refreshes_last_heartbeat_at() {
    let fsm = ExposureApprovalFsm::new();
    fsm.request_lan(subject("human:lucky")).await.unwrap();
    fsm.apply_lan_policy_decision("dec-42").await.unwrap();
    fsm.activate_lan().await.unwrap();

    let ExposureApprovalState::LanActive {
        last_heartbeat_at: before,
        ..
    } = fsm.current().await
    else {
        panic!("expected LanActive");
    };

    // tiny sleep to ensure timestamp changes
    tokio::time::sleep(Duration::from_millis(10)).await;

    fsm.record_lan_heartbeat().await.unwrap();

    let ExposureApprovalState::LanActive {
        last_heartbeat_at: after,
        ..
    } = fsm.current().await
    else {
        panic!("expected LanActive");
    };

    assert!(after > before, "heartbeat should refresh timestamp");
}

// ---------------------------------------------------------------------------
// Revoke / reset
// ---------------------------------------------------------------------------

#[tokio::test]
async fn revoke_from_lan_active_transitions_to_revoked() {
    let fsm = ExposureApprovalFsm::new();
    fsm.request_lan(subject("human:lucky")).await.unwrap();
    fsm.apply_lan_policy_decision("dec-42").await.unwrap();
    fsm.activate_lan().await.unwrap();
    fsm.revoke("operator decision").await.unwrap();
    assert_eq!(fsm.current().await.label(), ExposureApprovalLabel::Revoked);
}

#[tokio::test]
async fn revoke_from_lan_pending_transitions_to_revoked() {
    let fsm = ExposureApprovalFsm::new();
    fsm.request_lan(subject("human:lucky")).await.unwrap();
    fsm.revoke("aborted").await.unwrap();
    assert_eq!(fsm.current().await.label(), ExposureApprovalLabel::Revoked);
}

#[tokio::test]
async fn reset_to_loopback_from_revoked_succeeds() {
    let fsm = ExposureApprovalFsm::new();
    fsm.request_lan(subject("human:lucky")).await.unwrap();
    fsm.revoke("test").await.unwrap();
    fsm.reset_to_loopback().await.unwrap();
    assert_eq!(fsm.current().await.label(), ExposureApprovalLabel::Loopback);
}

// ---------------------------------------------------------------------------
// Invalid transitions (must be denied)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reset_to_loopback_from_lan_active_returns_exposure_escalation_denied() {
    let fsm = ExposureApprovalFsm::new();
    fsm.request_lan(subject("human:lucky")).await.unwrap();
    fsm.apply_lan_policy_decision("dec-42").await.unwrap();
    fsm.activate_lan().await.unwrap();
    let err = fsm.reset_to_loopback().await.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("ExposureEscalationDenied") || msg.contains("exposure escalation"));
}

#[tokio::test]
async fn activate_lan_from_loopback_returns_exposure_escalation_denied() {
    let fsm = ExposureApprovalFsm::new();
    let err = fsm.activate_lan().await.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("ExposureEscalationDenied") || msg.contains("exposure escalation"));
}

#[tokio::test]
async fn lan_active_to_public_pending_direct_transition_forbidden() {
    let fsm = ExposureApprovalFsm::new();
    fsm.request_lan(subject("human:lucky")).await.unwrap();
    fsm.apply_lan_policy_decision("dec-42").await.unwrap();
    fsm.activate_lan().await.unwrap();
    let err = fsm
        .request_public(subject("human:lucky"), "recovery-session-1")
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("ExposureEscalationDenied") || msg.contains("exposure escalation"));
}

// ---------------------------------------------------------------------------
// PUBLIC lifecycle (INV I10)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn request_public_with_empty_recovery_session_id_returns_exposure_escalation_denied() {
    let fsm = ExposureApprovalFsm::new();
    let err = fsm
        .request_public(subject("human:lucky"), "")
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("recovery-mode session required"));
}

#[tokio::test]
async fn request_public_with_recovery_session_id_transitions_to_public_pending() {
    let fsm = ExposureApprovalFsm::new();
    fsm.request_public(subject("human:lucky"), "recovery-abc")
        .await
        .unwrap();
    assert_eq!(
        fsm.current().await.label(),
        ExposureApprovalLabel::PublicPending
    );
    assert_eq!(fsm.history().await.len(), 1);
}

#[tokio::test]
async fn apply_public_co_signer_approval_from_pending_transitions_to_public_approved() {
    let fsm = ExposureApprovalFsm::new();
    fsm.request_public(subject("human:lucky"), "recovery-abc")
        .await
        .unwrap();
    let ttl = Utc::now() + ChronoDuration::hours(4);
    fsm.apply_public_co_signer_approval("pub-dec-1", subject("co-signer:admin"), ttl)
        .await
        .unwrap();
    assert_eq!(
        fsm.current().await.label(),
        ExposureApprovalLabel::PublicApproved
    );
    assert_eq!(fsm.history().await.len(), 2);
}

#[tokio::test]
async fn activate_public_from_approved_transitions_to_public_active() {
    let fsm = ExposureApprovalFsm::new();
    fsm.request_public(subject("human:lucky"), "recovery-abc")
        .await
        .unwrap();
    let ttl = Utc::now() + ChronoDuration::hours(4);
    fsm.apply_public_co_signer_approval("pub-dec-1", subject("co-signer:admin"), ttl)
        .await
        .unwrap();
    fsm.activate_public().await.unwrap();
    assert_eq!(
        fsm.current().await.label(),
        ExposureApprovalLabel::PublicActive
    );
    assert_eq!(fsm.history().await.len(), 3);
}

#[tokio::test]
async fn record_public_heartbeat_refreshes_last_heartbeat_at() {
    let fsm = ExposureApprovalFsm::new();
    fsm.request_public(subject("human:lucky"), "recovery-abc")
        .await
        .unwrap();
    let ttl = Utc::now() + ChronoDuration::hours(4);
    fsm.apply_public_co_signer_approval("pub-dec-1", subject("co-signer:admin"), ttl)
        .await
        .unwrap();
    fsm.activate_public().await.unwrap();

    let ExposureApprovalState::PublicActive {
        last_heartbeat_at: before,
        ..
    } = fsm.current().await
    else {
        panic!("expected PublicActive");
    };

    tokio::time::sleep(Duration::from_millis(10)).await;

    fsm.record_public_heartbeat().await.unwrap();

    let ExposureApprovalState::PublicActive {
        last_heartbeat_at: after,
        ..
    } = fsm.current().await
    else {
        panic!("expected PublicActive");
    };

    assert!(after > before, "heartbeat should refresh timestamp");
}

// ---------------------------------------------------------------------------
// Heartbeat / TTL guard tests (deterministic: 1ms intervals)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn check_heartbeat_lan_within_window_returns_ok() {
    let fsm = ExposureApprovalFsm::with_intervals(Duration::from_hours(24), Duration::from_mins(5));
    fsm.request_lan(subject("human:lucky")).await.unwrap();
    fsm.apply_lan_policy_decision("dec-42").await.unwrap();
    fsm.activate_lan().await.unwrap();

    // heartbeat is current (just activated) — should be OK
    fsm.check_heartbeat().await.unwrap();
    assert_eq!(
        fsm.current().await.label(),
        ExposureApprovalLabel::LanActive
    );
}

#[tokio::test]
async fn check_heartbeat_lan_beyond_window_auto_revokes() {
    let fsm =
        ExposureApprovalFsm::with_intervals(Duration::from_millis(1), Duration::from_secs(9999));
    fsm.request_lan(subject("human:lucky")).await.unwrap();
    fsm.apply_lan_policy_decision("dec-42").await.unwrap();
    fsm.activate_lan().await.unwrap();

    // push last_heartbeat_at into the past
    let past = Utc::now() - ChronoDuration::milliseconds(100);
    fsm.set_last_heartbeat_at_for_tests(past).await;

    fsm.check_heartbeat().await.unwrap();

    assert_eq!(fsm.current().await.label(), ExposureApprovalLabel::Revoked);
}

#[tokio::test]
async fn check_heartbeat_public_beyond_5_min_auto_revokes() {
    let fsm =
        ExposureApprovalFsm::with_intervals(Duration::from_secs(9999), Duration::from_millis(1));
    fsm.request_public(subject("human:lucky"), "recovery-abc")
        .await
        .unwrap();
    let ttl = Utc::now() + ChronoDuration::hours(4);
    fsm.apply_public_co_signer_approval("pub-dec-1", subject("co-signer:admin"), ttl)
        .await
        .unwrap();
    fsm.activate_public().await.unwrap();

    let past = Utc::now() - ChronoDuration::milliseconds(100);
    fsm.set_last_heartbeat_at_for_tests(past).await;

    fsm.check_heartbeat().await.unwrap();

    assert_eq!(fsm.current().await.label(), ExposureApprovalLabel::Revoked);
}

#[tokio::test]
async fn check_heartbeat_public_ttl_expired_auto_revokes_from_approved() {
    let fsm =
        ExposureApprovalFsm::with_intervals(Duration::from_secs(9999), Duration::from_secs(9999));
    fsm.request_public(subject("human:lucky"), "recovery-abc")
        .await
        .unwrap();
    // TTL already in the past
    let ttl = Utc::now() - ChronoDuration::minutes(1);
    fsm.apply_public_co_signer_approval("pub-dec-1", subject("co-signer:admin"), ttl)
        .await
        .unwrap();

    // State is PublicApproved with expired TTL
    assert_eq!(
        fsm.current().await.label(),
        ExposureApprovalLabel::PublicApproved
    );

    fsm.check_heartbeat().await.unwrap();

    assert_eq!(fsm.current().await.label(), ExposureApprovalLabel::Revoked);
    // verify history contains the revoke transition
    let history = fsm.history().await;
    let last = history.last().unwrap();
    assert_eq!(last.to, ExposureApprovalLabel::Revoked);
}

// ---------------------------------------------------------------------------
// Full lifecycle — history length
// ---------------------------------------------------------------------------

#[tokio::test]
async fn history_after_full_lifecycle_loopback_to_public_active_has_4_entries_or_more() {
    let fsm = ExposureApprovalFsm::new();
    fsm.request_public(subject("human:lucky"), "recovery-full")
        .await
        .unwrap();
    let ttl = Utc::now() + ChronoDuration::hours(4);
    fsm.apply_public_co_signer_approval("pub-full", subject("co-signer:admin"), ttl)
        .await
        .unwrap();
    fsm.activate_public().await.unwrap();
    fsm.record_public_heartbeat().await.unwrap();

    let history = fsm.history().await;
    assert!(
        history.len() >= 4,
        "expected 4+ entries, got {}",
        history.len()
    );

    // spot-check the transition reason sequence
    assert_eq!(
        history[0].reason,
        ExposureTransitionReason::PublicRequest {
            requester: subject("human:lucky"),
            recovery_session_id: "recovery-full".into(),
        }
    );
    assert_eq!(
        history[1].reason,
        ExposureTransitionReason::PublicCoSignerApproved {
            decision_id: "pub-full".into(),
            co_signer: subject("co-signer:admin"),
            ttl_expires_at: ttl,
        }
    );
    assert_eq!(history[2].reason, ExposureTransitionReason::PublicActivated);
    assert_eq!(history[3].reason, ExposureTransitionReason::PublicHeartbeat);
}

// ---------------------------------------------------------------------------
// Concurrent access — no panic
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_request_then_revoke_no_panic() {
    let fsm = std::sync::Arc::new(ExposureApprovalFsm::new());

    let fsm_a = fsm.clone();
    let t1 = tokio::spawn(async move {
        fsm_a.request_lan(subject("human:lucky")).await.unwrap();
    });

    let fsm_b = fsm.clone();
    let t2 = tokio::spawn(async move {
        fsm_b.revoke("concurrent").await.unwrap();
    });

    let _ = tokio::join!(t1, t2);
    // either LanPending or Revoked — both are valid depending on ordering
    let label = fsm.current().await.label();
    assert!(
        label == ExposureApprovalLabel::LanPending || label == ExposureApprovalLabel::Revoked,
        "expected LanPending or Revoked, got {label}"
    );
}
