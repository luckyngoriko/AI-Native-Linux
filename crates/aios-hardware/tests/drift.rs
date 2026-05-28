#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::unwrap_in_result,
    clippy::doc_markdown,
    clippy::missing_const_for_fn,
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
use chrono::{DateTime, Utc};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::OsRng;

// -- helpers ----------------------------------------------------------------

fn fixed_ts() -> DateTime<Utc> {
    chrono::DateTime::from_timestamp(1_700_000_000, 0).expect("valid fixed unix timestamp")
}

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
        probed_at: fixed_ts(),
    }
}

fn test_keypair() -> (SigningKey, VerifyingKey) {
    let signing = SigningKey::generate(&mut OsRng);
    let verifying = signing.verifying_key();
    (signing, verifying)
}

fn build_graph(
    devices: Vec<HardwareDeviceRecord>,
    host_id: &str,
    sk: &SigningKey,
) -> HardwareGraph {
    let mut builder = HardwareGraphBuilder::new(host_id);
    for dev in devices {
        builder.add_device(dev).unwrap();
    }
    builder.build_and_sign(sk, "fp:test").unwrap()
}

// -- PriorGraphStore tests --------------------------------------------------

#[tokio::test]
async fn prior_store_new_returns_none_for_current() {
    let store = PriorGraphStore::new();
    assert!(store.current().await.is_none());
}

#[tokio::test]
async fn prior_store_store_then_current_returns_id() {
    let (sk, _vk) = test_keypair();
    let graph = build_graph(
        vec![test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci)],
        "host-01",
        &sk,
    );
    let expected_id = graph.id.clone();

    let store = PriorGraphStore::new();
    store.store(&graph).await;
    assert_eq!(store.current().await, Some(expected_id));
}

// -- DriftDetector tests ----------------------------------------------------

#[tokio::test]
async fn drift_detector_first_boot_returns_first_boot_with_current_id() {
    let (sk, _vk) = test_keypair();
    let graph = build_graph(
        vec![test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci)],
        "host-01",
        &sk,
    );
    let expected_id = graph.id.clone();

    let store = Arc::new(PriorGraphStore::new());
    let detector = DriftDetector::new(Arc::clone(&store));
    let signal = detector.check(&graph).await.unwrap();

    match signal {
        DriftSignal::FirstBoot { current } => assert_eq!(current, expected_id),
        other => panic!("expected FirstBoot, got {other:?}"),
    }
}

#[tokio::test]
async fn drift_detector_same_graph_returns_no_drift() {
    let (sk, _vk) = test_keypair();
    let graph = build_graph(
        vec![test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci)],
        "host-01",
        &sk,
    );

    let store = Arc::new(PriorGraphStore::with_prior(graph.clone()));
    let detector = DriftDetector::new(Arc::clone(&store));
    let signal = detector.check(&graph).await.unwrap();

    assert_eq!(signal, DriftSignal::NoDrift);
}

#[tokio::test]
async fn drift_detector_added_device_returns_drift_detected_with_added_one() {
    let (sk, _vk) = test_keypair();
    let prior = build_graph(
        vec![test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci)],
        "host-01",
        &sk,
    );
    let current = build_graph(
        vec![
            test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci),
            test_device("pci:8086:0002", DeviceClass::Memory, BusKind::Pcie),
        ],
        "host-01",
        &sk,
    );

    let store = Arc::new(PriorGraphStore::with_prior(prior));
    let detector = DriftDetector::new(Arc::clone(&store));
    let signal = detector.check(&current).await.unwrap();

    match signal {
        DriftSignal::DriftDetected { change, .. } => {
            assert_eq!(change.added.len(), 1);
            assert_eq!(change.added[0], DeviceId("pci:8086:0002".into()));
            assert!(change.removed.is_empty());
            assert!(change.modified.is_empty());
            assert_eq!(change.kept, 1);
        }
        other => panic!("expected DriftDetected, got {other:?}"),
    }
}

#[tokio::test]
async fn drift_detector_removed_device_returns_drift_detected_with_removed_one() {
    let (sk, _vk) = test_keypair();
    let prior = build_graph(
        vec![
            test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci),
            test_device("pci:8086:0002", DeviceClass::Memory, BusKind::Pcie),
        ],
        "host-01",
        &sk,
    );
    let current = build_graph(
        vec![test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci)],
        "host-01",
        &sk,
    );

    let store = Arc::new(PriorGraphStore::with_prior(prior));
    let detector = DriftDetector::new(Arc::clone(&store));
    let signal = detector.check(&current).await.unwrap();

    match signal {
        DriftSignal::DriftDetected { change, .. } => {
            assert!(change.added.is_empty());
            assert_eq!(change.removed.len(), 1);
            assert_eq!(change.removed[0], DeviceId("pci:8086:0002".into()));
            assert!(change.modified.is_empty());
            assert_eq!(change.kept, 1);
        }
        other => panic!("expected DriftDetected, got {other:?}"),
    }
}

