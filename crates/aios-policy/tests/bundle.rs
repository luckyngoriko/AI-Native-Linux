//! Integration tests for the T-022 policy bundle loader (S2.3 §12).
//!
//! Coverage:
//!
//! - Happy-path round-trip (sign → serialise → load → verify → assert struct
//!   equality).
//! - Every documented failure mode of [`BundleLoader::load_from_bytes`]:
//!   malformed JSON, bad-signature, unknown authority, version-pin mismatch,
//!   per-rule condition parse failure.
//! - Determinism (same bytes parsed twice ⇒ equal `PolicyBundle`).
//! - Empty-rules bundle is a valid bundle.
//! - 50-rule smoke for performance / scale.
//! - `RuleScope` serde round-trip for every variant.
//! - `PolicyError` bundle-class variant `Display` strings match the canonical
//!   short text declared on the variant.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    clippy::unwrap_used
)]

use std::collections::HashMap;

use chrono::{TimeZone, Utc};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;

use aios_policy::bundle::{PolicyBundle, PolicyRule, RuleEffect, RuleScope};
use aios_policy::bundle_loader::BundleLoader;
use aios_policy::error::PolicyError;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Mint a fresh Ed25519 keypair via the OS CSPRNG.
fn fresh_keypair() -> (SigningKey, VerifyingKey) {
    let sk = SigningKey::generate(&mut OsRng);
    let vk = sk.verifying_key();
    (sk, vk)
}

/// Build an unsigned bundle skeleton with the supplied rules.
fn unsigned_bundle(authority: &str, rules: Vec<PolicyRule>) -> PolicyBundle {
    PolicyBundle {
        bundle_version: "polb_0123456789abcdef0123456789abcdef".to_string(),
        bundle_id: "test-base.v1".to_string(),
        created_at: Utc.with_ymd_and_hms(2026, 5, 24, 12, 0, 0).unwrap(),
        signing_authority: authority.to_string(),
        signature_ed25519: Vec::new(),
        rules,
    }
}

/// Sign the supplied bundle in-place, replacing `signature_ed25519` with a
/// fresh Ed25519 signature over the canonical body bytes.
fn sign_bundle(bundle: &mut PolicyBundle, sk: &SigningKey) {
    let body = bundle.canonical_signed_body_bytes().unwrap();
    let sig = sk.sign(&body);
    bundle.signature_ed25519 = sig.to_bytes().to_vec();
}

/// Returns serialized JSON bytes of a freshly signed bundle plus a loader
/// whose trust store contains the publisher key under `authority_name`.
fn signed_bundle_and_loader(
    authority_name: &str,
    rules: Vec<PolicyRule>,
) -> (Vec<u8>, BundleLoader) {
    let (sk, vk) = fresh_keypair();
    let mut bundle = unsigned_bundle(authority_name, rules);
    sign_bundle(&mut bundle, &sk);
    let bytes = serde_json::to_vec(&bundle).unwrap();
    let mut trust = HashMap::new();
    trust.insert(authority_name.to_string(), vk);
    (bytes, BundleLoader::new(trust))
}

