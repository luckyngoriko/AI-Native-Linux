//! Cross-crate integration tests — aios-apps bridges ↔ runtime + sgr + sandbox.
//!
//! Each bridge is tested against the corresponding upstream `InMemory*`
//! implementation to verify that apps lifecycle operations translate correctly
//! into typed calls on the upstream traits.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::unwrap_in_result,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use aios_apps::{
    AppProfile, CapabilityHandle, CompatibilityRating, EcosystemHonestyClass, EcosystemRuntime,
    EvidenceLevel, PackageId, RatingDimension, RecipeTrustClass, RuntimeBridge, SandboxBridge,
    SgrBridge, UpdatePlanId,
};
use aios_capability_runtime::InMemoryCapabilityRuntime;
use aios_sandbox::{InMemorySandboxComposer, ProfileId};
use aios_sgr::{InMemoryServiceGraph, UnitId};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_package_id() -> PackageId {
    PackageId("test.pkg.v1".into())
}

fn test_app_profile() -> AppProfile {
    AppProfile {
        app_id: "test.app".into(),
        ecosystem_runtime: EcosystemRuntime::RuntimeLinuxNative,
        current_recipe_trust_class: RecipeTrustClass::RecipeCommunity,
        headline_rating: CompatibilityRating::Gold,
        headline_evidence_level: EvidenceLevel::SingleOperatorObserved,
        worst_dimension: RatingDimension::LaunchReliability,
        ecosystem_honesty_class: EcosystemHonestyClass::FullySupported,
    }
}

// ---------------------------------------------------------------------------
// RuntimeBridge tests
// ---------------------------------------------------------------------------

#[test]
fn runtime_bridge_new_constructs() {
    let runtime = Arc::new(InMemoryCapabilityRuntime::new());
    let _bridge = RuntimeBridge::new(runtime);
}

#[tokio::test]
async fn runtime_bridge_dispatch_install_succeeds() {
    let runtime = Arc::new(InMemoryCapabilityRuntime::new());
    let bridge = RuntimeBridge::new(runtime);

    let ctx = bridge
        .dispatch_install(
            &test_package_id(),
            "human:test",
            &[CapabilityHandle {
                capability_id: "cap_test".into(),
            }],
        )
        .await
        .expect("install dispatch should succeed");

    assert_eq!(
        ctx.status,
        aios_capability_runtime::ActionLifecycleState::Succeeded
    );
}

#[tokio::test]
async fn runtime_bridge_dispatch_update_activation_succeeds() {
    let runtime = Arc::new(InMemoryCapabilityRuntime::new());
    let bridge = RuntimeBridge::new(runtime);

    let plan_id = UpdatePlanId("upd_test_plan".into());

    let ctx = bridge
        .dispatch_update_activation(&plan_id, "v2.0.0", "human:test")
        .await
        .expect("update activation dispatch should succeed");

    assert_eq!(
        ctx.status,
        aios_capability_runtime::ActionLifecycleState::Succeeded
    );
}

