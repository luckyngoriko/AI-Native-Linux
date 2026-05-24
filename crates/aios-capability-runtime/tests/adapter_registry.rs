//! T-028 integration tests for [`InMemoryAdapterRegistry`].
//!
//! Anchors S10.1 §10 (adapter manifest contract). Each test pins one of the
//! brief's twelve required behaviours:
//!
//! 1. Register valid manifest with valid Ed25519 sig → succeeds.
//! 2. Register manifest with bad sig → `AdapterSignatureInvalid`.
//! 3. Register manifest from unknown authority → `AdapterUnknownAuthority`.
//! 4. Register manifest with `adapter_id` already taken → `AdapterAlreadyRegistered`.
//! 5. `lookup_by_id` known adapter → `Some`.
//! 6. `lookup_by_id` unknown adapter → `None`.
//! 7. `lookup_by_id` retired adapter → `None` (`FAIL_CLOSED` per §3.4).
//! 8. `lookup_for_target` with matching capability → `Some`.
//! 9. `lookup_for_target` with no match → `None`.
//! 10. `list()` returns all registered (retired included).
//! 11. Trait impl: `Arc<InMemoryAdapterRegistry>` usable as `Arc<dyn AdapterRegistry>`.
//! 12. Pipeline integration: unknown adapter via `with_adapter_registry` fails closed.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_capability_runtime::adapter_manifest::AdapterActionDeclaration;
use aios_capability_runtime::{
    canonical_signed_manifest_bytes, encode_hex_signature, ActionDispatchKind,
    ActionLifecycleState, AdapterIOMode, AdapterManifest, AdapterRegistry, AdapterStability,
    CapabilityRuntime, ExecutionFailureReason, InMemoryAdapterRegistry, InMemoryCapabilityRuntime,
    RuntimeContext, RuntimeError,
};

// ---------------------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------------------

const TRUSTED_KEY_ID: &str = "publisher:aios-root:v1";
const UNTRUSTED_KEY_ID: &str = "publisher:unknown:v1";

fn fresh_keypair() -> (SigningKey, VerifyingKey) {
    let sk = SigningKey::generate(&mut OsRng);
    let vk = sk.verifying_key();
    (sk, vk)
}

/// Build an unsigned manifest with sensible defaults for a `service.restart`
/// adapter. Tests override individual fields then call [`sign_manifest`].
fn unsigned_manifest(adapter_id: &str, stability: AdapterStability) -> AdapterManifest {
    let now = Utc::now();
    AdapterManifest {
        adapter_id: adapter_id.to_string(),
        adapter_version: "0.1.0".to_string(),
        vendor: "aios".to_string(),
        name: "systemd".to_string(),
        declared_stability: stability,
        io_mode: AdapterIOMode::TypedParametersOnly,
        dispatch_kind: ActionDispatchKind::SubprocessFork,
        declared_actions: vec![AdapterActionDeclaration {
            action_kind: "service.restart".to_string(),
            target_schema: serde_json::json!({"type": "object"}),
            response_schema: serde_json::json!({"type": "object"}),
            rollback_strategy: "IDEMPOTENT_REAPPLY".to_string(),
            timeout_seconds: 30,
            template_string: None,
            template_substitution_variables: vec![],
        }],
        declared_invariants_supported: vec!["INV-013".to_string()],
        default_adapter_timeout_seconds: 60,
        default_sandbox_profile_id: "service-restart-default".to_string(),
        adapter_signature: String::new(),
        signing_key_id: TRUSTED_KEY_ID.to_string(),
        manifest_created_at: now,
        manifest_expires_at: now + Duration::days(365),
    }
}

/// Populate `manifest.adapter_signature` with the lower-hex Ed25519 signature
/// over the canonical signed body (fields 1..11).
fn sign_manifest(manifest: &mut AdapterManifest, sk: &SigningKey) {
    let body = canonical_signed_manifest_bytes(manifest).expect("body serialise");
    let sig = sk.sign(&body);
    manifest.adapter_signature = encode_hex_signature(&sig.to_bytes());
}

/// Build a registry whose trust store contains one publisher.
fn registry_with(sk: &SigningKey) -> InMemoryAdapterRegistry {
    let mut trusted = HashMap::new();
    trusted.insert(TRUSTED_KEY_ID.to_string(), sk.verifying_key());
    InMemoryAdapterRegistry::new(trusted)
}

