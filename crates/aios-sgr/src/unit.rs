//! S15.1 unit manifest types.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};
use ulid::Ulid;

use crate::dependency::UnitDependency;
use crate::state::UnitState;

/// Stable S15.1 unit identifier.
///
/// The Rev.2 manifest spec uses the canonical wire shape
/// `unit:<vendor>:<name>[:<variant>]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnitId(String);

impl UnitId {
    /// Canonical unit id prefix.
    pub const PREFIX: &'static str = "unit:";

    /// Validate and adopt an externally supplied S15.1 unit id.
    ///
    /// # Errors
    ///
    /// Returns a string error when the id is empty, has the wrong prefix, has
    /// the wrong number of colon-separated fields, or contains a segment that
    /// is outside the S15.1 character and length bounds.
    pub fn parse(input: &str) -> Result<Self, String> {
        validate_unit_id(input).map(Self)
    }

    /// Mint a deterministic-shape local unit id under a publisher vendor.
    ///
    /// The generated name token is `unit_<lowercase-ulid>`, embedded inside
    /// the S15.1 `unit:<vendor>:<name>` namespace.
    ///
    /// # Errors
    ///
    /// Returns a string error when `vendor` is not a valid S15.1 vendor token.
    pub fn generated_for_vendor(vendor: &str) -> Result<Self, String> {
        let ulid = Ulid::new().to_string().to_ascii_lowercase();
        let name = format!("unit_{ulid}");
        Self::from_parts(vendor, &name, None)
    }

    /// Build an id from validated S15.1 segments.
    ///
    /// # Errors
    ///
    /// Returns a string error when any segment violates S15.1 token rules.
    pub fn from_parts(vendor: &str, name: &str, variant: Option<&str>) -> Result<Self, String> {
        validate_vendor(vendor)?;
        validate_name_like(name, "name")?;
        if let Some(value) = variant {
            validate_name_like(value, "variant")?;
            return Ok(Self(format!(
                "{}{}:{}:{}",
                Self::PREFIX,
                vendor,
                name,
                value
            )));
        }

        Ok(Self(format!("{}{}:{}", Self::PREFIX, vendor, name)))
    }

    /// Borrow the canonical wire string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UnitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for UnitId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

fn validate_unit_id(input: &str) -> Result<String, String> {
    if input.is_empty() {
        return Err("unit id is empty".to_owned());
    }

    let Some(rest) = input.strip_prefix(UnitId::PREFIX) else {
        return Err(format!("expected prefix {}, got {input}", UnitId::PREFIX));
    };

    let parts = rest.split(':').collect::<Vec<_>>();
    match parts.as_slice() {
        [vendor, name] => {
            validate_vendor(vendor)?;
            validate_name_like(name, "name")?;
        }
        [vendor, name, variant] => {
            validate_vendor(vendor)?;
            validate_name_like(name, "name")?;
            validate_name_like(variant, "variant")?;
        }
        _ => {
            return Err(format!(
                "unit id must be unit:<vendor>:<name>[:<variant>], got {input}"
            ));
        }
    }

    Ok(input.to_owned())
}

fn validate_vendor(input: &str) -> Result<(), String> {
    if input.is_empty() {
        return Err("vendor segment is empty".to_owned());
    }
    if input.len() > 63 {
        return Err("vendor segment exceeds 63 characters".to_owned());
    }
    if input.starts_with('-') || input.ends_with('-') {
        return Err("vendor segment cannot start or end with '-'".to_owned());
    }
    if !input
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(format!(
            "vendor segment contains invalid characters: {input}"
        ));
    }

    Ok(())
}

fn validate_name_like(input: &str, label: &str) -> Result<(), String> {
    if input.is_empty() {
        return Err(format!("{label} segment is empty"));
    }
    if input.len() > 63 {
        return Err(format!("{label} segment exceeds 63 characters"));
    }
    if !input
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(format!(
            "{label} segment contains invalid characters: {input}"
        ));
    }

    Ok(())
}

/// S15.1 closed `UnitKind` enum, ten values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UnitKind {
    /// `SERVICE` - long-running daemon.
    Service,
    /// `ONE_SHOT_JOB` - single-execution job that runs to completion.
    OneShotJob,
    /// `TIMER` - periodic trigger.
    Timer,
    /// `MOUNT` - filesystem mount or AIOS-FS namespace projection.
    Mount,
    /// `DEVICE` - hardware device binding.
    Device,
    /// `APP_SESSION` - per-user app instance.
    AppSession,
    /// `AGENT_WORKER` - subject-bearing AI agent worker process.
    AgentWorker,
    /// `MODEL_SERVER` - local inference server.
    ModelServer,
    /// `RECOVERY_TASK` - recovery-mode-only task.
    RecoveryTask,
    /// `OBSERVER` - passive observer.
    Observer,
}

/// T-084 desired-state request vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DesiredState {
    /// `RUNNING` - converge the unit to a live state.
    Running,
    /// `STOPPED` - converge the unit to a stopped state.
    Stopped,
    /// `RESTARTED` - request a restart transition.
    Restarted,
    /// `RELOADED` - request an in-place reload or reconfigure transition.
    Reloaded,
}

/// S15.1 closed `RestartPolicy` enum, five values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RestartPolicy {
    /// `NEVER` - never restart automatically.
    Never,
    /// `ON_FAILURE` - restart only on unexpected failure.
    OnFailure,
    /// `ALWAYS` - restart on every exit.
    Always,
    /// `UNLESS_STOPPED` - restart unless the operator explicitly stopped it.
    UnlessStopped,
    /// `SCHEDULED` - restart only on a timer cadence.
    Scheduled,
}

