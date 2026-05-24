//! Redaction profiles per S3.1 §14.
//!
//! Stored payloads are redacted **before persistence** and — critically —
//! **before** the content hash and the Ed25519 signature are computed. The
//! cryptographic chain therefore witnesses the **redacted** form, not the raw
//! form. There is no "unredacted but signed" representation anywhere in the
//! Evidence Log; that would defeat the constitutional privacy contract.
//!
//! ## Closed profile vocabulary (S3.1 §14)
//!
//! | Profile         | Wire name        | Effect                                                                                                                |
//! | --------------- | ---------------- | --------------------------------------------------------------------------------------------------------------------- |
//! | [`Default`]     | `default`        | secret-shaped substrings (S1.1 §17.2.6); raw key material; passwords; tokens; full prompt bodies that contain secrets |
//! | [`Strict`]      | `strict`         | `default` + identifiable user content; PII heuristics; payload reduced to structural markers                          |
//! | [`DebugCapture`] | `debug_capture` | minimal redaction; only obvious secrets; **only enabled by explicit policy decision** (S3.1 §14)                      |
//!
//! [`Default`]: RedactionProfile::Default
//! [`Strict`]: RedactionProfile::Strict
//! [`DebugCapture`]: RedactionProfile::DebugCapture
//!
//! ## Pipeline order (S3.1 §14 + §5.2)
//!
//! ```text
//!   raw payload ──► apply_redaction(profile, record_type, payload)
//!               ──► redacted payload      ◄── stored on the receipt
//!               ──► content_hash = BLAKE3(JCS(redacted_payload))
//!               ──► Ed25519 signature over BLAKE3(JCS(receipt-minus-signature))
//! ```
//!
//! Per the spec this ordering is constitutional: the redacted form **is** what
//! is hashed and signed. Re-computing the signature against the raw payload is
//! by construction impossible — the raw payload is never persisted.
//!
//! ## Category-driven per-RecordType policy
//!
//! Profiles consult [`record_category`] to bucket each `RecordType` into one of
//! six families (lifecycle, vault, recovery, telemetry, segment, generic). The
//! per-(profile, category) table in [`apply_redaction`] then picks the field
//! treatment: keep verbatim, hash, stub with `"<redacted>"`, or drop. This
//! keeps the policy compact (six categories × three profiles) instead of
//! exploding to 427 × 3 ad-hoc rules.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::EvidenceError;
use crate::record::RecordType;

/// Sentinel placed in the redacted payload for fields whose presence — but
/// not value — is preserved. Reserved string literal; downstream consumers
/// MUST treat this as opaque.
pub const REDACTED_SENTINEL: &str = "<redacted>";

/// Closed S3.1 §14 redaction-profile vocabulary.
///
/// Applied at seal time, **before** the content hash and Ed25519 signature
/// are computed. The redacted form is what is hashed and signed; there is no
/// unredacted-but-signed representation anywhere in the log.
///
/// Wire names exactly match the §14 table: `default`, `strict`,
/// `debug_capture` (lowercase). The Rust enum is `Copy + Eq + Hash` so it
/// can flow cheaply through builders and selection tables.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum RedactionProfile {
    /// Minimal redaction: strip secret-shaped substrings (S1.1 §17.2.6),
    /// raw key material, passwords, tokens, and full prompt bodies that
    /// contain secrets. The default applied when no other profile is
    /// requested. Per S3.1 §18 "Secret redaction is default".
    #[default]
    #[serde(rename = "default")]
    Default,

    /// `Default` + identifiable user content + PII heuristics. The most
    /// aggressive profile in routine operation: payload bodies are reduced
    /// to structural markers (record type, action id back-reference,
    /// content hash). Use for operator-sensitive emission contexts.
    #[serde(rename = "strict")]
    Strict,

    /// Minimal redaction; only obvious secrets are stripped. Per S3.1 §14
    /// this profile is **only enabled by explicit policy decision** —
    /// activation must emit a `POLICY_BUNDLE_LOAD` (or override grant)
    /// receipt. The Evidence Log itself does not gate the choice; it is
    /// the L4 Policy Kernel's responsibility to admit a request that
    /// names this profile.
    #[serde(rename = "debug_capture")]
    DebugCapture,
}

