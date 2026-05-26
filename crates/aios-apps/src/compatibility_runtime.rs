//! S12.3 Compatibility Runtime Adapters — per-ecosystem launch orchestration.
//!
//! The `CompatibilityRuntimeAdapter` async trait is the contract every runtime
//! adapter implements. Five stub adapters cover Linux/Windows/Android/Web/AIOSNative.
//! Stubs are replaced with real runtime invocation in a later milestone.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ecosystem::EcosystemRuntime;
use crate::error::AppsError;
use crate::package_store::AppPackage;

// ---------------------------------------------------------------------------
// SubjectRef — an opaque reference to a subject for launch authorization
// ---------------------------------------------------------------------------

/// An opaque reference to an AIOS subject (human, agent, application, service).
/// Carries the canonical subject id for evidence linkage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubjectRef {
    /// Canonical subject identifier (e.g. `human:lucky`).
    pub canonical_id: String,
}

// ---------------------------------------------------------------------------
// RuntimeCapability — what a runtime adapter can provide
// ---------------------------------------------------------------------------

/// A named capability a runtime adapter exposes for capability-based dispatch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCapability {
    /// Short capability name (e.g. "gpu_passthrough", "network_isolation").
    pub name: String,
    /// Human-readable description for operator-facing surfaces.
    pub description: String,
}

// ---------------------------------------------------------------------------
// LaunchContext — the inputs to a single launch request
// ---------------------------------------------------------------------------

/// The full context for a single `launch` call through a runtime adapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchContext {
    /// The subject requesting the launch.
    pub subject: SubjectRef,
    /// Optional sandbox profile id from S3.2 compose step.
    pub sandbox_profile_id: Option<String>,
    /// Whether the system is currently in recovery mode.
    pub recovery_mode: bool,
    /// When the launch request was created.
    pub started_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// LaunchOutcome — the four possible outcomes of a launch attempt
// ---------------------------------------------------------------------------

/// The outcome of a single `CompatibilityRuntimeAdapter::launch` call.
///
/// Four variants per S12.3: launched, requires VM, incompatible ecosystem,
/// or runtime unavailable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LaunchOutcome {
    /// App launched successfully; instance id returned for lifecycle tracking.
    Launched {
        /// Unique instance identifier for this launch.
        instance_id: String,
        /// The ecosystem runtime that handled the launch.
        ecosystem: EcosystemRuntime,
        /// When the instance started.
        started_at: DateTime<Utc>,
    },
    /// The ecosystem requires a full VM; Wine/Waydroid translation insufficient.
    RequiresVm {
        /// Operator-visible reason (e.g. "WSL2 or VM").
        reason: String,
    },
    /// The package's declared ecosystem is incompatible with this adapter.
    IncompatibleEcosystem {
        /// Other ecosystem runtimes that could handle this package.
        available_alternatives: Vec<EcosystemRuntime>,
    },
    /// No adapter registered for the requested ecosystem.
    RuntimeUnavailable(String),
}

// ---------------------------------------------------------------------------
// CompatibilityRuntimeAdapter trait
// ---------------------------------------------------------------------------

/// S12.3 — the async contract every runtime adapter must implement.
///
/// Each adapter owns one `EcosystemRuntime` and handles launch/stop/lifecycle
/// for packages targeting that ecosystem.
#[async_trait]
pub trait CompatibilityRuntimeAdapter: Send + Sync {
    /// The ecosystem this adapter targets.
    fn ecosystem(&self) -> EcosystemRuntime;

    /// Launch a package under this adapter's runtime.
    async fn launch(
        &self,
        package: &AppPackage,
        context: &LaunchContext,
    ) -> Result<LaunchOutcome, AppsError>;

    /// Stop a previously launched instance.
    async fn stop(&self, instance_id: &str) -> Result<(), AppsError>;

    /// Return the capabilities this adapter provides.
    fn available_capabilities(&self) -> Vec<RuntimeCapability>;
}

// ---------------------------------------------------------------------------
// Helper: detect ecosystem from manifest_bytes JSON
// ---------------------------------------------------------------------------

