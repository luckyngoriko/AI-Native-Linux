//! Rust↔proto conversions for the gRPC `SandboxService` surface (T-110).
//!
//! Also contains `SandboxError` → `tonic::Status` mapping per S3.2.

#![allow(
    missing_docs,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::result_large_err,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "conversion function names are intentionally literal and covered by tests"
)]

use chrono::{DateTime, TimeZone, Utc};
use prost_types::Timestamp;
use tonic::Status;

use crate::composer::SubjectRef;
use crate::service::proto;
use crate::{
    GpuCapabilityBinding, GpuCapabilityClass, GpuPolicy, IommuStatus, IsolationKind,
    NetworkPosture, ProfileId, ResourceLimits, ResourceRemaining, ResourceRequest, ResourceUsage,
    SandboxError, SandboxProfile,
};

// ---------------------------------------------------------------------------
// Timestamp helpers
// ---------------------------------------------------------------------------

pub fn datetime_to_proto(dt: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: dt.timestamp(),
        nanos: i32::try_from(dt.timestamp_subsec_nanos()).unwrap_or(0),
    }
}

pub fn datetime_from_proto(ts: Timestamp) -> DateTime<Utc> {
    Utc.timestamp_opt(ts.seconds, u32::try_from(ts.nanos).unwrap_or(0))
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap_or_default())
}

pub fn optional_datetime_to_proto(dt: Option<DateTime<Utc>>) -> Option<Timestamp> {
    dt.map(datetime_to_proto)
}

pub fn optional_datetime_from_proto(ts: Option<Timestamp>) -> Option<DateTime<Utc>> {
    ts.map(datetime_from_proto)
}

// ---------------------------------------------------------------------------
// IsolationKind
// ---------------------------------------------------------------------------

pub const fn isolation_kind_to_proto(kind: IsolationKind) -> i32 {
    match kind {
        IsolationKind::NamespaceLocal => proto::IsolationKindProto::NamespaceLocal as i32,
        IsolationKind::ProcessContainer => proto::IsolationKindProto::ProcessContainer as i32,
        IsolationKind::VmGuest => proto::IsolationKindProto::VmGuest as i32,
        IsolationKind::BrowserOriginIsolated => {
            proto::IsolationKindProto::BrowserOriginIsolated as i32
        }
        IsolationKind::NoIsolation => proto::IsolationKindProto::NoIsolation as i32,
    }
}

pub fn isolation_kind_from_proto(v: i32) -> Result<IsolationKind, Status> {
    let kind = proto::IsolationKindProto::try_from(v)
        .map_err(|_| Status::invalid_argument(format!("unknown IsolationKindProto value: {v}")))?;
    match kind {
        proto::IsolationKindProto::NamespaceLocal => Ok(IsolationKind::NamespaceLocal),
        proto::IsolationKindProto::ProcessContainer => Ok(IsolationKind::ProcessContainer),
        proto::IsolationKindProto::VmGuest => Ok(IsolationKind::VmGuest),
        proto::IsolationKindProto::BrowserOriginIsolated => {
            Ok(IsolationKind::BrowserOriginIsolated)
        }
        proto::IsolationKindProto::NoIsolation => Ok(IsolationKind::NoIsolation),
        proto::IsolationKindProto::IsolationKindUnspecified => Err(Status::invalid_argument(
            "IsolationKindProto must not be ISOLATION_KIND_UNSPECIFIED",
        )),
    }
}

// ---------------------------------------------------------------------------
// GpuCapabilityClass
// ---------------------------------------------------------------------------

pub const fn gpu_capability_class_to_proto(class: GpuCapabilityClass) -> i32 {
    match class {
        GpuCapabilityClass::GpuPassiveDisplay => {
            proto::GpuCapabilityClassProto::GpuPassiveDisplay as i32
        }
        GpuCapabilityClass::GpuBasic2d => proto::GpuCapabilityClassProto::GpuBasic2d as i32,
        GpuCapabilityClass::GpuRich2d => proto::GpuCapabilityClassProto::GpuRich2d as i32,
        GpuCapabilityClass::GpuFull3d => proto::GpuCapabilityClassProto::GpuFull3d as i32,
        GpuCapabilityClass::GpuComputeHeavy => {
            proto::GpuCapabilityClassProto::GpuComputeHeavy as i32
        }
    }
}

