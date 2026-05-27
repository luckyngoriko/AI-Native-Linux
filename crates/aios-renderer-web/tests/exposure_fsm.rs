#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::no_effect_underscore_binding,
    clippy::unwrap_used,
    clippy::bool_assert_comparison,
    missing_docs
)]

use std::time::Duration;

use aios_renderer_web::{ExposureFsm, ExposureLevel, ExposureLevelLabel, ExposureTransitionReason};

// ── constructors / accessors ──────────────────────────────────────────

#[tokio::test]
async fn new_fsm_starts_at_localhost_no_history() {
    let fsm = ExposureFsm::new();
    let current = fsm.current().await;
    assert!(matches!(current, ExposureLevel::Localhost));
    let history = fsm.history().await;
    assert!(history.is_empty());
}

// ── happy-path LAN escalation chain ───────────────────────────────────

#[tokio::test]
async fn request_lan_escalation_from_localhost_transitions_to_lan_pending() {
    let fsm = ExposureFsm::new();
    fsm.request_lan_escalation("operator:root").await.unwrap();
    let current = fsm.current().await;
    assert!(matches!(current, ExposureLevel::LanPending { .. }));
    let history = fsm.history().await;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].from, ExposureLevelLabel::Localhost);
    assert_eq!(history[0].to, ExposureLevelLabel::LanPending);
    assert!(matches!(
        history[0].reason,
        ExposureTransitionReason::OperatorRequest { .. }
    ));
}

#[tokio::test]
async fn apply_policy_decision_from_lan_pending_transitions_to_lan_approved() {
    let fsm = ExposureFsm::new();
    fsm.request_lan_escalation("operator:root").await.unwrap();
    fsm.apply_policy_decision("dec-001").await.unwrap();
    let current = fsm.current().await;
    assert!(matches!(current, ExposureLevel::LanApproved { .. }));
    let history = fsm.history().await;
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].from, ExposureLevelLabel::LanPending);
    assert_eq!(history[1].to, ExposureLevelLabel::LanApproved);
    assert!(matches!(
        history[1].reason,
        ExposureTransitionReason::PolicyApprovalGranted { .. }
    ));
}

#[tokio::test]
async fn activate_lan_exposure_from_lan_approved_transitions_to_lan_active_with_initial_heartbeat()
{
    let fsm = ExposureFsm::new();
    fsm.request_lan_escalation("operator:root").await.unwrap();
    fsm.apply_policy_decision("dec-001").await.unwrap();
    fsm.activate_lan_exposure().await.unwrap();
    let current = fsm.current().await;
    assert!(matches!(current, ExposureLevel::LanActive { .. }));
    if let ExposureLevel::LanActive {
        activated_at,
        last_heartbeat_at,
    } = current
    {
        assert_eq!(activated_at, last_heartbeat_at);
    }
    let history = fsm.history().await;
    assert_eq!(history.len(), 3);
    assert_eq!(history[2].from, ExposureLevelLabel::LanApproved);
    assert_eq!(history[2].to, ExposureLevelLabel::LanActive);
}

#[tokio::test]
async fn record_heartbeat_in_lan_active_refreshes_last_heartbeat_at() {
    let fsm = ExposureFsm::new();
    fsm.request_lan_escalation("operator:root").await.unwrap();
    fsm.apply_policy_decision("dec-001").await.unwrap();
    fsm.activate_lan_exposure().await.unwrap();

    let ExposureLevel::LanActive {
        last_heartbeat_at: before,
        ..
    } = fsm.current().await
    else {
        panic!("expected LanActive");
    };

    // Sleep a tiny bit so the new timestamp is observably different.
    tokio::time::sleep(Duration::from_millis(10)).await;

    fsm.record_heartbeat().await.unwrap();

    let ExposureLevel::LanActive {
        last_heartbeat_at: after,
        ..
    } = fsm.current().await
    else {
        panic!("expected LanActive after heartbeat");
    };

    assert!(after > before, "heartbeat should refresh last_heartbeat_at");
    // heartbeat is a self-transition — no history entry
    assert_eq!(fsm.history().await.len(), 3);
}

// ── revoke from various states ────────────────────────────────────────

