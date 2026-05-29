//! Manifest canonicalisation per S11.1 §5.2.
//!
//! The signing surface is the **canonical hash** `manifest_canonical_hash`, not
//! the raw proto bytes. Canonicalisation proceeds in four steps:
//!
//! 1. Project the manifest into JSON via deterministic proto3→JSON projection
//!    (struct fields serialised via `serde_json`).
//! 2. Remove `ed25519_signature` to get the hash-predictable projection.
//! 3. Hash with BLAKE3, truncate to 128 bits (16 bytes).
//! 4. Lowercase-hex-encode → 32 characters.
//!
//! The `ed25519_signature` is signed over the **ASCII bytes** of the lowercase-hex
//! `manifest_canonical_hash` string. This indirection keeps the signing surface a
//! 64-byte payload and makes the signature easy to verify against either the proto
//! or the JSON projection without re-canonicalising.

use crate::manifest::PackageManifest;

/// Encodes `bytes` as a lowercase-hex string (2 chars per byte).
fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // `write!` into a `String` is infallible — the `let _ =` silences the
        // unused-Result lint without using `.unwrap()` (which the workspace
        // lint denies).
        let _ = std::fmt::Write::write_fmt(&mut s, format_args!("{b:02x}"));
    }
    s
}

/// Computes the §5.2 content hash over arbitrary bytes.
///
/// `BLAKE3(bytes)` → first 16 bytes → 32-char lowercase hex.
///
/// This is the hash that the host computes over the fetched package content
/// and compares against [`PackageManifest::content_hash`] at step 5 of the
/// install pipeline.
#[must_use]
pub fn content_hash(bytes: &[u8]) -> String {
    let hash = blake3::hash(bytes);
    hex_encode(&hash.as_bytes()[..16])
}

/// Computes the §5.2 manifest canonical hash.
///
/// Steps:
/// 1. Serialise the manifest to a `serde_json::Value`.
/// 2. Remove `ed25519_signature` and `manifest_canonical_hash` from the object
///    — the canonical hash cannot include itself (self-referential), and the
///    signature must be excluded so the hash is a pure projection of the
///    manifest identity + content binding + scope + lifecycle fields.
/// 3. Re-serialise to a compact (no-whitespace) JSON string. Because
///    `serde_json::Map` is backed by `BTreeMap`, keys are lexicographically
///    sorted — this satisfies RFC 8785 key-ordering discipline without a
///    separate JCS pass.
/// 4. `BLAKE3(canonical_json_bytes)` → first 16 bytes → 32-char lowercase hex.
///
/// # Determinism
///
/// The canonical hash is independent of struct field declaration order because
/// the intermediate representation is a `BTreeMap` keyed by field name.
/// Two manifests with identical semantic content produce the same canonical
/// hash regardless of the order fields are defined in the source struct.
///
/// # Excluded fields
///
/// - `ed25519_signature` — the signature is computed *over* the hash; it
///   cannot be part of the hash itself.
/// - `manifest_canonical_hash` — self-referential; the hash is computed on
///   the manifest with this field cleared so that it can later be set to the
///   result without changing the result.
#[must_use]
pub fn manifest_canonical_hash(m: &PackageManifest) -> String {
    // Step 1: project to JSON value
    // serde_json::to_value is infallible for types that derive Serialize;
    // the else branch exists only to satisfy the `unwrap_used = "deny"` lint.
    let Ok(mut value) = serde_json::to_value(m) else {
        return String::new();
    };

    // Step 2: remove signature and self-referential hash fields
    if let serde_json::Value::Object(ref mut map) = value {
        map.remove("ed25519_signature");
        map.remove("manifest_canonical_hash");
    }

    // Step 3: re-serialise — BTreeMap iteration order = lexicographic key sort
    let Ok(canonical_json) = serde_json::to_string(&value) else {
        return String::new();
    };

    // Step 4: hash + truncate + hex
    let hash = blake3::hash(canonical_json.as_bytes());
    hex_encode(&hash.as_bytes()[..16])
}