pub fn gpu_capability_class_from_proto(v: i32) -> Result<GpuCapabilityClass, Status> {
    let class = proto::GpuCapabilityClassProto::try_from(v).map_err(|_| {
        Status::invalid_argument(format!("unknown GpuCapabilityClassProto value: {v}"))
    })?;
    match class {
        proto::GpuCapabilityClassProto::GpuPassiveDisplay => {
            Ok(GpuCapabilityClass::GpuPassiveDisplay)
        }
        proto::GpuCapabilityClassProto::GpuBasic2d => Ok(GpuCapabilityClass::GpuBasic2d),
        proto::GpuCapabilityClassProto::GpuRich2d => Ok(GpuCapabilityClass::GpuRich2d),
        proto::GpuCapabilityClassProto::GpuFull3d => Ok(GpuCapabilityClass::GpuFull3d),
        proto::GpuCapabilityClassProto::GpuComputeHeavy => Ok(GpuCapabilityClass::GpuComputeHeavy),
        proto::GpuCapabilityClassProto::GpuCapabilityClassUnspecified => Err(
            Status::invalid_argument("GpuCapabilityClassProto must not be UNSPECIFIED"),
        ),
    }
}

// ---------------------------------------------------------------------------
// NetworkPosture
// ---------------------------------------------------------------------------

pub const fn network_posture_to_proto(posture: NetworkPosture) -> i32 {
    match posture {
        NetworkPosture::DenyAll => proto::NetworkPostureProto::DenyAll as i32,
        NetworkPosture::LoopbackOnly => proto::NetworkPostureProto::LoopbackOnly as i32,
        NetworkPosture::HostLimited => proto::NetworkPostureProto::HostLimited as i32,
        NetworkPosture::ExplicitAllowlist => proto::NetworkPostureProto::ExplicitAllowlist as i32,
        NetworkPosture::Full => proto::NetworkPostureProto::Full as i32,
    }
}

pub fn network_posture_from_proto(v: i32) -> Result<NetworkPosture, Status> {
    let posture = proto::NetworkPostureProto::try_from(v)
        .map_err(|_| Status::invalid_argument(format!("unknown NetworkPostureProto value: {v}")))?;
    match posture {
        proto::NetworkPostureProto::DenyAll => Ok(NetworkPosture::DenyAll),
        proto::NetworkPostureProto::LoopbackOnly => Ok(NetworkPosture::LoopbackOnly),
        proto::NetworkPostureProto::HostLimited => Ok(NetworkPosture::HostLimited),
        proto::NetworkPostureProto::ExplicitAllowlist => Ok(NetworkPosture::ExplicitAllowlist),
        proto::NetworkPostureProto::Full => Ok(NetworkPosture::Full),
        proto::NetworkPostureProto::NetworkPostureUnspecified => Err(Status::invalid_argument(
            "NetworkPostureProto must not be UNSPECIFIED",
        )),
    }
}

// ---------------------------------------------------------------------------
// IommuStatus
// ---------------------------------------------------------------------------

pub const fn iommu_status_to_proto(status: IommuStatus) -> i32 {
    match status {
        IommuStatus::Available => proto::IommuStatusProto::Available as i32,
        IommuStatus::Unavailable => proto::IommuStatusProto::Unavailable as i32,
        IommuStatus::Unknown => proto::IommuStatusProto::Unknown as i32,
    }
}

pub fn iommu_status_from_proto(v: i32) -> Result<IommuStatus, Status> {
    let status = proto::IommuStatusProto::try_from(v)
        .map_err(|_| Status::invalid_argument(format!("unknown IommuStatusProto value: {v}")))?;
    match status {
        proto::IommuStatusProto::Available => Ok(IommuStatus::Available),
        proto::IommuStatusProto::Unavailable => Ok(IommuStatus::Unavailable),
        proto::IommuStatusProto::Unknown => Ok(IommuStatus::Unknown),
        proto::IommuStatusProto::IommuStatusUnspecified => Err(Status::invalid_argument(
            "IommuStatusProto must not be UNSPECIFIED",
        )),
    }
}

// ---------------------------------------------------------------------------
// ResourceLimits
// ---------------------------------------------------------------------------

pub fn resource_limits_to_proto(limits: &ResourceLimits) -> proto::ResourceLimitsProto {
    proto::ResourceLimitsProto {
        cpu_quota_percent: limits.cpu_quota_percent,
        memory_max_bytes: limits.memory_max_bytes,
        io_max_bytes_per_sec: limits.io_max_bytes_per_sec,
        network_max_bytes_per_sec: limits.network_max_bytes_per_sec,
        process_max_count: limits.process_max_count,
        file_descriptor_max: limits.file_descriptor_max,
        expires_at: optional_datetime_to_proto(limits.expires_at),
    }
}

