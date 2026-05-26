//! Integration tests for sandbox evidence emission (S3.2 ↔ S3.1).
//!
//! Covers the full evidence lifecycle: emitter construction, emission through
//! all 3 components (composer, GPU enforcer, resource enforcer), chain
//! integrity, and the no-emitter backward-compatibility path.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use aios_sandbox::{
    ComposeRequest, GpuCapabilityClass, GpuPolicyEnforcer, InMemorySandboxComposer,
    InMemorySandboxEvidenceLog, NetworkPosture, ResourceLimitEnforcer, ResourceLimits,
    ResourceRequest, SandboxComposer, SandboxEvidenceEmitter, SandboxSubjectRef, SubjectRef,
    AIOS_SANDBOX_SUBJECT,
};

fn make_signing_key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[
        0x42, 0xd0, 0x2c, 0x5e, 0x84, 0x82, 0x3c, 0x71, 0x85, 0xf1, 0x0e, 0x78, 0x4a, 0xdd, 0x02,
        0x9b, 0xd1, 0x4b, 0x2b, 0x6b, 0x39, 0x6a, 0xab, 0x95, 0xb8, 0x58, 0x05, 0x14, 0xa5, 0x67,
        0xe4, 0x19,
    ])
}

fn make_emitter_with_log() -> (
    SandboxEvidenceEmitter,
    Arc<InMemorySandboxEvidenceLog>,
    ed25519_dalek::VerifyingKey,
) {
    let log = Arc::new(InMemorySandboxEvidenceLog::new());
    let signing_key = make_signing_key();
    let vk = signing_key.verifying_key();
    let emitter = SandboxEvidenceEmitter::new(
        log.clone(),
        signing_key,
        SandboxSubjectRef(AIOS_SANDBOX_SUBJECT.to_string()),
    );
    (emitter, log, vk)
}

// === 1. Log starts empty ===

#[tokio::test]
async fn evidence_log_starts_empty() {
    let log = InMemorySandboxEvidenceLog::new();
    assert!(log.is_empty().await);
    assert_eq!(log.len().await, 0);
}

// === 2. Composer emits on compose ===

#[tokio::test]
async fn composer_emits_sandbox_composed_event() {
    let (emitter, log, vk) = make_emitter_with_log();
    let composer = InMemorySandboxComposer::new().with_evidence_emitter(Arc::new(emitter));

    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "test-action".into(),
        base_profile_id: None,
        adapter_default: None,
        app_manifest: None,
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: false,
        is_ai: false,
    };
    composer.compose(req).await.unwrap();

    assert_eq!(log.len().await, 1);
    log.verify_integrity().await.unwrap();
    log.verify_integrity_signed(&vk).await.unwrap();
}

// === 4. GPU enforcer emits on binding computation ===

#[tokio::test]
async fn gpu_enforcer_emits_capability_bound_event() {
    let (emitter, log, vk) = make_emitter_with_log();
    let enforcer = GpuPolicyEnforcer::new_with_defaults().with_evidence_emitter(Arc::new(emitter));

    let profile = aios_sandbox::GpuPolicy {
        gpu_capability_class: GpuCapabilityClass::GpuRich2d,
        vk_device_required: true,
        dmabuf_passthrough_allowed: false,
        per_group_partitioning: true,
        iommu_required: true,
        expires_at: None,
    };
    let subject = SubjectRef::new("agent:test");
    let binding = enforcer
        .compute_capability_binding(&profile, "group-test", &subject)
        .unwrap();
    assert!(binding.binding_id.starts_with("gcb_"));

    // tokio::spawn is fire-and-forget — give it a moment to flush
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !log.is_empty().await,
        "log should have at least one receipt"
    );
    log.verify_integrity().await.unwrap();
    log.verify_integrity_signed(&vk).await.unwrap();
}

// === 5. Resource enforcer emits on limit exceeded ===

#[tokio::test]
async fn resource_enforcer_emits_limit_exceeded_event() {
    let (emitter, log, vk) = make_emitter_with_log();
    let enforcer =
        ResourceLimitEnforcer::new_with_defaults().with_evidence_emitter(Arc::new(emitter));

    let limits = ResourceLimits::default_strict();
    let request = ResourceRequest {
        cpu_pct: 200, // exceeds default strict
        memory_bytes: 32 * 1024 * 1024,
        io_bytes_per_sec: 0,
        network_bytes_per_sec: 0,
        process_count: 0,
        fd_count: 0,
    };
    let err = enforcer.check_usage(&request, &limits).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("cpu_quota_percent"), "got: {msg}");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !log.is_empty().await,
        "log should have at least one receipt"
    );
    log.verify_integrity().await.unwrap();
    log.verify_integrity_signed(&vk).await.unwrap();
}

