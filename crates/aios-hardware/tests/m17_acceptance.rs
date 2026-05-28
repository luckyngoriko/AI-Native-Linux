//! T-174 — M17 acceptance tests (6-phase E2E).
//!
//! Each phase validates a cross-cutting integration pathway end-to-end:
//! 1. Boot → graph built + device lifecycle
//! 2. Policy → GPU binding → sandbox constraint + HardwareError → PolicyDecision
//! 3. Audit → DriverBinding → ActionEnvelope
//! 4. Evidence → emitter chain with all event types + chain integrity
//! 5. Recovery → drift detection → recovery signal
//! 6. Network → graph summary → posture hint

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::wildcard_imports,
    clippy::enum_glob_use,
    clippy::module_name_repetitions,
    clippy::missing_const_for_fn,
    clippy::needless_collect,
    clippy::format_collect,
    unused_imports,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::Utc;
use ed25519_dalek::SigningKey;
use rand_core::OsRng;

use aios_hardware::integration::capability_bridge::driver_binding_to_action_envelope;
use aios_hardware::integration::network_bridge::graph_summary;
use aios_hardware::integration::policy_bridge::hardware_error_to_policy_denial;
use aios_hardware::integration::recovery_bridge::drift_should_trigger_recovery;
use aios_hardware::integration::sandbox_bridge::gpu_binding_to_sandbox_constraint;

use aios_hardware::{
    BusKind, DeviceClass, DeviceLifecycleState, DeviceTrustClass, DmabufBroker, DriverBinding,
    DriverBindingRegistry, DriverProvenance, FirmwareApplyStrategy, FirmwareBlob, FirmwareBlobId,
    FirmwareScope, FirmwareTrustResult, FirmwareUpdateClass, FirmwareUpdateOrchestrator,
    FirmwareUpdateState, GpuCapabilityClass, GpuDevice, GpuId, GpuResourceRegistry, GpuVendorKind,
    HardwareDeviceRecord, HardwareError, HardwareErrorCode, HardwareGraphBuilder, HardwareManager,
    InMemoryHardwareManager, IommuFloorEnforcer, RemovableDevicePolicy, RemovableDevicePolicyTable,
};

use aios_hardware::evidence::{
    FirmwarePhaseRecord, HardwareEvidenceEmitter, HardwareRecordType,
    InMemoryHardwareEvidenceEmitter, WithEmitter,
};
use aios_hardware::ids::{DeviceId, DriverBindingId, FirmwareBlobId as FwBlobId, GpuId as GId};
use aios_hardware::{DeviceQuarantineReason, EvidenceReceipt};

// -- helpers ----------------------------------------------------------------

fn test_device(id: &str, class: DeviceClass, bus: BusKind) -> HardwareDeviceRecord {
    HardwareDeviceRecord {
        device_id: DeviceId(id.into()),
        class,
        bus,
        vendor_id: 0x8086,
        product_id: 0x9A49,
        vendor_name: "Intel".into(),
        product_name: "TestDevice".into(),
        trust_class: DeviceTrustClass::Untrusted,
        lifecycle: DeviceLifecycleState::Detected,
        driver_provenance: None,
        firmware_version: None,
        removable: false,
        iommu_protected: false,
        probed_at: Utc::now(),
    }
}

// =========================================================================
// PHASE 1: Boot — Graph built + device lifecycle (4 tests)
// =========================================================================

#[test]
fn phase1_graph_built_and_signed() {
    let sk = SigningKey::generate(&mut OsRng);
    let mut builder = HardwareGraphBuilder::new("host-accept");
    builder
        .add_device(test_device("cpu0", DeviceClass::Cpu, BusKind::Pcie))
        .expect("add_device");
    builder
        .add_device(test_device(
            "nvme0",
            DeviceClass::StorageNvme,
            BusKind::Nvme,
        ))
        .expect("add_device");
    builder
        .add_device(test_device("gpu0", DeviceClass::GpuDiscrete, BusKind::Pcie))
        .expect("add_device");

    let graph = builder
        .build_and_sign(&sk, "fp-accept")
        .expect("build_and_sign");

    assert!(!graph.id.0.is_empty(), "graph must have non-empty id");
    assert_eq!(graph.devices.len(), 3, "graph must contain 3 devices");
    assert!(!graph.signature.is_empty(), "graph must be signed");
}

