//! T-141 compilation tests — `WebCompilationRule` + `WebCompilationContext` (19 tests).
//!
//! Covers: full 19-variant `NodeKind` → DOM/Web Component mapping, GPU-bearing
//! detection, surface zone classification, recovery-mode admission, WebGPU
//! fallback, compile-time const evaluation, and the `DEFAULT_CODE_VERSION`
//! regression guard.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::no_effect_underscore_binding
)]

use aios_renderer_kde::NodeKind;
use aios_renderer_web::{
    WebCompilationContext, WebCompilationRule, WebRendererMode, WebSurfaceZone,
    DEFAULT_CODE_VERSION,
};

// ── DEFAULT_CODE_VERSION regression ─────────────────────────────────────────

#[test]
fn default_code_version_constant_present_unchanged() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-renderer-web/0.0.1-T139");
}

// ── Full 19-variant coverage ────────────────────────────────────────────────

#[test]
fn every_node_kind_has_a_compilation_rule() {
    for &kind in NodeKind::ALL {
        let rule = WebCompilationRule::for_node(kind);
        assert_eq!(
            rule.kind, kind,
            "rule.kind must match the requested NodeKind"
        );
        assert!(
            !rule.dom_tag.is_empty(),
            "dom_tag must be non-empty for {kind:?}"
        );
        assert!(
            !rule.css_display.is_empty(),
            "css_display must be non-empty for {kind:?}"
        );
    }
}

// ── GPU-bearing detection ───────────────────────────────────────────────────

#[test]
fn webgpu_bearing_kinds_visualization_and_stream() {
    for &kind in NodeKind::ALL {
        let rule = WebCompilationRule::for_node(kind);
        if matches!(kind, NodeKind::Visualization | NodeKind::Stream) {
            assert!(
                rule.requires_webgpu,
                "{kind:?} must have requires_webgpu = true"
            );
        } else {
            assert!(
                !rule.requires_webgpu,
                "{kind:?} must have requires_webgpu = false"
            );
        }
    }
    // SurfaceEmbed: has its own WebGPU inside the iframe but does not require
    // the outer renderer's WebGPU adapter.
    let se = WebCompilationRule::for_node(NodeKind::SurfaceEmbed);
    assert!(!se.requires_webgpu);
    assert!(se.requires_iframe);
}

// ── DOM tag assertions ──────────────────────────────────────────────────────

#[test]
fn dom_tag_for_text_is_span() {
    let rule = WebCompilationRule::for_node(NodeKind::Text);
    assert_eq!(rule.dom_tag, "span");
}

#[test]
fn dom_tag_for_heading_is_h1() {
    // Canonical tag is "h1"; actual h-level (1..6) is payload-driven at render time.
    let rule = WebCompilationRule::for_node(NodeKind::Heading);
    assert_eq!(rule.dom_tag, "h1");
}

#[test]
fn dom_tag_for_visualization_is_canvas() {
    let rule = WebCompilationRule::for_node(NodeKind::Visualization);
    assert_eq!(rule.dom_tag, "canvas");
}

#[test]
fn dom_tag_for_stream_is_video() {
    let rule = WebCompilationRule::for_node(NodeKind::Stream);
    assert_eq!(rule.dom_tag, "video");
}

#[test]
fn dom_tag_for_surface_embed_is_iframe() {
    let rule = WebCompilationRule::for_node(NodeKind::SurfaceEmbed);
    assert_eq!(rule.dom_tag, "iframe");
}

// ── Web Component assertions ────────────────────────────────────────────────

#[test]
fn web_component_for_security_indicator_is_aios_security_indicator() {
    let rule = WebCompilationRule::for_node(NodeKind::SecurityIndicator);
    assert_eq!(rule.web_component, Some("aios-security-indicator"));
}

#[test]
fn web_component_for_approval_prompt_is_aios_approval_prompt() {
    let rule = WebCompilationRule::for_node(NodeKind::ApprovalPrompt);
    assert_eq!(rule.web_component, Some("aios-approval-prompt"));
}

#[test]
fn web_component_for_evidence_link_is_aios_evidence_link() {
    let rule = WebCompilationRule::for_node(NodeKind::EvidenceLink);
    assert_eq!(rule.web_component, Some("aios-evidence-link"));
}