/// Construct a minimal-valid rule (no conditions, no constraints).
fn allow_rule(rule_id: &str) -> PolicyRule {
    PolicyRule {
        rule_id: rule_id.to_string(),
        scope: RuleScope::Global,
        effect: RuleEffect::Allow,
        priority: 0,
        subjects: vec!["human:lucky".to_string()],
        actions: vec!["service.restart".to_string()],
        conditions: Vec::new(),
        constraints: None,
        approval: None,
        reason_code: "ScopedAllow".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn happy_path_round_trip_parses_and_verifies() {
    let (bytes, loader) = signed_bundle_and_loader("publisher-a", vec![allow_rule("r1")]);
    let bundle = loader
        .load_from_bytes(&bytes)
        .expect("happy path must load");
    assert_eq!(bundle.bundle_id, "test-base.v1");
    assert_eq!(bundle.rules.len(), 1);
    assert_eq!(bundle.rules[0].rule_id, "r1");
}

#[test]
fn malformed_json_yields_invalid_policy_bundle() {
    let loader = BundleLoader::new(HashMap::new());
    let err = loader
        .load_from_bytes(b"{ not valid json")
        .expect_err("malformed JSON must reject");
    match err {
        PolicyError::InvalidPolicyBundle(msg) => {
            assert!(msg.starts_with("JSON deserialise:"), "msg = {msg}");
        }
        other => panic!("expected InvalidPolicyBundle, got {other:?}"),
    }
}

#[test]
fn per_rule_condition_parse_failure_rejects_whole_bundle() {
    let mut bad_rule = allow_rule("r_bad_cond");
    // §9.1 grammar: `or` is disallowed (only `and` joiners), so this fails parse.
    bad_rule.conditions = vec!["subject.recovery_mode = false or true".to_string()];
    let (bytes, loader) = signed_bundle_and_loader("publisher-b", vec![bad_rule]);

    let err = loader.load_from_bytes(&bytes).expect_err("must reject");
    match err {
        PolicyError::InvalidPolicyBundle(msg) => {
            assert!(
                msg.starts_with("rule r_bad_cond condition: "),
                "msg = {msg}"
            );
        }
        other => panic!("expected InvalidPolicyBundle, got {other:?}"),
    }
}

#[test]
fn bad_signature_yields_bundle_signature_invalid() {
    let (mut bytes, loader) = signed_bundle_and_loader("publisher-c", vec![allow_rule("r1")]);
    // Flip a byte in the JSON to invalidate the signature without breaking the JSON shape.
    // Replace `human:lucky` with `human:alice` to perturb the canonical body the signature
    // covers without touching the signature itself.
    let s = String::from_utf8(bytes.clone()).unwrap();
    let mutated = s.replace("human:lucky", "human:alice");
    assert_ne!(s, mutated, "string substitution must take effect");
    bytes = mutated.into_bytes();

    let err = loader
        .load_from_bytes(&bytes)
        .expect_err("perturbed body must fail signature");
    assert!(matches!(err, PolicyError::BundleSignatureInvalid));
}

#[test]
fn unknown_authority_rejected() {
    let (sk, _vk) = fresh_keypair();
    let mut bundle = unsigned_bundle("publisher-NOT-IN-TRUST-STORE", vec![allow_rule("r1")]);
    sign_bundle(&mut bundle, &sk);
    let bytes = serde_json::to_vec(&bundle).unwrap();
    // Empty trust store → any authority is unknown.
    let loader = BundleLoader::new(HashMap::new());

    let err = loader.load_from_bytes(&bytes).expect_err("must reject");
    match err {
        PolicyError::BundleUnknownAuthority(name) => {
            assert_eq!(name, "publisher-NOT-IN-TRUST-STORE");
        }
        other => panic!("expected BundleUnknownAuthority, got {other:?}"),
    }
}

#[test]
fn version_pin_mismatch_rejected() {
    let (sk, vk) = fresh_keypair();
    let mut bundle = unsigned_bundle("publisher-d", vec![allow_rule("r1")]);
    sign_bundle(&mut bundle, &sk);
    let bytes = serde_json::to_vec(&bundle).unwrap();
    let mut trust = HashMap::new();
    trust.insert("publisher-d".to_string(), vk);
    let loader = BundleLoader::with_version_pin(trust, "polb_DIFFERENT_PIN");

    let err = loader.load_from_bytes(&bytes).expect_err("must reject");
    match err {
        PolicyError::BundleVersionMismatch { expected, found } => {
            assert_eq!(expected, "polb_DIFFERENT_PIN");
            assert_eq!(found, "polb_0123456789abcdef0123456789abcdef");
        }
        other => panic!("expected BundleVersionMismatch, got {other:?}"),
    }
}

#[test]
fn empty_rules_list_is_valid() {
    let (bytes, loader) = signed_bundle_and_loader("publisher-e", Vec::new());
    let bundle = loader
        .load_from_bytes(&bytes)
        .expect("empty bundle must load");
    assert!(bundle.rules.is_empty());
}

#[test]
fn determinism_same_bytes_parse_to_equal_struct() {
    let (bytes, loader) = signed_bundle_and_loader("publisher-f", vec![allow_rule("r1")]);
    let a = loader.load_from_bytes(&bytes).unwrap();
    let b = loader.load_from_bytes(&bytes).unwrap();
    assert_eq!(a, b);
    // Re-serialise and re-parse — must still be equal.
    let bytes_again = serde_json::to_vec(&a).unwrap();
    let c = loader.load_from_bytes(&bytes_again).unwrap();
    assert_eq!(a, c);
}

#[test]
fn signature_round_trip_full_positive_path() {
    // Explicit positive coverage: ed25519-dalek keygen + sign + bundle + verify.
    let (sk, vk) = fresh_keypair();
    let mut bundle = unsigned_bundle("publisher-g", vec![allow_rule("rg1"), allow_rule("rg2")]);
    sign_bundle(&mut bundle, &sk);
    let bytes = serde_json::to_vec(&bundle).unwrap();
    let mut trust = HashMap::new();
    trust.insert("publisher-g".to_string(), vk);
    let loader = BundleLoader::new(trust);

    let loaded = loader.load_from_bytes(&bytes).unwrap();
    assert_eq!(
        loaded.signature_ed25519.len(),
        64,
        "Ed25519 sig is 64 bytes"
    );
    assert_eq!(loaded.rules.len(), 2);
}

#[test]
fn rule_scope_serde_round_trips_every_variant() {
    use serde_json::json;
    for (variant, label) in [
        (RuleScope::Global, "GLOBAL"),
        (RuleScope::PerSubjectType, "PER_SUBJECT_TYPE"),
        (RuleScope::PerActionTarget, "PER_ACTION_TARGET"),
        (RuleScope::PerSubject, "PER_SUBJECT"),
        (RuleScope::PerAction, "PER_ACTION"),
    ] {
        let s = serde_json::to_string(&variant).unwrap();
        assert_eq!(s, json!(label).to_string());
        let back: RuleScope = serde_json::from_str(&s).unwrap();
        assert_eq!(back, variant);
    }
}

#[test]
fn fifty_rule_bundle_parses_and_signs_cleanly() {
    let rules: Vec<PolicyRule> = (0..50).map(|i| allow_rule(&format!("r{i}"))).collect();
    let (bytes, loader) = signed_bundle_and_loader("publisher-h", rules);
    let bundle = loader
        .load_from_bytes(&bytes)
        .expect("50-rule bundle must load");
    assert_eq!(bundle.rules.len(), 50);
    // Spot-check first / last rule_id.
    assert_eq!(bundle.rules[0].rule_id, "r0");
    assert_eq!(bundle.rules[49].rule_id, "r49");
}

#[test]
fn policy_error_bundle_variant_display_strings_match_canonical_text() {
    let e = PolicyError::InvalidPolicyBundle("rule x condition: nope".to_string());
    assert_eq!(
        e.to_string(),
        "invalid policy bundle: rule x condition: nope"
    );

    let e = PolicyError::BundleSignatureInvalid;
    assert_eq!(e.to_string(), "bundle signature invalid");

    let e = PolicyError::BundleVersionMismatch {
        expected: "polb_aaa".to_string(),
        found: "polb_bbb".to_string(),
    };
    assert_eq!(
        e.to_string(),
        "bundle version mismatch: expected polb_aaa, found polb_bbb"
    );

    let e = PolicyError::BundleUnknownAuthority("not-trusted".to_string());
    assert_eq!(
        e.to_string(),
        "bundle signed by unknown authority: not-trusted"
    );
}

#[test]
fn rule_effect_unspecified_rejected_with_canonical_message() {
    // RULE_EFFECT_UNSPECIFIED is reserved for proto3 wire compat and must not
    // round-trip through the loader (mirrors the spec §11.2 enum tag-0 rule).
    let mut bad = allow_rule("r_unspec");
    bad.effect = RuleEffect::Unspecified;
    let (bytes, loader) = signed_bundle_and_loader("publisher-i", vec![bad]);

    let err = loader.load_from_bytes(&bytes).expect_err("must reject");
    match err {
        PolicyError::InvalidPolicyBundle(msg) => {
            assert!(msg.starts_with("rule r_unspec effect: "), "msg = {msg}");
        }
        other => panic!("expected InvalidPolicyBundle, got {other:?}"),
    }
}

#[test]
fn version_pin_match_allows_load() {
    // Positive case for the version pin path — same expected and found ⇒ load.
    let (sk, vk) = fresh_keypair();
    let mut bundle = unsigned_bundle("publisher-j", vec![allow_rule("r1")]);
    sign_bundle(&mut bundle, &sk);
    let bytes = serde_json::to_vec(&bundle).unwrap();
    let mut trust = HashMap::new();
    trust.insert("publisher-j".to_string(), vk);
    let loader = BundleLoader::with_version_pin(trust, "polb_0123456789abcdef0123456789abcdef");

    let loaded = loader.load_from_bytes(&bytes).expect("must load");
    assert_eq!(
        loaded.bundle_version,
        "polb_0123456789abcdef0123456789abcdef"
    );
}

#[test]
fn load_from_file_delegates_to_load_from_bytes() {
    use std::io::Write;

    let (bytes, loader) = signed_bundle_and_loader("publisher-k", vec![allow_rule("r1")]);
    let tmp = std::env::temp_dir().join(format!(
        "aios-policy-bundle-test-{}.json",
        ulid::Ulid::new()
    ));
    {
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(&bytes).unwrap();
    }

    let loaded = loader
        .load_from_file(&tmp)
        .expect("file-backed load must succeed");
    assert_eq!(loaded.rules.len(), 1);

    // Clean up.
    let _ = std::fs::remove_file(&tmp);
}