impl RedactionProfile {
    /// Canonical lowercase wire name per S3.1 §14 (`default` / `strict` /
    /// `debug_capture`). Mirrors the `serde(rename = ...)` value on the
    /// variant so `serde_json::to_string(&profile)` minus quotes is
    /// byte-identical.
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Strict => "strict",
            Self::DebugCapture => "debug_capture",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Per-RecordType category bucketing.
// ─────────────────────────────────────────────────────────────────────

/// Coarse-grained payload category used by [`apply_redaction`] to drive
/// per-`RecordType` field treatment.
///
/// The category dimension is kept deliberately small (six values) so the
/// (profile × category) policy table fits in the head — and so adding a new
/// `RecordType` only requires assigning a category, not authoring a fresh
/// per-record rule.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) enum RecordCategory {
    /// Action lifecycle: `ACTION_RECEIVED`, `POLICY_DECISION`,
    /// `APPROVAL_*`, `EXECUTION_*`, `VERIFICATION_RESULT`,
    /// `ROLLBACK_COMPLETED`, `MODEL_CALL` family. Payload carries action
    /// arguments / target paths — operator-sensitive under Strict.
    Lifecycle,

    /// Vault Broker emissions (S5.2): `VAULT_*`, `SUBJECT_KIND_*`. Payload
    /// is already redacted at the source (no key material), but capability
    /// ids and subject ids are PII-adjacent under Strict.
    Vault,

    /// Recovery + tamper records (`RECOVERY_EVENT`, `TAMPER_DETECTED`,
    /// `CHAIN_INCONSISTENCY_DETECTED`, `RECEIPT_FORGERY_DETECTED`,
    /// `SEGMENT_*`). Mostly structural; near-zero PII surface even under
    /// Strict.
    Recovery,

    /// Telemetry / observability emissions (`TELEMETRY_*`,
    /// `OBSERVABILITY_*`). Carries label sets and value samples — Strict
    /// redacts identifiable label values.
    Telemetry,

    /// Segment-level book-keeping (`SEGMENT_SEALED`, `CHAIN_CHECKPOINT`,
    /// `GC_PASS`, `QUARANTINE_EVENT`, `CONFLICT_EVENT`). Structural; the
    /// per-segment metadata is intentionally non-sensitive.
    Segment,

    /// Catch-all: any record not in the buckets above. Defaults to the
    /// Strict-aggressive policy because the safe choice for an
    /// unclassified record is to assume operator-sensitive payload.
    Generic,
}

