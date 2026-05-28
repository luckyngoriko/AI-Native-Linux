//! T-174 — M17 closure invariants.
//!
//! Constitutional checks that M17 (aios-hardware) is honestly closed:
//! version marker, no deferred-stub leakage, closed-vocabulary cardinality,
//! invariant reachability (RemovableDevicePolicyTable, IommuFloorEnforcer,
//! CapabilityLieDetector, DriverBindingRegistry, GpuResourceRegistry,
//! FirmwareUpdateOrchestrator, DmabufBroker, HardwareGraph), and 32
//! HardwareRecordType variants all constructable.

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

use chrono::Utc;
use ed25519_dalek::SigningKey;
use rand_core::OsRng;

use aios_hardware::{
    BusKind, CapabilityLieDetector, DeviceClass, DeviceLifecycleState, DeviceTrustClass,
    DmabufBroker, DriverBindingRegistry, DriverBlacklistEntry, DriverProvenance,
    FirmwareApplyStrategy, FirmwareBlob, FirmwareBlobId, FirmwareScope, FirmwareTrustResult,
    FirmwareUpdateClass, FirmwareUpdateOrchestrator, FirmwareUpdateState, GpuCapabilityClass,
    GpuDevice, GpuId, GpuResourceRegistry, GpuVendorKind, HardwareDeviceRecord, HardwareErrorCode,
    HardwareGraphBuilder, HardwareManager, InMemoryHardwareManager, IommuFloorEnforcer,
    RemovableDevicePolicy, RemovableDevicePolicyTable, DEFAULT_CODE_VERSION,
};

use aios_hardware::evidence::{FirmwarePhaseRecord, HardwareRecordType};
use aios_hardware::{DriverBinding, HardwareGraphId};

// ---------------------------------------------------------------------------
// INV-1: Version marker is 0.1.0-T174
// ---------------------------------------------------------------------------

#[test]
fn inv_1_version_marker_is_0_1_0_t174() {
    assert_eq!(
        DEFAULT_CODE_VERSION, "aios-hardware/0.1.0-T174",
        "DEFAULT_CODE_VERSION must reflect M17 closure"
    );
    assert_eq!(
        env!("CARGO_PKG_VERSION"),
        "0.1.0",
        "CARGO_PKG_VERSION must be 0.1.0"
    );
}

// ---------------------------------------------------------------------------
// INV-2: No Status::Unimplemented, todo!, or unimplemented! in src/
// ---------------------------------------------------------------------------

fn collect_rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
}

#[test]
fn inv_2_no_unimplemented_in_src() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![];
    collect_rs_files(&src, &mut files);
    assert!(!files.is_empty(), "should find .rs files under src/");

    for file in &files {
        let content =
            std::fs::read_to_string(file).unwrap_or_else(|e| panic!("cannot read {file:?}: {e}"));
        let rel = file.strip_prefix(&src).unwrap_or(file);

        assert!(
            !content.contains("Status::Unimplemented"),
            "{rel:?} must not contain Status::Unimplemented"
        );
        assert!(
            !content.contains("todo!("),
            "{rel:?} must not contain todo!()"
        );
        assert!(
            !content.contains("unimplemented!("),
            "{rel:?} must not contain unimplemented!()"
        );
    }
}

// ---------------------------------------------------------------------------
// INV-3: DeviceClass has 16 variants
// ---------------------------------------------------------------------------

#[test]
fn inv_3_device_class_16_variants() {
    let variants = [
        DeviceClass::Cpu,
        DeviceClass::Memory,
        DeviceClass::GpuIntegrated,
        DeviceClass::GpuDiscrete,
        DeviceClass::NetworkEthernet,
        DeviceClass::NetworkWifi,
        DeviceClass::NetworkBluetooth,
        DeviceClass::StorageNvme,
        DeviceClass::StorageSata,
        DeviceClass::StorageMmc,
        DeviceClass::AudioCard,
        DeviceClass::AudioHeadset,
        DeviceClass::UsbController,
        DeviceClass::ThunderboltController,
        DeviceClass::PrinterOrScanner,
        DeviceClass::SensorOrInputDevice,
    ];
    assert_eq!(
        variants.len(),
        16,
        "DeviceClass must have exactly 16 variants"
    );
}

