//! T-130 token compilation tests — 16 tests covering `VisualToken` → `QtRecipe`,
//! `canonical_value` parsing, INV I6 enforcement, and serde round-trips.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::no_effect_underscore_binding
)]

use aios_renderer_kde::{
    compile_token, compile_token_with_ctx, IconLookupCtx, KdeRendererError, QEasingCurve,
    QFontWeight, QPaletteRole, QtRecipe, ShapeKind, VisualToken, VisualTokenKind,
};

// ── Color tokens ──────────────────────────────────────────────────────────────

#[test]
fn color_hex_round_trip() {
    let token = VisualToken {
        id: "color.window.background".into(),
        kind: VisualTokenKind::Color,
        canonical_value: "#1e1e1e".into(),
    };
    let recipe = compile_token(&token).expect("valid color token");
    match recipe {
        QtRecipe::Palette { role, color_hex } => {
            assert_eq!(role, QPaletteRole::WindowBackground);
            assert_eq!(color_hex, "#1e1e1e");
        }
        other => panic!("expected QtRecipe::Palette, got {other:?}"),
    }
}

#[test]
fn color_token_with_window_background_id_maps_to_window_background_role() {
    let token = VisualToken {
        id: "color.window.background".into(),
        kind: VisualTokenKind::Color,
        canonical_value: "#2e2e2e".into(),
    };
    let recipe = compile_token(&token).expect("valid color token");
    match recipe {
        QtRecipe::Palette { role, .. } => {
            assert_eq!(role, QPaletteRole::WindowBackground);
        }
        other => panic!("expected QtRecipe::Palette, got {other:?}"),
    }
}

#[test]
fn color_token_with_text_id_maps_to_window_text_role() {
    let token = VisualToken {
        id: "color.text".into(),
        kind: VisualTokenKind::Color,
        canonical_value: "#ffffff".into(),
    };
    let recipe = compile_token(&token).expect("valid color token");
    match recipe {
        QtRecipe::Palette { role, .. } => {
            assert_eq!(role, QPaletteRole::Text);
        }
        other => panic!("expected QtRecipe::Palette, got {other:?}"),
    }
}

