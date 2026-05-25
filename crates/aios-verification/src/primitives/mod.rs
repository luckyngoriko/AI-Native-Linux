//! Primitive execution tiers for the S2.4 verification vocabulary.
//!
//! T-066 classifies the frozen 36-entry T-064 vocabulary as:
//!
//! - Tier 1: pure deterministic predicates over supplied JSON payloads:
//!   `tree_max_depth`, `web_chrome_z_index_at_least`,
//!   `namespace_catalog_version`, `subject_session_flag_state`,
//!   `secret_pattern_match`.
//! - Tier 2: local filesystem/process/socket probes through [`LocalProbe`]:
//!   `service_active`, `service_inactive`, `package_installed`, `port_open`,
//!   `port_closed`, `file_exists`, `file_hash`, `repo_exists`,
//!   `web_renderer_bound_to`.
//! - Tier 3: network/control-plane/cross-crate primitives deferred from M8:
//!   every remaining primitive, including `http_ok`, policy/evidence,
//!   AIOS-FS namespace, renderer/theme/GPU, network, DNS/VPN, approval, and
//!   spec-table probes.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::net::TcpListener;

use crate::{PrimitiveResult, VerificationPrimitive};

pub mod tier1;
pub mod tier2;
pub mod tier3;

/// Primitive implementation tier selected for T-066 dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveTier {
    /// Pure deterministic predicate over the supplied JSON payload.
    Tier1,
    /// Local filesystem/process/socket probe behind [`LocalProbe`].
    Tier2,
    /// Deferred network/control-plane/cross-crate probe.
    Tier3,
}

/// Return the T-066 tier classification for a S2.4 primitive.
#[must_use]
pub const fn primitive_tier(primitive: VerificationPrimitive) -> PrimitiveTier {
    match primitive {
        VerificationPrimitive::TreeMaxDepth
        | VerificationPrimitive::WebChromeZIndexAtLeast
        | VerificationPrimitive::NamespaceCatalogVersion
        | VerificationPrimitive::SubjectSessionFlagState
        | VerificationPrimitive::SecretPatternMatch => PrimitiveTier::Tier1,
        VerificationPrimitive::ServiceActive
        | VerificationPrimitive::ServiceInactive
        | VerificationPrimitive::PackageInstalled
        | VerificationPrimitive::PortOpen
        | VerificationPrimitive::PortClosed
        | VerificationPrimitive::FileExists
        | VerificationPrimitive::FileHash
        | VerificationPrimitive::RepoExists
        | VerificationPrimitive::WebRendererBoundTo => PrimitiveTier::Tier2,
        VerificationPrimitive::HttpOk
        | VerificationPrimitive::AiosfsPointer
        | VerificationPrimitive::PolicyDecision
        | VerificationPrimitive::EvidenceExists
        | VerificationPrimitive::NetworkSubjectOutboundClass
        | VerificationPrimitive::NetworkActiveExposureClass
        | VerificationPrimitive::NetworkExternalModelCallBrokeredOnly
        | VerificationPrimitive::DnsResolverBackend
        | VerificationPrimitive::VpnTunnelActive
        | VerificationPrimitive::MdnsPosture
        | VerificationPrimitive::AiosfsPathInNamespace
        | VerificationPrimitive::SurfaceInZone
        | VerificationPrimitive::TreeContainsKind
        | VerificationPrimitive::ThemeSatisfiesInvariants
        | VerificationPrimitive::ThemeConstitutionalIconsIntact
        | VerificationPrimitive::GpuBindingClass
        | VerificationPrimitive::AiosfsPathOwnerResolved
        | VerificationPrimitive::AiosfsPathRecoveryTreatmentSet
        | VerificationPrimitive::StatusIndicatorVisible
        | VerificationPrimitive::FilesystemRootIntact
        | VerificationPrimitive::SpecConsumesTable
        | VerificationPrimitive::ApprovalBindingState => PrimitiveTier::Tier3,
    }
}

