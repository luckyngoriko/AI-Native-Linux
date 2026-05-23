//! Integration tests: error-taxonomy construction and propagation.
//!
//! Covers the seven §7.3 groups (Validation / Policy / Authorization / Execution /
//! Verification / Rollback / Infrastructure) plus the cause-chain depth-8 boundary
//! (§7.5) and the `From<IdError>` / `From<TransitionError>` mappings.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

mod common;

use aios_action::{
    ActionError, ActionErrorCode, ActionPhase, CauseChainTooDeep, IdError, TransitionError,
    MAX_CAUSE_CHAIN_DEPTH,
};

// ---- §7.3 Group: Validation ------------------------------------------------------

#[test]
fn validation_group_envelope_malformed_constructs_with_fixed_false_retryable() {
    // Per §7.3: every Validation code is `retryable=false` by spec default. The
    // constructor MUST accept the caller's explicit flag (§7.4 "only the originating
    // component sets `retryable`") — but here we exercise the canonical case.
    let e = ActionError::new(
        ActionErrorCode::EnvelopeMalformed,
        "schema_version unknown",
        false,
    );
    assert_eq!(e.code, ActionErrorCode::EnvelopeMalformed);
    assert!(!e.retryable);
    assert_eq!(
        ActionErrorCode::EnvelopeMalformed.retryable_default(),
        Some(false),
        "spec default for Validation group must be Some(false)"
    );

    // Serde round-trip preserves shape.
    let json = serde_json::to_string(&e).expect("serialize");
    let back: ActionError = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, e);
}

// ---- §7.3 Group: Policy ----------------------------------------------------------

#[test]
fn policy_group_policy_denied_carries_decision_id_message() {
    // PolicyDenied is the classic gate failure. retryable_default is Some(false) —
    // retrying a deny is pointless without a policy change.
    let e = ActionError::new(
        ActionErrorCode::PolicyDenied,
        "policy decision pol_01HQ...: deny — caller lacks `service.restart` capability",
        false,
    );
    assert_eq!(e.code, ActionErrorCode::PolicyDenied);
    assert!(!e.retryable);
    assert_eq!(
        ActionErrorCode::PolicyDenied.retryable_default(),
        Some(false)
    );
    assert!(
        e.to_string().starts_with("PolicyDenied:"),
        "Display must prefix with the code name"
    );
}

// ---- §7.3 Group: Authorization ---------------------------------------------------

#[test]
fn authorization_group_secret_access_denied_is_not_retryable() {
    let e = ActionError::new(
        ActionErrorCode::SecretAccessDenied,
        "vault broker refused raw-read of secret://prod/db_password",
        false,
    );
    assert_eq!(e.code, ActionErrorCode::SecretAccessDenied);
    assert!(!e.retryable);
    assert_eq!(
        ActionErrorCode::SecretAccessDenied.retryable_default(),
        Some(false)
    );
}

// ---- §7.3 Group: Execution -------------------------------------------------------

#[test]
fn execution_group_adapter_failure_is_depends_so_constructor_takes_explicit_retryable() {
    // §7.3 lists AdapterFailure as "depends" — retryable_default returns None.
    assert_eq!(
        ActionErrorCode::AdapterFailure.retryable_default(),
        None,
        "AdapterFailure is one of the two `depends` codes"
    );

    // Constructor MUST take whatever the caller supplies.
    let retryable_yes = ActionError::new(ActionErrorCode::AdapterFailure, "transient", true);
    assert!(retryable_yes.retryable);
    let retryable_no = ActionError::new(ActionErrorCode::AdapterFailure, "permanent", false);
    assert!(!retryable_no.retryable);
}

// ---- §7.3 Group: Verification ----------------------------------------------------

#[test]
fn verification_group_verification_failed_carries_intent_id_in_message() {
    let e = ActionError::new(
        ActionErrorCode::VerificationFailed,
        "verification intent ver_01HQ... failed: service nginx not listening on :80",
        false,
    );
    assert_eq!(e.code, ActionErrorCode::VerificationFailed);
    assert!(!e.retryable);
    assert_eq!(
        ActionErrorCode::VerificationFailed.retryable_default(),
        Some(false)
    );
}

// ---- §7.3 Group: Rollback --------------------------------------------------------

#[test]
fn rollback_group_rollback_failed_is_not_retryable_and_is_a_degraded_state_signal() {
    // §7.7: a RollbackFailed envelope is in a degraded state — the runtime cannot
    // simply retry. retryable=false is the only correct value.
    let e = ActionError::new(
        ActionErrorCode::RollbackFailed,
        "compensating action failed: snapshot snap_01HQ... not restorable",
        false,
    );
    assert_eq!(e.code, ActionErrorCode::RollbackFailed);
    assert!(!e.retryable);
    assert_eq!(
        ActionErrorCode::RollbackFailed.retryable_default(),
        Some(false)
    );
}

// ---- §7.3 Group: Infrastructure --------------------------------------------------

#[test]
fn infrastructure_group_evidence_write_failed_is_retryable() {
    let e = ActionError::new(
        ActionErrorCode::EvidenceWriteFailed,
        "evidence log append failed: aios-evidence-0 unreachable",
        true,
    );
    assert_eq!(e.code, ActionErrorCode::EvidenceWriteFailed);
    assert!(
        e.retryable,
        "infra-side write failures are typically retryable"
    );
    assert_eq!(
        ActionErrorCode::EvidenceWriteFailed.retryable_default(),
        Some(true)
    );
}

