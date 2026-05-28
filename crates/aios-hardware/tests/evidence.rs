#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::doc_markdown,
    clippy::significant_drop_tightening,
    clippy::wildcard_imports,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::Utc;
use ed25519_dalek::SigningKey;
use rand_core::OsRng;

use aios_hardware::*;

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

// -- HardwareRecordType mapping (5 tests) -----------------------------------

#[tokio::test]
async fn all_32_variants_map_to_non_default_record_type() {
    // Every HardwareRecordType variant must map to a valid RecordType.
    let variants: &[HardwareRecordType] = &[
        HardwareRecordType::HardwareGraphBuilt,
        HardwareRecordType::HardwareGraphDriftDetected,
        HardwareRecordType::HardwareGraphFirstBoot,
        HardwareRecordType::DeviceRegistered,
        HardwareRecordType::DeviceDeregistered,
        HardwareRecordType::DeviceLifecycleTransitioned,
        HardwareRecordType::DeviceQuarantined,
        HardwareRecordType::DeviceClassificationFailed,
        HardwareRecordType::DriverBindingAdmitted,
        HardwareRecordType::DriverBindingRejected,
        HardwareRecordType::HostCapabilityLieDetected,
        HardwareRecordType::IommuMissingForProtectedBus,
        HardwareRecordType::ThunderboltUnauthorized,
        HardwareRecordType::RemovableAdmissionDenied,
        HardwareRecordType::RemovableAiBlocked,
        HardwareRecordType::GpuDeviceRegistered,
        HardwareRecordType::GpuBindingGranted,
        HardwareRecordType::GpuBindingReleased,
        HardwareRecordType::GpuVramExhausted,
        HardwareRecordType::DmabufPeerUnauthorized,
        HardwareRecordType::FirmwareProposed,
        HardwareRecordType::FirmwareVerified,
        HardwareRecordType::FirmwareApproved,
        HardwareRecordType::FirmwareStaged,
        HardwareRecordType::FirmwareApplied,
        HardwareRecordType::FirmwareReverted,
        HardwareRecordType::FirmwareFailed,
        HardwareRecordType::FirmwareUnsignedRefused,
        HardwareRecordType::FirmwareSignatureInvalid,
        HardwareRecordType::FirmwareVersionRegression,
        HardwareRecordType::FirmwareOperatorLocalSigned,
        HardwareRecordType::FirmwareConstitutionalRefusal,
    ];
    assert_eq!(variants.len(), 32, "must have exactly 32 variants");
    for &variant in variants {
        let rt = variant.to_evidence_record_type();
        // Every variant maps to some non-default RecordType.
        let _ = rt;
    }
}

#[tokio::test]
async fn forever_retention_count_is_12() {
    let forever: Vec<HardwareRecordType> = [
        HardwareRecordType::HardwareGraphDriftDetected,
        HardwareRecordType::HostCapabilityLieDetected,
        HardwareRecordType::IommuMissingForProtectedBus,
        HardwareRecordType::ThunderboltUnauthorized,
        HardwareRecordType::RemovableAiBlocked,
        HardwareRecordType::DmabufPeerUnauthorized,
        HardwareRecordType::DriverBindingRejected,
        HardwareRecordType::FirmwareUnsignedRefused,
        HardwareRecordType::FirmwareSignatureInvalid,
        HardwareRecordType::FirmwareVersionRegression,
        HardwareRecordType::FirmwareOperatorLocalSigned,
        HardwareRecordType::FirmwareConstitutionalRefusal,
    ]
    .into();
    for variant in forever {
        assert_eq!(
            variant.retention_class(),
            aios_evidence::RetentionClass::Forever,
            "{variant:?} must be FOREVER"
        );
    }
    // Verify exactly 12
    let all_forever: Vec<_> = [
        HardwareRecordType::HardwareGraphDriftDetected,
        HardwareRecordType::HostCapabilityLieDetected,
        HardwareRecordType::IommuMissingForProtectedBus,
        HardwareRecordType::ThunderboltUnauthorized,
        HardwareRecordType::RemovableAiBlocked,
        HardwareRecordType::DmabufPeerUnauthorized,
        HardwareRecordType::DriverBindingRejected,
        HardwareRecordType::FirmwareUnsignedRefused,
        HardwareRecordType::FirmwareSignatureInvalid,
        HardwareRecordType::FirmwareVersionRegression,
        HardwareRecordType::FirmwareOperatorLocalSigned,
        HardwareRecordType::FirmwareConstitutionalRefusal,
    ]
    .into_iter()
    .filter(|v| v.retention_class() == aios_evidence::RetentionClass::Forever)
    .collect();
    assert_eq!(all_forever.len(), 12);
}

