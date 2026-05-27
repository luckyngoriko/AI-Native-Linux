//! Integration tests for `VisualToken` → CSS custom property mapping (T-142).
//!
//! Tests the public API surface: `compile_token_to_css`,
//! `compile_token_to_css_with_ctx`, and `recipe_to_css_block`.

#![allow(clippy::unwrap_used, clippy::panic)]

use aios_renderer_kde::{VisualToken, VisualTokenKind};
use aios_renderer_web::css_compile::{
    compile_token_to_css, compile_token_to_css_with_ctx, recipe_to_css_block, CssEasing, CssRecipe,
    WebIconLookupCtx, WebShapeKind,
};
use aios_renderer_web::WebRendererError;

fn make_token(id: &str, kind: VisualTokenKind, value: &str) -> VisualToken {
    VisualToken {
        id: id.to_string(),
        kind,
        canonical_value: value.to_string(),
    }
}

fn signed_ctx() -> WebIconLookupCtx {
    WebIconLookupCtx {
        theme_id: "aios-dark".into(),
        root_signed: true,
    }
}

fn unsigned_ctx() -> WebIconLookupCtx {
    WebIconLookupCtx {
        theme_id: "aios-dark".into(),
        root_signed: false,
    }
}

// ── Color ──────────────────────────────────────────────────────────────────────

#[test]
fn compile_color_token() {
    let token = make_token("color.window.background", VisualTokenKind::Color, "#1e1e2e");
    let recipe = compile_token_to_css(&token).unwrap();
    assert_eq!(
        recipe,
        CssRecipe::Variable {
            property: "--aios-color-window-background".into(),
            value: "#1e1e2e".into(),
        }
    );
}

#[test]
fn compile_color_without_hash_prefix() {
    let token = make_token("color.foo", VisualTokenKind::Color, "aabbcc");
    let recipe = compile_token_to_css(&token).unwrap();
    assert_eq!(
        recipe,
        CssRecipe::Variable {
            property: "--aios-color-foo".into(),
            value: "#aabbcc".into(),
        }
    );
}

#[test]
fn compile_color_invalid_hex_returns_error() {
    let token = make_token("color.bad", VisualTokenKind::Color, "#xyz123");
    assert!(compile_token_to_css(&token).is_err());
}

// ── Font ───────────────────────────────────────────────────────────────────────

#[test]
fn compile_font_token() {
    let token = make_token(
        "font.body",
        VisualTokenKind::Font,
        "Inter|16|400|false|false",
    );
    let recipe = compile_token_to_css(&token).unwrap();
    assert_eq!(
        recipe,
        CssRecipe::Font {
            family: "Inter".into(),
            size_px: 16,
            weight: 400,
            italic: false,
            mono: false,
        }
    );
}

#[test]
fn compile_font_italic_mono() {
    let token = make_token(
        "font.code",
        VisualTokenKind::Font,
        "JetBrains Mono|14|700|true|true",
    );
    let recipe = compile_token_to_css(&token).unwrap();
    assert_eq!(
        recipe,
        CssRecipe::Font {
            family: "JetBrains Mono".into(),
            size_px: 14,
            weight: 700,
            italic: true,
            mono: true,
        }
    );
}

#[test]
fn compile_font_invalid_returns_error() {
    let token = make_token(
        "font.bad",
        VisualTokenKind::Font,
        "Inter|notanumber|400|false|false",
    );
    assert!(compile_token_to_css(&token).is_err());
}

// ── Spacing ────────────────────────────────────────────────────────────────────

#[test]
fn compile_spacing_token() {
    let token = make_token("spacing.m", VisualTokenKind::Spacing, "8");
    let recipe = compile_token_to_css(&token).unwrap();
    assert_eq!(
        recipe,
        CssRecipe::Spacing {
            property: "--aios-spacing-m".into(),
            value_px: 8,
        }
    );
}

#[test]
fn compile_spacing_zero() {
    let token = make_token("spacing.none", VisualTokenKind::Spacing, "0");
    let recipe = compile_token_to_css(&token).unwrap();
    assert_eq!(
        recipe,
        CssRecipe::Spacing {
            property: "--aios-spacing-none".into(),
            value_px: 0,
        }
    );
}

// ── Motion ─────────────────────────────────────────────────────────────────────