/// S15.1 restart budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestartBudget {
    /// Maximum attempts within the reset window.
    pub max_attempts: u32,
    /// Window after which the attempt counter resets.
    pub reset_window_seconds: u32,
    /// Initial restart backoff.
    pub backoff_initial_seconds: u32,
    /// Maximum restart backoff.
    pub backoff_max_seconds: u32,
}

/// S15.1 closed `HealthCheckKind` enum, five values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HealthCheckKind {
    /// `TCP_PORT` - connect to a declared local TCP port.
    TcpPort,
    /// `HTTP_OK` - issue an HTTP health request and require a 2xx response.
    HttpOk,
    /// `COMMAND_EXIT_ZERO` - typed adapter probe exits successfully.
    CommandExitZero,
    /// `AIOSFS_POINTER_HEALTHY` - AIOS-FS pointer resolves to a live version.
    AiosfsPointerHealthy,
    /// `CUSTOM_PROBE` - registered closed-schema probe.
    CustomProbe,
}

/// S15.1 health-check cadence and argument payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthCheckSpec {
    /// Closed health-check kind.
    pub kind: HealthCheckKind,
    /// Probe interval in seconds.
    pub probe_interval_seconds: u32,
    /// Probe timeout in seconds.
    pub probe_timeout_seconds: u32,
    /// Startup grace window in seconds.
    pub startup_grace_seconds: u32,
    /// Consecutive failures before unhealthy.
    pub unhealthy_threshold: u32,
    /// Consecutive successes before healthy.
    pub healthy_threshold: u32,
    /// Typed kind-specific probe arguments.
    pub args: serde_json::Value,
}

/// S15.1 verification intent reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationIntentRef {
    /// S2.4 intent type token.
    #[serde(rename = "type")]
    pub type_: String,
    /// Typed intent arguments.
    pub args: serde_json::Value,
}

/// S15.1 rollback trigger vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RollbackTrigger {
    /// `ON_STARTUP_FAILURE` - rollback when startup fails.
    OnStartupFailure,
    /// `ON_HEALTH_FAILURE` - rollback when health failure exhausts budget.
    OnHealthFailure,
    /// `ON_OPERATOR_REQUEST` - rollback only by operator request.
    OnOperatorRequest,
    /// `NEVER` - no rollback path.
    Never,
}

/// S15.1 rollback pointer binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollbackPointer {
    /// AIOS-FS pointer id or path projection.
    pub aiosfs_pointer_id: String,
    /// Expected current version id for CAS.
    pub expected_current_version_id: String,
    /// Trigger that dispatches rollback.
    pub trigger: RollbackTrigger,
}

/// Optional GPU budget sub-record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GpuBudget {
    /// Whether the unit requires GPU compute capability.
    pub requires_compute: bool,
    /// Maximum VRAM bytes.
    pub vram_bytes_max: u64,
}

/// S15.1 resource budget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceBudget {
    /// Maximum memory bytes.
    pub memory_bytes_max: u64,
    /// CPU quota in fractional cores.
    pub cpu_quota_cores: f64,
    /// Maximum ephemeral disk bytes.
    pub disk_bytes_max: u64,
    /// Maximum file descriptors.
    pub file_descriptors_max: u32,
    /// Maximum process count.
    pub process_count_max: u32,
    /// Maximum inbound queue depth.
    pub queue_depth_max: u32,
    /// Optional GPU budget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu: Option<GpuBudget>,
}

/// S15.1 `UnitManifest` plus the T-084 desired-state projection fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitManifest {
    /// Schema version, normally `aios.unit.v1alpha1`.
    pub schema_version: String,
    /// Stable unit id.
    pub unit_id: UnitId,
    /// Closed unit kind.
    pub unit_kind: UnitKind,
    /// Human-readable display name.
    pub display_name: String,
    /// Human-readable description.
    pub description: String,
    /// Manifest issuance timestamp.
    pub issued_at: DateTime<Utc>,
    /// Publisher id.
    pub publisher_id: String,
    /// Publisher root id.
    pub publisher_root_id: String,
    /// Ed25519 publisher signature bytes.
    pub publisher_signature: Vec<u8>,
    /// Truncated canonical BLAKE3 hash.
    pub canonical_hash: String,
    /// Unit dependencies.
    pub dependencies: Vec<UnitDependency>,
    /// Sandbox profile reference.
    pub sandbox_profile_ref: String,
    /// Verification intent references.
    pub verification_intent: Vec<VerificationIntentRef>,
    /// Rollback pointer binding.
    pub rollback_pointer: RollbackPointer,
    /// Resource budget.
    pub resource_budget: ResourceBudget,
    /// Restart policy.
    pub restart_policy: RestartPolicy,
    /// Restart budget.
    pub restart_budget: RestartBudget,
    /// Health-check spec.
    pub health_check: HealthCheckSpec,
    /// Startup deadline in seconds.
    pub startup_deadline_seconds: u32,
    /// Stop deadline in seconds.
    pub stop_deadline_seconds: u32,
    /// Adapter target typed payload.
    pub adapter_target: serde_json::Value,
    /// Optional bounded labels payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<serde_json::Value>,
    /// Optional batch correlation id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// Desired runtime state requested by the active graph.
    pub desired_state: DesiredState,
    /// Capabilities provided by this unit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provides: Vec<String>,
    /// Optional SGR-side adapter declaration id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_id: Option<String>,
}

/// Runtime view of a unit manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceUnit {
    /// Stable unit id.
    pub unit_id: UnitId,
    /// Immutable manifest snapshot.
    pub manifest: UnitManifest,
    /// Current S15.1 runtime state.
    pub state: UnitState,
    /// Timestamp of the last state transition.
    pub last_transition_at: DateTime<Utc>,
    /// Evidence receipt ids linked to this unit lifecycle.
    pub evidence_chain: Vec<String>,
}
