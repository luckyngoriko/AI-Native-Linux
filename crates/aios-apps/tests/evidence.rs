//! T-123 — E2E evidence emission integration tests for aios-apps drivers.
//!
//! These tests verify that the optional evidence emitter hooks are correctly
//! wired into `InMemoryPackageStore`, `InMemoryUpdateDriver`, and
//! `InMemorySessionDriver`, and that the no-emitter path preserves backward
//! compatibility.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ed25519_dalek::Signer;
use rand_core::{OsRng, RngCore};

use aios_apps::{
    AppPackage, AppsError, InMemoryAppsEvidenceEmitter, InMemoryPackageStore,
    InMemorySessionDriver, InMemoryUpdateDriver, OpenSessionRequest, PackageId, PackageStore,
    Principal, RollbackReason, SessionDriver, SessionExitReason, SessionFilter, UpdatePlanRequest,
    UpdateRollbackDriver,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn emitter() -> Arc<InMemoryAppsEvidenceEmitter> {
    Arc::new(InMemoryAppsEvidenceEmitter::new("service:aios-apps"))
}

fn signing_key() -> ed25519_dalek::SigningKey {
    let mut secret_bytes: [u8; 32] = [0u8; 32];
    OsRng.fill_bytes(&mut secret_bytes);
    ed25519_dalek::SigningKey::from_bytes(&secret_bytes)
}

fn make_package(name: &str, version: &str, key: &ed25519_dalek::SigningKey) -> AppPackage {
    let manifest = serde_json::json!({
        "name": name,
        "version": version,
        "kind": "application",
    });
    let manifest_bytes = serde_json::to_vec(&manifest).expect("ser");
    let content_hash = blake3::hash(&manifest_bytes).to_hex().to_string();
    let sig = key.sign(&manifest_bytes);
    AppPackage {
        package_id: PackageId(format!(
            "pkg_{}",
            ulid::Ulid::new().to_string().to_lowercase()
        )),
        name: name.to_string(),
        version: version.to_string(),
        manifest_bytes,
        content_hash_blake3: content_hash,
        ed25519_signature: sig.to_vec(),
        signer_public_key: key.verifying_key().as_bytes().to_vec(),
        registered_at: chrono::Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// Package store — no-emitter backward compatibility
// ---------------------------------------------------------------------------

#[tokio::test]
async fn package_store_no_emitter_registers_and_lists() {
    let key = signing_key();
    let mut trusted = HashMap::new();
    trusted.insert(
        key.verifying_key().as_bytes().to_vec(),
        "test-authority".into(),
    );
    let store = InMemoryPackageStore::new(trusted);
    let pkg = make_package("test-app", "1.0", &key);

    let id = store.register_package(pkg.clone()).await.expect("register");
    let found = store.lookup_package(&id).await.expect("lookup");
    assert_eq!(found.name, "test-app");
}

#[tokio::test]
async fn package_store_no_emitter_preserves_count() {
    let key = signing_key();
    let mut trusted = HashMap::new();
    trusted.insert(
        key.verifying_key().as_bytes().to_vec(),
        "test-authority".into(),
    );
    let store = InMemoryPackageStore::new(trusted);

    store
        .register_package(make_package("a", "1.0", &key))
        .await
        .expect("register a");
    store
        .register_package(make_package("b", "1.0", &key))
        .await
        .expect("register b");

    assert_eq!(store.package_count().await, 2);
}

// ---------------------------------------------------------------------------
// Package store — with emitter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn package_store_with_emitter_emits_on_register() {
    let key = signing_key();
    let mut trusted = HashMap::new();
    trusted.insert(
        key.verifying_key().as_bytes().to_vec(),
        "test-authority".into(),
    );
    let em = emitter();
    let store = InMemoryPackageStore::new(trusted).with_emitter(em.clone());

    store
        .register_package(make_package("firefox", "120.0", &key))
        .await
        .expect("register");

    assert_eq!(em.receipt_count().await, 1);
    em.verify_chain().await.expect("chain ok");
}

#[tokio::test]
async fn package_store_with_emitter_increments_sequence() {
    let key = signing_key();
    let mut trusted = HashMap::new();
    trusted.insert(
        key.verifying_key().as_bytes().to_vec(),
        "test-authority".into(),
    );
    let em = emitter();
    let store = InMemoryPackageStore::new(trusted).with_emitter(em.clone());

    store
        .register_package(make_package("a", "1.0", &key))
        .await
        .expect("reg a");
    store
        .register_package(make_package("b", "2.0", &key))
        .await
        .expect("reg b");
    store
        .register_package(make_package("c", "3.0", &key))
        .await
        .expect("reg c");

    assert_eq!(em.receipt_count().await, 3);
    em.verify_chain().await.expect("chain ok");
}

#[tokio::test]
async fn package_store_register_fails_with_untrusted_authority_no_emit() {
    let key = signing_key();
    let trusted: HashMap<Vec<u8>, String> = HashMap::new();
    // Empty — no authorities trusted.
    let em = emitter();
    let store = InMemoryPackageStore::new(trusted).with_emitter(em.clone());

    let result = store
        .register_package(make_package("evil", "1.0", &key))
        .await;
    assert!(matches!(result, Err(AppsError::ValidationFailed(_))));
    // No evidence emitted because validation failed before registration.
    assert_eq!(em.receipt_count().await, 0);
}

// ---------------------------------------------------------------------------
// Update driver — no-emitter backward compatibility
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_driver_no_emitter_full_lifecycle() {
    let driver = InMemoryUpdateDriver::new();
    let plan = driver
        .plan_update(UpdatePlanRequest {
            package_id: PackageId("pkg_01".into()),
            from_version: "1.0".into(),
            to_version: "2.0".into(),
            requester: Principal {
                canonical_id: "human:test".into(),
            },
            dry_run: false,
        })
        .await
        .expect("plan");

    driver
        .execute_update(plan.id.clone())
        .await
        .expect("execute");
    driver.verify_update(plan.id.clone()).await.expect("verify");
    driver
        .activate_update(plan.id.clone())
        .await
        .expect("activate");

    let final_plan = driver.get_update(plan.id).await.expect("get");
    assert_eq!(final_plan.state.to_string(), "Active");
}

// ---------------------------------------------------------------------------
// Update driver — with emitter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_driver_with_emitter_emits_on_each_phase() {
    let em = emitter();
    let driver = InMemoryUpdateDriver::new().with_emitter(em.clone());

    let plan = driver
        .plan_update(UpdatePlanRequest {
            package_id: PackageId("pkg_01".into()),
            from_version: "1.0".into(),
            to_version: "2.0".into(),
            requester: Principal {
                canonical_id: "human:test".into(),
            },
            dry_run: false,
        })
        .await
        .expect("plan");

    driver
        .execute_update(plan.id.clone())
        .await
        .expect("execute");
    driver.verify_update(plan.id.clone()).await.expect("verify");
    driver
        .activate_update(plan.id.clone())
        .await
        .expect("activate");

    // Planned (plan_update does not emit), Executed, Verified, Activated = 3
    assert_eq!(em.receipt_count().await, 3);
    em.verify_chain().await.expect("chain ok");
}

#[tokio::test]
async fn update_driver_rollback_emits_with_reason() {
    let em = emitter();
    let driver = InMemoryUpdateDriver::new().with_emitter(em.clone());

    let plan = driver
        .plan_update(UpdatePlanRequest {
            package_id: PackageId("pkg_01".into()),
            from_version: "1.0".into(),
            to_version: "2.0".into(),
            requester: Principal {
                canonical_id: "human:test".into(),
            },
            dry_run: false,
        })
        .await
        .expect("plan");

    driver
        .execute_update(plan.id.clone())
        .await
        .expect("execute");

    driver
        .rollback_update(plan.id.clone(), RollbackReason::UserRequested)
        .await
        .expect("rollback");

    // Executed + RolledBack(UserRequested) = 2
    assert_eq!(em.receipt_count().await, 2);
    em.verify_chain().await.expect("chain ok");
}

// ---------------------------------------------------------------------------
// Session driver — no-emitter backward compatibility
// ---------------------------------------------------------------------------

#[tokio::test]
async fn session_driver_no_emitter_open_and_close() {
    let driver = InMemorySessionDriver::new_with_defaults();
    let pkg = PackageId("pkg_01".into());

    let desc = driver
        .open_session(OpenSessionRequest {
            package_id: pkg.clone(),
            ecosystem: aios_apps::EcosystemRuntime::RuntimeLinuxNative,
            requester: Principal {
                canonical_id: "human:test".into(),
            },
            capability_grants: vec![],
            timeout: Duration::from_secs(300),
        })
        .await
        .expect("open");

    let receipt = driver
        .close_session(desc.session_id.clone())
        .await
        .expect("close");
    assert_eq!(receipt.exit_reason, SessionExitReason::ClosedByOwner);
}

// ---------------------------------------------------------------------------
// Session driver — with emitter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn session_driver_with_emitter_emits_open_and_close() {
    let em = emitter();
    let driver = InMemorySessionDriver::new_with_defaults().with_emitter(em.clone());
    let pkg = PackageId("pkg_01".into());

    let desc = driver
        .open_session(OpenSessionRequest {
            package_id: pkg.clone(),
            ecosystem: aios_apps::EcosystemRuntime::RuntimeLinuxNative,
            requester: Principal {
                canonical_id: "human:test".into(),
            },
            capability_grants: vec![],
            timeout: Duration::from_secs(300),
        })
        .await
        .expect("open");

    driver
        .close_session(desc.session_id.clone())
        .await
        .expect("close");

    // Opened + Closed = 2
    assert_eq!(em.receipt_count().await, 2);
    em.verify_chain().await.expect("chain ok");
}

