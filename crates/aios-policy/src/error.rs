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

    /// A §9 condition source string failed to parse — unknown field, unknown
    /// namespace, unknown operator, malformed value list, disallowed `or` / `not`
    /// / parens. Raised by [`crate::conditions_parser::parse`].
    ///
    /// Carries the rendered [`crate::conditions_parser::ConditionParseError`]
    /// message verbatim so the pipeline can include it in
    /// [`crate::decision::PolicyDecision::reason`].
    #[error("condition parse failed: {0}")]
    ConditionParse(String),

    /// A parsed §9 condition could not be evaluated against the runtime context
    /// (type-mismatched comparison, ordering operator on a bool field, mixed-type
    /// `in` value list). Raised by [`crate::conditions_eval::evaluate`].
    #[error("condition eval failed: {0}")]
    ConditionEval(String),

    /// The serialised policy bundle JSON failed to parse, or one of its rules
    /// carried a malformed `condition` string. Raised by
    /// [`crate::bundle_loader::BundleLoader::load_from_bytes`] (S2.3 §12.4 /
    /// §19.1 / §15).
    ///
    /// The carried detail is the canonical short reason — e.g.
    /// `"rule allow_X condition: unknown namespace"` for a per-rule parser
    /// failure, or `"JSON deserialise: …"` for a top-level body failure.
    #[error("invalid policy bundle: {0}")]
    InvalidPolicyBundle(String),

    /// The bundle's `signature_ed25519` did not verify against the publisher
    /// verifying key fetched from the trust store keyed by `signing_authority`.
    /// Raised by [`crate::bundle_loader::BundleLoader::load_from_bytes`] per
    /// S2.3 §12.3 ("Bundle signature failure → engine enters degraded mode").
    #[error("bundle signature invalid")]
    BundleSignatureInvalid,

    /// Version pinning was requested on the loader and the bundle's
    /// `bundle_version` did not match the pinned expectation. Raised by
    /// [`crate::bundle_loader::BundleLoader::load_from_bytes`] per S2.3 §12.2
    /// + §13.1 (determinism anchor).
    #[error("bundle version mismatch: expected {expected}, found {found}")]
    BundleVersionMismatch {
        /// Pinned version the loader was configured to require.
        expected: String,
        /// Version actually present in the bundle body.
        found: String,
    },

    /// The bundle's declared `signing_authority` was not present in the
    /// loader's trust store. Raised by
    /// [`crate::bundle_loader::BundleLoader::load_from_bytes`] per S2.3 §12.3
    /// ("Bundle signature must verify against the publisher key in the AIOS
    /// trust store").
    #[error("bundle signed by unknown authority: {0}")]
    BundleUnknownAuthority(String),
}
