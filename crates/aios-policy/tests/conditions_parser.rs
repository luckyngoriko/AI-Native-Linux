//! T-019 integration tests for the §9.1 conditions parser.
//!
//! Each test pins one slice of the EBNF — the operator coverage tests pin one
//! operator each, the value-type tests pin one literal type each, and the
//! error-path tests pin one rejection mode each. The determinism test is the
//! constitutional contract for §13.1.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use aios_policy::{
    parse_condition, ClosedField, CompareOp, Condition, ConditionParseError, Namespace, Predicate,
    Value,
};

fn single(c: &Condition) -> &Predicate {
    assert_eq!(
        c.predicates.len(),
        1,
        "expected exactly one predicate, got {}",
        c.predicates.len()
    );
    &c.predicates[0]
}

#[test]
fn parse_eq_operator_with_string_literal() {
    let c = parse_condition(r#"request.action = "service.restart""#).expect("parses");
    match single(&c) {
        Predicate::Compare { field, op, rhs } => {
            assert_eq!(*field, ClosedField::RequestAction);
            assert_eq!(*op, CompareOp::Eq);
            assert_eq!(*rhs, Value::String("service.restart".to_owned()));
        }
        other => panic!("expected Compare, got {other:?}"),
    }
}

#[test]
fn parse_neq_operator_with_identifier_literal() {
    let c = parse_condition("subject.subject_type != human").expect("parses");
    match single(&c) {
        Predicate::Compare { op, rhs, .. } => {
            assert_eq!(*op, CompareOp::Neq);
            assert_eq!(*rhs, Value::Identifier("human".to_owned()));
        }
        other => panic!("expected Compare, got {other:?}"),
    }
}

#[test]
fn parse_ord_operators_with_int_literal() {
    for (src, op) in [
        ("time.hour_utc < 9", CompareOp::Lt),
        ("time.hour_utc <= 9", CompareOp::Lte),
        ("time.hour_utc > 9", CompareOp::Gt),
        ("time.hour_utc >= 9", CompareOp::Gte),
    ] {
        let c = parse_condition(src).expect("parses");
        match single(&c) {
            Predicate::Compare {
                op: actual, rhs, ..
            } => {
                assert_eq!(*actual, op, "operator mismatch for {src}");
                assert_eq!(*rhs, Value::Int(9));
            }
            other => panic!("expected Compare for {src}, got {other:?}"),
        }
    }
}

#[test]
fn parse_in_with_string_value_list() {
    let c =
        parse_condition(r#"target.service in ["nginx", "postgresql", "docker"]"#).expect("parses");
    match single(&c) {
        Predicate::In { field, values } => {
            assert_eq!(
                *field,
                ClosedField::TargetAdapterDeclared("service".to_owned())
            );
            assert_eq!(values.len(), 3);
            assert_eq!(values[0], Value::String("nginx".to_owned()));
            assert_eq!(values[2], Value::String("docker".to_owned()));
        }
        other => panic!("expected In, got {other:?}"),
    }
}

#[test]
fn parse_contains_with_string_literal() {
    let c = parse_condition(r#"subject.capabilities contains "service.restart""#).expect("parses");
    match single(&c) {
        Predicate::Contains { field, needle } => {
            assert_eq!(*field, ClosedField::SubjectCapabilities);
            assert_eq!(needle, "service.restart");
        }
        other => panic!("expected Contains, got {other:?}"),
    }
}

#[test]
fn parse_exists_predicate() {
    let c = parse_condition("object.privacy_class exists").expect("parses");
    match single(&c) {
        Predicate::Exists { field } => {
            assert_eq!(*field, ClosedField::ObjectPrivacyClass);
        }
        other => panic!("expected Exists, got {other:?}"),
    }
}

#[test]
fn parse_bool_value() {
    let c = parse_condition("subject.recovery_mode = true").expect("parses");
    match single(&c) {
        Predicate::Compare { rhs, .. } => assert_eq!(*rhs, Value::Bool(true)),
        other => panic!("expected Compare, got {other:?}"),
    }
}

#[test]
fn parse_negative_int_value() {
    let c = parse_condition("time.hour_utc > -1").expect("parses");
    match single(&c) {
        Predicate::Compare { rhs, .. } => assert_eq!(*rhs, Value::Int(-1)),
        other => panic!("expected Compare, got {other:?}"),
    }
}

#[test]
fn parse_and_conjunction_of_multiple_predicates() {
    let src = r#"subject.recovery_mode = false and request.action = "service.restart" and target.service in ["nginx"]"#;
    let c = parse_condition(src).expect("parses");
    assert_eq!(c.predicates.len(), 3, "three conjuncts expected");
    assert!(matches!(
        c.predicates[0],
        Predicate::Compare {
            field: ClosedField::SubjectRecoveryMode,
            ..
        }
    ));
    assert!(matches!(c.predicates[2], Predicate::In { .. }));
}

#[test]
fn parse_bare_time_recovery_mode_boolean_sugar() {
    // The §9.1 last alternative: `"time" "." "recovery_mode"` as a bare predicate
    // is sugar for `time.recovery_mode = true`.
    let c = parse_condition("time.recovery_mode").expect("parses");
    match single(&c) {
        Predicate::Compare { field, op, rhs } => {
            assert_eq!(*field, ClosedField::TimeRecoveryMode);
            assert_eq!(*op, CompareOp::Eq);
            assert_eq!(*rhs, Value::Bool(true));
        }
        other => panic!("expected Compare from bare sugar, got {other:?}"),
    }
}

#[test]
fn parse_rejects_unknown_namespace() {
    let err = parse_condition(r#"env.region = "eu""#).expect_err("unknown namespace");
    match err {
        ConditionParseError::UnknownNamespace {
            line,
            column,
            namespace,
        } => {
            assert_eq!(line, 1);
            assert_eq!(column, 1, "namespace position is start of source");
            assert_eq!(namespace, "env");
        }
        other => panic!("expected UnknownNamespace, got {other:?}"),
    }
}

#[test]
fn parse_rejects_unknown_field_in_closed_namespace() {
    let err = parse_condition(r#"subject.spoofed_field = "x""#).expect_err("unknown closed field");
    match err {
        ConditionParseError::UnknownField {
            field,
            line,
            column,
        } => {
            assert_eq!(line, 1);
            assert_eq!(column, 1);
            assert_eq!(field, "subject.spoofed_field");
        }
        other => panic!("expected UnknownField, got {other:?}"),
    }
}

#[test]
fn parse_admits_unknown_target_subpath_as_adapter_declared() {
    // The `target` namespace is the §9.2 escape hatch — adapter manifests own
    // their sub-vocabulary, so the parser MUST admit unknown target sub-paths.
    let c = parse_condition(r#"target.url = "https://example/""#).expect("adapter-declared OK");
    match single(&c) {
        Predicate::Compare { field, .. } => {
            assert_eq!(*field, ClosedField::TargetAdapterDeclared("url".to_owned()));
            assert_eq!(field.namespace(), Namespace::Target);
        }
        other => panic!("expected Compare, got {other:?}"),
    }
}

#[test]
fn parse_rejects_unknown_operator() {
    let err = parse_condition("subject.recovery_mode ~~ false").expect_err("bad operator");
    match err {
        ConditionParseError::UnknownOperator { operator, .. } => {
            assert!(
                operator.starts_with("~~"),
                "operator excerpt should start with the offender, got `{operator}`"
            );
        }
        other => panic!("expected UnknownOperator, got {other:?}"),
    }
}

#[test]
fn parse_rejects_disallowed_or_keyword() {
    let err = parse_condition("subject.recovery_mode = true or subject.is_ai = false")
        .expect_err("or is disallowed");
    match err {
        ConditionParseError::DisallowedToken { token, line, .. } => {
            assert_eq!(token, "or");
            assert_eq!(line, 1);
        }
        other => panic!("expected DisallowedToken, got {other:?}"),
    }
}

#[test]
fn parse_rejects_disallowed_parentheses() {
    let err = parse_condition("(subject.recovery_mode = true)").expect_err("parens disallowed");
    assert!(matches!(err, ConditionParseError::DisallowedToken { .. }));
}

#[test]
fn parse_rejects_disallowed_not_keyword() {
    let err = parse_condition("subject.recovery_mode = true and not subject.is_ai = true")
        .expect_err("not is disallowed");
    match err {
        ConditionParseError::DisallowedToken { token, .. } => {
            assert_eq!(token, "not");
        }
        other => panic!("expected DisallowedToken, got {other:?}"),
    }
}

#[test]
fn parse_rejects_empty_in_value_list() {
    let err = parse_condition("target.service in []").expect_err("empty IN rejected");
    assert!(matches!(err, ConditionParseError::EmptyValueList { .. }));
}

#[test]
fn parse_rejects_unterminated_string() {
    let err =
        parse_condition(r#"request.action = "service.restart"#).expect_err("unterminated string");
    assert!(matches!(
        err,
        ConditionParseError::UnterminatedString { .. }
    ));
}

#[test]
fn parse_rejects_fractional_number() {
    let err = parse_condition("time.hour_utc = 9.5").expect_err("fractional number rejected");
    match err {
        ConditionParseError::InvalidInteger { literal, .. } => {
            assert_eq!(literal, "9.5");
        }
        other => panic!("expected InvalidInteger, got {other:?}"),
    }
}

#[test]
fn parse_error_position_is_accurate_on_second_line() {
    // Source split across lines — the unknown namespace is on line 2.
    let src = "subject.recovery_mode = true and\nenv.x = 1";
    let err = parse_condition(src).expect_err("unknown namespace on line 2");
    match err {
        ConditionParseError::UnknownNamespace { line, column, .. } => {
            assert_eq!(line, 2, "error must report line 2");
            assert_eq!(column, 1, "error must report column 1");
        }
        other => panic!("expected UnknownNamespace, got {other:?}"),
    }
}

#[test]
fn parse_dotted_subpath_resolves_to_typed_field() {
    // `request.risk.destructive` — multi-segment subpath under request namespace.
    let c = parse_condition("request.risk.destructive = true").expect("parses");
    match single(&c) {
        Predicate::Compare { field, .. } => {
            assert_eq!(*field, ClosedField::RequestRiskDestructive);
        }
        other => panic!("expected Compare, got {other:?}"),
    }
}

#[test]
fn parse_yaml_rule_snippet_from_section_11_1() {
    // Direct quote of the §11.1 sample rule's `conditions:` block — every line
    // must parse cleanly when joined into one condition.
    let composed = concat!(
        r#"request.environment = "LOCAL" and "#,
        r#"target.service in ["nginx", "postgresql", "docker"] and "#,
        r#"object.privacy_class <= "INTERNAL" and "#,
        "subject.recovery_mode = false"
    );
    let c = parse_condition(composed).expect("§11.1 sample parses");
    assert_eq!(c.predicates.len(), 4);
}

#[test]
fn parse_round_trip_is_deterministic_one_hundred_iterations() {
    // §13.1 determinism contract: same source string → same AST every parse.
    let src = r#"subject.recovery_mode = false and request.action = "service.restart" and target.service in ["nginx", "postgresql"] and subject.capabilities contains "service.restart" and time.hour_utc >= 8"#;
    let first = parse_condition(src).expect("parses");
    for i in 0..100 {
        let again = parse_condition(src).expect("re-parses");
        assert_eq!(again, first, "AST drift on iteration {i}");
    }
}

#[test]
fn parse_in_does_not_match_internal_substring() {
    // The lexer's `try_consume` MUST not mistake "in" inside "internal" for the
    // `in` keyword. Construct a value list where the literal `INTERNAL` appears,
    // and a separate comparison against `subject.session_class`.
    let c = parse_condition(r#"subject.session_class = "INTERNAL""#).expect("parses cleanly");
    match single(&c) {
        Predicate::Compare { rhs, .. } => assert_eq!(*rhs, Value::String("INTERNAL".to_owned())),
        other => panic!("expected Compare, got {other:?}"),
    }
}

#[test]
fn parse_handles_value_list_with_trailing_whitespace() {
    let c = parse_condition(r#"target.scope in [ "GROUP" , "USER" ]"#).expect("parses");
    match single(&c) {
        Predicate::In { values, .. } => {
            assert_eq!(values.len(), 2);
            assert_eq!(values[0], Value::String("GROUP".to_owned()));
            assert_eq!(values[1], Value::String("USER".to_owned()));
        }
        other => panic!("expected In, got {other:?}"),
    }
}
