//! gRPC server adapters for `HardwareManagerService` + `GpuResourceService` +
//! `FirmwareTrustService` (T-172).
//!
//! Each adapter holds `Arc<dyn Trait>` / `Arc<Concrete>` fields, converts proto
//! requests to domain types via [`super::conversions`], calls the backing impl,
//! and translates errors to [`tonic::Status`] via [`hardware_error_to_status`].

#![allow(
    clippy::result_large_err,
    missing_docs,
    clippy::missing_errors_doc,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::wildcard_imports,
    clippy::too_many_lines
)]

use std::sync::Arc;

use tonic::{Request, Response, Status};

use crate::capability_lie::CapabilityLieDetector;
use crate::classifier::DeviceClassifier;
use crate::dmabuf::DmabufBroker;
use crate::drift::DriftDetector;
use crate::driver_binding::DriverBindingRegistry;
use crate::firmware_update::FirmwareUpdateOrchestrator;
use crate::gpu_resource::GpuResourceRegistry;
use crate::iommu::IommuFloorEnforcer;
use crate::manager::HardwareManager;
use crate::removable_policy::RemovableDevicePolicyTable;
use crate::service::conversions;
use crate::service::conversions::hardware_error_to_status;
use crate::service::proto;
use crate::service::proto::firmware_trust_service_server::{
    FirmwareTrustService, FirmwareTrustServiceServer,
};
use crate::service::proto::gpu_resource_service_server::{
    GpuResourceService, GpuResourceServiceServer,
};
use crate::service::proto::hardware_manager_service_server::{
    HardwareManagerService, HardwareManagerServiceServer,
};

// ── HardwareManagerServer ───────────────────────────────────────────────────

/// Mounts backing implementations behind the `HardwareManagerService`
#[derive(Clone)]
pub struct HardwareManagerServer {
    manager: Arc<dyn HardwareManager>,
    driver_registry: Arc<DriverBindingRegistry>,
    drift_detector: Arc<DriftDetector>,
    removable_policy: Arc<RemovableDevicePolicyTable>,
    iommu: Arc<IommuFloorEnforcer>,
    lie_detector: Arc<CapabilityLieDetector>,
}

impl std::fmt::Debug for HardwareManagerServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HardwareManagerServer")
            .finish_non_exhaustive()
    }
}

impl HardwareManagerServer {
    #[must_use]
    pub fn new(
        manager: Arc<dyn HardwareManager>,
        driver_registry: Arc<DriverBindingRegistry>,
        drift_detector: Arc<DriftDetector>,
        removable_policy: Arc<RemovableDevicePolicyTable>,
        iommu: Arc<IommuFloorEnforcer>,
        lie_detector: Arc<CapabilityLieDetector>,
    ) -> Self {
        Self {
            manager,
            driver_registry,
            drift_detector,
            removable_policy,
            iommu,
            lie_detector,
        }
    }
}

#[tonic::async_trait]
impl HardwareManagerService for HardwareManagerServer {
    // ── Device registry (5 RPCs) ──────────────────────────────────────────

