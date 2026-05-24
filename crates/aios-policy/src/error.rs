//! Error taxonomy for the Policy Kernel decision pipeline.
//!
//! These variants are the typed Rust mirror of the canonical short codes that appear
//! in [`crate::decision::PolicyDecision::reason_code`] for the short-circuit cases of
//! S2.3 §3 (subject hydration, enrichment) and §15 (bundle load / schema validation).
//!
//! `reason_code` itself is a String on the decision because future decisions may carry
//! codes from bundle authors (e.g. `"ScopedAllow"`); this enum covers only the
//! pipeline-internal failures the crate is responsible for surfacing.

use thiserror::Error;

/// Failure modes the Policy Kernel itself raises (as opposed to bundle-authored deny
/// reasons).
///
/// Stays a small surface in T-016; the bundle-evaluation error set (rule conflict,
/// unknown constraint, condition parse error, …) is added in T-017+ alongside the
/// evaluator implementation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PolicyError {
    /// Subject hydration via L4 identity failed (unknown, expired, or revoked subject).
    /// Decision short-circuits to `DENY` with `reason_code = SubjectUnauthenticated`
    /// per S2.3 §7.
    #[error("subject hydration failed: subject unauthenticated")]
    SubjectUnauthenticated,

    /// Resource enrichment via AIOS-FS or adapter manifest is unavailable.
    /// Decision short-circuits to `DENY` with `reason_code = EnrichmentUnavailable`
    /// per S2.3 §8.
    #[error("resource enrichment unavailable")]
    EnrichmentUnavailable,

    /// Loading the policy bundle (signed YAML / proto archive) failed at I/O,
    /// signature, or version-pin level (S2.3 §15).
    #[error("policy bundle load failed: {reason}")]
    BundleLoad {
        /// Human-readable English description of the load failure.
        reason: String,
    },

    /// The policy bundle parsed but is structurally invalid: unknown condition field,
    /// unknown constraint name, malformed predicate (S2.3 §9 / §10 / §15).
    #[error("policy bundle schema invalid: {detail}")]
    SchemaInvalid {
        /// Human-readable English description of the schema violation.
        detail: String,
    },

    /// Constraints attached to a decision violated a field-level invariant from
    /// S2.3 §10 / §13.2 (e.g. zero `ttl_seconds`, zero `max_runtime_seconds`,
    /// past `expires_at`). Raised by [`crate::Constraints::validate`].
    #[error("constraints invalid: {0}")]
    ConstraintsInvalid(String),
}