#[tokio::test]
async fn phase1_device_register_deregister_lifecycle() {
    let manager = InMemoryHardwareManager::new();
    let record = test_device("dev-life", DeviceClass::NetworkEthernet, BusKind::Pcie);

    manager
        .register_device(record)
        .await
        .expect("register_device");

    let devices = manager.list_pending_devices().await;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].device_id.0, "dev-life");

    manager
        .deregister_device(&DeviceId("dev-life".into()))
        .await
        .expect("deregister_device");

    let after = manager.list_pending_devices().await;
    assert_eq!(after.len(), 0);
}

#[tokio::test]
async fn phase1_gpu_device_registered_and_queried() {
    let registry = GpuResourceRegistry::new();
    let gpu = GpuDevice {
        gpu_id: GpuId("gpu-boot".into()),
        vendor: GpuVendorKind::Amd,
        product_name: "Radeon Test".into(),
        vram_total_bytes: 16_000_000_000,
        supported_classes: vec![
            GpuCapabilityClass::RenderOnly,
            GpuCapabilityClass::ComputeOnly,
        ],
        iommu_protected: true,
        host_canonical_id: "host-boot".into(),
    };
    registry.register_device(gpu).await.expect("register GPU");
    let devices = registry.list_devices().await;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].vendor, GpuVendorKind::Amd);
}

#[tokio::test]
async fn phase1_firmware_orchestrator_initialized() {
    let orch = FirmwareUpdateOrchestrator::new();
    let blob = FirmwareBlob {
        blob_id: FirmwareBlobId("fw-boot".into()),
        update_class: FirmwareUpdateClass::CpuMicrocode,
        scope: FirmwareScope::Cpu,
        target_device: None,
        vendor_name: "Intel".into(),
        version: "1.0.0".into(),
        blake3_hash: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
        signature: vec![],
        signer_fingerprint: "fp-boot".into(),
        published_at: Utc::now(),
    };
    let plan = orch
        .propose(blob, FirmwareApplyStrategy::Atomic)
        .await
        .expect("propose update");
    assert!(
        !plan.blob.blob_id.0.is_empty(),
        "plan must have non-empty blob id"
    );
}

// =========================================================================
// PHASE 2: Policy — GPU binding → sandbox constraint + HardwareError → PolicyDecision (4 tests)
// =========================================================================

#[tokio::test]
async fn phase2_gpu_binding_flows_to_sandbox_constraint() {
    use aios_hardware::gpu_resource::GpuCapabilityBinding;

    let registry = GpuResourceRegistry::new();
    registry
        .register_device(GpuDevice {
            gpu_id: GpuId("gpu-pol".into()),
            vendor: GpuVendorKind::Nvidia,
            product_name: "RTX Test".into(),
            vram_total_bytes: 24_000_000_000,
            supported_classes: vec![GpuCapabilityClass::ComputeOnly],
            iommu_protected: true,
            host_canonical_id: "host-pol".into(),
        })
        .await
        .expect("register");

    let binding = registry
        .request_binding(aios_hardware::gpu_resource::BindingRequest {
            gpu_id: GpuId("gpu-pol".into()),
            group_id: "group:pol".into(),
            subject_canonical_id: "human:lucky".into(),
            capability_class: GpuCapabilityClass::ComputeOnly,
            vram_bytes: 4_000_000_000,
            ttl: None,
        })
        .await
        .expect("request binding");

    // Cross-crate bridge: hardware GpuCapabilityBinding → sandbox GpuPolicy
    let constraint = gpu_binding_to_sandbox_constraint(&binding);
    assert_eq!(
        constraint.gpu_capability_class,
        aios_sandbox::GpuCapabilityClass::GpuComputeHeavy
    );
    assert!(constraint.vk_device_required);
    assert!(!constraint.dmabuf_passthrough_allowed);
    assert!(constraint.per_group_partitioning);
    assert!(constraint.iommu_required);
}

