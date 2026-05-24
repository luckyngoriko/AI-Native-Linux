//! Policy bundle on-the-wire data model — S2.3 §11.2 + §12.
//!
//! A [`PolicyBundle`] is the serialised, signed unit of policy distribution. The
//! shape mirrors the spec §11.2 proto `PolicyRule` (rules are the core payload)
//! wrapped in the §12 distribution envelope (`bundle_version`, `bundle_id`,
//! `signing_authority`, `signature_ed25519`, `created_at`).
//!
//! ## Wire format
//!
//! The bundle is distributed as a single JSON document. The §12.1 spec lays out
//! a directory layout (`manifest.json` + `rules/*.yaml` + `signatures/*.sig`); the
//! rev.2 first-pass loader collapses that into a single JSON envelope so the
//! signature can be verified over a single canonical byte sequence with no
//! tar/zip framing concerns. The §12.1 multi-file layout remains the spec target
//! and is the natural extension when a packager wraps this single-file shape.
//!
//! ## Determinism (S2.3 §13.1)
//!
//! All fields use `#[serde(deny_unknown_fields)]` and field ordering is the
//! Rust struct declaration order — `serde_json` preserves that order on
//! serialisation. The same bundle bytes therefore parse to a byte-equal
//! [`PolicyBundle`] on every architecture (per the §13 determinism contract).
//!
//! ## Rule shape vs spec §11.2
//!
//! Spec §11.2 defines a `PolicyRule` proto with `subjects[]`, `actions[]`,
//! `conditions[]` (multiple conditions, `AND`-ed together — see §9.1 grammar:
//! `condition_set ::= predicate ("and" predicate)*`), `effect`, `priority`,
//! `constraints`, `approval`, `metadata`. We mirror that surface here exactly,
//! with the additional `reason_code` field (the canonical short code emitted on
//! the decision's `reason_code` per §4) so bundle authors can attach a stable
//! audit label to each rule.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::constraints::{ApprovalRequirement, Constraints};

// ---------------------------------------------------------------------------
// RuleEffect — closed (S2.3 §11.2 `RuleEffect`)
// ---------------------------------------------------------------------------

/// The terminal direction a matching rule contributes to the precedence ladder
/// (S2.3 §11.2 `RuleEffect`).
///
/// `Unspecified` is reserved for proto3 wire compatibility (matches
/// `RULE_EFFECT_UNSPECIFIED = 0`) and is rejected by the loader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuleEffect {
    /// Proto3 zero-value sentinel; never accepted by the loader.
    Unspecified,
    /// Rule contributes to the scoped-allow tier (S2.3 §5 step 5).
    Allow,
    /// Rule contributes to the scoped-deny tier (S2.3 §5 step 4).
    Deny,
}

// ---------------------------------------------------------------------------
// RuleScope — closed (T-022 explicit scope tier)
// ---------------------------------------------------------------------------

/// The precedence tier this rule is authored against.
///
/// T-022 — orthogonal label over the spec §5 ladder so the loader / pipeline
/// can route rules without re-deriving the tier from `subjects[]` /
/// `actions[]`.
///
/// The five values mirror the §5 tiers that admit bundle-authored rules
/// (tiers 1–3 are constitutional / engine-built and not addressable by
/// bundles; tiers 6–7 are the post-evaluation defaults). Bundles populate
/// tiers 4 and 5; the additional `Global`, `PerSubjectType`, and
/// `PerActionTarget` labels are routing hints for the matcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuleScope {
    /// Rule applies to every subject / every action — the broadest matcher.
    Global,
    /// Rule applies to a closed subject-type class (e.g. all AI subjects).
    PerSubjectType,
    /// Rule applies to a closed action-target class (e.g. all `service.*`).
    PerActionTarget,
    /// Rule applies to a single named subject (e.g. `human:lucky`).
    PerSubject,
    /// Rule applies to a single named action (e.g. `service.restart`).
    PerAction,
}

// ---------------------------------------------------------------------------
// PolicyRule — S2.3 §11.2
// ---------------------------------------------------------------------------

/// A single bundle-authored rule (S2.3 §11.2).
///
/// The `conditions` field holds a list of §9.1 source strings; each is
/// independently parsed by [`crate::conditions_parser::parse`] at bundle-load
/// time and the WHOLE bundle is rejected on the first per-rule parse failure
/// (S2.3 §19.1 "rules parse").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRule {
    /// Stable rule identifier (S2.3 §11.2 field 1). Echoed into the decision's
    /// audit chain when this rule contributes to the outcome.
    pub rule_id: String,

    /// Precedence-tier routing hint (T-022 — see [`RuleScope`]).
    pub scope: RuleScope,

    /// ALLOW vs DENY direction (S2.3 §11.2 field 2).
    pub effect: RuleEffect,

    /// Higher number = evaluated earlier within the same precedence step
    /// (S2.3 §11.2 field 3). Default 0.
    #[serde(default)]
    pub priority: i32,

    /// Subject matchers — canonical subject ids or `group:...` aliases
    /// (S2.3 §11.2 field 4). Empty list = matches every subject.
    #[serde(default)]
    pub subjects: Vec<String>,

    /// Action matchers (S2.3 §11.2 field 5). Empty list = matches every action.
    #[serde(default)]
    pub actions: Vec<String>,

    /// Zero or more §9.1 condition source strings, `AND`-ed together (S2.3 §11.2
    /// field 6 + §9.1 grammar). Each string is parsed independently at load
    /// time; any per-string parse failure rejects the whole bundle.
    #[serde(default)]
    pub conditions: Vec<String>,

    /// Execution constraints attached when this rule's effect is `Allow`
    /// (S2.3 §11.2 field 7 / §10). Optional — absent = no rule-level
    /// constraint contribution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<Constraints>,

    /// Approval requirement (S2.3 §11.2 field 8 / §15). Optional — absent =
    /// no approval contribution from this rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval: Option<ApprovalRequirement>,

    /// Canonical short reason code echoed onto the decision's `reason_code`
    /// when this rule wins evaluation (S2.3 §4 field 7). Must be a stable,
    /// audit-friendly identifier such as `"ScopedAllow"` or
    /// `"BundleDeny_AIInstall"`.
    pub reason_code: String,
}

