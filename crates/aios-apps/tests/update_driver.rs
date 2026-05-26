//! Integration tests for `UpdateRollbackDriver` trait and `InMemoryUpdateDriver`.
//!
//! Covers plan → execute → verify → activate happy path, rollback paths,
//! FSM enforcement, dry-run semantics, and concurrent operations.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use aios_apps::{
    AppsError, InMemoryUpdateDriver, PackageId, Principal, RollbackExitState, RollbackReason,
    UpdatePlanRequest, UpdateRollbackDriver, UpdateState,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_request() -> UpdatePlanRequest {
    UpdatePlanRequest {
        package_id: PackageId(format!(
            "pkg_{}",
            ulid::Ulid::new().to_string().to_lowercase()
        )),
        from_version: "1.0.0".into(),
        to_version: "2.0.0".into(),
        requester: Principal {
            canonical_id: "human:test".into(),
        },
        dry_run: false,
    }
}

// ---------------------------------------------------------------------------
// 1. plan_update returns Planned state
// ---------------------------------------------------------------------------

#[tokio::test]
async fn plan_update_returns_planned_state() {
    let driver = InMemoryUpdateDriver::new();
    let plan = driver
        .plan_update(default_request())
        .await
        .expect("plan should succeed");
    assert_eq!(plan.state, UpdateState::Planned);
    assert_eq!(plan.from_version, "1.0.0");
    assert_eq!(plan.to_version, "2.0.0");
}

// ---------------------------------------------------------------------------
// 2. execute on Planned transitions to Executed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_on_planned_transitions_to_executed() {
    let driver = InMemoryUpdateDriver::new();
    let plan = driver.plan_update(default_request()).await.unwrap();
    let plan_id = plan.id.clone();
    let outcome = driver
        .execute_update(plan_id.clone())
        .await
        .expect("execute should succeed");
    assert!(outcome.artifacts_swapped > 0);
    let fetched = driver.get_update(plan_id).await.unwrap();
    assert_eq!(fetched.state, UpdateState::Executed);
}

// ---------------------------------------------------------------------------
// 3. verify on Executed transitions to Verified
// ---------------------------------------------------------------------------

#[tokio::test]
async fn verify_on_executed_transitions_to_verified() {
    let driver = InMemoryUpdateDriver::new();
    let plan = driver.plan_update(default_request()).await.unwrap();
    driver.execute_update(plan.id.clone()).await.unwrap();
    let verification = driver
        .verify_update(plan.id.clone())
        .await
        .expect("verify should succeed");
    assert!(verification.hash_match);
    assert_eq!(verification.profile_compat, 100);
    let fetched = driver.get_update(plan.id).await.unwrap();
    assert_eq!(fetched.state, UpdateState::Verified);
}

// ---------------------------------------------------------------------------
// 4. activate on Verified transitions to Active
// ---------------------------------------------------------------------------

#[tokio::test]
async fn activate_on_verified_transitions_to_active() {
    let driver = InMemoryUpdateDriver::new();
    let plan = driver.plan_update(default_request()).await.unwrap();
    driver.execute_update(plan.id.clone()).await.unwrap();
    driver.verify_update(plan.id.clone()).await.unwrap();
    driver
        .activate_update(plan.id.clone())
        .await
        .expect("activate should succeed");
    let fetched = driver.get_update(plan.id).await.unwrap();
    assert_eq!(fetched.state, UpdateState::Active);
}

// ---------------------------------------------------------------------------
// 5. dry_run plan does not persist
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dry_run_plan_does_not_persist() {
    let driver = InMemoryUpdateDriver::new();
    let mut req = default_request();
    req.dry_run = true;
    let plan = driver.plan_update(req).await.expect("plan should succeed");
    assert_eq!(plan.state, UpdateState::Planned);
    let err = driver.get_update(plan.id).await.unwrap_err();
    assert!(matches!(err, AppsError::UpdatePlanNotFound(_)));
}

// ---------------------------------------------------------------------------
// 6. execute on non-existent plan → UpdatePlanNotFound
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_on_nonexistent_plan_returns_not_found() {
    let driver = InMemoryUpdateDriver::new();
    let unknown_id = aios_apps::UpdatePlanId("updp_nonexistent".into());
    let err = driver.execute_update(unknown_id).await.unwrap_err();
    assert!(matches!(err, AppsError::UpdatePlanNotFound(_)));
}