pub fn resource_limits_from_proto(pl: proto::ResourceLimitsProto) -> ResourceLimits {
    ResourceLimits {
        cpu_quota_percent: pl.cpu_quota_percent,
        memory_max_bytes: pl.memory_max_bytes,
        io_max_bytes_per_sec: pl.io_max_bytes_per_sec,
        network_max_bytes_per_sec: pl.network_max_bytes_per_sec,
        process_max_count: pl.process_max_count,
        file_descriptor_max: pl.file_descriptor_max,
        expires_at: optional_datetime_from_proto(pl.expires_at),
    }
}

// ---------------------------------------------------------------------------
// GpuPolicy
// ---------------------------------------------------------------------------

pub fn gpu_policy_to_proto(policy: &GpuPolicy) -> proto::GpuPolicyProto {
    proto::GpuPolicyProto {
        gpu_capability_class: gpu_capability_class_to_proto(policy.gpu_capability_class),
        vk_device_required: policy.vk_device_required,
        dmabuf_passthrough_allowed: policy.dmabuf_passthrough_allowed,
        per_group_partitioning: policy.per_group_partitioning,
        iommu_required: policy.iommu_required,
        expires_at: optional_datetime_to_proto(policy.expires_at),
    }
}

pub fn gpu_policy_from_proto(pp: proto::GpuPolicyProto) -> Result<GpuPolicy, Status> {
    Ok(GpuPolicy {
        gpu_capability_class: gpu_capability_class_from_proto(pp.gpu_capability_class)?,
        vk_device_required: pp.vk_device_required,
        dmabuf_passthrough_allowed: pp.dmabuf_passthrough_allowed,
        per_group_partitioning: pp.per_group_partitioning,
        iommu_required: pp.iommu_required,
        expires_at: optional_datetime_from_proto(pp.expires_at),
    })
}

// ---------------------------------------------------------------------------
// SandboxProfile
// ---------------------------------------------------------------------------

pub fn sandbox_profile_to_proto(profile: &SandboxProfile) -> proto::SandboxProfileProto {
    proto::SandboxProfileProto {
        profile_id: profile.profile_id.to_string(),
        name: profile.name.clone(),
        description: profile.description.clone(),
        isolation_kind: isolation_kind_to_proto(profile.isolation_kind),
        resource_limits: Some(resource_limits_to_proto(&profile.resource_limits)),
        gpu_policy: Some(gpu_policy_to_proto(&profile.gpu_policy)),
        network_posture: network_posture_to_proto(profile.network_posture),
        syscall_allowlist: profile.syscall_allowlist.clone().unwrap_or_default(),
        signing_authority: profile.signing_authority.clone(),
        signature_ed25519: profile.signature_ed25519.clone(),
    }
}

pub fn sandbox_profile_from_proto(
    pp: proto::SandboxProfileProto,
) -> Result<SandboxProfile, Status> {
    let syscall_allowlist = if pp.syscall_allowlist.is_empty() {
        None
    } else {
        Some(pp.syscall_allowlist)
    };

    Ok(SandboxProfile {
        profile_id: ProfileId(pp.profile_id),
        name: pp.name,
        description: pp.description,
        isolation_kind: isolation_kind_from_proto(pp.isolation_kind)?,
        resource_limits: pp
            .resource_limits
            .map_or_else(ResourceLimits::default_strict, resource_limits_from_proto),
        gpu_policy: pp
            .gpu_policy
            .map(gpu_policy_from_proto)
            .transpose()?
            .unwrap_or_else(GpuPolicy::default_deny_all),
        network_posture: network_posture_from_proto(pp.network_posture)?,
        syscall_allowlist,
        signing_authority: pp.signing_authority,
        signature_ed25519: pp.signature_ed25519,
    })
}

// ---------------------------------------------------------------------------
// ResourceRequest / ResourceUsage / ResourceRemaining
// ---------------------------------------------------------------------------

pub const fn resource_request_to_proto(req: &ResourceRequest) -> proto::ResourceRequestProto {
    proto::ResourceRequestProto {
        cpu_pct: req.cpu_pct,
        memory_bytes: req.memory_bytes,
        io_bytes_per_sec: req.io_bytes_per_sec,
        network_bytes_per_sec: req.network_bytes_per_sec,
        process_count: req.process_count,
        fd_count: req.fd_count,
    }
}

pub const fn resource_request_from_proto(pr: proto::ResourceRequestProto) -> ResourceRequest {
    ResourceRequest {
        cpu_pct: pr.cpu_pct,
        memory_bytes: pr.memory_bytes,
        io_bytes_per_sec: pr.io_bytes_per_sec,
        network_bytes_per_sec: pr.network_bytes_per_sec,
        process_count: pr.process_count,
        fd_count: pr.fd_count,
    }
}