/// Best-effort ecosystem detection from an `AppPackage`'s manifest bytes.
///
/// Tries to parse the manifest as JSON and read an `"ecosystem"` string field.
/// Falls back to `RuntimeLinuxNative` when detection fails — the default
/// assumption for packages without explicit ecosystem metadata.
pub(crate) fn detect_ecosystem(package: &AppPackage) -> EcosystemRuntime {
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&package.manifest_bytes) {
        if let Some(eco_str) = json.get("ecosystem").and_then(|v| v.as_str()) {
            match eco_str {
                "RUNTIME_LINUX_NATIVE" => return EcosystemRuntime::RuntimeLinuxNative,
                "RUNTIME_FLATPAK" => return EcosystemRuntime::RuntimeFlatpak,
                "RUNTIME_APPIMAGE" => return EcosystemRuntime::RuntimeAppimage,
                "RUNTIME_SNAP" => return EcosystemRuntime::RuntimeSnap,
                "RUNTIME_DISTROBOX" => return EcosystemRuntime::RuntimeDistrobox,
                "RUNTIME_WINDOWS_PROTON" => return EcosystemRuntime::RuntimeWindowsProton,
                "RUNTIME_WINDOWS_VM" => return EcosystemRuntime::RuntimeWindowsVm,
                "RUNTIME_ANDROID_WAYDROID" => return EcosystemRuntime::RuntimeAndroidWaydroid,
                "RUNTIME_ANDROID_VM_WITH_GMS" => return EcosystemRuntime::RuntimeAndroidVmWithGms,
                "RUNTIME_MACOS_DARLING" => return EcosystemRuntime::RuntimeMacosDarling,
                "RUNTIME_MACOS_VM" => return EcosystemRuntime::RuntimeMacosVm,
                "RUNTIME_REMOTE_APPLE_BRIDGE" => return EcosystemRuntime::RuntimeRemoteAppleBridge,
                _ => {}
            }
        }
    }
    EcosystemRuntime::RuntimeLinuxNative
}

/// Best-effort kind detection from an `AppPackage`'s manifest bytes.
///
/// Returns `Some("APP")` or `Some("ADAPTER")` when a `"kind"` JSON field
/// is present in the manifest; `None` otherwise.
fn detect_kind(package: &AppPackage) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(&package.manifest_bytes)
        .ok()
        .and_then(|json| {
            json.get("kind")
                .and_then(|v| v.as_str())
                .map(str::to_uppercase)
        })
}

// ---------------------------------------------------------------------------
// LinuxRuntimeAdapter
// ---------------------------------------------------------------------------

/// Stub adapter for `RuntimeLinuxNative` — direct ELF execution.
///
/// Always returns `Launched` with a deterministic instance id.
#[derive(Clone, Debug, Default)]
pub struct LinuxRuntimeAdapter;

impl LinuxRuntimeAdapter {
    /// Create a new Linux runtime adapter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CompatibilityRuntimeAdapter for LinuxRuntimeAdapter {
    fn ecosystem(&self) -> EcosystemRuntime {
        EcosystemRuntime::RuntimeLinuxNative
    }

    async fn launch(
        &self,
        _package: &AppPackage,
        context: &LaunchContext,
    ) -> Result<LaunchOutcome, AppsError> {
        let instance_id = format!(
            "linux_instance_{}",
            ulid::Ulid::new().to_string().to_lowercase()
        );
        Ok(LaunchOutcome::Launched {
            instance_id,
            ecosystem: EcosystemRuntime::RuntimeLinuxNative,
            started_at: context.started_at,
        })
    }

    async fn stop(&self, _instance_id: &str) -> Result<(), AppsError> {
        Ok(())
    }

    fn available_capabilities(&self) -> Vec<RuntimeCapability> {
        vec![
            RuntimeCapability {
                name: "native_elf_execution".into(),
                description: "Direct ELF binary execution on the Linux substrate".into(),
            },
            RuntimeCapability {
                name: "subprocess_fork".into(),
                description: "Process fork/exec under composed sandbox".into(),
            },
        ]
    }
}

// ---------------------------------------------------------------------------
// WindowsRuntimeAdapter
// ---------------------------------------------------------------------------

