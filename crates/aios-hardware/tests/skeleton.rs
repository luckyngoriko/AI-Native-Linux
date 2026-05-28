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

use aios_hardware::*;
use strum::EnumCount;
use strum::IntoEnumIterator;

#[test]
fn default_code_version_constant_is_correct() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-hardware/0.1.0-T174");
}

#[test]
fn device_class_has_16_variants() {
    assert_eq!(DeviceClass::COUNT, 16);
}

#[test]
fn bus_kind_has_8_variants() {
    assert_eq!(BusKind::COUNT, 8);
}

#[test]
fn driver_provenance_has_5_variants_with_aios_verified_first() {
    assert_eq!(DriverProvenance::COUNT, 5);
    let mut iter = DriverProvenance::iter();
    assert_eq!(iter.next(), Some(DriverProvenance::AiosVerified));
}

#[test]
fn device_lifecycle_state_has_8_variants() {
    assert_eq!(DeviceLifecycleState::COUNT, 8);
}

#[test]
fn device_trust_class_has_5_variants() {
    assert_eq!(DeviceTrustClass::COUNT, 5);
}

#[test]
fn device_quarantine_reason_has_8_variants() {
    assert_eq!(DeviceQuarantineReason::COUNT, 8);
}

#[test]
fn removable_device_policy_has_5_variants_including_recovery_denied() {
    assert_eq!(RemovableDevicePolicy::COUNT, 5);
    // Verify RecoveryDenied is the last variant
    assert_eq!(
        serde_json::to_value(RemovableDevicePolicy::RecoveryDenied).unwrap(),
        serde_json::json!("RecoveryDenied")
    );
}

#[test]
fn gpu_capability_class_has_5_variants_per_s8_2() {
    assert_eq!(GpuCapabilityClass::COUNT, 5);
}

#[test]
fn gpu_vendor_kind_has_at_least_5_variants() {
    assert_eq!(GpuVendorKind::COUNT, 6);
}

#[test]
fn firmware_update_class_has_5_variants_per_s8_5() {
    assert_eq!(FirmwareUpdateClass::COUNT, 5);
}

#[test]
fn firmware_scope_has_8_variants() {
    assert_eq!(FirmwareScope::COUNT, 8);
}

#[test]
fn firmware_update_state_has_8_variants_with_proposed_first_and_reverted_last() {
    assert_eq!(FirmwareUpdateState::COUNT, 8);
    let mut iter = FirmwareUpdateState::iter();
    assert_eq!(iter.next(), Some(FirmwareUpdateState::Proposed));
    assert_eq!(iter.next_back(), Some(FirmwareUpdateState::Reverted));
}

#[test]
fn firmware_trust_result_has_8_variants_including_unsigned_refused() {
    assert_eq!(FirmwareTrustResult::COUNT, 8);
    // Verify UnsignedRefused exists
    assert_eq!(
        serde_json::to_value(FirmwareTrustResult::UnsignedRefused).unwrap(),
        serde_json::json!("UnsignedRefused")
    );
}

#[test]
fn firmware_apply_strategy_has_3_variants_atomic_staged_deferred() {
    assert_eq!(FirmwareApplyStrategy::COUNT, 3);
}

#[test]
fn firmware_defer_reason_has_5_variants_including_battery() {
    assert_eq!(FirmwareDeferReason::COUNT, 5);
    assert_eq!(
        serde_json::to_value(FirmwareDeferReason::BatteryNotPluggedIn).unwrap(),
        serde_json::json!("BatteryNotPluggedIn")
    );
}

#[test]
fn device_id_serde_round_trip() {
    let id = DeviceId("pci:8086:9a49".into());
    let json = serde_json::to_string(&id).unwrap();
    let back: DeviceId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, back);
}

#[test]
fn hardware_graph_id_serde_round_trip() {
    let id = HardwareGraphId("hwgraph_deadbeefcafebabe0123456789abcdef".into());
    let json = serde_json::to_string(&id).unwrap();
    let back: HardwareGraphId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, back);
}

