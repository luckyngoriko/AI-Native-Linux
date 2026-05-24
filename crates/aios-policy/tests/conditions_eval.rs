//! T-019 integration tests for the §9 conditions evaluator.
//!
//! Each test pins one slice of the evaluator semantics — operator behaviour against
//! canned contexts, AND short-circuit, type mismatches surfacing as typed errors
//! (never panics), and the parse-then-evaluate round trip.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_policy::{
    evaluate_condition, parse_condition, ClockSnapshot, ClosedField, CompareOp, Condition,
    ConditionEvalError, EnrichmentSnapshot, EvalContext, HydratedSubject, Predicate, SubjectType,
    Value,
};

fn subj_human(recovery_mode: bool) -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: "human:lucky".to_owned(),
        subject_type: SubjectType::Human,
        groups: vec!["operators".to_owned(), "users".to_owned()],
        capabilities: vec!["service.restart".to_owned(), "service.reload".to_owned()],
        session_class: "INTERNAL".to_owned(),
        recovery_mode,
        is_ai: false,
    }
}

fn subj_ai() -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: "agent:planner".to_owned(),
        subject_type: SubjectType::Agent,
        groups: vec!["agents".to_owned()],
        capabilities: vec!["read.only".to_owned()],
        session_class: "PUBLIC".to_owned(),
        recovery_mode: false,
        is_ai: true,
    }
}

fn envelope(action: &str, target: serde_json::Value) -> ActionEnvelope {
    ActionEnvelope::new(
        Identity::new("human:lucky", false),
        Request::new(action, target),
        Trace::new("00000000000000000000000000000001", "0000000000000001", None),
    )
}

fn enrich() -> EnrichmentSnapshot {
    EnrichmentSnapshot {
        snapshot_id: "snap_eval".to_owned(),
    }
}

const fn ctx<'a>(
    subj: &'a HydratedSubject,
    env: &'a ActionEnvelope,
    enr: &'a EnrichmentSnapshot,
    now: ClockSnapshot,
) -> EvalContext<'a> {
    EvalContext {
        subject: subj,
        envelope: env,
        enrichment: enr,
        now,
    }
}

#[test]
fn eval_eq_operator_on_subject_recovery_mode_true_case() {
    let s = subj_human(true);
    let e = envelope("service.restart", serde_json::json!({}));
    let n = enrich();
    let c = ctx(&s, &e, &n, ClockSnapshot::default());

    let cond = Condition::conjunction(vec![Predicate::Compare {
        field: ClosedField::SubjectRecoveryMode,
        op: CompareOp::Eq,
        rhs: Value::Bool(true),
    }]);
    assert!(evaluate_condition(&cond, &c).expect("eval ok"));
}

#[test]
fn eval_neq_operator_on_subject_recovery_mode() {
    let s = subj_human(false);
    let e = envelope("service.restart", serde_json::json!({}));
    let n = enrich();
    let c = ctx(&s, &e, &n, ClockSnapshot::default());

    let cond = Condition::conjunction(vec![Predicate::Compare {
        field: ClosedField::SubjectRecoveryMode,
        op: CompareOp::Neq,
        rhs: Value::Bool(true),
    }]);
    assert!(evaluate_condition(&cond, &c).expect("eval ok"));
}

#[test]
fn eval_ord_operators_on_int_time_hour_utc() {
    let s = subj_human(false);
    let e = envelope("service.restart", serde_json::json!({}));
    let n = enrich();
    let clock = ClockSnapshot {
        recovery_mode: false,
        weekday: 2,
        hour_utc: 10,
    };
    let c = ctx(&s, &e, &n, clock);

    for (op, rhs, expected) in [
        (CompareOp::Lt, 11_i64, true),
        (CompareOp::Lt, 10_i64, false),
        (CompareOp::Lte, 10_i64, true),
        (CompareOp::Gt, 9_i64, true),
        (CompareOp::Gt, 10_i64, false),
        (CompareOp::Gte, 10_i64, true),
    ] {
        let cond = Condition::conjunction(vec![Predicate::Compare {
            field: ClosedField::TimeHourUtc,
            op,
            rhs: Value::Int(rhs),
        }]);
        let result = evaluate_condition(&cond, &c).expect("eval ok");
        assert_eq!(
            result,
            expected,
            "operator {} against int {rhs} (hour_utc=10) yielded wrong result",
            op.as_str()
        );
    }
}

#[test]
fn eval_in_operator_against_string_list_field_membership() {
    let s = subj_human(false);
    let e = envelope("service.restart", serde_json::json!({}));
    let n = enrich();
    let c = ctx(&s, &e, &n, ClockSnapshot::default());

    let cond_hit = Condition::conjunction(vec![Predicate::In {
        field: ClosedField::SubjectGroups,
        values: vec![Value::String("operators".to_owned())],
    }]);
    assert!(evaluate_condition(&cond_hit, &c).expect("eval ok"));

    let cond_miss = Condition::conjunction(vec![Predicate::In {
        field: ClosedField::SubjectGroups,
        values: vec![Value::String("admins".to_owned())],
    }]);
    assert!(!evaluate_condition(&cond_miss, &c).expect("eval ok"));
}

