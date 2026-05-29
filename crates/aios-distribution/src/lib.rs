//! `aios-distribution` — core types for the AIOS Distribution layer (S11.1, schema
//! `aios.distribution.v1alpha1`).
//!
//! This crate implements the **typed skeleton** for the Repository Model + Trust Roots
//! defined in `L10_Distribution_Ecosystem_Marketplace/01_repository_model.md`. It is
//! the contract surface that every later L10 sub-spec builds on.
//!
//! ## Scope of T-187 (M19 open)
//!
//! - [`PublisherTrustLevel`] closed enum — 5 tiers (S11.1 §3.1).
//! - [`RepositoryKind`] closed enum — 5 repository classes (S11.1 §3.2).
//! - [`UpdateChannel`] closed enum — 4 channels (S11.1 §3.3).
//! - [`PackageKind`] closed enum — 9 kinds (S11.1 §3.4).
//! - [`InstallScope`] closed enum — 4 scopes (S11.1 §3.5).
//! - [`PackageInstallState`] closed FSM — 10 states (S11.1 §3.7).
//! - [`PackageVerificationResult`] closed enum — 10 outcomes (S11.1 §3.8).
//! - [`MirrorSemantic`] closed enum — 3 semantics (S11.1 §3.9).
//! - [`TakedownReason`] closed enum — 7 reasons (S11.1 §3.10).
//! - 6 identifier newtypes ([`PackageId`], [`PublisherId`], [`PublisherRootId`],
//!   [`PackageSigningKeyId`], [`RepositoryId`], [`ManifestId`]).
//! - [`DistributionError`] + [`DistributionErrorCode`] — 15-code error taxonomy.
//!
//! ## Deferred to T-188+
//!
//! - Trust chain logic (T-188).
//! - Manifest verification (T-189).
//! - Install FSM (T-190).
//! - Mirror blacklisting (T-191).
//! - Deplatform discipline (T-192).
//! - Update channel enforcement (T-193).
//! - Capability lie audit (T-194).
//! - gRPC surface (T-195).
//! - Evidence emission (T-196).
//! - Cross-crate wiring (T-197).
//!
//! ## Constitutional invariants enforced here
//!
//! - **No `unsafe`, no `panic!`, no `unwrap`/`expect`, no `todo!`/`unimplemented!`** — workspace
//!   lints forbid them; every fallible path returns a typed `Result`.

#![forbid(unsafe_code)]

pub mod canonical;
pub mod catalog;
pub mod error;
pub mod ids;
pub mod install_state;
pub mod manifest;
pub mod manifest_pipeline;
pub mod mirror;
pub mod package_kind;
pub mod repository;
pub mod takedown;
pub mod trust;
pub mod trust_chain;
pub mod verifier;

pub use canonical::{content_hash, manifest_canonical_hash, signing_payload};
pub use catalog::{PublisherCatalog, SigningKeyCatalog};
pub use error::{DistributionError, DistributionErrorCode};
pub use ids::{
    ManifestId, PackageId, PackageSigningKeyId, PublisherId, PublisherRootId, RepositoryId,
};
pub use install_state::{PackageInstallState, PackageVerificationResult};
pub use manifest::{NetworkManifestRef, PackageManifest, SandboxProfileRef};
pub use manifest_pipeline::{is_eol, validate_fields, verify_manifest, ManifestField};
pub use mirror::MirrorSemantic;
pub use package_kind::{InstallScope, PackageKind};
pub use repository::{RepositoryKind, UpdateChannel};
pub use takedown::TakedownReason;
pub use trust::PublisherTrustLevel;
pub use trust_chain::{
    canonical_depth, AiosRootKey, LinkSignature, PackageSigningKey, PublisherRoot, SignedPayload,
    MAX_CHAIN_DEPTH,
};
pub use verifier::TrustChainVerifier;

/// Code version marker for T-198 closure invariants.
///
/// This constant anchors the crate's identity at compile time so that closure
/// tests (T-198) can verify the distribution layer shipped with the correct
/// typed contract before cross-crate wiring lands in T-197.
pub const DEFAULT_CODE_VERSION: &str = "aios-distribution/0.0.1-T189";
