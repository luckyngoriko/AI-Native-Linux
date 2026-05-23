//! Integration tests: golden JSON fixtures.
//!
//! Each fixture under `tests/fixtures/*.json` represents a hand-written, canonical
//! envelope shape that captures one branch of the lifecycle FSM (success, policy
//! denial, post-hoc rollback). The tests in this file:
//!
//! 1. parse each fixture into an `ActionEnvelope`,
//! 2. validate the parsed envelope's phase ↔ conditions consistency,
//! 3. re-serialize via the JCS canonicalizer and confirm the canonical form is
//!    stable across the parse → re-serialize cycle,
//! 4. assert phase / structural expectations specific to each fixture.
//!
//! The final test in the file pins the new RFC 8785 / ECMA-262 number normalization
//! deficit that was deliberately deferred from T-002.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

mod common;

use aios_action::{jcs_canonicalize, ActionEnvelope, ActionPhase, Request};

use crate::common::load_fixture;

// ---- envelope_success_full.json --------------------------------------------------

#[test]
fn golden_success_full_parses_and_validates() {
    let raw = load_fixture("envelope_success_full.json");
    let env: ActionEnvelope = serde_json::from_str(&raw).expect("fixture must parse");

    // Structural expectations specific to the success fixture.
    assert_eq!(env.schema_version, "aios.action.v1alpha1");
    assert_eq!(env.execution.phase, ActionPhase::Succeeded);
    assert!(
        env.execution.started_at.is_some(),
        "Succeeded envelope must have started_at"
    );
    assert!(
        env.execution.ended_at.is_some(),
        "Succeeded envelope must have ended_at"
    );
    assert!(
        env.execution.sandbox_profile_id.is_some(),
        "Succeeded fixture pins sandbox_profile_id"
    );

    // Canonical condition set must be satisfied (PolicyEvaluated, Sandboxed,
    // Executed, Verified — all True).
    env.validate_phase_conditions()
        .expect("golden Succeeded fixture must satisfy canonical condition set");

    // Identity flags from the fixture.
    assert!(env.identity.is_ai);
    assert!(env.identity.session_id.is_some());

    // Idempotency hash must be computable (the fixture pins an idempotency_key).
    assert!(env.request.idempotency_key.is_some());
    let h = env
        .idempotency_hash()
        .expect("hash must succeed")
        .expect("key is set so hash must be Some");
    assert_eq!(h.len(), 64, "BLAKE3 hex must be 64 chars");
}

#[test]
fn golden_success_full_canonical_round_trip_is_stable() {
    let raw = load_fixture("envelope_success_full.json");
    let env: ActionEnvelope = serde_json::from_str(&raw).expect("fixture must parse");

    // Step 1: canonicalize the parsed envelope.
    let canon_a = jcs_canonicalize(&env).expect("canonicalize a");

    // Step 2: round-trip through the canonical form and canonicalize again.
    let reparsed: ActionEnvelope =
        serde_json::from_str(&canon_a).expect("canonical form must reparse");
    let canon_b = jcs_canonicalize(&reparsed).expect("canonicalize b");

    assert_eq!(
        canon_a, canon_b,
        "JCS canonical form must be a fixed point under parse-canonicalize"
    );

    // And the original fixture's JCS canonicalization must match: parse the raw
    // fixture as Value, canonicalize, then assert equality with the envelope-derived
    // form. This catches any structural drift between the fixture and the Rust
    // struct (e.g. an unknown field that would be silently dropped on deserialize
    // would change the canonical form).
    let raw_value: serde_json::Value =
        serde_json::from_str(&raw).expect("fixture must parse as Value");
    let raw_canon = jcs_canonicalize(&raw_value).expect("canonicalize raw");
    assert_eq!(
        canon_a, raw_canon,
        "fixture's canonical bytes must match the envelope's canonical bytes — \
         a mismatch means an unknown field was silently dropped during deserialize"
    );
}

// ---- envelope_policy_denied.json -------------------------------------------------

#[test]
fn golden_policy_denied_parses_and_validates() {
    let raw = load_fixture("envelope_policy_denied.json");
    let env: ActionEnvelope = serde_json::from_str(&raw).expect("fixture must parse");

    assert_eq!(env.execution.phase, ActionPhase::Failed);
    assert!(
        env.execution.started_at.is_none(),
        "policy-denied envelope never began execution — started_at must be None"
    );
    assert!(env.execution.ended_at.is_some());
    assert!(
        env.execution.sandbox_profile_id.is_none(),
        "no sandbox bound when policy denies at the gate"
    );

    // S0.1 §6.6: Failed only requires PolicyEvaluated.
    env.validate_phase_conditions()
        .expect("Failed fixture must satisfy minimal canonical set");

    assert_eq!(env.execution.conditions.len(), 1);
}

#[test]
fn golden_policy_denied_canonical_round_trip_is_stable() {
    let raw = load_fixture("envelope_policy_denied.json");
    let env: ActionEnvelope = serde_json::from_str(&raw).expect("fixture must parse");

    let canon_a = jcs_canonicalize(&env).expect("canonicalize");
    let reparsed: ActionEnvelope = serde_json::from_str(&canon_a).expect("canonical reparse");
    let canon_b = jcs_canonicalize(&reparsed).expect("canonicalize again");
    assert_eq!(canon_a, canon_b);
}

