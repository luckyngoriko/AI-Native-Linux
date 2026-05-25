//! T-056 round-trip tests for the `aios-renderer-cli` skeleton.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::collections::BTreeMap;

use strum::{EnumCount, IntoEnumIterator};

use aios_renderer_cli::{
    AnsiSupportLevel, CliCompilationResult, CliEvidenceRecordKind, CliInputMode, CliRenderMode,
    OutputFormat, RenderContext, RenderError, Renderable,
};

#[test]
fn output_format_has_spec_count() {
    assert_eq!(OutputFormat::COUNT, 4);
    assert_eq!(OutputFormat::iter().count(), 4);
}

#[test]
fn s76_closed_enum_counts_match_spec() {
    assert_eq!(CliRenderMode::COUNT, 5);
    assert_eq!(CliCompilationResult::COUNT, 11);
    assert_eq!(CliInputMode::COUNT, 6);
    assert_eq!(AnsiSupportLevel::COUNT, 5);
    assert_eq!(CliEvidenceRecordKind::COUNT, 11);
}

#[test]
fn output_format_round_trips_through_serde_json() {
    for format in OutputFormat::iter() {
        let json = serde_json::to_string(&format).expect("serialise output format");
        let back: OutputFormat = serde_json::from_str(&json).expect("deserialise output format");
        assert_eq!(format, back);
    }
}

#[test]
fn output_format_from_str_is_case_insensitive() {
    assert_eq!(OutputFormat::from_str("json"), Ok(OutputFormat::Json));
    assert_eq!(OutputFormat::from_str("JSON"), Ok(OutputFormat::Json));
    assert_eq!(OutputFormat::from_str("Json"), Ok(OutputFormat::Json));
}

#[test]
fn output_format_from_str_rejects_invalid_values() {
    let err = OutputFormat::from_str("invalid").expect_err("invalid format must reject");
    assert_eq!(err, RenderError::UnknownFormat("invalid".to_owned()));
}

#[test]
fn terminal_defaults_are_sensible() {
    let ctx = RenderContext::new_terminal_defaults();

    assert!(ctx.width.is_none_or(|width| width > 0));
    assert!(ctx.redact_secrets);
    assert!(!ctx.locale.is_empty());
}

#[test]
fn pipe_defaults_are_plain_and_unbounded() {
    let ctx = RenderContext::new_pipe_defaults();

    assert!(!ctx.color);
    assert_eq!(ctx.width, None);
    assert!(ctx.redact_secrets);
}

#[test]
fn redact_secrets_defaults_to_true() {
    assert!(RenderContext::new_terminal_defaults().redact_secrets);
    assert!(RenderContext::new_pipe_defaults().redact_secrets);
}

#[test]
fn str_renderable_text_and_json_are_direct() {
    let ctx = RenderContext::new_pipe_defaults();

    assert_eq!(
        "string"
            .render(OutputFormat::Text, &ctx)
            .expect("render text"),
        "string"
    );
    assert_eq!(
        "string"
            .render(OutputFormat::Json, &ctx)
            .expect("render json"),
        "\"string\""
    );
}

#[test]
fn vec_renderable_includes_all_elements_in_each_format() {
    let ctx = RenderContext::new_pipe_defaults();
    let values = vec![1_u64, 2, 3];

    for format in OutputFormat::iter() {
        let rendered = values.render(format, &ctx).expect("render vec");
        for expected in ["1", "2", "3"] {
            assert!(
                rendered.contains(expected),
                "{format:?} output must include {expected}: {rendered}"
            );
        }
    }
}

#[test]
fn btreemap_text_rendering_is_sorted_by_key() {
    let ctx = RenderContext::new_pipe_defaults();
    let mut values = BTreeMap::new();
    values.insert("b".to_owned(), 2_u64);
    values.insert("a".to_owned(), 1_u64);

    let rendered = values
        .render(OutputFormat::Text, &ctx)
        .expect("render BTreeMap");
    let a_pos = rendered.find("a: 1").expect("a key present");
    let b_pos = rendered.find("b: 2").expect("b key present");

    assert!(
        a_pos < b_pos,
        "BTreeMap text output must be sorted: {rendered}"
    );
}

#[test]
fn render_error_display_strings_are_non_empty() {
    let cases = [
        RenderError::UnknownFormat("xml".to_owned()),
        RenderError::Unsupported {
            type_name: "Widget".to_owned(),
            format: OutputFormat::Tree,
        },
        RenderError::SerializationFailed("bad json".to_owned()),
        RenderError::WidthOverflow {
            needed: 120,
            available: 80,
        },
        RenderError::Internal("invariant violated".to_owned()),
    ];

    for err in cases {
        assert!(!err.to_string().is_empty());
    }
}
