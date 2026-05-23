//! Canonical encoding + BLAKE3 hashing (S0.1 §8.5).
//!
//! Produces deterministic byte sequences for any `serde::Serialize` value, so that
//! `request_hash` and `idempotency_hash` (S0.1 §3.3) are stable across re-encodings.
//!
//! ## What this module guarantees today
//!
//! - **Lexicographic object key ordering.** JSON objects are emitted with keys sorted by
//!   byte order, recursively, matching JCS §3.2.3.
//! - **No insignificant whitespace.** Output has no spaces, tabs, or newlines outside of
//!   string literals.
//! - **Stable string encoding.** Strings are emitted via `serde_json` which already
//!   escapes per RFC 8259; the surface that hits this module is `Request` content,
//!   where all strings are ASCII identifiers in practice.
//! - **Array order preserved.** Sequences keep declaration order (JCS §3.2.4).
//! - **BLAKE3-256.** 64-char lowercase hex output; `[..32]` truncation helper for the
//!   W11-B 32-hex-char id components (S0.1 §3.2.2).
//!
//! ## What this module deliberately does NOT do (yet)
//!
//! - **ECMA-262 number normalization (JCS §3.2.2.3).** Numbers are emitted via
//!   `serde_json`'s default formatting, which is RFC 8259-compliant but not always
//!   bit-identical to ECMA-262 (relevant only for non-integer floats with imprecise
//!   binary representation). The `Request` surface today contains no `f64` payload
//!   outside `target: Value`; full ECMA-262 number formatting will land alongside the
//!   golden-fixture suite (T-006) once a vetted RFC 8785 implementation is approved
//!   into the workspace.
//! - **Unicode string normalization (NFC).** Deferred for the same reason; not on the
//!   hot path for current consumers.
//!
//! Within these bounds the encoding **is deterministic** for the structures hashed by
//! [`crate::request::Request::request_hash`] and
//! [`crate::envelope::ActionEnvelope::idempotency_hash`]: identical logical input
//! produces identical bytes regardless of construction order or map backing type.

use serde::Serialize;
use serde_json::{Map, Value};
use thiserror::Error;

/// Failure modes for canonical encoding.
#[derive(Debug, Error)]
pub enum CanonicalError {
    /// The input value could not be projected into a `serde_json::Value`.
    ///
    /// In practice this only fires for `Serialize` implementations that themselves return
    /// an error (custom `Serializer` adapters); the types in this crate never trigger it.
    #[error("failed to project value into JSON for canonicalization: {0}")]
    Projection(String),

    /// The intermediate JSON tree could not be serialized to bytes.
    #[error("failed to serialize canonical JSON: {0}")]
    Serialize(String),
}

/// Produce JCS-canonical JSON bytes for any `Serialize` value.
///
/// The output is UTF-8 text with lexicographic object key ordering, no insignificant
/// whitespace, and array order preserved. See the module docs for caveats on number
/// formatting and Unicode normalization.
///
/// # Errors
///
/// Returns [`CanonicalError::Projection`] when the value's `Serialize` implementation
/// itself fails, and [`CanonicalError::Serialize`] when the canonical writer fails
/// (effectively unreachable for in-memory writers, but surfaced for completeness).
pub fn jcs_canonicalize<T: Serialize>(value: &T) -> Result<String, CanonicalError> {
    // Step 1: project into `serde_json::Value`. Using the value tree (rather than a
    // streaming serializer) lets us sort object keys recursively in a single pass and
    // keeps the implementation auditable.
    let projected: Value =
        serde_json::to_value(value).map_err(|e| CanonicalError::Projection(e.to_string()))?;

    // Step 2: walk the tree, sorting object keys lexicographically.
    let sorted = sort_value(projected);

    // Step 3: emit without whitespace.
    serde_json::to_string(&sorted).map_err(|e| CanonicalError::Serialize(e.to_string()))
}

/// Recursively sort all object keys in a `serde_json::Value`.
///
/// `serde_json::Map` already uses `BTreeMap` (when the `preserve_order` feature is
/// disabled, which it is at the workspace level), so map keys are sorted on
/// construction. We rebuild the maps anyway to defend against future workspace
/// configuration drift and to keep the canonicalization explicit and obvious.
fn sort_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut pairs: Vec<(String, Value)> = map.into_iter().collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            let mut sorted = Map::new();
            for (k, v) in pairs {
                sorted.insert(k, sort_value(v));
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_value).collect()),
        other => other,
    }
}

/// Compute the full 64-character lowercase hex BLAKE3-256 hash of `canonical_bytes`.
///
/// Use this for evidence-grade hashes (idempotency, request integrity in the evidence
/// log). For id components that must fit the 32-hex-char W11-B form, see
/// [`blake3_truncated`].
#[must_use]
pub fn blake3_hash(canonical_bytes: &[u8]) -> String {
    blake3::hash(canonical_bytes).to_hex().to_string()
}

