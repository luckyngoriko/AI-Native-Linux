#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::unwrap_in_result,
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

// ---------------------------------------------------------------------------
// Test helpers — dmabuf
// ---------------------------------------------------------------------------

fn demo_handle(id: &str, gpu: &str) -> DmabufHandle {
    DmabufHandle {
        handle_id: id.into(),
        source_gpu: GpuId(gpu.into()),
        source_group: "group-alpha".into(),
        source_subject: format!("subject-{id}"),
        size_bytes: 4096,
        format_code: 0x3432_5241, // AR24 little-endian
        created_at: chrono::Utc::now(),
    }
}

fn demo_peer(target_gpu: &str, target_group: &str, target_subject: &str) -> DmabufPeer {
    DmabufPeer {
        target_gpu: GpuId(target_gpu.into()),
        target_group: target_group.into(),
        target_subject: target_subject.into(),
    }
}

fn demo_peer_set(handle_id: &str, peers: Vec<DmabufPeer>) -> DmabufPeerSet {
    DmabufPeerSet {
        handle_id: handle_id.into(),
        authorized_peers: peers,
        policy_decision_id: format!("pol-{handle_id}"),
    }
}

// ---------------------------------------------------------------------------
// dmabuf — create_handle_then_list_returns_1
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_handle_then_list_returns_1() {
    let broker = DmabufBroker::new();
    broker
        .create_handle(demo_handle("h1", "gpu0"))
        .await
        .expect("create");
    let handles = broker.list_handles().await;
    assert_eq!(handles.len(), 1);
    assert_eq!(handles[0].handle_id, "h1");
}

// ---------------------------------------------------------------------------
// dmabuf — create_duplicate_handle_returns_internal_error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_duplicate_handle_returns_internal_error() {
    let broker = DmabufBroker::new();
    broker
        .create_handle(demo_handle("h1", "gpu0"))
        .await
        .expect("first");
    let err = broker
        .create_handle(demo_handle("h1", "gpu1"))
        .await
        .expect_err("duplicate");
    assert_eq!(err.code(), HardwareErrorCode::Internal);
}

// ---------------------------------------------------------------------------
// dmabuf — authorize_peer_set_for_known_handle_succeeds
// ---------------------------------------------------------------------------

#[tokio::test]
async fn authorize_peer_set_for_known_handle_succeeds() {
    let broker = DmabufBroker::new();
    broker
        .create_handle(demo_handle("h1", "gpu0"))
        .await
        .expect("create");
    let ps = demo_peer_set("h1", vec![demo_peer("gpu1", "group-beta", "alice")]);
    broker.authorize_peer_set(ps).await.expect("authorize");
    let sets = broker.list_peer_sets().await;
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].handle_id, "h1");
}

// ---------------------------------------------------------------------------
// dmabuf — authorize_peer_set_for_unknown_handle_returns_internal_error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn authorize_peer_set_for_unknown_handle_returns_internal_error() {
    let broker = DmabufBroker::new();
    let ps = demo_peer_set("h-missing", vec![demo_peer("gpu1", "g", "s")]);
    let err = broker.authorize_peer_set(ps).await.expect_err("missing");
    assert_eq!(err.code(), HardwareErrorCode::Internal);
}

// ---------------------------------------------------------------------------
// dmabuf — check_import_with_matching_peer_returns_ok
// ---------------------------------------------------------------------------

#[tokio::test]
async fn check_import_with_matching_peer_returns_ok() {
    let broker = DmabufBroker::new();
    broker
        .create_handle(demo_handle("h1", "gpu0"))
        .await
        .expect("create");
    let ps = demo_peer_set("h1", vec![demo_peer("gpu1", "group-beta", "alice")]);
    broker.authorize_peer_set(ps).await.expect("authorize");

    let result = broker
        .check_import("h1", &GpuId("gpu1".into()), "group-beta", "alice")
        .await;
    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// dmabuf — check_import_with_no_peer_set_returns_dmabuf_peer_unauthorized
// ---------------------------------------------------------------------------

#[tokio::test]
async fn check_import_with_no_peer_set_returns_dmabuf_peer_unauthorized() {
    let broker = DmabufBroker::new();
    broker
        .create_handle(demo_handle("h1", "gpu0"))
        .await
        .expect("create");

    let err = broker
        .check_import("h1", &GpuId("gpu1".into()), "group-beta", "alice")
        .await
        .expect_err("no peer set");
    assert_eq!(err.code(), HardwareErrorCode::DmabufPeerUnauthorized);
}

// ---------------------------------------------------------------------------
// dmabuf — check_import_with_non_matching_peer_returns_dmabuf_peer_unauthorized
// ---------------------------------------------------------------------------

#[tokio::test]
async fn check_import_with_non_matching_peer_returns_dmabuf_peer_unauthorized() {
    let broker = DmabufBroker::new();
    broker
        .create_handle(demo_handle("h1", "gpu0"))
        .await
        .expect("create");
    let ps = demo_peer_set("h1", vec![demo_peer("gpu1", "group-beta", "alice")]);
    broker.authorize_peer_set(ps).await.expect("authorize");

    // bob is not in the authorized peer set
    let err = broker
        .check_import("h1", &GpuId("gpu1".into()), "group-beta", "bob")
        .await
        .expect_err("unauthorized peer");
    assert_eq!(err.code(), HardwareErrorCode::DmabufPeerUnauthorized);
}

// ---------------------------------------------------------------------------
// dmabuf — revoke_handle_removes_handle_and_peer_set
// ---------------------------------------------------------------------------

#[tokio::test]
async fn revoke_handle_removes_handle_and_peer_set() {
    let broker = DmabufBroker::new();
    broker
        .create_handle(demo_handle("h1", "gpu0"))
        .await
        .expect("create");
    let ps = demo_peer_set("h1", vec![demo_peer("gpu1", "g", "s")]);
    broker.authorize_peer_set(ps).await.expect("authorize");

    broker.revoke_handle("h1").await.expect("revoke");

    // Both handle and peer set are gone
    assert!(broker.list_handles().await.is_empty());
    assert!(broker.list_peer_sets().await.is_empty());
}

// ---------------------------------------------------------------------------
// dmabuf — revoke_unknown_handle_returns_internal_error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn revoke_unknown_handle_returns_internal_error() {
    let broker = DmabufBroker::new();
    let err = broker
        .revoke_handle("nonexistent")
        .await
        .expect_err("unknown");
    assert_eq!(err.code(), HardwareErrorCode::Internal);
}