/// Minimal probe verdict used by tier helpers before it is wrapped as a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeVerdict {
    /// Predicate verdict. `false` with [`Self::error`] set means probe error.
    pub passed: bool,
    /// Observed value emitted by the probe.
    pub actual: Value,
    /// Probe error detail when no normal predicate verdict was produced.
    pub error: Option<String>,
}

impl ProbeVerdict {
    /// Build a passing verdict.
    #[must_use]
    pub const fn passed(actual: Value) -> Self {
        Self {
            passed: true,
            actual,
            error: None,
        }
    }

    /// Build a failing predicate verdict.
    #[must_use]
    pub const fn failed(actual: Value) -> Self {
        Self {
            passed: false,
            actual,
            error: None,
        }
    }

    /// Build a probe-error verdict.
    #[must_use]
    pub fn probe_error(reason: impl Into<String>) -> Self {
        Self {
            passed: false,
            actual: Value::Null,
            error: Some(format!("PROBE_ERROR: {}", reason.into())),
        }
    }
}

/// Wrap a tier verdict into the public per-primitive result shape.
#[must_use]
pub fn primitive_result(
    primitive: VerificationPrimitive,
    expected: &Value,
    verdict: ProbeVerdict,
) -> PrimitiveResult {
    PrimitiveResult {
        primitive_kind: primitive,
        passed: verdict.passed,
        actual: verdict.actual,
        expected: expected.clone(),
        elapsed_ms: 0,
        error: verdict.error,
    }
}

/// Local read-only probe surface used by Tier-2 primitives.
#[async_trait]
pub trait LocalProbe: Send + Sync {
    /// Return whether a local path exists.
    async fn file_exists(&self, path: &str) -> bool;
    /// Return the lowercase BLAKE3 hex digest for a local path.
    async fn file_blake3(&self, path: &str) -> Option<String>;
    /// Return whether a local process with this name appears to be running.
    async fn process_running(&self, name: &str) -> bool;
    /// Return whether a local TCP port appears to be listening.
    async fn port_listening(&self, port: u16) -> bool;
    /// Return a local environment variable value.
    async fn env_var(&self, name: &str) -> Option<String>;
    /// Run a local command and return its exit code.
    async fn command_exit_code(&self, cmd: &str, args: &[String]) -> Option<i32>;
}

/// Production local probe backed by standard library Linux-safe checks.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdLocalProbe;

#[async_trait]
impl LocalProbe for StdLocalProbe {
    async fn file_exists(&self, path: &str) -> bool {
        Path::new(path).exists()
    }

    async fn file_blake3(&self, path: &str) -> Option<String> {
        fs::read(path)
            .ok()
            .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
    }

    async fn process_running(&self, name: &str) -> bool {
        process_running_from_proc(name)
    }

    async fn port_listening(&self, port: u16) -> bool {
        match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => {
                drop(listener);
                false
            }
            Err(err) => err.kind() == ErrorKind::AddrInUse,
        }
    }

    async fn env_var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    async fn command_exit_code(&self, cmd: &str, args: &[String]) -> Option<i32> {
        Command::new(cmd)
            .args(args)
            .status()
            .ok()
            .and_then(|status| status.code())
    }
}

/// Key type for [`MockLocalProbe`] canned responses.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MockProbeKey {
    /// Response for [`LocalProbe::file_exists`].
    FileExists(String),
    /// Response for [`LocalProbe::file_blake3`].
    FileBlake3(String),
    /// Response for [`LocalProbe::process_running`].
    ProcessRunning(String),
    /// Response for [`LocalProbe::port_listening`].
    PortListening(u16),
    /// Response for [`LocalProbe::env_var`].
    EnvVar(String),
    /// Response for [`LocalProbe::command_exit_code`].
    CommandExitCode {
        /// Command binary.
        cmd: String,
        /// Command arguments.
        args: Vec<String>,
    },
}