fn happy_envelope() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("human:lucky", false),
        Request::new("service.restart", serde_json::json!({"service": "nginx"})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

fn unknown_kind_envelope() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("human:lucky", false),
        Request::new(
            "this.action.kind.is.not.declared",
            serde_json::json!({"x": 1}),
        ),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

// ---------------------------------------------------------------------------
// 1. Valid manifest with valid signature → register succeeds.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_valid_signed_manifest_succeeds() {
    let (sk, _vk) = fresh_keypair();
    let registry = registry_with(&sk);
    let mut manifest = unsigned_manifest("adapter:aios:systemd:0.1.0", AdapterStability::Stable);
    sign_manifest(&mut manifest, &sk);

    registry
        .register(manifest, Utc::now())
        .await
        .expect("valid signed manifest must register");

    assert_eq!(registry.len().await, 1);
}

// ---------------------------------------------------------------------------
// 2. Tampered signature → AdapterSignatureInvalid.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_with_bad_signature_rejects() {
    let (sk, _vk) = fresh_keypair();
    let registry = registry_with(&sk);
    let mut manifest = unsigned_manifest("adapter:aios:systemd:0.1.0", AdapterStability::Stable);
    sign_manifest(&mut manifest, &sk);

    // Flip one byte of the signature (hex chars '0'/'1' are swapped).
    let mut bytes = manifest.adapter_signature.into_bytes();
    bytes[0] = if bytes[0] == b'0' { b'1' } else { b'0' };
    manifest.adapter_signature = String::from_utf8(bytes).expect("ascii");

    let err = registry
        .register(manifest, Utc::now())
        .await
        .expect_err("tampered signature must reject");
    assert!(
        matches!(err, RuntimeError::AdapterSignatureInvalid),
        "expected AdapterSignatureInvalid, got {err:?}"
    );
    assert!(registry.is_empty().await);
}

#[tokio::test]
async fn register_with_modified_body_rejects() {
    // Sign one body, then mutate a signed field — verification must fail.
    let (sk, _vk) = fresh_keypair();
    let registry = registry_with(&sk);
    let mut manifest = unsigned_manifest("adapter:aios:systemd:0.1.0", AdapterStability::Stable);
    sign_manifest(&mut manifest, &sk);

    // Mutate the vendor (a signed field). Signature must no longer verify.
    manifest.vendor = "evil-corp".to_string();

    let err = registry
        .register(manifest, Utc::now())
        .await
        .expect_err("modified-after-sign manifest must reject");
    assert!(matches!(err, RuntimeError::AdapterSignatureInvalid));
}

// ---------------------------------------------------------------------------
// 3. Manifest from unknown authority → AdapterUnknownAuthority.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_from_unknown_authority_rejects() {
    let (sk, _vk) = fresh_keypair();
    let registry = registry_with(&sk);
    let mut manifest = unsigned_manifest("adapter:aios:systemd:0.1.0", AdapterStability::Stable);
    manifest.signing_key_id = UNTRUSTED_KEY_ID.to_string();
    sign_manifest(&mut manifest, &sk);

    let err = registry
        .register(manifest, Utc::now())
        .await
        .expect_err("unknown authority must reject");
    assert!(
        matches!(err, RuntimeError::AdapterUnknownAuthority(ref s) if s == UNTRUSTED_KEY_ID),
        "expected AdapterUnknownAuthority({UNTRUSTED_KEY_ID}), got {err:?}"
    );
    assert!(registry.is_empty().await);
}

// ---------------------------------------------------------------------------
// 4. Duplicate adapter_id → AdapterAlreadyRegistered.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_duplicate_adapter_id_rejects() {
    let (sk, _vk) = fresh_keypair();
    let registry = registry_with(&sk);

    let mut first = unsigned_manifest("adapter:aios:systemd:0.1.0", AdapterStability::Stable);
    sign_manifest(&mut first, &sk);
    registry
        .register(first, Utc::now())
        .await
        .expect("first registration");

    // Second manifest, same adapter_id, freshly signed.
    let mut second = unsigned_manifest("adapter:aios:systemd:0.1.0", AdapterStability::Stable);
    second.adapter_version = "0.2.0".to_string();
    sign_manifest(&mut second, &sk);
    let err = registry
        .register(second, Utc::now())
        .await
        .expect_err("duplicate id must reject");
    assert!(
        matches!(err, RuntimeError::AdapterAlreadyRegistered(ref s) if s == "adapter:aios:systemd:0.1.0"),
        "expected AdapterAlreadyRegistered, got {err:?}"
    );
    assert_eq!(registry.len().await, 1, "no double-insert");
}

