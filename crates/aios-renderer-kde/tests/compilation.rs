//! T-129 compilation tests — 19 unit tests covering the `NodeKind` → Qt/QML
//! compilation table and `CompilationContext` invariant enforcement per S7.4 §3.2.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code; panic-on-failure is idiomatic"
)]

use aios_renderer_kde::{
    CompilationContext, CompilationRule, NodeKind, NodeSurfaceKind, RendererMode,
    DEFAULT_CODE_VERSION,
};

// ── CompilationRule::for_node — full-table coverage ────────────────────────

#[test]
fn every_node_kind_has_a_compilation_rule() {
    assert_eq!(NodeKind::ALL.len(), 19);

    for kind in NodeKind::ALL {
        let rule = CompilationRule::for_node(*kind);
        assert_eq!(rule.kind, *kind);
        assert!(
            !rule.qml_module.is_empty(),
            "NodeKind::{kind:?} must have a non-empty qml_module"
        );
        assert!(
            !rule.qml_type.is_empty(),
            "NodeKind::{kind:?} must have a non-empty qml_type"
        );
    }
}

#[test]
fn gpu_bearing_kinds_match_skeleton_test() {
    for kind in NodeKind::ALL {
        let rule = CompilationRule::for_node(*kind);
        match kind {
            NodeKind::Visualization | NodeKind::Stream | NodeKind::SurfaceEmbed => {
                assert!(
                    rule.requires_gpu,
                    "NodeKind::{kind:?} must be GPU-bearing per S7.4 §3.2"
                );
            }
            _ => {
                assert!(
                    !rule.requires_gpu,
                    "NodeKind::{kind:?} must NOT be GPU-bearing per S7.4 §3.2"
                );
            }
        }
    }
}

// ── QML module assertions ──────────────────────────────────────────────────

#[test]
fn qml_module_for_visualization_uses_quick() {
    let rule = CompilationRule::for_node(NodeKind::Visualization);
    assert!(
        rule.qml_module.starts_with("QtQuick"),
        "Visualization qml_module expected 'QtQuick', got '{}'",
        rule.qml_module
    );
}

#[test]
fn qml_module_for_security_indicator_uses_aios_primitives() {
    let rule = CompilationRule::for_node(NodeKind::SecurityIndicator);
    assert_eq!(
        rule.qml_module, "AIOSPrimitives",
        "SecurityIndicator must use AIOSPrimitives"
    );
}

#[test]
fn qml_module_for_approval_prompt_uses_aios_primitives() {
    let rule = CompilationRule::for_node(NodeKind::ApprovalPrompt);
    assert_eq!(
        rule.qml_module, "AIOSPrimitives",
        "ApprovalPrompt must use AIOSPrimitives"
    );
}

#[test]
fn qml_module_for_list_uses_quick_controls() {
    let rule = CompilationRule::for_node(NodeKind::List);
    assert!(
        rule.qml_module.starts_with("QtQuick.Controls"),
        "List qml_module expected 'QtQuick.Controls', got '{}'",
        rule.qml_module
    );
}

// ── Surface kind assertions ─────────────────────────────────────────────────

#[test]
fn surface_kind_for_app_surface_is_app_surface() {
    let rule = CompilationRule::for_node(NodeKind::SurfaceEmbed);
    assert_eq!(
        rule.surface_kind,
        NodeSurfaceKind::AppSurface,
        "SurfaceEmbed must map to AppSurface"
    );
}

#[test]
fn surface_kind_for_stream_surface_is_stream_surface() {
    let rule = CompilationRule::for_node(NodeKind::Stream);
    assert_eq!(
        rule.surface_kind,
        NodeSurfaceKind::StreamSurface,
        "Stream must map to StreamSurface"
    );
}

#[test]
fn surface_kind_for_aios_surface_is_aios_surface() {
    let rule = CompilationRule::for_node(NodeKind::SecurityIndicator);
    assert_eq!(
        rule.surface_kind,
        NodeSurfaceKind::AiosSurface,
        "SecurityIndicator must map to AiosSurface"
    );
}

#[test]
fn surface_kind_for_text_is_none() {
    let rule = CompilationRule::for_node(NodeKind::Text);
    assert_eq!(
        rule.surface_kind,
        NodeSurfaceKind::None,
        "Text surface_kind must be None (content-only)"
    );
}

// ── Allowed parents ─────────────────────────────────────────────────────────