/// Value type for [`MockLocalProbe`] canned responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockProbeValue {
    /// Boolean response.
    Bool(bool),
    /// String response.
    String(String),
    /// Signed integer response.
    I32(i32),
}

/// Deterministic local probe fixture for integration tests.
#[derive(Debug, Clone, Default)]
pub struct MockLocalProbe {
    /// Canned method responses keyed by probe call.
    pub responses: HashMap<MockProbeKey, MockProbeValue>,
}

impl MockLocalProbe {
    /// Seed a file-exists response.
    #[must_use]
    pub fn with_file_exists(mut self, path: impl Into<String>, exists: bool) -> Self {
        self.responses.insert(
            MockProbeKey::FileExists(path.into()),
            MockProbeValue::Bool(exists),
        );
        self
    }

    /// Seed a file BLAKE3 response.
    #[must_use]
    pub fn with_file_blake3(mut self, path: impl Into<String>, hash: impl Into<String>) -> Self {
        self.responses.insert(
            MockProbeKey::FileBlake3(path.into()),
            MockProbeValue::String(hash.into()),
        );
        self
    }

    /// Seed a process-running response.
    #[must_use]
    pub fn with_process_running(mut self, name: impl Into<String>, running: bool) -> Self {
        self.responses.insert(
            MockProbeKey::ProcessRunning(name.into()),
            MockProbeValue::Bool(running),
        );
        self
    }

    /// Seed a port-listening response.
    #[must_use]
    pub fn with_port_listening(mut self, port: u16, listening: bool) -> Self {
        self.responses.insert(
            MockProbeKey::PortListening(port),
            MockProbeValue::Bool(listening),
        );
        self
    }

    /// Seed an environment-variable response.
    #[must_use]
    pub fn with_env_var(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.responses.insert(
            MockProbeKey::EnvVar(name.into()),
            MockProbeValue::String(value.into()),
        );
        self
    }

    /// Seed a command exit-code response.
    #[must_use]
    pub fn with_command_exit_code(
        mut self,
        cmd: impl Into<String>,
        args: Vec<String>,
        exit_code: Option<i32>,
    ) -> Self {
        if let Some(exit_code) = exit_code {
            self.responses.insert(
                MockProbeKey::CommandExitCode {
                    cmd: cmd.into(),
                    args,
                },
                MockProbeValue::I32(exit_code),
            );
        }
        self
    }

    fn bool_response(&self, key: &MockProbeKey) -> bool {
        matches!(self.responses.get(key), Some(MockProbeValue::Bool(true)))
    }

    fn string_response(&self, key: &MockProbeKey) -> Option<String> {
        match self.responses.get(key) {
            Some(MockProbeValue::String(value)) => Some(value.clone()),
            _ => None,
        }
    }

    fn i32_response(&self, key: &MockProbeKey) -> Option<i32> {
        match self.responses.get(key) {
            Some(MockProbeValue::I32(value)) => Some(*value),
            _ => None,
        }
    }
}

#[async_trait]
impl LocalProbe for MockLocalProbe {
    async fn file_exists(&self, path: &str) -> bool {
        self.bool_response(&MockProbeKey::FileExists(path.to_owned()))
    }

    async fn file_blake3(&self, path: &str) -> Option<String> {
        self.string_response(&MockProbeKey::FileBlake3(path.to_owned()))
    }

    async fn process_running(&self, name: &str) -> bool {
        self.bool_response(&MockProbeKey::ProcessRunning(name.to_owned()))
    }

    async fn port_listening(&self, port: u16) -> bool {
        self.bool_response(&MockProbeKey::PortListening(port))
    }

    async fn env_var(&self, name: &str) -> Option<String> {
        self.string_response(&MockProbeKey::EnvVar(name.to_owned()))
    }

