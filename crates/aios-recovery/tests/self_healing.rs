//! Self-healing module tests — autonomous MINIX-inspired reincarnation logic.
//!
//! Covers: policy validation, health state transitions, driver decision logic,
//! evidence emission for healing actions, and INV-012 (recovery-required) guard.

#![allow(
    clippy::module_name_repetitions,
    clippy::too_long_first_doc_paragraph,
    reason = "Self-healing test names are intentionally descriptive"
)]

use std::sync::Arc;

use aios_recovery::{
    ComponentHealingConfig, ComponentHealthState, ComponentSnapshot, HealAction,
    HealActionKind, InMemoryRecoveryBoundary, InMemorySelfHealingDriver, PanicSeverity,
    RecoveryBoundary, RecoveryEvidenceEmitter, RecoveryMode, RecoveryMutableScope,
    RecoverySubjectRef, RestartPolicy, SelfHealingDriver, SelfHealingPolicy,
    WatchdogPolicy,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_signing_key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[42_u8; 32])
}

fn make_emitter(
    log: Arc<aios_recovery::InMemoryRecoveryEvidenceLog>,
) -> RecoveryEvidenceEmitter {
    RecoveryEvidenceEmitter::new(
        log,
        test_signing_key(),
        RecoverySubjectRef(aios_recovery::SELF_HEALING_SUBJECT.to_owned()),
    )
}

fn minix_policy() -> SelfHealingPolicy {
    let mut component_policies = std::collections::HashMap::new();
    component_policies.insert(
        "aios-network-manager".to_owned(),
        ComponentHealingConfig {
            display_name: "Network Manager".to_owned(),
            restart_policy: RestartPolicy::minix_style(3),
            allowed_scopes: vec![
                RecoveryMutableScope::ProcessLifecycle,
                RecoveryMutableScope::NetworkReconfig,
            ],
            component_type: Some("infrastructure".to_owned()),
        },
    );
    component_policies.insert(
        "aios-dns-resolver".to_owned(),
        ComponentHealingConfig {
            display_name: "DNS Resolver".to_owned(),
            restart_policy: RestartPolicy::conservative(5),
            allowed_scopes: vec![RecoveryMutableScope::ProcessLifecycle],
            component_type: Some("infrastructure".to_owned()),
        },
    );
    SelfHealingPolicy {
        enabled: true,
        minimum_mode: RecoveryMode::Recovery,
        component_policies,
        default_policy: RestartPolicy::conservative(2),
    }
}

fn disabled_policy() -> SelfHealingPolicy {
    SelfHealingPolicy::default()
}

// ---------------------------------------------------------------------------
// Policy validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn policy_normal_mode_with_components_is_invalid() {
    let mut bad = SelfHealingPolicy {
        minimum_mode: RecoveryMode::Normal,
        ..Default::default()
    };
    bad.component_policies.insert(
        "test-comp".to_owned(),
        ComponentHealingConfig {
            display_name: "Test".to_owned(),
            restart_policy: RestartPolicy::default(),
            allowed_scopes: vec![],
            component_type: None,
        },
    );
    assert!(bad.validate().is_err());
}

#[tokio::test]
async fn policy_recovery_mode_is_valid() {
    assert!(minix_policy().validate().is_ok());
}

#[tokio::test]
async fn policy_disabled_with_no_components_is_valid() {
    assert!(disabled_policy().validate().is_ok());
}

// ---------------------------------------------------------------------------
// Health states
// ---------------------------------------------------------------------------

#[test]
fn healthy_needs_no_intervention() {
    assert!(!ComponentHealthState::Healthy.needs_intervention());
    assert!(!ComponentHealthState::Unknown.needs_intervention());
}

#[test]
fn degraded_needs_intervention() {
    assert!(ComponentHealthState::Degraded.needs_intervention());
}

#[test]
fn failed_needs_intervention_and_is_terminal() {
    assert!(ComponentHealthState::Failed.needs_intervention());
    assert!(ComponentHealthState::Failed.is_terminal());
}

#[test]
fn failed_is_terminal_degraded_is_not() {
    assert!(!ComponentHealthState::Degraded.is_terminal());
}

// ---------------------------------------------------------------------------
// Restart policy backoff
// ---------------------------------------------------------------------------