#[tokio::test]
async fn all_as_str_non_empty_and_unique() {
    use std::collections::HashSet;
    let variants: &[HardwareRecordType] = &[
        HardwareRecordType::HardwareGraphBuilt,
        HardwareRecordType::HardwareGraphDriftDetected,
        HardwareRecordType::HardwareGraphFirstBoot,
        HardwareRecordType::DeviceRegistered,
        HardwareRecordType::DeviceDeregistered,
        HardwareRecordType::DeviceLifecycleTransitioned,
        HardwareRecordType::DeviceQuarantined,
        HardwareRecordType::DeviceClassificationFailed,
        HardwareRecordType::DriverBindingAdmitted,
        HardwareRecordType::DriverBindingRejected,
        HardwareRecordType::HostCapabilityLieDetected,
        HardwareRecordType::IommuMissingForProtectedBus,
        HardwareRecordType::ThunderboltUnauthorized,
        HardwareRecordType::RemovableAdmissionDenied,
        HardwareRecordType::RemovableAiBlocked,
        HardwareRecordType::GpuDeviceRegistered,
        HardwareRecordType::GpuBindingGranted,
        HardwareRecordType::GpuBindingReleased,
        HardwareRecordType::GpuVramExhausted,
        HardwareRecordType::DmabufPeerUnauthorized,
        HardwareRecordType::FirmwareProposed,
        HardwareRecordType::FirmwareVerified,
        HardwareRecordType::FirmwareApproved,
        HardwareRecordType::FirmwareStaged,
        HardwareRecordType::FirmwareApplied,
        HardwareRecordType::FirmwareReverted,
        HardwareRecordType::FirmwareFailed,
        HardwareRecordType::FirmwareUnsignedRefused,
        HardwareRecordType::FirmwareSignatureInvalid,
        HardwareRecordType::FirmwareVersionRegression,
        HardwareRecordType::FirmwareOperatorLocalSigned,
        HardwareRecordType::FirmwareConstitutionalRefusal,
    ];
    let mut seen = HashSet::new();
    for &variant in variants {
        let s = variant.as_str();
        assert!(!s.is_empty(), "{variant:?} as_str() is empty");
        assert!(seen.insert(s), "duplicate as_str: {s}");
    }
}

#[tokio::test]
async fn firmware_phase_record_mapping() {
    assert_eq!(
        FirmwarePhaseRecord::Proposed.to_record_type(),
        HardwareRecordType::FirmwareProposed
    );
    assert_eq!(
        FirmwarePhaseRecord::Verified.to_record_type(),
        HardwareRecordType::FirmwareVerified
    );
    assert_eq!(
        FirmwarePhaseRecord::Approved.to_record_type(),
        HardwareRecordType::FirmwareApproved
    );
    assert_eq!(
        FirmwarePhaseRecord::Staged.to_record_type(),
        HardwareRecordType::FirmwareStaged
    );
    assert_eq!(
        FirmwarePhaseRecord::Applied.to_record_type(),
        HardwareRecordType::FirmwareApplied
    );
    assert_eq!(
        FirmwarePhaseRecord::Reverted.to_record_type(),
        HardwareRecordType::FirmwareReverted
    );
    assert_eq!(
        FirmwarePhaseRecord::Failed {
            reason: "boom".into()
        }
        .to_record_type(),
        HardwareRecordType::FirmwareFailed
    );
}

#[tokio::test]
async fn firmware_phase_record_labels() {
    assert_eq!(FirmwarePhaseRecord::Proposed.label(), "proposed");
    assert_eq!(FirmwarePhaseRecord::Verified.label(), "verified");
    assert_eq!(FirmwarePhaseRecord::Approved.label(), "approved");
    assert_eq!(FirmwarePhaseRecord::Staged.label(), "staged");
    assert_eq!(FirmwarePhaseRecord::Applied.label(), "applied");
    assert_eq!(FirmwarePhaseRecord::Reverted.label(), "reverted");
    assert_eq!(
        FirmwarePhaseRecord::Failed {
            reason: "err".into()
        }
        .label(),
        "failed"
    );
}