#[test]
fn phase2_hardware_error_to_policy_denial_all_variants() {
    let errors: Vec<HardwareError> = vec![
        HardwareError::DeviceNotFound(DeviceId("d".into())),
        HardwareError::IommuMissing(DeviceId("d".into())),
        HardwareError::ThunderboltUnauthorized(DeviceId("d".into())),
        HardwareError::CapabilityLie {
            device: DeviceId("d".into()),
            advertised: "PCIe Gen4".into(),
            observed: "PCIe Gen2".into(),
        },
        HardwareError::RemovableDenied {
            device: DeviceId("d".into()),
            policy: RemovableDevicePolicy::DenyDefault,
        },
        HardwareError::GpuVramExhausted {
            gpu: GpuId("g".into()),
            requested: 4096,
            available: 512,
        },
        HardwareError::FirmwareUnsigned(FirmwareBlobId("f".into())),
        HardwareError::Internal("test".into()),
    ];

    for (i, err) in errors.iter().enumerate() {
        let decision = hardware_error_to_policy_denial(err, &format!("poldec_phase2_{i}"));
        assert_eq!(
            decision.decision,
            aios_policy::Decision::Deny,
            "every HardwareError must produce Deny"
        );
        assert!(
            !decision.reason_code.is_empty(),
            "reason_code must be non-empty for variant {i}"
        );
        assert!(!decision.policy_decision_id.is_empty());
    }
}

#[test]
fn phase2_gpu_binding_all_five_classes_map_to_sandbox() {
    let pairs = [
        (
            GpuCapabilityClass::RenderOnly,
            aios_sandbox::GpuCapabilityClass::GpuFull3d,
        ),
        (
            GpuCapabilityClass::ComputeOnly,
            aios_sandbox::GpuCapabilityClass::GpuComputeHeavy,
        ),
        (
            GpuCapabilityClass::RenderAndCompute,
            aios_sandbox::GpuCapabilityClass::GpuFull3d,
        ),
        (
            GpuCapabilityClass::VideoEncode,
            aios_sandbox::GpuCapabilityClass::GpuRich2d,
        ),
        (
            GpuCapabilityClass::VideoDecode,
            aios_sandbox::GpuCapabilityClass::GpuRich2d,
        ),
    ];

    for (hw_class, expected_sandbox) in &pairs {
        let binding = aios_hardware::gpu_resource::GpuCapabilityBinding {
            binding_id: "test-map".into(),
            gpu_id: GpuId("g".into()),
            group_id: "group:x".into(),
            subject_canonical_id: "human:test".into(),
            capability_class: *hw_class,
            vram_bytes_reserved: 256 * 1024 * 1024,
            vk_device_partition_id: "vkdp".into(),
            bound_at: Utc::now(),
            expires_at: None,
        };
        let constraint = gpu_binding_to_sandbox_constraint(&binding);
        assert_eq!(
            constraint.gpu_capability_class, *expected_sandbox,
            "{hw_class:?} must map to {expected_sandbox:?}"
        );
    }
}

#[tokio::test]
async fn phase2_iommu_enforcement_and_removable_policy_integration() {
    assert!(IommuFloorEnforcer::iommu_required_for_bus(
        BusKind::Thunderbolt
    ));
    assert!(IommuFloorEnforcer::iommu_required_for_bus(BusKind::Usb4));
    assert!(!IommuFloorEnforcer::iommu_required_for_bus(BusKind::Usb2));

    let enforcer = IommuFloorEnforcer::new();
    enforcer
        .record_observation(DeviceId("tb-dev".into()), BusKind::Thunderbolt, true)
        .await
        .expect("record_observation");

    let table = RemovableDevicePolicyTable::new();
    table
        .set_policy(
            DeviceId("usb-dev".into()),
            RemovableDevicePolicy::DenyDefault,
            "human:operator",
        )
        .await
        .expect("set_policy");
    let policy = table.get_policy(&DeviceId("usb-dev".into())).await;
    assert_eq!(policy, RemovableDevicePolicy::DenyDefault);
}

// =========================================================================
// PHASE 3: Audit — DriverBinding → ActionEnvelope (2 tests)
// =========================================================================