#[test]
fn compile_motion_ease_in() {
    let token = make_token("motion.fade", VisualTokenKind::Motion, "300|ease-in");
    let recipe = compile_token_to_css(&token).unwrap();
    assert_eq!(
        recipe,
        CssRecipe::Motion {
            property: "--aios-motion-fade".into(),
            duration_ms: 300,
            easing: CssEasing::EaseIn,
        }
    );
}

#[test]
fn compile_motion_cubic_bezier() {
    let token = make_token(
        "motion.bounce",
        VisualTokenKind::Motion,
        "500|cubic-bezier(0.34,1.56,0.64,1)",
    );
    let recipe = compile_token_to_css(&token).unwrap();
    assert_eq!(
        recipe,
        CssRecipe::Motion {
            property: "--aios-motion-bounce".into(),
            duration_ms: 500,
            easing: CssEasing::CubicBezier(0.34, 1.56, 0.64, 1.0),
        }
    );
}

#[test]
fn compile_motion_invalid_easing_returns_error() {
    let token = make_token("motion.bad", VisualTokenKind::Motion, "300|bounce-in-out");
    assert!(compile_token_to_css(&token).is_err());
}

// ── Icon ───────────────────────────────────────────────────────────────────────

#[test]
fn compile_icon_without_ctx_succeeds() {
    let token = make_token("icon.close", VisualTokenKind::Icon, "xmark");
    let recipe = compile_token_to_css(&token).unwrap();
    assert_eq!(
        recipe,
        CssRecipe::Icon {
            property: "--aios-icon-close".into(),
            glyph_id: "xmark".into(),
        }
    );
}

#[test]
fn compile_icon_with_signed_ctx_succeeds() {
    let token = make_token("icon.close", VisualTokenKind::Icon, "xmark");
    let recipe = compile_token_to_css_with_ctx(&token, &signed_ctx()).unwrap();
    assert_eq!(
        recipe,
        CssRecipe::Icon {
            property: "--aios-icon-close".into(),
            glyph_id: "xmark".into(),
        }
    );
}

#[test]
fn compile_icon_with_unsigned_ctx_fails_inv_i6() {
    let token = make_token("icon.close", VisualTokenKind::Icon, "xmark");
    let result = compile_token_to_css_with_ctx(&token, &unsigned_ctx());
    assert!(result.is_err());
    match result.unwrap_err() {
        WebRendererError::IconBundleVerificationFailed { theme_id, .. } => {
            assert_eq!(theme_id, "aios-dark");
        }
        _ => panic!("expected IconBundleVerificationFailed"),
    }
}

// ── Shape ──────────────────────────────────────────────────────────────────────

#[test]
fn compile_shape_rounded_rect() {
    let token = make_token("shape.card", VisualTokenKind::Shape, "rounded-rect|8");
    let recipe = compile_token_to_css(&token).unwrap();
    assert_eq!(
        recipe,
        CssRecipe::Shape {
            property: "--aios-shape-card".into(),
            kind: WebShapeKind::RoundedRect,
            radius_px: 8,
        }
    );
}

#[test]
fn compile_shape_circle() {
    let token = make_token("shape.avatar", VisualTokenKind::Shape, "circle|0");
    let recipe = compile_token_to_css(&token).unwrap();
    assert_eq!(
        recipe,
        CssRecipe::Shape {
            property: "--aios-shape-avatar".into(),
            kind: WebShapeKind::Circle,
            radius_px: 0,
        }
    );
}

// ── Elevation ──────────────────────────────────────────────────────────────────

#[test]
fn compile_elevation_token() {
    let token = make_token("elevation.card", VisualTokenKind::Elevation, "2|4|0.12");
    let recipe = compile_token_to_css(&token).unwrap();
    assert_eq!(
        recipe,
        CssRecipe::Elevation {
            property: "--aios-elevation-card".into(),
            z: 2,
            blur_px: 4,
            alpha: 0.12,
        }
    );
}

// ── CSS block assembly ─────────────────────────────────────────────────────────

#[test]
fn recipe_to_css_block_empty() {
    let block = recipe_to_css_block(&[]);
    assert_eq!(block, ":root {\n}");
}

#[test]
fn recipe_to_css_block_single_variable() {
    let recipes = vec![CssRecipe::Variable {
        property: "--aios-color-foo".into(),
        value: "#abc".into(),
    }];
    let block = recipe_to_css_block(&recipes);
    assert!(block.starts_with(":root {"));
    assert!(block.contains("  --aios-color-foo: #abc;"));
    assert!(block.ends_with('}'));
}