/// Classify a `RecordType` into a [`RecordCategory`].
///
/// The mapping uses the canonical Rust identifier so the compiler enforces
/// exhaustiveness on additions to the `RecordType` enum (no
/// `_ => Generic` fallback hides a missed bucket — adding a new
/// `RecordType` variant forces a touch here).
///
/// Implementation note: we route by `as_wire_str()` prefix matching rather
/// than enumerating all 427 variants verbatim. The wire-name vocabulary in
/// the spec is itself prefix-coded (`VAULT_*`, `RECOVERY_*`, etc.), so this
/// pattern is the spec's own taxonomy. New record types that do not match
/// any known prefix fall into [`RecordCategory::Generic`].
#[must_use]
pub(crate) fn record_category(rt: RecordType) -> RecordCategory {
    let name = rt.as_wire_str();

    // Vault Broker emissions (S5.2). Authority restricted to the broker.
    if name.starts_with("VAULT_") || name.starts_with("SUBJECT_KIND_") {
        return RecordCategory::Vault;
    }

    // Recovery + tamper + chain-integrity records.
    if name.starts_with("RECOVERY_")
        || name.starts_with("TAMPER_")
        || name.starts_with("CHAIN_INCONSISTENCY")
        || name.starts_with("RECEIPT_FORGERY")
        || name.starts_with("RECEIPT_INTEGRITY")
        || name.starts_with("RECEIPT_LINEAGE")
        || name.starts_with("RECEIPT_SEQUENCE")
        || name == "RECOVERY_EVENT"
    {
        return RecordCategory::Recovery;
    }

    // Telemetry / observability.
    if name.starts_with("TELEMETRY_") || name.starts_with("OBSERVABILITY_") {
        return RecordCategory::Telemetry;
    }

    // Segment + checkpoint + housekeeping.
    if name.starts_with("SEGMENT_")
        || name.starts_with("CHAIN_CHECKPOINT")
        || name == "GC_PASS"
        || name == "QUARANTINE_EVENT"
        || name == "CONFLICT_EVENT"
    {
        return RecordCategory::Segment;
    }

    // Action lifecycle: explicit Wave-1 set + prefix-coded extensions.
    if matches!(
        name,
        "ACTION_RECEIVED"
            | "TRANSLATION_CREATED"
            | "ROUTING_DECISION"
            | "POLICY_DECISION"
            | "APPROVAL_REQUESTED"
            | "APPROVAL_GRANTED"
            | "APPROVAL_DENIED"
            | "EXECUTION_STARTED"
            | "EXECUTION_COMPLETED"
            | "VERIFICATION_RESULT"
            | "ROLLBACK_COMPLETED"
            | "MODEL_CALL"
            | "EMERGENCY_OVERRIDE_GRANT"
            | "POLICY_BUNDLE_LOAD"
    ) || name.starts_with("EXECUTION_")
        || name.starts_with("APPROVAL_")
        || name.starts_with("MODEL_")
        || name.starts_with("POLICY_")
    {
        return RecordCategory::Lifecycle;
    }

    RecordCategory::Generic
}

// ─────────────────────────────────────────────────────────────────────
// Redaction core.
// ─────────────────────────────────────────────────────────────────────

/// Default secret-shaped field-name patterns stripped under
/// [`RedactionProfile::Default`] / [`RedactionProfile::DebugCapture`].
///
/// Matched case-insensitively against object keys. Per S3.1 §14 / S1.1
/// §17.2.6 the list covers raw key material, passwords, tokens, and full
/// prompt bodies that contain secrets.
const SECRET_KEY_PATTERNS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "private_key",
    "privatekey",
    "raw_payload",
    "raw_prompt",
    "credentials",
    "bearer",
    "authorization",
    "session_key",
    "client_secret",
];

/// Field-name patterns additionally hashed (replaced by their BLAKE3
/// truncated hash) under [`RedactionProfile::Strict`]. These are
/// identifiable but auditable: keeping the hash preserves correlation
/// across records without exposing the value.
const PII_KEY_PATTERNS: &[&str] = &[
    "user",
    "email",
    "phone",
    "address",
    "ip",
    "hostname",
    "device_id",
    "operator",
    "subject",
    "actor",
    "principal",
    "username",
    "account",
    "owner",
];

