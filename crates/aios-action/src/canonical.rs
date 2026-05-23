//! Canonical encoding + BLAKE3 hashing (S0.1 §8.5).
//!
//! Produces deterministic byte sequences for any `serde::Serialize` value, so that
//! `request_hash` and `idempotency_hash` (S0.1 §3.3) are stable across re-encodings.
//!
//! ## RFC 8785 full compliance via `serde_jcs` 0.1 (T-006 adoption)
//!
//! As of T-006 the encoder delegates to the Governor-approved [`serde_jcs`] 0.1 crate,
//! the canonical Rust implementation of the JSON Canonicalization Scheme (RFC 8785).
//! That gives us, in one drop-in dependency:
//!
//! - **Lexicographic object key ordering** (RFC 8785 §3.2.3, after Unicode code-point
//!   sort of UTF-16 key encodings).
//! - **No insignificant whitespace** (RFC 8785 §3.2.1).
//! - **ECMA-262 §6.12.7 number normalization** (RFC 8785 §3.2.2.3) — non-integer
//!   doubles that are equal under IEEE-754 produce the same canonical string. This is
//!   the deficit explicitly deferred from T-002 and now closed.
//! - **Array order preserved** (RFC 8785 §3.2.4).
//! - **String escaping per RFC 8259 §7** (RFC 8785 §3.2.2.2).
//!
//! On top of canonicalization we still own the **BLAKE3-256** layer: 64-char
//! lowercase hex output via [`blake3_hash`]; `[..32]` truncation helper via
//! [`blake3_truncated`] for the W11-B 32-hex-char id components (S0.1 §3.2.2).
//!
//! ## Unicode NFC
//!
//! RFC 8785 §3.2.2.2 explicitly does **not** mandate NFC normalization of string
//! contents — input strings are emitted verbatim once escaped. AIOS strings hashed
//! today (`Request::action`, `Identity::subject_canonical_id`, etc.) are ASCII
//! identifiers in practice; if a non-ASCII payload ever needs to hash identically
//! across NFC/NFD inputs, the **caller** must normalize before construction. This is
//! the same posture taken by the JCS specification.
//!
//! ## Guarantee for AIOS consumers
//!
//! Identical logical input — regardless of object-key insertion order, map backing
//! type, or numerically-equal float representations — produces identical canonical
//! bytes, and therefore identical [`crate::request::Request::request_hash`] and
//! [`crate::envelope::ActionEnvelope::idempotency_hash`] outputs.

use serde::Serialize;
use thiserror::Error;

/// Failure modes for canonical encoding.
///
/// The underlying [`serde_jcs`] encoder returns `serde_json::Error` for both
/// "the value's `Serialize` impl errored" and "the canonical writer failed". We
/// flatten both into a single [`CanonicalError::Projection`] variant carrying the
/// stringified message, so callers do not need to depend on `serde_json`'s error
/// type. In practice neither failure mode is reachable for the AIOS types hashed
/// by this module — every field is a primitive or a derived `Serialize`.
#[derive(Debug, Error)]
pub enum CanonicalError {
    /// The value could not be projected to canonical JSON.
    ///
    /// Carries the underlying `serde_json::Error` rendered as text. Triggered only
    /// by `Serialize` implementations that themselves fail (custom `Serializer`
    /// adapters); the types in this crate never trigger it.
    #[error("failed to project value into canonical JSON: {0}")]
    Projection(String),
}

/// Produce RFC 8785 canonical JSON bytes for any `Serialize` value.
///
/// Delegates to [`serde_jcs::to_string`] 0.1 — the Governor-approved Rust JCS
/// implementation. The output is UTF-8 text with lexicographic object key ordering,
/// no insignificant whitespace, ECMA-262 number normalization, and array order
/// preserved.
///
/// # Errors
///
/// Returns [`CanonicalError::Projection`] when the value's `Serialize` implementation
/// itself fails (the only failure surface in practice — the underlying JCS encoder
/// returns a `serde_json::Error`, which we wrap unchanged in the message text rather
/// than leak the underlying error type into our public API).
pub fn jcs_canonicalize<T: Serialize>(value: &T) -> Result<String, CanonicalError> {
    serde_jcs::to_string(value).map_err(|e| CanonicalError::Projection(e.to_string()))
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
    use super::{blake3_hash, blake3_truncated, jcs_canonicalize};
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
    fn jcs_preserves_array_order_at_top_level() {
        // Arrays must preserve declaration order (RFC 8785 §3.2.4).
        let v = json!([3, 1, 2, "z", "a"]);
        let canon = jcs_canonicalize(&v).expect("canonicalize must succeed");
        assert_eq!(canon, "[3,1,2,\"z\",\"a\"]");
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
