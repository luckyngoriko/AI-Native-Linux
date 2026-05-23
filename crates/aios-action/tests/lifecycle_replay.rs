//! Integration tests: full envelope lifecycle construction + JSON replay validation.
//!
//! These tests exercise the public `aios-action` surface end-to-end:
//!
//! 1. Build a fresh envelope in `Pending`.
//! 2. Transition through one of the three terminal paths (`Succeeded` / `Failed` / `RolledBack`).
//! 3. Add the canonical condition set required by S0.1 §6.6 for the terminal phase.
//! 4. Serialize the envelope to JSON.
//! 5. Re-parse the JSON.
//! 6. Re-validate `phase ↔ conditions` consistency on the re-parsed envelope.
//! 7. Confirm that attempts to step a terminal envelope forward are rejected.
//!
//! This is the canonical replay pattern that L9 evidence-log consumers run when
//! re-hydrating archived envelopes.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

mod common;

use aios_action::{ActionEnvelope, ActionPhase, TransitionError};

use crate::common::{make_envelope, walk_to_policy_denied, walk_to_rolled_back, walk_to_succeeded};

/// Replay the Succeeded path: build → walk → serialize → parse → re-validate → reject
/// further transitions.
#[test]
fn succeeded_envelope_round_trips_and_resists_further_transitions() {
    let mut env = make_envelope();
    walk_to_succeeded(&mut env);

    assert_eq!(env.execution.phase, ActionPhase::Succeeded);
    assert!(env.execution.started_at.is_some(), "started_at must be set");
    assert!(env.execution.ended_at.is_some(), "ended_at must be set");

    // Phase ↔ conditions: a Succeeded envelope must observe all four canonical
    // conditions True. The walker installs the canonical set; this is the contract.
    env.validate_phase_conditions()
        .expect("Succeeded envelope must validate before serialization");

    // Serialize → parse → re-validate.
    let json = serde_json::to_string(&env).expect("serialize");
    let replayed: ActionEnvelope = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(replayed.execution.phase, ActionPhase::Succeeded);
    replayed
        .validate_phase_conditions()
        .expect("re-parsed envelope must still validate");

    // S0.1 §6.3 terminality: no transition is allowed out of a Succeeded envelope.
    let mut replayed_mut = replayed;
    for target in [
        ActionPhase::Pending,
        ActionPhase::Running,
        ActionPhase::Failed,
        ActionPhase::RolledBack,
    ] {
        let err = replayed_mut
            .transition_to(target)
            .expect_err("terminal envelope must reject every further transition");
        assert!(
            matches!(err, TransitionError::TerminalPhase),
            "expected TerminalPhase for target {target:?}, got {err:?}"
        );
    }
}

/// Replay the Failed path (policy-denied at gate, Pending -> Failed).
#[test]
fn failed_envelope_round_trips_and_resists_further_transitions() {
    let mut env = make_envelope();
    walk_to_policy_denied(&mut env);

    assert_eq!(env.execution.phase, ActionPhase::Failed);
    assert!(
        env.execution.started_at.is_none(),
        "started_at must be None — execution never began on a policy-denied envelope"
    );
    assert!(env.execution.ended_at.is_some(), "ended_at must be set");

    // S0.1 §6.6: Failed only requires PolicyEvaluated.
    env.validate_phase_conditions()
        .expect("Failed envelope must validate with only PolicyEvaluated");

    let json = serde_json::to_string(&env).expect("serialize");
    let replayed: ActionEnvelope = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(replayed.execution.phase, ActionPhase::Failed);
    replayed
        .validate_phase_conditions()
        .expect("re-parsed Failed envelope must still validate");

    let mut replayed_mut = replayed;
    let err = replayed_mut
        .transition_to(ActionPhase::Running)
        .expect_err("Failed is terminal — Running must be rejected");
    assert!(matches!(err, TransitionError::TerminalPhase));
}