#[tokio::test]
async fn runtime_bridge_dispatch_install_with_empty_requester_fails() {
    let runtime = Arc::new(InMemoryCapabilityRuntime::new());
    let bridge = RuntimeBridge::new(runtime);

    // Empty subject triggers step_validate → EnvelopeValidationFailed → non-Succeeded
    let result = bridge.dispatch_install(&test_package_id(), "", &[]).await;

    match result {
        Err(aios_apps::AppsError::RuntimeReject(msg)) => {
            assert!(msg.contains("install action ended"), "should report state");
        }
        other => panic!("expected RuntimeReject, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// SgrBridge tests
// ---------------------------------------------------------------------------

#[test]
fn sgr_bridge_new_constructs() {
    let graph = Arc::new(InMemoryServiceGraph::new());
    let _bridge = SgrBridge::new(graph);
}

#[tokio::test]
async fn sgr_bridge_register_service_rejects_non_service_runtime_class() {
    let graph = Arc::new(InMemoryServiceGraph::new());
    let bridge = SgrBridge::new(graph);

    let err = bridge
        .register_service(&test_package_id(), &test_app_profile(), "application")
        .await
        .expect_err("non-service runtime_class should be rejected");

    match err {
        aios_apps::AppsError::InvalidRuntimeClass(msg) => {
            assert!(
                msg.contains("application"),
                "message should mention the class"
            );
        }
        other => panic!("expected InvalidRuntimeClass, got {other:?}"),
    }
}

#[tokio::test]
async fn sgr_bridge_register_service_rejects_daemon_runtime_class() {
    let graph = Arc::new(InMemoryServiceGraph::new());
    let bridge = SgrBridge::new(graph);

    let err = bridge
        .register_service(&test_package_id(), &test_app_profile(), "daemon")
        .await
        .expect_err("non-service runtime_class should be rejected");

    assert!(matches!(err, aios_apps::AppsError::InvalidRuntimeClass(_)));
}

#[tokio::test]
async fn sgr_bridge_register_service_calls_graph_for_service_class() {
    let graph = Arc::new(InMemoryServiceGraph::new());
    let bridge = SgrBridge::new(graph);

    // With "service" class, it should proceed past the guard.
    // Use a dot-free package id so UnitId::from_parts succeeds.
    let pkg = PackageId("testpkg-v1".into());
    let result = bridge
        .register_service(&pkg, &test_app_profile(), "service")
        .await;

    match result {
        Err(aios_apps::AppsError::RuntimeReject(msg)) => {
            assert!(
                msg.contains("sgr register failed"),
                "should propagate SGR error: {msg}"
            );
        }
        other => panic!("expected RuntimeReject from signature failure, got {other:?}"),
    }
}

#[tokio::test]
async fn sgr_bridge_start_service_unknown_unit_returns_not_found() {
    let graph = Arc::new(InMemoryServiceGraph::new());
    let bridge = SgrBridge::new(graph);

    let unknown_id = UnitId::from_parts("aios", "nonexistent", None).expect("valid unit id parts");

    let err = bridge
        .start_service(&unknown_id)
        .await
        .expect_err("unknown unit should return NotFound");

    assert!(matches!(err, aios_apps::AppsError::NotFound(_)));
}

// ---------------------------------------------------------------------------
// SandboxBridge tests
// ---------------------------------------------------------------------------

#[test]
fn sandbox_bridge_new_constructs() {
    let composer = Arc::new(InMemorySandboxComposer::new());
    let _bridge = SandboxBridge::new(composer);
}

#[tokio::test]
async fn sandbox_bridge_allocate_for_session_succeeds() {
    let composer = Arc::new(InMemorySandboxComposer::new());
    let bridge = SandboxBridge::new(composer);

    let profile_id = bridge
        .allocate_for_session(
            &test_package_id(),
            EcosystemRuntime::RuntimeLinuxNative,
            &[],
        )
        .await
        .expect("allocation should succeed");

    // The returned id should be usable later.
    assert!(!profile_id.0.is_empty());
}

#[tokio::test]
async fn sandbox_bridge_release_unknown_profile_returns_not_found() {
    let composer = Arc::new(InMemorySandboxComposer::new());
    let bridge = SandboxBridge::new(composer);

    let unknown = ProfileId::new();
    let err = bridge
        .release(&unknown)
        .await
        .expect_err("unknown profile should return NotFound");

    assert!(matches!(err, aios_apps::AppsError::NotFound(_)));
}

#[tokio::test]
async fn sandbox_bridge_allocate_then_release_works() {
    let composer = Arc::new(InMemorySandboxComposer::new());
    let bridge = SandboxBridge::new(composer);

    let profile_id = bridge
        .allocate_for_session(
            &test_package_id(),
            EcosystemRuntime::RuntimeLinuxNative,
            &[],
        )
        .await
        .expect("allocation should succeed");

    // Release of the known profile should succeed (it exists in the catalog).
    bridge
        .release(&profile_id)
        .await
        .expect("release of known profile should succeed");
}

// ---------------------------------------------------------------------------
// Static trait bound verification
// ---------------------------------------------------------------------------

#[test]
fn bridges_are_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RuntimeBridge>();
    assert_send_sync::<SgrBridge>();
    assert_send_sync::<SandboxBridge>();
}
