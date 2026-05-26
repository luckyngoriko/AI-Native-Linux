//! S12.4 Compatibility Knowledge — closed enums for per-app profile database.
//!
//! Defines the CompatibilityRating axis, the eight RatingDimension axes,
//! EvidenceLevel for corroboration quality, ProfileVisibility for sharing
//! scope, ProfileRetiredReason, and the KnownIssueClass taxonomy.

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

use crate::ecosystem::{EcosystemHonestyClass, EcosystemRuntime, RecipeTrustClass};

// ---------------------------------------------------------------------------
// S12.4 §3.1 — CompatibilityRating (5 closed values, ordinal best→worst)
// ---------------------------------------------------------------------------

/// S12.4 §3.1 — the five-point compatibility rating scale, deliberately
/// mirroring the ProtonDB convention. Ordinal from best to worst.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CompatibilityRating {
    /// Does not run; crashes on launch or corrupts state.
    Borked,
    /// Runs with a significant compromise; operator may proceed with caveats.
    Bronze,
    /// Runs with a minor, persistent compromise.
    Silver,
    /// Runs well after one trivial tweak; caveat documented.
    Gold,
    /// Runs as well as a native first-class implementation.
    Platinum,
}

// ---------------------------------------------------------------------------
// S12.4 §3.2 — RatingDimension (8 closed values)
// ---------------------------------------------------------------------------

/// S12.4 §3.2 — the axis along which a CompatibilityRating is measured.
/// Eight independent dimensions.
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
pub enum RatingDimension {
    /// Does the app reach an interactive state without manual intervention?
    LaunchReliability,
    /// Does the app stay running through a representative session?
    GameplayStability,
    /// Are graphical primitives rendered with intended fidelity?
    VisualQuality,
    /// Does the app produce/capture audio correctly?
    AudioFunctionality,
    /// Are inputs delivered correctly at perceived-native latency?
    InputHandling,
    /// Does network behaviour stay within declared NetworkOutboundManifest?
    NetworkBehavior,
    /// Are saves and persistent state written/read/migrated without corruption?
    SaveStateCorrectness,
    /// Can DRM/anti-cheat be honestly mediated by the runtime?
    DrmBehavior,
}

// ---------------------------------------------------------------------------
// S12.4 §3.3 — EvidenceLevel (4 closed values, ordered weakest→strongest)
// ---------------------------------------------------------------------------

/// S12.4 §3.3 — governs how heavily a per-dimension rating contributes to
/// aggregation and how visibly the L7 surface flags the rating.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceLevel {
    /// Single operator's stated opinion without runtime evidence.
    SelfReported,
    /// Single operator's rating with attached runtime evidence.
    SingleOperatorObserved,
    /// Three or more independent operators agree within one bucket.
    MultiOperatorCorroborated,
    /// Publisher at AIOS_VERIFIED trust level attests the rating.
    VerifiedPublisher,
}

// ---------------------------------------------------------------------------
// S12.4 §3.4 — ProfileVisibility (3 closed values)
// ---------------------------------------------------------------------------

/// S12.4 §3.4 — the visibility scope of a per-operator contribution.
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
pub enum ProfileVisibility {
    /// Retained locally; never aggregated or shared.
    PersonalOnly,
    /// Shared only within the operator's AIOS group.
    GroupInternal,
    /// Published to AIOS_COMMUNITY_REPO; visible to all operators.
    Public,
}

// ---------------------------------------------------------------------------
// S12.4 §3.5 — ProfileRetiredReason (6 closed values)
// ---------------------------------------------------------------------------

/// S12.4 §3.5 — why a CompatibilityProfile was retired. Six values.
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
pub enum ProfileRetiredReason {
    /// Upstream publisher delisted or sunset the app.
    RetiredAppDelisted,
    /// Recipe class superseded by a structurally different class.
    RetiredRecipeReplaced,
    /// AIOS-root governance retired due to evidence-grade abuse.
    RetiredQuarantinedByAiosRoot,
    /// Associated publisher was deplatformed.
    RetiredPublisherDeplatformed,
    /// Sole contributing operator requested retirement.
    RetiredOperatorRequest,
    /// Recipe's honesty class violated; profile retired alongside.
    RetiredDueToHonestyViolation,
}

// ---------------------------------------------------------------------------
// S12.4 §4.2.1 — KnownIssueClass (11 closed values)
// ---------------------------------------------------------------------------

/// S12.4 §4.2.1 — the closed taxonomy of known issue classes for
/// per-app compatibility profiles. Eleven values.
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
pub enum KnownIssueClass {
    /// App crashes immediately on launch.
    IssueCrashOnLaunch,
    /// App crashes intermittently during use.
    IssueCrashIntermittent,
    /// A specific feature is missing or broken.
    IssueFeatureMissing,
    /// Visual rendering glitches (flickering, missing textures).
    IssueVisualGlitch,
    /// Audio glitches (dropouts, distortion, missing channels).
    IssueAudioGlitch,
    /// Input latency perceived as higher than native.
    IssueInputLatencyHigh,
    /// Online features fall back or fail.
    IssueNetworkFallback,
    /// Save state corruption or loss observed.
    IssueSaveStateLoss,
    /// DRM rejection preventing launch or feature access.
    IssueDrmRejection,
    /// Anti-cheat rejection (kernel-level or userspace).
    IssueAnticheatRejection,
    /// Runtime behaviour contradicts declared EcosystemHonestyClass.
    IssueHonestyClassDrift,
}

// ---------------------------------------------------------------------------
// AppProfile — top-level struct for a compatibility profile
// ---------------------------------------------------------------------------

/// A compatibility profile aggregating per-dimension ratings, known issues,
/// and reputation metadata for an (app, runtime) pair.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppProfile {
    /// The canonical app identifier.
    pub app_id: String,
    /// The ecosystem runtime this profile is scoped to.
    pub ecosystem_runtime: EcosystemRuntime,
    /// The recipe trust class currently rated.
    pub current_recipe_trust_class: RecipeTrustClass,
    /// The worst non-applicable dimension rating.
    pub headline_rating: CompatibilityRating,
    /// The lowest evidence level among headline dimensions.
    pub headline_evidence_level: EvidenceLevel,
    /// The worst dimension name (for operator display).
    pub worst_dimension: RatingDimension,
    /// The ecosystem honesty class disclosed for this app.
    pub ecosystem_honesty_class: EcosystemHonestyClass,
}