    async fn register_device(
        &self,
        request: Request<proto::HardwareDeviceRecordProto>,
    ) -> Result<Response<()>, Status> {
        let record = conversions::device_record_from_proto(&request.into_inner())
            .map_err(|e| hardware_error_to_status(&e))?;
        self.manager
            .register_device(record)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn deregister_device(
        &self,
        request: Request<proto::DeregisterDeviceRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.manager
            .deregister_device(&crate::ids::DeviceId(r.device_id))
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn get_device(
        &self,
        request: Request<proto::GetDeviceRequest>,
    ) -> Result<Response<proto::HardwareDeviceRecordProto>, Status> {
        let r = request.into_inner();
        let record = self
            .manager
            .get_device(&crate::ids::DeviceId(r.device_id))
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(conversions::device_record_to_proto(&record)))
    }

    async fn list_pending_devices(
        &self,
        _request: Request<proto::ListPendingDevicesRequest>,
    ) -> Result<Response<proto::ListPendingDevicesResponse>, Status> {
        let devices = self.manager.list_pending_devices().await;
        let protos: Vec<proto::HardwareDeviceRecordProto> = devices
            .iter()
            .map(conversions::device_record_to_proto)
            .collect();
        Ok(Response::new(proto::ListPendingDevicesResponse {
            devices: protos,
        }))
    }

    async fn set_device_lifecycle(
        &self,
        request: Request<proto::SetDeviceLifecycleRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let state =
            conversions::lifecycle_from_proto(r.state).map_err(|e| hardware_error_to_status(&e))?;
        self.manager
            .set_device_lifecycle(&crate::ids::DeviceId(r.device_id), state)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    // ── Graph (2 RPCs) ────────────────────────────────────────────────────

    async fn rebuild_graph(
        &self,
        request: Request<proto::RebuildGraphRequest>,
    ) -> Result<Response<proto::HardwareGraphProto>, Status> {
        let r = request.into_inner();
        let key_bytes: [u8; 32] = r
            .signer_key
            .as_slice()
            .try_into()
            .map_err(|_| Status::invalid_argument("signer_key must be 32 bytes"))?;
        let signer = ed25519_dalek::SigningKey::from_bytes(&key_bytes);
        let graph = self
            .manager
            .rebuild_graph(&r.host_canonical_id, &signer, &r.signer_fingerprint)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(conversions::graph_to_proto(&graph)))
    }

    async fn current_graph(
        &self,
        _request: Request<proto::CurrentGraphRequest>,
    ) -> Result<Response<proto::CurrentGraphResponse>, Status> {
        let graph = self
            .manager
            .current_graph()
            .await
            .map(|g| conversions::graph_to_proto(&g));
        Ok(Response::new(proto::CurrentGraphResponse { graph }))
    }

    // ── Classification (1 RPC) ────────────────────────────────────────────

    async fn classify_observation(
        &self,
        request: Request<proto::RawDeviceObservationProto>,
    ) -> Result<Response<proto::HardwareDeviceRecordProto>, Status> {
        let obs = conversions::observation_from_proto(&request.into_inner());
        let record = DeviceClassifier::classify_with_trust(&obs, chrono::Utc::now())
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(conversions::device_record_to_proto(&record)))
    }

    // ── Driver binding (2 RPCs) ───────────────────────────────────────────

    async fn admit_driver_binding(
        &self,
        request: Request<proto::DriverBindingProto>,
    ) -> Result<Response<()>, Status> {
        let binding = conversions::driver_binding_from_proto(&request.into_inner())
            .map_err(|e| hardware_error_to_status(&e))?;
        self.driver_registry
            .admit_binding(binding)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn lookup_driver_binding(
        &self,
        request: Request<proto::LookupDriverBindingRequest>,
    ) -> Result<Response<proto::LookupDriverBindingResponse>, Status> {
        let r = request.into_inner();
        let binding = self
            .driver_registry
            .lookup_binding(&crate::ids::DeviceId(r.device_id))
            .await
            .map(|b| conversions::driver_binding_to_proto(&b));
        Ok(Response::new(proto::LookupDriverBindingResponse {
            binding,
        }))
    }

    // ── Drift (1 RPC) ─────────────────────────────────────────────────────

    async fn check_drift(
        &self,
        request: Request<proto::HardwareGraphProto>,
    ) -> Result<Response<proto::DriftSignalProto>, Status> {
        let graph = conversions::graph_from_proto(&request.into_inner())
            .map_err(|e| hardware_error_to_status(&e))?;
        let signal = self
            .drift_detector
            .check(&graph)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(conversions::drift_signal_to_proto(&signal)))
    }

    // ── Removable + IOMMU (3 RPCs) ────────────────────────────────────────