#[test]
fn phase3_driver_binding_to_action_envelope() {
    let binding = DriverBinding {
        binding_id: DriverBindingId("drvb-audit".into()),
        device_id: DeviceId("dev-audit".into()),
        driver_module_name: "i915".into(),
        kernel_module_version: "6.8.0".into(),
        provenance: DriverProvenance::AiosVerified,
        blake3_hash: "abc123def456".into(),
        signer_fingerprint: "aabbccdd".into(),
        signature: vec![1, 2, 3],
        admitted_at: Utc::now(),
    };

    let envelope = driver_binding_to_action_envelope(&binding, "human:lucky");
    assert_eq!(
        envelope.request.action, "hardware.driver.bind_request",
        "action must be hardware.driver.bind_request"
    );
    assert_eq!(envelope.identity.subject_canonical_id, "human:lucky");

    // Verify the payload carries binding metadata
    let payload = &envelope.request.target;
    assert_eq!(payload["binding_id"], "drvb-audit");
    assert_eq!(payload["device_id"], "dev-audit");
    assert_eq!(payload["driver_module_name"], "i915");
    assert_eq!(payload["provenance"], "aios-verified");
}

#[test]
fn phase3_driver_binding_ai_requester() {
    let binding = DriverBinding {
        binding_id: DriverBindingId("drvb-ai".into()),
        device_id: DeviceId("dev-ai".into()),
        driver_module_name: "amdgpu".into(),
        kernel_module_version: "6.8.0".into(),
        provenance: DriverProvenance::SignedKernelModule,
        blake3_hash: "hash456".into(),
        signer_fingerprint: "eeff0011".into(),
        signature: vec![4, 5, 6],
        admitted_at: Utc::now(),
    };

    let envelope = driver_binding_to_action_envelope(&binding, "agent:dev");
    assert_eq!(envelope.identity.subject_canonical_id, "agent:dev");
    assert!(
        !envelope.identity.is_ai,
        "caller sets is_ai: false; cognitive core may override"
    );
}

// =========================================================================
// PHASE 4: Evidence — Emitter chain with all event types (3 tests)
// =========================================================================

#[tokio::test]
async fn phase4_evidence_chain_with_all_event_types() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:phase4");
    let record = test_device("dev-ev", DeviceClass::GpuDiscrete, BusKind::Pcie);

    // Device lifecycle events
    e.emit_device_registered(&record).await.expect("emit");
    e.emit_device_lifecycle_transitioned(
        &DeviceId("dev-ev".into()),
        DeviceLifecycleState::Detected,
        DeviceLifecycleState::Active,
    )
    .await
    .expect("emit");

    // IOMMU / Thunderbolt
    e.emit_iommu_missing(&DeviceId("dev-iommu".into()), BusKind::Thunderbolt)
        .await
        .expect("emit");
    e.emit_thunderbolt_unauthorized(&DeviceId("dev-tb".into()))
        .await
        .expect("emit");

    // GPU
    e.emit_gpu_device_registered(&GpuDevice {
        gpu_id: GpuId("gpu-ev".into()),
        vendor: GpuVendorKind::Nvidia,
        product_name: "RTX".into(),
        vram_total_bytes: 8_000_000_000,
        supported_classes: vec![GpuCapabilityClass::ComputeOnly],
        iommu_protected: true,
        host_canonical_id: "host-ev".into(),
    })
    .await
    .expect("emit");

    // Removable
    e.emit_removable_admission_denied(
        &DeviceId("usb-ev".into()),
        RemovableDevicePolicy::DenyDefault,
    )
    .await
    .expect("emit");

    // Firmware
    let blob = FirmwareBlob {
        blob_id: FirmwareBlobId("fw-ev".into()),
        update_class: FirmwareUpdateClass::CpuMicrocode,
        scope: FirmwareScope::BiosUefi,
        target_device: None,
        vendor_name: "Intel".into(),
        version: "1.0".into(),
        blake3_hash: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
        signature: vec![],
        signer_fingerprint: "fp-ev".into(),
        published_at: Utc::now(),
    };
    let plan = aios_hardware::firmware_update::FirmwareUpdatePlan {
        blob,
        current_state: FirmwareUpdateState::Proposed,
        apply_strategy: FirmwareApplyStrategy::Atomic,
        trust_result: None,
        history: vec![],
        installed_version_before: None,
    };
    e.emit_firmware_event(&plan, FirmwarePhaseRecord::Proposed)
        .await
        .expect("emit");

    // Verify chain integrity
    assert_eq!(e.receipt_count().await, 7);
    e.verify_chain().await.expect("chain integrity must hold");
}