// ---------------------------------------------------------------------------
// 5 + 6. lookup_by_id — present + absent.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lookup_by_id_returns_registered_adapter() {
    let (sk, _vk) = fresh_keypair();
    let registry = registry_with(&sk);
    let mut manifest = unsigned_manifest("adapter:aios:systemd:0.1.0", AdapterStability::Stable);
    sign_manifest(&mut manifest, &sk);
    registry
        .register(manifest, Utc::now())
        .await
        .expect("register");

    let found = registry
        .lookup_by_id("adapter:aios:systemd:0.1.0")
        .await
        .expect("known adapter must resolve");
    assert_eq!(found.manifest.adapter_id, "adapter:aios:systemd:0.1.0");
}

#[tokio::test]
async fn lookup_by_id_unknown_returns_none() {
    let (sk, _vk) = fresh_keypair();
    let registry = registry_with(&sk);
    assert!(registry
        .lookup_by_id("adapter:no.such.adapter:0")
        .await
        .is_none());
}

// ---------------------------------------------------------------------------
// 7. Retired adapter → lookup_by_id returns None (FAIL_CLOSED per §3.4).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lookup_by_id_skips_retired_adapter() {
    let (sk, _vk) = fresh_keypair();
    let registry = registry_with(&sk);
    let mut manifest = unsigned_manifest("adapter:aios:legacy:0.1.0", AdapterStability::Retired);
    sign_manifest(&mut manifest, &sk);
    registry
        .register(manifest, Utc::now())
        .await
        .expect("retired adapter still accepted for registration");

    // Registered, visible via list(), but invisible to dispatch-bound lookup.
    assert_eq!(registry.len().await, 1);
    assert!(
        registry
            .lookup_by_id("adapter:aios:legacy:0.1.0")
            .await
            .is_none(),
        "retired adapter must not be dispatchable"
    );
}

// ---------------------------------------------------------------------------
// 8 + 9. lookup_for_target — match + miss.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lookup_for_target_finds_declared_action_kind() {
    let (sk, _vk) = fresh_keypair();
    let registry = registry_with(&sk);
    let mut manifest = unsigned_manifest("adapter:aios:systemd:0.1.0", AdapterStability::Stable);
    sign_manifest(&mut manifest, &sk);
    registry
        .register(manifest, Utc::now())
        .await
        .expect("register");

    let found = registry
        .lookup_for_target("service.restart")
        .await
        .expect("declared action kind must resolve");
    assert_eq!(found.manifest.adapter_id, "adapter:aios:systemd:0.1.0");
}

#[tokio::test]
async fn lookup_for_target_misses_unknown_kind() {
    let (sk, _vk) = fresh_keypair();
    let registry = registry_with(&sk);
    let mut manifest = unsigned_manifest("adapter:aios:systemd:0.1.0", AdapterStability::Stable);
    sign_manifest(&mut manifest, &sk);
    registry
        .register(manifest, Utc::now())
        .await
        .expect("register");

    assert!(registry.lookup_for_target("pkg.install").await.is_none());
}

// ---------------------------------------------------------------------------
// 10. list() returns all registered (including retired).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_returns_all_registered_including_retired() {
    let (sk, _vk) = fresh_keypair();
    let registry = registry_with(&sk);

    let mut active = unsigned_manifest("adapter:aios:systemd:0.1.0", AdapterStability::Stable);
    sign_manifest(&mut active, &sk);
    registry.register(active, Utc::now()).await.expect("active");

    let mut retired = unsigned_manifest("adapter:aios:legacy:0.1.0", AdapterStability::Retired);
    sign_manifest(&mut retired, &sk);
    registry
        .register(retired, Utc::now())
        .await
        .expect("retired");

    let all = registry.list().await;
    assert_eq!(all.len(), 2);
    let ids: Vec<_> = all.iter().map(|r| r.manifest.adapter_id.as_str()).collect();
    assert!(ids.contains(&"adapter:aios:systemd:0.1.0"));
    assert!(ids.contains(&"adapter:aios:legacy:0.1.0"));
}