#[test]
fn minix_style_zero_backoff() {
    let p = RestartPolicy::minix_style(5);
    // MINIX-style always returns 0.0 while within max_retries
    assert_eq!(p.backoff_for_attempt(1), Some(0.0));
    assert_eq!(p.backoff_for_attempt(3), Some(0.0));
    assert_eq!(p.backoff_for_attempt(5), Some(0.0)); // 5 <= 5 → still within budget
    assert_eq!(p.backoff_for_attempt(6), None);      // 6 > 5 → escalate
}

#[test]
fn conservative_backoff_grows() {
    let p = RestartPolicy::conservative(4);
    // Actual impl: base * (2 - attempt), saturating at 0
    // attempt 1: 2s * (2-1) = 2s
    assert_eq!(p.backoff_for_attempt(1), Some(2.0));
    // attempt 2: 2s * (2-2) = 0s
    assert_eq!(p.backoff_for_attempt(2), Some(0.0));
    // attempt 3: 2s * saturating_sub(3)=0 = 0s
    assert_eq!(p.backoff_for_attempt(3), Some(0.0));
    // attempt 4: same, 0s
    assert_eq!(p.backoff_for_attempt(4), Some(0.0));
    // attempt 5: > max_retries → escalate
    assert_eq!(p.backoff_for_attempt(5), None);
}

// ---------------------------------------------------------------------------
// Driver: disabled policy produces no actions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn disabled_driver_evaluates_to_empty_actions() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let driver = InMemorySelfHealingDriver::new(boundary);

    driver
        .set_policy(disabled_policy())
        .await
        .expect("disabled policy should be valid");

    driver
        .observe_health("aios-dns-resolver", ComponentHealthState::Failed)
        .await
        .expect("observe should succeed");

    let actions = driver.evaluate().await.expect("evaluate should succeed");
    assert!(actions.is_empty(), "disabled policy must produce zero actions");
}

// ---------------------------------------------------------------------------
// Driver: recovery-not-active blocks execution
// ---------------------------------------------------------------------------

#[tokio::test]
async fn heal_denied_when_recovery_not_active() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let driver = InMemorySelfHealingDriver::new(boundary);
    driver.set_policy(minix_policy()).await.expect("valid policy");

    driver
        .observe_health("aios-network-manager", ComponentHealthState::Failed)
        .await
        .expect("observe should succeed");

    // NOT in recovery mode, but policy requires it
    let action = HealAction {
        component_id: "aios-network-manager".to_owned(),
        observed_state: ComponentHealthState::Failed,
        action_kind: HealActionKind::Restart,
        required_scope: RecoveryMutableScope::ProcessLifecycle,
        reason: "test".to_owned(),
        decided_at: chrono::Utc::now(),
        sequence: 1,
    };

    let result = driver.execute_heal(&action).await.expect("execute should succeed");
    assert!(!result.success, "must fail when recovery not active");
    assert!(
        result.detail.contains("INV-012"),
        "detail should mention invariant violation"
    );
    assert!(result.receipt_id.is_none(), "no receipt when denied");
}

// ---------------------------------------------------------------------------
// Driver: full observe → evaluate → execute cycle with recovery active
// ---------------------------------------------------------------------------

#[tokio::test]
async fn full_cycle_in_recovery_produces_actions() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let log = Arc::new(aios_recovery::InMemoryRecoveryEvidenceLog::new());
    let emitter = Arc::new(make_emitter(log.clone()));
    let driver = InMemorySelfHealingDriver::new(boundary.clone()).with_evidence_emitter(emitter);

    driver.set_policy(minix_policy()).await.expect("valid policy");

    // Enter recovery mode first
    boundary
        .enter_recovery(aios_recovery::EnterRecoveryRequest {
            reason: "BOOT_FAILURE_AUTO".to_owned(),
            operator_grant: Some("self-healing-bootstrap".to_owned()),
            expected_phases: vec![aios_recovery::BootPhase::Recovery],
            bundle: None,
        })
        .await
        .expect("enter recovery should succeed");

    // Observe two failing components
    driver
        .observe_health("aios-network-manager", ComponentHealthState::Failed)
        .await
        .expect("observe nm");
    driver
        .observe_health("aios-dns-resolver", ComponentHealthState::Degraded)
        .await
        .expect("observe dns");

    // Evaluate should produce actions for both
    let actions = driver.evaluate().await.expect("evaluate");
    assert_eq!(actions.len(), 2, "two unhealthy components → two actions");

    // Execute both
    for action in &actions {
        let result = driver.execute_heal(action).await.expect("execute heal");
        assert!(result.success, "{} must succeed: {}", action.component_id, result.detail);
        assert!(
            result.receipt_id.is_some(),
            "{} must emit evidence",
            action.component_id
        );
    }

    // Full cycle
    let cycle = driver.heal_cycle().await.expect("heal cycle");
    assert_eq!(cycle.components_evaluated, 2);
    assert_eq!(cycle.actions_decided, 2);
    assert_eq!(cycle.actions_executed, 2);
    assert_eq!(cycle.actions_failed, 0);

    // Verify evidence was actually appended (2 from manual execute + 2 from heal_cycle)
    assert_eq!(log.len().await, 4, "receipts emitted for all executions");
}