#[test]
fn eval_contains_operator_on_string_list_field() {
    let s = subj_human(false);
    let e = envelope("service.restart", serde_json::json!({}));
    let n = enrich();
    let c = ctx(&s, &e, &n, ClockSnapshot::default());

    let cond = Condition::conjunction(vec![Predicate::Contains {
        field: ClosedField::SubjectCapabilities,
        needle: "service.restart".to_owned(),
    }]);
    assert!(evaluate_condition(&cond, &c).expect("eval ok"));

    let cond_miss = Condition::conjunction(vec![Predicate::Contains {
        field: ClosedField::SubjectCapabilities,
        needle: "package.install".to_owned(),
    }]);
    assert!(!evaluate_condition(&cond_miss, &c).expect("eval ok"));
}

#[test]
fn eval_contains_on_string_field_is_substring_match() {
    let s = subj_human(false);
    let e = envelope("service.restart", serde_json::json!({}));
    let n = enrich();
    let c = ctx(&s, &e, &n, ClockSnapshot::default());

    let cond = Condition::conjunction(vec![Predicate::Contains {
        field: ClosedField::RequestAction,
        needle: "restart".to_owned(),
    }]);
    assert!(evaluate_condition(&cond, &c).expect("eval ok"));
}

#[test]
fn eval_exists_predicate_on_present_and_absent_fields() {
    let s = subj_human(false);
    let e = envelope("service.restart", serde_json::json!({}));
    let n = enrich();
    let c = ctx(&s, &e, &n, ClockSnapshot::default());

    let cond_present = Condition::conjunction(vec![Predicate::Exists {
        field: ClosedField::SubjectCanonicalSubjectId,
    }]);
    assert!(evaluate_condition(&cond_present, &c).expect("eval ok"));

    let cond_absent = Condition::conjunction(vec![Predicate::Exists {
        // Wave 6 field not on HydratedSubject yet → Absent in T-019.
        field: ClosedField::SubjectAiExternalPosture,
    }]);
    assert!(!evaluate_condition(&cond_absent, &c).expect("eval ok"));
}

#[test]
fn eval_and_short_circuits_on_first_false() {
    let s = subj_human(false);
    let e = envelope("service.restart", serde_json::json!({}));
    let n = enrich();
    let c = ctx(&s, &e, &n, ClockSnapshot::default());

    // Second conjunct would raise a type-mismatch error if evaluated; the first
    // conjunct is false, so short-circuit must skip it and return Ok(false).
    let cond = Condition::conjunction(vec![
        Predicate::Compare {
            field: ClosedField::SubjectRecoveryMode,
            op: CompareOp::Eq,
            rhs: Value::Bool(true), // false in the context → conjunct 0 is false
        },
        Predicate::Compare {
            field: ClosedField::SubjectRecoveryMode,
            op: CompareOp::Lt, // <-- would raise UnsupportedOperator if reached
            rhs: Value::Bool(true),
        },
    ]);
    let result = evaluate_condition(&cond, &c).expect("short-circuit must skip the bad conjunct");
    assert!(
        !result,
        "conjunction yields false on first-false short-circuit"
    );
}

#[test]
fn eval_type_mismatch_returns_error_not_panic() {
    let s = subj_human(false);
    let e = envelope("service.restart", serde_json::json!({}));
    let n = enrich();
    let c = ctx(&s, &e, &n, ClockSnapshot::default());

    // bool field, int RHS → TypeMismatch (or UnsupportedOperator with Eq it would
    // be a type mismatch, since the types do not match at all).
    let cond = Condition::conjunction(vec![Predicate::Compare {
        field: ClosedField::SubjectRecoveryMode,
        op: CompareOp::Eq,
        rhs: Value::Int(42),
    }]);
    let err = evaluate_condition(&cond, &c).expect_err("must surface a typed error");
    assert!(
        matches!(err, ConditionEvalError::TypeMismatch { .. }),
        "expected TypeMismatch, got {err:?}"
    );
}

#[test]
fn eval_heterogeneous_in_list_returns_error() {
    let s = subj_human(false);
    let e = envelope("service.restart", serde_json::json!({}));
    let n = enrich();
    let c = ctx(&s, &e, &n, ClockSnapshot::default());

    let cond = Condition::conjunction(vec![Predicate::In {
        field: ClosedField::SubjectGroups,
        values: vec![Value::String("operators".to_owned()), Value::Int(7)],
    }]);
    let err = evaluate_condition(&cond, &c).expect_err("mixed-type list rejected");
    assert!(matches!(
        err,
        ConditionEvalError::HeterogeneousValueList { .. }
    ));
}

