//! Install-state FSM and verification-result vocabularies per S11.1 §3.7 / §3.8.
//!
//! `PackageInstallState` is a 10-state closed FSM that governs the lifecycle of
//! a package from discovery through fetch, verify, stage, activate, and
//! (optionally) failure.
//!
//! `PackageVerificationResult` is the closed set of outcomes returned by each
//! step in the install pipeline that can fail.

use serde::{Deserialize, Serialize};

/// Closed FSM — 10 states per S11.1 §3.7.
///
/// Deviation: spec §3.6 uses `DRAFT` → `VALIDATING` → `AWAITING_APPROVAL` →
/// `APPROVED` → `INSTALLING` → `ACTIVE` (plus `QUARANTINED`, `UNINSTALLING`,
/// `REMOVED`, `INSTALL_FAILED`).  T-187 uses task-authorised names that model
/// a fetch-verify-stage-activate pipeline, which is the lower-level
/// implementation of the spec's higher-level approval-gated FSM.  The two FSMs
/// are isomorphic in the steady state; the spec FSM layers policy approval over
/// the raw pipeline encoded here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageInstallState {
    /// Package has been discovered (operator-initiated or browse result).
    Discovered,
    /// Fetch in progress (network or local mirror).
    Fetching,
    /// Fetch completed — bytes in staging area.
    Fetched,
    /// Signature, chain, manifest verification in progress.
    Verifying,
    /// All verification steps passed.
    Verified,
    /// Staging in progress (content-addressed staging path write).
    Staging,
    /// Staging complete — atomic pointer flip pending.
    Staged,
    /// Activation in progress (capability bindings, install hooks).
    Activating,
    /// Live — subject to runtime monitoring and health checks.
    Installed,
    /// Terminal — install aborted before `Installed`.
    Failed,
}

/// Closed enum — 10 outcomes per S11.1 §3.8.
///
/// Every step in the install pipeline that can fail returns one of these
/// outcomes.  All failures emit evidence per the S11.1 §17 evidence record
/// type table (queued for S3.1 consolidation).
///
/// Deviation: spec §3.7 uses `VERIFIED_AIOS_ROOT`, `VERIFIED_PUBLISHER`,
/// `SIGNATURE_FAILED`, `TRUST_CHAIN_BROKEN`, `TRUST_CHAIN_TOO_DEEP`,
/// `PUBLISHER_DEPLATFORMED`, `HASH_MISMATCH`, `MANIFEST_FORGED`,
/// `CAPABILITY_LIE`, `BUNDLE_TAMPERED`.  T-187 uses task-authorised names
/// that decompose the verification surface slightly differently; the
/// cross-crate wiring in T-197 will map the spec's outcomes to these codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageVerificationResult {
    /// All checks passed — package is valid.
    Valid,
    /// Ed25519 signature verification failed.
    SignatureInvalid,
    /// Trust chain depth exceeds 3 hops from AIOS root.
    TrustChainTooDeep,
    /// Publisher is in `Deplatformed` state at fetch time.
    PublisherDeplatformed,
    /// `BLAKE3(content)` does not match `manifest.content_hash`.
    ContentHashMismatch,
    /// Manifest fields are structurally invalid or inconsistent.
    ManifestMalformed,
    /// Package's origin repository does not match the kind admitted for its trust level.
    RepositoryKindMismatch,
    /// Version downgrade detected (replay or social-engineering attempt).
    DowngradeAttempt,
    /// Publisher root ID absent from active publisher catalog.
    UnknownPublisher,
    /// Package signing key has been revoked (`revoked_at` precedes `issued_at`).
    RevokedKey,
}