#[test]
fn allowed_parents_for_list_includes_container() {
    let rule = CompilationRule::for_node(NodeKind::List);
    assert!(
        rule.allowed_parents.contains(&NodeKind::Container),
        "List allowed_parents must include Container"
    );
}

// ── CompilationContext — normal mode ────────────────────────────────────────

#[test]
fn compilation_context_normal_mode_compiles_visualization() {
    let ctx = CompilationContext {
        renderer_mode: RendererMode::Normal,
        recovery_active: false,
    };
    let result = ctx.compile(NodeKind::Visualization);
    assert!(result.is_ok(), "Normal mode must compile Visualization");
    let rule = result.unwrap();
    assert_eq!(rule.kind, NodeKind::Visualization);
    assert!(rule.requires_gpu);
}

// ── CompilationContext — degraded mode (INV I7) ─────────────────────────────

#[test]
fn compilation_context_degraded_mode_rejects_gpu_bearing() {
    let ctx = CompilationContext {
        renderer_mode: RendererMode::Degraded("kwin_unreachable".into()),
        recovery_active: false,
    };
    let result = ctx.compile(NodeKind::Visualization);
    assert!(
        result.is_err(),
        "Degraded mode must reject GPU-bearing kind"
    );
    let err = result.unwrap_err();
    assert!(
        matches!(
            &err,
            aios_renderer_kde::KdeRendererError::Degraded(msg) if msg.contains("gpu-bearing")
        ),
        "expected Degraded error, got {err:?}"
    );
}

#[test]
fn compilation_context_degraded_mode_compiles_text() {
    let ctx = CompilationContext {
        renderer_mode: RendererMode::Degraded("kwin_unreachable".into()),
        recovery_active: false,
    };
    let result = ctx.compile(NodeKind::Text);
    assert!(result.is_ok(), "Degraded mode must compile non-GPU Text");
    let rule = result.unwrap();
    assert_eq!(rule.kind, NodeKind::Text);
    assert!(!rule.requires_gpu);
}

// ── CompilationContext — recovery mode (INV I5) ─────────────────────────────

#[test]
fn compilation_context_recovery_mode_compiles_aios_surface() {
    let ctx = CompilationContext {
        renderer_mode: RendererMode::Normal,
        recovery_active: true,
    };
    let result = ctx.compile(NodeKind::SecurityIndicator);
    assert!(
        result.is_ok(),
        "Recovery mode must compile AiosSurface kind"
    );
    let rule = result.unwrap();
    assert_eq!(rule.kind, NodeKind::SecurityIndicator);
    assert_eq!(rule.surface_kind, NodeSurfaceKind::AiosSurface);
}

#[test]
fn compilation_context_recovery_mode_rejects_text() {
    let ctx = CompilationContext {
        renderer_mode: RendererMode::Normal,
        recovery_active: true,
    };
    let result = ctx.compile(NodeKind::Text);
    assert!(
        result.is_err(),
        "Recovery mode must reject non-AiosSurface kind"
    );
    let err = result.unwrap_err();
    assert!(
        matches!(
            &err,
            aios_renderer_kde::KdeRendererError::Internal(msg) if msg.contains("AIOS_SURFACE")
        ),
        "expected Internal containing 'AIOS_SURFACE', got {err:?}"
    );
}

// ── Compile-time construction check ─────────────────────────────────────────

#[test]
fn compilation_rule_round_trip_constructable_at_compile_time() {
    // Verify that the const-check item exists and covers all 19 variants.
    // The const item `COMPILATION_RULE_CONST_CHECK` forces compile-time
    // evaluation; this test just confirms the array is accessible at runtime.
    let check = aios_renderer_kde::compilation::COMPILATION_RULE_CONST_CHECK;
    assert_eq!(check.len(), 19);
    for (i, rule) in check.iter().enumerate() {
        assert_eq!(rule.kind, NodeKind::ALL[i]);
    }
}

// ── DEFAULT_CODE_VERSION regression ─────────────────────────────────────────

#[test]
fn default_code_version_constant_present_unchanged() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-renderer-kde/0.1.0-T138");
}

// ── Extra surface-kind coverage ─────────────────────────────────────────────

#[test]
fn surface_kind_for_container_is_none() {
    let rule = CompilationRule::for_node(NodeKind::Container);
    assert_eq!(rule.surface_kind, NodeSurfaceKind::None);
}

#[test]
fn qml_module_for_evidence_link_uses_aios_primitives() {
    let rule = CompilationRule::for_node(NodeKind::EvidenceLink);
    assert_eq!(rule.qml_module, "AIOSPrimitives");
}
