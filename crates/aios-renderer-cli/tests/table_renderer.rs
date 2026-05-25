//! Tests for fixed-width table rendering.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_renderer_cli::{RenderContext, RenderError, TableAlign, TableRenderer, TableSpec};

fn ctx(color: bool, width: Option<u16>, locale: &str) -> RenderContext {
    RenderContext {
        color,
        width,
        redact_secrets: true,
        verbose: false,
        locale: locale.to_owned(),
    }
}

#[test]
fn single_row_table_uses_utf8_box_drawing() {
    let renderer = TableRenderer::new(ctx(true, Some(80), "en_US.UTF-8"));
    let spec = TableSpec {
        headers: vec!["Name".to_owned(), "Age".to_owned()],
        rows: vec![vec!["Ada".to_owned(), "36".to_owned()]],
        align: vec![TableAlign::Left, TableAlign::Right],
    };

    let rendered = renderer.render(&spec).expect("render table");

    assert_eq!(
        rendered,
        "┌──────┬─────┐\n\
         │ Name │ Age │\n\
         ├──────┼─────┤\n\
         │ Ada  │  36 │\n\
         └──────┴─────┘"
    );
}

#[test]
fn multi_row_table_uses_varying_column_widths() {
    let renderer = TableRenderer::new(ctx(true, Some(80), "en_US.UTF-8"));
    let spec = TableSpec {
        headers: vec!["Host".to_owned(), "State".to_owned()],
        rows: vec![
            vec!["gw".to_owned(), "up".to_owned()],
            vec!["storage-node".to_owned(), "degraded".to_owned()],
        ],
        align: vec![TableAlign::Left, TableAlign::Left],
    };

    let rendered = renderer.render(&spec).expect("render table");

    assert!(rendered.contains("│ storage-node │ degraded │"));
    assert!(rendered.starts_with("┌──────────────┬──────────┐"));
}

#[test]
fn right_alignment_pads_on_the_left() {
    let renderer = TableRenderer::new(ctx(true, Some(80), "en_US.UTF-8"));
    let spec = TableSpec {
        headers: vec!["count".to_owned()],
        rows: vec![vec!["7".to_owned()]],
        align: vec![TableAlign::Right],
    };

    let rendered = renderer.render(&spec).expect("render table");

    assert!(rendered.contains("│     7 │"));
}

#[test]
fn center_alignment_splits_padding_across_both_sides() {
    let renderer = TableRenderer::new(ctx(true, Some(80), "en_US.UTF-8"));
    let spec = TableSpec {
        headers: vec!["name".to_owned()],
        rows: vec![vec!["io".to_owned()]],
        align: vec![TableAlign::Center],
    };

    let rendered = renderer.render(&spec).expect("render table");

    assert!(rendered.contains("│  io  │"));
}

#[test]
fn width_overflow_reports_needed_and_available_columns() {
    let renderer = TableRenderer::new(ctx(true, Some(8), "en_US.UTF-8"));
    let spec = TableSpec {
        headers: vec!["Name".to_owned(), "Age".to_owned()],
        rows: vec![vec!["Ada".to_owned(), "36".to_owned()]],
        align: vec![TableAlign::Left, TableAlign::Right],
    };

    let err = renderer
        .render(&spec)
        .expect_err("narrow width must overflow");

    assert_eq!(
        err,
        RenderError::WidthOverflow {
            needed: 14,
            available: 8
        }
    );
}

#[test]
fn ascii_fallback_is_used_for_plain_non_utf8_context() {
    let renderer = TableRenderer::new(ctx(false, Some(80), "C"));
    let spec = TableSpec {
        headers: vec!["Name".to_owned()],
        rows: vec![vec!["Ada".to_owned()]],
        align: vec![TableAlign::Left],
    };

    let rendered = renderer.render(&spec).expect("render table");

    assert_eq!(rendered, "+------+\n| Name |\n+------+\n| Ada  |\n+------+");
}
