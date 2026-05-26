//! S12.1 App Runtime Model — closed enums for ecosystem classification.
//!
//! Defines the twelve-target EcosystemRuntime, the honesty disclosure class,
//! the manifest translation strategies, recipe trust levels, and manifest
//! delta outcomes from the AI-assisted four-phase setup mechanism.

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

// ---------------------------------------------------------------------------
// S12.1 §3.1 — EcosystemRuntime (12 closed values)
// ---------------------------------------------------------------------------

/// S12.1 §3.1 — the closed set of foreign-app runtimes AIOS can target.
/// Each runtime is itself an AIOS package of `PackageKind = ADAPTER`.
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
pub enum EcosystemRuntime {
    /// Direct ELF execution; full first-class support.
    RuntimeLinuxNative,
    /// Flatpak rebuild; capabilities from manifest.json finishes.
    RuntimeFlatpak,
    /// Extract + run; no embedded manifest.
    RuntimeAppimage,
    /// Snap rebuild; canonical-trust path.
    RuntimeSnap,
    /// Full Linux distro environment as one AIOS app.
    RuntimeDistrobox,
    /// Wine/Proton per-app prefix; Win32 syscall translation.
    RuntimeWindowsProton,
    /// KVM + QEMU + Windows guest for apps Wine cannot run.
    RuntimeWindowsVm,
    /// Waydroid LXC + AOSP image; Android apps without Google services.
    RuntimeAndroidWaydroid,
    /// KVM + AOSP + GMS image; Android apps requiring Play Services.
    RuntimeAndroidVmWithGms,
    /// Darling translation layer; subset of macOS CLI apps.
    RuntimeMacosDarling,
    /// KVM (OSX-KVM) + macOS guest.
    RuntimeMacosVm,
    /// Bridge to the operator's actual Apple device for iOS apps.
    RuntimeRemoteAppleBridge,
}

// ---------------------------------------------------------------------------
// S12.1 §3.2 — EcosystemHonestyClass (4 closed values)
// ---------------------------------------------------------------------------

/// S12.1 §3.2 — constitutionally required disclosure on every package install.
/// The L7 marketplace surface MUST display this class.
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
pub enum EcosystemHonestyClass {
    /// Runs as a first-class AIOS app; no translation surprise.
    FullySupported,
    /// Works for many apps within the runtime; specific issues disclosed.
    PartiallySupported,
    /// Needs a full VM runtime; performance/RAM/disk implications disclosed.
    RequiresVm,
    /// Explicit "this cannot run on non-native hardware."
    NotRunnableOnNonNative,
}

// ---------------------------------------------------------------------------
// S12.1 §3.3 — ManifestTranslationStrategy (8 closed values)
// ---------------------------------------------------------------------------

/// S12.1 §3.3 — how the Phase B proposer derives capabilities from a foreign
/// artifact. Eight strategies, selected automatically based on artifact format.
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
pub enum ManifestTranslationStrategy {
    /// Parse AndroidManifest.xml `<uses-permission>` entries.
    AndroidManifestXml,
    /// Parse Flatpak manifest.json finishes section.
    FlatpakManifestJson,
    /// Parse snapcraft.yaml plugs/slots.
    SnapcraftYaml,
    /// Pull ProtonDB + WineHQ AppDB rating and recipe metadata.
    ProtonRecipe,
    /// Phase A observation in a max-restricted Wine prefix.
    WinePrefixProbe,
    /// Phase A behavioral extraction from AppImage.
    AppimageBehavioral,
    /// Parse macOS Info.plist Entitlements.
    MacInfoPlist,
    /// No local translation; bridge target configured for remote iOS device.
    IosRemoteBridge,
}

// ---------------------------------------------------------------------------
// S12.1 §3.5 — RecipeTrustClass (4 closed values)
// ---------------------------------------------------------------------------

/// S12.1 §3.5 — trust class for a Community Recipe Registry entry.
/// Derived from publisher tier and recipe-specific reputation history.
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
pub enum RecipeTrustClass {
    /// Authored or curated by AIOS_ROOT or VERIFIED publishers.
    RecipeAiosCurated,
    /// Authored by COMMUNITY publishers or individual operators.
    RecipeCommunity,
    /// One-shot import from upstream; attribution preserved.
    RecipeImported,
    /// Recipe with capability-lie history; auto-quarantined.
    RecipeQuarantined,
}

// ---------------------------------------------------------------------------
// S12.1 §3.6 — ManifestDeltaOutcome (4 closed values)
// ---------------------------------------------------------------------------

/// S12.1 §3.6 — outcome of the Phase D continuous-refinement loop.
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
pub enum ManifestDeltaOutcome {
    /// AI emitted a manifest delta proposal; awaiting operator approval.
    DeltaProposed,
    /// Operator approved; new versioned manifest is signed and recorded.
    DeltaApproved,
    /// Operator rejected; existing manifest remains in force.
    DeltaRejected,
    /// Phase D observation showed undeclared capability; auto-quarantine.
    DeltaCapabilityLie,
}