// ---------------------------------------------------------------------------
// 7. execute on already-Executed → InvalidStateTransition
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_on_already_executed_returns_invalid_transition() {
    let driver = InMemoryUpdateDriver::new();
    let plan = driver.plan_update(default_request()).await.unwrap();
    driver.execute_update(plan.id.clone()).await.unwrap();
    // Second execute attempts Executed → Executing, which is illegal.
    let err = driver.execute_update(plan.id).await.unwrap_err();
    assert!(matches!(err, AppsError::InvalidStateTransition { .. }));
}

// ---------------------------------------------------------------------------
// 8. activate on Executed (skipped verify) → InvalidStateTransition
// ---------------------------------------------------------------------------

#[tokio::test]
async fn activate_on_executed_skipped_verify_returns_invalid_transition() {
    let driver = InMemoryUpdateDriver::new();
    let plan = driver.plan_update(default_request()).await.unwrap();
    driver.execute_update(plan.id.clone()).await.unwrap();
    // Activating from Executed is illegal — must go through Verifying→Verified.
    let err = driver.activate_update(plan.id).await.unwrap_err();
    assert!(matches!(err, AppsError::InvalidStateTransition { .. }));
}

// ---------------------------------------------------------------------------
// 9. rollback from Active (regression) → RolledBack
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rollback_from_active_returns_rolled_back() {
    let driver = InMemoryUpdateDriver::new();
    let plan = driver.plan_update(default_request()).await.unwrap();
    driver.execute_update(plan.id.clone()).await.unwrap();
    driver.verify_update(plan.id.clone()).await.unwrap();
    driver.activate_update(plan.id.clone()).await.unwrap();
    let receipt = driver
        .rollback_update(plan.id.clone(), RollbackReason::RegressionDetected)
        .await
        .expect("rollback should succeed");
    assert_eq!(receipt.reverted_to, "1.0.0");
    assert_eq!(receipt.exit_state, RollbackExitState::Reverted);
    let fetched = driver.get_update(plan.id).await.unwrap();
    assert_eq!(fetched.state, UpdateState::RolledBack);
}

// ---------------------------------------------------------------------------
// 10. rollback from Executed → RolledBack
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rollback_from_executed_returns_rolled_back() {
    let driver = InMemoryUpdateDriver::new();
    let plan = driver.plan_update(default_request()).await.unwrap();
    driver.execute_update(plan.id.clone()).await.unwrap();
    let receipt = driver
        .rollback_update(plan.id.clone(), RollbackReason::UserRequested)
        .await
        .expect("rollback should succeed");
    assert_eq!(receipt.exit_state, RollbackExitState::Reverted);
    let fetched = driver.get_update(plan.id).await.unwrap();
    assert_eq!(fetched.state, UpdateState::RolledBack);
}

// ---------------------------------------------------------------------------
// 11. rollback on already-RolledBack → InvalidStateTransition
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rollback_on_already_rolled_back_returns_invalid_transition() {
    let driver = InMemoryUpdateDriver::new();
    let plan = driver.plan_update(default_request()).await.unwrap();
    driver.execute_update(plan.id.clone()).await.unwrap();
    driver
        .rollback_update(plan.id.clone(), RollbackReason::UserRequested)
        .await
        .unwrap();
    let err = driver
        .rollback_update(plan.id, RollbackReason::UserRequested)
        .await
        .unwrap_err();
    assert!(matches!(err, AppsError::InvalidStateTransition { .. }));
}

// ---------------------------------------------------------------------------
// 12. get_update returns current state
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_update_returns_current_state_at_each_stage() {
    let driver = InMemoryUpdateDriver::new();
    let plan = driver.plan_update(default_request()).await.unwrap();

    let fetched = driver.get_update(plan.id.clone()).await.unwrap();
    assert_eq!(fetched.state, UpdateState::Planned);

    driver.execute_update(plan.id.clone()).await.unwrap();
    let fetched = driver.get_update(plan.id.clone()).await.unwrap();
    assert_eq!(fetched.state, UpdateState::Executed);

    driver.verify_update(plan.id.clone()).await.unwrap();
    let fetched = driver.get_update(plan.id.clone()).await.unwrap();
    assert_eq!(fetched.state, UpdateState::Verified);

    driver.activate_update(plan.id.clone()).await.unwrap();
    let fetched = driver.get_update(plan.id).await.unwrap();
    assert_eq!(fetched.state, UpdateState::Active);
}