// ---------------------------------------------------------------------------
// dmabuf — concurrent_create_5_distinct_handles_no_panic
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_create_5_distinct_handles_no_panic() {
    let broker = Arc::new(DmabufBroker::new());
    let mut handles = Vec::new();
    for i in 0..5 {
        let broker = Arc::clone(&broker);
        handles.push(tokio::spawn(async move {
            let h = DmabufHandle {
                handle_id: format!("h{i}"),
                source_gpu: GpuId(format!("gpu{i}")),
                source_group: "group-alpha".into(),
                source_subject: format!("subject-{i}"),
                size_bytes: 4096,
                format_code: 0,
                created_at: chrono::Utc::now(),
            };
            broker.create_handle(h).await
        }));
    }

    for h in handles {
        let result = h.await.expect("join");
        assert!(result.is_ok(), "create must succeed: {result:?}");
    }

    assert_eq!(broker.list_handles().await.len(), 5);
}

// ---------------------------------------------------------------------------
// capability_lie — helper
// ---------------------------------------------------------------------------

fn device(id: &str) -> DeviceId {
    DeviceId(id.into())
}

// ---------------------------------------------------------------------------
// capability_lie — advertise_then_observe_matching_returns_match
// ---------------------------------------------------------------------------

#[tokio::test]
async fn advertise_then_observe_matching_returns_match() {
    let detector = CapabilityLieDetector::new();
    detector
        .advertise(AdvertisedCapability {
            device_id: device("pci:00"),
            key: "iommu".into(),
            advertised_value: "on".into(),
        })
        .await
        .expect("advertise");

    let outcome = detector
        .observe(ObservedCapability {
            device_id: device("pci:00"),
            key: "iommu".into(),
            observed_value: "on".into(),
            observed_at: chrono::Utc::now(),
        })
        .await
        .expect("observe");

    assert!(matches!(outcome, CapabilityLieOutcome::Match));
}

// ---------------------------------------------------------------------------
// capability_lie — observe_without_advertise_returns_match
// ---------------------------------------------------------------------------

#[tokio::test]
async fn observe_without_advertise_returns_match() {
    let detector = CapabilityLieDetector::new();
    let outcome = detector
        .observe(ObservedCapability {
            device_id: device("pci:00"),
            key: "iommu".into(),
            observed_value: "on".into(),
            observed_at: chrono::Utc::now(),
        })
        .await
        .expect("observe");

    assert!(matches!(outcome, CapabilityLieOutcome::Match));
}

// ---------------------------------------------------------------------------
// capability_lie — advertise_then_observe_mismatch_iommu_returns_lie_hard
// ---------------------------------------------------------------------------

#[tokio::test]
async fn advertise_then_observe_mismatch_with_iommu_returns_lie_hard() {
    let detector = CapabilityLieDetector::new();
    detector
        .advertise(AdvertisedCapability {
            device_id: device("pci:00"),
            key: "iommu".into(),
            advertised_value: "on".into(),
        })
        .await
        .expect("advertise");

    let outcome = detector
        .observe(ObservedCapability {
            device_id: device("pci:00"),
            key: "iommu".into(),
            observed_value: "off".into(),
            observed_at: chrono::Utc::now(),
        })
        .await
        .expect("observe");

    match outcome {
        CapabilityLieOutcome::Lie { severity, .. } => {
            assert_eq!(severity, LieSeverity::Hard);
        }
        CapabilityLieOutcome::Match => panic!("expected Lie, got Match"),
    }
}