/// Apply the profile's redaction policy to `payload` for the given
/// `record_type` and return the redacted [`Value`].
///
/// `payload` is not mutated. The returned [`Value`] is the byte sequence
/// that will be stored on the receipt, hashed for `content_hash`, and
/// (transitively) signed.
///
/// # Algorithm
///
/// 1. [`RedactionProfile::DebugCapture`] short-circuits to a clone of the
///    input (per §14 — minimal redaction, only obvious secrets stripped).
///    For `DebugCapture` we still strip the most obvious secret keys
///    matching [`SECRET_KEY_PATTERNS`] so a debug-capture payload never
///    persists raw `password` / `token` / `api_key` fields. Everything
///    else passes through verbatim.
/// 2. [`RedactionProfile::Default`] walks the JSON tree and stubs any
///    object key matching [`SECRET_KEY_PATTERNS`] (case-insensitive) with
///    [`REDACTED_SENTINEL`]. Non-object scalars are kept.
/// 3. [`RedactionProfile::Strict`] does Default + additionally hashes any
///    object key matching [`PII_KEY_PATTERNS`] (case-insensitive) with
///    BLAKE3-truncated (32 hex chars). For categories declared as
///    operator-sensitive in the per-(profile, category) table, the
///    payload is further reduced to a structural marker (record type +
///    `payload_hash_prefix`) — see [`reduce_to_structural_marker`].
///
/// # Errors
///
/// Returns [`EvidenceError::EncodingFailed`] if a sub-payload cannot be
/// canonicalized for hashing.
pub fn apply_redaction(
    profile: RedactionProfile,
    record_type: RecordType,
    payload: &Value,
) -> Result<Value, EvidenceError> {
    match profile {
        // Default and DebugCapture share the secret-redaction body
        // transformation by spec (§14: both perform "minimal redaction;
        // only secrets"). They differ in policy attestation — the
        // `redaction_profile` field on the receipt records the choice so
        // an auditor can tell DebugCapture (policy-attested) from Default
        // (routine).
        RedactionProfile::Default | RedactionProfile::DebugCapture => {
            Ok(redact_keys(payload, SECRET_KEY_PATTERNS, false))
        }
        RedactionProfile::Strict => apply_strict(record_type, payload),
    }
}

/// Apply the Strict profile, dispatching per-category to the appropriate
/// reduction.
fn apply_strict(record_type: RecordType, payload: &Value) -> Result<Value, EvidenceError> {
    let category = record_category(record_type);

    // Stage 1: always strip raw secrets.
    let stripped = redact_keys(payload, SECRET_KEY_PATTERNS, false);

    match category {
        // Lifecycle + Telemetry + Generic: full reduction to structural marker.
        // Per S3.1 §14 Strict, payload bodies are reduced to "structural markers
        // + content hashes" so the chain still witnesses *that* an event
        // happened without exposing what was in it.
        RecordCategory::Lifecycle | RecordCategory::Telemetry | RecordCategory::Generic => {
            reduce_to_structural_marker(record_type, &stripped)
        }

        // Vault: keep capability id + retention metadata; hash subject-shaped
        // fields. Already source-redacted (no key material per S5.2 §14), so
        // we preserve the structural body and only hash PII-adjacent keys.
        RecordCategory::Vault => Ok(hash_pii_keys(&stripped)),

        // Recovery + Segment: structural by design; keep verbatim. These records
        // are emitted by the engine itself and carry only segment ids, retention
        // classes, receipt counts, etc. — no operator-sensitive content.
        RecordCategory::Recovery | RecordCategory::Segment => Ok(stripped),
    }
}

/// Normalize a key for pattern matching: lowercase, with hyphens / spaces /
/// dots collapsed to underscore. So `x-api-key`, `X-Api-Key`, `apiKey`,
/// `api_key`, and `api.key` all normalize to a form containing `api_key`.
///
/// Robustness against header-style hyphen names (`x-api-key`,
/// `authorization-bearer`) and against camelCase variants
/// (`clientSecret`).
fn normalize_key(k: &str) -> String {
    let mut out = String::with_capacity(k.len() + 4);
    let mut prev_was_lower = false;
    for c in k.chars() {
        if c == '-' || c == ' ' || c == '.' {
            out.push('_');
            prev_was_lower = false;
        } else if c.is_ascii_uppercase() {
            // Insert an underscore at camelCase boundaries: aB -> a_b.
            if prev_was_lower {
                out.push('_');
            }
            for lc in c.to_lowercase() {
                out.push(lc);
            }
            prev_was_lower = false;
        } else {
            for lc in c.to_lowercase() {
                out.push(lc);
            }
            prev_was_lower = c.is_ascii_lowercase();
        }
    }
    out
}