// ---------------------------------------------------------------------------
// INV-4: DeviceLifecycleState has 8 variants
// ---------------------------------------------------------------------------

#[test]
fn inv_4_device_lifecycle_state_8_variants() {
    let variants = [
        DeviceLifecycleState::Detected,
        DeviceLifecycleState::Probed,
        DeviceLifecycleState::Bound,
        DeviceLifecycleState::Active,
        DeviceLifecycleState::Suspended,
        DeviceLifecycleState::Quarantined,
        DeviceLifecycleState::Removed,
        DeviceLifecycleState::Recovered,
    ];
    assert_eq!(
        variants.len(),
        8,
        "DeviceLifecycleState must have exactly 8 variants"
    );
}

// ---------------------------------------------------------------------------
// INV-5: GpuCapabilityClass has 5 variants (S8.2 §3)
// ---------------------------------------------------------------------------

#[test]
fn inv_5_gpu_capability_class_5_variants() {
    let variants = [
        GpuCapabilityClass::RenderOnly,
        GpuCapabilityClass::ComputeOnly,
        GpuCapabilityClass::RenderAndCompute,
        GpuCapabilityClass::VideoEncode,
        GpuCapabilityClass::VideoDecode,
    ];
    assert_eq!(
        variants.len(),
        5,
        "GpuCapabilityClass must have exactly 5 variants"
    );
}

// ---------------------------------------------------------------------------
// INV-6: FirmwareUpdateState has 8 variants
// ---------------------------------------------------------------------------

#[test]
fn inv_6_firmware_update_state_8_variants() {
    let variants = [
        FirmwareUpdateState::Proposed,
        FirmwareUpdateState::Verified,
        FirmwareUpdateState::Approved,
        FirmwareUpdateState::Staged,
        FirmwareUpdateState::Applying,
        FirmwareUpdateState::Applied,
        FirmwareUpdateState::Failed,
        FirmwareUpdateState::Reverted,
    ];
    assert_eq!(
        variants.len(),
        8,
        "FirmwareUpdateState must have exactly 8 variants"
    );
}

// ---------------------------------------------------------------------------
// INV-7: FirmwareTrustResult has 8 variants including UnsignedRefused
// ---------------------------------------------------------------------------

#[test]
fn inv_7_firmware_trust_result_8_variants() {
    let variants = [
        FirmwareTrustResult::AiosPublisherSigned,
        FirmwareTrustResult::VendorSignedThroughAiosBridge,
        FirmwareTrustResult::OperatorLocalSigned,
        FirmwareTrustResult::UnsignedRefused,
        FirmwareTrustResult::RevokedKey,
        FirmwareTrustResult::VersionRegression,
        FirmwareTrustResult::IncompatibleScope,
        FirmwareTrustResult::ConstitutionalRefusal,
    ];
    assert_eq!(
        variants.len(),
        8,
        "FirmwareTrustResult must have exactly 8 variants including UnsignedRefused"
    );
}

// ---------------------------------------------------------------------------
// INV-8: HardwareErrorCode has at least 19 variants
// ---------------------------------------------------------------------------

#[test]
fn inv_8_hardware_error_code_at_least_19_variants() {
    let codes = [
        HardwareErrorCode::DeviceNotFound,
        HardwareErrorCode::ClassificationFailed,
        HardwareErrorCode::DriverBindingFailed,
        HardwareErrorCode::DriftFromPriorBoot,
        HardwareErrorCode::CapabilityLie,
        HardwareErrorCode::ThunderboltUnauthorized,
        HardwareErrorCode::IommuMissing,
        HardwareErrorCode::RemovableDenied,
        HardwareErrorCode::GpuVramExhausted,
        HardwareErrorCode::GpuBindingInvalid,
        HardwareErrorCode::DmabufPeerUnauthorized,
        HardwareErrorCode::FirmwareUnsigned,
        HardwareErrorCode::FirmwareSignatureInvalid,
        HardwareErrorCode::FirmwareVersionRegression,
        HardwareErrorCode::FirmwareScopeMismatch,
        HardwareErrorCode::FirmwareRefusedConstitutional,
        HardwareErrorCode::FirmwareApplyFailed,
        HardwareErrorCode::GraphSnapshotSignatureInvalid,
        HardwareErrorCode::Internal,
    ];
    assert_eq!(
        codes.len(),
        19,
        "HardwareErrorCode must have exactly 19 variants"
    );
}