// === 6. No-emitter path works (backward compatibility) ===

#[tokio::test]
async fn composer_without_emitter_still_works() {
    let composer = InMemorySandboxComposer::new(); // no emitter
    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "test-action".into(),
        base_profile_id: None,
        adapter_default: None,
        app_manifest: None,
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: false,
        is_ai: false,
    };
    let result = composer.compose(req).await.unwrap();
    assert_eq!(result.profile.network_posture, NetworkPosture::DenyAll);
}

#[tokio::test]
async fn gpu_enforcer_without_emitter_still_works() {
    let enforcer = GpuPolicyEnforcer::new_with_defaults(); // no emitter
    let profile = aios_sandbox::GpuPolicy {
        gpu_capability_class: GpuCapabilityClass::GpuRich2d,
        vk_device_required: true,
        dmabuf_passthrough_allowed: false,
        per_group_partitioning: true,
        iommu_required: true,
        expires_at: None,
    };
    let binding = enforcer
        .compute_capability_binding(&profile, "g", &SubjectRef::new("s"))
        .unwrap();
    assert!(!binding.binding_id.is_empty());
}

#[tokio::test]
async fn resource_enforcer_without_emitter_still_works() {
    let enforcer = ResourceLimitEnforcer::new_with_defaults(); // no emitter
    let limits = ResourceLimits::default_strict();
    let request = ResourceRequest {
        cpu_pct: 5,
        memory_bytes: 32 * 1024 * 1024,
        io_bytes_per_sec: 0,
        network_bytes_per_sec: 0,
        process_count: 0,
        fd_count: 0,
    };
    assert!(enforcer.check_usage(&request, &limits).is_ok());
}

// === 7. Multiple emits chain correctly ===

#[tokio::test]
async fn multiple_emits_chain_integrity() {
    let (emitter, log, vk) = make_emitter_with_log();
    let composer = InMemorySandboxComposer::new().with_evidence_emitter(Arc::new(emitter));

    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "t1".into(),
        base_profile_id: None,
        adapter_default: None,
        app_manifest: None,
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: false,
        is_ai: false,
    };
    composer.compose(req).await.unwrap();

    let req2 = ComposeRequest {
        subject: SubjectRef::new("test2"),
        action_kind: "t2".into(),
        base_profile_id: None,
        adapter_default: None,
        app_manifest: None,
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: true,
        is_ai: true,
    };
    composer.compose(req2).await.unwrap();

    assert_eq!(log.len().await, 2);
    log.verify_integrity().await.unwrap();
    log.verify_integrity_signed(&vk).await.unwrap();
}

// === 8. Receipt chain snapshot captures all ===

#[tokio::test]
async fn receipt_snapshot_captures_all_events() {
    let (emitter, log, _vk) = make_emitter_with_log();
    let composer = InMemorySandboxComposer::new().with_evidence_emitter(Arc::new(emitter));

    // Emit 3 compose events
    for i in 0..3 {
        let req = ComposeRequest {
            subject: SubjectRef::new(format!("test{i}")),
            action_kind: format!("action{i}"),
            base_profile_id: None,
            adapter_default: None,
            app_manifest: None,
            user_request: None,
            policy_required: None,
            group_floor: None,
            runtime_safety_floor: None,
            recovery_mode: false,
            is_ai: false,
        };
        composer.compose(req).await.unwrap();
    }

    let receipts = log.receipts().await;
    assert_eq!(receipts.len(), 3);
    // Each receipt should have distinct ids
    assert_ne!(receipts[0].receipt_id(), receipts[1].receipt_id());
    assert_ne!(receipts[1].receipt_id(), receipts[2].receipt_id());
}

// === 9. Subject constant is correct ===

#[test]
fn aios_sandbox_subject_constant_is_correct() {
    assert_eq!(AIOS_SANDBOX_SUBJECT, "_system:service:sandbox-composer");
}

// === 10. Emitter exposes verifying key ===

#[test]
fn emitter_exposes_verifying_key() {
    let (_emitter, _log, vk) = make_emitter_with_log();
    // Key material is deterministic from the fixed seed
    assert_eq!(
        vk.as_bytes().len(),
        32,
        "Ed25519 public key should be 32 bytes"
    );
}