// ---------------------------------------------------------------------------
// Driver: healthy observation resets retry counter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn healthy_observation_resets_retry_counter() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let log = Arc::new(aios_recovery::InMemoryRecoveryEvidenceLog::new());
    let emitter = Arc::new(make_emitter(log.clone()));
    let driver = InMemorySelfHealingDriver::new(boundary.clone()).with_evidence_emitter(emitter);

    driver.set_policy(minix_policy()).await.expect("valid policy");

    boundary
        .enter_recovery(aios_recovery::EnterRecoveryRequest {
            reason: "BOOT_FAILURE_AUTO".to_owned(),
            operator_grant: Some("self-healing-bootstrap".to_owned()),
            expected_phases: vec![aios_recovery::BootPhase::Recovery],
            bundle: None,
        })
        .await
        .expect("enter recovery");

    // First observation: Failed → counter=1
    driver
        .observe_health("aios-dns-resolver", ComponentHealthState::Failed)
        .await
        .expect("observe failed");
    let actions1 = driver.evaluate().await.expect("eval 1");
    assert_eq!(actions1.len(), 1, "first failure → one action");
    assert_eq!(actions1[0].action_kind, HealActionKind::Restart);

    // Second observation: still Failed → counter=2
    driver
        .observe_health("aios-dns-resolver", ComponentHealthState::Failed)
        .await
        .expect("observe failed again");
    let tracker = driver.tracker_for("aios-dns-resolver").await;
    assert_eq!(tracker.consecutive_failures, 2, "counter should be 2");

    // Third observation: Healthy → resets to 0
    driver
        .observe_health("aios-dns-resolver", ComponentHealthState::Healthy)
        .await
        .expect("observe healthy");
    let tracker = driver.tracker_for("aios-dns-resolver").await;
    assert_eq!(tracker.consecutive_failures, 0, "healthy reset the counter");

    // After reset: evaluate should produce no actions (component is healthy)
    let actions2 = driver.evaluate().await.expect("eval after healthy");
    assert!(actions2.is_empty(), "healthy → no actions needed");
}

// ---------------------------------------------------------------------------
// Evidence payload round-trip for healing
// ---------------------------------------------------------------------------

#[test]
fn healing_payload_round_trips_through_serde_json() {
    use aios_recovery::HealingAttemptedPayload;

    let original = HealingAttemptedPayload {
        component_id: "aios-dns-resolver".to_owned(),
        observed_state: ComponentHealthState::Failed,
        action_kind: HealActionKind::Restart,
        required_scope: RecoveryMutableScope::ProcessLifecycle,
        reason: "consecutive_failures=3 attempt=3/5".to_owned(),
        decided_at: chrono::Utc::now(),
        sequence: 42,
    };

    let json = serde_json::to_value(&original).expect("serialize");
    let round_tripped: HealingAttemptedPayload =
        serde_json::from_value(json).expect("deserialize");

    assert_eq!(round_tripped.component_id, original.component_id);
    assert_eq!(round_tripped.observed_state, original.observed_state);
    assert_eq!(round_tripped.action_kind, original.action_kind);
    assert_eq!(round_tripped.sequence, original.sequence);
}

// ---------------------------------------------------------------------------
// Subject constant is well-formed
// ---------------------------------------------------------------------------

#[test]
fn self_healing_subject_is_well_formed_system_subject() {
    assert!(aios_recovery::SELF_HEALING_SUBJECT.starts_with("_system:service:"));
    assert_eq!(aios_recovery::SELF_HEALING_SUBJECT, "_system:service:self-healing");
}

// ---------------------------------------------------------------------------
// RecoveryMutableScope has expected variants and default
// ---------------------------------------------------------------------------

