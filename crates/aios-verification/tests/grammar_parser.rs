//! Parser coverage for the S2.4 verification expression grammar.

use std::error::Error;

use aios_verification::{
    grammar_parser::parse, PrimitiveInvocation, VerificationDuration, VerificationDurationUnit,
    VerificationGrammar, VerificationPrimitive,
};
use serde_json::json;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

#[allow(
    clippy::missing_const_for_fn,
    reason = "test helper accepts serde_json::Value fixtures"
)]
fn primitive(kind: VerificationPrimitive, args: serde_json::Value) -> VerificationGrammar {
    VerificationGrammar::Primitive(PrimitiveInvocation { kind, args })
}

fn parse_error(source: &str) -> String {
    match parse(source) {
        Ok(ast) => format!("unexpected parse success: {ast}"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn parses_simple_primitive_call() -> TestResult {
    let ast = parse(r#"file.exists(object_or_path="/tmp/x")"#)?;

    assert_eq!(
        ast,
        primitive(
            VerificationPrimitive::FileExists,
            json!({"object_or_path": "/tmp/x"})
        )
    );
    Ok(())
}

#[test]
fn parses_proto_snake_case_primitive_alias() -> TestResult {
    let ast = parse(r#"file_exists(object_or_path="/tmp/x")"#)?;

    assert_eq!(
        ast,
        primitive(
            VerificationPrimitive::FileExists,
            json!({"object_or_path": "/tmp/x"})
        )
    );
    Ok(())
}

#[test]
fn parses_all_composition_chain() -> TestResult {
    let ast =
        parse(r#"all[file.exists(object_or_path="/tmp/x"), service.active(service="kwin")]"#)?;

    assert_eq!(
        ast,
        VerificationGrammar::All(vec![
            primitive(
                VerificationPrimitive::FileExists,
                json!({"object_or_path": "/tmp/x"})
            ),
            primitive(
                VerificationPrimitive::ServiceActive,
                json!({"service": "kwin"})
            ),
        ])
    );
    Ok(())
}

#[test]
fn parses_quoted_string_args_with_escape() -> TestResult {
    let ast = parse(r#"http.ok(url="http://127.0.0.1/", expected_body_substring="ready \"ok\"")"#)?;

    assert_eq!(
        ast,
        primitive(
            VerificationPrimitive::HttpOk,
            json!({
                "url": "http://127.0.0.1/",
                "expected_body_substring": "ready \"ok\"",
            })
        )
    );
    Ok(())
}

#[test]
fn parses_integer_args() -> TestResult {
    let ast = parse(r#"port.open(host="127.0.0.1", port=8930, protocol="tcp")"#)?;

    assert_eq!(
        ast,
        primitive(
            VerificationPrimitive::PortOpen,
            json!({"host": "127.0.0.1", "port": 8930, "protocol": "tcp"})
        )
    );
    Ok(())
}

#[test]
fn parses_bool_args() -> TestResult {
    let ast = parse(
        r#"namespace_catalog_version(expected_catalog_id="nscat_abcd", require_exact_match=true)"#,
    )?;

    assert_eq!(
        ast,
        primitive(
            VerificationPrimitive::NamespaceCatalogVersion,
            json!({"expected_catalog_id": "nscat_abcd", "require_exact_match": true})
        )
    );
    Ok(())
}

#[test]
fn parses_any_composition_per_s24_spec() -> TestResult {
    let ast = parse(
        r#"any[file.exists(object_or_path="/tmp/a"), file.exists(object_or_path="/tmp/b")]"#,
    )?;

    assert!(matches!(ast, VerificationGrammar::Any(terms) if terms.len() == 2));
    Ok(())
}

#[test]
fn parses_not_composition_per_s24_spec() -> TestResult {
    let ast = parse(r#"not(file.exists(object_or_path="/tmp/missing"))"#)?;

    assert!(matches!(ast, VerificationGrammar::Not(_)));
    Ok(())
}

#[test]
fn parses_eventually_composition_with_durations() -> TestResult {
    let ast = parse(
        r#"eventually(file.exists(object_or_path="/tmp/x"), max_duration=5s, interval=250ms)"#,
    )?;

    assert_eq!(
        ast,
        VerificationGrammar::Eventually {
            term: Box::new(primitive(
                VerificationPrimitive::FileExists,
                json!({"object_or_path": "/tmp/x"})
            )),
            max_duration: VerificationDuration {
                value: 5,
                unit: VerificationDurationUnit::Seconds,
            },
            interval: VerificationDuration {
                value: 250,
                unit: VerificationDurationUnit::Milliseconds,
            },
        }
    );
    Ok(())
}

#[test]
fn rejects_infix_or_token() {
    let error = parse_error(
        "file.exists(object_or_path=\"/tmp/a\") or file.exists(object_or_path=\"/tmp/b\")",
    );

    assert!(error.contains("disallowed grammar token"));
    assert!(error.contains("or"));
    assert!(error.contains("line 1, column 38"));
}

#[test]
fn rejects_infix_and_token() {
    let error = parse_error(
        "file.exists(object_or_path=\"/tmp/a\") and file.exists(object_or_path=\"/tmp/b\")",
    );

    assert!(error.contains("disallowed grammar token"));
    assert!(error.contains("and"));
}

#[test]
fn rejects_parenthesized_grouping() {
    let error = parse_error(r#"(file.exists(object_or_path="/tmp/x"))"#);

    assert!(error.contains("disallowed grammar token"));
    assert!(error.contains("grouping"));
    assert!(error.contains("line 1, column 1"));
}

#[test]
fn rejects_unknown_primitive_name_with_closed_vocabulary_error() {
    let error = parse_error(r#"process.running(name="kwin")"#);

    assert!(error.contains("unknown verification primitive"));
    assert!(error.contains("process.running"));
    assert!(error.contains("closed S2.4 vocabulary"));
}

#[test]
fn rejects_missing_required_arg_with_position_info() {
    let error = parse_error("all[\n  file.exists(),\n  service.active(service=\"kwin\")\n]");

    assert!(error.contains("missing required arg `object_or_path`"));
    assert!(error.contains("line 2, column 3"));
}

#[test]
fn rejects_malformed_quoting() {
    let error = parse_error(r#"file.exists(object_or_path="/tmp/x)"#);

    assert!(error.contains("unterminated string literal"));
    assert!(error.contains("line 1, column 28"));
}

#[test]
fn error_positions_include_line_and_column() {
    let error = parse_error("all[\n  file.exists(object_or_path=\"/tmp/x\"),\n  nope()\n]");

    assert!(error.contains("line 3, column 3"));
}

#[test]
fn parses_same_source_deterministically() -> TestResult {
    let source = r#"all[file.exists(object_or_path="/tmp/x"), service.active(service="kwin")]"#;
    let expected = parse(source)?;

    for _ in 0..100 {
        assert_eq!(parse(source)?, expected);
    }
    Ok(())
}

#[test]
fn display_round_trip_is_stable() -> TestResult {
    let ast =
        parse(r#"all[file.exists(object_or_path="/tmp/x"), service.active(service="kwin")]"#)?;
    let rendered = ast.to_string();

    assert_eq!(parse(&rendered)?, ast);
    Ok(())
}

#[test]
fn rejects_all_with_fewer_than_two_terms() {
    let error = parse_error(r#"all[file.exists(object_or_path="/tmp/x")]"#);

    assert!(error.contains("all requires at least 2 terms"));
}
