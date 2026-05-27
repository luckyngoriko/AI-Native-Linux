//! Integration tests for `SessionDriver` trait and `InMemorySessionDriver`.
//!
//! Covers session lifecycle: open, get, heartbeat, list, close, timeout,
//! and concurrent operations.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::time::Duration;

use aios_apps::{
    EcosystemRuntime, InMemorySessionDriver, OpenSessionRequest, PackageId, Principal,
    SessionDriver, SessionFilter, SessionId, SessionState,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_request() -> OpenSessionRequest {
    OpenSessionRequest {
        package_id: PackageId(format!(
            "pkg_{}",
            ulid::Ulid::new().to_string().to_lowercase()
        )),
        ecosystem: EcosystemRuntime::RuntimeLinuxNative,
        requester: Principal {
            canonical_id: "human:test".into(),
        },
        capability_grants: vec![],
        timeout: Duration::from_mins(5),
    }
}

fn request_with_ecosystem(ecosystem: EcosystemRuntime) -> OpenSessionRequest {
    OpenSessionRequest {
        ecosystem,
        ..default_request()
    }
}

// ---------------------------------------------------------------------------
// 1. open → Active session returned
// ---------------------------------------------------------------------------

#[tokio::test]
async fn open_session_returns_active_session() {
    let driver = InMemorySessionDriver::new_with_defaults();
    let desc = driver
        .open_session(default_request())
        .await
        .expect("open should succeed");
    assert_eq!(desc.state, SessionState::Active);
    assert_eq!(desc.ecosystem, EcosystemRuntime::RuntimeLinuxNative);
    assert_eq!(desc.requester.canonical_id, "human:test");
    assert_eq!(desc.timeout_seconds, 300);
}

// ---------------------------------------------------------------------------
// 2. open with mismatched ecosystem (no adapter) → error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn open_session_with_unregistered_ecosystem_returns_error() {
    let driver = InMemorySessionDriver::new_with_defaults();
    let req = request_with_ecosystem(EcosystemRuntime::RuntimeMacosVm);
    let err = driver.open_session(req).await.unwrap_err();
    // ProfileNotFound is the closest variant for "no adapter for ecosystem".
    assert!(matches!(err, aios_apps::AppsError::ProfileNotFound { .. }));
}

// ---------------------------------------------------------------------------
// 3. get_session known → success
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_session_known_returns_descriptor() {
    let driver = InMemorySessionDriver::new_with_defaults();
    let desc = driver
        .open_session(default_request())
        .await
        .expect("open should succeed");
    let fetched = driver
        .get_session(desc.session_id.clone())
        .await
        .expect("get should succeed");
    assert_eq!(fetched.session_id, desc.session_id);
    assert_eq!(fetched.state, SessionState::Active);
}

// ---------------------------------------------------------------------------
// 4. get_session unknown → SessionNotFound
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_session_unknown_returns_not_found() {
    let driver = InMemorySessionDriver::new_with_defaults();
    let unknown_id = SessionId("sess_nonexistent".into());
    let err = driver.get_session(unknown_id).await.unwrap_err();
    assert!(matches!(err, aios_apps::AppsError::SessionNotFound(_)));
}

// ---------------------------------------------------------------------------
// 5. close_session → SessionTerminationReceipt with ClosedByOwner
// ---------------------------------------------------------------------------

#[tokio::test]
async fn close_session_returns_receipt_with_closed_by_owner() {
    let driver = InMemorySessionDriver::new_with_defaults();
    let desc = driver
        .open_session(default_request())
        .await
        .expect("open should succeed");
    let receipt = driver
        .close_session(desc.session_id.clone())
        .await
        .expect("close should succeed");
    assert_eq!(receipt.session_id, desc.session_id);
    assert!(matches!(
        receipt.exit_reason,
        aios_apps::SessionExitReason::ClosedByOwner
    ));
    assert!(receipt.final_metrics.total_uptime_seconds <= 1);
}

// ---------------------------------------------------------------------------
// 6. close non-existent → SessionNotFound
// ---------------------------------------------------------------------------

#[tokio::test]
async fn close_session_nonexistent_returns_not_found() {
    let driver = InMemorySessionDriver::new_with_defaults();
    let unknown_id = SessionId("sess_nonexistent".into());
    let err = driver.close_session(unknown_id).await.unwrap_err();
    assert!(matches!(err, aios_apps::AppsError::SessionNotFound(_)));
}

// ---------------------------------------------------------------------------
// 7. heartbeat keeps session Active
// ---------------------------------------------------------------------------

#[tokio::test]
async fn heartbeat_keeps_session_active() {
    let driver = InMemorySessionDriver::new_with_defaults();
    let desc = driver
        .open_session(default_request())
        .await
        .expect("open should succeed");
    driver
        .heartbeat(desc.session_id.clone())
        .await
        .expect("heartbeat should succeed");
    let fetched = driver
        .get_session(desc.session_id.clone())
        .await
        .expect("get should succeed");
    assert_eq!(fetched.state, SessionState::Active);
}

// ---------------------------------------------------------------------------
// 8. list_sessions(All) returns all open
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_sessions_all_returns_all_open() {
    let driver = InMemorySessionDriver::new_with_defaults();
    let d1 = driver
        .open_session(default_request())
        .await
        .expect("open 1");
    let d2 = driver
        .open_session(default_request())
        .await
        .expect("open 2");
    let d3 = driver
        .open_session(default_request())
        .await
        .expect("open 3");
    let all = driver.list_sessions(SessionFilter::All).await;
    assert_eq!(all.len(), 3);
    let ids: Vec<_> = all.iter().map(|d| d.session_id.clone()).collect();
    assert!(ids.contains(&d1.session_id));
    assert!(ids.contains(&d2.session_id));
    assert!(ids.contains(&d3.session_id));
}

