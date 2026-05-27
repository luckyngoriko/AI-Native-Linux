//! `NodeKind` → Qt/QML compilation table (S7.4 §3.2).
//!
//! `CompilationRule` expands T-127's single-string `qml_primitive` hint into a
//! full deterministic compilation rule: QML module, QML type, GPU device
//! requirement, surface kind, and allowed parent set.
//!
//! `CompilationContext` enforces two invariants at compile-request time:
//! * INV I5 — recovery shell only admits `NodeKind`s whose surface kind is
//!   `AiosSurface`.
//! * INV I7 — degraded (text-only fallback) mode rejects GPU-bearing kinds.

use crate::error::KdeRendererError;
use crate::node_kind::NodeKind;
use crate::types::RendererMode;

// ── NodeSurfaceKind ─────────────────────────────────────────────────────────

/// Which surface class a `NodeKind` maps to at the compositor level (S7.4 §3.2).
///
/// Content-only kinds (`Text`, `List`, `ActionButton`, etc.) map to `None` —
/// they are rendered inside a parent surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeSurfaceKind {
    /// Content rendered inside a parent surface; no own surface.
    None,
    /// AIOS-owned surface (chrome indicators, approval prompts, evidence, agent
    /// output).
    AiosSurface,
    /// Embedded application surface (from S7.1 `APP_SURFACE`).
    AppSurface,
    /// Live data stream surface (S7.1 `STREAM_SURFACE`).
    StreamSurface,
    /// Chrome overlay reserved for the AIOS chrome service (INV I4).
    ChromeOverlay,
}

// ── CompilationRule ─────────────────────────────────────────────────────────

/// Full deterministic compilation rule for a single `NodeKind` (S7.4 §3.2 table).
///
/// Every `NodeKind` maps to exactly one compile-time `CompilationRule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompilationRule {
    /// The node kind this rule describes.
    pub kind: NodeKind,
    /// QML import module (e.g. `"QtQuick"`, `"QtQuick.Controls"`,
    /// `"AIOSPrimitives"`).
    pub qml_module: &'static str,
    /// QML type name within the module (e.g. `"Item"`, `"Label"`,
    /// `"AIOSSecurityIndicator"`).
    pub qml_type: &'static str,
    /// Whether this kind requires a GPU binding (`wgpu` / `VkDevice`).
    pub requires_gpu: bool,
    /// Compositor surface class; `None` for content-only kinds.
    pub surface_kind: NodeSurfaceKind,
    /// The set of `NodeKind` values that may legally parent this kind.
    /// Chrome kinds return an empty slice (they are restricted to the
    /// chrome composition zone, not to a `NodeKind`-identified container).
    pub allowed_parents: &'static [NodeKind],
}

