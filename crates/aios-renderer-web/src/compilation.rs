//! `NodeKind` → DOM tag + Web Component compilation table (S7.5 §3).
//!
//! `WebCompilationRule` maps each of the 19 closed `NodeKind` variants to its
//! deterministic DOM tag, optional Web Component name, GPU/iframe requirement,
//! surface zone, and CSS `display` property per the S7.5 §3 compilation table.
//!
//! `WebCompilationContext` enforces three invariants at compile-request time:
//! * Recovery mode — only Chrome-zone kinds are admitted.
//! * Degraded mode — GPU-bearing kinds are rejected.
//! * WebGPU unavailable — GPU-bearing kinds are rejected.

use crate::error::WebRendererError;
use crate::types::WebRendererMode;
use aios_renderer_kde::NodeKind;

// ── WebSurfaceZone ────────────────────────────────────────────────────────────

/// Which renderer surface zone a `NodeKind` maps to (S7.5 §3.1).
///
/// Determines shadow-DOM placement and style isolation. Chrome-zone elements
/// render inside the closed shadow root (INV I2 + I7); Content-zone elements
/// render in the light DOM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebSurfaceZone {
    /// Rendered inside the closed Chrome shadow root (z-index 9999).
    /// Constitutional kinds only — AI subjects cannot author (S7.2 §I5).
    Chrome,
    /// Standard content rendered in the light DOM inside an application surface.
    Content,
    /// Background / non-interactive decorative layer.
    Background,
    /// Recovery shell surface served from `/aios/recovery` (INV I8).
    Recovery,
}

// ── WebCompilationRule ────────────────────────────────────────────────────────

/// Full deterministic compilation rule for a single `NodeKind` (S7.5 §3 table).
///
/// Every `NodeKind` maps to exactly one compile-time `WebCompilationRule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebCompilationRule {
    /// The node kind this rule describes.
    pub kind: NodeKind,
    /// The HTML / custom-element tag name (e.g. `"div"`, `"span"`, `"canvas"`).
    pub dom_tag: &'static str,
    /// Optional Web Component custom element name (e.g. `"aios-card"`).
    /// `None` for standard HTML elements with no custom-element wrapper.
    pub web_component: Option<&'static str>,
    /// Whether this kind requires a WebGPU adapter for rendering.
    pub requires_webgpu: bool,
    /// Whether this kind requires an `<iframe>` isolation boundary.
    /// `SurfaceEmbed` sets this to `true` — the embedded application owns its
    /// own WebGPU context inside the frame.
    pub requires_iframe: bool,
    /// Which renderer surface zone this kind belongs to.
    pub surface_zone: WebSurfaceZone,
    /// The CSS `display` property value (e.g. `"flex"`, `"block"`, `"inline"`).
    pub css_display: &'static str,
}

impl WebCompilationRule {
    /// Return the compile-time compilation rule for `kind`.
    ///
    /// The mapping follows the S7.5 §3 deterministic compilation table.
    /// Each arm lists exactly one `NodeKind` for spec-auditability.
    #[must_use]
    #[allow(
        clippy::match_same_arms,
        clippy::too_many_lines,
        reason = "S7.5 §3 table keeps each NodeKind on its own arm for clarity"
    )]
    pub const fn for_node(kind: NodeKind) -> Self {
        match kind {
            // ── Structural ──────────────────────────────────────────────
            NodeKind::Container => Self {
                kind: NodeKind::Container,
                dom_tag: "div",
                web_component: None,
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Content,
                css_display: "flex",
            },
            NodeKind::Divider => Self {
                kind: NodeKind::Divider,
                dom_tag: "hr",
                web_component: None,
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Content,
                css_display: "block",
            },
            NodeKind::Spacer => Self {
                kind: NodeKind::Spacer,
                dom_tag: "div",
                web_component: None,
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Content,
                css_display: "block",
            },

            // ── Text & typography ───────────────────────────────────────
            NodeKind::Text => Self {
                kind: NodeKind::Text,
                dom_tag: "span",
                web_component: None,
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Content,
                css_display: "inline",
            },
            NodeKind::Heading => Self {
                kind: NodeKind::Heading,
                dom_tag: "h1",
                web_component: None,
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Content,
                css_display: "block",
            },
            NodeKind::InlineCode => Self {
                kind: NodeKind::InlineCode,
                dom_tag: "code",
                web_component: None,
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Content,
                css_display: "inline",
            },
            NodeKind::CodeBlock => Self {
                kind: NodeKind::CodeBlock,
                dom_tag: "pre",
                web_component: None,
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Content,
                css_display: "block",
            },

            // ── Grouping / structure ────────────────────────────────────
            NodeKind::Card => Self {
                kind: NodeKind::Card,
                dom_tag: "div",
                web_component: Some("aios-card"),
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Content,
                css_display: "block",
            },
            NodeKind::List => Self {
                kind: NodeKind::List,
                dom_tag: "ul",
                web_component: Some("aios-list"),
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Content,
                css_display: "block",
            },
            NodeKind::Table => Self {
                kind: NodeKind::Table,
                dom_tag: "table",
                web_component: Some("aios-table"),
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Content,
                css_display: "block",
            },
            NodeKind::Form => Self {
                kind: NodeKind::Form,
                dom_tag: "form",
                web_component: None,
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Content,
                css_display: "block",
            },

            // ── Interactive ─────────────────────────────────────────────
            NodeKind::ActionButton => Self {
                kind: NodeKind::ActionButton,
                dom_tag: "button",
                web_component: Some("aios-action-button"),
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Content,
                css_display: "inline-block",
            },

            // ── GPU-bearing ─────────────────────────────────────────────
            NodeKind::Visualization => Self {
                kind: NodeKind::Visualization,
                dom_tag: "canvas",
                web_component: None,
                requires_webgpu: true,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Content,
                css_display: "block",
            },
            NodeKind::Stream => Self {
                kind: NodeKind::Stream,
                dom_tag: "video",
                web_component: Some("aios-stream"),
                requires_webgpu: true,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Content,
                css_display: "block",
            },
            NodeKind::SurfaceEmbed => Self {
                kind: NodeKind::SurfaceEmbed,
                dom_tag: "iframe",
                web_component: None,
                requires_webgpu: false,
                requires_iframe: true,
                surface_zone: WebSurfaceZone::Content,
                css_display: "block",
            },

            // ── AIOS chrome (constitutional) ────────────────────────────
            NodeKind::SecurityIndicator => Self {
                kind: NodeKind::SecurityIndicator,
                dom_tag: "div",
                web_component: Some("aios-security-indicator"),
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Chrome,
                css_display: "block",
            },
            NodeKind::ApprovalPrompt => Self {
                kind: NodeKind::ApprovalPrompt,
                dom_tag: "dialog",
                web_component: Some("aios-approval-prompt"),
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Chrome,
                css_display: "block",
            },
            NodeKind::EvidenceLink => Self {
                kind: NodeKind::EvidenceLink,
                dom_tag: "span",
                web_component: Some("aios-evidence-link"),
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Chrome,
                css_display: "inline",
            },
            NodeKind::AgentMessage => Self {
                kind: NodeKind::AgentMessage,
                dom_tag: "div",
                web_component: Some("aios-agent-message"),
                requires_webgpu: false,
                requires_iframe: false,
                surface_zone: WebSurfaceZone::Content,
                css_display: "block",
            },
        }
    }
}