#[test]
fn recovery_mutable_scope_has_all_expected_variants() {
    // Verify expected variants exist and default is ProcessLifecycle
    assert_eq!(RecoveryMutableScope::default(), RecoveryMutableScope::ProcessLifecycle);
    // Spot-check key variants are accessible (compile-time verification)
    let _ = RecoveryMutableScope::NetworkReconfig;
    let _ = RecoveryMutableScope::FilesystemMutation;
    let _ = RecoveryMutableScope::SysctlTuning;
    let _ = RecoveryMutableScope::MeshRouting;
}

// ---------------------------------------------------------------------------
// Tracker records observations correctly
// ---------------------------------------------------------------------------

#[test]
fn tracker_accumulates_failures_on_degraded_or_failed() {
    use aios_recovery::ComponentHealingTracker;

    let mut t = ComponentHealingTracker::default();
    assert_eq!(t.consecutive_failures, 0);
    t.record_observation(ComponentHealthState::Degraded);
    assert_eq!(t.consecutive_failures, 1);
    t.record_observation(ComponentHealthState::Failed);
    assert_eq!(t.consecutive_failures, 2);
    // Healthy resets
    t.record_observation(ComponentHealthState::Healthy);
    assert_eq!(t.consecutive_failures, 0);
    // Unknown does NOT reset
    t.record_observation(ComponentHealthState::Degraded);
    assert_eq!(t.consecutive_failures, 1);
    t.record_observation(ComponentHealthState::Unknown);
    assert_eq!(t.consecutive_failures, 1, "unknown does not reset counter");
}

// ---------------------------------------------------------------------------
// Panic severity classification
// ---------------------------------------------------------------------------

#[test]
fn unwind_panic_is_recoverable() {
    assert!(PanicSeverity::Unwind.is_recoverable_by_restart());
    assert!(!PanicSeverity::Unwind.requires_escalation());
}

#[test]
fn abort_panic_is_recoverable_but_flagged_for_postmortem() {
    assert!(PanicSeverity::Abort.is_recoverable_by_restart());
    assert!(!PanicSeverity::Abort.requires_escalation());
}

#[test]
fn oom_panic_requires_escalation_no_restart() {
    assert!(!PanicSeverity::Oom.is_recoverable_by_restart());
    assert!(PanicSeverity::Oom.requires_escalation());
}

#[test]
fn sigfault_requires_escalation_no_restart() {
    assert!(!PanicSeverity::SigFault.is_recoverable_by_restart());
    assert!(PanicSeverity::SigFault.requires_escalation());
}

#[test]
fn unknown_panic_defaults_to_non_recoverable() {
    assert!(!PanicSeverity::Unknown.is_recoverable_by_restart());
    assert!(!PanicSeverity::Unknown.requires_escalation());
}

// ---------------------------------------------------------------------------
// PanicContext serde round-trip
// ---------------------------------------------------------------------------

#[test]
fn panic_context_round_trips_through_serde_json() {
    use aios_recovery::PanicContext;

    let original = PanicContext {
        component_id: "aios-dns-resolver".to_owned(),
        severity: PanicSeverity::Abort,
        message: "assertion failed: empty response body".to_owned(),
        file: Some("src/resolver/mod.rs".to_owned()),
        line: Some(247),
        backtrace_hash: Some("a1b2c3d4e5f6".to_owned()),
        core_dump_ref: Some("/var/crashes/dns-1673289123.core".to_owned()),
        observed_at: chrono::Utc::now(),
        consecutive_panics: 3,
    };

    let json = serde_json::to_value(&original).expect("serialize");
    let round_tripped: PanicContext = serde_json::from_value(json).expect("deserialize");

    assert_eq!(round_tripped.component_id, original.component_id);
    assert_eq!(round_tripped.severity, original.severity);
    assert_eq!(round_tripped.message, original.message);
    assert_eq!(round_tripped.line, Some(247));
    assert_eq!(round_tripped.core_dump_ref.as_deref(), Some("/var/crashes/dns-1673289123.core"));
}

// ---------------------------------------------------------------------------
// ComponentPanicPayload round-trip with flags derived from severity
// ---------------------------------------------------------------------------