#[tokio::test]
async fn phase4_evidence_forever_retention_classes() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:phase4b");

    // Events that trigger FOREVER retention
    e.emit_iommu_missing(&DeviceId("d1".into()), BusKind::Thunderbolt)
        .await
        .expect("emit");
    e.emit_thunderbolt_unauthorized(&DeviceId("d2".into()))
        .await
        .expect("emit");
    e.emit_dmabuf_peer_unauthorized("hdl-1", &GpuId("a".into()), &GpuId("b".into()))
        .await
        .expect("emit");
    e.emit_firmware_unsigned_refused(&FirmwareBlobId("fw-u".into()), "fp-none")
        .await
        .expect("emit");

    assert_eq!(e.receipt_count().await, 4);
    e.verify_chain().await.expect("chain integrity");
}

#[tokio::test]
async fn phase4_evidence_serde_roundtrip_and_emitter_new() {
    let receipt = EvidenceReceipt {
        record_id: "evr-test".into(),
        hash: "deadbeef".into(),
        sequence: 7,
    };
    let json = serde_json::to_string(&receipt).expect("serialize");
    let back: EvidenceReceipt = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.record_id, receipt.record_id);
    assert_eq!(back.sequence, 7);

    let e = InMemoryHardwareEvidenceEmitter::new("_system:phase4c");
    assert_eq!(e.receipt_count().await, 0);
}

// =========================================================================
// PHASE 5: Recovery — Drift detection → recovery signal (3 tests)
// =========================================================================

#[tokio::test]
async fn phase5_drift_no_drift_signal() {
    use aios_hardware::drift::{DriftDetector, DriftSignal, PriorGraphStore};

    let store = Arc::new(PriorGraphStore::new());
    let detector = DriftDetector::new(store);

    let sk = SigningKey::generate(&mut OsRng);
    let mut builder = HardwareGraphBuilder::new("host-rec");
    builder
        .add_device(test_device("cpu-rec", DeviceClass::Cpu, BusKind::Pcie))
        .expect("add");
    let graph = builder
        .build_and_sign(&sk, "fp-rec")
        .expect("build_and_sign");

    // First boot — no prior graph stored
    let signal = detector.check(&graph).await.expect("check");
    assert!(
        matches!(signal, DriftSignal::FirstBoot { .. }),
        "first boot should be FirstBoot, not drift"
    );
}

#[tokio::test]
async fn phase5_recovery_bridge_signals() {
    use aios_hardware::drift::{EvilMaidEvidenceMarker, EvilMaidRecommendedAction, GraphDiff};
    use aios_hardware::HardwareGraphId;

    let recovery_marker = EvilMaidEvidenceMarker {
        prior: HardwareGraphId("pg-rec".into()),
        current: HardwareGraphId("cg-rec".into()),
        diff: GraphDiff {
            added: vec![],
            removed: vec![DeviceId("missing-dev".into())],
            modified: vec![],
            kept: 3,
        },
        detected_at: Utc::now(),
        recommended_action: EvilMaidRecommendedAction::EnterRecoveryMode,
    };
    assert!(
        drift_should_trigger_recovery(&recovery_marker),
        "EnterRecoveryMode must trigger recovery"
    );

    let quarantine_marker = EvilMaidEvidenceMarker {
        prior: HardwareGraphId("pg-q".into()),
        current: HardwareGraphId("cg-q".into()),
        diff: GraphDiff {
            added: vec![DeviceId("new-dev".into())],
            removed: vec![],
            modified: vec![],
            kept: 5,
        },
        detected_at: Utc::now(),
        recommended_action: EvilMaidRecommendedAction::AutoQuarantineNewDevices,
    };
    assert!(
        !drift_should_trigger_recovery(&quarantine_marker),
        "AutoQuarantineNewDevices must NOT trigger recovery"
    );

    let investigation_marker = EvilMaidEvidenceMarker {
        prior: HardwareGraphId("pg-inv".into()),
        current: HardwareGraphId("cg-inv".into()),
        diff: GraphDiff {
            added: vec![],
            removed: vec![],
            modified: vec![DeviceId("mod-dev".into())],
            kept: 4,
        },
        detected_at: Utc::now(),
        recommended_action: EvilMaidRecommendedAction::OperatorInvestigation,
    };
    assert!(
        !drift_should_trigger_recovery(&investigation_marker),
        "OperatorInvestigation must NOT trigger recovery"
    );
}