#[test]
fn recipe_to_css_block_font_emits_five_properties() {
    let recipes = vec![CssRecipe::Font {
        family: "Inter".into(),
        size_px: 16,
        weight: 400,
        italic: false,
        mono: false,
    }];
    let block = recipe_to_css_block(&recipes);
    assert!(block.contains("--aios-font-inter-family: \"Inter\";"));
    assert!(block.contains("--aios-font-inter-size: 16px;"));
    assert!(block.contains("--aios-font-inter-weight: 400;"));
    assert!(block.contains("--aios-font-inter-style: normal;"));
    assert!(block.contains("--aios-font-inter-mono: 0;"));
}

#[test]
fn recipe_to_css_block_font_italic_mono() {
    let recipes = vec![CssRecipe::Font {
        family: "JetBrains Mono".into(),
        size_px: 14,
        weight: 700,
        italic: true,
        mono: true,
    }];
    let block = recipe_to_css_block(&recipes);
    assert!(block.contains("--aios-font-jetbrains-mono-family: \"JetBrains Mono\";"));
    assert!(block.contains("--aios-font-jetbrains-mono-style: italic;"));
    assert!(block.contains("--aios-font-jetbrains-mono-mono: 1;"));
}

#[test]
fn recipe_to_css_block_spacing() {
    let recipes = vec![CssRecipe::Spacing {
        property: "--aios-spacing-m".into(),
        value_px: 8,
    }];
    let block = recipe_to_css_block(&recipes);
    assert!(block.contains("  --aios-spacing-m: 8px;"));
}

#[test]
fn recipe_to_css_block_motion() {
    let recipes = vec![CssRecipe::Motion {
        property: "--aios-motion-fade".into(),
        duration_ms: 300,
        easing: CssEasing::EaseInOut,
    }];
    let block = recipe_to_css_block(&recipes);
    assert!(block.contains("  --aios-motion-fade-duration: 300ms;"));
    assert!(block.contains("  --aios-motion-fade-easing: ease-in-out;"));
}

#[test]
fn recipe_to_css_block_shape_circle() {
    let recipes = vec![CssRecipe::Shape {
        property: "--aios-shape-avatar".into(),
        kind: WebShapeKind::Circle,
        radius_px: 0,
    }];
    let block = recipe_to_css_block(&recipes);
    assert!(block.contains("  --aios-shape-avatar: 50%;"));
}

#[test]
fn recipe_to_css_block_shape_pill() {
    let recipes = vec![CssRecipe::Shape {
        property: "--aios-shape-badge".into(),
        kind: WebShapeKind::Pill,
        radius_px: 0,
    }];
    let block = recipe_to_css_block(&recipes);
    assert!(block.contains("  --aios-shape-badge: 9999px;"));
}

#[test]
fn recipe_to_css_block_elevation() {
    let recipes = vec![CssRecipe::Elevation {
        property: "--aios-elevation-card".into(),
        z: 2,
        blur_px: 4,
        alpha: 0.12,
    }];
    let block = recipe_to_css_block(&recipes);
    assert!(block.contains("  --aios-elevation-card-z: 2;"));
    assert!(block.contains("  --aios-elevation-card-blur: 4px;"));
    assert!(block.contains("  --aios-elevation-card-alpha: 0.12;"));
}

#[test]
fn recipe_to_css_block_multi_recipe() {
    let recipes = vec![
        CssRecipe::Variable {
            property: "--aios-color-bg".into(),
            value: "#fff".into(),
        },
        CssRecipe::Spacing {
            property: "--aios-spacing-s".into(),
            value_px: 4,
        },
    ];
    let block = recipe_to_css_block(&recipes);
    assert!(block.contains("  --aios-color-bg: #fff;"));
    assert!(block.contains("  --aios-spacing-s: 4px;"));
    assert_eq!(block.matches(':').count(), 3);
}

#[test]
fn non_icon_token_with_unsigned_ctx_succeeds() {
    let token = make_token("color.bg", VisualTokenKind::Color, "#ffffff");
    let recipe = compile_token_to_css_with_ctx(&token, &unsigned_ctx()).unwrap();
    assert!(matches!(recipe, CssRecipe::Variable { .. }));
}
