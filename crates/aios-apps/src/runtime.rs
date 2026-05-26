//! S12.1 §4 — AppRuntime async trait for the four-phase cross-ecosystem mechanism.
//!
//! Phase A (observe), Phase B (translate), and Phase D (refine) are typed
//! actions driven through this trait. Phase C (first-run audit) lives in
//! S11.1 and is cited, not duplicated.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use strum_macros::{EnumCount, EnumIter};
use tokio::sync::RwLock;

use crate::ecosystem::{
    EcosystemHonestyClass, EcosystemRuntime, ManifestDeltaOutcome, ManifestTranslationStrategy,
};
use crate::error::AppsError;

// ---------------------------------------------------------------------------
// SyscallClass — closed enum from S12.1 §3.4 ObservedBehavior
// ---------------------------------------------------------------------------

/// S12.1 §3.4 — closed taxonomy of syscall classes observed during Phase A
/// pre-flight observation. Ten values (proto `UNSPECIFIED` excluded).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SyscallClass {
    /// Filesystem read operations observed.
    FilesystemRead,
    /// Filesystem write operations observed.
    FilesystemWrite,
    /// Outbound network connection attempts.
    NetworkOutbound,
    /// Inbound network listen attempts.
    NetworkInbound,
    /// Process fork syscalls.
    ProcessFork,
    /// Process exec syscalls.
    ProcessExec,
    /// Inter-process communication.
    Ipc,
    /// GPU command submission.
    GpuSubmit,
    /// Audio device access.
    Audio,
    /// Clipboard read/write access.
    Clipboard,
}

// ---------------------------------------------------------------------------
// ObservedBehavior — Phase A output per S12.1 §3.4
// ---------------------------------------------------------------------------

/// S12.1 §3.4 — behavioural summary from Phase A pre-flight observation.
///
/// Carries structural facts only; never raw secret data, raw clipboard
/// contents, raw filesystem bytes, or raw network payloads (INV-015).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
pub struct ObservedBehavior {
    /// Observation identifier (`obs_<ulid26>`).
    pub observation_id: String,
    /// BLAKE3 hex digest of the artefact under observation (truncated to 32 hex chars).
    pub artifact_hash: String,
    /// Observation duration in seconds (≤ 300 per S12.1 §4.1 hard cap).
    pub observed_for_seconds: u32,
    /// Syscall classes observed during the window.
    pub observed_syscalls: Vec<SyscallClass>,
    /// Canonical paths the artefact attempted to read that were blocked.
    pub blocked_filesystem_reads: Vec<String>,
    /// Canonical paths the artefact attempted to write that were blocked.
    pub blocked_filesystem_writes: Vec<String>,
    /// FQDNs the artefact attempted to resolve; never raw IP.
    pub attempted_dns_resolutions: Vec<String>,
    /// Canonicalised outbound endpoints the artefact attempted to reach.
    pub attempted_outbound_endpoints: Vec<String>,
    /// Whether the artefact attempted GPU initialisation.
    pub attempted_gpu_init: bool,
    /// Whether the artefact attempted audio initialisation.
    pub attempted_audio_init: bool,
    /// Whether the artefact attempted microphone access.
    pub attempted_microphone_open: bool,
    /// Whether the artefact attempted camera access.
    pub attempted_camera_open: bool,
    /// Whether the artefact attempted clipboard read.
    pub attempted_clipboard_read: bool,
    /// Whether the artefact attempted clipboard write.
    pub attempted_clipboard_write: bool,
    /// PII-stripped error messages collected during observation.
    pub error_messages_redacted: Vec<String>,
    /// Whether the artefact's main process exited on its own.
    pub process_terminated_normally: bool,
    /// Exit code if the process terminated.
    pub exit_code: u32,
}

impl ObservedBehavior {
    /// Hard cap on observation duration per S12.1 §4.1.
    pub const MAX_OBSERVATION_SECONDS: u32 = 300;
}

// ---------------------------------------------------------------------------
// AppManifestProposal — Phase B output per S12.1 §4.2
// ---------------------------------------------------------------------------