    async fn evaluate_removable_admission(
        &self,
        request: Request<proto::EvaluateRemovableAdmissionRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let bus =
            conversions::bus_kind_from_proto(r.bus).map_err(|e| hardware_error_to_status(&e))?;
        crate::iommu::evaluate_removable_admission(
            &self.removable_policy,
            &self.iommu,
            &crate::ids::DeviceId(r.device_id),
            bus,
            &r.requester,
        )
        .await
        .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn record_iommu_observation(
        &self,
        request: Request<proto::RecordIommuObservationRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let bus =
            conversions::bus_kind_from_proto(r.bus).map_err(|e| hardware_error_to_status(&e))?;
        self.iommu
            .record_observation(crate::ids::DeviceId(r.device_id), bus, r.iommu_observed)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn set_removable_policy(
        &self,
        request: Request<proto::SetRemovablePolicyRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let policy = conversions::removable_policy_from_proto(r.policy)?;
        self.removable_policy
            .set_policy(crate::ids::DeviceId(r.device_id), policy, &r.setter)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    // ── Capability-lie (2 RPCs) ───────────────────────────────────────────

    async fn advertise_capability(
        &self,
        request: Request<proto::AdvertisedCapabilityProto>,
    ) -> Result<Response<()>, Status> {
        let p = request.into_inner();
        let cap = crate::capability_lie::AdvertisedCapability {
            device_id: crate::ids::DeviceId(p.device_id),
            key: p.key,
            advertised_value: p.advertised_value,
        };
        self.lie_detector
            .advertise(cap)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn observe_capability(
        &self,
        request: Request<proto::ObservedCapabilityProto>,
    ) -> Result<Response<proto::CapabilityLieOutcomeProto>, Status> {
        let p = request.into_inner();
        let obs = crate::capability_lie::ObservedCapability {
            device_id: crate::ids::DeviceId(p.device_id),
            key: p.key,
            observed_value: p.observed_value,
            observed_at: conversions::datetime_from_proto(p.observed_at),
        };
        let outcome = self
            .lie_detector
            .observe(obs)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(conversions::lie_outcome_to_proto(&outcome)))
    }
}

// ── GpuResourceServer ───────────────────────────────────────────────────────

/// Mounts the GPU resource registry and dmabuf broker behind the gRPC
/// `GpuResourceService` trait.
#[derive(Clone)]
pub struct GpuResourceServer {
    registry: Arc<GpuResourceRegistry>,
    dmabuf: Arc<DmabufBroker>,
}

impl std::fmt::Debug for GpuResourceServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuResourceServer").finish_non_exhaustive()
    }
}

impl GpuResourceServer {
    #[must_use]
    pub fn new(registry: Arc<GpuResourceRegistry>, dmabuf: Arc<DmabufBroker>) -> Self {
        Self { registry, dmabuf }
    }
}

#[tonic::async_trait]
impl GpuResourceService for GpuResourceServer {
    // ── Device (2 RPCs) ───────────────────────────────────────────────────

    async fn register_gpu(
        &self,
        request: Request<proto::GpuDeviceProto>,
    ) -> Result<Response<()>, Status> {
        let device = conversions::gpu_device_from_proto(&request.into_inner())
            .map_err(|e| hardware_error_to_status(&e))?;
        self.registry
            .register_device(device)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn list_gpus(
        &self,
        _request: Request<proto::ListGpusRequest>,
    ) -> Result<Response<proto::ListGpusResponse>, Status> {
        let gpus = self.registry.list_devices().await;
        let protos: Vec<proto::GpuDeviceProto> =
            gpus.iter().map(conversions::gpu_device_to_proto).collect();
        Ok(Response::new(proto::ListGpusResponse { gpus: protos }))
    }

    // ── Partition (1 RPC) ─────────────────────────────────────────────────

    async fn ensure_partition(
        &self,
        request: Request<proto::EnsurePartitionRequest>,
    ) -> Result<Response<proto::VkDevicePartitionProto>, Status> {
        let r = request.into_inner();
        let partition = self
            .registry
            .ensure_partition(&crate::ids::GpuId(r.gpu_id), &r.group_id)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(conversions::partition_to_proto(&partition)))
    }

