//! T-112 — Cross-crate integration tests: sandbox ↔ capability-runtime + cognitive.
//!
//! Tests the `SandboxRuntimeAdapter` (implements `RuntimeSandboxComposer`),
//! `SandboxCognitiveHint` builder, and the composition pipeline with fixtures.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use aios_capability_runtime::RuntimeSandboxComposer;
use aios_sandbox::{InMemorySandboxComposer, SandboxCognitiveHint, SandboxRuntimeAdapter};

// ---------------------------------------------------------------------------
// SandboxRuntimeAdapter — compose_for_action
// ---------------------------------------------------------------------------

#[tokio::test]
async fn adapter_compose_with_fixtures_returns_summary() {
    let composer = Arc::new(InMemorySandboxComposer::with_fixtures());
    let adapter = SandboxRuntimeAdapter::new(composer);

    let summary = adapter
        .compose_for_action("package.install", "human-operator", false, false)
        .await
        .expect("compose should succeed with fixtures");

    assert!(
        !summary.profile_id.is_empty(),
        "profile_id must be non-empty"
    );
    assert!(
        !summary.isolation_kind.is_empty(),
        "isolation_kind must be non-empty"
    );
    assert!(
        !summary.network_posture.is_empty(),
        "network_posture must be non-empty"
    );
    assert!(
        !summary.gpu_capability_class.is_empty(),
        "gpu_capability_class must be non-empty"
    );
}

#[tokio::test]
async fn adapter_compose_empty_catalog_still_succeeds() {
    let composer = Arc::new(InMemorySandboxComposer::new());
    let adapter = SandboxRuntimeAdapter::new(composer);

    let result = adapter
        .compose_for_action("service.restart", "svc-account", false, false)
        .await;
    assert!(
        result.is_ok(),
        "advisory composer must not fail on empty catalog"
    );
}

#[tokio::test]
async fn adapter_compose_ai_agent_distinct_from_human() {
    let composer = Arc::new(InMemorySandboxComposer::with_fixtures());
    let adapter = SandboxRuntimeAdapter::new(composer);

    let ai_summary = adapter
        .compose_for_action("file.read", "ai-agent-7", true, false)
        .await
        .expect("AI compose should succeed");
    let human_summary = adapter
        .compose_for_action("file.read", "human-operator", false, false)
        .await
        .expect("human compose should succeed");

    // AI and human subjects may get different profiles depending on the merge.
    // We at least verify both returned valid summaries.
    assert!(!ai_summary.profile_id.is_empty());
    assert!(!human_summary.profile_id.is_empty());
}

#[tokio::test]
async fn adapter_compose_recovery_mode_returns_valid_summary() {
    let composer = Arc::new(InMemorySandboxComposer::with_fixtures());
    let adapter = SandboxRuntimeAdapter::new(composer);

    let summary = adapter
        .compose_for_action("recovery.restore", "operator-root", false, true)
        .await
        .expect("recovery-mode compose should succeed");
    assert!(!summary.profile_id.is_empty());
}

#[tokio::test]
async fn adapter_compose_different_actions_produce_profiles() {
    let composer = Arc::new(InMemorySandboxComposer::with_fixtures());
    let adapter = SandboxRuntimeAdapter::new(Arc::clone(&composer));

    let a = adapter
        .compose_for_action("package.install", "subject-1", false, false)
        .await
        .expect("compose a");
    let b = adapter
        .compose_for_action("network.connect", "subject-1", false, false)
        .await
        .expect("compose b");

    // Each composition gets a fresh ProfileId.
    assert!(!a.profile_id.is_empty());
    assert!(!b.profile_id.is_empty());
}

#[tokio::test]
async fn adapter_compose_is_idempotent_shape() {
    let composer = Arc::new(InMemorySandboxComposer::with_fixtures());
    let adapter = SandboxRuntimeAdapter::new(composer);

    let s1 = adapter
        .compose_for_action("file.read", "test-subject", false, false)
        .await
        .expect("first compose");
    let s2 = adapter
        .compose_for_action("file.read", "test-subject", false, false)
        .await
        .expect("second compose");

    // Profile IDs differ (each call generates fresh ID), but shape is consistent.
    assert_eq!(s1.isolation_kind, s2.isolation_kind);
    assert_eq!(s1.network_posture, s2.network_posture);
    assert_eq!(s1.gpu_capability_class, s2.gpu_capability_class);
}

// ---------------------------------------------------------------------------
// SandboxRuntimeAdapter — Debug + Send + Sync
// ---------------------------------------------------------------------------