#[tokio::test]
async fn phase5_drift_detector_with_prior_graph() {
    use aios_hardware::drift::{DriftDetector, DriftSignal, PriorGraphStore};

    let sk = SigningKey::generate(&mut OsRng);

    let mut builder1 = HardwareGraphBuilder::new("host-drift");
    builder1
        .add_device(test_device("cpu", DeviceClass::Cpu, BusKind::Pcie))
        .expect("add");
    let graph1 = builder1.build_and_sign(&sk, "fp-drift").expect("build");

    let store = Arc::new(PriorGraphStore::new());
    store.store(&graph1).await;

    let detector = DriftDetector::new(store);

    // Same graph — no drift
    let signal = detector.check(&graph1).await.expect("check");
    assert!(
        matches!(signal, DriftSignal::NoDrift),
        "same graph must produce NoDrift"
    );

    // Different graph — drift detected
    let mut builder2 = HardwareGraphBuilder::new("host-drift");
    builder2
        .add_device(test_device("cpu", DeviceClass::Cpu, BusKind::Pcie))
        .expect("add");
    builder2
        .add_device(test_device(
            "new-gpu",
            DeviceClass::GpuDiscrete,
            BusKind::Pcie,
        ))
        .expect("add");
    let graph2 = builder2.build_and_sign(&sk, "fp-drift2").expect("build");

    let signal2 = detector.check(&graph2).await.expect("check");
    assert!(
        matches!(signal2, DriftSignal::DriftDetected { .. }),
        "different graph must produce DriftDetected"
    );
}

// =========================================================================
// PHASE 6: Network — Graph summary → posture hint (2 tests)
// =========================================================================

#[test]
fn phase6_graph_summary_to_posture_hint() {
    let sk = SigningKey::generate(&mut OsRng);
    let mut builder = HardwareGraphBuilder::new("host-net");

    builder
        .add_device(test_device(
            "eth0",
            DeviceClass::NetworkEthernet,
            BusKind::Pcie,
        ))
        .expect("add");
    builder
        .add_device(test_device(
            "wifi0",
            DeviceClass::NetworkWifi,
            BusKind::Usb3,
        ))
        .expect("add");
    builder
        .add_device(test_device("gpu0", DeviceClass::GpuDiscrete, BusKind::Pcie))
        .expect("add");

    let graph = builder.build_and_sign(&sk, "fp-net").expect("build");
    let hint = graph_summary(&graph);

    assert!(hint.has_ethernet);
    assert!(hint.has_wifi);
    assert!(hint.has_discrete_gpu);
    assert!(!hint.has_thunderbolt);
    assert_eq!(hint.device_count, 3);
    assert!(!hint.has_risk_signal(), "no thunderbolt = no risk signal");
}

#[test]
fn phase6_thunderbolt_triggers_risk_signal() {
    let sk = SigningKey::generate(&mut OsRng);
    let mut builder = HardwareGraphBuilder::new("host-tb");

    builder
        .add_device(test_device(
            "tb-ctrl",
            DeviceClass::ThunderboltController,
            BusKind::Thunderbolt,
        ))
        .expect("add");

    let graph = builder.build_and_sign(&sk, "fp-tb").expect("build");
    let hint = graph_summary(&graph);

    assert!(hint.has_thunderbolt);
    assert!(
        hint.has_risk_signal(),
        "thunderbolt must trigger risk signal"
    );
    assert_eq!(hint.device_count, 1);
}