/// Stub adapter for `RuntimeWindowsProton` — Wine/Proton translation.
///
/// Always returns `RequiresVm` in the stub; real Wine prefix creation
/// lands in a later milestone.
#[derive(Clone, Debug, Default)]
pub struct WindowsRuntimeAdapter;

impl WindowsRuntimeAdapter {
    /// Create a new Windows runtime adapter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CompatibilityRuntimeAdapter for WindowsRuntimeAdapter {
    fn ecosystem(&self) -> EcosystemRuntime {
        EcosystemRuntime::RuntimeWindowsProton
    }

    async fn launch(
        &self,
        _package: &AppPackage,
        _context: &LaunchContext,
    ) -> Result<LaunchOutcome, AppsError> {
        Ok(LaunchOutcome::RequiresVm {
            reason: "WSL2 or VM".into(),
        })
    }

    async fn stop(&self, _instance_id: &str) -> Result<(), AppsError> {
        Ok(())
    }

    fn available_capabilities(&self) -> Vec<RuntimeCapability> {
        vec![
            RuntimeCapability {
                name: "wine_prefix_creation".into(),
                description: "Per-app Wine/Proton prefix creation and management".into(),
            },
            RuntimeCapability {
                name: "win32_syscall_translation".into(),
                description: "Win32 API to Linux syscall translation via Wine".into(),
            },
        ]
    }
}

// ---------------------------------------------------------------------------
// AndroidRuntimeAdapter
// ---------------------------------------------------------------------------

/// Stub adapter for `RuntimeAndroidWaydroid` — Waydroid container execution.
///
/// Always returns `RequiresVm` in the stub; real Waydroid container
/// orchestration lands in a later milestone.
#[derive(Clone, Debug, Default)]
pub struct AndroidRuntimeAdapter;

impl AndroidRuntimeAdapter {
    /// Create a new Android runtime adapter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CompatibilityRuntimeAdapter for AndroidRuntimeAdapter {
    fn ecosystem(&self) -> EcosystemRuntime {
        EcosystemRuntime::RuntimeAndroidWaydroid
    }

    async fn launch(
        &self,
        _package: &AppPackage,
        _context: &LaunchContext,
    ) -> Result<LaunchOutcome, AppsError> {
        Ok(LaunchOutcome::RequiresVm {
            reason: "ART container or VM".into(),
        })
    }

    async fn stop(&self, _instance_id: &str) -> Result<(), AppsError> {
        Ok(())
    }

    fn available_capabilities(&self) -> Vec<RuntimeCapability> {
        vec![
            RuntimeCapability {
                name: "waydroid_container".into(),
                description: "Waydroid LXC container creation and APK launch".into(),
            },
            RuntimeCapability {
                name: "android_apk_sideload".into(),
                description: "APK sideload into Waydroid container".into(),
            },
        ]
    }
}

// ---------------------------------------------------------------------------
// WebRuntimeAdapter
// ---------------------------------------------------------------------------

/// Stub adapter for web applications under the Linux substrate.
///
/// Returns `Launched` when the package manifest declares `kind: "APP"`;
/// returns `IncompatibleEcosystem` otherwise. Web apps run as native
/// Linux processes (browser engine or WASM runtime).
///
/// Note: `EcosystemRuntime` has no dedicated Web variant; this adapter
/// maps to `RuntimeLinuxNative` per the T-115 closed vocabulary.
#[derive(Clone, Debug, Default)]
pub struct WebRuntimeAdapter;

impl WebRuntimeAdapter {
    /// Create a new Web runtime adapter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CompatibilityRuntimeAdapter for WebRuntimeAdapter {
    fn ecosystem(&self) -> EcosystemRuntime {
        EcosystemRuntime::RuntimeLinuxNative
    }

    async fn launch(
        &self,
        package: &AppPackage,
        context: &LaunchContext,
    ) -> Result<LaunchOutcome, AppsError> {
        let kind = detect_kind(package);
        if kind.as_deref() == Some("APP") {
            let instance_id = format!(
                "web_instance_{}",
                ulid::Ulid::new().to_string().to_lowercase()
            );
            Ok(LaunchOutcome::Launched {
                instance_id,
                ecosystem: EcosystemRuntime::RuntimeLinuxNative,
                started_at: context.started_at,
            })
        } else {
            Ok(LaunchOutcome::IncompatibleEcosystem {
                available_alternatives: vec![EcosystemRuntime::RuntimeLinuxNative],
            })
        }
    }