#[tokio::test]
async fn session_driver_close_emits_exit_reason() {
    let em = emitter();
    let driver = InMemorySessionDriver::new_with_defaults().with_emitter(em.clone());
    let pkg = PackageId("pkg_01".into());

    let desc = driver
        .open_session(OpenSessionRequest {
            package_id: pkg.clone(),
            ecosystem: aios_apps::EcosystemRuntime::RuntimeLinuxNative,
            requester: Principal {
                canonical_id: "human:test".into(),
            },
            capability_grants: vec![],
            timeout: Duration::from_secs(300),
        })
        .await
        .expect("open");

    driver
        .close_session(desc.session_id.clone())
        .await
        .expect("close");

    // Verify the closed receipt carries the exit reason.
    let close_payload = em.get_payload(1).await.expect("second receipt");
    assert_eq!(close_payload["exit_reason"], "CLOSED_BY_OWNER");
}

// ---------------------------------------------------------------------------
// Full E2E — package → update → session chain
// ---------------------------------------------------------------------------

#[tokio::test]
async fn full_e2e_package_update_session_chain() {
    let key = signing_key();
    let em = emitter();

    // Package store with emitter.
    let mut trusted = HashMap::new();
    trusted.insert(
        key.verifying_key().as_bytes().to_vec(),
        "test-authority".into(),
    );
    let store = InMemoryPackageStore::new(trusted).with_emitter(em.clone());
    let pkg = make_package("my-app", "1.0", &key);
    let pkg_id = store.register_package(pkg).await.expect("register");

    // Update driver with emitter.
    let update_driver = InMemoryUpdateDriver::new().with_emitter(em.clone());
    let plan = update_driver
        .plan_update(UpdatePlanRequest {
            package_id: pkg_id.clone(),
            from_version: "1.0".into(),
            to_version: "2.0".into(),
            requester: Principal {
                canonical_id: "human:test".into(),
            },
            dry_run: false,
        })
        .await
        .expect("plan");
    update_driver
        .execute_update(plan.id.clone())
        .await
        .expect("execute");
    update_driver
        .verify_update(plan.id.clone())
        .await
        .expect("verify");
    update_driver
        .activate_update(plan.id.clone())
        .await
        .expect("activate");

    // Session driver with emitter.
    let session_driver = InMemorySessionDriver::new_with_defaults().with_emitter(em.clone());
    let desc = session_driver
        .open_session(OpenSessionRequest {
            package_id: pkg_id.clone(),
            ecosystem: aios_apps::EcosystemRuntime::RuntimeLinuxNative,
            requester: Principal {
                canonical_id: "human:test".into(),
            },
            capability_grants: vec![],
            timeout: Duration::from_secs(300),
        })
        .await
        .expect("open");
    session_driver
        .close_session(desc.session_id.clone())
        .await
        .expect("close");

    // register(1) + execute(1) + verify(1) + activate(1) + open(1) + close(1) = 6
    assert_eq!(em.receipt_count().await, 6);
    em.verify_chain().await.expect("full chain ok");
}