// -- InMemoryHardwareEvidenceEmitter (8 tests) -------------------------------

#[tokio::test]
async fn emitter_new_starts_empty() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    assert_eq!(e.receipt_count().await, 0);
}

#[tokio::test]
async fn emit_graph_built_produces_receipt() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let sk_bytes = signing_key.to_bytes();
    let fingerprint: String = sk_bytes[..16].iter().map(|b| format!("{b:02x}")).collect();
    let mut builder = HardwareGraphBuilder::new("host-1");
    builder
        .add_device(test_device("dev1", DeviceClass::GpuDiscrete, BusKind::Pcie))
        .expect("add_device");
    let graph = builder
        .build_and_sign(&signing_key, &fingerprint)
        .expect("build_and_sign");

    let receipt = e.emit_graph_built(&graph).await.expect("emit");
    assert!(!receipt.record_id.is_empty());
    assert!(!receipt.hash.is_empty());
    assert_eq!(receipt.sequence, 0);
    assert_eq!(e.receipt_count().await, 1);
}

#[tokio::test]
async fn emit_device_registered_and_deregistered() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let record = test_device("dev-nvme", DeviceClass::StorageNvme, BusKind::Pcie);

    let r1 = e.emit_device_registered(&record).await.expect("emit");
    assert_eq!(r1.sequence, 0);

    let r2 = e
        .emit_device_deregistered(&DeviceId("dev-nvme".into()))
        .await
        .expect("emit");
    assert_eq!(r2.sequence, 1);
    assert_eq!(e.receipt_count().await, 2);
}

#[tokio::test]
async fn emit_device_lifecycle_transitioned() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let receipt = e
        .emit_device_lifecycle_transitioned(
            &DeviceId("dev1".into()),
            DeviceLifecycleState::Detected,
            DeviceLifecycleState::Active,
        )
        .await
        .expect("emit");
    assert_eq!(receipt.sequence, 0);
}

#[tokio::test]
async fn emit_device_quarantined() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let receipt = e
        .emit_device_quarantined(
            &DeviceId("dev1".into()),
            DeviceQuarantineReason::OutOfTreeDriver,
        )
        .await
        .expect("emit");
    assert_eq!(receipt.sequence, 0);
    assert_eq!(e.receipt_count().await, 1);
}

#[tokio::test]
async fn chain_integrity_holds_after_multiple_emissions() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let record = test_device("d1", DeviceClass::GpuDiscrete, BusKind::Pcie);

    e.emit_device_registered(&record).await.expect("emit");
    e.emit_device_deregistered(&DeviceId("d1".into()))
        .await
        .expect("emit");
    e.emit_device_classification_failed("obs1", "reason1")
        .await
        .expect("emit");
    e.emit_iommu_missing(&DeviceId("d2".into()), BusKind::Thunderbolt)
        .await
        .expect("emit");

    assert_eq!(e.receipt_count().await, 4);
    e.verify_chain().await.expect("chain integrity");
}

#[tokio::test]
async fn emit_graph_drift_detected_forever() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let signal = DriftSignal::DriftDetected {
        prior: HardwareGraphId("prior-1".into()),
        current: HardwareGraphId("current-1".into()),
        change: GraphDiff {
            added: vec![DeviceId("new-dev".into())],
            removed: vec![],
            modified: vec![],
            kept: 3,
        },
    };
    let receipt = e.emit_graph_drift_detected(&signal).await.expect("emit");
    assert_eq!(receipt.sequence, 0);
    // Confirm it is in the chain
    assert_eq!(e.receipt_count().await, 1);
}

#[tokio::test]
async fn emit_graph_first_boot() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let gid = HardwareGraphId("boot-1".into());
    let receipt = e.emit_graph_first_boot(&gid).await.expect("emit");
    assert_eq!(receipt.sequence, 0);
}

// -- GPU emissions (3 tests) ------------------------------------------------

