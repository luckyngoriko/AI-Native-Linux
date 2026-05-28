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

use aios_hardware::*;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::OsRng;

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
        trust_class: DeviceTrustClass::VendorSigned,
        lifecycle: DeviceLifecycleState::Detected,
        driver_provenance: None,
        firmware_version: None,
        removable: false,
        iommu_protected: true,
        probed_at: chrono::Utc::now(),
    }
}

/// Same as `test_device` but with a fixed `probed_at` timestamp so BLAKE3
/// content-addressing is deterministic across rebuilds.
fn test_device_fixed(id: &str, class: DeviceClass, bus: BusKind) -> HardwareDeviceRecord {
    let mut dev = test_device(id, class, bus);
    dev.probed_at =
        chrono::DateTime::from_timestamp(1_700_000_000, 0).expect("valid fixed unix timestamp");
    dev
}

fn test_keypair() -> (SigningKey, VerifyingKey) {
    let signing = SigningKey::generate(&mut OsRng);
    let verifying = signing.verifying_key();
    (signing, verifying)
}

// -- basic manager tests ---------------------------------------------------

#[tokio::test]
async fn new_manager_has_no_current_graph() {
    let m = InMemoryHardwareManager::new();
    assert!(m.current_graph().await.is_none());
}

#[tokio::test]
async fn register_device_then_list_pending_returns_1() {
    let m = InMemoryHardwareManager::new();
    let dev = test_device("pci:8086:9a49", DeviceClass::GpuIntegrated, BusKind::Pcie);
    m.register_device(dev).await.unwrap();
    let pending = m.list_pending_devices().await;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].device_id, DeviceId("pci:8086:9a49".into()));
}

#[tokio::test]
async fn register_duplicate_device_id_returns_internal_error() {
    let m = InMemoryHardwareManager::new();
    let dev = test_device("pci:8086:9a49", DeviceClass::GpuIntegrated, BusKind::Pcie);
    m.register_device(dev.clone()).await.unwrap();
    let result = m.register_device(dev).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), HardwareErrorCode::Internal);
}

#[tokio::test]
async fn deregister_known_device_removes_from_pending() {
    let m = InMemoryHardwareManager::new();
    let dev = test_device("pci:8086:9a49", DeviceClass::GpuIntegrated, BusKind::Pcie);
    m.register_device(dev).await.unwrap();
    assert_eq!(m.list_pending_devices().await.len(), 1);
    m.deregister_device(&DeviceId("pci:8086:9a49".into()))
        .await
        .unwrap();
    assert_eq!(m.list_pending_devices().await.len(), 0);
}

#[tokio::test]
async fn deregister_unknown_device_returns_device_not_found() {
    let m = InMemoryHardwareManager::new();
    let result = m.deregister_device(&DeviceId("pci:0000:0000".into())).await;
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code(),
        HardwareErrorCode::DeviceNotFound
    );
}

#[tokio::test]
async fn get_device_known_returns_record() {
    let m = InMemoryHardwareManager::new();
    let dev = test_device("pci:8086:9a49", DeviceClass::GpuIntegrated, BusKind::Pcie);
    m.register_device(dev).await.unwrap();
    let fetched = m
        .get_device(&DeviceId("pci:8086:9a49".into()))
        .await
        .unwrap();
    assert_eq!(fetched.device_id, DeviceId("pci:8086:9a49".into()));
    assert_eq!(fetched.class, DeviceClass::GpuIntegrated);
}

#[tokio::test]
async fn get_device_unknown_returns_device_not_found() {
    let m = InMemoryHardwareManager::new();
    let result = m.get_device(&DeviceId("pci:0000:0000".into())).await;
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code(),
        HardwareErrorCode::DeviceNotFound
    );
}

#[tokio::test]
async fn set_device_lifecycle_known_succeeds() {
    let m = InMemoryHardwareManager::new();
    let dev = test_device("pci:8086:9a49", DeviceClass::GpuIntegrated, BusKind::Pcie);
    m.register_device(dev).await.unwrap();
    m.set_device_lifecycle(
        &DeviceId("pci:8086:9a49".into()),
        DeviceLifecycleState::Active,
    )
    .await
    .unwrap();
    let updated = m
        .get_device(&DeviceId("pci:8086:9a49".into()))
        .await
        .unwrap();
    assert_eq!(updated.lifecycle, DeviceLifecycleState::Active);
}