/// S12.1 §4.2 — a signed manifest proposal produced by Phase B translation.
/// The AI proposer emits this; the operator approves via S5.3.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppManifestProposal {
    /// Proposal identifier (`prop_<ulid26>`).
    pub proposal_id: String,
    /// App identifier (`app_<ulid26>`) assigned at proposal time.
    pub app_id: String,
    /// Target ecosystem runtime.
    pub ecosystem_runtime: EcosystemRuntime,
    /// Honesty class disclosed for this app.
    pub honesty_class: EcosystemHonestyClass,
    /// Translation strategy used.
    pub strategy: ManifestTranslationStrategy,
    /// Operator-visible plain-language rationale.
    pub honesty_disclosure_text: String,
    /// Compatibility caveats (e.g., "Anti-cheat refuses Wine; switch to VM").
    pub compatibility_caveats: Vec<String>,
    /// Declared capability ids per S1.1.
    pub declared_capabilities: Vec<String>,
    /// BLAKE3 hex digest of the ObservedBehavior input.
    pub observed_behavior_hash: String,
    /// Ed25519 signature bytes over the proposal (excluding this field).
    pub proposer_signature: Vec<u8>,
    /// Canonical subject id of the proposer.
    pub proposer_subject_canonical_id: String,
}

// ---------------------------------------------------------------------------
// AppRuntime trait — the async contract for cross-ecosystem app lifecycle
// ---------------------------------------------------------------------------

/// S12.1 §4 — the async trait driving the four-phase cross-ecosystem
/// mechanism (A: observe, B: translate, D: refine). Phase C (first-run
/// capability audit) is owned by S11.1 and is cited, not duplicated.
#[async_trait]
pub trait AppRuntime: Send + Sync {
    /// Phase A — pre-flight observation in a max-restricted sandbox.
    ///
    /// Returns an `ObservedBehavior` summary. Duration is clamped to
    /// `ObservedBehavior::MAX_OBSERVATION_SECONDS` (300 s hard cap per §4.1).
    async fn observe_in_sandbox(
        &self,
        artifact_blob_hash: &str,
        ecosystem_runtime: EcosystemRuntime,
        max_observation_duration_seconds: u32,
    ) -> Result<ObservedBehavior, AppsError>;

    /// Phase B — manifest translation from observed behaviour.
    ///
    /// Produces a signed `AppManifestProposal` that the operator must
    /// approve (S5.3) before install. The AI proposer never calls install
    /// (INV-002).
    async fn translate_manifest(
        &self,
        observed_behavior: ObservedBehavior,
        strategy: ManifestTranslationStrategy,
        target_runtime: EcosystemRuntime,
    ) -> Result<AppManifestProposal, AppsError>;

    /// Phase D — propose a manifest delta for continuous refinement.
    ///
    /// Returns a `ManifestDeltaOutcome`. The operator must approve the
    /// delta before it takes effect.
    async fn propose_manifest_delta(
        &self,
        app_id: &str,
        reason: &str,
    ) -> Result<ManifestDeltaOutcome, AppsError>;
}

// ---------------------------------------------------------------------------
// InMemoryAppRuntime
// ---------------------------------------------------------------------------

/// In-memory `AppRuntime` harness for tests and early integration.
///
/// Stores observations and proposals in memory with deterministic stub
/// behaviour suitable for the §22 golden path.
#[derive(Clone, Debug)]
pub struct InMemoryAppRuntime {
    observations: Arc<RwLock<HashMap<String, ObservedBehavior>>>,
    proposals: Arc<RwLock<HashMap<String, AppManifestProposal>>>,
}

