//! gRPC `SandboxService` server adapter (T-110).
//!
//! Wraps `InMemorySandboxComposer`, `GpuPolicyEnforcer`, and `ResourceLimitEnforcer`
//! behind a tonic gRPC service implementing the S3.2 `SandboxService` surface.

use std::sync::Arc;

use async_trait::async_trait;
use tonic::{Request, Response, Status};

use crate::composer::{ComposeRequest, SandboxComposer, SubjectRef};
use crate::gpu_enforcer::GpuPolicyEnforcer;
use crate::in_memory_composer::InMemorySandboxComposer;
use crate::resource_enforcer::ResourceLimitEnforcer;
use crate::service::conversions::{
    gpu_binding_to_proto, gpu_policy_from_proto, iommu_status_from_proto,
    resource_limits_from_proto, resource_remaining_to_proto, resource_request_from_proto,
    resource_usage_from_proto, sandbox_error_to_status, sandbox_profile_from_proto,
    sandbox_profile_to_proto,
};
use crate::service::proto;
use crate::{ProfileId, SandboxError};

/// gRPC adapter mounting the in-memory sandbox composer + enforcers behind tonic.
#[derive(Clone)]
pub struct SandboxServiceImpl {
    composer: Arc<InMemorySandboxComposer>,
    gpu_enforcer: Arc<GpuPolicyEnforcer>,
    resource_enforcer: Arc<ResourceLimitEnforcer>,
}

impl SandboxServiceImpl {
    /// Construct an adapter over the sandbox composer and policy enforcers.
    #[must_use]
    pub const fn new(
        composer: Arc<InMemorySandboxComposer>,
        gpu_enforcer: Arc<GpuPolicyEnforcer>,
        resource_enforcer: Arc<ResourceLimitEnforcer>,
    ) -> Self {
        Self {
            composer,
            gpu_enforcer,
            resource_enforcer,
        }
    }
}

#[async_trait]
impl proto::sandbox_service_server::SandboxService for SandboxServiceImpl {
    // ── ComposeProfile ──

    async fn compose_profile(
        &self,
        request: Request<proto::ComposeProfileRequest>,
    ) -> Result<Response<proto::ComposeProfileResponse>, Status> {
        let req = request.into_inner();

        let compose_req = ComposeRequest {
            subject: SubjectRef(req.subject),
            action_kind: req.action_kind,
            base_profile_id: req.base_profile_id.map(ProfileId),
            adapter_default: req
                .adapter_default
                .map(sandbox_profile_from_proto)
                .transpose()?,
            app_manifest: req
                .app_manifest
                .map(sandbox_profile_from_proto)
                .transpose()?,
            user_request: req
                .user_request
                .map(sandbox_profile_from_proto)
                .transpose()?,
            policy_required: req
                .policy_required
                .map(sandbox_profile_from_proto)
                .transpose()?,
            group_floor: req
                .group_floor
                .map(sandbox_profile_from_proto)
                .transpose()?,
            runtime_safety_floor: req
                .runtime_safety_floor
                .map(sandbox_profile_from_proto)
                .transpose()?,
            recovery_mode: req.recovery_mode,
            is_ai: req.is_ai,
        };

        let result = self
            .composer
            .compose(compose_req)
            .await
            .map_err(sandbox_error_to_status)?;

        Ok(Response::new(proto::ComposeProfileResponse {
            profile: Some(sandbox_profile_to_proto(&result.profile)),
            merged_sources: result.merged_sources,
            recovery_mode_enforced: result.recovery_mode_enforced,
            ai_mode_enforced: result.ai_mode_enforced,
        }))
    }

    // ── GetProfile ──

    async fn get_profile(
        &self,
        request: Request<proto::GetProfileRequest>,
    ) -> Result<Response<proto::GetProfileResponse>, Status> {
        let req = request.into_inner();
        let profile_id = ProfileId(req.profile_id);

        let profile = self
            .composer
            .get_profile(&profile_id)
            .await
            .map_err(sandbox_error_to_status)?;

        Ok(Response::new(proto::GetProfileResponse {
            profile: Some(sandbox_profile_to_proto(&profile)),
        }))
    }

    // ── ListProfiles ──

    async fn list_profiles(
        &self,
        _request: Request<proto::ListProfilesRequest>,
    ) -> Result<Response<proto::ListProfilesResponse>, Status> {
        let profiles = self
            .composer
            .list_profiles()
            .await
            .map_err(sandbox_error_to_status)?;

        Ok(Response::new(proto::ListProfilesResponse {
            profiles: profiles.iter().map(sandbox_profile_to_proto).collect(),
        }))
    }

    // ── ValidateProfile ──

