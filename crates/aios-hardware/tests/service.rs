//! Integration tests for the gRPC `HardwareManagerService` +
//! `GpuResourceService` + `FirmwareTrustService` surfaces (T-172).
//!
//! Each test boots an in-process tonic server backed by in-memory implementations,
//! connects via a TCP listener, and exercises one RPC path.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::wildcard_imports,
    clippy::similar_names,
    clippy::too_many_lines,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use tonic::Request;

use aios_hardware::capability_lie::CapabilityLieDetector;
use aios_hardware::dmabuf::DmabufBroker;
use aios_hardware::drift::{DriftDetector, PriorGraphStore};
use aios_hardware::driver_binding::DriverBindingRegistry;
use aios_hardware::firmware::FirmwareUpdateClass;
use aios_hardware::firmware_update::FirmwareUpdateOrchestrator;
use aios_hardware::gpu_resource::GpuResourceRegistry;
use aios_hardware::iommu::IommuFloorEnforcer;
use aios_hardware::manager::InMemoryHardwareManager;
use aios_hardware::removable_policy::RemovableDevicePolicyTable;
use aios_hardware::service::proto::firmware_trust_service_client::FirmwareTrustServiceClient;
use aios_hardware::service::proto::gpu_resource_service_client::GpuResourceServiceClient;
use aios_hardware::service::proto::hardware_manager_service_client::HardwareManagerServiceClient;
use aios_hardware::service::proto::{
    AdvertisedCapabilityProto, ApplyFirmwareRequest, ApproveFirmwareRequest, BindingRequestProto,
    BusKindProto, CheckDmabufImportRequest, CurrentGraphRequest, DeviceClassProto,
    DeviceLifecycleStateProto, DmabufHandleProto, DmabufPeerProto, DmabufPeerSetProto,
    DriverBindingProto, DriverProvenanceProto, EvaluateRemovableAdmissionRequest,
    FirmwareApplyStrategyProto, FirmwareScopeProto, FirmwareUpdateClassProto, GetAccountingRequest,
    GetDeviceRequest, GetFirmwarePlanRequest, GpuCapabilityClassProto, GpuDeviceProto,
    GpuVendorKindProto, ListGpusRequest, ListPendingDevicesRequest, LookupDriverBindingRequest,
    ObservedCapabilityProto, ProposeFirmwareRequest, RawDeviceObservationProto,
    RebuildGraphRequest, RecordIommuObservationRequest, ReleaseBindingRequest,
    RemovableDevicePolicyProto, SetDeviceLifecycleRequest, SetRemovablePolicyRequest,
    StageFirmwareRequest, VerifyFirmwareRequest,
};
use aios_hardware::service::{
    build_firmware_router, build_gpu_router, build_hardware_router, FirmwareTrustServer,
    GpuResourceServer, HardwareManagerServer,
};

// ── Helpers ─────────────────────────────────────────────────────────────────

fn dummy_device_record() -> aios_hardware::service::proto::HardwareDeviceRecordProto {
    aios_hardware::service::proto::HardwareDeviceRecordProto {
        device_id: "pci-0000:00:02.0".into(),
        class: DeviceClassProto::GpuIntegrated as i32,
        bus: BusKindProto::Pcie as i32,
        vendor_id: 0x8086,
        product_id: 0x9bc4,
        vendor_name: "Intel".into(),
        product_name: "UHD Graphics 630".into(),
        trust_class: aios_hardware::service::proto::DeviceTrustClassProto::Untrusted as i32,
        lifecycle: DeviceLifecycleStateProto::Detected as i32,
        driver_provenance: None,
        firmware_version: None,
        removable: false,
        iommu_protected: true,
        probed_at: None,
    }
}

// ── HardwareManager test harness ────────────────────────────────────────────

struct Harness {
    client: HardwareManagerServiceClient<tonic::transport::Channel>,
}