    async fn stop(&self, _instance_id: &str) -> Result<(), AppsError> {
        Ok(())
    }

    fn available_capabilities(&self) -> Vec<RuntimeCapability> {
        vec![
            RuntimeCapability {
                name: "browser_engine".into(),
                description: "Chromium/WebKit browser engine for web apps".into(),
            },
            RuntimeCapability {
                name: "wasm_runtime".into(),
                description: "WebAssembly runtime for sandboxed web execution".into(),
            },
        ]
    }
}

// ---------------------------------------------------------------------------
// AiosNativeRuntimeAdapter
// ---------------------------------------------------------------------------

/// Stub adapter for AIOS-native packages — first-class AIOS apps.
///
/// Always returns `Launched`; no virtualization needed. AIOS-native apps
/// are ELF binaries that link against the AIOS SDK and declare capabilities
/// in their manifest directly.
///
/// Note: `EcosystemRuntime` has no dedicated AIOS-native variant; this
/// adapter maps to `RuntimeLinuxNative` per the T-115 closed vocabulary.
#[derive(Clone, Debug, Default)]
pub struct AiosNativeRuntimeAdapter;

impl AiosNativeRuntimeAdapter {
    /// Create a new AIOS-native runtime adapter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CompatibilityRuntimeAdapter for AiosNativeRuntimeAdapter {
    fn ecosystem(&self) -> EcosystemRuntime {
        EcosystemRuntime::RuntimeLinuxNative
    }

    async fn launch(
        &self,
        _package: &AppPackage,
        context: &LaunchContext,
    ) -> Result<LaunchOutcome, AppsError> {
        let instance_id = format!(
            "aios_instance_{}",
            ulid::Ulid::new().to_string().to_lowercase()
        );
        Ok(LaunchOutcome::Launched {
            instance_id,
            ecosystem: EcosystemRuntime::RuntimeLinuxNative,
            started_at: context.started_at,
        })
    }

    async fn stop(&self, _instance_id: &str) -> Result<(), AppsError> {
        Ok(())
    }