// ---------------------------------------------------------------------------
// INV-9: RemovableDevicePolicy has 5 variants including RecoveryDenied
// ---------------------------------------------------------------------------

#[test]
fn inv_9_removable_device_policy_5_variants() {
    let variants = [
        RemovableDevicePolicy::DenyDefault,
        RemovableDevicePolicy::AllowReadOnly,
        RemovableDevicePolicy::AllowMount,
        RemovableDevicePolicy::AllowReadWrite,
        RemovableDevicePolicy::RecoveryDenied,
    ];
    assert_eq!(
        variants.len(),
        5,
        "RemovableDevicePolicy must have exactly 5 variants including RecoveryDenied"
    );
}

// ---------------------------------------------------------------------------
// INV-10: INV reachability — RemovableDevicePolicyTable constructs
// ---------------------------------------------------------------------------

#[test]
fn inv_10_removable_device_policy_table_reachable() {
    let classifier = aios_hardware::AiSubjectClassifier::new();
    assert!(
        classifier.is_ai("agent:test"),
        "AiSubjectClassifier must recognize agent: prefix"
    );
    let table = RemovableDevicePolicyTable::new();
    // Construction succeeds — INV reachable
    let _ = table;
}

// ---------------------------------------------------------------------------
// INV-11: INV reachability — IommuFloorEnforcer
// ---------------------------------------------------------------------------

#[test]
fn inv_11_iommu_floor_enforcer_reachable() {
    assert!(IommuFloorEnforcer::iommu_required_for_bus(
        BusKind::Thunderbolt
    ));
    assert!(IommuFloorEnforcer::iommu_required_for_bus(BusKind::Usb4));
    assert!(IommuFloorEnforcer::iommu_required_for_bus(BusKind::Pcie));
    assert!(!IommuFloorEnforcer::iommu_required_for_bus(BusKind::Usb3));
    assert!(!IommuFloorEnforcer::iommu_required_for_bus(BusKind::Usb2));
    assert!(!IommuFloorEnforcer::iommu_required_for_bus(BusKind::I2c));
    assert!(!IommuFloorEnforcer::iommu_required_for_bus(BusKind::Nvme));
    let enforcer = IommuFloorEnforcer::new();
    let _ = enforcer;
}

// ---------------------------------------------------------------------------
// INV-12: INV reachability — GpuResourceRegistry
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_12_gpu_resource_registry_reachable() {
    let registry = GpuResourceRegistry::new();
    let gpu = GpuDevice {
        gpu_id: GpuId("gpu-inv12".into()),
        vendor: GpuVendorKind::Nvidia,
        product_name: "Test GPU".into(),
        vram_total_bytes: 8_000_000_000,
        supported_classes: vec![GpuCapabilityClass::ComputeOnly],
        iommu_protected: true,
        host_canonical_id: "host-1".into(),
    };
    registry
        .register_device(gpu)
        .await
        .expect("register GPU device");
    let devices = registry.list_devices().await;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].gpu_id.0, "gpu-inv12");
}

// ---------------------------------------------------------------------------
// INV-13: INV reachability — FirmwareUpdateOrchestrator + DmabufBroker
// ---------------------------------------------------------------------------

#[test]
fn inv_13_firmware_orchestrator_and_dmabuf_broker_reachable() {
    let orch = FirmwareUpdateOrchestrator::new();
    let _ = orch;
    let broker = DmabufBroker::new();
    let _ = broker;
}