#[tokio::test]
async fn drift_detector_modified_device_returns_drift_detected_with_modified_one() {
    let (sk, _vk) = test_keypair();
    let prior = build_graph(
        vec![test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci)],
        "host-01",
        &sk,
    );

    // Same device id but different class → modified
    let current = build_graph(
        vec![test_device(
            "pci:8086:0001",
            DeviceClass::GpuIntegrated,
            BusKind::Pci,
        )],
        "host-01",
        &sk,
    );

    let store = Arc::new(PriorGraphStore::with_prior(prior));
    let detector = DriftDetector::new(Arc::clone(&store));
    let signal = detector.check(&current).await.unwrap();

    match signal {
        DriftSignal::DriftDetected { change, .. } => {
            assert!(change.added.is_empty());
            assert!(change.removed.is_empty());
            assert_eq!(change.modified.len(), 1);
            assert_eq!(change.modified[0], DeviceId("pci:8086:0001".into()));
            assert_eq!(change.kept, 0);
        }
        other => panic!("expected DriftDetected, got {other:?}"),
    }
}

#[tokio::test]
async fn drift_detector_mixed_changes_returns_drift_detected_with_all_three() {
    let (sk, _vk) = test_keypair();
    let prior = build_graph(
        vec![
            test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci),
            test_device("pci:8086:0002", DeviceClass::StorageNvme, BusKind::Nvme),
            test_device("pci:8086:0003", DeviceClass::NetworkEthernet, BusKind::Pcie),
        ],
        "host-01",
        &sk,
    );

    // Remove pci:0003, modify pci:0001 (different class), add pci:0004, keep pci:0002
    let current = build_graph(
        vec![
            test_device("pci:8086:0001", DeviceClass::GpuIntegrated, BusKind::Pci), // modified
            test_device("pci:8086:0002", DeviceClass::StorageNvme, BusKind::Nvme),  // kept
            test_device("pci:8086:0004", DeviceClass::UsbController, BusKind::Usb3), // added
        ],
        "host-01",
        &sk,
    );

    let store = Arc::new(PriorGraphStore::with_prior(prior));
    let detector = DriftDetector::new(Arc::clone(&store));
    let signal = detector.check(&current).await.unwrap();

    match signal {
        DriftSignal::DriftDetected { change, .. } => {
            assert_eq!(change.added.len(), 1);
            assert_eq!(change.removed.len(), 1);
            assert_eq!(change.modified.len(), 1);
            assert_eq!(change.kept, 1);
        }
        other => panic!("expected DriftDetected, got {other:?}"),
    }
}

#[tokio::test]
async fn compute_graph_diff_kept_count_correct() {
    let (sk, _vk) = test_keypair();
    let graph = build_graph(
        vec![
            test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci),
            test_device("pci:8086:0002", DeviceClass::Memory, BusKind::Pcie),
            test_device("pci:8086:0003", DeviceClass::StorageNvme, BusKind::Nvme),
        ],
        "host-01",
        &sk,
    );

    // Same graph → NoDrift means all 3 devices matched (kept=3 in the diff,
    // though NoDrift short-circuits before diffing)
    let store = Arc::new(PriorGraphStore::with_prior(graph.clone()));
    let detector = DriftDetector::new(Arc::clone(&store));
    let signal = detector.check(&graph).await.unwrap();

    assert_eq!(signal, DriftSignal::NoDrift);

    // Force a diff with a different host_id (same devices → all kept)
    let current2 = build_graph(
        vec![
            test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci),
            test_device("pci:8086:0002", DeviceClass::Memory, BusKind::Pcie),
            test_device("pci:8086:0003", DeviceClass::StorageNvme, BusKind::Nvme),
        ],
        "host-02", // different host → different graph id
        &sk,
    );

    let signal2 = detector.check(&current2).await.unwrap();
    match signal2 {
        DriftSignal::DriftDetected { change, .. } => {
            assert_eq!(change.kept, 3);
            assert!(change.added.is_empty());
            assert!(change.removed.is_empty());
            assert!(change.modified.is_empty());
        }
        other => panic!("expected DriftDetected, got {other:?}"),
    }
}

// -- EvilMaidEvidenceMarker tests -------------------------------------------

#[tokio::test]
async fn evil_maid_marker_from_no_drift_returns_none() {
    let marker = EvilMaidEvidenceMarker::from_drift(&DriftSignal::NoDrift, fixed_ts(), None);
    assert!(marker.is_none());
}

#[tokio::test]
async fn evil_maid_marker_from_first_boot_returns_none() {
    let signal = DriftSignal::FirstBoot {
        current: HardwareGraphId("hwgraph_00000000000000000000000000000000".into()),
    };
    let marker = EvilMaidEvidenceMarker::from_drift(&signal, fixed_ts(), None);
    assert!(marker.is_none());
}

