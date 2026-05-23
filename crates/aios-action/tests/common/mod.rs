//! Shared helpers for the `aios-action` integration test suite.
//!
//! Each integration test target (`golden_envelope`, `lifecycle_replay`, `error_paths`)
//! includes this module via `mod common;`. The helpers here only depend on the
//! public surface of the crate — they exercise the same API path that an external
//! consumer (Capability Runtime, evidence projector, etc.) would use.

#![allow(
    dead_code,
    reason = "each test target uses a subset of these helpers; allow-dead-code keeps the module compiling for every target"
)]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal — these helpers are test-only"
)]

use std::fs;
use std::path::PathBuf;

use aios_action::{
    ActionEnvelope, ActionPhase, Condition, ConditionStatus, ConditionType, Identity, Request,
    Trace,
};
use chrono::{DateTime, Duration, TimeZone, Utc};

/// Absolute path to `crates/aios-action/tests/fixtures/`, regardless of where the
/// test binary is invoked from.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Read a fixture file by leaf name (e.g. `"envelope_success_full.json"`).
pub fn load_fixture(name: &str) -> String {
    let path = fixtures_dir().join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()))
}

/// Build a fresh envelope in `Pending` with deterministic test-only identity / request / trace.
pub fn make_envelope() -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("agent:dev", true),
        Request::new("service.restart", serde_json::json!({"service": "nginx"})),
        Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
    )
}

/// Append a `True` condition observed at `observed_at`. Panics on monotonicity
/// violation (this is a test setup helper, not a production path).
pub fn push_true_at(env: &mut ActionEnvelope, ct: ConditionType, observed_at: DateTime<Utc>) {
    env.add_condition(Condition {
        condition_type: ct,
        status: ConditionStatus::True,
        observed_at,
        message: format!("{ct:?} observed in test setup"),
    })
    .expect("add_condition must succeed in test setup");
}

/// Append a `True` condition observed `now + offset_secs`.
pub fn push_true(env: &mut ActionEnvelope, ct: ConditionType, offset_secs: i64) {
    push_true_at(env, ct, Utc::now() + Duration::seconds(offset_secs));
}

/// Walk a fresh envelope all the way to `Succeeded` with the canonical four-condition set,
/// using deterministic test timestamps so the same wall-clock invocation produces an
/// envelope whose conditions stand in fixed temporal order.
pub fn walk_to_succeeded(env: &mut ActionEnvelope) {
    // T0: PolicyEvaluated observed at envelope birth.
    let t0 = Utc::now();
    push_true_at(env, ConditionType::PolicyEvaluated, t0);
    push_true_at(
        env,
        ConditionType::PolicyAccepted,
        t0 + Duration::seconds(1),
    );

    // T6: Pending -> Running.
    env.transition_to(ActionPhase::Running)
        .expect("Pending -> Running must succeed in test setup");

    push_true_at(env, ConditionType::Sandboxed, t0 + Duration::seconds(2));
    push_true_at(env, ConditionType::Executed, t0 + Duration::seconds(3));
    push_true_at(env, ConditionType::Verified, t0 + Duration::seconds(4));

    // T7: Running -> Succeeded.
    env.transition_to(ActionPhase::Succeeded)
        .expect("Running -> Succeeded must succeed in test setup");
}

/// Walk a fresh envelope to `Failed` at the policy gate (Pending -> Failed).
///
/// The canonical Failed condition set is just `{PolicyEvaluated}` per S0.1 §6.6.
pub fn walk_to_policy_denied(env: &mut ActionEnvelope) {
    let t0 = Utc::now();
    push_true_at(env, ConditionType::PolicyEvaluated, t0);
    env.transition_to(ActionPhase::Failed)
        .expect("Pending -> Failed must succeed in test setup");
}

/// Walk a fresh envelope to `RolledBack`: Pending -> Running -> `RolledBack` with the
/// canonical four-condition set `{PolicyEvaluated, Sandboxed, Executed, RolledBack}`.
pub fn walk_to_rolled_back(env: &mut ActionEnvelope) {
    let t0 = Utc::now();
    push_true_at(env, ConditionType::PolicyEvaluated, t0);
    push_true_at(
        env,
        ConditionType::PolicyAccepted,
        t0 + Duration::seconds(1),
    );
    env.transition_to(ActionPhase::Running)
        .expect("Pending -> Running must succeed in test setup");
    push_true_at(env, ConditionType::Sandboxed, t0 + Duration::seconds(2));
    push_true_at(env, ConditionType::Executed, t0 + Duration::seconds(3));
    push_true_at(env, ConditionType::RolledBack, t0 + Duration::seconds(4));
    env.transition_to(ActionPhase::RolledBack)
        .expect("Running -> RolledBack must succeed in test setup");
}

/// A frozen UTC timestamp for golden-fixture construction.
///
/// `2026-05-23T18:00:00Z` is a stable point chosen so golden fixtures never drift
/// with wall-clock progress and can be diffed bit-for-bit across reruns.
pub fn frozen_ts(offset_secs: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 23, 18, 0, 0)
        .single()
        .expect("frozen base timestamp must be a unique UTC instant")
        + Duration::seconds(offset_secs)
}
