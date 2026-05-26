#![allow(clippy::expect_used, clippy::panic)]

//! Integration tests for the S12.1 App Runtime Model implementation.
//!
//! Covers Phase A (observe), Phase B (translate), Phase D (refine/delta),
//! and the `SyscallClass` closed enum with its 10 variants.

use aios_apps::*;
use strum::{EnumCount, IntoEnumIterator};

// ============================================================================
// SyscallClass — 10 variants, round-trip
// ============================================================================

#[test]
fn test_37_syscall_class_variant_count() {
    let expected: usize = SyscallClass::COUNT;
    let iterated: Vec<_> = SyscallClass::iter().collect();
    assert_eq!(iterated.len(), expected, "variant count mismatch");
    assert_eq!(expected, 10, "SyscallClass must have exactly 10 variants");

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: SyscallClass = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

// ============================================================================
// ObservedBehavior — round-trip + deny_unknown_fields
// ============================================================================

#[test]
fn test_38_observed_behavior_roundtrip() {
    let behavior = ObservedBehavior {
        observation_id: "obs_test".into(),
        artifact_hash: "abc123".into(),
        observed_for_seconds: 60,
        observed_syscalls: vec![SyscallClass::FilesystemRead, SyscallClass::NetworkOutbound],
        blocked_filesystem_reads: vec!["/etc/passwd".into()],
        blocked_filesystem_writes: vec!["/etc/shadow".into()],
        attempted_dns_resolutions: vec!["example.com".into()],
        attempted_outbound_endpoints: vec!["1.2.3.4:443".into()],
        attempted_gpu_init: true,
        attempted_audio_init: false,
        attempted_microphone_open: false,
        attempted_camera_open: false,
        attempted_clipboard_read: true,
        attempted_clipboard_write: false,
        error_messages_redacted: vec!["redacted error".into()],
        process_terminated_normally: true,
        exit_code: 0,
    };

    let json = serde_json::to_string(&behavior).expect("serialize");
    let back: ObservedBehavior = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(behavior.observation_id, back.observation_id);
    assert_eq!(behavior.artifact_hash, back.artifact_hash);
    assert_eq!(behavior.observed_for_seconds, back.observed_for_seconds);
    assert_eq!(behavior.observed_syscalls, back.observed_syscalls);
    assert_eq!(
        behavior.blocked_filesystem_reads,
        back.blocked_filesystem_reads
    );
    assert_eq!(
        behavior.attempted_outbound_endpoints,
        back.attempted_outbound_endpoints
    );
    assert_eq!(behavior.attempted_gpu_init, back.attempted_gpu_init);
    assert_eq!(
        behavior.attempted_clipboard_read,
        back.attempted_clipboard_read
    );
    assert_eq!(
        behavior.process_terminated_normally,
        back.process_terminated_normally
    );
    assert_eq!(behavior.exit_code, back.exit_code);
}

#[test]
fn test_39_observed_behavior_rejects_unknown_fields() {
    let json = r#"{"observation_id":"obs_01","artifact_hash":"abc","observed_for_seconds":10,"observed_syscalls":[],"blocked_filesystem_reads":[],"blocked_filesystem_writes":[],"attempted_dns_resolutions":[],"attempted_outbound_endpoints":[],"attempted_gpu_init":false,"attempted_audio_init":false,"attempted_microphone_open":false,"attempted_camera_open":false,"attempted_clipboard_read":false,"attempted_clipboard_write":false,"error_messages_redacted":[],"process_terminated_normally":true,"exit_code":0,"extra":"nope"}"#;
    let result: Result<ObservedBehavior, _> = serde_json::from_str(json);
    assert!(result.is_err(), "should reject unknown fields");
}

#[test]
fn test_40_observed_behavior_max_constant() {
    assert_eq!(ObservedBehavior::MAX_OBSERVATION_SECONDS, 300);
}

// ============================================================================
// AppManifestProposal — round-trip + deny_unknown_fields
// ============================================================================

#[test]
fn test_41_app_manifest_proposal_roundtrip() {
    let proposal = AppManifestProposal {
        proposal_id: "prop_test".into(),
        app_id: "app_test".into(),
        ecosystem_runtime: EcosystemRuntime::RuntimeWindowsProton,
        honesty_class: EcosystemHonestyClass::PartiallySupported,
        strategy: ManifestTranslationStrategy::WinePrefixProbe,
        honesty_disclosure_text: "disclosure".into(),
        compatibility_caveats: vec!["Anti-cheat may reject".into()],
        declared_capabilities: vec!["cap_graphics".into()],
        observed_behavior_hash: "obs_hash".into(),
        proposer_signature: vec![1, 2, 3],
        proposer_subject_canonical_id: "_system:service:app-proposer".into(),
    };

    let json = serde_json::to_string(&proposal).expect("serialize");
    let back: AppManifestProposal = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(proposal.proposal_id, back.proposal_id);
    assert_eq!(proposal.app_id, back.app_id);
    assert_eq!(proposal.ecosystem_runtime, back.ecosystem_runtime);
    assert_eq!(proposal.honesty_class, back.honesty_class);
    assert_eq!(proposal.strategy, back.strategy);
    assert_eq!(proposal.declared_capabilities, back.declared_capabilities);
    assert_eq!(proposal.proposer_signature, back.proposer_signature);
    assert_eq!(
        proposal.proposer_subject_canonical_id,
        back.proposer_subject_canonical_id
    );
}

#[test]
fn test_42_app_manifest_proposal_rejects_unknown_fields() {
    let json = r#"{"proposal_id":"p","app_id":"a","ecosystem_runtime":"RUNTIME_LINUX_NATIVE","honesty_class":"FULLY_SUPPORTED","strategy":"NATIVE_PACKAGE","honesty_disclosure_text":"","compatibility_caveats":[],"declared_capabilities":[],"observed_behavior_hash":"","proposer_signature":[],"proposer_subject_canonical_id":"s","evil":true}"#;
    let result: Result<AppManifestProposal, _> = serde_json::from_str(json);
    assert!(result.is_err(), "should reject unknown fields");
}

// ============================================================================
// InMemoryAppRuntime — Phase A: observe
// ============================================================================

#[tokio::test]
async fn test_43_observe_creates_populated_behavior() {
    let runtime = InMemoryAppRuntime::new();
    let result = runtime
        .observe_in_sandbox("blake3_hash", EcosystemRuntime::RuntimeLinuxNative, 30)
        .await
        .expect("observation should succeed");

    assert!(result.observation_id.starts_with("obs_"));
    assert_eq!(result.artifact_hash, "blake3_hash");
    assert_eq!(result.observed_for_seconds, 30);
    assert!(result.observed_syscalls.is_empty());
    assert!(result.process_terminated_normally);
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_44_observe_rejects_duration_above_hard_cap() {
    let runtime = InMemoryAppRuntime::new();
    let result = runtime
        .observe_in_sandbox("hash", EcosystemRuntime::RuntimeLinuxNative, 301)
        .await;

    assert!(result.is_err());
    match result {
        Err(AppsError::ObservationRejected { reason, .. }) => {
            assert!(reason.contains("exceeds hard cap"));
        }
        other => panic!("expected ObservationRejected, got {other:?}"),
    }
}

#[tokio::test]
async fn test_45_observe_accepts_exact_hard_cap() {
    let runtime = InMemoryAppRuntime::new();
    let result = runtime
        .observe_in_sandbox("hash", EcosystemRuntime::RuntimeLinuxNative, 300)
        .await
        .expect("300 s (exact cap) should succeed");

    assert_eq!(result.observed_for_seconds, 300);
}

#[tokio::test]
async fn test_46_observe_stores_in_memory() {
    let runtime = InMemoryAppRuntime::new();
    assert_eq!(runtime.observation_count().await, 0);

    runtime
        .observe_in_sandbox("hash1", EcosystemRuntime::RuntimeLinuxNative, 10)
        .await
        .expect("ok");
    assert_eq!(runtime.observation_count().await, 1);

    runtime
        .observe_in_sandbox("hash2", EcosystemRuntime::RuntimeFlatpak, 20)
        .await
        .expect("ok");
    assert_eq!(runtime.observation_count().await, 2);
}

// ============================================================================
// InMemoryAppRuntime — Phase B: translate
// ============================================================================

#[tokio::test]
async fn test_47_translate_creates_populated_proposal() {
    let runtime = InMemoryAppRuntime::new();
    let behavior = ObservedBehavior {
        observation_id: "obs_test".into(),
        artifact_hash: "abc".into(),
        observed_for_seconds: 10,
        observed_syscalls: vec![],
        blocked_filesystem_reads: vec![],
        blocked_filesystem_writes: vec![],
        attempted_dns_resolutions: vec![],
        attempted_outbound_endpoints: vec![],
        attempted_gpu_init: false,
        attempted_audio_init: false,
        attempted_microphone_open: false,
        attempted_camera_open: false,
        attempted_clipboard_read: false,
        attempted_clipboard_write: false,
        error_messages_redacted: vec![],
        process_terminated_normally: true,
        exit_code: 0,
    };

    let proposal = runtime
        .translate_manifest(
            behavior,
            ManifestTranslationStrategy::AndroidManifestXml,
            EcosystemRuntime::RuntimeLinuxNative,
        )
        .await
        .expect("translate should succeed");

    assert!(proposal.proposal_id.starts_with("prop_"));
    assert!(proposal.app_id.starts_with("app_"));
    assert_eq!(
        proposal.ecosystem_runtime,
        EcosystemRuntime::RuntimeLinuxNative
    );
    assert_eq!(
        proposal.honesty_class,
        EcosystemHonestyClass::FullySupported
    );
    assert_eq!(
        proposal.strategy,
        ManifestTranslationStrategy::AndroidManifestXml
    );
}

#[tokio::test]
async fn test_48_translate_stores_proposal() {
    let runtime = InMemoryAppRuntime::new();
    assert_eq!(runtime.proposal_count().await, 0);

    let behavior = ObservedBehavior {
        observation_id: "obs_test".into(),
        artifact_hash: "abc".into(),
        observed_for_seconds: 10,
        observed_syscalls: vec![],
        blocked_filesystem_reads: vec![],
        blocked_filesystem_writes: vec![],
        attempted_dns_resolutions: vec![],
        attempted_outbound_endpoints: vec![],
        attempted_gpu_init: false,
        attempted_audio_init: false,
        attempted_microphone_open: false,
        attempted_camera_open: false,
        attempted_clipboard_read: false,
        attempted_clipboard_write: false,
        error_messages_redacted: vec![],
        process_terminated_normally: true,
        exit_code: 0,
    };

    runtime
        .translate_manifest(
            behavior,
            ManifestTranslationStrategy::AndroidManifestXml,
            EcosystemRuntime::RuntimeLinuxNative,
        )
        .await
        .expect("ok");
    assert_eq!(runtime.proposal_count().await, 1);
}

// ============================================================================
// InMemoryAppRuntime — Phase D: propose delta
// ============================================================================

#[tokio::test]
async fn test_49_propose_delta_returns_delta_proposed() {
    let runtime = InMemoryAppRuntime::new();
    let result = runtime
        .propose_manifest_delta("app_01", "operator requested re-audit")
        .await
        .expect("delta should succeed");

    assert_eq!(result, ManifestDeltaOutcome::DeltaProposed);
}

// ============================================================================
// InMemoryAppRuntime — concurrent safety
// ============================================================================

#[tokio::test]
async fn test_50_concurrent_observations_no_collision() {
    let runtime = InMemoryAppRuntime::new();
    let rt = std::sync::Arc::new(runtime);

    let mut handles = Vec::new();
    for i in 0..10u8 {
        let rt = rt.clone();
        handles.push(tokio::spawn(async move {
            rt.observe_in_sandbox(
                &format!("hash_{i}"),
                EcosystemRuntime::RuntimeLinuxNative,
                5,
            )
            .await
            .expect("observation should succeed")
        }));
    }

    for handle in handles {
        handle.await.expect("join");
    }

    assert_eq!(rt.observation_count().await, 10);
}

// ============================================================================
// InMemoryAppRuntime — inject seam
// ============================================================================

#[tokio::test]
async fn test_51_inject_observation_seam() {
    let runtime = InMemoryAppRuntime::new();
    let behavior = ObservedBehavior {
        observation_id: "obs_injected".into(),
        artifact_hash: "abc".into(),
        observed_for_seconds: 1,
        observed_syscalls: vec![SyscallClass::Ipc],
        blocked_filesystem_reads: vec![],
        blocked_filesystem_writes: vec![],
        attempted_dns_resolutions: vec![],
        attempted_outbound_endpoints: vec![],
        attempted_gpu_init: false,
        attempted_audio_init: false,
        attempted_microphone_open: false,
        attempted_camera_open: false,
        attempted_clipboard_read: false,
        attempted_clipboard_write: false,
        error_messages_redacted: vec![],
        process_terminated_normally: true,
        exit_code: 0,
    };

    runtime.inject_observation(behavior).await;
    assert_eq!(runtime.observation_count().await, 1);
}

// ============================================================================
// honesty_class_for_runtime — S12.1 §3.1 mapping for all 12 runtimes
// ============================================================================

#[tokio::test]
async fn test_52_honesty_class_fully_supported() {
    let runtime = InMemoryAppRuntime::new();

    for rt in [
        EcosystemRuntime::RuntimeLinuxNative,
        EcosystemRuntime::RuntimeFlatpak,
        EcosystemRuntime::RuntimeSnap,
    ] {
        let behavior = ObservedBehavior {
            observation_id: format!("obs_{rt}"),
            artifact_hash: "abc".into(),
            observed_for_seconds: 1,
            observed_syscalls: vec![],
            blocked_filesystem_reads: vec![],
            blocked_filesystem_writes: vec![],
            attempted_dns_resolutions: vec![],
            attempted_outbound_endpoints: vec![],
            attempted_gpu_init: false,
            attempted_audio_init: false,
            attempted_microphone_open: false,
            attempted_camera_open: false,
            attempted_clipboard_read: false,
            attempted_clipboard_write: false,
            error_messages_redacted: vec![],
            process_terminated_normally: true,
            exit_code: 0,
        };
        let proposal = runtime
            .translate_manifest(
                behavior,
                ManifestTranslationStrategy::AndroidManifestXml,
                rt,
            )
            .await
            .expect("translate");
        assert_eq!(
            proposal.honesty_class,
            EcosystemHonestyClass::FullySupported,
            "wrong honesty class for {rt}"
        );
    }
}

#[tokio::test]
async fn test_53_honesty_class_partially_supported() {
    let runtime = InMemoryAppRuntime::new();

    for rt in [
        EcosystemRuntime::RuntimeAppimage,
        EcosystemRuntime::RuntimeDistrobox,
        EcosystemRuntime::RuntimeWindowsProton,
        EcosystemRuntime::RuntimeAndroidWaydroid,
        EcosystemRuntime::RuntimeMacosDarling,
    ] {
        let behavior = ObservedBehavior {
            observation_id: format!("obs_{rt}"),
            artifact_hash: "abc".into(),
            observed_for_seconds: 1,
            observed_syscalls: vec![],
            blocked_filesystem_reads: vec![],
            blocked_filesystem_writes: vec![],
            attempted_dns_resolutions: vec![],
            attempted_outbound_endpoints: vec![],
            attempted_gpu_init: false,
            attempted_audio_init: false,
            attempted_microphone_open: false,
            attempted_camera_open: false,
            attempted_clipboard_read: false,
            attempted_clipboard_write: false,
            error_messages_redacted: vec![],
            process_terminated_normally: true,
            exit_code: 0,
        };
        let proposal = runtime
            .translate_manifest(
                behavior,
                ManifestTranslationStrategy::AndroidManifestXml,
                rt,
            )
            .await
            .expect("translate");
        assert_eq!(
            proposal.honesty_class,
            EcosystemHonestyClass::PartiallySupported,
            "wrong honesty class for {rt}"
        );
    }
}

#[tokio::test]
async fn test_54_honesty_class_requires_vm() {
    let runtime = InMemoryAppRuntime::new();

    for rt in [
        EcosystemRuntime::RuntimeWindowsVm,
        EcosystemRuntime::RuntimeAndroidVmWithGms,
        EcosystemRuntime::RuntimeMacosVm,
    ] {
        let behavior = ObservedBehavior {
            observation_id: format!("obs_{rt}"),
            artifact_hash: "abc".into(),
            observed_for_seconds: 1,
            observed_syscalls: vec![],
            blocked_filesystem_reads: vec![],
            blocked_filesystem_writes: vec![],
            attempted_dns_resolutions: vec![],
            attempted_outbound_endpoints: vec![],
            attempted_gpu_init: false,
            attempted_audio_init: false,
            attempted_microphone_open: false,
            attempted_camera_open: false,
            attempted_clipboard_read: false,
            attempted_clipboard_write: false,
            error_messages_redacted: vec![],
            process_terminated_normally: true,
            exit_code: 0,
        };
        let proposal = runtime
            .translate_manifest(
                behavior,
                ManifestTranslationStrategy::AndroidManifestXml,
                rt,
            )
            .await
            .expect("translate");
        assert_eq!(
            proposal.honesty_class,
            EcosystemHonestyClass::RequiresVm,
            "wrong honesty class for {rt}"
        );
    }
}

#[tokio::test]
async fn test_55_honesty_class_not_runnable() {
    let runtime = InMemoryAppRuntime::new();
    let behavior = ObservedBehavior {
        observation_id: "obs_apple".into(),
        artifact_hash: "abc".into(),
        observed_for_seconds: 1,
        observed_syscalls: vec![],
        blocked_filesystem_reads: vec![],
        blocked_filesystem_writes: vec![],
        attempted_dns_resolutions: vec![],
        attempted_outbound_endpoints: vec![],
        attempted_gpu_init: false,
        attempted_audio_init: false,
        attempted_microphone_open: false,
        attempted_camera_open: false,
        attempted_clipboard_read: false,
        attempted_clipboard_write: false,
        error_messages_redacted: vec![],
        process_terminated_normally: true,
        exit_code: 0,
    };

    let proposal = runtime
        .translate_manifest(
            behavior,
            ManifestTranslationStrategy::AndroidManifestXml,
            EcosystemRuntime::RuntimeRemoteAppleBridge,
        )
        .await
        .expect("translate");
    assert_eq!(
        proposal.honesty_class,
        EcosystemHonestyClass::NotRunnableOnNonNative
    );
}

// ============================================================================
// AppRuntime trait object usability
// ============================================================================

#[tokio::test]
async fn test_56_trait_object_usable() {
    let runtime: std::sync::Arc<dyn AppRuntime> = std::sync::Arc::new(InMemoryAppRuntime::new());
    let behavior = runtime
        .observe_in_sandbox("hash", EcosystemRuntime::RuntimeLinuxNative, 5)
        .await
        .expect("observe");
    assert!(behavior.process_terminated_normally);

    let delta = runtime
        .propose_manifest_delta("app_01", "re-audit")
        .await
        .expect("delta");
    assert_eq!(delta, ManifestDeltaOutcome::DeltaProposed);
}

// ============================================================================
// ObservationRejected — error display
// ============================================================================

#[test]
fn test_57_observation_rejected_error_display() {
    let err = AppsError::ObservationRejected {
        observation_id: "obs_01".into(),
        reason: "exceeds hard cap".into(),
    };
    let msg = err.to_string();
    assert!(msg.contains("obs_01"));
    assert!(msg.contains("exceeds hard cap"));
}

// ============================================================================
// Default impl
// ============================================================================

#[test]
fn test_58_in_memory_runtime_default() {
    let _runtime = InMemoryAppRuntime::default();
}