/// Compute the 32-character lowercase hex BLAKE3-256 prefix of `canonical_bytes`.
///
/// Matches the W11-B universal truncation rule (S0.1 §3.2.2): the first 128 bits of the
/// BLAKE3 digest expressed as 32 lowercase hex characters. Used as the body of
/// content-addressed identifiers such as `tplan_<32hex>`.
#[must_use]
pub fn blake3_truncated(canonical_bytes: &[u8]) -> String {
    let full = blake3::hash(canonical_bytes).to_hex();
    full.as_str()
        .get(..32)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::{blake3_hash, blake3_truncated, jcs_canonicalize, sort_value};
    use serde_json::json;
    use std::collections::{BTreeMap, HashMap};

    // ---- JCS determinism --------------------------------------------------------

    #[test]
    fn jcs_canonical_form_is_independent_of_object_key_insertion_order() {
        // Same logical object, different insertion sequences.
        let a = json!({
            "zeta":   1,
            "alpha":  2,
            "middle": 3,
        });

        let b = json!({
            "alpha":  2,
            "middle": 3,
            "zeta":   1,
        });

        let ca = jcs_canonicalize(&a).expect("canonicalize a must succeed");
        let cb = jcs_canonicalize(&b).expect("canonicalize b must succeed");

        assert_eq!(ca, cb, "JCS output must not depend on key insertion order");
        // And keys must be in lex order in the output.
        assert!(
            ca.find("\"alpha\"").unwrap_or(usize::MAX)
                < ca.find("\"middle\"").unwrap_or(usize::MAX)
                && ca.find("\"middle\"").unwrap_or(usize::MAX)
                    < ca.find("\"zeta\"").unwrap_or(usize::MAX),
            "keys must appear in lexicographic order, got: {ca}"
        );
    }

    #[test]
    fn jcs_collapses_btreemap_and_hashmap_with_same_content_to_identical_bytes() {
        // Two different Rust map types with identical logical content must canonicalize
        // to the same bytes — this is the cross-encoding determinism guarantee.
        let mut hashmap: HashMap<String, i32> = HashMap::new();
        hashmap.insert("b".to_owned(), 2);
        hashmap.insert("a".to_owned(), 1);
        hashmap.insert("c".to_owned(), 3);

        let mut btree: BTreeMap<String, i32> = BTreeMap::new();
        btree.insert("c".to_owned(), 3);
        btree.insert("a".to_owned(), 1);
        btree.insert("b".to_owned(), 2);

        let c_hash = jcs_canonicalize(&hashmap).expect("hashmap canonicalize must succeed");
        let c_btree = jcs_canonicalize(&btree).expect("btreemap canonicalize must succeed");

        assert_eq!(
            c_hash, c_btree,
            "HashMap and BTreeMap with same content must produce identical canonical bytes"
        );
    }

    #[test]
    fn jcs_recursively_sorts_nested_objects() {
        // Mixed structure: outer object has reverse-order keys, inner array contains
        // objects with reverse-order keys too. Canonical output must sort recursively.
        let v = json!({
            "z": [
                { "y": 1, "x": 2 },
                { "b": 3, "a": 4 },
            ],
            "a": { "n": 1, "m": 2 },
        });

        let canon = jcs_canonicalize(&v).expect("canonicalize must succeed");

        // Top-level: "a" before "z".
        let a_pos = canon.find("\"a\":").unwrap_or(usize::MAX);
        let z_pos = canon.find("\"z\":").unwrap_or(usize::MAX);
        assert!(a_pos < z_pos, "top-level keys not sorted: {canon}");

        // Inner array first object: "x" before "y".
        let x_pos = canon.find("\"x\"").unwrap_or(usize::MAX);
        let y_pos = canon.find("\"y\"").unwrap_or(usize::MAX);
        assert!(x_pos < y_pos, "nested keys not sorted: {canon}");
    }

    #[test]
    fn jcs_emits_no_insignificant_whitespace() {
        let v = json!({"a": 1, "b": [1, 2, 3]});
        let canon = jcs_canonicalize(&v).expect("canonicalize must succeed");
        // serde_json::to_string emits no insignificant whitespace by default; assert it.
        assert!(!canon.contains(' '), "must not contain spaces: {canon}");
        assert!(!canon.contains('\n'), "must not contain newlines: {canon}");
        assert!(!canon.contains('\t'), "must not contain tabs: {canon}");
    }

    #[test]
    fn sort_value_preserves_array_order() {
        // Arrays must preserve declaration order (JCS §3.2.4).
        let v = json!([3, 1, 2, "z", "a"]);
        let sorted = sort_value(v);
        assert_eq!(sorted, json!([3, 1, 2, "z", "a"]));
    }

    // ---- BLAKE3 helpers ---------------------------------------------------------

    #[test]
    fn blake3_hash_returns_64_lowercase_hex_chars() {
        let h = blake3_hash(b"");
        assert_eq!(h.len(), 64, "BLAKE3 hex digest must be 64 chars, got {h}");
        assert!(
            h.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "BLAKE3 digest must be lowercase hex, got {h}"
        );
    }

    #[test]
    fn blake3_hash_matches_known_fixture_for_empty_input() {
        // Known BLAKE3-256 hash of the empty byte string. This fixture comes from the
        // BLAKE3 reference test vectors and pins our hex encoding once and for all.
        let h = blake3_hash(b"");
        assert_eq!(
            h, "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
            "BLAKE3('') reference vector must match"
        );
    }

    #[test]
    fn blake3_truncated_returns_first_32_hex_chars_of_full_digest() {
        // The truncated form must be a strict prefix of the full digest.
        let full = blake3_hash(b"hello world");
        let trunc = blake3_truncated(b"hello world");
        assert_eq!(trunc.len(), 32, "truncated digest must be 32 chars");
        assert_eq!(
            trunc,
            &full[..32],
            "truncated digest must be the first 32 hex chars of the full digest"
        );
    }
}