pub const fn resource_usage_to_proto(usage: &ResourceUsage) -> proto::ResourceUsageProto {
    proto::ResourceUsageProto {
        cpu_pct: usage.cpu_pct,
        memory_bytes: usage.memory_bytes,
        io_bytes_per_sec: usage.io_bytes_per_sec,
        network_bytes_per_sec: usage.network_bytes_per_sec,
        process_count: usage.process_count,
        fd_count: usage.fd_count,
    }
}

pub const fn resource_usage_from_proto(pu: proto::ResourceUsageProto) -> ResourceUsage {
    ResourceUsage {
        cpu_pct: pu.cpu_pct,
        memory_bytes: pu.memory_bytes,
        io_bytes_per_sec: pu.io_bytes_per_sec,
        network_bytes_per_sec: pu.network_bytes_per_sec,
        process_count: pu.process_count,
        fd_count: pu.fd_count,
    }
}

pub const fn resource_remaining_to_proto(rem: &ResourceRemaining) -> proto::ResourceRemainingProto {
    proto::ResourceRemainingProto {
        cpu_pct: rem.cpu_pct,
        memory_bytes: rem.memory_bytes,
        io_bytes_per_sec: rem.io_bytes_per_sec,
        network_bytes_per_sec: rem.network_bytes_per_sec,
        process_count: rem.process_count,
        fd_count: rem.fd_count,
    }
}

pub const fn resource_remaining_from_proto(pr: proto::ResourceRemainingProto) -> ResourceRemaining {
    ResourceRemaining {
        cpu_pct: pr.cpu_pct,
        memory_bytes: pr.memory_bytes,
        io_bytes_per_sec: pr.io_bytes_per_sec,
        network_bytes_per_sec: pr.network_bytes_per_sec,
        process_count: pr.process_count,
        fd_count: pr.fd_count,
    }
}

// ---------------------------------------------------------------------------
// GpuCapabilityBinding
// ---------------------------------------------------------------------------

pub fn gpu_binding_to_proto(binding: &GpuCapabilityBinding) -> proto::GpuCapabilityBindingProto {
    proto::GpuCapabilityBindingProto {
        binding_id: binding.binding_id.clone(),
        gpu_capability_class: gpu_capability_class_to_proto(binding.gpu_capability_class),
        group_id: binding.group_id.clone(),
        subject: binding.subject.to_string(),
        vk_device_required: binding.vk_device_required,
        dmabuf_passthrough_allowed: binding.dmabuf_passthrough_allowed,
        iommu_required: binding.iommu_required,
        degraded_isolation: binding.degraded_isolation,
        issued_at: Some(datetime_to_proto(binding.issued_at)),
        expires_at: optional_datetime_to_proto(binding.expires_at),
    }
}

pub fn gpu_binding_from_proto(
    pb: proto::GpuCapabilityBindingProto,
) -> Result<GpuCapabilityBinding, Status> {
    Ok(GpuCapabilityBinding {
        binding_id: pb.binding_id,
        gpu_capability_class: gpu_capability_class_from_proto(pb.gpu_capability_class)?,
        group_id: pb.group_id,
        subject: SubjectRef(pb.subject),
        vk_device_required: pb.vk_device_required,
        dmabuf_passthrough_allowed: pb.dmabuf_passthrough_allowed,
        iommu_required: pb.iommu_required,
        degraded_isolation: pb.degraded_isolation,
        issued_at: pb.issued_at.map_or_else(Utc::now, datetime_from_proto),
        expires_at: optional_datetime_from_proto(pb.expires_at),
    })
}

// ---------------------------------------------------------------------------
// SandboxError → tonic::Status
// ---------------------------------------------------------------------------

/// Map a `SandboxError` to a `tonic::Status` with the appropriate gRPC code.
#[allow(
    clippy::needless_pass_by_value,
    reason = "error consumed for owned Status payload"
)]
pub fn sandbox_error_to_status(err: SandboxError) -> Status {
    match &err {
        SandboxError::ProfileNotFound(_) => Status::not_found(err.to_string()),
        SandboxError::InvalidProfile(_)
        | SandboxError::ResourceLimitsViolation { .. }
        | SandboxError::GpuPolicyViolation(_)
        | SandboxError::SyscallNotAllowed { .. }
        | SandboxError::IsolationKindNotSupported { .. } => {
            Status::failed_precondition(err.to_string())
        }
        SandboxError::ManifestSignatureInvalid | SandboxError::ManifestUnknownAuthority(_) => {
            Status::permission_denied(err.to_string())
        }
        SandboxError::Internal(_) => Status::internal(err.to_string()),
    }
}