    // ── Binding (2 RPCs) ──────────────────────────────────────────────────

    async fn request_binding(
        &self,
        request: Request<proto::BindingRequestProto>,
    ) -> Result<Response<proto::GpuCapabilityBindingProto>, Status> {
        let req = conversions::binding_request_from_proto(&request.into_inner())
            .map_err(|e| hardware_error_to_status(&e))?;
        let binding = self
            .registry
            .request_binding(req)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(conversions::binding_to_proto(&binding)))
    }

    async fn release_binding(
        &self,
        request: Request<proto::ReleaseBindingRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.registry
            .release_binding(&r.binding_id)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    // ── Accounting (1 RPC) ────────────────────────────────────────────────

    async fn get_accounting(
        &self,
        request: Request<proto::GetAccountingRequest>,
    ) -> Result<Response<proto::GetAccountingResponse>, Status> {
        let r = request.into_inner();
        let entries = self
            .registry
            .get_accounting(&crate::ids::GpuId(r.gpu_id), &r.group_id)
            .await;
        let protos: Vec<proto::VramAccountingProto> = entries
            .iter()
            .map(conversions::accounting_to_proto)
            .collect();
        Ok(Response::new(proto::GetAccountingResponse {
            entries: protos,
        }))
    }

    // ── Dmabuf (4 RPCs) ───────────────────────────────────────────────────

    async fn create_dmabuf_handle(
        &self,
        request: Request<proto::DmabufHandleProto>,
    ) -> Result<Response<()>, Status> {
        let handle = conversions::dmabuf_handle_from_proto(&request.into_inner());
        self.dmabuf
            .create_handle(handle)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn authorize_dmabuf_peer_set(
        &self,
        request: Request<proto::DmabufPeerSetProto>,
    ) -> Result<Response<()>, Status> {
        let peer_set = conversions::dmabuf_peer_set_from_proto(&request.into_inner());
        self.dmabuf
            .authorize_peer_set(peer_set)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn check_dmabuf_import(
        &self,
        request: Request<proto::CheckDmabufImportRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.dmabuf
            .check_import(
                &r.handle_id,
                &crate::ids::GpuId(r.target_gpu),
                &r.target_group,
                &r.target_subject,
            )
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn revoke_dmabuf_handle(
        &self,
        request: Request<proto::RevokeDmabufHandleRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.dmabuf
            .revoke_handle(&r.handle_id)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }
}

// ── FirmwareTrustServer ─────────────────────────────────────────────────────

/// Mounts the firmware update orchestrator behind the gRPC
/// `FirmwareTrustService` trait.
#[derive(Clone)]
pub struct FirmwareTrustServer {
    orchestrator: Arc<FirmwareUpdateOrchestrator>,
}

impl std::fmt::Debug for FirmwareTrustServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FirmwareTrustServer")
            .finish_non_exhaustive()
    }
}

impl FirmwareTrustServer {
    #[must_use]
    pub fn new(orchestrator: Arc<FirmwareUpdateOrchestrator>) -> Self {
        Self { orchestrator }
    }
}

#[tonic::async_trait]
impl FirmwareTrustService for FirmwareTrustServer {
    async fn propose_firmware(
        &self,
        request: Request<proto::ProposeFirmwareRequest>,
    ) -> Result<Response<proto::FirmwareUpdatePlanProto>, Status> {
        let r = request.into_inner();
        let strategy = conversions::apply_strategy_from_proto(r.apply_strategy)
            .map_err(|e| hardware_error_to_status(&e))?;
        let blob = r
            .blob
            .ok_or_else(|| Status::invalid_argument("blob is required"))?;
        let blob = conversions::firmware_blob_from_proto(&blob)
            .map_err(|e| hardware_error_to_status(&e))?;
        let plan = self
            .orchestrator
            .propose(blob, strategy)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(conversions::firmware_plan_to_proto(&plan)))
    }

    async fn verify_firmware(
        &self,
        request: Request<proto::VerifyFirmwareRequest>,
    ) -> Result<Response<proto::VerifyFirmwareResponse>, Status> {
        let r = request.into_inner();
        let blob_id = crate::ids::FirmwareBlobId(r.blob_id);
        let result = self
            .orchestrator
            .verify(&blob_id)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(proto::VerifyFirmwareResponse {
            result: conversions::trust_result_to_proto(result),
        }))
    }