// ---------------------------------------------------------------------------
// 9. list_sessions(ByPackage) filters
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_sessions_by_package_filters() {
    let driver = InMemorySessionDriver::new_with_defaults();
    let pkg_a = PackageId("pkg_aaa".into());
    let pkg_b = PackageId("pkg_bbb".into());

    let mut req_a = default_request();
    req_a.package_id = pkg_a.clone();
    driver.open_session(req_a.clone()).await.expect("open a1");
    driver.open_session(req_a).await.expect("open a2");

    let mut req_b = default_request();
    req_b.package_id = pkg_b.clone();
    driver.open_session(req_b).await.expect("open b");

    let a_sessions = driver.list_sessions(SessionFilter::ByPackage(pkg_a)).await;
    assert_eq!(a_sessions.len(), 2);

    let b_sessions = driver.list_sessions(SessionFilter::ByPackage(pkg_b)).await;
    assert_eq!(b_sessions.len(), 1);
}

// ---------------------------------------------------------------------------
// 10. list_sessions(ByPrincipal) filters
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_sessions_by_principal_filters() {
    let driver = InMemorySessionDriver::new_with_defaults();
    let alice = Principal {
        canonical_id: "human:alice".into(),
    };
    let bob = Principal {
        canonical_id: "human:bob".into(),
    };

    let mut req_a = default_request();
    req_a.requester = alice.clone();
    driver.open_session(req_a).await.expect("open alice");

    let mut req_b = default_request();
    req_b.requester = bob.clone();
    driver.open_session(req_b).await.expect("open bob");

    let alice_sessions = driver
        .list_sessions(SessionFilter::ByPrincipal(alice))
        .await;
    assert_eq!(alice_sessions.len(), 1);

    let bob_sessions = driver.list_sessions(SessionFilter::ByPrincipal(bob)).await;
    assert_eq!(bob_sessions.len(), 1);
}

// ---------------------------------------------------------------------------
// 11. list_sessions(ByState(Active)) filters
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_sessions_by_state_filters() {
    let driver = InMemorySessionDriver::new_with_defaults();
    let d1 = driver
        .open_session(default_request())
        .await
        .expect("open 1");
    driver.close_session(d1.session_id).await.expect("close 1");

    driver
        .open_session(default_request())
        .await
        .expect("open 2");

    let active = driver
        .list_sessions(SessionFilter::ByState(SessionState::Active))
        .await;
    assert_eq!(active.len(), 1);

    let terminated = driver
        .list_sessions(SessionFilter::ByState(SessionState::Terminated))
        .await;
    assert_eq!(terminated.len(), 1);
}

// ---------------------------------------------------------------------------
// 12. concurrent open from 5 tasks → no panic, distinct SessionIds
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_open_from_five_tasks_no_panic_distinct_ids() {
    use std::sync::Arc;

    let driver = Arc::new(InMemorySessionDriver::new_with_defaults());
    let mut handles = vec![];

    for _ in 0..5 {
        let d = Arc::clone(&driver);
        let req = default_request();
        handles.push(tokio::spawn(async move { d.open_session(req).await }));
    }

    let mut ids = vec![];
    for handle in handles {
        let desc = handle.await.expect("join").expect("open should succeed");
        ids.push(desc.session_id);
    }

    // All ids must be distinct.
    let unique_count = {
        let mut sorted = ids.iter().map(|id| id.0.clone()).collect::<Vec<_>>();
        sorted.sort();
        sorted.dedup();
        sorted.len()
    };
    assert_eq!(unique_count, 5, "expected 5 distinct session ids");
}

// ---------------------------------------------------------------------------
// 13. E2E: open → list → heartbeat → close → list shows Terminated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_open_list_heartbeat_close_list_shows_terminated() {
    let driver = InMemorySessionDriver::new_with_defaults();
    let desc = driver.open_session(default_request()).await.expect("open");

    let all_before = driver.list_sessions(SessionFilter::All).await;
    assert_eq!(all_before.len(), 1);
    assert_eq!(all_before[0].state, SessionState::Active);

    driver
        .heartbeat(desc.session_id.clone())
        .await
        .expect("heartbeat");

    let receipt = driver
        .close_session(desc.session_id.clone())
        .await
        .expect("close");
    assert!(matches!(
        receipt.exit_reason,
        aios_apps::SessionExitReason::ClosedByOwner
    ));

    let all_after = driver.list_sessions(SessionFilter::All).await;
    assert_eq!(all_after.len(), 1);
    assert_eq!(all_after[0].state, SessionState::Terminated);
}

// ---------------------------------------------------------------------------
// 14. timeout: open with zero Duration → reaches Terminated on next operation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn timeout_zero_duration_terminates_on_next_operation() {
    let driver = InMemorySessionDriver::new_with_defaults();
    let mut req = default_request();
    req.timeout = Duration::ZERO;
    let desc = driver.open_session(req).await.expect("open");

    // Session was created Active, but timeout=0 means immediate expiry.
    // On the next operation (get_session), the lazy timeout check fires.
    let fetched = driver
        .get_session(desc.session_id)
        .await
        .expect("get should succeed");
    assert_eq!(fetched.state, SessionState::Terminated);
}
