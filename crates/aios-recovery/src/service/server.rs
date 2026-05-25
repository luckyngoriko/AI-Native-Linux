//! gRPC `RecoveryService` server adapter + bootstrap helpers (T-079).

#![allow(clippy::result_large_err)]

use std::net::SocketAddr;
use std::sync::Arc;

use aios_fs::{AiosPath, SubjectRef};
use async_trait::async_trait;
use tonic::transport::server::{Router, Server};
use tonic::{Request, Response, Status};

use crate::service::conversions::{
    candidate_id_from_string, enter_recovery_request_from_proto, first_boot_context_to_proto,
    kernel_candidate_to_proto, kernel_manifest_from_proto, recovery_error_to_status,
    recovery_state_to_proto,
};
use crate::service::proto;
use crate::service::SCHEMA_VERSION;
use crate::{
    FirstBootDriver, InMemoryRecoveryBoundary, KernelPipelineDriver, RecoveryBoundary,
    RecoveryGuard,
};

/// gRPC adapter mounting the in-memory S9 recovery drivers behind tonic.
#[derive(Clone)]
pub struct RecoveryServiceImpl {
    boundary: Arc<InMemoryRecoveryBoundary>,
    first_boot: Arc<FirstBootDriver>,
    kernel_pipeline: Arc<KernelPipelineDriver>,
    guard: Arc<RecoveryGuard>,
}

impl RecoveryServiceImpl {
    /// Construct an adapter over the S9 in-memory recovery drivers.
    #[must_use]
    pub const fn new(
        boundary: Arc<InMemoryRecoveryBoundary>,
        first_boot: Arc<FirstBootDriver>,
        kernel_pipeline: Arc<KernelPipelineDriver>,
        guard: Arc<RecoveryGuard>,
    ) -> Self {
        Self {
            boundary,
            first_boot,
            kernel_pipeline,
            guard,
        }
    }

    /// Return the wrapped recovery boundary.
    #[must_use]
    pub fn boundary(&self) -> Arc<InMemoryRecoveryBoundary> {
        Arc::clone(&self.boundary)
    }

    /// Return the wrapped first-boot driver.
    #[must_use]
    pub fn first_boot(&self) -> Arc<FirstBootDriver> {
        Arc::clone(&self.first_boot)
    }

    /// Return the wrapped kernel pipeline driver.
    #[must_use]
    pub fn kernel_pipeline(&self) -> Arc<KernelPipelineDriver> {
        Arc::clone(&self.kernel_pipeline)
    }

    /// Return the wrapped recovery guard.
    #[must_use]
    pub fn guard(&self) -> Arc<RecoveryGuard> {
        Arc::clone(&self.guard)
    }
}