impl CompilationRule {
    /// Return the compile-time compilation rule for `kind`.
    ///
    /// The mapping follows the S7.4 §3.2 deterministic compilation table.
    /// Each arm lists exactly one `NodeKind` for spec-auditability, even
    /// when several share the same QML module and type.
    #[must_use]
    #[allow(
        clippy::match_same_arms,
        clippy::too_many_lines,
        reason = "S7.4 §3.2 table keeps each NodeKind on its own arm for clarity"
    )]
    pub const fn for_node(kind: NodeKind) -> Self {
        match kind {
            // ── Structural ──────────────────────────────────────────────
            NodeKind::Container => Self {
                kind: NodeKind::Container,
                qml_module: "QtQuick",
                qml_type: "Item",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::None,
                allowed_parents: &[
                    NodeKind::Container,
                    NodeKind::Card,
                    NodeKind::Form,
                    NodeKind::List,
                    NodeKind::Table,
                ],
            },
            NodeKind::Divider => Self {
                kind: NodeKind::Divider,
                qml_module: "QtQuick",
                qml_type: "Item",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::None,
                allowed_parents: &[
                    NodeKind::Container,
                    NodeKind::Card,
                    NodeKind::Form,
                    NodeKind::List,
                    NodeKind::Table,
                ],
            },
            NodeKind::Spacer => Self {
                kind: NodeKind::Spacer,
                qml_module: "QtQuick",
                qml_type: "Item",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::None,
                allowed_parents: &[
                    NodeKind::Container,
                    NodeKind::Card,
                    NodeKind::Form,
                    NodeKind::List,
                    NodeKind::Table,
                ],
            },

            // ── Text & typography ───────────────────────────────────────
            NodeKind::Text => Self {
                kind: NodeKind::Text,
                qml_module: "QtQuick.Controls",
                qml_type: "Label",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::None,
                allowed_parents: &[
                    NodeKind::Container,
                    NodeKind::Card,
                    NodeKind::Form,
                    NodeKind::List,
                    NodeKind::Table,
                ],
            },
            NodeKind::Heading => Self {
                kind: NodeKind::Heading,
                qml_module: "QtQuick.Controls",
                qml_type: "Label",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::None,
                allowed_parents: &[NodeKind::Container, NodeKind::Card, NodeKind::Form],
            },
            NodeKind::InlineCode => Self {
                kind: NodeKind::InlineCode,
                qml_module: "QtQuick.Controls",
                qml_type: "Label",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::None,
                allowed_parents: &[NodeKind::Container, NodeKind::Card, NodeKind::Text],
            },
            NodeKind::CodeBlock => Self {
                kind: NodeKind::CodeBlock,
                qml_module: "QtQuick.Controls",
                qml_type: "TextArea",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::None,
                allowed_parents: &[NodeKind::Container, NodeKind::Card, NodeKind::Form],
            },

            // ── Grouping / structure ────────────────────────────────────
            NodeKind::Card => Self {
                kind: NodeKind::Card,
                qml_module: "QtQuick.Controls",
                qml_type: "Frame",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::None,
                allowed_parents: &[
                    NodeKind::Container,
                    NodeKind::Form,
                    NodeKind::List,
                    NodeKind::Table,
                ],
            },
            NodeKind::List => Self {
                kind: NodeKind::List,
                qml_module: "QtQuick.Controls",
                qml_type: "ListView",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::None,
                allowed_parents: &[NodeKind::Container, NodeKind::Card, NodeKind::Form],
            },
            NodeKind::Table => Self {
                kind: NodeKind::Table,
                qml_module: "QtQuick.Controls",
                qml_type: "TableView",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::None,
                allowed_parents: &[NodeKind::Container, NodeKind::Card, NodeKind::Form],
            },
            NodeKind::Form => Self {
                kind: NodeKind::Form,
                qml_module: "QtQuick.Controls",
                qml_type: "Pane",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::None,
                allowed_parents: &[NodeKind::Container, NodeKind::Card],
            },

            // ── Interactive ─────────────────────────────────────────────
            NodeKind::ActionButton => Self {
                kind: NodeKind::ActionButton,
                qml_module: "QtQuick.Controls",
                qml_type: "Button",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::None,
                allowed_parents: &[NodeKind::Container, NodeKind::Card, NodeKind::Form],
            },

            // ── GPU-bearing ─────────────────────────────────────────────
            NodeKind::Visualization => Self {
                kind: NodeKind::Visualization,
                qml_module: "QtQuick",
                qml_type: "QQuickItem",
                requires_gpu: true,
                surface_kind: NodeSurfaceKind::None,
                allowed_parents: &[],
            },
            NodeKind::Stream => Self {
                kind: NodeKind::Stream,
                qml_module: "QtQuick",
                qml_type: "QQuickItem",
                requires_gpu: true,
                surface_kind: NodeSurfaceKind::StreamSurface,
                allowed_parents: &[],
            },
            NodeKind::SurfaceEmbed => Self {
                kind: NodeKind::SurfaceEmbed,
                qml_module: "QtQuick",
                qml_type: "QQuickItem",
                requires_gpu: true,
                surface_kind: NodeSurfaceKind::AppSurface,
                allowed_parents: &[],
            },

            // ── AIOS chrome (constitutional) ────────────────────────────
            NodeKind::SecurityIndicator => Self {
                kind: NodeKind::SecurityIndicator,
                qml_module: "AIOSPrimitives",
                qml_type: "AIOSSecurityIndicator",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::AiosSurface,
                allowed_parents: &[],
            },
            NodeKind::ApprovalPrompt => Self {
                kind: NodeKind::ApprovalPrompt,
                qml_module: "AIOSPrimitives",
                qml_type: "AIOSApprovalDialog",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::AiosSurface,
                allowed_parents: &[],
            },
            NodeKind::EvidenceLink => Self {
                kind: NodeKind::EvidenceLink,
                qml_module: "AIOSPrimitives",
                qml_type: "AIOSEvidenceLink",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::AiosSurface,
                allowed_parents: &[],
            },
            NodeKind::AgentMessage => Self {
                kind: NodeKind::AgentMessage,
                qml_module: "AIOSPrimitives",
                qml_type: "AIOSAgentMessage",
                requires_gpu: false,
                surface_kind: NodeSurfaceKind::AiosSurface,
                allowed_parents: &[],
            },
        }
    }
}

