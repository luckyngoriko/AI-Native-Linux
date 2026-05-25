//! Tests for tree rendering.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_renderer_cli::{RenderContext, TreeNode, TreeRenderer};

fn ctx(color: bool, locale: &str) -> RenderContext {
    RenderContext {
        color,
        width: Some(80),
        redact_secrets: true,
        verbose: false,
        locale: locale.to_owned(),
    }
}

fn node(label: &str, children: Vec<TreeNode>) -> TreeNode {
    TreeNode {
        label: label.to_owned(),
        children,
    }
}

#[test]
fn single_root_renders_label_only() {
    let renderer = TreeRenderer::new(ctx(true, "en_US.UTF-8"));
    let root = node("root", vec![]);

    assert_eq!(renderer.render(&root).expect("render tree"), "root");
}

#[test]
fn root_with_two_children_uses_branch_and_last_markers() {
    let renderer = TreeRenderer::new(ctx(true, "en_US.UTF-8"));
    let root = node("root", vec![node("alpha", vec![]), node("beta", vec![])]);

    assert_eq!(
        renderer.render(&root).expect("render tree"),
        "root\n├── alpha\n└── beta"
    );
}

#[test]
fn nested_three_deep_tree_renders_clean_prefixes() {
    let renderer = TreeRenderer::new(ctx(true, "en_US.UTF-8"));
    let root = node("root", vec![node("branch", vec![node("leaf", vec![])])]);

    assert_eq!(
        renderer.render(&root).expect("render tree"),
        "root\n└── branch\n    └── leaf"
    );
}

#[test]
fn mixed_nested_tree_keeps_vertical_continuation() {
    let renderer = TreeRenderer::new(ctx(true, "en_US.UTF-8"));
    let root = node(
        "root",
        vec![
            node("branch", vec![node("leaf", vec![])]),
            node("sibling", vec![]),
        ],
    );

    assert_eq!(
        renderer.render(&root).expect("render tree"),
        "root\n├── branch\n│   └── leaf\n└── sibling"
    );
}

#[test]
fn empty_children_render_without_extra_blank_lines() {
    let renderer = TreeRenderer::new(ctx(true, "en_US.UTF-8"));
    let root = node("root", vec![node("leaf", Vec::new())]);

    assert_eq!(
        renderer.render(&root).expect("render tree"),
        "root\n└── leaf"
    );
}

#[test]
fn ascii_fallback_is_used_for_plain_non_utf8_context() {
    let renderer = TreeRenderer::new(ctx(false, "C"));
    let root = node("root", vec![node("leaf", vec![])]);

    assert_eq!(
        renderer.render(&root).expect("render tree"),
        "root\n`-- leaf"
    );
}
