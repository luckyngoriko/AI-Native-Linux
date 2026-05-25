//! S15.3 adapter declaration types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// S15.3 closed `AdapterRegistrationState` enum, six values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdapterRegistrationState {
    /// `DRAFT` - candidate manifest submitted.
    Draft,
    /// `VALIDATING` - admission checks are in flight.
    Validating,
    /// `REGISTERED` - manifest is sealed and dispatchable.
    Registered,
    /// `DEGRADED` - registered but health signal crossed a threshold.
    Degraded,
    /// `DEREGISTERED` - removed from the live directory.
    Deregistered,
    /// `RETIRED` - terminal forensic state.
    Retired,
}

/// S15.3 closed `AdapterCapabilityClass` enum, ten values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdapterCapabilityClass {
    /// `FILESYSTEM_WRITE` - mutates filesystem state.
    FilesystemWrite,
    /// `FILESYSTEM_READ` - reads filesystem state.
    FilesystemRead,
    /// `SERVICE_LIFECYCLE` - starts, stops, restarts, or reloads services.
    ServiceLifecycle,
    /// `NETWORK_OUTBOUND` - initiates outbound network connections.
    NetworkOutbound,
    /// `VAULT_OPERATION` - uses Vault Broker operations.
    VaultOperation,
    /// `GPU_RENDER` - uses GPU rendering.
    GpuRender,
    /// `GPU_COMPUTE` - uses GPU compute.
    GpuCompute,
    /// `EXTERNAL_API_CALL` - calls an external HTTPS API.
    ExternalApiCall,
    /// `EVIDENCE_EMIT` - emits adapter-owned evidence records.
    EvidenceEmit,
    /// `SCHEDULER_PRIVILEGED` - requests privileged scheduler operations.
    SchedulerPrivileged,
}

/// S15.3 closed `AdapterIOMode` enum, two values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdapterIOMode {
    /// `TYPED_PARAMETERS_ONLY` - accepts typed target parameters only.
    TypedParametersOnly,
    /// `TEMPLATE_PARAMETERS` - accepts a closed template with typed variables.
    TemplateParameters,
}

/// S15.3 closed `AdapterDispatchKind` enum, four values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdapterDispatchKind {
    /// `IN_PROCESS_RPC` - in-process handler.
    InProcessRpc,
    /// `SUBPROCESS_FORK` - per-action subprocess.
    SubprocessFork,
    /// `ISOLATED_SANDBOX` - full sandbox dispatch.
    IsolatedSandbox,
    /// `DRY_RUN` - simulation-only dispatch.
    DryRun,
}

/// S15.3 local mirror of the S10.1 adapter stability ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdapterStability {
    /// `REGISTERED` - admitted but not promoted.
    Registered,
    /// `EXPERIMENTAL` - functional but not hardened.
    Experimental,
    /// `STABLE` - hardened and eligible for strongest fast path.
    Stable,
    /// `DEPRECATED` - still accepted but discouraged.
    Deprecated,
    /// `RETIRED` - no new dispatches accepted.
    Retired,
}

/// S15.3 closed `AdapterFailureMode` enum, ten values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdapterFailureMode {
    /// `SANDBOX_APPLICATION_FAILED` - sandbox could not be applied.
    SandboxApplicationFailed,
    /// `ADAPTER_TIMEOUT` - adapter exceeded its timeout.
    AdapterTimeout,
    /// `ADAPTER_PANIC` - adapter crashed or panicked.
    AdapterPanic,
    /// `RESOURCE_BUDGET_EXCEEDED` - resource budget was exceeded.
    ResourceBudgetExceeded,
    /// `DEPENDENCY_UNREADY` - adapter dependency was not ready.
    DependencyUnready,
    /// `BACKEND_UNAVAILABLE` - external backend was unreachable.
    BackendUnavailable,
    /// `ROLLBACK_PRECONDITION_FAILED` - rollback precondition failed.
    RollbackPreconditionFailed,
    /// `BINDING_EXPIRED` - approval or override binding expired.
    BindingExpired,
    /// `ADAPTER_REFUSED` - adapter explicitly refused.
    AdapterRefused,
    /// `KIND_OR_CAPABILITY_OVERRUN` - adapter exceeded its declaration.
    KindOrCapabilityOverrun,
}