// ---------------------------------------------------------------------------
// 11. Trait impl: Arc<InMemoryAdapterRegistry> coercible to Arc<dyn AdapterRegistry>.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn arc_of_in_memory_registry_is_dyn_adapter_registry() {
    let (sk, _vk) = fresh_keypair();
    let registry = registry_with(&sk);
    let mut manifest = unsigned_manifest("adapter:aios:systemd:0.1.0", AdapterStability::Stable);
    sign_manifest(&mut manifest, &sk);
    registry
        .register(manifest, Utc::now())
        .await
        .expect("register");

    let concrete: Arc<InMemoryAdapterRegistry> = Arc::new(registry);
    let dynamic: Arc<dyn AdapterRegistry> = Arc::clone(&concrete) as Arc<dyn AdapterRegistry>;
    let handle = dynamic
        .lookup("service.restart")
        .expect("trait lookup must resolve known kind");
    // The handle's dispatch_kind echoes the manifest's preferred kind.
    assert_eq!(handle.dispatch_kind(), ActionDispatchKind::SubprocessFork);
    // Also verify the concrete-side count is unchanged (no leak).
    assert_eq!(concrete.len().await, 1);
}

#[tokio::test]
async fn trait_lookup_misses_unknown_kind() {
    let (sk, _vk) = fresh_keypair();
    let registry = registry_with(&sk);
    let concrete: Arc<InMemoryAdapterRegistry> = Arc::new(registry);
    let dynamic: Arc<dyn AdapterRegistry> = concrete;
    assert!(dynamic.lookup("no.such.kind").is_none());
}

// ---------------------------------------------------------------------------
// 12. Pipeline integration: unknown adapter → FAILED + DependencyUnready.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn runtime_with_registry_fails_closed_on_unknown_adapter() {
    let (sk, _vk) = fresh_keypair();
    let registry = registry_with(&sk);
    let mut manifest = unsigned_manifest("adapter:aios:systemd:0.1.0", AdapterStability::Stable);
    sign_manifest(&mut manifest, &sk);
    registry
        .register(manifest, Utc::now())
        .await
        .expect("register");

    let runtime = InMemoryCapabilityRuntime::new().with_adapter_registry(Arc::new(registry));
    let ctx = RuntimeContext::new("human:lucky", "polb_v1", "code_v1");
    let envelope = unknown_kind_envelope();

    let result = runtime
        .submit_action(&envelope, &ctx)
        .await
        .expect("pipeline returns a context even on FAIL_CLOSED");

    assert_eq!(result.status, ActionLifecycleState::Failed);
    assert_eq!(
        result.error,
        Some(ExecutionFailureReason::DependencyUnready)
    );
}

#[tokio::test]
async fn runtime_with_registry_drives_known_adapter_to_succeeded() {
    // Sanity: a known action kind still drives to SUCCEEDED under the
    // T-027 stub steps (verify is the success short-circuit). T-028's
    // step_execute is structurally identical to T-027 when the registry
    // *hits*.
    let (sk, _vk) = fresh_keypair();
    let registry = registry_with(&sk);
    let mut manifest = unsigned_manifest("adapter:aios:systemd:0.1.0", AdapterStability::Stable);
    sign_manifest(&mut manifest, &sk);
    registry
        .register(manifest, Utc::now())
        .await
        .expect("register");

    let runtime = InMemoryCapabilityRuntime::new().with_adapter_registry(Arc::new(registry));
    let ctx = RuntimeContext::new("human:lucky", "polb_v1", "code_v1");
    let envelope = happy_envelope();

    let result = runtime
        .submit_action(&envelope, &ctx)
        .await
        .expect("happy path");
    assert_eq!(result.status, ActionLifecycleState::Succeeded);
    assert_eq!(result.error, None);
}

#[tokio::test]
async fn runtime_without_registry_preserves_t027_baseline() {
    // Regression guard: a runtime with no registry attached must drive an
    // unknown action kind to SUCCEEDED through the structural stub steps
    // (T-027 contract). The FAIL_CLOSED behaviour engages only when
    // `with_adapter_registry` is called.
    let runtime = InMemoryCapabilityRuntime::new();
    let ctx = RuntimeContext::new("human:lucky", "polb_v1", "code_v1");
    let envelope = unknown_kind_envelope();
    let result = runtime
        .submit_action(&envelope, &ctx)
        .await
        .expect("baseline pipeline");
    assert_eq!(result.status, ActionLifecycleState::Succeeded);
    assert_eq!(result.error, None);
}