// ---------------------------------------------------------------------------
// PolicyBundle — S2.3 §12 envelope
// ---------------------------------------------------------------------------

/// A signed, version-pinned bundle of policy rules ready for distribution
/// (S2.3 §12).
///
/// ## Field roles
///
/// - `bundle_version` — content-addressed `polb_<hex>` identifier (S2.3 §12.2).
///   This is the determinism anchor (§13.1) and the cache key component (§13.3).
/// - `bundle_id` — operator-facing human label distinct from the content hash
///   (e.g. `"user-base.v1"`).
/// - `created_at` — RFC 3339 UTC timestamp of bundle authorship.
/// - `signing_authority` — string key into the loader's trust store; selects
///   the [`ed25519_dalek::VerifyingKey`] used to verify `signature_ed25519`.
/// - `signature_ed25519` — Ed25519 signature over the canonical body bytes
///   (the `serde_json` serialisation of `signed_body()` — see
///   [`crate::bundle_loader`]). 64 raw bytes.
/// - `rules` — the bundle payload (S2.3 §11.2). Empty list is valid (an empty
///   bundle correctly produces a default-deny decision per §5 step 6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyBundle {
    /// Content-addressed version id `polb_<hex_lower(BLAKE3(canonical_body))>[:32]`
    /// (S2.3 §12.2).
    pub bundle_version: String,

    /// Operator-facing label distinct from `bundle_version` (e.g. `"user-base.v1"`).
    pub bundle_id: String,

    /// Authorship timestamp; serialised RFC 3339.
    pub created_at: DateTime<Utc>,

    /// Trust-store key for the publisher verifying key (S2.3 §12.3).
    pub signing_authority: String,

    /// Raw 64-byte Ed25519 signature over the canonical body bytes
    /// (`SignedBundleBody`). On the JSON wire this field is base64-STANDARD
    /// encoded by the [`mod@crate::bundle_loader`] codec — see the loader for
    /// the exact serialisation pipeline.
    #[serde(with = "base64_bytes")]
    pub signature_ed25519: Vec<u8>,

    /// Bundle payload — zero or more bundle-authored rules.
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
}

impl PolicyBundle {
    /// Renders the canonical signed body (every field except the signature
    /// itself). The bytes returned are the **exact** input the publisher key
    /// is expected to have signed; the loader feeds these into the Ed25519
    /// verifier.
    ///
    /// Determinism: `serde_json` preserves struct field declaration order, the
    /// inner [`SignedBundleBody`] uses the same field order as
    /// [`PolicyBundle`] minus the signature, and no map types are involved.
    /// The output is therefore byte-deterministic across processes and
    /// architectures per S2.3 §13.1.
    ///
    /// # Errors
    ///
    /// Returns [`crate::PolicyError::InvalidPolicyBundle`] if serialisation
    /// fails (this can only happen in pathological cases — e.g. non-UTF-8
    /// payloads — that are excluded by the struct field types).
    pub fn canonical_signed_body_bytes(&self) -> Result<Vec<u8>, crate::PolicyError> {
        let body = SignedBundleBody::from(self);
        serde_json::to_vec(&body).map_err(|e| {
            crate::PolicyError::InvalidPolicyBundle(format!("signed-body serialise: {e}"))
        })
    }
}

/// View of [`PolicyBundle`] omitting the signature, used as the exact byte
/// input to Ed25519 sign + verify (S2.3 §12.3).
///
/// Field order MUST match [`PolicyBundle`] (minus `signature_ed25519`); a
/// re-ordering silently breaks every signature in the wild.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SignedBundleBody<'a> {
    pub bundle_version: &'a str,
    pub bundle_id: &'a str,
    pub created_at: &'a DateTime<Utc>,
    pub signing_authority: &'a str,
    pub rules: &'a [PolicyRule],
}

impl<'a> From<&'a PolicyBundle> for SignedBundleBody<'a> {
    fn from(b: &'a PolicyBundle) -> Self {
        Self {
            bundle_version: &b.bundle_version,
            bundle_id: &b.bundle_id,
            created_at: &b.created_at,
            signing_authority: &b.signing_authority,
            rules: &b.rules,
        }
    }
}

// ---------------------------------------------------------------------------
// base64 codec for `signature_ed25519`
// ---------------------------------------------------------------------------

/// Serde codec module for `signature_ed25519: Vec<u8>` ↔ base64-STANDARD JSON
/// string.
///
/// Raw Ed25519 signatures are 64 binary bytes; embedding them as base64 in the
/// JSON envelope keeps the bundle human-readable and round-trip-safe under
/// serde without leaking control bytes.
mod base64_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(de)?;
        STANDARD
            .decode(s.as_bytes())
            .map_err(|e| serde::de::Error::custom(format!("base64 decode: {e}")))
    }
}