// ---------------------------------------------------------------------------
// 13. concurrent plan_update → distinct plan ids
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_plan_update_produces_distinct_ids() {
    use std::sync::Arc;

    let driver = Arc::new(InMemoryUpdateDriver::new());
    let mut handles = vec![];

    for _ in 0..5 {
        let d = Arc::clone(&driver);
        let req = default_request();
        handles.push(tokio::spawn(async move { d.plan_update(req).await }));
    }

    let mut ids = vec![];
    for handle in handles {
        let plan = handle.await.expect("join").expect("plan should succeed");
        ids.push(plan.id);
    }

    let unique_count = {
        let mut sorted = ids.iter().map(|id| id.0.clone()).collect::<Vec<_>>();
        sorted.sort();
        sorted.dedup();
        sorted.len()
    };
    assert_eq!(unique_count, 5, "expected 5 distinct plan ids");
}

// ---------------------------------------------------------------------------
// 14. E2E happy path: plan → execute → verify → activate
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_happy_path_plan_execute_verify_activate() {
    let driver = InMemoryUpdateDriver::new();
    let plan = driver.plan_update(default_request()).await.unwrap();
    assert_eq!(plan.state, UpdateState::Planned);

    let outcome = driver.execute_update(plan.id.clone()).await.unwrap();
    assert!(outcome.artifacts_swapped > 0);

    let verification = driver.verify_update(plan.id.clone()).await.unwrap();
    assert!(verification.hash_match);

    driver.activate_update(plan.id.clone()).await.unwrap();

    let fetched = driver.get_update(plan.id).await.unwrap();
    assert_eq!(fetched.state, UpdateState::Active);
    assert_eq!(fetched.from_version, "1.0.0");
    assert_eq!(fetched.to_version, "2.0.0");
}

// ---------------------------------------------------------------------------
// 15. E2E rollback path: plan → execute → verify → activate → rollback
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_rollback_path_plan_execute_verify_activate_rollback() {
    let driver = InMemoryUpdateDriver::new();
    let plan = driver.plan_update(default_request()).await.unwrap();

    driver.execute_update(plan.id.clone()).await.unwrap();
    driver.verify_update(plan.id.clone()).await.unwrap();
    driver.activate_update(plan.id.clone()).await.unwrap();

    let receipt = driver
        .rollback_update(plan.id.clone(), RollbackReason::RegressionDetected)
        .await
        .expect("rollback should succeed");
    assert_eq!(receipt.reverted_to, "1.0.0");
    assert_eq!(receipt.exit_state, RollbackExitState::Reverted);

    let fetched = driver.get_update(plan.id).await.unwrap();
    assert_eq!(fetched.state, UpdateState::RolledBack);
    assert_eq!(fetched.from_version, "1.0.0");
}

// ---------------------------------------------------------------------------
// 16. verify on non-existent plan → UpdatePlanNotFound
// ---------------------------------------------------------------------------

#[tokio::test]
async fn verify_on_nonexistent_plan_returns_not_found() {
    let driver = InMemoryUpdateDriver::new();
    let unknown_id = aios_apps::UpdatePlanId("updp_nonexistent".into());
    let err = driver.verify_update(unknown_id).await.unwrap_err();
    assert!(matches!(err, AppsError::UpdatePlanNotFound(_)));
}

// ---------------------------------------------------------------------------
// 17. activate on non-existent plan → UpdatePlanNotFound
// ---------------------------------------------------------------------------

#[tokio::test]
async fn activate_on_nonexistent_plan_returns_not_found() {
    let driver = InMemoryUpdateDriver::new();
    let unknown_id = aios_apps::UpdatePlanId("updp_nonexistent".into());
    let err = driver.activate_update(unknown_id).await.unwrap_err();
    assert!(matches!(err, AppsError::UpdatePlanNotFound(_)));
}

// ---------------------------------------------------------------------------
// 18. rollback on non-existent plan → UpdatePlanNotFound
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rollback_on_nonexistent_plan_returns_not_found() {
    let driver = InMemoryUpdateDriver::new();
    let unknown_id = aios_apps::UpdatePlanId("updp_nonexistent".into());
    let err = driver
        .rollback_update(unknown_id, RollbackReason::UserRequested)
        .await
        .unwrap_err();
    assert!(matches!(err, AppsError::UpdatePlanNotFound(_)));
}