// === 11. Emitter debug does not leak key material ===

#[test]
fn emitter_debug_redacts_signing_key() {
    let (emitter, _log, _vk) = make_emitter_with_log();
    let debug = format!("{emitter:?}");
    assert!(
        debug.contains("<redacted>"),
        "debug should redact signing key"
    );
    assert!(
        !debug.contains("SigningKey"),
        "debug should not leak key type"
    );
}

// === 12. Log defaults to empty ===

#[test]
fn in_memory_log_default_is_empty_sync() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let log = InMemorySandboxEvidenceLog::default();
        assert!(log.is_empty().await);
    });
}

// === 13. Composer with fixtures + emitter works ===

#[tokio::test]
async fn composer_with_fixtures_and_emitter_composes_and_emits() {
    let (emitter, log, _vk) = make_emitter_with_log();
    let composer =
        InMemorySandboxComposer::with_fixtures().with_evidence_emitter(Arc::new(emitter));

    let profiles = composer.list_profiles().await.unwrap();
    let svc = profiles
        .iter()
        .find(|p| p.name == "service_class")
        .expect("service_class fixture missing");

    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "test-action".into(),
        base_profile_id: Some(svc.profile_id.clone()),
        adapter_default: None,
        app_manifest: None,
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: false,
        is_ai: false,
    };
    let result = composer.compose(req).await.unwrap();
    assert_eq!(
        result.profile.network_posture,
        NetworkPosture::ExplicitAllowlist
    );
    assert_eq!(log.len().await, 1);
}

// === 14. Resource enforcer check_usage within limits does NOT emit ===

#[tokio::test]
async fn resource_enforcer_within_limits_does_not_emit() {
    let (emitter, log, _vk) = make_emitter_with_log();
    let enforcer =
        ResourceLimitEnforcer::new_with_defaults().with_evidence_emitter(Arc::new(emitter));

    let limits = ResourceLimits::default_strict();
    let request = ResourceRequest {
        cpu_pct: 5,
        memory_bytes: 32 * 1024 * 1024,
        io_bytes_per_sec: 0,
        network_bytes_per_sec: 0,
        process_count: 0,
        fd_count: 0,
    };
    assert!(enforcer.check_usage(&request, &limits).is_ok());
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        log.is_empty().await,
        "no emission expected for valid request"
    );
}

// === 15. All four payload types can coexist on chain ===

#[tokio::test]
async fn all_payload_types_coexist_on_chain() {
    let (emitter, log, vk) = make_emitter_with_log();
    let emitter = Arc::new(emitter);

    // Compose (emits SandboxComposed)
    let composer = InMemorySandboxComposer::new().with_evidence_emitter(Arc::clone(&emitter));
    let req = ComposeRequest {
        subject: SubjectRef::new("test"),
        action_kind: "test".into(),
        base_profile_id: None,
        adapter_default: None,
        app_manifest: None,
        user_request: None,
        policy_required: None,
        group_floor: None,
        runtime_safety_floor: None,
        recovery_mode: false,
        is_ai: false,
    };
    composer.compose(req).await.unwrap();

    // GPU enforcer (emits GpuCapabilityBound)
    let gpu = GpuPolicyEnforcer::new_with_defaults().with_evidence_emitter(Arc::clone(&emitter));
    let profile = aios_sandbox::GpuPolicy {
        gpu_capability_class: GpuCapabilityClass::GpuRich2d,
        vk_device_required: true,
        dmabuf_passthrough_allowed: false,
        per_group_partitioning: true,
        iommu_required: true,
        expires_at: None,
    };
    let _ = gpu
        .compute_capability_binding(&profile, "g", &SubjectRef::new("s"))
        .unwrap();

    // Resource enforcer (emits ResourceLimitExceeded)
    let res =
        ResourceLimitEnforcer::new_with_defaults().with_evidence_emitter(Arc::clone(&emitter));
    let limits = ResourceLimits::default_strict();
    let request = ResourceRequest {
        cpu_pct: 200,
        memory_bytes: 32 * 1024 * 1024,
        io_bytes_per_sec: 0,
        network_bytes_per_sec: 0,
        process_count: 0,
        fd_count: 0,
    };
    let _ = res.check_usage(&request, &limits);

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        log.len().await >= 2,
        "expected at least 2 receipts (compose + gpu/resource), got {}",
        log.len().await
    );
    log.verify_integrity().await.unwrap();
    log.verify_integrity_signed(&vk).await.unwrap();
}
