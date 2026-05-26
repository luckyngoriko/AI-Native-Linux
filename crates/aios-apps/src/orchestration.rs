//! S12.3 Compatibility Runtime — closed enums for orchestration.
//!
//! Defines the OrchestrationKind selection, LaunchOutcome reporting,
//! Wine prefix isolation strategy, Waydroid container isolation level,
//! and VM fallback justification.

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

// ---------------------------------------------------------------------------
// S12.3 §3.1 — OrchestrationKind (8 closed values)
// ---------------------------------------------------------------------------

/// S12.3 §3.1 — the distinct combination of (EcosystemRuntime, reuse strategy,
/// isolation strategy) selected by the orchestrator per launch. Eight kinds.
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
pub enum OrchestrationKind {
    /// Direct ELF launch under SUBPROCESS_FORK with composed sandbox applied.
    NativeDirectLaunch,
    /// Flatpak bwrap runtime invoked inside the AIOS-composed sandbox.
    FlatpakFork,
    /// AppImage extracted to a per-app private mount; entrypoint launched.
    AppimageExtract,
    /// Wine/Proton prefix created fresh per launch.
    WinePrefixNew,
    /// Wine/Proton prefix from a prior launch is reused.
    WinePrefixExisting,
    /// Waydroid LXC container created fresh; APK launched inside.
    WaydroidContainerNew,
    /// Waydroid LXC container from prior launch reused.
    WaydroidContainerExisting,
    /// KVM guest booted; app entrypoint invoked via guest agent.
    KvmVmBoot,
}

// ---------------------------------------------------------------------------
// S12.3 §3.2 — LaunchOutcome (7 closed values)
// ---------------------------------------------------------------------------

/// S12.3 §3.2 — the outcome of a single `app.launch` action. Seven values.
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
pub enum LaunchOutcome {
    /// App process started inside sandbox; health signal received.
    Launched,
    /// Required EcosystemRuntime adapter package is not installed or inactive.
    LaunchFailedRuntimeMissing,
    /// S3.2 ComposeProfile or ApplyProfile returned an error.
    LaunchFailedSandboxDeny,
    /// S8.2 GpuPolicy denied the requested GPU class.
    LaunchFailedGpuDeny,
    /// NetworkPolicy denied an endpoint the runtime adapter itself requires.
    LaunchFailedNetworkDeny,
    /// Post-launch verification probe did not receive expected health signal.
    LaunchFailedVerifyFail,
    /// Launch did not complete within the OrchestrationKind's hard timeout.
    TimedOut,
}

// ---------------------------------------------------------------------------
// S12.3 §3.3 — WinePrefixKind (3 closed values)
// ---------------------------------------------------------------------------

/// S12.3 §3.3 — selects the prefix isolation strategy for
/// `RUNTIME_WINDOWS_PROTON` launches. Three kinds.
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
pub enum WinePrefixKind {
    /// Fresh prefix per launch. Default for transient apps.
    PerAppFresh,
    /// Persistent prefix scoped to a single app; reused across launches.
    PerAppPersistent,
    /// Shared prefix scoped to a single user across multiple apps.
    SharedPerUser,
}

// ---------------------------------------------------------------------------
// S12.3 §3.4 — WaydroidIsolationLevel (3 closed values)
// ---------------------------------------------------------------------------

/// S12.3 §3.4 — selects the container isolation strategy for
/// `RUNTIME_ANDROID_WAYDROID` launches. Three levels.
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
pub enum WaydroidIsolationLevel {
    /// Each app runs in its own Waydroid container; highest isolation.
    PerApp,
    /// All Android apps for a single user share one Waydroid container.
    PerUser,
    /// All Android apps for a single AIOS group share one container.
    PerGroup,
}

// ---------------------------------------------------------------------------
// S12.3 §3.5 — VMFallbackKind (4 closed values)
// ---------------------------------------------------------------------------

/// S12.3 §3.5 — the documented justification for using a VM-based
/// EcosystemRuntime. Four kinds.
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
pub enum VMFallbackKind {
    /// Game or app ships kernel-level anti-cheat that refuses to run under Wine.
    WindowsAntiCheat,
    /// App requires a Windows kernel driver.
    KernelDriver,
    /// App requires hardware emulation impractical through Wine or Waydroid.
    ExoticHardware,
    /// Operator explicitly chose VM fallback despite Wine/Waydroid availability.
    OperatorForced,
}