impl InMemoryAppRuntime {
    /// Create an empty in-memory runtime.
    #[must_use]
    pub fn new() -> Self {
        Self {
            observations: Arc::new(RwLock::new(HashMap::new())),
            proposals: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Read-only snapshot of stored observations (for test assertions).
    pub async fn observation_count(&self) -> usize {
        self.observations.read().await.len()
    }

    /// Read-only snapshot of stored proposals (for test assertions).
    pub async fn proposal_count(&self) -> usize {
        self.proposals.read().await.len()
    }

    /// Direct test seam: inject a stored observation.
    pub async fn inject_observation(&self, obs: ObservedBehavior) {
        self.observations
            .write()
            .await
            .insert(obs.observation_id.clone(), obs);
    }
}

impl Default for InMemoryAppRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AppRuntime for InMemoryAppRuntime {
    async fn observe_in_sandbox(
        &self,
        artifact_blob_hash: &str,
        _ecosystem_runtime: EcosystemRuntime,
        max_observation_duration_seconds: u32,
    ) -> Result<ObservedBehavior, AppsError> {
        // Hard cap per S12.1 §4.1.
        let duration =
            max_observation_duration_seconds.min(ObservedBehavior::MAX_OBSERVATION_SECONDS);

        // Per S12.1 §4.1: if the observation would exceed the hard cap and
        // the operator has not pre-approved extended observation, emit a
        // timeout evidence record. This stub rejects durations > 300 s.
        if max_observation_duration_seconds > ObservedBehavior::MAX_OBSERVATION_SECONDS {
            return Err(AppsError::ObservationRejected {
                observation_id: String::new(),
                reason: format!(
                    "requested {} s exceeds hard cap of {} s",
                    max_observation_duration_seconds,
                    ObservedBehavior::MAX_OBSERVATION_SECONDS,
                ),
            });
        }

        let observation_id = format!("obs_{}", ulid::Ulid::new().to_string().to_lowercase());

        // Stub observation: no syscalls observed, no blocked access, clean exit.
        let behavior = ObservedBehavior {
            observation_id: observation_id.clone(),
            artifact_hash: artifact_blob_hash.to_string(),
            observed_for_seconds: duration,
            observed_syscalls: Vec::new(),
            blocked_filesystem_reads: Vec::new(),
            blocked_filesystem_writes: Vec::new(),
            attempted_dns_resolutions: Vec::new(),
            attempted_outbound_endpoints: Vec::new(),
            attempted_gpu_init: false,
            attempted_audio_init: false,
            attempted_microphone_open: false,
            attempted_camera_open: false,
            attempted_clipboard_read: false,
            attempted_clipboard_write: false,
            error_messages_redacted: Vec::new(),
            process_terminated_normally: true,
            exit_code: 0,
        };

        self.observations
            .write()
            .await
            .insert(observation_id, behavior.clone());
        Ok(behavior)
    }

    async fn translate_manifest(
        &self,
        observed_behavior: ObservedBehavior,
        strategy: ManifestTranslationStrategy,
        target_runtime: EcosystemRuntime,
    ) -> Result<AppManifestProposal, AppsError> {
        let proposal_id = format!("prop_{}", ulid::Ulid::new().to_string().to_lowercase());
        let app_id = format!("app_{}", ulid::Ulid::new().to_string().to_lowercase());

        // Stub proposal: derives honesty class from the runtime's spec table
        // (S12.1 §3.1), carries empty capabilities and caveats.
        let honesty_class = Self::honesty_class_for_runtime(target_runtime);

        let proposal = AppManifestProposal {
            proposal_id: proposal_id.clone(),
            app_id,
            ecosystem_runtime: target_runtime,
            honesty_class,
            strategy,
            honesty_disclosure_text: format!(
                "{} app observed for {} s under {} runtime",
                observed_behavior.artifact_hash,
                observed_behavior.observed_for_seconds,
                target_runtime,
            ),
            compatibility_caveats: Vec::new(),
            declared_capabilities: Vec::new(),
            observed_behavior_hash: observed_behavior.observation_id,
            proposer_signature: Vec::new(),
            proposer_subject_canonical_id: "_system:service:app-proposer".to_string(),
        };

        self.proposals
            .write()
            .await
            .insert(proposal_id, proposal.clone());
        Ok(proposal)
    }

    async fn propose_manifest_delta(
        &self,
        _app_id: &str,
        _reason: &str,
    ) -> Result<ManifestDeltaOutcome, AppsError> {
        // Stub: always proposes a delta for operator review.
        Ok(ManifestDeltaOutcome::DeltaProposed)
    }
}

impl InMemoryAppRuntime {
    /// Map an `EcosystemRuntime` to its default `EcosystemHonestyClass`
    /// per the S12.1 §3.1 table. Used as a deterministic stub for Phase B.
    const fn honesty_class_for_runtime(runtime: EcosystemRuntime) -> EcosystemHonestyClass {
        match runtime {
            EcosystemRuntime::RuntimeLinuxNative
            | EcosystemRuntime::RuntimeFlatpak
            | EcosystemRuntime::RuntimeSnap => EcosystemHonestyClass::FullySupported,
            EcosystemRuntime::RuntimeAppimage
            | EcosystemRuntime::RuntimeDistrobox
            | EcosystemRuntime::RuntimeWindowsProton
            | EcosystemRuntime::RuntimeAndroidWaydroid
            | EcosystemRuntime::RuntimeMacosDarling => EcosystemHonestyClass::PartiallySupported,
            EcosystemRuntime::RuntimeWindowsVm
            | EcosystemRuntime::RuntimeAndroidVmWithGms
            | EcosystemRuntime::RuntimeMacosVm => EcosystemHonestyClass::RequiresVm,
            EcosystemRuntime::RuntimeRemoteAppleBridge => {
                EcosystemHonestyClass::NotRunnableOnNonNative
            }
        }
    }
}