    fn available_capabilities(&self) -> Vec<RuntimeCapability> {
        vec![
            RuntimeCapability {
                name: "aios_sdk".into(),
                description: "AIOS-native SDK capability declarations".into(),
            },
            RuntimeCapability {
                name: "typed_action_dispatch".into(),
                description: "Direct typed-action dispatch through the capability runtime".into(),
            },
        ]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    // Helper: build an AppPackage with arbitrary manifest bytes.
    fn stub_package(manifest_bytes: &[u8]) -> AppPackage {
        AppPackage {
            package_id: crate::package::PackageId(format!(
                "pkg_{}",
                ulid::Ulid::new().to_string().to_lowercase()
            )),
            name: "test-app".into(),
            version: "0.1.0".into(),
            manifest_bytes: manifest_bytes.to_vec(),
            content_hash_blake3: blake3::hash(manifest_bytes).to_hex().to_string(),
            ed25519_signature: Vec::new(),
            signer_public_key: Vec::new(),
            registered_at: Utc::now(),
        }
    }

    fn stub_context() -> LaunchContext {
        LaunchContext {
            subject: SubjectRef {
                canonical_id: "human:test".into(),
            },
            sandbox_profile_id: None,
            recovery_mode: false,
            started_at: Utc::now(),
        }
    }

    // --- Adapter construction + ecosystem identity ---

    #[test]
    fn linux_adapter_constructable_and_ecosystem_correct() {
        let a = LinuxRuntimeAdapter::new();
        assert_eq!(a.ecosystem(), EcosystemRuntime::RuntimeLinuxNative);
    }

    #[test]
    fn windows_adapter_constructable_and_ecosystem_correct() {
        let a = WindowsRuntimeAdapter::new();
        assert_eq!(a.ecosystem(), EcosystemRuntime::RuntimeWindowsProton);
    }

    #[test]
    fn android_adapter_constructable_and_ecosystem_correct() {
        let a = AndroidRuntimeAdapter::new();
        assert_eq!(a.ecosystem(), EcosystemRuntime::RuntimeAndroidWaydroid);
    }

    #[test]
    fn web_adapter_constructable_and_ecosystem_correct() {
        let a = WebRuntimeAdapter::new();
        assert_eq!(a.ecosystem(), EcosystemRuntime::RuntimeLinuxNative);
    }

    #[test]
    fn aios_native_adapter_constructable_and_ecosystem_correct() {
        let a = AiosNativeRuntimeAdapter::new();
        assert_eq!(a.ecosystem(), EcosystemRuntime::RuntimeLinuxNative);
    }

    // --- Launch behaviour ---

    #[tokio::test]
    async fn linux_adapter_launch_returns_launched() {
        let a = LinuxRuntimeAdapter::new();
        let pkg = stub_package(b"{}");
        let ctx = stub_context();
        let outcome = a.launch(&pkg, &ctx).await.expect("launch should succeed");
        match outcome {
            LaunchOutcome::Launched {
                instance_id,
                ecosystem,
                ..
            } => {
                assert!(instance_id.starts_with("linux_instance_"));
                assert_eq!(ecosystem, EcosystemRuntime::RuntimeLinuxNative);
            }
            other => panic!("expected Launched, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn windows_adapter_launch_returns_requires_vm() {
        let a = WindowsRuntimeAdapter::new();
        let pkg = stub_package(b"{}");
        let ctx = stub_context();
        let outcome = a.launch(&pkg, &ctx).await.expect("launch should succeed");
        match outcome {
            LaunchOutcome::RequiresVm { reason } => {
                assert_eq!(reason, "WSL2 or VM");
            }
            other => panic!("expected RequiresVm, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn android_adapter_launch_returns_requires_vm() {
        let a = AndroidRuntimeAdapter::new();
        let pkg = stub_package(b"{}");
        let ctx = stub_context();
        let outcome = a.launch(&pkg, &ctx).await.expect("launch should succeed");
        match outcome {
            LaunchOutcome::RequiresVm { reason } => {
                assert_eq!(reason, "ART container or VM");
            }
            other => panic!("expected RequiresVm, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn web_adapter_launch_app_kind_returns_launched() {
        let a = WebRuntimeAdapter::new();
        let pkg = stub_package(br#"{"kind": "APP"}"#);
        let ctx = stub_context();
        let outcome = a.launch(&pkg, &ctx).await.expect("launch should succeed");
        match outcome {
            LaunchOutcome::Launched { instance_id, .. } => {
                assert!(instance_id.starts_with("web_instance_"));
            }
            other => panic!("expected Launched, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn web_adapter_launch_adapter_kind_returns_incompatible() {
        let a = WebRuntimeAdapter::new();
        let pkg = stub_package(br#"{"kind": "ADAPTER"}"#);
        let ctx = stub_context();
        let outcome = a.launch(&pkg, &ctx).await.expect("launch should succeed");
        match outcome {
            LaunchOutcome::IncompatibleEcosystem {
                available_alternatives,
            } => {
                assert!(!available_alternatives.is_empty());
            }
            other => panic!("expected IncompatibleEcosystem, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn aios_native_adapter_launch_returns_launched() {
        let a = AiosNativeRuntimeAdapter::new();
        let pkg = stub_package(b"{}");
        let ctx = stub_context();
        let outcome = a.launch(&pkg, &ctx).await.expect("launch should succeed");
        match outcome {
            LaunchOutcome::Launched {
                instance_id,
                ecosystem,
                ..
            } => {
                assert!(instance_id.starts_with("aios_instance_"));
                assert_eq!(ecosystem, EcosystemRuntime::RuntimeLinuxNative);
            }
            other => panic!("expected Launched, got {other:?}"),
        }
    }

    // --- Stop behaviour ---

    #[tokio::test]
    async fn stop_is_idempotent_for_all_adapters() {
        let adapters: Vec<Box<dyn CompatibilityRuntimeAdapter>> = vec![
            Box::new(LinuxRuntimeAdapter::new()),
            Box::new(WindowsRuntimeAdapter::new()),
            Box::new(AndroidRuntimeAdapter::new()),
            Box::new(WebRuntimeAdapter::new()),
            Box::new(AiosNativeRuntimeAdapter::new()),
        ];
        for adapter in &adapters {
            adapter
                .stop("any_instance")
                .await
                .expect("stop should succeed");
        }
    }

    // --- available_capabilities ---

    #[test]
    fn linux_adapter_available_capabilities_non_empty() {
        let caps = LinuxRuntimeAdapter::new().available_capabilities();
        assert!(!caps.is_empty());
        assert!(caps.iter().any(|c| c.name == "native_elf_execution"));
    }

    #[test]
    fn windows_adapter_available_capabilities_non_empty() {
        let caps = WindowsRuntimeAdapter::new().available_capabilities();
        assert!(!caps.is_empty());
        assert!(caps.iter().any(|c| c.name == "win32_syscall_translation"));
    }

    #[test]
    fn android_adapter_available_capabilities_non_empty() {
        let caps = AndroidRuntimeAdapter::new().available_capabilities();
        assert!(!caps.is_empty());
        assert!(caps.iter().any(|c| c.name == "waydroid_container"));
    }

    // --- LaunchOutcome serde round-trip ---

    #[test]
    fn launch_outcome_serde_roundtrip_launched() {
        let outcome = LaunchOutcome::Launched {
            instance_id: "linux_instance_01j".into(),
            ecosystem: EcosystemRuntime::RuntimeLinuxNative,
            started_at: Utc::now(),
        };
        let json = serde_json::to_string(&outcome).expect("serialize");
        let round: LaunchOutcome = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(outcome, round);
    }

    #[test]
    fn launch_outcome_serde_roundtrip_requires_vm() {
        let outcome = LaunchOutcome::RequiresVm {
            reason: "WSL2 or VM".into(),
        };
        let json = serde_json::to_string(&outcome).expect("serialize");
        let round: LaunchOutcome = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(outcome, round);
    }

    #[test]
    fn launch_outcome_serde_roundtrip_incompatible() {
        let outcome = LaunchOutcome::IncompatibleEcosystem {
            available_alternatives: vec![
                EcosystemRuntime::RuntimeLinuxNative,
                EcosystemRuntime::RuntimeFlatpak,
            ],
        };
        let json = serde_json::to_string(&outcome).expect("serialize");
        let round: LaunchOutcome = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(outcome, round);
    }

    #[test]
    fn launch_outcome_serde_roundtrip_runtime_unavailable() {
        let outcome = LaunchOutcome::RuntimeUnavailable("no adapter for RUNTIME_MACOS_VM".into());
        let json = serde_json::to_string(&outcome).expect("serialize");
        let round: LaunchOutcome = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(outcome, round);
    }

    // --- detect_ecosystem ---

    #[test]
    fn detect_ecosystem_from_manifest_json() {
        let pkg = stub_package(br#"{"ecosystem": "RUNTIME_WINDOWS_PROTON"}"#);
        assert_eq!(
            detect_ecosystem(&pkg),
            EcosystemRuntime::RuntimeWindowsProton
        );
    }

    #[test]
    fn detect_ecosystem_defaults_to_linux_native() {
        let pkg = stub_package(b"not json");
        assert_eq!(detect_ecosystem(&pkg), EcosystemRuntime::RuntimeLinuxNative);
    }

    // --- LaunchContext + SubjectRef + RuntimeCapability serde ---

    #[test]
    fn launch_context_serde_roundtrip() {
        let ctx = LaunchContext {
            subject: SubjectRef {
                canonical_id: "human:lucky".into(),
            },
            sandbox_profile_id: Some("sbx_01abc".into()),
            recovery_mode: true,
            started_at: Utc::now(),
        };
        let json = serde_json::to_string(&ctx).expect("serialize");
        let round: LaunchContext = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(ctx.subject.canonical_id, round.subject.canonical_id);
        assert_eq!(ctx.sandbox_profile_id, round.sandbox_profile_id);
        assert!(round.recovery_mode);
    }

    #[test]
    fn runtime_capability_serde_roundtrip() {
        let cap = RuntimeCapability {
            name: "gpu_passthrough".into(),
            description: "Passthrough GPU access to the sandbox".into(),
        };
        let json = serde_json::to_string(&cap).expect("serialize");
        let round: RuntimeCapability = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cap, round);
    }
}