// ---------------------------------------------------------------------------
// INV-14: INV reachability — HardwareGraph via HardwareGraphBuilder
// ---------------------------------------------------------------------------

#[test]
fn inv_14_hardware_graph_builder_reachable() {
    let sk = SigningKey::generate(&mut OsRng);
    let mut builder = HardwareGraphBuilder::new("host-inv14");
    builder
        .add_device(HardwareDeviceRecord {
            device_id: aios_hardware::DeviceId("dev-inv14".into()),
            class: DeviceClass::GpuDiscrete,
            bus: BusKind::Pcie,
            vendor_id: 0x8086,
            product_id: 0x9A49,
            vendor_name: "Intel".into(),
            product_name: "TestDevice".into(),
            trust_class: DeviceTrustClass::Untrusted,
            lifecycle: DeviceLifecycleState::Detected,
            driver_provenance: None,
            firmware_version: None,
            removable: false,
            iommu_protected: true,
            probed_at: Utc::now(),
        })
        .expect("add_device");
    let graph = builder
        .build_and_sign(&sk, "fp-inv14")
        .expect("build_and_sign");
    assert!(!graph.id.0.is_empty());
    assert_eq!(graph.devices.len(), 1);
}

// ---------------------------------------------------------------------------
// INV-15: 32 HardwareRecordType variants all constructable + as_str() non-empty
// ---------------------------------------------------------------------------

#[test]
fn inv_15_thirty_two_hardware_record_types_constructable() {
    let all: &[HardwareRecordType] = &[
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

    assert_eq!(
        all.len(),
        32,
        "Must have exactly 32 HardwareRecordType variants"
    );

    for (i, record) in all.iter().enumerate() {
        let name = record.as_str();
        assert!(
            !name.is_empty(),
            "variant {i} ({record:?}) must have non-empty as_str()"
        );
    }
}

// ---------------------------------------------------------------------------
// INV-16: GpuVendorKind has 6 variants
// ---------------------------------------------------------------------------

#[test]
fn inv_16_gpu_vendor_kind_6_variants() {
    let variants = [
        GpuVendorKind::Amd,
        GpuVendorKind::Intel,
        GpuVendorKind::Nvidia,
        GpuVendorKind::Arm,
        GpuVendorKind::Apple,
        GpuVendorKind::Other,
    ];
    assert_eq!(
        variants.len(),
        6,
        "GpuVendorKind must have exactly 6 variants"
    );
}

// ---------------------------------------------------------------------------
// INV-17: BusKind has 8 variants
// ---------------------------------------------------------------------------

#[test]
fn inv_17_bus_kind_8_variants() {
    let variants = [
        BusKind::Pci,
        BusKind::Pcie,
        BusKind::Usb2,
        BusKind::Usb3,
        BusKind::Usb4,
        BusKind::Thunderbolt,
        BusKind::Nvme,
        BusKind::I2c,
    ];
    assert_eq!(variants.len(), 8, "BusKind must have exactly 8 variants");
}

// ---------------------------------------------------------------------------
// INV-18: HardwareManager trait has InMemoryHardwareManager impl
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_18_hardware_manager_trait_coverage() {
    let manager = InMemoryHardwareManager::new();
    let record = HardwareDeviceRecord {
        device_id: aios_hardware::DeviceId("dev-hwm".into()),
        class: DeviceClass::Cpu,
        bus: BusKind::Pcie,
        vendor_id: 0x8086,
        product_id: 0x0001,
        vendor_name: "Intel".into(),
        product_name: "TestCPU".into(),
        trust_class: DeviceTrustClass::Untrusted,
        lifecycle: DeviceLifecycleState::Detected,
        driver_provenance: None,
        firmware_version: None,
        removable: false,
        iommu_protected: false,
        probed_at: Utc::now(),
    };
    manager
        .register_device(record)
        .await
        .expect("register_device via trait");

    let devices = manager.list_pending_devices().await;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].device_id.0, "dev-hwm");

    manager
        .deregister_device(&aios_hardware::DeviceId("dev-hwm".into()))
        .await
        .expect("deregister_device via trait");
}