    async fn approve_firmware(
        &self,
        request: Request<proto::ApproveFirmwareRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.orchestrator
            .approve(&crate::ids::FirmwareBlobId(r.blob_id))
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn stage_firmware(
        &self,
        request: Request<proto::StageFirmwareRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.orchestrator
            .stage(&crate::ids::FirmwareBlobId(r.blob_id))
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn apply_firmware(
        &self,
        request: Request<proto::ApplyFirmwareRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.orchestrator
            .apply(&crate::ids::FirmwareBlobId(r.blob_id))
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn finalize_staged_apply(
        &self,
        request: Request<proto::FinalizeStagedApplyRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.orchestrator
            .finalize_staged_apply(&crate::ids::FirmwareBlobId(r.blob_id))
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn revert_firmware(
        &self,
        request: Request<proto::RevertFirmwareRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.orchestrator
            .revert(&crate::ids::FirmwareBlobId(r.blob_id), &r.reason)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn fail_firmware(
        &self,
        request: Request<proto::FailFirmwareRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.orchestrator
            .fail(&crate::ids::FirmwareBlobId(r.blob_id), &r.reason)
            .await
            .map_err(|e| hardware_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn get_firmware_plan(
        &self,
        request: Request<proto::GetFirmwarePlanRequest>,
    ) -> Result<Response<proto::FirmwareUpdatePlanProto>, Status> {
        let r = request.into_inner();
        let blob_id = r.blob_id.clone();
        let plan = self
            .orchestrator
            .get_plan(&crate::ids::FirmwareBlobId(blob_id))
            .await
            .ok_or_else(|| Status::not_found(format!("firmware plan not found: {}", r.blob_id)))?;
        Ok(Response::new(conversions::firmware_plan_to_proto(&plan)))
    }

    async fn list_firmware_plans(
        &self,
        _request: Request<proto::ListFirmwarePlansRequest>,
    ) -> Result<Response<proto::ListFirmwarePlansResponse>, Status> {
        let plans = self.orchestrator.list_plans().await;
        let protos: Vec<proto::FirmwareUpdatePlanProto> = plans
            .iter()
            .map(conversions::firmware_plan_to_proto)
            .collect();
        Ok(Response::new(proto::ListFirmwarePlansResponse {
            plans: protos,
        }))
    }
}

// ── Router builders ─────────────────────────────────────────────────────────

use tonic::transport::server::Router;

/// Build a tonic [`Router`] for the `HardwareManagerService`.
pub fn build_hardware_router(svc: HardwareManagerServer) -> Router {
    tonic::transport::server::Server::builder().add_service(HardwareManagerServiceServer::new(svc))
}

/// Build a tonic [`Router`] for the `GpuResourceService`.
pub fn build_gpu_router(svc: GpuResourceServer) -> Router {
    tonic::transport::server::Server::builder().add_service(GpuResourceServiceServer::new(svc))
}

/// Build a tonic [`Router`] for the `FirmwareTrustService`.
pub fn build_firmware_router(svc: FirmwareTrustServer) -> Router {
    tonic::transport::server::Server::builder().add_service(FirmwareTrustServiceServer::new(svc))
}