#[test]
fn eval_parse_then_evaluate_round_trip_for_section_11_1_sample() {
    let s = subj_human(false);
    let e = envelope("service.restart", serde_json::json!({ "service": "nginx" }));
    let n = enrich();
    let c = ctx(&s, &e, &n, ClockSnapshot::default());

    let src = r#"request.action = "service.restart" and target.service in ["nginx", "postgresql", "docker"] and subject.recovery_mode = false"#;
    let cond = parse_condition(src).expect("§11.1 sample parses");
    let result = evaluate_condition(&cond, &c).expect("eval ok");
    assert!(
        result,
        "the sample rule's conditions hold against a matching context"
    );
}

#[test]
fn eval_target_adapter_declared_resolves_from_request_target_json() {
    let s = subj_human(false);
    let e = envelope("service.restart", serde_json::json!({ "service": "nginx" }));
    let n = enrich();
    let c = ctx(&s, &e, &n, ClockSnapshot::default());

    let cond = Condition::conjunction(vec![Predicate::Compare {
        field: ClosedField::TargetAdapterDeclared("service".to_owned()),
        op: CompareOp::Eq,
        rhs: Value::String("nginx".to_owned()),
    }]);
    assert!(evaluate_condition(&cond, &c).expect("eval ok"));

    let cond_miss = Condition::conjunction(vec![Predicate::Compare {
        field: ClosedField::TargetAdapterDeclared("service".to_owned()),
        op: CompareOp::Eq,
        rhs: Value::String("postgresql".to_owned()),
    }]);
    assert!(!evaluate_condition(&cond_miss, &c).expect("eval ok"));
}

#[test]
fn eval_subject_is_ai_distinguishes_agent_from_human() {
    let human = subj_human(false);
    let ai = subj_ai();
    let e = envelope("read.metadata", serde_json::json!({}));
    let n = enrich();

    let cond_human_check = Condition::conjunction(vec![Predicate::Compare {
        field: ClosedField::SubjectIsAi,
        op: CompareOp::Eq,
        rhs: Value::Bool(false),
    }]);

    assert!(evaluate_condition(
        &cond_human_check,
        &ctx(&human, &e, &n, ClockSnapshot::default())
    )
    .expect("eval ok"));
    assert!(!evaluate_condition(
        &cond_human_check,
        &ctx(&ai, &e, &n, ClockSnapshot::default())
    )
    .expect("eval ok"));
}

#[test]
fn eval_absent_field_is_false_for_compare_and_in_but_does_not_error() {
    // T-021 will fill in subject.primary_group_id; today it surfaces as Absent.
    // The evaluator must return Ok(false) for `=`, `!=`, `in`, `contains` on an
    // Absent field — never Err. This keeps bundle rules robust against partially
    // hydrated subjects without raising a noisy error.
    let s = subj_human(false);
    let e = envelope("service.restart", serde_json::json!({}));
    let n = enrich();
    let c = ctx(&s, &e, &n, ClockSnapshot::default());

    let cond_eq = Condition::conjunction(vec![Predicate::Compare {
        field: ClosedField::SubjectPrimaryGroupId,
        op: CompareOp::Eq,
        rhs: Value::String("ops".to_owned()),
    }]);
    assert!(!evaluate_condition(&cond_eq, &c).expect("eval ok"));

    let cond_in = Condition::conjunction(vec![Predicate::In {
        field: ClosedField::SubjectPrimaryGroupId,
        values: vec![Value::String("ops".to_owned())],
    }]);
    assert!(!evaluate_condition(&cond_in, &c).expect("eval ok"));

    let cond_contains = Condition::conjunction(vec![Predicate::Contains {
        field: ClosedField::SubjectPrimaryGroupId,
        needle: "ops".to_owned(),
    }]);
    assert!(!evaluate_condition(&cond_contains, &c).expect("eval ok"));
}

#[test]
fn eval_full_conjunction_all_true_returns_true() {
    let s = subj_human(false);
    let e = envelope("service.restart", serde_json::json!({ "service": "nginx" }));
    let n = enrich();
    let clock = ClockSnapshot {
        recovery_mode: false,
        weekday: 3,
        hour_utc: 12,
    };
    let c = ctx(&s, &e, &n, clock);

    let cond = Condition::conjunction(vec![
        Predicate::Compare {
            field: ClosedField::SubjectIsAi,
            op: CompareOp::Eq,
            rhs: Value::Bool(false),
        },
        Predicate::Compare {
            field: ClosedField::SubjectRecoveryMode,
            op: CompareOp::Eq,
            rhs: Value::Bool(false),
        },
        Predicate::Compare {
            field: ClosedField::RequestAction,
            op: CompareOp::Eq,
            rhs: Value::String("service.restart".to_owned()),
        },
        Predicate::In {
            field: ClosedField::SubjectGroups,
            values: vec![
                Value::String("operators".to_owned()),
                Value::String("admins".to_owned()),
            ],
        },
        Predicate::Compare {
            field: ClosedField::TimeHourUtc,
            op: CompareOp::Gte,
            rhs: Value::Int(8),
        },
    ]);
    assert!(evaluate_condition(&cond, &c).expect("eval ok"));
}