// ── WebCompilationContext ─────────────────────────────────────────────────────

/// Runtime context that gates compilation-rule resolution (S7.5 §3.2).
///
/// Three invariants are enforced at compile-request time:
/// * **Recovery mode** — when `recovery_active` is `true`, only kinds whose
///   surface zone is `Chrome` are permitted.
/// * **Degraded mode** — when `renderer_mode` is `Degraded(_)`, any kind with
///   `requires_webgpu = true` is rejected.
/// * **WebGPU unavailable** — when `webgpu_available` is `false`, any kind with
///   `requires_webgpu = true` is rejected.
pub struct WebCompilationContext {
    /// Current renderer operational mode.
    pub renderer_mode: WebRendererMode,
    /// Whether the recovery shell is active.
    pub recovery_active: bool,
    /// Whether a WebGPU adapter is available on this device.
    pub webgpu_available: bool,
}

impl WebCompilationContext {
    /// Resolve the compilation rule for `kind`, enforcing recovery-zone
    /// admission and WebGPU availability invariants.
    ///
    /// # Errors
    ///
    /// * `WebRendererError::Internal("recovery: surface restricted")` —
    ///   `recovery_active` is `true` and `kind`'s surface zone is not `Chrome`.
    /// * `WebRendererError::WebgpuAdapterUnavailable("gpu-bearing kind disallowed")` —
    ///   `renderer_mode` is `Degraded(_)` or `webgpu_available` is `false`,
    ///   and `kind` requires WebGPU.
    pub fn compile(&self, kind: NodeKind) -> Result<WebCompilationRule, WebRendererError> {
        let rule = WebCompilationRule::for_node(kind);

        // Recovery mode — only Chrome-zone kinds are admitted.
        if self.recovery_active && !matches!(rule.surface_zone, WebSurfaceZone::Chrome) {
            return Err(WebRendererError::Internal(
                "recovery: surface restricted".into(),
            ));
        }

        // Degraded mode or WebGPU unavailable — reject GPU-bearing kinds.
        if (matches!(self.renderer_mode, WebRendererMode::Degraded(_)) || !self.webgpu_available)
            && rule.requires_webgpu
        {
            return Err(WebRendererError::WebgpuAdapterUnavailable(
                "gpu-bearing kind disallowed".into(),
            ));
        }

        Ok(rule)
    }
}

// ── Compile-time sanity check ─────────────────────────────────────────────────

/// Verify that `WebCompilationRule::for_node` is evaluable at compile time.
///
/// This `const` item forces the compiler to prove that `for_node` is a
/// const-evaluable function for every `NodeKind` variant.
#[doc(hidden)]
pub const WEB_COMPILATION_RULE_CONST_CHECK: [WebCompilationRule; NodeKind::LEN] = {
    let mut i = 0;
    let mut rules = [WebCompilationRule::for_node(NodeKind::Container); NodeKind::LEN];
    while i < NodeKind::ALL.len() {
        rules[i] = WebCompilationRule::for_node(NodeKind::ALL[i]);
        i += 1;
    }
    rules
};