/// Replay the `RolledBack` path: Pending -> Running -> `RolledBack` with the canonical
/// `{PolicyEvaluated, Sandboxed, Executed, RolledBack}` condition set.
#[test]
fn rolled_back_envelope_round_trips_and_resists_further_transitions() {
    let mut env = make_envelope();
    walk_to_rolled_back(&mut env);

    assert_eq!(env.execution.phase, ActionPhase::RolledBack);
    assert!(env.execution.started_at.is_some());
    assert!(env.execution.ended_at.is_some());

    env.validate_phase_conditions()
        .expect("RolledBack envelope must validate");

    let json = serde_json::to_string(&env).expect("serialize");
    let replayed: ActionEnvelope = serde_json::from_str(&json).expect("deserialize");
    replayed
        .validate_phase_conditions()
        .expect("re-parsed RolledBack envelope must still validate");
    assert_eq!(replayed.execution.phase, ActionPhase::RolledBack);

    let mut replayed_mut = replayed;
    let err = replayed_mut
        .transition_to(ActionPhase::Succeeded)
        .expect_err("RolledBack is terminal");
    assert!(matches!(err, TransitionError::TerminalPhase));
}

/// The classic S0.1 §6.3 mistake: a successfully-executed action discovers a fault
/// later and is rolled back in production. The spec is unambiguous: post-hoc
/// rollback is a **new** envelope. The original Succeeded envelope must remain
/// Succeeded and the FSM must reject the in-place flip.
#[test]
fn post_hoc_rollback_must_be_a_new_envelope_not_an_in_place_transition() {
    // 1. Build the successful envelope.
    let mut original = make_envelope();
    walk_to_succeeded(&mut original);
    assert_eq!(original.execution.phase, ActionPhase::Succeeded);

    // 2. Attempt the in-place Succeeded -> RolledBack flip. Must fail TerminalPhase.
    let err = original
        .transition_to(ActionPhase::RolledBack)
        .expect_err("in-place post-hoc rollback must be rejected");
    assert!(matches!(err, TransitionError::TerminalPhase));
    // ... and the envelope must remain unchanged.
    assert_eq!(original.execution.phase, ActionPhase::Succeeded);

    // 3. Express the rollback intent as a separate compensating envelope (the
    //    spec-mandated pattern). For this test the compensating envelope is just a
    //    second `RolledBack`-walked envelope; the production runtime would
    //    additionally bind a parent_action_id to the original.
    let mut compensating = make_envelope();
    walk_to_rolled_back(&mut compensating);
    assert_eq!(compensating.execution.phase, ActionPhase::RolledBack);

    // The two envelopes are independent. Mutating the compensating envelope did
    // not (and cannot) change the original.
    assert_eq!(original.execution.phase, ActionPhase::Succeeded);
}

/// A round-tripped envelope must hash to the same `request_hash` as the original —
/// JSON serialization MUST NOT perturb canonical request bytes.
#[test]
fn request_hash_is_stable_across_envelope_json_round_trip() {
    let mut env = make_envelope();
    walk_to_succeeded(&mut env);

    let original_hash = env.request.request_hash().expect("hash original");

    let json = serde_json::to_string(&env).expect("serialize");
    let replayed: ActionEnvelope = serde_json::from_str(&json).expect("deserialize");

    let replayed_hash = replayed.request.request_hash().expect("hash replayed");
    assert_eq!(
        original_hash, replayed_hash,
        "request_hash must be stable across envelope JSON round-trip"
    );
}

/// Function-pointer type alias for the three terminal-walker helpers in
/// `tests::common`. Kept local to this file to satisfy `clippy::type_complexity`
/// without polluting the shared common module.
type TerminalWalker = fn(&mut ActionEnvelope);

/// A long-form replay that walks every Pending -> Running -> terminal edge in turn,
/// confirming all three terminals reject all five possible further transitions.
#[test]
fn every_terminal_phase_blocks_every_further_transition() {
    let walkers: [(TerminalWalker, ActionPhase); 3] = [
        (walk_to_succeeded, ActionPhase::Succeeded),
        (walk_to_policy_denied, ActionPhase::Failed),
        (walk_to_rolled_back, ActionPhase::RolledBack),
    ];
    let targets = [
        ActionPhase::Pending,
        ActionPhase::Running,
        ActionPhase::Succeeded,
        ActionPhase::Failed,
        ActionPhase::RolledBack,
    ];

    for (walk, expected_phase) in walkers {
        let mut env = make_envelope();
        walk(&mut env);
        assert_eq!(env.execution.phase, expected_phase);

        for target in targets {
            let err = env.transition_to(target).expect_err(
                "every transition from a terminal phase must error (TerminalPhase, never Illegal)",
            );
            assert!(
                matches!(err, TransitionError::TerminalPhase),
                "expected TerminalPhase from {expected_phase:?} -> {target:?}, got {err:?}"
            );
        }
    }
}