/// Returns the signing payload for a given canonical hash hex string.
///
/// Per §5.2 last paragraph, the Ed25519 signature is computed over the
/// **ASCII bytes** of the lowercase-hex `manifest_canonical_hash` string.
/// This function exposes that slice so callers can sign or verify without
/// re-deriving the canonical hash.
///
/// # Example
///
/// ```ignore
/// let ch = manifest_canonical_hash(&manifest);
/// let payload = signing_payload(&ch);
/// // payload is &[u8] — the ASCII bytes of the 32-char hex string
/// signing_key.sign(payload);
/// ```
#[must_use]
pub const fn signing_payload(canonical_hash_hex: &str) -> &[u8] {
    canonical_hash_hex.as_bytes()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::needless_collect,
    clippy::format_collect,
    clippy::too_many_arguments,
    clippy::float_cmp
)]
mod tests {
    use super::*;
    use crate::ids::{PackageSigningKeyId, PublisherRootId};
    use crate::manifest::{NetworkManifestRef, SandboxProfileRef};
    use crate::mirror::MirrorSemantic;
    use crate::package_kind::{InstallScope, PackageKind};
    use crate::repository::{RepositoryKind, UpdateChannel};
    use crate::trust::PublisherTrustLevel;
    use chrono::{DateTime, Utc};

    fn make_test_manifest() -> PackageManifest {
        // Freeze issued_at to a fixed timestamp so that two calls to
        // make_test_manifest() produce byte-identical manifests for
        // determinism tests.
        // If the fixed parse fails (shouldn't), fall back to Utc::now()
        // which is non-deterministic but won't panic.
        let fixed_time = DateTime::parse_from_rfc3339("2026-05-29T12:00:00Z")
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));
        PackageManifest {
            package_id: "pkg:testvendor:testapp".into(),
            version: "1.0.0".into(),
            kind: PackageKind::App,
            publisher_trust: PublisherTrustLevel::Verified,
            publisher_root_id: PublisherRootId("pub:testvendor".into()),
            package_signing_key_id: PackageSigningKeyId("pks:testvendor:release".into()),
            content_hash: "a".repeat(32),
            manifest_canonical_hash: String::new(),
            ed25519_signature: vec![0xAA; 64],
            installable_scope: InstallScope::Either,
            required_sandbox: SandboxProfileRef("test-profile".into()),
            declared_capabilities: vec!["filesystem.read".into()],
            network_manifest: NetworkManifestRef("test-net".into()),
            issued_at: fixed_time,
            eol_at: None,
            channel: UpdateChannel::Stable,
            originating_repository: RepositoryKind::AiosVerifiedRepo,
            mirror_url: "https://mirror.example.com".into(),
            mirror_semantic: MirrorSemantic::Cached,
        }
    }

    #[test]
    fn content_hash_is_32_lower_hex() {
        let ch = content_hash(b"hello world");
        assert_eq!(ch.len(), 32);
        assert!(ch
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn content_hash_is_stable() {
        let a = content_hash(b"test content");
        let b = content_hash(b"test content");
        assert_eq!(a, b);
    }

    #[test]
    fn different_content_produces_different_hash() {
        let a = content_hash(b"foo");
        let b = content_hash(b"bar");
        assert_ne!(a, b);
    }

    #[test]
    fn canonical_hash_is_32_lower_hex() {
        let m = make_test_manifest();
        let ch = manifest_canonical_hash(&m);
        assert_eq!(ch.len(), 32);
        assert!(ch
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn canonical_hash_determinism() {
        let m = make_test_manifest();
        let a = manifest_canonical_hash(&m);
        let b = manifest_canonical_hash(&m);
        assert_eq!(a, b);
    }

    #[test]
    fn canonical_hash_changes_when_field_changes() {
        let mut m = make_test_manifest();
        let a = manifest_canonical_hash(&m);
        m.version = "2.0.0".into();
        let b = manifest_canonical_hash(&m);
        assert_ne!(a, b);
    }

    #[test]
    fn canonical_hash_excludes_signature() {
        let mut m = make_test_manifest();
        let a = manifest_canonical_hash(&m);
        m.ed25519_signature = vec![0xFF; 64];
        let b = manifest_canonical_hash(&m);
        // Signature is excluded from canonical hash → no change
        assert_eq!(a, b);
    }

    #[test]
    fn signing_payload_is_ascii_bytes() {
        let ch = "abcdef0123456789abcdef0123456789";
        let payload = signing_payload(ch);
        assert_eq!(payload, ch.as_bytes());
        assert_eq!(payload.len(), 32);
    }

    #[test]
    fn canonical_hash_two_identical_manifests_same_hash() {
        let m1 = make_test_manifest();
        let m2 = make_test_manifest();
        assert_eq!(manifest_canonical_hash(&m1), manifest_canonical_hash(&m2));
    }
}