#[tokio::test]
async fn set_device_lifecycle_unknown_returns_device_not_found() {
    let m = InMemoryHardwareManager::new();
    let result = m
        .set_device_lifecycle(
            &DeviceId("pci:0000:0000".into()),
            DeviceLifecycleState::Active,
        )
        .await;
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code(),
        HardwareErrorCode::DeviceNotFound
    );
}

// -- graph tests -----------------------------------------------------------

#[tokio::test]
async fn rebuild_graph_with_no_devices_yields_empty_graph() {
    let m = InMemoryHardwareManager::new();
    let (sk, _vk) = test_keypair();
    let graph = m.rebuild_graph("host-01", &sk, "fp:test").await.unwrap();
    assert_eq!(graph.devices.len(), 0);
}

#[tokio::test]
async fn rebuild_graph_with_3_devices_yields_3_device_graph() {
    let m = InMemoryHardwareManager::new();
    for (id, cls, bus) in [
        ("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci),
        ("pci:8086:0002", DeviceClass::Memory, BusKind::Pcie),
        ("pci:8086:0003", DeviceClass::StorageNvme, BusKind::Nvme),
    ] {
        m.register_device(test_device(id, cls, bus)).await.unwrap();
    }
    let (sk, _vk) = test_keypair();
    let graph = m.rebuild_graph("host-01", &sk, "fp:test").await.unwrap();
    assert_eq!(graph.devices.len(), 3);
}

#[tokio::test]
async fn rebuild_graph_id_format_starts_with_hwgraph_underscore() {
    let m = InMemoryHardwareManager::new();
    m.register_device(test_device(
        "pci:8086:9a49",
        DeviceClass::GpuIntegrated,
        BusKind::Pcie,
    ))
    .await
    .unwrap();
    let (sk, _vk) = test_keypair();
    let graph = m.rebuild_graph("host-01", &sk, "fp:test").await.unwrap();
    assert!(graph.id.0.starts_with("hwgraph_"));
}

#[tokio::test]
async fn rebuild_graph_id_is_32_hex_chars_after_prefix() {
    let m = InMemoryHardwareManager::new();
    m.register_device(test_device(
        "pci:8086:9a49",
        DeviceClass::GpuIntegrated,
        BusKind::Pcie,
    ))
    .await
    .unwrap();
    let (sk, _vk) = test_keypair();
    let graph = m.rebuild_graph("host-01", &sk, "fp:test").await.unwrap();
    let hex_part = &graph.id.0["hwgraph_".len()..];
    assert_eq!(hex_part.len(), 32);
    assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
}

#[tokio::test]
async fn rebuild_graph_is_deterministic_for_same_input() {
    let (sk, _vk) = test_keypair();

    let build = || async {
        let m = InMemoryHardwareManager::new();
        for (id, cls, bus) in [
            ("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci),
            ("pci:8086:0002", DeviceClass::Memory, BusKind::Pcie),
        ] {
            m.register_device(test_device_fixed(id, cls, bus))
                .await
                .unwrap();
        }
        m.rebuild_graph("host-01", &sk, "fp:test").await.unwrap()
    };

    let g1 = build().await;
    let g2 = build().await;
    assert_eq!(g1.id, g2.id, "determinism: same input → same id");
    assert_eq!(
        g1.signature, g2.signature,
        "determinism: same input → same signature"
    );
}

#[tokio::test]
async fn rebuild_graph_changes_id_when_a_device_is_added() {
    let (sk, _vk) = test_keypair();

    let m1 = InMemoryHardwareManager::new();
    m1.register_device(test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci))
        .await
        .unwrap();
    let g1 = m1.rebuild_graph("host-01", &sk, "fp:test").await.unwrap();

    let m2 = InMemoryHardwareManager::new();
    m2.register_device(test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci))
        .await
        .unwrap();
    m2.register_device(test_device(
        "pci:8086:0002",
        DeviceClass::Memory,
        BusKind::Pcie,
    ))
    .await
    .unwrap();
    let g2 = m2.rebuild_graph("host-01", &sk, "fp:test").await.unwrap();

    assert_ne!(
        g1.id, g2.id,
        "different devices → different id (drift detection precondition)"
    );
}