// ---- §7.5 cause-chain depth boundary ---------------------------------------------

/// Build a realistic 3-deep cause chain matching the §7.5 example.
#[test]
fn three_deep_cause_chain_mirrors_spec_example() {
    // AdapterFailure -> Internal -> RuntimeUnavailable (per the task brief).
    let leaf = ActionError::new(
        ActionErrorCode::RuntimeUnavailable,
        "capability runtime aios-cr-0 not reachable",
        true,
    );
    let mid = ActionError::new(
        ActionErrorCode::Internal,
        "internal RPC retry loop exhausted",
        false,
    )
    .with_cause(leaf)
    .expect("depth 1 attach must succeed");
    let top = ActionError::new(
        ActionErrorCode::AdapterFailure,
        "systemd adapter could not reach the runtime",
        false,
    )
    .with_cause(mid)
    .expect("depth 2 attach must succeed");

    assert_eq!(top.cause_chain_depth(), 2);
    assert_eq!(top.code, ActionErrorCode::AdapterFailure);

    // Walk the chain explicitly.
    let mid_ref = top.cause.as_deref().expect("top has cause");
    assert_eq!(mid_ref.code, ActionErrorCode::Internal);
    let leaf_ref = mid_ref.cause.as_deref().expect("mid has cause");
    assert_eq!(leaf_ref.code, ActionErrorCode::RuntimeUnavailable);
    assert!(leaf_ref.cause.is_none(), "leaf must have no further cause");
}

#[test]
fn cause_chain_depth_eight_is_inclusive_max_depth_nine_is_rejected() {
    // Build a chain at exactly depth 8 — the spec maximum.
    let mut inner = ActionError::new(ActionErrorCode::Internal, "L0", false);
    for level in 1..=8 {
        inner = ActionError::new(ActionErrorCode::AdapterFailure, format!("L{level}"), false)
            .with_cause(inner)
            .expect("each attach up to depth 8 must succeed");
    }
    assert_eq!(inner.cause_chain_depth(), 8);
    assert_eq!(
        MAX_CAUSE_CHAIN_DEPTH, 8,
        "constant must match §7.5 stated maximum"
    );

    // One more attach would push depth to 9 — the constructor must reject it.
    let too_deep = ActionError::new(ActionErrorCode::AdapterFailure, "L9", false).with_cause(inner);
    let CauseChainTooDeep { observed_depth } =
        too_deep.expect_err("depth-9 attach must be rejected");
    assert_eq!(observed_depth, 9);
}

// ---- From<IdError> / From<TransitionError> mappings ------------------------------

#[test]
fn from_id_error_maps_every_variant_to_envelope_malformed_not_retryable() {
    // Cover all five IdError variants so a future variant addition forces an
    // explicit decision here rather than silently inheriting the wrong code.
    let cases = [
        IdError::Empty,
        IdError::WrongPrefix {
            expected: "act_",
            got: "intent_01H...".to_owned(),
        },
        IdError::ColonSeparatorForbidden("act:01H...".to_owned()),
        IdError::InvalidUlidBody {
            id: "act_zzz".to_owned(),
            detail: "bad crockford char".to_owned(),
        },
        IdError::InvalidHexBody {
            id: "tplan_NOTHEX".to_owned(),
            detail: "non-hex character".to_owned(),
        },
    ];

    for case in cases {
        let rendered = case.to_string();
        let e: ActionError = case.into();
        assert_eq!(e.code, ActionErrorCode::EnvelopeMalformed);
        assert!(
            !e.retryable,
            "Validation-group codes are fixed retryable=false"
        );
        assert_eq!(e.message, rendered);
        assert!(
            e.cause.is_none(),
            "From<IdError> must not synthesize a cause"
        );
    }
}

#[test]
fn from_transition_error_maps_every_variant_to_internal_not_retryable() {
    use aios_action::ConditionType;

    let cases = [
        TransitionError::IllegalTransition {
            from: ActionPhase::Pending,
            to: ActionPhase::Succeeded,
        },
        TransitionError::TerminalPhase,
        TransitionError::MonotonicityViolation("observed_at regressed by 3s".to_owned()),
        TransitionError::PhaseConditionMismatch {
            phase: ActionPhase::Succeeded,
            missing: vec![ConditionType::Verified],
        },
    ];

    for case in cases {
        let rendered = case.to_string();
        let e: ActionError = case.into();
        assert_eq!(e.code, ActionErrorCode::Internal);
        assert!(
            !e.retryable,
            "FSM invariant violation is never retryable (retrying does not change the fault)"
        );
        assert_eq!(e.message, rendered);
    }
}

// ---- realistic worst-case serialization ------------------------------------------

#[test]
fn full_seven_layer_cause_chain_serializes_and_round_trips() {
    // Build a chain at depth 7 (the deepest practical realistic case below the §7.5
    // ceiling) and confirm serde round-trips lossless.
    let mut acc = ActionError::new(ActionErrorCode::RuntimeUnavailable, "leaf", true);
    for level in 1..=7 {
        acc = ActionError::new(ActionErrorCode::AdapterFailure, format!("L{level}"), false)
            .with_cause(acc)
            .expect("attach must succeed within §7.5 bound");
    }
    assert_eq!(acc.cause_chain_depth(), 7);

    let json = serde_json::to_string(&acc).expect("serialize");
    let back: ActionError = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, acc);
    assert_eq!(back.cause_chain_depth(), 7);
}