// ---------------------------------------------------------------------------
// capability_lie — advertise_then_observe_mismatch_driver_provenance_constitutional
// ---------------------------------------------------------------------------

#[tokio::test]
async fn advertise_then_observe_mismatch_with_driver_provenance_returns_lie_constitutional() {
    let detector = CapabilityLieDetector::new();
    detector
        .advertise(AdvertisedCapability {
            device_id: device("pci:00"),
            key: "driver_provenance".into(),
            advertised_value: "aios-signed".into(),
        })
        .await
        .expect("advertise");

    let outcome = detector
        .observe(ObservedCapability {
            device_id: device("pci:00"),
            key: "driver_provenance".into(),
            observed_value: "out-of-tree".into(),
            observed_at: chrono::Utc::now(),
        })
        .await
        .expect("observe");

    match outcome {
        CapabilityLieOutcome::Lie { severity, .. } => {
            assert_eq!(severity, LieSeverity::Constitutional);
        }
        CapabilityLieOutcome::Match => panic!("expected Lie, got Match"),
    }
}

// ---------------------------------------------------------------------------
// capability_lie — advertise_then_observe_mismatch_firmware_version_returns_lie_soft
// ---------------------------------------------------------------------------

#[tokio::test]
async fn advertise_then_observe_mismatch_with_firmware_version_returns_lie_soft() {
    let detector = CapabilityLieDetector::new();
    detector
        .advertise(AdvertisedCapability {
            device_id: device("pci:00"),
            key: "firmware_version".into(),
            advertised_value: "1.2.3".into(),
        })
        .await
        .expect("advertise");

    let outcome = detector
        .observe(ObservedCapability {
            device_id: device("pci:00"),
            key: "firmware_version".into(),
            observed_value: "1.2.4".into(),
            observed_at: chrono::Utc::now(),
        })
        .await
        .expect("observe");

    match outcome {
        CapabilityLieOutcome::Lie { severity, .. } => {
            assert_eq!(severity, LieSeverity::Soft);
        }
        CapabilityLieOutcome::Match => panic!("expected Lie, got Match"),
    }
}

// ---------------------------------------------------------------------------
// capability_lie — advertise_then_observe_mismatch_unknown_key_defaults_to_hard
// ---------------------------------------------------------------------------

#[tokio::test]
async fn advertise_then_observe_mismatch_with_unknown_key_defaults_to_hard() {
    let detector = CapabilityLieDetector::new();
    detector
        .advertise(AdvertisedCapability {
            device_id: device("pci:00"),
            key: "some_unknown_feature".into(),
            advertised_value: "yes".into(),
        })
        .await
        .expect("advertise");

    let outcome = detector
        .observe(ObservedCapability {
            device_id: device("pci:00"),
            key: "some_unknown_feature".into(),
            observed_value: "no".into(),
            observed_at: chrono::Utc::now(),
        })
        .await
        .expect("observe");

    match outcome {
        CapabilityLieOutcome::Lie { severity, .. } => {
            assert_eq!(severity, LieSeverity::Hard);
        }
        CapabilityLieOutcome::Match => panic!("expected Lie, got Match"),
    }
}

// ---------------------------------------------------------------------------
// capability_lie — list_advertised_after_3_returns_3
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_advertised_after_3_returns_3() {
    let detector = CapabilityLieDetector::new();
    for (id, key, val) in [
        ("pci:00", "iommu", "on"),
        ("pci:01", "tpm_pcr_count", "24"),
        ("pci:02", "firmware_version", "3.0"),
    ] {
        detector
            .advertise(AdvertisedCapability {
                device_id: device(id),
                key: key.into(),
                advertised_value: val.into(),
            })
            .await
            .expect("advertise");
    }

    let list = detector.list_advertised().await;
    assert_eq!(list.len(), 3);
}

// ---------------------------------------------------------------------------
// capability_lie — capability_lie_outcome_serde_round_trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn capability_lie_outcome_serde_round_trip() {
    // Lie variant
    let lie = CapabilityLieOutcome::Lie {
        device: device("pci:00"),
        key: "iommu".into(),
        advertised: "on".into(),
        observed: "off".into(),
        severity: LieSeverity::Hard,
    };
    let json = serde_json::to_string(&lie).expect("serialize");
    let back: CapabilityLieOutcome = serde_json::from_str(&json).expect("deserialize");
    match back {
        CapabilityLieOutcome::Lie {
            device,
            key,
            advertised,
            observed,
            severity,
        } => {
            assert_eq!(device, DeviceId("pci:00".into()));
            assert_eq!(key, "iommu");
            assert_eq!(advertised, "on");
            assert_eq!(observed, "off");
            assert_eq!(severity, LieSeverity::Hard);
        }
        CapabilityLieOutcome::Match => panic!("expected Lie"),
    }

    // Match variant
    let m = CapabilityLieOutcome::Match;
    let json2 = serde_json::to_string(&m).expect("serialize match");
    let back2: CapabilityLieOutcome = serde_json::from_str(&json2).expect("deserialize match");
    assert!(matches!(back2, CapabilityLieOutcome::Match));
}