#[tokio::test]
async fn revoke_from_lan_active_transitions_to_revoked() {
    let fsm = ExposureFsm::new();
    fsm.request_lan_escalation("operator:root").await.unwrap();
    fsm.apply_policy_decision("dec-001").await.unwrap();
    fsm.activate_lan_exposure().await.unwrap();

    fsm.revoke("manual shutoff").await.unwrap();
    assert!(matches!(fsm.current().await, ExposureLevel::Revoked { .. }));
    let history = fsm.history().await;
    let last = history.last().unwrap();
    assert_eq!(last.to, ExposureLevelLabel::Revoked);
    assert!(matches!(
        last.reason,
        ExposureTransitionReason::Revoked { .. }
    ));
}

#[tokio::test]
async fn revoke_from_lan_pending_transitions_to_revoked() {
    let fsm = ExposureFsm::new();
    fsm.request_lan_escalation("operator:root").await.unwrap();
    fsm.revoke("cancelled before approval").await.unwrap();
    assert!(matches!(fsm.current().await, ExposureLevel::Revoked { .. }));
}

#[tokio::test]
async fn revoke_from_lan_approved_transitions_to_revoked() {
    let fsm = ExposureFsm::new();
    fsm.request_lan_escalation("operator:root").await.unwrap();
    fsm.apply_policy_decision("dec-001").await.unwrap();
    fsm.revoke("cancelled before activation").await.unwrap();
    assert!(matches!(fsm.current().await, ExposureLevel::Revoked { .. }));
}

// ── reset / re-arm ────────────────────────────────────────────────────

#[tokio::test]
async fn reset_to_localhost_from_revoked_succeeds() {
    let fsm = ExposureFsm::new();
    fsm.request_lan_escalation("operator:root").await.unwrap();
    fsm.revoke("test").await.unwrap();
    let history_before = fsm.history().await.len();

    fsm.reset_to_localhost().await.unwrap();
    assert!(matches!(fsm.current().await, ExposureLevel::Localhost));
    assert_eq!(fsm.history().await.len(), history_before + 1);
}

#[tokio::test]
async fn reset_to_localhost_from_lan_active_returns_exposure_escalation_denied() {
    let fsm = ExposureFsm::new();
    fsm.request_lan_escalation("operator:root").await.unwrap();
    fsm.apply_policy_decision("dec-001").await.unwrap();
    fsm.activate_lan_exposure().await.unwrap();

    let err = fsm.reset_to_localhost().await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("exposure escalation denied"));
    assert!(msg.contains("LanActive"));
    assert!(msg.contains("Localhost"));
}

// ── public escalation ─────────────────────────────────────────────────

#[tokio::test]
async fn escalate_to_public_from_localhost_with_recovery_auth_succeeds() {
    let fsm = ExposureFsm::new();
    fsm.escalate_to_public("recovery-op", "dec-pub-001")
        .await
        .unwrap();
    assert!(matches!(fsm.current().await, ExposureLevel::Public { .. }));
    let history = fsm.history().await;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].from, ExposureLevelLabel::Localhost);
    assert_eq!(history[0].to, ExposureLevelLabel::Public);
    assert!(matches!(
        history[0].reason,
        ExposureTransitionReason::RecoveryAuthorized { .. }
    ));
}

#[tokio::test]
async fn escalate_to_public_from_lan_active_returns_exposure_escalation_denied() {
    let fsm = ExposureFsm::new();
    fsm.request_lan_escalation("operator:root").await.unwrap();
    fsm.apply_policy_decision("dec-001").await.unwrap();
    fsm.activate_lan_exposure().await.unwrap();

    let err = fsm
        .escalate_to_public("recovery-op", "dec-pub-001")
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("exposure escalation denied"));
    assert!(msg.contains("LanActive"));
    assert!(msg.contains("Public"));
}

// ── disallowed-by-design transitions ──────────────────────────────────

#[tokio::test]
async fn activate_lan_exposure_from_localhost_returns_exposure_escalation_denied() {
    let fsm = ExposureFsm::new();
    let err = fsm.activate_lan_exposure().await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("exposure escalation denied"));
    assert!(msg.contains("Localhost"));
    assert!(msg.contains("LanActive"));
}

#[tokio::test]
async fn apply_policy_decision_from_localhost_returns_exposure_escalation_denied() {
    let fsm = ExposureFsm::new();
    let err = fsm.apply_policy_decision("dec-001").await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("exposure escalation denied"));
    assert!(msg.contains("Localhost"));
    assert!(msg.contains("LanApproved"));
}