#[test]
fn firmware_blob_id_serde_round_trip() {
    let id = FirmwareBlobId("fw_iwlwifi_20250501".into());
    let json = serde_json::to_string(&id).unwrap();
    let back: FirmwareBlobId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, back);
}

#[test]
fn hardware_error_code_has_at_least_19_variants() {
    assert_eq!(HardwareErrorCode::COUNT, 19);
}

#[test]
fn hardware_error_code_for_device_not_found_matches() {
    let err = HardwareError::DeviceNotFound(DeviceId("pci:8086:9a49".into()));
    assert_eq!(err.code(), HardwareErrorCode::DeviceNotFound);
}

#[test]
fn hardware_error_code_for_firmware_unsigned_matches() {
    let err = HardwareError::FirmwareUnsigned(FirmwareBlobId("fw_test".into()));
    assert_eq!(err.code(), HardwareErrorCode::FirmwareUnsigned);
}

#[test]
fn hardware_error_code_for_gpu_vram_exhausted_matches() {
    let err = HardwareError::GpuVramExhausted {
        gpu: GpuId("gpu0".into()),
        requested: 1024,
        available: 512,
    };
    assert_eq!(err.code(), HardwareErrorCode::GpuVramExhausted);
}

#[test]
fn hardware_error_display_round_trip_all_variants_non_empty() {
    let device = DeviceId("pci:8086:9a49".into());
    let gpu = GpuId("gpu0".into());
    let graph = HardwareGraphId("hwgraph_0000".into());
    let blob = FirmwareBlobId("fw_test".into());
    let scope = FirmwareScope::Cpu;
    let policy = RemovableDevicePolicy::DenyDefault;

    let errors: Vec<HardwareError> = vec![
        HardwareError::DeviceNotFound(device.clone()),
        HardwareError::ClassificationFailed {
            device: device.clone(),
            reason: "test".into(),
        },
        HardwareError::DriverBindingFailed {
            device: device.clone(),
            reason: "test".into(),
        },
        HardwareError::DriftFromPriorBoot {
            prior_graph_id: graph.clone(),
            current_graph_id: HardwareGraphId("hwgraph_0001".into()),
            changed_devices: vec![device.clone()],
        },
        HardwareError::CapabilityLie {
            device: device.clone(),
            advertised: "gpu".into(),
            observed: "none".into(),
        },
        HardwareError::ThunderboltUnauthorized(device.clone()),
        HardwareError::IommuMissing(device.clone()),
        HardwareError::RemovableDenied { device, policy },
        HardwareError::GpuVramExhausted {
            gpu: gpu.clone(),
            requested: 1024,
            available: 512,
        },
        HardwareError::GpuBindingInvalid {
            gpu: gpu.clone(),
            reason: "test".into(),
        },
        HardwareError::DmabufPeerUnauthorized {
            src: gpu,
            target: GpuId("gpu1".into()),
        },
        HardwareError::FirmwareUnsigned(blob.clone()),
        HardwareError::FirmwareSignatureInvalid {
            blob: blob.clone(),
            reason: "test".into(),
        },
        HardwareError::FirmwareVersionRegression {
            blob: blob.clone(),
            attempted: "2.0".into(),
            installed: "1.0".into(),
        },
        HardwareError::FirmwareScopeMismatch {
            blob: blob.clone(),
            expected: scope,
            advertised: FirmwareScope::Gpu,
        },
        HardwareError::FirmwareRefusedConstitutional {
            blob: blob.clone(),
            reason: "test".into(),
        },
        HardwareError::FirmwareApplyFailed {
            blob,
            reason: "test".into(),
        },
        HardwareError::GraphSnapshotSignatureInvalid(graph),
        HardwareError::Internal("test".into()),
    ];

    for err in &errors {
        let display = format!("{err}");
        assert!(
            !display.is_empty(),
            "display should not be empty for {err:?}"
        );
    }
}