/// Local SGR-side mirror of S10.1 `RollbackStrategy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdapterRollbackStrategy {
    /// `NONE` - no rollback path.
    None,
    /// `IDEMPOTENT_REVERSE` - reverse action can be computed.
    IdempotentReverse,
    /// `CHECKPOINT_BASED` - restore from checkpoint.
    CheckpointBased,
    /// `EXTERNAL_REQUIRED` - operator intervention is required.
    ExternalRequired,
    /// `ROLLBACK_STRATEGY_UNSPECIFIED` - proto-compatible sentinel.
    #[serde(rename = "ROLLBACK_STRATEGY_UNSPECIFIED")]
    Unspecified,
}

/// Per-action declaration nested inside an S15.3 adapter manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterActionDeclaration {
    /// Dotted L5 action kind.
    pub action_kind: String,
    /// JSON-schema-shaped target schema.
    pub target_schema: serde_json::Value,
    /// JSON-schema-shaped response schema.
    pub response_schema: serde_json::Value,
    /// Rollback strategy for this action.
    pub rollback_strategy: AdapterRollbackStrategy,
    /// Per-action timeout in seconds.
    pub timeout_seconds: u32,
    /// Optional template string for template-parameter mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_string: Option<String>,
    /// Closed substitution variables allowed by `template_string`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub template_substitution_variables: Vec<String>,
    /// Per-action capability overlay.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub per_action_capabilities: Vec<AdapterCapabilityClass>,
}

/// S15.3 author-facing adapter manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterManifest {
    /// Canonical `adapter:<vendor>:<name>:<version>` id.
    pub adapter_id: String,
    /// Vendor token.
    pub vendor: String,
    /// Adapter name token.
    pub name: String,
    /// `SemVer` adapter version.
    pub adapter_version: String,
    /// Adapter spec version, normally `v1alpha1`.
    pub spec_version: String,
    /// Declared action kinds and schemas.
    pub declared_actions: Vec<AdapterActionDeclaration>,
    /// Manifest-level capability classes.
    pub declared_capabilities: Vec<AdapterCapabilityClass>,
    /// L0 invariant ids the adapter declares support for.
    pub declared_invariants_supported: Vec<String>,
    /// Input/output posture.
    pub io_mode: AdapterIOMode,
    /// Preferred dispatch kind.
    pub preferred_dispatch_kind: AdapterDispatchKind,
    /// Maximum declared stability.
    pub declared_stability: AdapterStability,
    /// S3.2 sandbox profile minimum, kept opaque until sandbox crate lands.
    pub sandbox_profile_minimum: serde_json::Value,
    /// Declared failure modes.
    pub declared_failure_modes: Vec<AdapterFailureMode>,
    /// Default adapter timeout in seconds.
    pub default_adapter_timeout_seconds: u32,
    /// Default rollback timeout in seconds.
    pub default_rollback_timeout_seconds: u32,
    /// Enumerated outbound network hosts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub network_outbound_hosts: Vec<String>,
    /// Enumerated external HTTPS API hosts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_api_hosts: Vec<String>,
    /// Evidence record types the adapter may emit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub declared_evidence_record_types: Vec<String>,
    /// S11.1 source package id.
    pub source_package_id: String,
    /// Publisher catalog root id.
    pub publisher_root_id: String,
    /// Ed25519 manifest signature bytes.
    pub manifest_signature: Vec<u8>,
    /// Signing key id.
    pub signing_key_id: String,
    /// Manifest creation timestamp.
    pub manifest_created_at: DateTime<Utc>,
    /// Manifest expiry timestamp.
    pub manifest_expires_at: DateTime<Utc>,
}

/// SGR-side capability declaration summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterCapability {
    /// Capability declaration id.
    pub capability_id: String,
    /// Capability names or action kinds provided by this adapter.
    pub provides: Vec<String>,
    /// Capability classes or prerequisites required by this adapter.
    pub requires: Vec<String>,
    /// Risk-template id applied by policy/runtime handoff.
    pub risk_template: String,
    /// Ed25519 signature over the capability declaration manifest.
    pub manifest_signature_ed25519: Vec<u8>,
}

/// SGR-side adapter declaration payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "declaration_type",
    content = "payload",
    rename_all = "SCREAMING_SNAKE_CASE"
)]
pub enum AdapterDeclaration {
    /// Full S15.3 manifest registration payload.
    Manifest(Box<AdapterManifest>),
    /// Compact capability declaration payload.
    Capability(AdapterCapability),
}