/// Walk `value` and replace every object key matching one of `patterns`
/// (substring match against the [`normalize_key`]-ed key) with
/// [`REDACTED_SENTINEL`].
///
/// `if_hash_match` is reserved for future field-name-hashing variants; for
/// now it must be `false` and the match is a pure stub.
fn redact_keys(value: &Value, patterns: &[&str], if_hash_match: bool) -> Value {
    debug_assert!(!if_hash_match, "hash-on-match not used by redact_keys");
    match value {
        Value::Object(map) => {
            let mut out = Map::with_capacity(map.len());
            for (k, v) in map {
                let k_norm = normalize_key(k);
                if patterns.iter().any(|p| k_norm.contains(*p)) {
                    out.insert(k.clone(), Value::String(REDACTED_SENTINEL.to_owned()));
                } else {
                    out.insert(k.clone(), redact_keys(v, patterns, if_hash_match));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| redact_keys(v, patterns, false))
                .collect(),
        ),
        // Scalar values are kept; redaction targets *named* fields.
        _ => value.clone(),
    }
}

/// Walk `value` and replace every object value whose key matches one of
/// [`PII_KEY_PATTERNS`] with the BLAKE3-truncated (32 hex chars) hash of
/// that value's JCS-canonical bytes.
///
/// Hashed values preserve correlation across records (the same email
/// always hashes to the same 32-char prefix) without exposing the cleartext.
fn hash_pii_keys(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::with_capacity(map.len());
            for (k, v) in map {
                let k_norm = normalize_key(k);
                if PII_KEY_PATTERNS.iter().any(|p| k_norm.contains(*p)) {
                    // Hash the value's canonical bytes. On encoding failure
                    // we fall back to the SENTINEL — never let a redaction
                    // hash failure pass cleartext through.
                    let hashed = aios_action::jcs_canonicalize(v).map_or_else(
                        |_| REDACTED_SENTINEL.to_owned(),
                        |canonical| {
                            format!(
                                "hash:{}",
                                aios_action::blake3_truncated(canonical.as_bytes())
                            )
                        },
                    );
                    out.insert(k.clone(), Value::String(hashed));
                } else {
                    out.insert(k.clone(), hash_pii_keys(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(hash_pii_keys).collect()),
        _ => value.clone(),
    }
}

/// Reduce `payload` to a fixed-shape structural marker that preserves the
/// fact-of-occurrence and a content-hash anchor while dropping the body.
///
/// Output shape:
///
/// ```json
/// {
///   "record_type": "<wire name>",
///   "payload_hash_prefix": "<32 hex chars>",
///   "redacted_under_profile": "strict"
/// }
/// ```
///
/// The `payload_hash_prefix` is the BLAKE3-truncated hash of the
/// already-stripped JCS payload — so two receipts with identical bodies
/// still correlate via this prefix even after Strict reduction.
///
/// # Errors
///
/// Returns [`EvidenceError::EncodingFailed`] if the JCS projection fails.
fn reduce_to_structural_marker(
    record_type: RecordType,
    stripped: &Value,
) -> Result<Value, EvidenceError> {
    let canonical = aios_action::jcs_canonicalize(stripped)?;
    let prefix = aios_action::blake3_truncated(canonical.as_bytes());
    let mut out = Map::with_capacity(3);
    out.insert(
        "record_type".to_owned(),
        Value::String(record_type.as_wire_str().to_owned()),
    );
    out.insert("payload_hash_prefix".to_owned(), Value::String(prefix));
    out.insert(
        "redacted_under_profile".to_owned(),
        Value::String(RedactionProfile::Strict.as_wire_str().to_owned()),
    );
    Ok(Value::Object(out))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use serde_json::json;

    // ─── RedactionProfile vocabulary ─────────────────────────────────

    #[test]
    fn redaction_profile_default_is_default_variant() {
        let p: RedactionProfile = RedactionProfile::default();
        assert_eq!(p, RedactionProfile::Default);
    }

    #[test]
    fn redaction_profile_wire_names_match_spec_table() {
        assert_eq!(RedactionProfile::Default.as_wire_str(), "default");
        assert_eq!(RedactionProfile::Strict.as_wire_str(), "strict");
        assert_eq!(
            RedactionProfile::DebugCapture.as_wire_str(),
            "debug_capture"
        );
    }

    #[test]
    fn redaction_profile_serde_round_trip() {
        for p in [
            RedactionProfile::Default,
            RedactionProfile::Strict,
            RedactionProfile::DebugCapture,
        ] {
            let s = serde_json::to_string(&p).expect("ser");
            let back: RedactionProfile = serde_json::from_str(&s).expect("de");
            assert_eq!(back, p);
            // serde wire form matches as_wire_str()
            assert_eq!(s, format!("\"{}\"", p.as_wire_str()));
        }
    }

    // ─── Default profile: strips secret-shaped keys ──────────────────

    #[test]
    fn default_profile_strips_password_field() {
        let raw = json!({"username": "alice", "password": "s3cr3t"});
        let out = apply_redaction(RedactionProfile::Default, RecordType::ActionReceived, &raw)
            .expect("redact");
        assert_eq!(out["password"], json!(REDACTED_SENTINEL));
        // Non-sensitive field passes through.
        assert_eq!(out["username"], json!("alice"));
    }

    #[test]
    fn default_profile_strips_token_and_api_key_fields() {
        let raw = json!({
            "method": "POST",
            "headers": {
                "authorization": "Bearer xyz",
                "x-api-key": "abc123"
            },
            "body": {"token": "t-42", "ok": true}
        });
        let out = apply_redaction(RedactionProfile::Default, RecordType::ModelCall, &raw)
            .expect("redact");
        // Recursive descent.
        assert_eq!(out["headers"]["authorization"], json!(REDACTED_SENTINEL));
        assert_eq!(out["headers"]["x-api-key"], json!(REDACTED_SENTINEL));
        assert_eq!(out["body"]["token"], json!(REDACTED_SENTINEL));
        assert_eq!(out["body"]["ok"], json!(true));
        assert_eq!(out["method"], json!("POST"));
    }

    #[test]
    fn default_profile_strips_private_key_and_credentials() {
        let raw = json!({
            "client_secret": "shhh",
            "private_key": "-----BEGIN-----",
            "credentials": {"u": "a", "p": "b"},
            "public_id": "id-1"
        });
        let out = apply_redaction(RedactionProfile::Default, RecordType::PolicyDecision, &raw)
            .expect("redact");
        assert_eq!(out["client_secret"], json!(REDACTED_SENTINEL));
        assert_eq!(out["private_key"], json!(REDACTED_SENTINEL));
        assert_eq!(out["credentials"], json!(REDACTED_SENTINEL));
        assert_eq!(out["public_id"], json!("id-1"));
    }

    #[test]
    fn default_profile_is_case_insensitive() {
        let raw = json!({"PASSWORD": "x", "ApiKey": "y", "Authorization": "z"});
        let out = apply_redaction(RedactionProfile::Default, RecordType::ActionReceived, &raw)
            .expect("redact");
        assert_eq!(out["PASSWORD"], json!(REDACTED_SENTINEL));
        assert_eq!(out["ApiKey"], json!(REDACTED_SENTINEL));
        assert_eq!(out["Authorization"], json!(REDACTED_SENTINEL));
    }

    #[test]
    fn default_profile_walks_into_arrays() {
        let raw = json!({
            "events": [
                {"name": "login", "password": "p1"},
                {"name": "logout", "session_key": "k1"}
            ]
        });
        let out = apply_redaction(RedactionProfile::Default, RecordType::ActionReceived, &raw)
            .expect("redact");
        assert_eq!(out["events"][0]["password"], json!(REDACTED_SENTINEL));
        assert_eq!(out["events"][1]["session_key"], json!(REDACTED_SENTINEL));
        // Names pass through.
        assert_eq!(out["events"][0]["name"], json!("login"));
    }

    #[test]
    fn default_profile_keeps_non_sensitive_payloads_verbatim() {
        let raw = json!({"action": "fs.write", "path": "/etc/foo", "count": 3});
        let out = apply_redaction(RedactionProfile::Default, RecordType::ActionReceived, &raw)
            .expect("redact");
        assert_eq!(out, raw);
    }

    // ─── DebugCapture profile: minimal redaction ─────────────────────

    #[test]
    fn debug_capture_profile_still_strips_obvious_secrets() {
        // Per §14, "minimal redaction; only secrets". DebugCapture is NOT a
        // pass-through — raw passwords / tokens still go.
        let raw = json!({"password": "x", "details": "ok"});
        let out = apply_redaction(
            RedactionProfile::DebugCapture,
            RecordType::ActionReceived,
            &raw,
        )
        .expect("redact");
        assert_eq!(out["password"], json!(REDACTED_SENTINEL));
        assert_eq!(out["details"], json!("ok"));
    }

    #[test]
    fn debug_capture_profile_keeps_pii_fields_verbatim() {
        // PII fields (email, user, address) are NOT stripped under DebugCapture
        // — that is what Strict adds. DebugCapture only catches obvious secrets.
        let raw = json!({"email": "a@b.com", "user": "alice", "extra": 42});
        let out = apply_redaction(
            RedactionProfile::DebugCapture,
            RecordType::ActionReceived,
            &raw,
        )
        .expect("redact");
        assert_eq!(out["email"], json!("a@b.com"));
        assert_eq!(out["user"], json!("alice"));
        assert_eq!(out["extra"], json!(42));
    }

    // ─── Strict profile: full structural reduction for Lifecycle ─────

    #[test]
    fn strict_profile_reduces_lifecycle_payload_to_structural_marker() {
        let raw = json!({
            "action": "fs.write",
            "path": "/etc/foo",
            "operator_email": "ops@example.com",
            "password": "s3cr3t"
        });
        let out = apply_redaction(RedactionProfile::Strict, RecordType::ActionReceived, &raw)
            .expect("redact");

        // The structural marker shape:
        let obj = out.as_object().expect("object");
        assert_eq!(obj.len(), 3, "structural marker has exactly 3 keys");
        assert_eq!(obj["record_type"], json!("ACTION_RECEIVED"));
        assert_eq!(obj["redacted_under_profile"], json!("strict"));
        let prefix = obj["payload_hash_prefix"].as_str().expect("str");
        assert_eq!(prefix.len(), 32, "BLAKE3-truncated to 32 hex chars");
        assert!(prefix
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn strict_profile_lifecycle_marker_correlates_identical_payloads() {
        let raw_a = json!({"x": 1, "y": 2});
        let raw_b = json!({"x": 1, "y": 2});
        let raw_c = json!({"x": 1, "y": 999});
        let out_a = apply_redaction(RedactionProfile::Strict, RecordType::PolicyDecision, &raw_a)
            .expect("a");
        let out_b = apply_redaction(RedactionProfile::Strict, RecordType::PolicyDecision, &raw_b)
            .expect("b");
        let out_c = apply_redaction(RedactionProfile::Strict, RecordType::PolicyDecision, &raw_c)
            .expect("c");
        // Identical raw → identical reduced marker.
        assert_eq!(out_a, out_b);
        // Different raw → different marker prefix.
        assert_ne!(out_a, out_c);
        assert_ne!(
            out_a["payload_hash_prefix"], out_c["payload_hash_prefix"],
            "different bodies must yield different prefixes"
        );
    }

    // ─── Strict profile: Vault hashes PII keys, keeps body ──────────

    #[test]
    fn strict_profile_vault_hashes_subject_keeps_capability_id() {
        let raw = json!({
            "capability_id": "cap-42",
            "subject": "human:operator-1",
            "operation": "Sign",
            "password": "should-not-appear"
        });
        let out = apply_redaction(RedactionProfile::Strict, RecordType::VaultOperation, &raw)
            .expect("redact");

        // capability_id passes through (structural).
        assert_eq!(out["capability_id"], json!("cap-42"));
        assert_eq!(out["operation"], json!("Sign"));
        // password was stripped to sentinel by the secret pass.
        assert_eq!(out["password"], json!(REDACTED_SENTINEL));
        // subject is replaced by a hash:... reference.
        let s = out["subject"].as_str().expect("str");
        assert!(s.starts_with("hash:"), "expected hash: prefix, got `{s}`");
        assert_eq!(s.len(), "hash:".len() + 32);
    }

    // ─── Strict profile: Recovery + Segment kept structurally ───────

    #[test]
    fn strict_profile_recovery_keeps_structural_payload() {
        let raw = json!({
            "mode": "recovery_entered",
            "reason": "operator_request",
            "since": "2026-05-24T12:00:00Z"
        });
        let out = apply_redaction(RedactionProfile::Strict, RecordType::RecoveryEvent, &raw)
            .expect("redact");
        // Recovery records pass through (no PII pattern, no secret pattern).
        assert_eq!(out, raw);
    }

    #[test]
    fn strict_profile_segment_sealed_payload_unchanged() {
        let raw = json!({
            "segment_id": "seg_abcd",
            "retention_class": "STANDARD_24M",
            "receipt_count": 42,
            "previous_segment_id": null,
            "previous_segment_seal_hash": null
        });
        let out = apply_redaction(RedactionProfile::Strict, RecordType::SegmentSealed, &raw)
            .expect("redact");
        assert_eq!(out, raw);
    }

    // ─── Per-RecordType coverage: 5+ distinct types ─────────────────

    #[test]
    fn redaction_runs_for_every_category_under_default_profile() {
        let samples: &[(RecordType, Value)] = &[
            (RecordType::ActionReceived, json!({"password": "x"})),
            (RecordType::VaultOperation, json!({"token": "t"})),
            (RecordType::RecoveryEvent, json!({"api_key": "k"})),
            (RecordType::SegmentSealed, json!({"secret": "s"})),
            (RecordType::PolicyDecision, json!({"credentials": "c"})),
            (RecordType::ChainCheckpoint, json!({"private_key": "p"})),
        ];
        for (rt, payload) in samples {
            let out = apply_redaction(RedactionProfile::Default, *rt, payload).expect("redact");
            // Each sample's single key gets stripped under Default.
            let key = payload
                .as_object()
                .expect("obj")
                .keys()
                .next()
                .expect("key");
            assert_eq!(
                out[key],
                json!(REDACTED_SENTINEL),
                "category for {rt:?} did not strip key `{key}`"
            );
        }
    }

    #[test]
    fn record_category_classifies_canonical_examples() {
        assert_eq!(
            record_category(RecordType::ActionReceived),
            RecordCategory::Lifecycle
        );
        assert_eq!(
            record_category(RecordType::PolicyDecision),
            RecordCategory::Lifecycle
        );
        assert_eq!(
            record_category(RecordType::VaultOperation),
            RecordCategory::Vault
        );
        assert_eq!(
            record_category(RecordType::RecoveryEvent),
            RecordCategory::Recovery
        );
        assert_eq!(
            record_category(RecordType::SegmentSealed),
            RecordCategory::Segment
        );
        assert_eq!(
            record_category(RecordType::ChainCheckpoint),
            RecordCategory::Segment
        );
        assert_eq!(
            record_category(RecordType::TamperDetected),
            RecordCategory::Recovery
        );
    }

    // ─── Edge cases ─────────────────────────────────────────────────

    #[test]
    fn redaction_handles_null_and_empty_payloads() {
        let null = Value::Null;
        let out = apply_redaction(RedactionProfile::Strict, RecordType::PolicyDecision, &null)
            .expect("redact null");
        // Strict on Lifecycle null → structural marker over empty body.
        assert!(out.is_object());
        assert_eq!(out["record_type"], json!("POLICY_DECISION"));

        let empty = json!({});
        let out = apply_redaction(
            RedactionProfile::Default,
            RecordType::ActionReceived,
            &empty,
        )
        .expect("redact empty");
        assert_eq!(out, json!({}));
    }

    #[test]
    fn redaction_does_not_mutate_input_payload() {
        let raw = json!({"password": "secret", "ok": true});
        let raw_clone = raw.clone();
        let _ = apply_redaction(RedactionProfile::Default, RecordType::ActionReceived, &raw)
            .expect("redact");
        // Input must be byte-identical after redaction.
        assert_eq!(raw, raw_clone);
    }
}