// ---------------------------------------------------------------------------
// Concurrent emission — multiple drivers sharing one emitter
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_package_registrations_share_emitter() {
    let key = signing_key();
    let mut trusted = HashMap::new();
    trusted.insert(
        key.verifying_key().as_bytes().to_vec(),
        "test-authority".into(),
    );
    let em = emitter();
    let store = Arc::new(InMemoryPackageStore::new(trusted).with_emitter(em.clone()));
    let key = Arc::new(key);

    let h1 = {
        let store = store.clone();
        let key = key.clone();
        tokio::spawn(async move {
            store
                .register_package(make_package("a", "1.0", &key))
                .await
                .expect("reg a")
        })
    };
    let h2 = {
        let store = store.clone();
        let key = key.clone();
        tokio::spawn(async move {
            store
                .register_package(make_package("b", "1.0", &key))
                .await
                .expect("reg b")
        })
    };
    let h3 = {
        let store = store.clone();
        let key = key.clone();
        tokio::spawn(async move {
            store
                .register_package(make_package("c", "1.0", &key))
                .await
                .expect("reg c")
        })
    };

    let _ = tokio::join!(h1, h2, h3);
    assert_eq!(em.receipt_count().await, 3);
    em.verify_chain().await.expect("concurrent chain ok");
}