#[async_trait]
impl proto::recovery_service_server::RecoveryService for RecoveryServiceImpl {
    async fn enter_recovery(
        &self,
        request: Request<proto::EnterRecoveryRequestProto>,
    ) -> Result<Response<proto::RecoveryStateProto>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        let request = enter_recovery_request_from_proto(request)?;
        let state = self
            .boundary
            .enter_recovery(request)
            .await
            .map_err(|err| recovery_error_to_status(&err))?;
        Ok(Response::new(recovery_state_to_proto(&state)))
    }

    async fn exit_recovery(
        &self,
        request: Request<proto::ExitRecoveryRequestProto>,
    ) -> Result<Response<proto::RecoveryStateProto>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        if request.exit_token.trim().is_empty() {
            return Err(Status::invalid_argument("exit_token is required"));
        }
        let state = self
            .boundary
            .exit_recovery(&request.exit_token)
            .await
            .map_err(|err| recovery_error_to_status(&err))?;
        Ok(Response::new(recovery_state_to_proto(&state)))
    }

    async fn get_recovery_state(
        &self,
        request: Request<proto::GetRecoveryStateRequest>,
    ) -> Result<Response<proto::RecoveryStateProto>, Status> {
        validate_schema_version(&request.into_inner().schema_version)?;
        let state = self.boundary.current_state().await;
        Ok(Response::new(recovery_state_to_proto(&state)))
    }

    async fn register_kernel_candidate(
        &self,
        request: Request<proto::RegisterKernelCandidateRequest>,
    ) -> Result<Response<proto::KernelCandidateProto>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        let manifest = kernel_manifest_from_proto(
            request
                .manifest
                .ok_or_else(|| Status::invalid_argument("manifest is required"))?,
        )?;
        let candidate = self
            .kernel_pipeline
            .register_candidate(manifest, request.signature_ed25519)
            .await
            .map_err(|err| recovery_error_to_status(&err))?;
        Ok(Response::new(kernel_candidate_to_proto(&candidate)))
    }

    async fn verify_kernel_candidate(
        &self,
        request: Request<proto::VerifyKernelCandidateRequest>,
    ) -> Result<Response<proto::KernelCandidateProto>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        let candidate_id = candidate_id_from_string(request.candidate_id)?;
        let candidate = self
            .kernel_pipeline
            .verify_candidate(&candidate_id)
            .await
            .map_err(|err| recovery_error_to_status(&err))?;
        Ok(Response::new(kernel_candidate_to_proto(&candidate)))
    }

    async fn activate_kernel_candidate(
        &self,
        request: Request<proto::ActivateKernelCandidateRequest>,
    ) -> Result<Response<proto::KernelCandidateProto>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        let candidate_id = candidate_id_from_string(request.candidate_id)?;
        let candidate = self
            .kernel_pipeline
            .activate_candidate(&candidate_id)
            .await
            .map_err(|err| recovery_error_to_status(&err))?;
        Ok(Response::new(kernel_candidate_to_proto(&candidate)))
    }

    async fn rollback_kernel_candidate(
        &self,
        request: Request<proto::RollbackKernelCandidateRequest>,
    ) -> Result<Response<proto::KernelCandidateProto>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        let candidate_id = candidate_id_from_string(request.candidate_id)?;
        let candidate = self
            .kernel_pipeline
            .rollback_candidate(&candidate_id)
            .await
            .map_err(|err| recovery_error_to_status(&err))?;
        Ok(Response::new(kernel_candidate_to_proto(&candidate)))
    }

    async fn list_kernel_candidates(
        &self,
        request: Request<proto::ListKernelCandidatesRequest>,
    ) -> Result<Response<proto::ListKernelCandidatesResponse>, Status> {
        validate_schema_version(&request.into_inner().schema_version)?;
        let candidates = self
            .kernel_pipeline
            .list_candidates()
            .await
            .iter()
            .map(kernel_candidate_to_proto)
            .collect();
        Ok(Response::new(proto::ListKernelCandidatesResponse {
            candidates,
        }))
    }

    async fn get_active_kernel(
        &self,
        request: Request<proto::GetActiveKernelRequest>,
    ) -> Result<Response<proto::GetActiveKernelResponse>, Status> {
        validate_schema_version(&request.into_inner().schema_version)?;
        let active = self
            .kernel_pipeline
            .get_active()
            .await
            .map(|candidate| kernel_candidate_to_proto(&candidate));
        Ok(Response::new(proto::GetActiveKernelResponse { active }))
    }

    async fn run_first_boot_provisioning(
        &self,
        request: Request<proto::RunFirstBootProvisioningRequest>,
    ) -> Result<Response<proto::FirstBootContextProto>, Status> {
        validate_schema_version(&request.into_inner().schema_version)?;
        let context = self
            .first_boot
            .run_provisioning()
            .await
            .map_err(|err| recovery_error_to_status(&err))?;
        Ok(Response::new(first_boot_context_to_proto(&context)))
    }

    async fn get_first_boot_status(
        &self,
        request: Request<proto::GetFirstBootStatusRequest>,
    ) -> Result<Response<proto::FirstBootContextProto>, Status> {
        validate_schema_version(&request.into_inner().schema_version)?;
        let context = self.first_boot.current_context().await;
        Ok(Response::new(first_boot_context_to_proto(&context)))
    }

    async fn check_recovery_mutation(
        &self,
        request: Request<proto::CheckRecoveryMutationRequest>,
    ) -> Result<Response<proto::CheckRecoveryMutationResponse>, Status> {
        let request = request.into_inner();
        validate_schema_version(&request.schema_version)?;
        if request.path.trim().is_empty() {
            return Err(Status::invalid_argument("path is required"));
        }
        if request.subject.trim().is_empty() {
            return Err(Status::invalid_argument("subject is required"));
        }

        self.guard
            .check_mutation(
                &AiosPath::new(request.path),
                &SubjectRef(request.subject),
                request.is_ai,
            )
            .await
            .map_err(|err| recovery_error_to_status(&err))?;
        Ok(Response::new(proto::CheckRecoveryMutationResponse {
            allowed: true,
        }))
    }
}

fn validate_schema_version(schema_version: &str) -> Result<(), Status> {
    if schema_version.is_empty() || schema_version == SCHEMA_VERSION {
        return Ok(());
    }
    Err(Status::failed_precondition(format!(
        "unsupported schema_version `{schema_version}`"
    )))
}

/// Build a `tonic::transport::server::Router` with `RecoveryService` mounted.
#[must_use]
pub fn build_router(svc: RecoveryServiceImpl) -> Router {
    Server::builder().add_service(proto::recovery_service_server::RecoveryServiceServer::new(
        svc,
    ))
}

/// Convenience helper: bind to `addr` and serve until the future is dropped.
///
/// # Errors
///
/// Returns the underlying [`tonic::transport::Error`] when the bind / listen
/// loop fails.
pub async fn serve(
    svc: RecoveryServiceImpl,
    addr: SocketAddr,
) -> Result<(), tonic::transport::Error> {
    build_router(svc).serve(addr).await
}