#[test]
fn web_component_for_text_is_none() {
    let rule = WebCompilationRule::for_node(NodeKind::Text);
    assert_eq!(rule.web_component, None);
}

// ── Surface zone classification ─────────────────────────────────────────────

#[test]
fn surface_zone_for_chrome_kinds() {
    let chrome_kinds = [
        NodeKind::SecurityIndicator,
        NodeKind::ApprovalPrompt,
        NodeKind::EvidenceLink,
    ];
    for &kind in &chrome_kinds {
        let rule = WebCompilationRule::for_node(kind);
        assert!(
            matches!(rule.surface_zone, WebSurfaceZone::Chrome),
            "{kind:?} must map to Chrome zone"
        );
    }
}

#[test]
fn surface_zone_for_content_kinds() {
    let content_kinds = [NodeKind::Container, NodeKind::Text, NodeKind::Heading];
    for &kind in &content_kinds {
        let rule = WebCompilationRule::for_node(kind);
        assert!(
            matches!(rule.surface_zone, WebSurfaceZone::Content),
            "{kind:?} must map to Content zone"
        );
    }
}

// ── CompilationContext — normal path ────────────────────────────────────────

#[test]
fn compilation_context_normal_with_webgpu_compiles_visualization() {
    let ctx = WebCompilationContext {
        renderer_mode: WebRendererMode::Normal,
        recovery_active: false,
        webgpu_available: true,
    };
    let result = ctx.compile(NodeKind::Visualization);
    assert!(
        result.is_ok(),
        "Visualization must compile with GPU available"
    );
    let rule = result.unwrap();
    assert_eq!(rule.dom_tag, "canvas");
    assert!(rule.requires_webgpu);
}

// ── CompilationContext — WebGPU unavailable ─────────────────────────────────

#[test]
fn compilation_context_without_webgpu_rejects_visualization() {
    let ctx = WebCompilationContext {
        renderer_mode: WebRendererMode::Normal,
        recovery_active: false,
        webgpu_available: false,
    };
    let result = ctx.compile(NodeKind::Visualization);
    assert!(result.is_err());
    match result.unwrap_err() {
        aios_renderer_web::WebRendererError::WebgpuAdapterUnavailable(msg) => {
            assert!(msg.contains("gpu-bearing"));
        }
        other => panic!("expected WebgpuAdapterUnavailable, got {other:?}"),
    }
}

// ── CompilationContext — Degraded mode ──────────────────────────────────────

#[test]
fn compilation_context_degraded_mode_rejects_visualization() {
    let ctx = WebCompilationContext {
        renderer_mode: WebRendererMode::Degraded("webgpu_init_failed".into()),
        recovery_active: false,
        webgpu_available: true, // GPU available but mode is degraded
    };
    let result = ctx.compile(NodeKind::Visualization);
    assert!(result.is_err());
    match result.unwrap_err() {
        aios_renderer_web::WebRendererError::WebgpuAdapterUnavailable(msg) => {
            assert!(msg.contains("gpu-bearing"));
        }
        other => panic!("expected WebgpuAdapterUnavailable, got {other:?}"),
    }
}

// ── CompilationContext — Recovery mode ──────────────────────────────────────

#[test]
fn compilation_context_recovery_active_rejects_text() {
    let ctx = WebCompilationContext {
        renderer_mode: WebRendererMode::Recovery,
        recovery_active: true,
        webgpu_available: false,
    };
    let result = ctx.compile(NodeKind::Text);
    assert!(result.is_err());
    match result.unwrap_err() {
        aios_renderer_web::WebRendererError::Internal(msg) => {
            assert!(msg.contains("recovery: surface restricted"));
        }
        other => panic!("expected Internal recovery restriction, got {other:?}"),
    }
}

// ── Compile-time const evaluation ───────────────────────────────────────────

#[test]
fn compilation_rule_const_constructable_at_compile_time() {
    // Force the compiler to prove the const check item exists and is evaluable.
    let check = aios_renderer_web::WEB_COMPILATION_RULE_CONST_CHECK;
    assert_eq!(check.len(), NodeKind::LEN);
    for (i, rule) in check.iter().enumerate() {
        assert_eq!(
            rule.kind,
            NodeKind::ALL[i],
            "const check index {i}: rule.kind must match NodeKind::ALL[{i}]"
        );
    }
}