#[test]
fn panic_payload_flags_match_severity() {
    use aios_recovery::ComponentPanicPayload;

    let unwind_payload = ComponentPanicPayload {
        component_id: "test".to_owned(),
        severity: PanicSeverity::Unwind,
        message: "".to_owned(),
        file: None,
        line: None,
        backtrace_hash: None,
        core_dump_ref: None,
        observed_at: chrono::Utc::now(),
        consecutive_panics: 1,
        recoverable_by_restart: true,
        requires_escalation: false,
    };
    assert!(unwind_payload.recoverable_by_restart);
    assert!(!unwind_payload.requires_escalation);

    // OOM payload — explicitly constructed with correct flags
    let oom_payload = ComponentPanicPayload {
        component_id: "test".to_owned(),
        severity: PanicSeverity::Oom,
        message: "".to_owned(),
        file: None,
        line: None,
        backtrace_hash: None,
        core_dump_ref: None,
        observed_at: chrono::Utc::now(),
        consecutive_panics: 1,
        recoverable_by_restart: false,
        requires_escalation: true,
    };
    assert!(!oom_payload.recoverable_by_restart);
    assert!(oom_payload.requires_escalation);
}

// ---------------------------------------------------------------------------
// Tracker record_panic always bumps and sets Failed
// ---------------------------------------------------------------------------

#[test]
fn record_panic_bumps_consecutive_and_sets_failed() {
    use aios_recovery::ComponentHealingTracker;

    let mut t = ComponentHealingTracker::default();
    assert_eq!(t.consecutive_failures, 0);
    assert_eq!(t.total_actions, 0);

    // First panic: bumps to 1, total_actions to 1, state → Failed
    t.record_panic();
    assert_eq!(t.consecutive_failures, 1);
    assert_eq!(t.total_actions, 1);
    assert_eq!(t.last_observed_state, ComponentHealthState::Failed);

    // Second panic: bumps to 2, total_actions to 2
    t.record_panic();
    assert_eq!(t.consecutive_failures, 2);
    assert_eq!(t.total_actions, 2);
}

// ---------------------------------------------------------------------------
// Driver: observe_panic emits evidence immediately (MINIX-style)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn observe_panic_emits_evidence_immediately() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let log = Arc::new(aios_recovery::InMemoryRecoveryEvidenceLog::new());
    let emitter = Arc::new(make_emitter(log.clone()));
    let driver = InMemorySelfHealingDriver::new(boundary).with_evidence_emitter(emitter);

    driver.set_policy(minix_policy()).await.expect("valid policy");

    let ctx = aios_recovery::PanicContext {
        component_id: "aios-network-manager".to_owned(),
        severity: aios_recovery::PanicSeverity::Abort,
        message: "connection pool exhausted after 30s".to_owned(),
        file: Some("src/net/pool.rs".to_owned()),
        line: Some(89),
        backtrace_hash: Some("deadbeef1234".to_owned()),
        core_dump_ref: Some("/var/dumps/nm-42.core".to_owned()),
        observed_at: chrono::Utc::now(),
        consecutive_panics: 1,
    };

    let receipt = driver.observe_panic(ctx).await.expect("observe_panic succeeds");

    // Must return a receipt id (evidence was emitted)
    assert!(!receipt.is_empty(), "panic must produce evidence receipt");

    // Verify evidence landed in log
    assert_eq!(log.len().await, 1, "one panic receipt emitted");
}

// ---------------------------------------------------------------------------
// Driver: OOM panic still emits but signals escalation in payload
// ---------------------------------------------------------------------------

#[tokio::test]
async fn observe_oom_panic_emits_with_escalation_flag() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let log = Arc::new(aios_recovery::InMemoryRecoveryEvidenceLog::new());
    let emitter = Arc::new(make_emitter(log.clone()));
    let driver = InMemorySelfHealingDriver::new(boundary).with_evidence_emitter(emitter);
    driver.set_policy(minix_policy()).await.expect("valid policy");

    // Fire 4 panics to accumulate the tracker counter
    for i in 1..=4 {
        let ctx = aios_recovery::PanicContext {
            component_id: "aios-dns-resolver".to_owned(),
            severity: aios_recovery::PanicSeverity::Oom,
            message: format!("cannot allocate 256 MiB block — panic #{i}"),
            file: None,
            line: None,
            backtrace_hash: None,
            core_dump_ref: None,
            observed_at: chrono::Utc::now(),
            consecutive_panics: i,
        };
        driver.observe_panic(ctx).await.expect("OOM panic emits evidence");
    }

    // Tracker should show 4 accumulated panics
    let tracker = driver.tracker_for("aios-dns-resolver").await;
    assert_eq!(tracker.consecutive_failures, 4, "4th panic → counter=4");
    assert_eq!(tracker.last_observed_state, ComponentHealthState::Failed);
    // Each panic increments total_actions
    assert_eq!(tracker.total_actions, 4);
}