async fn make_hardware_svc(
    driver_registry: Option<Arc<DriverBindingRegistry>>,
) -> HardwareManagerServiceClient<tonic::transport::Channel> {
    let manager = Arc::new(InMemoryHardwareManager::new());
    let dr = driver_registry.unwrap_or_else(|| Arc::new(DriverBindingRegistry::new()));
    let drift_detector = Arc::new(DriftDetector::new(Arc::new(PriorGraphStore::new())));
    let removable_policy = Arc::new(RemovableDevicePolicyTable::new());
    let iommu = Arc::new(IommuFloorEnforcer::new());
    let lie_detector = Arc::new(CapabilityLieDetector::new());

    let svc = HardwareManagerServer::new(
        manager,
        dr,
        drift_detector,
        removable_policy,
        iommu,
        lie_detector,
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = build_hardware_router(svc);

    tokio::spawn(async move {
        router
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    HardwareManagerServiceClient::connect(format!("http://{addr}"))
        .await
        .unwrap()
}

impl Harness {
    async fn new() -> Self {
        Self {
            client: make_hardware_svc(None).await,
        }
    }
}

// ── GpuResource test harness ────────────────────────────────────────────────

struct GpuHarness {
    client: GpuResourceServiceClient<tonic::transport::Channel>,
}

async fn make_gpu_client() -> GpuResourceServiceClient<tonic::transport::Channel> {
    let registry = Arc::new(GpuResourceRegistry::new());
    let dmabuf = Arc::new(DmabufBroker::new());
    let svc = GpuResourceServer::new(registry, dmabuf);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = build_gpu_router(svc);

    tokio::spawn(async move {
        router
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    GpuResourceServiceClient::connect(format!("http://{addr}"))
        .await
        .unwrap()
}

impl GpuHarness {
    async fn new() -> Self {
        Self {
            client: make_gpu_client().await,
        }
    }
}

// ── FirmwareTrust test harness ──────────────────────────────────────────────

struct FirmwareHarness {
    client: FirmwareTrustServiceClient<tonic::transport::Channel>,
}

async fn make_firmware_client(
    orch: Option<Arc<FirmwareUpdateOrchestrator>>,
) -> FirmwareTrustServiceClient<tonic::transport::Channel> {
    let orchestrator = orch.unwrap_or_else(|| Arc::new(FirmwareUpdateOrchestrator::new()));
    let svc = FirmwareTrustServer::new(orchestrator);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = build_firmware_router(svc);

    tokio::spawn(async move {
        router
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    FirmwareTrustServiceClient::connect(format!("http://{addr}"))
        .await
        .unwrap()
}

impl FirmwareHarness {
    async fn new() -> Self {
        Self {
            client: make_firmware_client(None).await,
        }
    }
}

// ── HardwareManagerService tests ────────────────────────────────────────────

#[tokio::test]
async fn test_register_device() {
    let h = Harness::new().await;
    let mut rec = dummy_device_record();
    rec.device_id = "pci-0000:00:02.0".into();

    h.client
        .clone()
        .register_device(Request::new(rec.clone()))
        .await
        .unwrap();

    let fetched = h
        .client
        .clone()
        .get_device(Request::new(GetDeviceRequest {
            device_id: "pci-0000:00:02.0".into(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(fetched.device_id, "pci-0000:00:02.0");
    assert_eq!(fetched.class, DeviceClassProto::GpuIntegrated as i32);
}

#[tokio::test]
async fn test_deregister_device() {
    let h = Harness::new().await;
    let mut rec = dummy_device_record();
    rec.device_id = "usb-3-1".into();

    h.client
        .clone()
        .register_device(Request::new(rec))
        .await
        .unwrap();

    h.client
        .clone()
        .deregister_device(Request::new(
            aios_hardware::service::proto::DeregisterDeviceRequest {
                device_id: "usb-3-1".into(),
            },
        ))
        .await
        .unwrap();

    let err = h
        .client
        .clone()
        .get_device(Request::new(GetDeviceRequest {
            device_id: "usb-3-1".into(),
        }))
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn test_get_device_not_found() {
    let h = Harness::new().await;
    let err = h
        .client
        .clone()
        .get_device(Request::new(GetDeviceRequest {
            device_id: "nonexistent-device".into(),
        }))
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn test_list_pending_devices() {
    let h = Harness::new().await;

    let mut d1 = dummy_device_record();
    d1.device_id = "dev-a".into();
    let mut d2 = dummy_device_record();
    d2.device_id = "dev-b".into();

    h.client
        .clone()
        .register_device(Request::new(d1))
        .await
        .unwrap();
    h.client
        .clone()
        .register_device(Request::new(d2))
        .await
        .unwrap();

    let list = h
        .client
        .clone()
        .list_pending_devices(Request::new(ListPendingDevicesRequest {}))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(list.devices.len(), 2);
}

#[tokio::test]
async fn test_set_device_lifecycle() {
    let h = Harness::new().await;
    let mut rec = dummy_device_record();
    rec.device_id = "dev-lifecycle-test".into();

    h.client
        .clone()
        .register_device(Request::new(rec))
        .await
        .unwrap();

    h.client
        .clone()
        .set_device_lifecycle(Request::new(SetDeviceLifecycleRequest {
            device_id: "dev-lifecycle-test".into(),
            state: DeviceLifecycleStateProto::Bound as i32,
        }))
        .await
        .unwrap();

    let fetched = h
        .client
        .clone()
        .get_device(Request::new(GetDeviceRequest {
            device_id: "dev-lifecycle-test".into(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(fetched.lifecycle, DeviceLifecycleStateProto::Bound as i32);
}

#[tokio::test]
async fn test_classify_observation() {
    let h = Harness::new().await;

    // PCI base class 0x03 = display; sub_class 0x00 = VGA
    let obs = RawDeviceObservationProto {
        bus: BusKindProto::Pcie as i32,
        bus_address: "0000:00:02.0".into(),
        vendor_id: 0x8086,
        product_id: 0x9bc4,
        class_hint: 0x0003_0000, // base=0x03(display), sub=0x00(VGA)
        vendor_name: Some("Intel".into()),
        product_name: Some("UHD Graphics".into()),
        removable_hint: false,
        iommu_protected_hint: true,
        firmware_version_hint: None,
    };

    let result = h
        .client
        .clone()
        .classify_observation(Request::new(obs))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(result.class, DeviceClassProto::GpuIntegrated as i32);
    assert_eq!(result.bus, BusKindProto::Pcie as i32);
}

#[tokio::test]
async fn test_driver_binding_flow() {
    let sk = SigningKey::from_bytes(&[0x42u8; 32]);
    let pk = sk.verifying_key();
    let fp = "sha256:aaaa";

    let mut registry = DriverBindingRegistry::new();
    registry.register_authority(fp, pk);

    let client = make_hardware_svc(Some(Arc::new(registry))).await;

    let mut binding = DriverBindingProto {
        binding_id: "bind-001".into(),
        device_id: "dev-driver-test".into(),
        driver_module_name: "i915".into(),
        kernel_module_version: "1.0".into(),
        provenance: DriverProvenanceProto::AiosVerified as i32,
        blake3_hash: "deadbeef".into(),
        signer_fingerprint: fp.into(),
        signature: vec![],
        admitted_at: None,
    };

    // Sign with driver binding canonical bytes format:
    // binding_id\n device_id\n driver_module_name\n kernel_module_version\n provenance_label\n blake3_hash
    let mut msg = Vec::new();
    msg.extend_from_slice(b"bind-001");
    msg.push(b'\n');
    msg.extend_from_slice(b"dev-driver-test");
    msg.push(b'\n');
    msg.extend_from_slice(b"i915");
    msg.push(b'\n');
    msg.extend_from_slice(b"1.0");
    msg.push(b'\n');
    msg.extend_from_slice(b"aios-verified");
    msg.push(b'\n');
    msg.extend_from_slice(b"deadbeef");
    let sig = sk.sign(&msg);
    binding.signature = sig.to_vec();

    client
        .clone()
        .admit_driver_binding(Request::new(binding))
        .await
        .unwrap();

    let lookup = client
        .clone()
        .lookup_driver_binding(Request::new(LookupDriverBindingRequest {
            device_id: "dev-driver-test".into(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(lookup.binding.is_some());
    let b = lookup.binding.unwrap();
    assert_eq!(b.device_id, "dev-driver-test");
    assert_eq!(b.driver_module_name, "i915");
    drop(client);
}

#[tokio::test]
async fn test_set_removable_policy() {
    let h = Harness::new().await;

    h.client
        .clone()
        .set_removable_policy(Request::new(SetRemovablePolicyRequest {
            device_id: "removable-dev".into(),
            policy: RemovableDevicePolicyProto::AllowReadOnly as i32,
            setter: "aios-admin".into(),
        }))
        .await
        .unwrap();

    h.client
        .clone()
        .evaluate_removable_admission(Request::new(EvaluateRemovableAdmissionRequest {
            device_id: "removable-dev".into(),
            bus: BusKindProto::Usb3 as i32,
            requester: "aios-admin".into(),
        }))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_capability_lie_detect() {
    let h = Harness::new().await;

    let adv = AdvertisedCapabilityProto {
        device_id: "cap-dev".into(),
        key: "pcie-gen".into(),
        advertised_value: "4".into(),
    };
    h.client
        .clone()
        .advertise_capability(Request::new(adv))
        .await
        .unwrap();

    let obs = ObservedCapabilityProto {
        device_id: "cap-dev".into(),
        key: "pcie-gen".into(),
        observed_value: "4".into(),
        observed_at: None,
    };
    let outcome = h
        .client
        .clone()
        .observe_capability(Request::new(obs))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(outcome.outcome, 1); // MATCH
}

#[tokio::test]
async fn test_record_iommu_observation() {
    let h = Harness::new().await;

    h.client
        .clone()
        .record_iommu_observation(Request::new(RecordIommuObservationRequest {
            device_id: "iommu-test-dev".into(),
            bus: BusKindProto::Pcie as i32,
            iommu_observed: true,
        }))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_rebuild_and_current_graph() {
    let h = Harness::new().await;

    let mut rec = dummy_device_record();
    rec.device_id = "graph-test-dev".into();
    h.client
        .clone()
        .register_device(Request::new(rec))
        .await
        .unwrap();

    let sk = SigningKey::from_bytes(&[0xAAu8; 32]);
    let graph = h
        .client
        .clone()
        .rebuild_graph(Request::new(RebuildGraphRequest {
            host_canonical_id: "host-01".into(),
            signer_key: sk.to_bytes().to_vec(),
            signer_fingerprint: "sha256:graph-signer".into(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!graph.graph_id.is_empty());
    assert_eq!(graph.host_canonical_id, "host-01");

    let curr = h
        .client
        .clone()
        .current_graph(Request::new(CurrentGraphRequest {}))
        .await
        .unwrap()
        .into_inner();

    assert!(curr.graph.is_some());
}

// ── GpuResourceService tests ────────────────────────────────────────────────

#[tokio::test]
async fn test_register_and_list_gpus() {
    let h = GpuHarness::new().await;

    let gpu = GpuDeviceProto {
        gpu_id: "gpu-amd-render".into(),
        vendor: GpuVendorKindProto::Amd as i32,
        product_name: "Radeon RX 6800".into(),
        vram_total_bytes: 16 * 1024 * 1024 * 1024,
        supported_classes: vec![GpuCapabilityClassProto::RenderAndCompute as i32],
        iommu_protected: true,
        host_canonical_id: "host-01".into(),
    };

    h.client
        .clone()
        .register_gpu(Request::new(gpu))
        .await
        .unwrap();

    let list = h
        .client
        .clone()
        .list_gpus(Request::new(ListGpusRequest {}))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(list.gpus.len(), 1);
    assert_eq!(list.gpus[0].product_name, "Radeon RX 6800");
}

#[tokio::test]
async fn test_request_gpu_binding() {
    let h = GpuHarness::new().await;

    let gpu = GpuDeviceProto {
        gpu_id: "gpu-bind-test".into(),
        vendor: GpuVendorKindProto::Nvidia as i32,
        product_name: "RTX 4090".into(),
        vram_total_bytes: 24 * 1024 * 1024 * 1024,
        supported_classes: vec![
            GpuCapabilityClassProto::RenderAndCompute as i32,
            GpuCapabilityClassProto::VideoEncode as i32,
        ],
        iommu_protected: true,
        host_canonical_id: "host-01".into(),
    };

    h.client
        .clone()
        .register_gpu(Request::new(gpu))
        .await
        .unwrap();

    let binding = h
        .client
        .clone()
        .request_binding(Request::new(BindingRequestProto {
            gpu_id: "gpu-bind-test".into(),
            group_id: "render-group".into(),
            subject_canonical_id: "subject-01".into(),
            capability_class: GpuCapabilityClassProto::RenderAndCompute as i32,
            vram_bytes: 8 * 1024 * 1024 * 1024,
            ttl_seconds: None,
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(binding.gpu_id, "gpu-bind-test");
    assert_eq!(binding.group_id, "render-group");
    assert_eq!(binding.vram_bytes_reserved, 8 * 1024 * 1024 * 1024);

    let acct = h
        .client
        .clone()
        .get_accounting(Request::new(GetAccountingRequest {
            gpu_id: "gpu-bind-test".into(),
            group_id: "render-group".into(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(acct.entries.len(), 1);

    h.client
        .clone()
        .release_binding(Request::new(ReleaseBindingRequest {
            binding_id: binding.binding_id.clone(),
        }))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_dmabuf_flow() {
    let h = GpuHarness::new().await;

    let handle_id = "dmabuf-001".to_string();
    h.client
        .clone()
        .create_dmabuf_handle(Request::new(DmabufHandleProto {
            handle_id: handle_id.clone(),
            source_gpu: "gpu-src".into(),
            source_group: "group-a".into(),
            source_subject: "subject-src".into(),
            size_bytes: 64 * 1024 * 1024,
            format_code: 0x3432_5241,
            created_at: None,
        }))
        .await
        .unwrap();

    h.client
        .clone()
        .authorize_dmabuf_peer_set(Request::new(DmabufPeerSetProto {
            handle_id: handle_id.clone(),
            authorized_peers: vec![DmabufPeerProto {
                target_gpu: "gpu-dst".into(),
                target_group: "group-b".into(),
                target_subject: "subject-dst".into(),
            }],
            policy_decision_id: "policy-001".into(),
        }))
        .await
        .unwrap();

    h.client
        .clone()
        .check_dmabuf_import(Request::new(CheckDmabufImportRequest {
            handle_id: handle_id.clone(),
            target_gpu: "gpu-dst".into(),
            target_group: "group-b".into(),
            target_subject: "subject-dst".into(),
        }))
        .await
        .unwrap();

    h.client
        .clone()
        .revoke_dmabuf_handle(Request::new(
            aios_hardware::service::proto::RevokeDmabufHandleRequest {
                handle_id: handle_id.clone(),
            },
        ))
        .await
        .unwrap();
}

// ── FirmwareTrustService tests ──────────────────────────────────────────────

fn build_signing_msg(
    blob_id: &str,
    class: FirmwareUpdateClass,
    scope: aios_hardware::firmware::FirmwareScope,
    version: &str,
    hash: &str,
) -> Vec<u8> {
    let mut msg = Vec::new();
    msg.extend_from_slice(blob_id.as_bytes());
    msg.extend_from_slice(class.label().as_bytes());
    msg.extend_from_slice(scope.label().as_bytes());
    msg.extend_from_slice(version.as_bytes());
    msg.extend_from_slice(hash.as_bytes());
    msg
}

#[tokio::test]
async fn test_firmware_propose_and_list() {
    let h = FirmwareHarness::new().await;

    let blob = aios_hardware::service::proto::FirmwareBlobProto {
        blob_id: "fw-blob-001".into(),
        update_class: FirmwareUpdateClassProto::CpuMicrocode as i32,
        scope: FirmwareScopeProto::CpuScope as i32,
        target_device: None,
        vendor_name: "Intel".into(),
        version: "0x42".into(),
        blake3_hash: "abc123".into(),
        signature: vec![],
        signer_fingerprint: String::new(),
        published_at: None,
    };

    let plan = h
        .client
        .clone()
        .propose_firmware(Request::new(ProposeFirmwareRequest {
            blob: Some(blob),
            apply_strategy: FirmwareApplyStrategyProto::Atomic as i32,
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(plan.current_state, 1); // PROPOSED

    let list = h
        .client
        .clone()
        .list_firmware_plans(Request::new(
            aios_hardware::service::proto::ListFirmwarePlansRequest {},
        ))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(list.plans.len(), 1);
}

#[tokio::test]
async fn test_firmware_verify_unsigned_fails() {
    let h = FirmwareHarness::new().await;

    let blob = aios_hardware::service::proto::FirmwareBlobProto {
        blob_id: "fw-unsigned".into(),
        update_class: FirmwareUpdateClassProto::GpuFirmware as i32,
        scope: FirmwareScopeProto::GpuScope as i32,
        target_device: None,
        vendor_name: "AMD".into(),
        version: "1.0".into(),
        blake3_hash: "deadbeef".into(),
        signature: vec![],
        signer_fingerprint: String::new(),
        published_at: None,
    };

    h.client
        .clone()
        .propose_firmware(Request::new(ProposeFirmwareRequest {
            blob: Some(blob),
            apply_strategy: FirmwareApplyStrategyProto::Atomic as i32,
        }))
        .await
        .unwrap();

    let err = h
        .client
        .clone()
        .verify_firmware(Request::new(VerifyFirmwareRequest {
            blob_id: "fw-unsigned".into(),
        }))
        .await
        .unwrap_err();

    assert_eq!(err.code(), tonic::Code::PermissionDenied);
}

#[tokio::test]
async fn test_firmware_verify_known_signer() {
    let sk = SigningKey::from_bytes(&[0x55u8; 32]);
    let pk = sk.verifying_key();
    let fp = "sha256:test-publisher";

    let mut orch = FirmwareUpdateOrchestrator::new();
    orch.register_aios_publisher_key(fp, pk);

    let blob_id = "fw-signed".to_string();
    let class = FirmwareUpdateClass::CpuMicrocode;
    let scope = aios_hardware::firmware::FirmwareScope::Cpu;
    let msg = build_signing_msg(&blob_id, class, scope, "2.0", "blake3hash");
    let sig = sk.sign(&msg);

    let mut client = make_firmware_client(Some(Arc::new(orch))).await;

    let blob = aios_hardware::service::proto::FirmwareBlobProto {
        blob_id: blob_id.clone(),
        update_class: FirmwareUpdateClassProto::CpuMicrocode as i32,
        scope: FirmwareScopeProto::CpuScope as i32,
        target_device: None,
        vendor_name: "Intel".into(),
        version: "2.0".into(),
        blake3_hash: "blake3hash".into(),
        signature: sig.to_vec(),
        signer_fingerprint: fp.into(),
        published_at: None,
    };

    client
        .propose_firmware(Request::new(ProposeFirmwareRequest {
            blob: Some(blob),
            apply_strategy: FirmwareApplyStrategyProto::Atomic as i32,
        }))
        .await
        .unwrap();

    let result = client
        .verify_firmware(Request::new(VerifyFirmwareRequest {
            blob_id: blob_id.clone(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(
        result.result,
        aios_hardware::service::proto::FirmwareTrustResultProto::AiosPublisherSigned as i32
    );
    drop(client);
}

#[tokio::test]
async fn test_firmware_full_fsm() {
    let sk = SigningKey::from_bytes(&[0x66u8; 32]);
    let pk = sk.verifying_key();
    let fp = "sha256:full-fsm-pub";

    let mut orch = FirmwareUpdateOrchestrator::new();
    orch.register_aios_publisher_key(fp, pk);

    let blob_id = "fw-full-fsm".to_string();
    let class = FirmwareUpdateClass::GpuFirmware;
    let scope = aios_hardware::firmware::FirmwareScope::Gpu;
    let msg = build_signing_msg(&blob_id, class, scope, "3.0", "fullhash");
    let sig = sk.sign(&msg);

    let mut client = make_firmware_client(Some(Arc::new(orch))).await;

    let blob = aios_hardware::service::proto::FirmwareBlobProto {
        blob_id: blob_id.clone(),
        update_class: FirmwareUpdateClassProto::GpuFirmware as i32,
        scope: FirmwareScopeProto::GpuScope as i32,
        target_device: None,
        vendor_name: "NVIDIA".into(),
        version: "3.0".into(),
        blake3_hash: "fullhash".into(),
        signature: sig.to_vec(),
        signer_fingerprint: fp.into(),
        published_at: None,
    };

    // Propose
    let plan = client
        .propose_firmware(Request::new(ProposeFirmwareRequest {
            blob: Some(blob),
            apply_strategy: FirmwareApplyStrategyProto::Atomic as i32,
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(plan.current_state, 1); // PROPOSED

    // Verify
    let verify = client
        .verify_firmware(Request::new(VerifyFirmwareRequest {
            blob_id: blob_id.clone(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        verify.result,
        aios_hardware::service::proto::FirmwareTrustResultProto::AiosPublisherSigned as i32
    );

    // Approve
    client
        .approve_firmware(Request::new(ApproveFirmwareRequest {
            blob_id: blob_id.clone(),
        }))
        .await
        .unwrap();

    // Stage
    client
        .stage_firmware(Request::new(StageFirmwareRequest {
            blob_id: blob_id.clone(),
        }))
        .await
        .unwrap();

    // Apply
    client
        .apply_firmware(Request::new(ApplyFirmwareRequest {
            blob_id: blob_id.clone(),
        }))
        .await
        .unwrap();

    // Get plan — should now be APPLIED
    let plan = client
        .get_firmware_plan(Request::new(GetFirmwarePlanRequest {
            blob_id: blob_id.clone(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(plan.current_state, 6); // APPLIED
    drop(client);
}

#[tokio::test]
async fn test_firmware_get_plan_not_found() {
    let h = FirmwareHarness::new().await;

    let err = h
        .client
        .clone()
        .get_firmware_plan(Request::new(GetFirmwarePlanRequest {
            blob_id: "nonexistent".into(),
        }))
        .await
        .unwrap_err();

    assert_eq!(err.code(), tonic::Code::NotFound);
}