#[test]
fn invalid_color_hex_returns_internal_error() {
    let token = VisualToken {
        id: "color.window.background".into(),
        kind: VisualTokenKind::Color,
        canonical_value: "not-a-color".into(),
    };
    let result = compile_token(&token);
    assert!(result.is_err());
    match result.unwrap_err() {
        KdeRendererError::Internal(msg) => {
            assert!(msg.contains("invalid color hex"), "got: {msg}");
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

// ── Font tokens ───────────────────────────────────────────────────────────────

#[test]
fn font_schema_round_trip() {
    let token = VisualToken {
        id: "font.body".into(),
        kind: VisualTokenKind::Font,
        canonical_value: "Noto Sans|10|bold|false|false".into(),
    };
    let recipe = compile_token(&token).expect("valid font token");
    match recipe {
        QtRecipe::Font {
            family,
            point_size_eq,
            weight,
            italic,
            monospace,
        } => {
            assert_eq!(family, "Noto Sans");
            assert!((point_size_eq - 10.0).abs() < f32::EPSILON);
            assert_eq!(weight, QFontWeight::Bold);
            assert!(!italic);
            assert!(!monospace);
        }
        other => panic!("expected QtRecipe::Font, got {other:?}"),
    }
}

#[test]
fn font_schema_monospace_flag_round_trip() {
    let token = VisualToken {
        id: "font.code".into(),
        kind: VisualTokenKind::Font,
        canonical_value: "Fira Code|12|medium|false|true".into(),
    };
    let recipe = compile_token(&token).expect("valid font token");
    match recipe {
        QtRecipe::Font {
            family,
            point_size_eq,
            weight,
            italic,
            monospace,
        } => {
            assert_eq!(family, "Fira Code");
            assert!((point_size_eq - 12.0).abs() < f32::EPSILON);
            assert_eq!(weight, QFontWeight::Medium);
            assert!(!italic);
            assert!(monospace);
        }
        other => panic!("expected QtRecipe::Font, got {other:?}"),
    }
}

// ── Spacing tokens ────────────────────────────────────────────────────────────

#[test]
fn spacing_token_round_trip() {
    let token = VisualToken {
        id: "spacing.gap".into(),
        kind: VisualTokenKind::Spacing,
        canonical_value: "16".into(),
    };
    let recipe = compile_token(&token).expect("valid spacing token");
    match recipe {
        QtRecipe::Spacing { logical_pixels } => {
            assert_eq!(logical_pixels, 16);
        }
        other => panic!("expected QtRecipe::Spacing, got {other:?}"),
    }
}

#[test]
fn spacing_invalid_returns_internal_error() {
    let token = VisualToken {
        id: "spacing.gap".into(),
        kind: VisualTokenKind::Spacing,
        canonical_value: "not-a-number".into(),
    };
    let result = compile_token(&token);
    assert!(result.is_err());
    match result.unwrap_err() {
        KdeRendererError::Internal(msg) => {
            assert!(msg.contains("invalid spacing"), "got: {msg}");
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

// ── Motion tokens ─────────────────────────────────────────────────────────────

#[test]
fn motion_token_round_trip() {
    let token = VisualToken {
        id: "motion.fade".into(),
        kind: VisualTokenKind::Motion,
        canonical_value: "200|out-cubic".into(),
    };
    let recipe = compile_token(&token).expect("valid motion token");
    match recipe {
        QtRecipe::Motion {
            duration_ms,
            easing,
        } => {
            assert_eq!(duration_ms, 200);
            assert_eq!(easing, QEasingCurve::OutCubic);
        }
        other => panic!("expected QtRecipe::Motion, got {other:?}"),
    }
}

#[test]
fn motion_invalid_easing_returns_internal_error() {
    let token = VisualToken {
        id: "motion.bad".into(),
        kind: VisualTokenKind::Motion,
        canonical_value: "200|warp-drive".into(),
    };
    let result = compile_token(&token);
    assert!(result.is_err());
    match result.unwrap_err() {
        KdeRendererError::Internal(msg) => {
            assert!(msg.contains("invalid easing"), "got: {msg}");
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

// ── Icon tokens ───────────────────────────────────────────────────────────────

#[test]
fn icon_compile_with_root_signed_ctx_succeeds() {
    let ctx = IconLookupCtx {
        theme_id: "dark".into(),
        root_signed: true,
    };
    let token = VisualToken {
        id: "COLOR_ACTION_AI".into(),
        kind: VisualTokenKind::Icon,
        canonical_value: "any".into(),
    };
    let recipe = compile_token_with_ctx(&token, &ctx).expect("root-signed icon");
    match recipe {
        QtRecipe::Icon {
            theme_path,
            fallback_to_breeze,
        } => {
            assert_eq!(
                theme_path,
                "/aios/system/themes/dark/icons/COLOR_ACTION_AI.svg"
            );
            assert!(!fallback_to_breeze);
        }
        other => panic!("expected QtRecipe::Icon, got {other:?}"),
    }
}

#[test]
fn icon_compile_without_root_signed_returns_icon_bundle_verification_failed() {
    let ctx = IconLookupCtx {
        theme_id: "dark".into(),
        root_signed: false,
    };
    let token = VisualToken {
        id: "COLOR_ACTION_AI".into(),
        kind: VisualTokenKind::Icon,
        canonical_value: "any".into(),
    };
    let result = compile_token_with_ctx(&token, &ctx);
    assert!(result.is_err());
    match result.unwrap_err() {
        KdeRendererError::IconBundleVerificationFailed { theme_id, reason } => {
            assert_eq!(theme_id, "dark");
            assert_eq!(reason, "not root-signed");
        }
        other => panic!("expected IconBundleVerificationFailed, got {other:?}"),
    }
}

// ── Shape tokens ──────────────────────────────────────────────────────────────

#[test]
fn shape_token_round_trip() {
    let token = VisualToken {
        id: "shape.card".into(),
        kind: VisualTokenKind::Shape,
        canonical_value: "rounded-rect|8".into(),
    };
    let recipe = compile_token(&token).expect("valid shape token");
    match recipe {
        QtRecipe::Shape {
            kind,
            corner_radius_px,
        } => {
            assert_eq!(kind, ShapeKind::RoundedRect);
            assert_eq!(corner_radius_px, 8);
        }
        other => panic!("expected QtRecipe::Shape, got {other:?}"),
    }
}

// ── Elevation tokens ──────────────────────────────────────────────────────────

#[test]
fn elevation_token_round_trip() {
    let token = VisualToken {
        id: "elevation.card".into(),
        kind: VisualTokenKind::Elevation,
        canonical_value: "4|12|160".into(),
    };
    let recipe = compile_token(&token).expect("valid elevation token");
    match recipe {
        QtRecipe::Elevation {
            z_order,
            shadow_blur_px,
            shadow_alpha,
        } => {
            assert_eq!(z_order, 4);
            assert_eq!(shadow_blur_px, 12);
            assert_eq!(shadow_alpha, 160);
        }
        other => panic!("expected QtRecipe::Elevation, got {other:?}"),
    }
}

// ── Serde round-trip ──────────────────────────────────────────────────────────

#[test]
fn serde_round_trip_qt_recipe() {
    let recipe = QtRecipe::Palette {
        role: QPaletteRole::WindowBackground,
        color_hex: "#1e1e1e".into(),
    };
    let json = serde_json::to_string(&recipe).expect("serialize QtRecipe");
    let roundtripped: QtRecipe = serde_json::from_str(&json).expect("deserialize QtRecipe");
    assert_eq!(roundtripped, recipe);

    // Also verify that the JSON contains the expected fields.
    assert!(json.contains("\"Palette\""));
    assert!(json.contains("\"WindowBackground\""));
    assert!(json.contains("\"#1e1e1e\""));
}

// ── Disabled modifier ─────────────────────────────────────────────────────────

#[test]
fn qt_recipe_palette_disabled_modifier_marker_present() {
    // QPaletteRole::Disabled exists as a variant, fulfilling the requirement
    // that a disabled modifier marker is present in the type system.
    let role = QPaletteRole::Disabled;
    // Serde round-trip confirms it is a real variant.
    let json = serde_json::to_string(&role).expect("serialize Disabled");
    let roundtripped: QPaletteRole = serde_json::from_str(&json).expect("deserialize Disabled");
    assert_eq!(roundtripped, QPaletteRole::Disabled);
    assert!(json.contains("Disabled"));

    // A token with a disabled-prefixed id maps to the Disabled role.
    let token = VisualToken {
        id: "color.disabled.text".into(),
        kind: VisualTokenKind::Color,
        canonical_value: "#888888".into(),
    };
    let recipe = compile_token(&token).expect("valid disabled color token");
    match recipe {
        QtRecipe::Palette { role, color_hex } => {
            assert_eq!(role, QPaletteRole::Disabled);
            assert_eq!(color_hex, "#888888");
        }
        other => panic!("expected QtRecipe::Palette, got {other:?}"),
    }
}
