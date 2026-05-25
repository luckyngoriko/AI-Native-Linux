//! Tests for text renderer helpers.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_renderer_cli::{RenderContext, TextRenderer};

fn ctx(color: bool, locale: &str) -> RenderContext {
    RenderContext {
        color,
        width: Some(80),
        redact_secrets: true,
        verbose: false,
        locale: locale.to_owned(),
    }
}

#[test]
fn render_kv_uses_text_key_value_format() {
    let renderer = TextRenderer::new(ctx(false, "C"));

    assert_eq!(
        renderer.render_kv("subject", "operator"),
        "subject: operator"
    );
}

#[test]
fn render_section_with_empty_body_keeps_title_line() {
    let renderer = TextRenderer::new(ctx(false, "C"));

    assert_eq!(renderer.render_section("Summary", &[]), "Summary\n");
}

#[test]
fn render_section_joins_body_lines_below_title() {
    let renderer = TextRenderer::new(ctx(false, "C"));
    let lines = vec!["alpha".to_owned(), "beta".to_owned()];

    assert_eq!(
        renderer.render_section("Summary", &lines),
        "Summary\nalpha\nbeta"
    );
}

#[test]
fn render_list_with_no_items_is_empty() {
    let renderer = TextRenderer::new(ctx(false, "C"));

    assert_eq!(renderer.render_list(&[]), "");
}

#[test]
fn render_list_with_one_utf8_item_uses_spec_bullet() {
    let renderer = TextRenderer::new(ctx(true, "en_US.UTF-8"));
    let items = vec!["first".to_owned()];

    assert_eq!(renderer.render_list(&items), "• first");
}

#[test]
fn render_list_with_many_ascii_items_uses_fallback_bullets() {
    let renderer = TextRenderer::new(ctx(false, "C"));
    let items = vec!["first".to_owned(), "second".to_owned()];

    assert_eq!(renderer.render_list(&items), "* first\n* second");
}

#[test]
fn color_for_state_returns_plain_text_when_color_is_disabled() {
    let renderer = TextRenderer::new(ctx(false, "en_US.UTF-8"));

    assert_eq!(renderer.color_for_state("succeeded"), "succeeded");
}

#[test]
fn color_for_state_wraps_known_state_when_color_is_enabled() {
    let renderer = TextRenderer::new(ctx(true, "en_US.UTF-8"));

    assert_eq!(
        renderer.color_for_state("succeeded"),
        "\u{1b}[32msucceeded\u{1b}[0m"
    );
}