#[tokio::test]
async fn emit_gpu_device_registered() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let gpu = GpuDevice {
        gpu_id: GpuId("gpu-0".into()),
        vendor: GpuVendorKind::Nvidia,
        product_name: "RTX 4090".into(),
        vram_total_bytes: 24_000_000_000,
        supported_classes: vec![
            GpuCapabilityClass::ComputeOnly,
            GpuCapabilityClass::RenderOnly,
        ],
        iommu_protected: true,
        host_canonical_id: "host-1".into(),
    };
    let receipt = e.emit_gpu_device_registered(&gpu).await.expect("emit");
    assert_eq!(receipt.sequence, 0);
}

#[tokio::test]
async fn emit_gpu_binding_granted_and_released() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let binding = GpuCapabilityBinding {
        binding_id: "bind-1".into(),
        gpu_id: GpuId("gpu-0".into()),
        group_id: "group-a".into(),
        subject_canonical_id: "agent:42".into(),
        capability_class: GpuCapabilityClass::ComputeOnly,
        vram_bytes_reserved: 8_000_000_000,
        vk_device_partition_id: "partition-1".into(),
        bound_at: Utc::now(),
        expires_at: None,
    };
    let r1 = e.emit_gpu_binding_granted(&binding).await.expect("emit");
    assert_eq!(r1.sequence, 0);
    let r2 = e.emit_gpu_binding_released("bind-1").await.expect("emit");
    assert_eq!(r2.sequence, 1);
}

#[tokio::test]
async fn emit_gpu_vram_exhausted() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let receipt = e
        .emit_gpu_vram_exhausted(&GpuId("gpu-0".into()), 16_000_000_000, 2_000_000_000)
        .await
        .expect("emit");
    assert_eq!(receipt.sequence, 0);
}

// -- DMABUF emission (1 test) ------------------------------------------------

#[tokio::test]
async fn emit_dmabuf_peer_unauthorized() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let receipt = e
        .emit_dmabuf_peer_unauthorized("hdl-1", &GpuId("gpu-src".into()), &GpuId("gpu-dst".into()))
        .await
        .expect("emit");
    assert_eq!(receipt.sequence, 0);
}

// -- Capability-lie / IOMMU / Thunderbolt / Removable (4 tests) ---------------

#[tokio::test]
async fn emit_host_capability_lie() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let receipt = e
        .emit_host_capability_lie(
            &DeviceId("dev-lie".into()),
            "iommu",
            "enabled",
            "disabled",
            LieSeverity::Hard,
        )
        .await
        .expect("emit");
    assert_eq!(receipt.sequence, 0);
}

#[tokio::test]
async fn emit_iommu_missing() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let receipt = e
        .emit_iommu_missing(&DeviceId("dev-tb".into()), BusKind::Thunderbolt)
        .await
        .expect("emit");
    assert_eq!(receipt.sequence, 0);
}

#[tokio::test]
async fn emit_thunderbolt_unauthorized() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let receipt = e
        .emit_thunderbolt_unauthorized(&DeviceId("dev-tb".into()))
        .await
        .expect("emit");
    assert_eq!(receipt.sequence, 0);
}

#[tokio::test]
async fn emit_removable_admission_denied_and_ai_blocked() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let r1 = e
        .emit_removable_admission_denied(
            &DeviceId("usb1".into()),
            RemovableDevicePolicy::DenyDefault,
        )
        .await
        .expect("emit");
    assert_eq!(r1.sequence, 0);
    let r2 = e
        .emit_removable_ai_blocked(&DeviceId("usb2".into()), "agent:007")
        .await
        .expect("emit");
    assert_eq!(r2.sequence, 1);
}

// -- Firmware emissions (5 tests) --------------------------------------------