// ---- envelope_rolled_back.json ---------------------------------------------------

#[test]
fn golden_rolled_back_parses_and_carries_parent_action_id() {
    let raw = load_fixture("envelope_rolled_back.json");
    let env: ActionEnvelope = serde_json::from_str(&raw).expect("fixture must parse");

    assert_eq!(env.execution.phase, ActionPhase::RolledBack);
    assert!(env.execution.started_at.is_some());
    assert!(env.execution.ended_at.is_some());

    // S0.1 §6.3 post-hoc rollback pattern: this is a compensating envelope that
    // names the original via parent_action_id, NOT an in-place Succeeded -> RolledBack
    // transition (which would be illegal under the FSM).
    assert!(
        env.request.parent_action_id.is_some(),
        "compensating envelope must reference the original via parent_action_id (§6.3 pattern)"
    );

    // Canonical condition set for RolledBack:
    //   {PolicyEvaluated, Sandboxed, Executed, RolledBack}
    env.validate_phase_conditions()
        .expect("RolledBack fixture must satisfy canonical condition set");
}

#[test]
fn golden_rolled_back_canonical_round_trip_is_stable() {
    let raw = load_fixture("envelope_rolled_back.json");
    let env: ActionEnvelope = serde_json::from_str(&raw).expect("fixture must parse");

    let canon_a = jcs_canonicalize(&env).expect("canonicalize");
    let reparsed: ActionEnvelope = serde_json::from_str(&canon_a).expect("canonical reparse");
    let canon_b = jcs_canonicalize(&reparsed).expect("canonicalize again");
    assert_eq!(canon_a, canon_b);
}

// ---- RFC 8785 ECMA-262 number normalization (T-002 deferred deficit) -------------

/// Verifies that the new `serde_jcs`-based canonicalizer treats numerically-equal
/// `f64` representations as a single canonical string per RFC 8785 §3.2.2.3
/// (ECMA-262 §6.12.7 / §7.1.12.1 number-to-string normalization).
///
/// This was the explicitly-deferred deficit from T-002: `serde_json`'s default
/// number formatting would emit `1.5e2` and `150.0` as distinct strings, producing
/// distinct `request_hash` values for what are, in IEEE-754 terms, the same number.
/// Under RFC 8785, both must produce the same canonical bytes.
#[test]
fn rfc8785_number_normalization_collapses_equal_floats_to_one_hash() {
    // Two requests with logically identical numeric content expressed via
    // different `serde_json::Value` constructions. After RFC 8785 number
    // normalization, their canonical bytes — and therefore their request_hash
    // values — must match.
    //
    // `serde_json::Number::from_f64(150.0)` and the literal `1.5e2` both decode
    // to the same IEEE-754 double, so ECMA-262 §7.1.12.1 yields the same
    // shortest-roundtrip string ("150") for both.
    let a = Request::new(
        "metrics.set",
        serde_json::json!({
            "value": 150.0,
            "label": "throughput",
        }),
    );
    let b_target = {
        // Construct the float via a different syntactic path. Both targets must
        // be numerically equal at the f64 level — that is the precondition for
        // RFC 8785 §3.2.2.3 to mandate identical canonical strings.
        let mut map = serde_json::Map::new();
        map.insert(
            "value".to_owned(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(1.5e2)
                    .expect("1.5e2 is finite and representable as a serde_json::Number"),
            ),
        );
        map.insert("label".to_owned(), serde_json::json!("throughput"));
        serde_json::Value::Object(map)
    };
    let b = Request::new("metrics.set", b_target);

    let canon_a = jcs_canonicalize(&a).expect("canonicalize a");
    let canon_b = jcs_canonicalize(&b).expect("canonicalize b");
    assert_eq!(
        canon_a, canon_b,
        "RFC 8785 §3.2.2.3 (ECMA-262 number normalization) requires \
         numerically-equal floats to produce identical canonical strings; \
         canon_a={canon_a} canon_b={canon_b}"
    );

    // And therefore the request_hash values must match. This is the T-002 deficit
    // being closed: under the old `serde_json::to_string` path these two requests
    // would have hashed differently, breaking idempotency_key dedup for any
    // caller that passed `1.5e2` to one retry and `150.0` to the next.
    let ha = a.request_hash().expect("hash a");
    let hb = b.request_hash().expect("hash b");
    assert_eq!(
        ha, hb,
        "request_hash MUST be identical for numerically-equal floats (§3.3 rule 1 safe-retry)"
    );
}

/// Confirms integer normalization works as well — `150` vs `150.0` represent
/// numerically-equal values but are stored as different `serde_json` variants
/// (`Number::I64(150)` vs `Number::F64(150.0)`). Their canonical forms must match.
#[test]
fn rfc8785_integer_and_float_one_fifty_collapse_to_one_canonical_form() {
    let int_form = serde_json::json!({"n": 150});
    let float_form = serde_json::json!({"n": 150.0});

    let canon_int = jcs_canonicalize(&int_form).expect("canonicalize int");
    let canon_float = jcs_canonicalize(&float_form).expect("canonicalize float");

    assert_eq!(
        canon_int, canon_float,
        "RFC 8785 §3.2.2.3: 150 and 150.0 are equal under IEEE-754 and must produce \
         the same canonical string; canon_int={canon_int} canon_float={canon_float}"
    );
}
