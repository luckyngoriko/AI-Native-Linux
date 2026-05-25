//! Rust-to-proto translations for the gRPC `SgrService` surface (T-089).

#![allow(
    missing_docs,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::result_large_err,
    clippy::too_many_lines,
    reason = "conversion function names are intentionally literal and covered by tests"
)]

use chrono::{DateTime, TimeZone, Utc};
use prost_types::{value::Kind, ListValue, NullValue, Timestamp, Value as ProstValue};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use tonic::Status;

use crate::service::proto;
use crate::{
    AdapterCapability, AdapterDeclaration, AdapterRegistrationState, DependencyEdge,
    DependencyKind, DesiredState, GpuBudget, GraphState, HealthCheckKind, HealthCheckSpec,
    RegisteredAdapter, ResourceBudget, RestartBudget, RestartPolicy, RollbackPointer,
    RollbackTrigger, ServiceUnit, SgrError, UnitDependency, UnitId, UnitKind, UnitManifest,
    UnitState, VerificationIntentRef,
};

pub const ACTION_ID_FORMAT_UNSPECIFIED: u32 = 0;
pub const ACTION_ID_FORMAT_UTF8: u32 = 1;

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

fn required_datetime_from_proto(
    ts: Option<Timestamp>,
    field: &'static str,
) -> Result<DateTime<Utc>, Status> {
    ts.map(datetime_from_proto)
        .ok_or_else(|| Status::invalid_argument(format!("{field} is required")))
}

// ---------------------------------------------------------------------------
// JSON Value helpers
// ---------------------------------------------------------------------------

pub fn json_to_prost_value(value: &JsonValue) -> ProstValue {
    let kind = match value {
        JsonValue::Null => Kind::NullValue(NullValue::NullValue as i32),
        JsonValue::Bool(value) => Kind::BoolValue(*value),
        JsonValue::Number(value) => Kind::NumberValue(value.as_f64().unwrap_or_default()),
        JsonValue::String(value) => Kind::StringValue(value.clone()),
        JsonValue::Array(values) => Kind::ListValue(ListValue {
            values: values.iter().map(json_to_prost_value).collect(),
        }),
        JsonValue::Object(map) => Kind::StructValue(prost_types::Struct {
            fields: map
                .iter()
                .map(|(key, value)| (key.clone(), json_to_prost_value(value)))
                .collect(),
        }),
    };
    ProstValue { kind: Some(kind) }
}

pub fn prost_value_to_json(value: &ProstValue) -> JsonValue {
    match value.kind.as_ref() {
        Some(Kind::NullValue(_)) | None => JsonValue::Null,
        Some(Kind::NumberValue(value)) => number_to_json(*value),
        Some(Kind::StringValue(value)) => JsonValue::String(value.clone()),
        Some(Kind::BoolValue(value)) => JsonValue::Bool(*value),
        Some(Kind::StructValue(value)) => JsonValue::Object(
            value
                .fields
                .iter()
                .map(|(key, value)| (key.clone(), prost_value_to_json(value)))
                .collect::<JsonMap<String, JsonValue>>(),
        ),
        Some(Kind::ListValue(value)) => {
            JsonValue::Array(value.values.iter().map(prost_value_to_json).collect())
        }
    }
}

fn number_to_json(value: f64) -> JsonValue {
    JsonNumber::from_f64(value).map_or(JsonValue::Null, JsonValue::Number)
}

fn optional_prost_value_to_json(value: Option<&ProstValue>) -> JsonValue {
    value.map_or(JsonValue::Null, prost_value_to_json)
}

// ---------------------------------------------------------------------------
// Error -> tonic::Status
// ---------------------------------------------------------------------------