#[tokio::test]
async fn evil_maid_marker_from_drift_with_removed_non_removable_recommends_enter_recovery_mode() {
    let (sk, _vk) = test_keypair();
    let prior = build_graph(
        vec![
            test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci),
            test_device("pci:8086:0002", DeviceClass::Memory, BusKind::Pcie),
        ],
        "host-01",
        &sk,
    );
    let current = build_graph(
        vec![test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci)],
        "host-01",
        &sk,
    );

    let store = Arc::new(PriorGraphStore::with_prior(prior.clone()));
    let detector = DriftDetector::new(Arc::clone(&store));
    let signal = detector.check(&current).await.unwrap();

    // Keep a clone of prior devices for the non-removability check
    let prior_devs = Some(prior.devices.clone());

    let marker = EvilMaidEvidenceMarker::from_drift(&signal, fixed_ts(), prior_devs.as_ref());
    let marker = marker.expect("should produce marker for DriftDetected");

    assert_eq!(
        marker.recommended_action,
        EvilMaidRecommendedAction::EnterRecoveryMode
    );
}

#[tokio::test]
async fn evil_maid_marker_from_drift_with_only_added_recommends_auto_quarantine_new_devices() {
    let (sk, _vk) = test_keypair();
    let prior = build_graph(
        vec![test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci)],
        "host-01",
        &sk,
    );
    let current = build_graph(
        vec![
            test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci),
            test_device("pci:8086:0002", DeviceClass::Memory, BusKind::Pcie),
        ],
        "host-01",
        &sk,
    );

    let store = Arc::new(PriorGraphStore::with_prior(prior));
    let detector = DriftDetector::new(Arc::clone(&store));
    let signal = detector.check(&current).await.unwrap();

    let marker = EvilMaidEvidenceMarker::from_drift(&signal, fixed_ts(), None);
    let marker = marker.expect("should produce marker for DriftDetected");

    assert_eq!(
        marker.recommended_action,
        EvilMaidRecommendedAction::AutoQuarantineNewDevices
    );
}

#[tokio::test]
async fn evil_maid_marker_from_drift_with_only_modified_recommends_operator_investigation() {
    let (sk, _vk) = test_keypair();
    let prior = build_graph(
        vec![test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci)],
        "host-01",
        &sk,
    );
    let current = build_graph(
        vec![test_device(
            "pci:8086:0001",
            DeviceClass::GpuIntegrated,
            BusKind::Pci,
        )],
        "host-01",
        &sk,
    );

    let store = Arc::new(PriorGraphStore::with_prior(prior));
    let detector = DriftDetector::new(Arc::clone(&store));
    let signal = detector.check(&current).await.unwrap();

    let marker = EvilMaidEvidenceMarker::from_drift(&signal, fixed_ts(), None);
    let marker = marker.expect("should produce marker for DriftDetected");

    assert_eq!(
        marker.recommended_action,
        EvilMaidRecommendedAction::OperatorInvestigation
    );
}

#[test]
fn evil_maid_marker_serde_round_trip() {
    let marker = EvilMaidEvidenceMarker {
        prior: HardwareGraphId("hwgraph_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
        current: HardwareGraphId("hwgraph_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into()),
        diff: GraphDiff {
            added: vec![DeviceId("pci:8086:0002".into())],
            removed: vec![],
            modified: vec![],
            kept: 3,
        },
        detected_at: fixed_ts(),
        recommended_action: EvilMaidRecommendedAction::AutoQuarantineNewDevices,
    };

    let json = serde_json::to_string(&marker).expect("serialization should succeed");
    let deserialized: EvilMaidEvidenceMarker =
        serde_json::from_str(&json).expect("deserialization should succeed");

    assert_eq!(marker.prior, deserialized.prior);
    assert_eq!(marker.current, deserialized.current);
    assert_eq!(marker.diff, deserialized.diff);
    assert_eq!(marker.recommended_action, deserialized.recommended_action);
}

#[tokio::test]
async fn concurrent_store_3_graphs_no_panic() {
    let (sk, _vk) = test_keypair();

    let g1 = build_graph(
        vec![test_device("pci:8086:0001", DeviceClass::Cpu, BusKind::Pci)],
        "host-01",
        &sk,
    );
    let g2 = build_graph(
        vec![test_device(
            "pci:8086:0002",
            DeviceClass::Memory,
            BusKind::Pcie,
        )],
        "host-01",
        &sk,
    );
    let g3 = build_graph(
        vec![test_device(
            "pci:8086:0003",
            DeviceClass::StorageNvme,
            BusKind::Nvme,
        )],
        "host-01",
        &sk,
    );

    // Clone ids before moving graphs into spawned tasks
    let id1 = g1.id.clone();
    let id2 = g2.id.clone();
    let id3 = g3.id.clone();

    let store = Arc::new(PriorGraphStore::new());
    let s1 = Arc::clone(&store);
    let s2 = Arc::clone(&store);
    let s3 = Arc::clone(&store);

    let h1 = tokio::spawn(async move { s1.store(&g1).await });
    let h2 = tokio::spawn(async move { s2.store(&g2).await });
    let h3 = tokio::spawn(async move { s3.store(&g3).await });

    h1.await.unwrap();
    h2.await.unwrap();
    h3.await.unwrap();

    // Final state is one of the 3 — just verify the store is populated
    let final_id = store.current().await;
    assert!(final_id.is_some());
    let id_str = final_id.unwrap().0;
    assert!(
        id_str == id1.0 || id_str == id2.0 || id_str == id3.0,
        "final id {id_str} should be one of the 3 stored graphs"
    );
}