// -- signature verification ------------------------------------------------

#[tokio::test]
async fn hardware_graph_verify_with_correct_authority_succeeds() {
    let m = InMemoryHardwareManager::new();
    m.register_device(test_device(
        "pci:8086:9a49",
        DeviceClass::GpuIntegrated,
        BusKind::Pcie,
    ))
    .await
    .unwrap();
    let (sk, vk) = test_keypair();
    let graph = m.rebuild_graph("host-01", &sk, "fp:test").await.unwrap();
    graph.verify(&vk).unwrap();
}

#[tokio::test]
async fn hardware_graph_verify_with_wrong_authority_returns_graph_snapshot_signature_invalid() {
    let m = InMemoryHardwareManager::new();
    m.register_device(test_device(
        "pci:8086:9a49",
        DeviceClass::GpuIntegrated,
        BusKind::Pcie,
    ))
    .await
    .unwrap();
    let (sk, _vk) = test_keypair();
    let graph = m.rebuild_graph("host-01", &sk, "fp:test").await.unwrap();

    let (_other_sk, other_vk) = test_keypair();
    let result = graph.verify(&other_vk);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code(),
        HardwareErrorCode::GraphSnapshotSignatureInvalid
    );
}

#[tokio::test]
async fn hardware_graph_verify_with_tampered_devices_returns_graph_snapshot_signature_invalid() {
    let m = InMemoryHardwareManager::new();
    m.register_device(test_device(
        "pci:8086:9a49",
        DeviceClass::GpuIntegrated,
        BusKind::Pcie,
    ))
    .await
    .unwrap();
    let (sk, vk) = test_keypair();
    let mut graph = m.rebuild_graph("host-01", &sk, "fp:test").await.unwrap();

    // Tamper: replace the device map with a different device
    let tampered_dev = test_device("pci:0000:0000", DeviceClass::UsbController, BusKind::Usb3);
    graph.devices.clear();
    graph
        .devices
        .insert(tampered_dev.device_id.clone(), tampered_dev);

    let result = graph.verify(&vk);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code(),
        HardwareErrorCode::GraphSnapshotSignatureInvalid
    );
}

#[tokio::test]
async fn current_graph_after_rebuild_returns_some() {
    let m = InMemoryHardwareManager::new();
    assert!(m.current_graph().await.is_none());

    m.register_device(test_device(
        "pci:8086:9a49",
        DeviceClass::GpuIntegrated,
        BusKind::Pcie,
    ))
    .await
    .unwrap();
    let (sk, _vk) = test_keypair();
    m.rebuild_graph("host-01", &sk, "fp:test").await.unwrap();

    assert!(m.current_graph().await.is_some());
}

// -- concurrency -----------------------------------------------------------

#[tokio::test]
async fn concurrent_register_5_devices_no_panic() {
    let m = Arc::new(InMemoryHardwareManager::new());

    let handles: Vec<_> = (0..5)
        .map(|i| {
            let m = Arc::clone(&m);
            tokio::spawn(async move {
                let id = format!("pci:8086:00{i:02}");
                let dev = HardwareDeviceRecord {
                    device_id: DeviceId(id),
                    class: DeviceClass::GpuIntegrated,
                    bus: BusKind::Pcie,
                    vendor_id: 0x8086,
                    product_id: 0x9A49,
                    vendor_name: "Intel".into(),
                    product_name: "TestDevice".into(),
                    trust_class: DeviceTrustClass::VendorSigned,
                    lifecycle: DeviceLifecycleState::Detected,
                    driver_provenance: None,
                    firmware_version: None,
                    removable: false,
                    iommu_protected: true,
                    probed_at: chrono::Utc::now(),
                };
                let _ = m.register_device(dev).await;
            })
        })
        .collect();

    for h in handles {
        h.await.unwrap();
    }

    // All 5 should be registered (distinct ids → no duplicates)
    assert_eq!(m.list_pending_devices().await.len(), 5);
}