// ── CompilationContext ──────────────────────────────────────────────────────

/// Runtime context that gates compilation-rule resolution.
///
/// Two invariants are enforced at compile-request time:
/// * **INV I5** — when `recovery_active` is `true`, only kinds whose surface
///   kind is `AiosSurface` are permitted.
/// * **INV I7** — when `renderer_mode` is `Degraded(_)`, any kind with
///   `requires_gpu = true` is rejected.
pub struct CompilationContext {
    /// Current renderer operational mode.
    pub renderer_mode: RendererMode,
    /// Whether the recovery shell is active (separate `KWin` session).
    pub recovery_active: bool,
}

impl CompilationContext {
    /// Resolve the compilation rule for `kind`, enforcing INV I5 and INV I7.
    ///
    /// # Errors
    ///
    /// * `KdeRendererError::Internal("recovery shell: AIOS_SURFACE only")` —
    ///   `recovery_active` is `true` and `kind`'s surface kind is not
    ///   `AiosSurface`.
    /// * `KdeRendererError::Degraded(_)` — `renderer_mode` is `Degraded(_)` and
    ///   `kind` is GPU-bearing.
    pub fn compile(&self, kind: NodeKind) -> Result<CompilationRule, KdeRendererError> {
        let rule = CompilationRule::for_node(kind);

        // INV I5 — recovery shell admits AiosSurface kinds only.
        if self.recovery_active && !matches!(rule.surface_kind, NodeSurfaceKind::AiosSurface) {
            return Err(KdeRendererError::Internal(
                "recovery shell: AIOS_SURFACE only".into(),
            ));
        }

        // INV I7 — degraded mode rejects GPU-bearing kinds.
        if let RendererMode::Degraded(_) = &self.renderer_mode {
            if rule.requires_gpu {
                return Err(KdeRendererError::Degraded(
                    "gpu-bearing kind disallowed".into(),
                ));
            }
        }

        Ok(rule)
    }
}

// ── Compile-time sanity check ───────────────────────────────────────────────

/// Verify that `CompilationRule::for_node` is evaluable at compile time.
///
/// This `const` item forces the compiler to prove that `for_node` is a
/// const-evaluable function for every `NodeKind` variant.
#[doc(hidden)]
pub const COMPILATION_RULE_CONST_CHECK: [CompilationRule; NodeKind::LEN] = {
    let mut i = 0;
    let mut rules = [CompilationRule::for_node(NodeKind::Container); NodeKind::LEN];
    while i < NodeKind::ALL.len() {
        rules[i] = CompilationRule::for_node(NodeKind::ALL[i]);
        i += 1;
    }
    rules
};