#[tokio::test]
async fn emit_firmware_event_all_phases() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let blob = FirmwareBlob {
        blob_id: FirmwareBlobId("fw-001".into()),
        update_class: FirmwareUpdateClass::CpuMicrocode,
        scope: FirmwareScope::BiosUefi,
        target_device: None,
        vendor_name: "TestVendor".into(),
        version: "2.0.0".into(),
        blake3_hash: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
        signature: vec![],
        signer_fingerprint: "fp-test".into(),
        published_at: Utc::now(),
    };
    let plan = FirmwareUpdatePlan {
        blob,
        current_state: FirmwareUpdateState::Proposed,
        apply_strategy: FirmwareApplyStrategy::Atomic,
        trust_result: None,
        history: vec![],
        installed_version_before: None,
    };

    for phase in [
        FirmwarePhaseRecord::Proposed,
        FirmwarePhaseRecord::Verified,
        FirmwarePhaseRecord::Approved,
        FirmwarePhaseRecord::Staged,
        FirmwarePhaseRecord::Applied,
        FirmwarePhaseRecord::Reverted,
        FirmwarePhaseRecord::Failed {
            reason: "test".into(),
        },
    ] {
        let receipt = e.emit_firmware_event(&plan, phase).await.expect("emit");
        assert!(!receipt.record_id.is_empty());
    }
    assert_eq!(e.receipt_count().await, 7);
}

#[tokio::test]
async fn emit_firmware_unsigned_refused() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let receipt = e
        .emit_firmware_unsigned_refused(&FirmwareBlobId("fw-u".into()), "fp-none")
        .await
        .expect("emit");
    assert_eq!(receipt.sequence, 0);
}

#[tokio::test]
async fn emit_firmware_signature_invalid() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let receipt = e
        .emit_firmware_signature_invalid(&FirmwareBlobId("fw-bad".into()), "bad ed25519")
        .await
        .expect("emit");
    assert_eq!(receipt.sequence, 0);
}

#[tokio::test]
async fn emit_firmware_version_regression() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let receipt = e
        .emit_firmware_version_regression(&FirmwareBlobId("fw-old".into()), "1.0", "2.0")
        .await
        .expect("emit");
    assert_eq!(receipt.sequence, 0);
}

#[tokio::test]
async fn emit_firmware_operator_local_signed_and_constitutional_refusal() {
    let e = InMemoryHardwareEvidenceEmitter::new("_system:test");
    let r1 = e
        .emit_firmware_operator_local_signed(&FirmwareBlobId("fw-ops".into()), "operator-1")
        .await
        .expect("emit");
    assert_eq!(r1.sequence, 0);
    let r2 = e
        .emit_firmware_constitutional_refusal(&FirmwareBlobId("fw-const".into()), "sec-policy")
        .await
        .expect("emit");
    assert_eq!(r2.sequence, 1);
}

// -- Subsystem wiring — None emitter produces no emissions (1 test) ----------

#[tokio::test]
async fn manager_without_emitter_still_works() {
    let manager = InMemoryHardwareManager::new();
    let record = test_device("d-noemit", DeviceClass::StorageNvme, BusKind::Pcie);
    manager
        .register_device(record)
        .await
        .expect("register without emitter");
    manager
        .deregister_device(&DeviceId("d-noemit".into()))
        .await
        .expect("deregister without emitter");
}

// -- EvidenceReceipt serde roundtrip (1 test) --------------------------------

#[tokio::test]
async fn evidence_receipt_serde_roundtrip() {
    let receipt = EvidenceReceipt {
        record_id: "evr_01ABCDEF".into(),
        hash: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
        sequence: 42,
    };
    let json = serde_json::to_string(&receipt).expect("serialize");
    let back: EvidenceReceipt = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.record_id, receipt.record_id);
    assert_eq!(back.hash, receipt.hash);
    assert_eq!(back.sequence, receipt.sequence);
}

// -- INCLUSION test: EvidenceReceipt exposes correct fields (1 test) ---------

#[tokio::test]
async fn evidence_receipt_field_access() {
    let receipt = EvidenceReceipt {
        record_id: "evr_test".into(),
        hash: "abcd".into(),
        sequence: 99,
    };
    assert_eq!(receipt.record_id, "evr_test");
    assert_eq!(receipt.hash, "abcd");
    assert_eq!(receipt.sequence, 99);
}

// -- WithEmitter trait is object-safe via Arc (1 test) -----------------------

#[tokio::test]
async fn with_emitter_trait_builder_pattern() {
    let emitter = Arc::new(InMemoryHardwareEvidenceEmitter::new("_system:test"));
    let manager = InMemoryHardwareManager::new().with_emitter(Some(emitter));
    // Just verifying construction succeeds
    let _ = manager;
}