    async fn validate_profile(
        &self,
        request: Request<proto::ValidateProfileRequest>,
    ) -> Result<Response<proto::ValidateProfileResponse>, Status> {
        let req = request.into_inner();
        let profile = req
            .profile
            .ok_or_else(|| Status::invalid_argument("profile is required"))?;

        let sp = sandbox_profile_from_proto(profile)?;

        sp.resource_limits
            .validate()
            .map_err(sandbox_error_to_status)?;

        Ok(Response::new(proto::ValidateProfileResponse {}))
    }

    // ── ValidateGpuPolicy ──

    async fn validate_gpu_policy(
        &self,
        request: Request<proto::ValidateGpuPolicyRequest>,
    ) -> Result<Response<proto::ValidateGpuPolicyResponse>, Status> {
        let req = request.into_inner();
        let policy = req
            .policy
            .map(gpu_policy_from_proto)
            .transpose()?
            .ok_or_else(|| Status::invalid_argument("policy is required"))?;
        let iommu_status = iommu_status_from_proto(req.iommu_status)?;

        // Internal consistency: dmabuf ⇒ iommu_required, >PassiveDisplay ⇒ vk_device
        self.gpu_enforcer
            .validate_policy(&policy)
            .map_err(sandbox_error_to_status)?;

        // Environmental check: iommu_required with unavailable IOMMU is degraded
        if policy.iommu_required && !iommu_status.is_available() {
            return Err(sandbox_error_to_status(SandboxError::GpuPolicyViolation(
                "iommu_required=true but IOMMU is not available; GPU DMA isolation degraded".into(),
            )));
        }

        Ok(Response::new(proto::ValidateGpuPolicyResponse {}))
    }

    // ── ComputeGpuBinding ──

    async fn compute_gpu_binding(
        &self,
        request: Request<proto::ComputeGpuBindingRequest>,
    ) -> Result<Response<proto::ComputeGpuBindingResponse>, Status> {
        let req = request.into_inner();
        let policy = req
            .policy
            .map(gpu_policy_from_proto)
            .transpose()?
            .ok_or_else(|| Status::invalid_argument("policy is required"))?;
        let iommu_status = iommu_status_from_proto(req.iommu_status)?;
        let subject = SubjectRef(req.subject);

        // Build a temporary enforcer with the request's IOMMU status
        let enforcer =
            GpuPolicyEnforcer::new(&self.gpu_enforcer.trusted_gpu_authority, iommu_status);

        let binding = enforcer
            .compute_capability_binding(&policy, &req.group_id, &subject)
            .map_err(sandbox_error_to_status)?;

        Ok(Response::new(proto::ComputeGpuBindingResponse {
            binding: Some(gpu_binding_to_proto(&binding)),
        }))
    }

    // ── CheckResourceUsage ──

    async fn check_resource_usage(
        &self,
        request: Request<proto::CheckResourceUsageRequest>,
    ) -> Result<Response<proto::CheckResourceUsageResponse>, Status> {
        let req = request.into_inner();
        let resource_req = req
            .request
            .map(resource_request_from_proto)
            .ok_or_else(|| Status::invalid_argument("request is required"))?;
        let limits = req
            .limits
            .map(resource_limits_from_proto)
            .ok_or_else(|| Status::invalid_argument("limits is required"))?;

        self.resource_enforcer
            .check_usage(&resource_req, &limits)
            .map_err(sandbox_error_to_status)?;

        Ok(Response::new(proto::CheckResourceUsageResponse {}))
    }

    // ── ComputeResourceRemaining ──

    async fn compute_resource_remaining(
        &self,
        request: Request<proto::ComputeResourceRemainingRequest>,
    ) -> Result<Response<proto::ComputeResourceRemainingResponse>, Status> {
        let req = request.into_inner();
        let usage = req
            .usage
            .map(resource_usage_from_proto)
            .ok_or_else(|| Status::invalid_argument("usage is required"))?;
        let limits = req
            .limits
            .map(resource_limits_from_proto)
            .ok_or_else(|| Status::invalid_argument("limits is required"))?;

        let remaining = self.resource_enforcer.compute_remaining(&usage, &limits);

        Ok(Response::new(proto::ComputeResourceRemainingResponse {
            remaining: Some(resource_remaining_to_proto(&remaining)),
        }))
    }

    // ── ValidateSyscall ──

    async fn validate_syscall(
        &self,
        request: Request<proto::ValidateSyscallRequest>,
    ) -> Result<Response<proto::ValidateSyscallResponse>, Status> {
        let req = request.into_inner();

        let allowlist: Option<Vec<String>> = if req.allowlist.is_empty() {
            None
        } else {
            Some(req.allowlist)
        };

        self.resource_enforcer
            .validate_syscall(&req.syscall, allowlist.as_deref())
            .map_err(sandbox_error_to_status)?;

        Ok(Response::new(proto::ValidateSyscallResponse {}))
    }
}
