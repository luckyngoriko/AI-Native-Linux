#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![allow(clippy::doc_markdown)]

//! AIOS L6 Apps, Packages, Compatibility — typed skeleton.
//!
//! This crate provides the closed-enum vocabularies and newtype identifiers
//! for the five L6 sub-specs:
//!
//! - **S12.1** App Runtime Model → [`ecosystem`]
//! - **S12.2** Package Object Model → [`package`]
//! - **S12.3** Compatibility Runtime → [`orchestration`]
//! - **S12.4** Compatibility Knowledge → [`app_profile`]
//! - **S6.5**  Session Container Model → [`session`]
//!
//! Every enum is closed; adding a variant is a versioned spec change.

pub mod app_profile;
pub mod ecosystem;
pub mod error;
pub mod orchestration;
pub mod package;
pub mod runtime;
pub mod session;

// Re-export all public types at crate root for convenience.
pub use app_profile::{
    AppProfile, CompatibilityRating, EvidenceLevel, KnownIssueClass, ProfileRetiredReason,
    ProfileVisibility, RatingDimension,
};
pub use ecosystem::{
    EcosystemHonestyClass, EcosystemRuntime, ManifestDeltaOutcome, ManifestTranslationStrategy,
    RecipeTrustClass,
};
pub use error::AppsError;
pub use orchestration::{
    LaunchOutcome, OrchestrationKind, VMFallbackKind, WaydroidIsolationLevel, WinePrefixKind,
};
pub use package::{
    PackageContentKind, PackageId, PackageObjectKind, PackageObjectState, PackageRecord,
    RollbackKind,
};
pub use runtime::{
    AppManifestProposal, AppRuntime, InMemoryAppRuntime, ObservedBehavior, SyscallClass,
};
pub use session::{
    SessionContainerMode, SessionContainerRuntime, SessionContainerState, SessionFailureClass,
    SessionId, SessionRecord, StreamProtocol,
};