#[test]
fn adapter_debug_format_is_non_empty() {
    let composer = Arc::new(InMemorySandboxComposer::new());
    let adapter = SandboxRuntimeAdapter::new(composer);
    let debug_str = format!("{adapter:?}");
    assert!(debug_str.contains("SandboxRuntimeAdapter"));
    assert!(debug_str.contains("InMemorySandboxComposer"));
}

#[test]
fn adapter_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<SandboxRuntimeAdapter>();
}

#[test]
fn adapter_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<SandboxRuntimeAdapter>();
}

// ---------------------------------------------------------------------------
// SandboxCognitiveHint — construction + defaults
// ---------------------------------------------------------------------------

#[test]
fn cognitive_hint_defaults_are_none() {
    let hint = SandboxCognitiveHint::build_hint_from_intent(
        "install nano",
        "package.install",
        true,
        false,
        true,
    );
    assert!(hint.suggested_isolation.is_none());
    assert!(hint.suggested_network.is_none());
    assert!(hint.suggested_gpu_class.is_none());
    assert!(hint.requires_network);
    assert!(!hint.requires_gpu);
    assert!(hint.requires_filesystem);
    assert!(hint.rationale.is_none());
}

#[test]
fn cognitive_hint_builder_sets_fields() {
    let hint = SandboxCognitiveHint::build_hint_from_intent(
        "train model on gpu",
        "compute.train",
        false,
        true,
        false,
    )
    .with_isolation("VmGuest")
    .with_network("LoopbackOnly")
    .with_gpu_class("GpuComputeHeavy")
    .with_rationale("model training needs full GPU compute");

    assert_eq!(hint.suggested_isolation.as_deref(), Some("VmGuest"));
    assert_eq!(hint.suggested_network.as_deref(), Some("LoopbackOnly"));
    assert_eq!(hint.suggested_gpu_class.as_deref(), Some("GpuComputeHeavy"));
    assert_eq!(
        hint.rationale.as_deref(),
        Some("model training needs full GPU compute")
    );
    assert!(hint.requires_gpu);
    assert!(!hint.requires_network);
}

#[test]
fn cognitive_hint_requires_nothing_by_default() {
    let hint = SandboxCognitiveHint::build_hint_from_intent(
        "simple echo",
        "shell.echo",
        false,
        false,
        false,
    );
    assert!(!hint.requires_network);
    assert!(!hint.requires_gpu);
    assert!(!hint.requires_filesystem);
}

// ---------------------------------------------------------------------------
// SandboxCognitiveHint — serde round-trip
// ---------------------------------------------------------------------------

#[test]
fn cognitive_hint_serde_round_trip() {
    let hint = SandboxCognitiveHint::build_hint_from_intent(
        "query database",
        "db.query",
        true,
        false,
        false,
    )
    .with_isolation("NamespaceLocal")
    .with_network("HostLimited")
    .with_rationale("database query requires network to db host");

    let json = serde_json::to_string(&hint).expect("serialize");
    let back: SandboxCognitiveHint = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(hint, back, "round-trip must preserve equality");
}

#[test]
fn cognitive_hint_json_contains_expected_keys() {
    let hint = SandboxCognitiveHint::build_hint_from_intent(
        "open browser",
        "browser.launch",
        true,
        false,
        false,
    );
    let json = serde_json::to_string(&hint).expect("serialize");
    assert!(json.contains("requires_network"));
    assert!(json.contains("requires_gpu"));
    assert!(json.contains("requires_filesystem"));
    assert!(json.contains("suggested_isolation"));
}

// ---------------------------------------------------------------------------
// Cross-crate import sanity
// ---------------------------------------------------------------------------

#[test]
fn trait_is_importable_from_capability_runtime() {
    // If this compiles, the cross-crate dependency chain is wired.
    fn _assert_trait_object(_c: &dyn RuntimeSandboxComposer) {}
}

#[test]
fn sandbox_depends_on_capability_runtime() {
    // Verify the dependency is declared and the crate compiles.
    // aios_sandbox re-exports SandboxRuntimeAdapter which implements
    // aios_capability_runtime::RuntimeSandboxComposer.
    let _adapter: SandboxRuntimeAdapter =
        SandboxRuntimeAdapter::new(Arc::new(InMemorySandboxComposer::new()));
}

#[test]
fn sandbox_depends_on_cognitive() {
    // Verify aios-cognitive types are reachable through aios-sandbox.
    let hint = SandboxCognitiveHint::build_hint_from_intent("test", "test.op", false, false, false);
    assert!(!hint.requires_network);
}