    async fn command_exit_code(&self, cmd: &str, args: &[String]) -> Option<i32> {
        self.i32_response(&MockProbeKey::CommandExitCode {
            cmd: cmd.to_owned(),
            args: args.to_vec(),
        })
    }
}

pub(crate) fn required_str<'a>(payload: &'a Value, field: &str) -> Result<&'a str, ProbeVerdict> {
    payload.get(field).and_then(Value::as_str).ok_or_else(|| {
        ProbeVerdict::probe_error(format!("missing required string field `{field}`"))
    })
}

pub(crate) fn optional_str<'a>(payload: &'a Value, field: &str) -> Option<&'a str> {
    payload.get(field).and_then(Value::as_str)
}

pub(crate) fn required_bool(payload: &Value, field: &str) -> Result<bool, ProbeVerdict> {
    payload
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| ProbeVerdict::probe_error(format!("missing required bool field `{field}`")))
}

pub(crate) fn optional_bool(payload: &Value, field: &str, default: bool) -> bool {
    payload
        .get(field)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

pub(crate) fn required_u64(payload: &Value, field: &str) -> Result<u64, ProbeVerdict> {
    payload.get(field).and_then(Value::as_u64).ok_or_else(|| {
        ProbeVerdict::probe_error(format!("missing required unsigned integer field `{field}`"))
    })
}

pub(crate) fn required_i32(payload: &Value, field: &str) -> Result<i32, ProbeVerdict> {
    let value = payload.get(field).and_then(Value::as_i64).ok_or_else(|| {
        ProbeVerdict::probe_error(format!("missing required integer field `{field}`"))
    })?;
    i32::try_from(value)
        .map_err(|_err| ProbeVerdict::probe_error(format!("field `{field}` is out of i32 range")))
}

pub(crate) fn required_u16(payload: &Value, field: &str) -> Result<u16, ProbeVerdict> {
    let value = required_u64(payload, field)?;
    u16::try_from(value)
        .map_err(|_err| ProbeVerdict::probe_error(format!("field `{field}` is out of u16 range")))
}

pub(crate) fn string_array(payload: &Value, field: &str) -> Result<Vec<String>, ProbeVerdict> {
    let values = payload
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ProbeVerdict::probe_error(format!("missing required string array field `{field}`"))
        })?;
    values
        .iter()
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| {
                ProbeVerdict::probe_error(format!("field `{field}` contains a non-string item"))
            })
        })
        .collect()
}

pub(crate) fn optional_string_array(
    payload: &Value,
    field: &str,
) -> Result<Vec<String>, ProbeVerdict> {
    match payload.get(field) {
        Some(Value::Array(_)) => string_array(payload, field),
        Some(_) => Err(ProbeVerdict::probe_error(format!(
            "field `{field}` must be a string array"
        ))),
        None => Ok(Vec::new()),
    }
}

pub(crate) fn bool_actual(field: &str, value: bool) -> Value {
    json!({field: value})
}

fn process_running_from_proc(name: &str) -> bool {
    fs::read_dir("/proc")
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter(|entry| entry.file_name().to_string_lossy().parse::<u32>().is_ok())
        .any(|entry| proc_entry_matches_name(&entry.path(), name))
}

fn proc_entry_matches_name(path: &Path, name: &str) -> bool {
    proc_comm_matches(path, name) || proc_cmdline_matches(path, name)
}

fn proc_comm_matches(path: &Path, name: &str) -> bool {
    fs::read_to_string(path.join("comm"))
        .ok()
        .is_some_and(|comm| comm.trim() == name)
}

fn proc_cmdline_matches(path: &Path, name: &str) -> bool {
    fs::read(path.join("cmdline")).ok().is_some_and(|cmdline| {
        cmdline
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .any(|part| command_part_matches_name(part, name))
    })
}

fn command_part_matches_name(part: &[u8], name: &str) -> bool {
    let decoded = String::from_utf8_lossy(part);
    Path::new(decoded.as_ref())
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|file_name| file_name == name)
}