pub fn sgr_error_to_status(err: &SgrError) -> Status {
    match err {
        SgrError::UnitNotFound(_) => Status::not_found(err.to_string()),
        SgrError::UnitAlreadyRegistered(_) => Status::already_exists(err.to_string()),
        SgrError::DependencyCycleDetected(_)
        | SgrError::InvalidStateTransition { .. }
        | SgrError::AdapterCapabilityMismatch { .. } => {
            Status::failed_precondition(err.to_string())
        }
        SgrError::ManifestSignatureInvalid
        | SgrError::ManifestUnknownAuthority(_)
        | SgrError::AdapterSuspended(_) => Status::permission_denied(err.to_string()),
        SgrError::DependencyTargetNotRegistered(_) => Status::invalid_argument(err.to_string()),
        SgrError::EvidenceEmitFailed(_) | SgrError::Internal(_) => {
            Status::internal(err.to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// ID helpers
// ---------------------------------------------------------------------------

pub fn unit_id_from_string(value: &str) -> Result<UnitId, Status> {
    UnitId::parse(value)
        .map_err(|err| Status::invalid_argument(format!("invalid unit_id `{value}`: {err}")))
}

// ---------------------------------------------------------------------------
// Enum conversions
// ---------------------------------------------------------------------------

pub const fn unit_kind_to_proto(kind: UnitKind) -> proto::UnitKindProto {
    match kind {
        UnitKind::Service => proto::UnitKindProto::UnitKindService,
        UnitKind::OneShotJob => proto::UnitKindProto::UnitKindOneShotJob,
        UnitKind::Timer => proto::UnitKindProto::UnitKindTimer,
        UnitKind::Mount => proto::UnitKindProto::UnitKindMount,
        UnitKind::Device => proto::UnitKindProto::UnitKindDevice,
        UnitKind::AppSession => proto::UnitKindProto::UnitKindAppSession,
        UnitKind::AgentWorker => proto::UnitKindProto::UnitKindAgentWorker,
        UnitKind::ModelServer => proto::UnitKindProto::UnitKindModelServer,
        UnitKind::RecoveryTask => proto::UnitKindProto::UnitKindRecoveryTask,
        UnitKind::Observer => proto::UnitKindProto::UnitKindObserver,
    }
}

pub fn unit_kind_from_proto(kind: proto::UnitKindProto) -> Result<UnitKind, Status> {
    match kind {
        proto::UnitKindProto::UnitKindService => Ok(UnitKind::Service),
        proto::UnitKindProto::UnitKindOneShotJob => Ok(UnitKind::OneShotJob),
        proto::UnitKindProto::UnitKindTimer => Ok(UnitKind::Timer),
        proto::UnitKindProto::UnitKindMount => Ok(UnitKind::Mount),
        proto::UnitKindProto::UnitKindDevice => Ok(UnitKind::Device),
        proto::UnitKindProto::UnitKindAppSession => Ok(UnitKind::AppSession),
        proto::UnitKindProto::UnitKindAgentWorker => Ok(UnitKind::AgentWorker),
        proto::UnitKindProto::UnitKindModelServer => Ok(UnitKind::ModelServer),
        proto::UnitKindProto::UnitKindRecoveryTask => Ok(UnitKind::RecoveryTask),
        proto::UnitKindProto::UnitKindObserver => Ok(UnitKind::Observer),
        proto::UnitKindProto::UnitKindUnspecified => {
            Err(Status::invalid_argument("unit_kind is unspecified"))
        }
    }
}

pub const fn desired_state_to_proto(state: DesiredState) -> proto::DesiredStateProto {
    match state {
        DesiredState::Running => proto::DesiredStateProto::DesiredStateRunning,
        DesiredState::Stopped => proto::DesiredStateProto::DesiredStateStopped,
        DesiredState::Restarted => proto::DesiredStateProto::DesiredStateRestarted,
        DesiredState::Reloaded => proto::DesiredStateProto::DesiredStateReloaded,
    }
}

pub fn desired_state_from_proto(state: proto::DesiredStateProto) -> Result<DesiredState, Status> {
    match state {
        proto::DesiredStateProto::DesiredStateRunning => Ok(DesiredState::Running),
        proto::DesiredStateProto::DesiredStateStopped => Ok(DesiredState::Stopped),
        proto::DesiredStateProto::DesiredStateRestarted => Ok(DesiredState::Restarted),
        proto::DesiredStateProto::DesiredStateReloaded => Ok(DesiredState::Reloaded),
        proto::DesiredStateProto::DesiredStateUnspecified => {
            Err(Status::invalid_argument("desired_state is unspecified"))
        }
    }
}

pub const fn unit_state_to_proto(state: UnitState) -> proto::UnitStateProto {
    match state {
        UnitState::Draft => proto::UnitStateProto::UnitStateDraft,
        UnitState::Queued => proto::UnitStateProto::UnitStateQueued,
        UnitState::Starting => proto::UnitStateProto::UnitStateStarting,
        UnitState::Running => proto::UnitStateProto::UnitStateRunning,
        UnitState::Healthy => proto::UnitStateProto::UnitStateHealthy,
        UnitState::Degraded => proto::UnitStateProto::UnitStateDegraded,
        UnitState::Unhealthy => proto::UnitStateProto::UnitStateUnhealthy,
        UnitState::Stopping => proto::UnitStateProto::UnitStateStopping,
        UnitState::Stopped => proto::UnitStateProto::UnitStateStopped,
        UnitState::Failed => proto::UnitStateProto::UnitStateFailed,
        UnitState::Retired => proto::UnitStateProto::UnitStateRetired,
    }
}

pub fn unit_state_from_proto(state: proto::UnitStateProto) -> Result<UnitState, Status> {
    match state {
        proto::UnitStateProto::UnitStateDraft => Ok(UnitState::Draft),
        proto::UnitStateProto::UnitStateQueued => Ok(UnitState::Queued),
        proto::UnitStateProto::UnitStateStarting => Ok(UnitState::Starting),
        proto::UnitStateProto::UnitStateRunning => Ok(UnitState::Running),
        proto::UnitStateProto::UnitStateHealthy => Ok(UnitState::Healthy),
        proto::UnitStateProto::UnitStateDegraded => Ok(UnitState::Degraded),
        proto::UnitStateProto::UnitStateUnhealthy => Ok(UnitState::Unhealthy),
        proto::UnitStateProto::UnitStateStopping => Ok(UnitState::Stopping),
        proto::UnitStateProto::UnitStateStopped => Ok(UnitState::Stopped),
        proto::UnitStateProto::UnitStateFailed => Ok(UnitState::Failed),
        proto::UnitStateProto::UnitStateRetired => Ok(UnitState::Retired),
        proto::UnitStateProto::UnitStateUnspecified => {
            Err(Status::invalid_argument("unit_state is unspecified"))
        }
    }
}

pub const fn dependency_kind_to_proto(kind: DependencyKind) -> proto::DependencyKindProto {
    match kind {
        DependencyKind::RequiresHealthy => proto::DependencyKindProto::DependencyRequiresHealthy,
        DependencyKind::RequiresRunning => proto::DependencyKindProto::DependencyRequiresRunning,
        DependencyKind::OrdersAfter => proto::DependencyKindProto::DependencyOrdersAfter,
    }
}

pub fn dependency_kind_from_proto(
    kind: proto::DependencyKindProto,
) -> Result<DependencyKind, Status> {
    match kind {
        proto::DependencyKindProto::DependencyRequiresHealthy => {
            Ok(DependencyKind::RequiresHealthy)
        }
        proto::DependencyKindProto::DependencyRequiresRunning => {
            Ok(DependencyKind::RequiresRunning)
        }
        proto::DependencyKindProto::DependencyOrdersAfter => Ok(DependencyKind::OrdersAfter),
        proto::DependencyKindProto::DependencyKindUnspecified => {
            Err(Status::invalid_argument("dependency kind is unspecified"))
        }
    }
}

pub const fn graph_state_to_proto(state: GraphState) -> proto::GraphStateProto {
    match state {
        GraphState::Empty => proto::GraphStateProto::GraphStateEmpty,
        GraphState::Resolving => proto::GraphStateProto::GraphStateResolving,
        GraphState::Converging => proto::GraphStateProto::GraphStateConverging,
        GraphState::Converged => proto::GraphStateProto::GraphStateConverged,
        GraphState::Degraded => proto::GraphStateProto::GraphStateDegraded,
        GraphState::Failed => proto::GraphStateProto::GraphStateFailed,
    }
}

pub fn graph_state_from_proto(state: proto::GraphStateProto) -> Result<GraphState, Status> {
    match state {
        proto::GraphStateProto::GraphStateEmpty => Ok(GraphState::Empty),
        proto::GraphStateProto::GraphStateResolving => Ok(GraphState::Resolving),
        proto::GraphStateProto::GraphStateConverging => Ok(GraphState::Converging),
        proto::GraphStateProto::GraphStateConverged => Ok(GraphState::Converged),
        proto::GraphStateProto::GraphStateDegraded => Ok(GraphState::Degraded),
        proto::GraphStateProto::GraphStateFailed => Ok(GraphState::Failed),
        proto::GraphStateProto::GraphStateUnspecified => {
            Err(Status::invalid_argument("graph_state is unspecified"))
        }
    }
}

pub const fn restart_policy_to_proto(policy: RestartPolicy) -> proto::RestartPolicyProto {
    match policy {
        RestartPolicy::Never => proto::RestartPolicyProto::RestartPolicyNever,
        RestartPolicy::OnFailure => proto::RestartPolicyProto::RestartPolicyOnFailure,
        RestartPolicy::Always => proto::RestartPolicyProto::RestartPolicyAlways,
        RestartPolicy::UnlessStopped => proto::RestartPolicyProto::RestartPolicyUnlessStopped,
        RestartPolicy::Scheduled => proto::RestartPolicyProto::RestartPolicyScheduled,
    }
}

pub fn restart_policy_from_proto(
    policy: proto::RestartPolicyProto,
) -> Result<RestartPolicy, Status> {
    match policy {
        proto::RestartPolicyProto::RestartPolicyNever => Ok(RestartPolicy::Never),
        proto::RestartPolicyProto::RestartPolicyOnFailure => Ok(RestartPolicy::OnFailure),
        proto::RestartPolicyProto::RestartPolicyAlways => Ok(RestartPolicy::Always),
        proto::RestartPolicyProto::RestartPolicyUnlessStopped => Ok(RestartPolicy::UnlessStopped),
        proto::RestartPolicyProto::RestartPolicyScheduled => Ok(RestartPolicy::Scheduled),
        proto::RestartPolicyProto::RestartPolicyUnspecified => {
            Err(Status::invalid_argument("restart_policy is unspecified"))
        }
    }
}

pub const fn health_check_kind_to_proto(kind: HealthCheckKind) -> proto::HealthCheckKindProto {
    match kind {
        HealthCheckKind::TcpPort => proto::HealthCheckKindProto::HealthCheckTcpPort,
        HealthCheckKind::HttpOk => proto::HealthCheckKindProto::HealthCheckHttpOk,
        HealthCheckKind::CommandExitZero => proto::HealthCheckKindProto::HealthCheckCommandExitZero,
        HealthCheckKind::AiosfsPointerHealthy => {
            proto::HealthCheckKindProto::HealthCheckAiosfsPointerHealthy
        }
        HealthCheckKind::CustomProbe => proto::HealthCheckKindProto::HealthCheckCustomProbe,
    }
}

pub fn health_check_kind_from_proto(
    kind: proto::HealthCheckKindProto,
) -> Result<HealthCheckKind, Status> {
    match kind {
        proto::HealthCheckKindProto::HealthCheckTcpPort => Ok(HealthCheckKind::TcpPort),
        proto::HealthCheckKindProto::HealthCheckHttpOk => Ok(HealthCheckKind::HttpOk),
        proto::HealthCheckKindProto::HealthCheckCommandExitZero => {
            Ok(HealthCheckKind::CommandExitZero)
        }
        proto::HealthCheckKindProto::HealthCheckAiosfsPointerHealthy => {
            Ok(HealthCheckKind::AiosfsPointerHealthy)
        }
        proto::HealthCheckKindProto::HealthCheckCustomProbe => Ok(HealthCheckKind::CustomProbe),
        proto::HealthCheckKindProto::HealthCheckKindUnspecified => {
            Err(Status::invalid_argument("health_check.kind is unspecified"))
        }
    }
}

pub const fn rollback_trigger_to_proto(trigger: RollbackTrigger) -> proto::RollbackTriggerProto {
    match trigger {
        RollbackTrigger::OnStartupFailure => proto::RollbackTriggerProto::RollbackOnStartupFailure,
        RollbackTrigger::OnHealthFailure => proto::RollbackTriggerProto::RollbackOnHealthFailure,
        RollbackTrigger::OnOperatorRequest => {
            proto::RollbackTriggerProto::RollbackOnOperatorRequest
        }
        RollbackTrigger::Never => proto::RollbackTriggerProto::RollbackNever,
    }
}

pub fn rollback_trigger_from_proto(
    trigger: proto::RollbackTriggerProto,
) -> Result<RollbackTrigger, Status> {
    match trigger {
        proto::RollbackTriggerProto::RollbackOnStartupFailure => {
            Ok(RollbackTrigger::OnStartupFailure)
        }
        proto::RollbackTriggerProto::RollbackOnHealthFailure => {
            Ok(RollbackTrigger::OnHealthFailure)
        }
        proto::RollbackTriggerProto::RollbackOnOperatorRequest => {
            Ok(RollbackTrigger::OnOperatorRequest)
        }
        proto::RollbackTriggerProto::RollbackNever => Ok(RollbackTrigger::Never),
        proto::RollbackTriggerProto::RollbackTriggerUnspecified => {
            Err(Status::invalid_argument("rollback trigger is unspecified"))
        }
    }
}

pub const fn adapter_registration_state_to_proto(
    state: AdapterRegistrationState,
) -> proto::AdapterRegistrationStateProto {
    match state {
        AdapterRegistrationState::Pending => {
            proto::AdapterRegistrationStateProto::AdapterRegistrationPending
        }
        AdapterRegistrationState::Active => {
            proto::AdapterRegistrationStateProto::AdapterRegistrationActive
        }
        AdapterRegistrationState::Suspended => {
            proto::AdapterRegistrationStateProto::AdapterRegistrationSuspended
        }
        AdapterRegistrationState::Retired => {
            proto::AdapterRegistrationStateProto::AdapterRegistrationRetired
        }
    }
}

pub fn adapter_registration_state_from_proto(
    state: proto::AdapterRegistrationStateProto,
) -> Result<AdapterRegistrationState, Status> {
    match state {
        proto::AdapterRegistrationStateProto::AdapterRegistrationPending => {
            Ok(AdapterRegistrationState::Pending)
        }
        proto::AdapterRegistrationStateProto::AdapterRegistrationActive => {
            Ok(AdapterRegistrationState::Active)
        }
        proto::AdapterRegistrationStateProto::AdapterRegistrationSuspended => {
            Ok(AdapterRegistrationState::Suspended)
        }
        proto::AdapterRegistrationStateProto::AdapterRegistrationRetired => {
            Ok(AdapterRegistrationState::Retired)
        }
        proto::AdapterRegistrationStateProto::AdapterRegistrationStateUnspecified => Err(
            Status::invalid_argument("adapter registration state is unspecified"),
        ),
    }
}

// ---------------------------------------------------------------------------
// Struct conversions
// ---------------------------------------------------------------------------

pub fn unit_dependency_to_proto(dependency: &UnitDependency) -> proto::UnitDependencyProto {
    proto::UnitDependencyProto {
        unit_id: dependency.unit_id.to_string(),
        kind: i32::from(dependency_kind_to_proto(dependency.kind)),
    }
}

pub fn unit_dependency_from_proto(
    dependency: &proto::UnitDependencyProto,
) -> Result<UnitDependency, Status> {
    let kind = dependency_kind_from_proto(dependency.kind())?;
    Ok(UnitDependency {
        unit_id: unit_id_from_string(&dependency.unit_id)?,
        kind,
    })
}

pub fn verification_intent_to_proto(
    intent: &VerificationIntentRef,
) -> proto::VerificationIntentRefProto {
    proto::VerificationIntentRefProto {
        intent_type: intent.type_.clone(),
        args: Some(json_to_prost_value(&intent.args)),
    }
}

pub fn verification_intent_from_proto(
    intent: proto::VerificationIntentRefProto,
) -> VerificationIntentRef {
    VerificationIntentRef {
        type_: intent.intent_type,
        args: optional_prost_value_to_json(intent.args.as_ref()),
    }
}

pub fn rollback_pointer_to_proto(pointer: &RollbackPointer) -> proto::RollbackPointerProto {
    proto::RollbackPointerProto {
        aiosfs_pointer_id: pointer.aiosfs_pointer_id.clone(),
        expected_current_version_id: pointer.expected_current_version_id.clone(),
        trigger: i32::from(rollback_trigger_to_proto(pointer.trigger)),
    }
}

pub fn rollback_pointer_from_proto(
    pointer: proto::RollbackPointerProto,
) -> Result<RollbackPointer, Status> {
    let trigger = rollback_trigger_from_proto(pointer.trigger())?;
    Ok(RollbackPointer {
        aiosfs_pointer_id: pointer.aiosfs_pointer_id,
        expected_current_version_id: pointer.expected_current_version_id,
        trigger,
    })
}

pub const fn gpu_budget_to_proto(gpu: &GpuBudget) -> proto::GpuBudgetProto {
    proto::GpuBudgetProto {
        requires_compute: gpu.requires_compute,
        vram_bytes_max: gpu.vram_bytes_max,
    }
}

pub const fn gpu_budget_from_proto(gpu: proto::GpuBudgetProto) -> GpuBudget {
    GpuBudget {
        requires_compute: gpu.requires_compute,
        vram_bytes_max: gpu.vram_bytes_max,
    }
}

pub fn resource_budget_to_proto(budget: &ResourceBudget) -> proto::ResourceBudgetProto {
    proto::ResourceBudgetProto {
        memory_bytes_max: budget.memory_bytes_max,
        cpu_quota_cores: budget.cpu_quota_cores,
        disk_bytes_max: budget.disk_bytes_max,
        file_descriptors_max: budget.file_descriptors_max,
        process_count_max: budget.process_count_max,
        queue_depth_max: budget.queue_depth_max,
        gpu: budget.gpu.as_ref().map(gpu_budget_to_proto),
    }
}

pub const fn resource_budget_from_proto(budget: proto::ResourceBudgetProto) -> ResourceBudget {
    ResourceBudget {
        memory_bytes_max: budget.memory_bytes_max,
        cpu_quota_cores: budget.cpu_quota_cores,
        disk_bytes_max: budget.disk_bytes_max,
        file_descriptors_max: budget.file_descriptors_max,
        process_count_max: budget.process_count_max,
        queue_depth_max: budget.queue_depth_max,
        gpu: match budget.gpu {
            Some(gpu) => Some(gpu_budget_from_proto(gpu)),
            None => None,
        },
    }
}

pub const fn restart_budget_to_proto(budget: &RestartBudget) -> proto::RestartBudgetProto {
    proto::RestartBudgetProto {
        max_attempts: budget.max_attempts,
        reset_window_seconds: budget.reset_window_seconds,
        backoff_initial_seconds: budget.backoff_initial_seconds,
        backoff_max_seconds: budget.backoff_max_seconds,
    }
}

pub const fn restart_budget_from_proto(budget: proto::RestartBudgetProto) -> RestartBudget {
    RestartBudget {
        max_attempts: budget.max_attempts,
        reset_window_seconds: budget.reset_window_seconds,
        backoff_initial_seconds: budget.backoff_initial_seconds,
        backoff_max_seconds: budget.backoff_max_seconds,
    }
}

pub fn health_check_to_proto(health_check: &HealthCheckSpec) -> proto::HealthCheckSpecProto {
    proto::HealthCheckSpecProto {
        kind: i32::from(health_check_kind_to_proto(health_check.kind)),
        probe_interval_seconds: health_check.probe_interval_seconds,
        probe_timeout_seconds: health_check.probe_timeout_seconds,
        startup_grace_seconds: health_check.startup_grace_seconds,
        unhealthy_threshold: health_check.unhealthy_threshold,
        healthy_threshold: health_check.healthy_threshold,
        args: Some(json_to_prost_value(&health_check.args)),
    }
}

pub fn health_check_from_proto(
    health_check: &proto::HealthCheckSpecProto,
) -> Result<HealthCheckSpec, Status> {
    Ok(HealthCheckSpec {
        kind: health_check_kind_from_proto(health_check.kind())?,
        probe_interval_seconds: health_check.probe_interval_seconds,
        probe_timeout_seconds: health_check.probe_timeout_seconds,
        startup_grace_seconds: health_check.startup_grace_seconds,
        unhealthy_threshold: health_check.unhealthy_threshold,
        healthy_threshold: health_check.healthy_threshold,
        args: optional_prost_value_to_json(health_check.args.as_ref()),
    })
}

pub fn unit_manifest_to_proto(manifest: &UnitManifest) -> proto::UnitManifestProto {
    proto::UnitManifestProto {
        schema_version: manifest.schema_version.clone(),
        unit_id: manifest.unit_id.to_string(),
        unit_kind: i32::from(unit_kind_to_proto(manifest.unit_kind)),
        display_name: manifest.display_name.clone(),
        description: manifest.description.clone(),
        issued_at: Some(datetime_to_proto(manifest.issued_at)),
        publisher_id: manifest.publisher_id.clone(),
        publisher_root_id: manifest.publisher_root_id.clone(),
        publisher_signature: manifest.publisher_signature.clone(),
        canonical_hash: manifest.canonical_hash.clone(),
        dependencies: manifest
            .dependencies
            .iter()
            .map(unit_dependency_to_proto)
            .collect(),
        sandbox_profile_ref: manifest.sandbox_profile_ref.clone(),
        verification_intent: manifest
            .verification_intent
            .iter()
            .map(verification_intent_to_proto)
            .collect(),
        rollback_pointer: Some(rollback_pointer_to_proto(&manifest.rollback_pointer)),
        resource_budget: Some(resource_budget_to_proto(&manifest.resource_budget)),
        restart_policy: i32::from(restart_policy_to_proto(manifest.restart_policy)),
        restart_budget: Some(restart_budget_to_proto(&manifest.restart_budget)),
        health_check: Some(health_check_to_proto(&manifest.health_check)),
        startup_deadline_seconds: manifest.startup_deadline_seconds,
        stop_deadline_seconds: manifest.stop_deadline_seconds,
        adapter_target: Some(json_to_prost_value(&manifest.adapter_target)),
        labels: manifest.labels.as_ref().map(json_to_prost_value),
        correlation_id: manifest.correlation_id.clone(),
        desired_state: i32::from(desired_state_to_proto(manifest.desired_state)),
        provides: manifest.provides.clone(),
        adapter_id: manifest.adapter_id.clone(),
    }
}

pub fn unit_manifest_from_proto(
    manifest: proto::UnitManifestProto,
) -> Result<UnitManifest, Status> {
    let unit_kind = unit_kind_from_proto(manifest.unit_kind())?;
    let restart_policy = restart_policy_from_proto(manifest.restart_policy())?;
    let desired_state = desired_state_from_proto(manifest.desired_state())?;
    let issued_at = required_datetime_from_proto(manifest.issued_at, "issued_at")?;
    let adapter_target = optional_prost_value_to_json(manifest.adapter_target.as_ref());
    let labels = manifest
        .labels
        .as_ref()
        .map(prost_value_to_json)
        .filter(|value| !value.is_null());
    let rollback_pointer = rollback_pointer_from_proto(
        manifest
            .rollback_pointer
            .ok_or_else(|| Status::invalid_argument("rollback_pointer is required"))?,
    )?;
    let resource_budget = resource_budget_from_proto(
        manifest
            .resource_budget
            .ok_or_else(|| Status::invalid_argument("resource_budget is required"))?,
    );
    let restart_budget = restart_budget_from_proto(
        manifest
            .restart_budget
            .ok_or_else(|| Status::invalid_argument("restart_budget is required"))?,
    );
    let health_check_proto = manifest
        .health_check
        .ok_or_else(|| Status::invalid_argument("health_check is required"))?;
    let health_check = health_check_from_proto(&health_check_proto)?;
    let dependencies = manifest
        .dependencies
        .iter()
        .map(unit_dependency_from_proto)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(UnitManifest {
        schema_version: manifest.schema_version,
        unit_id: unit_id_from_string(&manifest.unit_id)?,
        unit_kind,
        display_name: manifest.display_name,
        description: manifest.description,
        issued_at,
        publisher_id: manifest.publisher_id,
        publisher_root_id: manifest.publisher_root_id,
        publisher_signature: manifest.publisher_signature,
        canonical_hash: manifest.canonical_hash,
        dependencies,
        sandbox_profile_ref: manifest.sandbox_profile_ref,
        verification_intent: manifest
            .verification_intent
            .into_iter()
            .map(verification_intent_from_proto)
            .collect(),
        rollback_pointer,
        resource_budget,
        restart_policy,
        restart_budget,
        health_check,
        startup_deadline_seconds: manifest.startup_deadline_seconds,
        stop_deadline_seconds: manifest.stop_deadline_seconds,
        adapter_target,
        labels,
        correlation_id: manifest.correlation_id,
        desired_state,
        provides: manifest.provides,
        adapter_id: manifest.adapter_id,
    })
}

pub fn service_unit_to_proto(unit: &ServiceUnit) -> proto::ServiceUnitProto {
    proto::ServiceUnitProto {
        unit_id: unit.unit_id.to_string(),
        manifest: Some(unit_manifest_to_proto(&unit.manifest)),
        state: i32::from(unit_state_to_proto(unit.state)),
        last_transition_at: Some(datetime_to_proto(unit.last_transition_at)),
        evidence_chain: unit.evidence_chain.clone(),
    }
}

pub fn service_unit_from_proto(unit: proto::ServiceUnitProto) -> Result<ServiceUnit, Status> {
    let state = unit_state_from_proto(unit.state())?;
    Ok(ServiceUnit {
        unit_id: unit_id_from_string(&unit.unit_id)?,
        manifest: unit_manifest_from_proto(
            unit.manifest
                .ok_or_else(|| Status::invalid_argument("manifest is required"))?,
        )?,
        state,
        last_transition_at: required_datetime_from_proto(
            unit.last_transition_at,
            "last_transition_at",
        )?,
        evidence_chain: unit.evidence_chain,
    })
}

pub fn dependency_edge_to_proto(edge: &DependencyEdge) -> proto::DependencyEdgeProto {
    proto::DependencyEdgeProto {
        from_unit_id: edge.from_unit_id.to_string(),
        to_unit_id: edge.to_unit_id.to_string(),
        kind: i32::from(dependency_kind_to_proto(edge.kind)),
    }
}

pub fn dependency_edge_from_proto(
    edge: &proto::DependencyEdgeProto,
) -> Result<DependencyEdge, Status> {
    let kind = dependency_kind_from_proto(edge.kind())?;
    Ok(DependencyEdge {
        from_unit_id: unit_id_from_string(&edge.from_unit_id)?,
        to_unit_id: unit_id_from_string(&edge.to_unit_id)?,
        kind,
    })
}

pub fn adapter_capability_to_proto(
    capability: &AdapterCapability,
) -> proto::AdapterCapabilityProto {
    proto::AdapterCapabilityProto {
        capability_id: capability.capability_id.clone(),
        provides: capability.provides.clone(),
        requires: capability.requires.clone(),
        risk_template: capability.risk_template.clone(),
        manifest_signature_ed25519: capability.manifest_signature_ed25519.clone(),
    }
}

pub fn adapter_capability_from_proto(
    capability: proto::AdapterCapabilityProto,
) -> Result<AdapterCapability, Status> {
    if capability.capability_id.trim().is_empty() {
        return Err(Status::invalid_argument("capability_id is required"));
    }
    Ok(AdapterCapability {
        capability_id: capability.capability_id,
        provides: capability.provides,
        requires: capability.requires,
        risk_template: capability.risk_template,
        manifest_signature_ed25519: capability.manifest_signature_ed25519,
    })
}

pub fn adapter_declaration_to_json(declaration: &AdapterDeclaration) -> Result<Vec<u8>, Status> {
    serde_json::to_vec(declaration)
        .map_err(|err| Status::internal(format!("adapter declaration serialise: {err}")))
}

pub fn adapter_declaration_from_json(bytes: &[u8]) -> Result<AdapterDeclaration, Status> {
    if bytes.is_empty() {
        return Err(Status::invalid_argument("declaration_json is required"));
    }
    serde_json::from_slice(bytes)
        .map_err(|err| Status::invalid_argument(format!("invalid declaration_json: {err}")))
}

pub fn registered_adapter_to_proto(
    adapter: &RegisteredAdapter,
) -> Result<proto::RegisteredAdapterProto, Status> {
    Ok(proto::RegisteredAdapterProto {
        capability: Some(adapter_capability_to_proto(&adapter.capability)),
        declaration_json: adapter_declaration_to_json(&adapter.declaration)?,
        registered_at: Some(datetime_to_proto(adapter.registered_at)),
        state: i32::from(adapter_registration_state_to_proto(adapter.state)),
    })
}

pub fn registered_adapter_from_proto(
    adapter: proto::RegisteredAdapterProto,
) -> Result<RegisteredAdapter, Status> {
    let state = adapter_registration_state_from_proto(adapter.state())?;
    Ok(RegisteredAdapter {
        capability: adapter_capability_from_proto(
            adapter
                .capability
                .ok_or_else(|| Status::invalid_argument("capability is required"))?,
        )?,
        declaration: adapter_declaration_from_json(&adapter.declaration_json)?,
        registered_at: required_datetime_from_proto(adapter.registered_at, "registered_at")?,
        state,
    })
}