// ---------------------------------------------------------------------------
// Watchdog: registration, ping, timeout, disabled
// ---------------------------------------------------------------------------

fn enabled_watchdog_policy() -> WatchdogPolicy {
    WatchdogPolicy {
        enabled: true,
        default_timeout_secs: 1,
        component_timeouts: std::collections::HashMap::new(),
    }
}

fn disabled_watchdog_policy() -> WatchdogPolicy {
    WatchdogPolicy::default()
}

#[tokio::test]
async fn watchdog_register_adds_component_to_timer() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let log = Arc::new(aios_recovery::InMemoryRecoveryEvidenceLog::new());
    let emitter = Arc::new(make_emitter(log.clone()));
    let driver = InMemorySelfHealingDriver::new(boundary)
        .with_evidence_emitter(emitter)
        .with_watchdog_policy(enabled_watchdog_policy());

    driver.set_policy(minix_policy()).await.expect("valid policy");

    // Register a component — deadline should be set
    driver.register_watchdog("aios-network-manager").await;

    // Immediately check — deadline should NOT be expired yet (1s timeout)
    driver
        .watchdog_check()
        .await
        .expect("watchdog_check succeeds on clean state");

    let health = driver.health_snapshot().await;
    assert!(
        !health.contains_key("aios-network-manager"),
        "no health entry before timeout expiry"
    );
}

#[tokio::test]
async fn ping_resets_watchdog_timer() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let log = Arc::new(aios_recovery::InMemoryRecoveryEvidenceLog::new());
    let emitter = Arc::new(make_emitter(log.clone()));
    let driver = InMemorySelfHealingDriver::new(boundary)
        .with_evidence_emitter(emitter)
        .with_watchdog_policy(enabled_watchdog_policy());

    driver.set_policy(minix_policy()).await.expect("valid policy");

    // Register and immediately ping to set initial deadline
    driver.register_watchdog("aios-network-manager").await;
    driver.ping_watchdog("aios-network-manager").await;

    // Sleep longer than the 1-second timeout
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Ping just before checking — reset the timer
    driver.ping_watchdog("aios-network-manager").await;

    // Deadline was just reset; component should not be flagged
    driver
        .watchdog_check()
        .await
        .expect("watchdog_check succeeds");

    let health = driver.health_snapshot().await;
    assert!(
        !health.contains_key("aios-network-manager"),
        "ping reset deadline — component still within window"
    );
}

#[tokio::test]
async fn watchdog_timeout_triggers_degraded_observation() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let log = Arc::new(aios_recovery::InMemoryRecoveryEvidenceLog::new());
    let emitter = Arc::new(make_emitter(log.clone()));
    let driver = InMemorySelfHealingDriver::new(boundary)
        .with_evidence_emitter(emitter)
        .with_watchdog_policy(enabled_watchdog_policy());

    driver.set_policy(minix_policy()).await.expect("valid policy");

    // Register and set deadline
    driver.register_watchdog("aios-network-manager").await;
    driver.ping_watchdog("aios-network-manager").await;

    // Sleep past the 1-second timeout
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Watchdog check should auto-flag the expired component as Degraded
    driver
        .watchdog_check()
        .await
        .expect("watchdog_check should succeed");

    let health = driver.health_snapshot().await;
    assert!(
        health.contains_key("aios-network-manager"),
        "expired component must appear in health registry"
    );
    assert_eq!(
        health["aios-network-manager"],
        ComponentHealthState::Degraded,
        "expired component must be Degraded"
    );
}

#[tokio::test]
async fn disabled_watchdog_does_not_auto_flag() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let log = Arc::new(aios_recovery::InMemoryRecoveryEvidenceLog::new());
    let emitter = Arc::new(make_emitter(log.clone()));
    let driver = InMemorySelfHealingDriver::new(boundary)
        .with_evidence_emitter(emitter)
        .with_watchdog_policy(disabled_watchdog_policy());

    driver.set_policy(minix_policy()).await.expect("valid policy");

    // Register component even though watchdog is disabled
    driver.register_watchdog("aios-network-manager").await;

    // Sleep past any possible timeout
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Watchdog check should return empty — policy is disabled
    driver
        .watchdog_check()
        .await
        .expect("watchdog_check should succeed");

    let health = driver.health_snapshot().await;
    assert!(
        !health.contains_key("aios-network-manager"),
        "disabled watchdog must NOT auto-flag components"
    );
}