// ---------------------------------------------------------------------------
// INV-015 — no secret material in evidence payloads
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv015_no_secret_material_in_emitted_evidence() {
    let key = signing_key();
    let mut trusted = HashMap::new();
    trusted.insert(
        key.verifying_key().as_bytes().to_vec(),
        "test-authority".into(),
    );
    let em = emitter();
    let store = InMemoryPackageStore::new(trusted).with_emitter(em.clone());

    store
        .register_package(make_package("my-app", "1.0", &key))
        .await
        .expect("register");

    let payload = em.get_payload(0).await.expect("receipt present");
    let payload_str = serde_json::to_string(&payload).expect("ser");

    // No Ed25519 raw key bytes, passwords, or tokens.
    assert!(!payload_str.contains("private_key"));
    assert!(!payload_str.contains("secret"));
    assert!(!payload_str.contains("password"));
    assert!(!payload_str.contains("token"));
    assert!(!payload_str.contains("signing_key"));
    // No Ed25519 signature bytes in payload.
    assert!(!payload_str.contains("signature"));
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emitter_field_is_accessible_on_driver() {
    // Verify that the emitter field on InMemorySessionDriver does not prevent
    // normal session operations.
    let em = emitter();
    let driver = InMemorySessionDriver::new_with_defaults().with_emitter(em.clone());
    let pkg = PackageId("pkg_edge".into());

    let sessions_before = driver.list_sessions(SessionFilter::All).await;
    assert!(sessions_before.is_empty());

    let desc = driver
        .open_session(OpenSessionRequest {
            package_id: pkg.clone(),
            ecosystem: aios_apps::EcosystemRuntime::RuntimeLinuxNative,
            requester: Principal {
                canonical_id: "human:test".into(),
            },
            capability_grants: vec![],
            timeout: Duration::from_secs(300),
        })
        .await
        .expect("open");

    let sessions_after = driver.list_sessions(SessionFilter::All).await;
    assert_eq!(sessions_after.len(), 1);

    let got = driver
        .get_session(desc.session_id.clone())
        .await
        .expect("get");
    assert_eq!(got.package_id, pkg);

    driver
        .heartbeat(desc.session_id.clone())
        .await
        .expect("heartbeat");

    driver
        .close_session(desc.session_id.clone())
        .await
        .expect("close");
}

#[tokio::test]
async fn update_driver_dry_run_does_not_emit() {
    let em = emitter();
    let driver = InMemoryUpdateDriver::new().with_emitter(em.clone());

    let _plan = driver
        .plan_update(UpdatePlanRequest {
            package_id: PackageId("pkg_dry".into()),
            from_version: "1.0".into(),
            to_version: "2.0".into(),
            requester: Principal {
                canonical_id: "human:test".into(),
            },
            dry_run: true,
        })
        .await
        .expect("plan");

    // Dry-run plans are not persisted and should not emit evidence.
    assert_eq!(em.receipt_count().await, 0);
}

#[tokio::test]
async fn session_driver_unknown_ecosystem_fails_without_emit() {
    let em = emitter();
    let driver = InMemorySessionDriver::new_with_defaults().with_emitter(em.clone());

    let result = driver
        .open_session(OpenSessionRequest {
            package_id: PackageId("pkg_01".into()),
            // RuntimeMacosVm has no registered adapter in the default orchestrator.
            ecosystem: aios_apps::EcosystemRuntime::RuntimeMacosVm,
            requester: Principal {
                canonical_id: "human:test".into(),
            },
            capability_grants: vec![],
            timeout: Duration::from_secs(300),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(em.receipt_count().await, 0);
}