// ── heartbeat guard ───────────────────────────────────────────────────

#[tokio::test]
async fn check_heartbeat_within_window_returns_ok() {
    let fsm = ExposureFsm::new();
    fsm.request_lan_escalation("operator:root").await.unwrap();
    fsm.apply_policy_decision("dec-001").await.unwrap();
    fsm.activate_lan_exposure().await.unwrap();

    // Immediately check — should be within window.
    assert!(fsm.check_heartbeat().await.is_ok());
    // State is still LanActive.
    assert!(matches!(
        fsm.current().await,
        ExposureLevel::LanActive { .. }
    ));
}

#[tokio::test]
async fn check_heartbeat_beyond_window_revokes_and_returns_error() {
    let fsm = ExposureFsm::with_heartbeat_interval(Duration::from_millis(1));
    fsm.request_lan_escalation("operator:root").await.unwrap();
    fsm.apply_policy_decision("dec-001").await.unwrap();
    fsm.activate_lan_exposure().await.unwrap();

    // Push last_heartbeat_at into the distant past.
    fsm.set_last_heartbeat_at_for_tests(chrono::Utc::now() - chrono::Duration::seconds(60))
        .await;

    let err = fsm.check_heartbeat().await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("exposure escalation denied"));
    assert!(msg.contains("heartbeat missed"));

    // FSM should now be Revoked.
    assert!(matches!(fsm.current().await, ExposureLevel::Revoked { .. }));
    let history = fsm.history().await;
    let last = history.last().unwrap();
    assert_eq!(last.from, ExposureLevelLabel::LanActive);
    assert_eq!(last.to, ExposureLevelLabel::Revoked);
    assert!(matches!(
        last.reason,
        ExposureTransitionReason::HeartbeatMissed
    ));
}

// ── history correctness ───────────────────────────────────────────────

#[tokio::test]
async fn history_after_full_lifecycle_localhost_to_revoked_has_4_entries() {
    let fsm = ExposureFsm::new();
    // Step 1: Localhost → LanPending
    fsm.request_lan_escalation("operator:root").await.unwrap();
    // Step 2: LanPending → LanApproved
    fsm.apply_policy_decision("dec-001").await.unwrap();
    // Step 3: LanApproved → LanActive
    fsm.activate_lan_exposure().await.unwrap();
    // Step 4: LanActive → Revoked
    fsm.revoke("test").await.unwrap();

    let history = fsm.history().await;
    assert_eq!(history.len(), 4);
    assert_eq!(history[0].from, ExposureLevelLabel::Localhost);
    assert_eq!(history[0].to, ExposureLevelLabel::LanPending);
    assert_eq!(history[1].from, ExposureLevelLabel::LanPending);
    assert_eq!(history[1].to, ExposureLevelLabel::LanApproved);
    assert_eq!(history[2].from, ExposureLevelLabel::LanApproved);
    assert_eq!(history[2].to, ExposureLevelLabel::LanActive);
    assert_eq!(history[3].from, ExposureLevelLabel::LanActive);
    assert_eq!(history[3].to, ExposureLevelLabel::Revoked);
}

// ── concurrency ───────────────────────────────────────────────────────

#[tokio::test]
async fn concurrent_request_then_revoke_no_panic() {
    let fsm = std::sync::Arc::new(ExposureFsm::new());

    let fsm_a = std::sync::Arc::clone(&fsm);
    let t1 = tokio::spawn(async move {
        fsm_a.request_lan_escalation("operator:root").await.unwrap();
    });

    let fsm_b = std::sync::Arc::clone(&fsm);
    let t2 = tokio::spawn(async move {
        // Wait briefly so t1 likely wins the race.
        tokio::time::sleep(Duration::from_millis(5)).await;
        // This may or may not succeed depending on race, but must not panic.
        let _ = fsm_b.revoke("concurrent").await;
    });

    let (r1, r2) = tokio::join!(t1, t2);
    r1.unwrap();
    r2.unwrap();
    // Regardless of ordering, the FSM must be in a consistent state.
    let current = fsm.current().await;
    let label = current.label();
    assert!(
        label == ExposureLevelLabel::LanPending || label == ExposureLevelLabel::Revoked,
        "expected LanPending or Revoked, got {label:?}"
    );
}