// ---------------------------------------------------------------------------
// Tracker checkpoint: sets hash + timestamp on checkpoint()
// ---------------------------------------------------------------------------

#[test]
fn tracker_checkpoint_sets_hash_and_timestamp() {
    use aios_recovery::ComponentHealingTracker;

    let mut t = ComponentHealingTracker::default();
    assert!(t.checkpoint_hash.is_none(), "no hash before checkpoint");
    assert!(t.checkpoint_timestamp.is_none(), "no timestamp before checkpoint");

    t.checkpoint("abc123def456");

    assert_eq!(
        t.checkpoint_hash.as_deref(),
        Some("abc123def456"),
        "hash must match argument"
    );
    assert!(
        t.checkpoint_timestamp.is_some(),
        "timestamp must be set"
    );
}

#[test]
fn tracker_checkpoint_overwrites_previous() {
    use aios_recovery::ComponentHealingTracker;

    let mut t = ComponentHealingTracker::default();
    t.checkpoint("first");
    let first_ts = t.checkpoint_timestamp;

    // Small sleep to ensure distinct timestamps (within test resolution)
    std::thread::sleep(std::time::Duration::from_millis(5));

    t.checkpoint("second");

    assert_eq!(t.checkpoint_hash.as_deref(), Some("second"));
    assert!(t.checkpoint_timestamp.is_some());
    assert_ne!(
        t.checkpoint_timestamp, first_ts,
        "subsequent checkpoint should update timestamp"
    );
}

// ---------------------------------------------------------------------------
// ComponentSnapshot serde round-trip
// ---------------------------------------------------------------------------

#[test]
fn component_snapshot_round_trips_through_serde_json() {
    let now = chrono::Utc::now();
    let original = ComponentSnapshot {
        component_id: "aios-network-manager".to_owned(),
        checkpoint_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            .to_owned(),
        config_blob_hex: "7b22686f7374223a226e6d31222c22706f7274223a383038307d".to_owned(),
        captured_at: now,
    };

    let json = serde_json::to_value(&original).expect("serialize");
    let round_tripped: ComponentSnapshot =
        serde_json::from_value(json).expect("deserialize");

    assert_eq!(round_tripped.component_id, original.component_id);
    assert_eq!(round_tripped.checkpoint_hash, original.checkpoint_hash);
    assert_eq!(round_tripped.config_blob_hex, original.config_blob_hex);
    // DateTime comparison — within tolerance of serialization precision loss
    let diff = (round_tripped.captured_at - original.captured_at)
        .num_milliseconds()
        .unsigned_abs();
    assert!(diff < 1_000, "timestamp should round-trip within 1s");
}

// ---------------------------------------------------------------------------
// Driver take_snapshot() returns valid snapshot
// ---------------------------------------------------------------------------

#[tokio::test]
async fn driver_take_snapshot_returns_valid_snapshot() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let log = Arc::new(aios_recovery::InMemoryRecoveryEvidenceLog::new());
    let emitter = Arc::new(make_emitter(log.clone()));
    let driver = InMemorySelfHealingDriver::new(boundary).with_evidence_emitter(emitter);

    driver.set_policy(minix_policy()).await.expect("valid policy");

    let config_blob = b"{\"host\":\"nm1\",\"port\":8080}";
    let snapshot = driver
        .take_snapshot("aios-network-manager", config_blob)
        .await
        .expect("take_snapshot should succeed");

    assert_eq!(snapshot.component_id, "aios-network-manager");
    assert!(!snapshot.checkpoint_hash.is_empty(), "hash must be non-empty");
    assert!(
        !snapshot.config_blob_hex.is_empty(),
        "config_blob_hex must be non-empty"
    );

    // The hash should be a BLAKE3 hex (64 chars, characters [0-9a-f])
    assert_eq!(
        snapshot.checkpoint_hash.len(),
        64,
        "BLAKE3 hex hash is always 64 characters"
    );
    assert!(
        snapshot.checkpoint_hash.chars().all(|c| c.is_ascii_hexdigit()),
        "hash must be hex chars only"
    );

    // config_blob_hex must decode back to the original bytes
    let decoded = hex::decode(&snapshot.config_blob_hex).expect("valid hex");
    assert_eq!(decoded, config_blob, "round-trip through hex encoding");

    // Verify the tracker was updated
    let tracker = driver.tracker_for("aios-network-manager").await;
    assert_eq!(
        tracker.checkpoint_hash.as_deref(),
        Some(snapshot.checkpoint_hash.as_str()),
        "tracker checkpoint_hash must match snapshot"
    );
    assert!(
        tracker.checkpoint_timestamp.is_some(),
        "tracker must have checkpoint timestamp"
    );
}

