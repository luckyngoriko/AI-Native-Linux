//! Integration tests for the AIOS-FS query parser.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_fs::{parse_query, Query, QueryField, QueryOperator, QueryParseError, QueryValue};

fn first_predicate(source: &str) -> aios_fs::Predicate {
    let Query::And(predicates) = parse_query(source).expect("query parses");
    predicates.into_iter().next().expect("predicate present")
}

#[test]
fn comparison_operators_parse() {
    for (token, expected) in [
        ("=", QueryOperator::Eq),
        ("!=", QueryOperator::Neq),
        ("<", QueryOperator::Lt),
        ("<=", QueryOperator::Lte),
        (">", QueryOperator::Gt),
        (">=", QueryOperator::Gte),
    ] {
        let predicate = first_predicate(&format!("object.metadata.name {token} \"renderer\""));
        assert_eq!(predicate.field, QueryField::ObjectMetadataName);
        assert_eq!(predicate.op, expected);
        assert_eq!(predicate.rhs, QueryValue::String("renderer".to_owned()));
    }
}

#[test]
fn keyword_operators_parse() {
    let in_predicate = first_predicate("object.kind in [\"PROJECT\", \"WORKSPACE\"]");
    assert_eq!(in_predicate.op, QueryOperator::In);
    assert_eq!(
        in_predicate.rhs,
        QueryValue::StringList(vec!["PROJECT".to_owned(), "WORKSPACE".to_owned()])
    );

    let contains_predicate = first_predicate("object.policy_tags contains \"sdf\"");
    assert_eq!(contains_predicate.op, QueryOperator::Contains);
    assert_eq!(contains_predicate.rhs, QueryValue::String("sdf".to_owned()));

    let matches_predicate = first_predicate("object.metadata.name matches \"render*\"");
    assert_eq!(matches_predicate.op, QueryOperator::Matches);
    assert_eq!(
        matches_predicate.rhs,
        QueryValue::String("render*".to_owned())
    );
}

#[test]
fn value_types_parse() {
    assert_eq!(
        first_predicate("object.metadata.name = \"renderer\"").rhs,
        QueryValue::String("renderer".to_owned())
    );
    assert_eq!(
        first_predicate("object.metadata.name = 42").rhs,
        QueryValue::Int(42)
    );
    assert_eq!(
        first_predicate("object.metadata.name = true").rhs,
        QueryValue::Bool(true)
    );
    assert_eq!(
        first_predicate("object.kind in [PROJECT, WORKSPACE]").rhs,
        QueryValue::StringList(vec!["PROJECT".to_owned(), "WORKSPACE".to_owned()])
    );
    assert_eq!(
        first_predicate(
            "version.created_at in [\"2026-01-01T00:00:00Z\", \
             \"2026-01-02T00:00:00Z\"]"
        )
        .rhs,
        QueryValue::TimeRange {
            start: "2026-01-01T00:00:00Z".to_owned(),
            end: "2026-01-02T00:00:00Z".to_owned(),
        }
    );
}

#[test]
fn and_combines_predicates() {
    let Query::And(predicates) = parse_query(
        "object.kind = PROJECT and object.policy_tags contains \"sdf\" and pointer.kind = CURRENT",
    )
    .expect("query parses");

    assert_eq!(predicates.len(), 3);
    assert_eq!(predicates[0].field, QueryField::ObjectKind);
    assert_eq!(predicates[1].field, QueryField::ObjectPolicyTags);
    assert_eq!(predicates[2].field, QueryField::PointerKind);
}

#[test]
fn reject_or_not_and_parentheses_with_disallowed_token() {
    for source in [
        "object.kind = PROJECT or object.kind = FILE",
        "not object.kind = PROJECT",
        "(object.kind = PROJECT)",
    ] {
        let err = parse_query(source).expect_err("token must be disallowed");
        assert!(matches!(err, QueryParseError::DisallowedToken { .. }));
    }
}

#[test]
fn reject_unknown_field() {
    let err = parse_query("object.secret_value = \"token\"").expect_err("unknown field");
    assert!(matches!(err, QueryParseError::UnknownField { .. }));
}

#[test]
fn reject_unknown_namespace() {
    let err = parse_query("subject.is_ai = true").expect_err("unknown namespace");
    assert!(matches!(err, QueryParseError::UnknownNamespace { .. }));
}

#[test]
fn reject_unknown_operator() {
    let err = parse_query("object.kind like PROJECT").expect_err("unknown operator");
    assert!(matches!(err, QueryParseError::UnknownOperator { .. }));
}

#[test]
fn reports_position_accurate_error_column() {
    let err =
        parse_query("object.kind = PROJECT\n  or object.kind = FILE").expect_err("or rejected");
    match err {
        QueryParseError::DisallowedToken {
            line,
            column,
            token,
        } => {
            assert_eq!(line, 2);
            assert_eq!(column, 3);
            assert_eq!(token, "or");
        }
        other => panic!("expected DisallowedToken, got {other:?}"),
    }
}

#[test]
fn empty_source_returns_error() {
    let err = parse_query("").expect_err("empty source rejected");
    assert!(matches!(
        err,
        QueryParseError::UnexpectedEof { .. } | QueryParseError::UnexpectedToken { .. }
    ));
}

#[test]
fn whitespace_only_source_returns_error() {
    let err = parse_query(" \n\t ").expect_err("whitespace source rejected");
    assert!(matches!(
        err,
        QueryParseError::UnexpectedEof { .. } | QueryParseError::UnexpectedToken { .. }
    ));
}