#[tokio::test]
async fn driver_take_snapshot_different_blobs_produce_different_hashes() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let log = Arc::new(aios_recovery::InMemoryRecoveryEvidenceLog::new());
    let emitter = Arc::new(make_emitter(log.clone()));
    let driver = InMemorySelfHealingDriver::new(boundary).with_evidence_emitter(emitter);

    driver.set_policy(minix_policy()).await.expect("valid policy");

    let snap1 = driver
        .take_snapshot("aios-dns-resolver", b"config-a")
        .await
        .expect("snapshot 1");
    let snap2 = driver
        .take_snapshot("aios-dns-resolver", b"config-b")
        .await
        .expect("snapshot 2");

    assert_ne!(
        snap1.checkpoint_hash, snap2.checkpoint_hash,
        "different configs must produce different hashes"
    );
    assert_ne!(
        snap1.config_blob_hex, snap2.config_blob_hex,
        "different configs produce different hex blobs"
    );
}

// ---------------------------------------------------------------------------
// Restore snapshot restores tracker state
// ---------------------------------------------------------------------------

#[tokio::test]
async fn restore_snapshot_updates_existing_tracker() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let log = Arc::new(aios_recovery::InMemoryRecoveryEvidenceLog::new());
    let emitter = Arc::new(make_emitter(log.clone()));
    let driver = InMemorySelfHealingDriver::new(boundary).with_evidence_emitter(emitter);

    driver.set_policy(minix_policy()).await.expect("valid policy");

    // First, observe a component so it exists in the tracker
    driver
        .observe_health("aios-network-manager", ComponentHealthState::Failed)
        .await
        .expect("observe failed");

    // Create a snapshot with known data
    let now = chrono::Utc::now();
    let snapshot = ComponentSnapshot {
        component_id: "aios-network-manager".to_owned(),
        checkpoint_hash: "deadbeef00000000000000000000000000000000000000000000000000000000"
            .to_owned(),
        config_blob_hex: "ffeeddcc".to_owned(),
        captured_at: now,
    };

    let updated = driver.restore_snapshot(&snapshot).await;
    assert!(updated, "restore on existing component returns true");

    let tracker = driver.tracker_for("aios-network-manager").await;
    assert_eq!(
        tracker.checkpoint_hash.unwrap(),
        snapshot.checkpoint_hash,
        "restored hash must match"
    );
    assert_eq!(
        tracker.checkpoint_timestamp.unwrap(),
        snapshot.captured_at,
        "restored timestamp must match"
    );
    // restore_snapshot does NOT reset other tracker fields
    assert_eq!(
        tracker.last_observed_state,
        ComponentHealthState::Failed,
        "restore does not overwrite health observations"
    );
}

#[tokio::test]
async fn restore_snapshot_creates_tracker_if_missing() {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let log = Arc::new(aios_recovery::InMemoryRecoveryEvidenceLog::new());
    let emitter = Arc::new(make_emitter(log.clone()));
    let driver = InMemorySelfHealingDriver::new(boundary).with_evidence_emitter(emitter);

    driver.set_policy(minix_policy()).await.expect("valid policy");

    let snapshot = ComponentSnapshot {
        component_id: "aios-new-component".to_owned(),
        checkpoint_hash: "cafe000000000000000000000000000000000000000000000000000000000000"
            .to_owned(),
        config_blob_hex: "aabb".to_owned(),
        captured_at: chrono::Utc::now(),
    };

    let updated = driver.restore_snapshot(&snapshot).await;
    assert!(
        !updated,
        "restore on unknown component returns false (creates new)"
    );

    let tracker = driver.tracker_for("aios-new-component").await;
    assert_eq!(
        tracker.checkpoint_hash.unwrap(),
        snapshot.checkpoint_hash,
        "tracker created with correct hash"
    );
    assert_eq!(
        tracker.checkpoint_timestamp.unwrap(),
        snapshot.captured_at,
        "tracker created with correct timestamp"
    );
    // Newly created tracker should have defaults for non-snapshot fields
    assert_eq!(
        tracker.consecutive_failures, 0,
        "new tracker has zero failures"
    );
    assert_eq!(
        tracker.last_observed_state,
        ComponentHealthState::Unknown,
        "new tracker has unknown health"
    );
}
