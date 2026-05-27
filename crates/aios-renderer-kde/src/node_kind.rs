//! Closed `NodeKind` vocabulary and Qt/QML compilation hints (S7.2 §3 + S7.4 §4).
//!
//! The 19 declared `NodeKind` values are taken verbatim from the S7.2 shared UI
//! schema proto enum (§3). `NODE_KIND_UNSPECIFIED` (wire value 0) is the proto
//! sentinel and is not represented here — the renderer rejects unknown wire
//! values at deserialization.

use serde::{Deserialize, Serialize};

/// Closed UI node kind vocabulary — 19 declared values from S7.2 §3.
///
/// Adding a kind is a versioned spec change. Renderers reject trees containing
/// unknown kinds with `KdeUnknownNodeKind` (S7.4 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    /// Grouping container with layout hints; no own payload.
    Container,
    /// Structural separator (horizontal or vertical).
    Divider,
    /// Flexible or fixed gap.
    Spacer,
    /// Paragraph or label text.
    Text,
    /// Semantic heading (level 1..6); accessibility-load-bearing.
    Heading,
    /// Code-styled inline span within text flow.
    InlineCode,
    /// Multi-line code block with optional language hint.
    CodeBlock,
    /// Grouped content card with title, body, and actions.
    Card,
    /// Ordered or unordered list; data source inline or S2.1 view.
    List,
    /// Tabular data; filterable and sortable.
    Table,
    /// Input collection form; submits as S0.1 action envelope.
    Form,
    /// Invokes a typed action when activated.
    ActionButton,
    /// Chart/graph/topology rendered via wgpu inside an `AIOS_SURFACE`.
    Visualization,
    /// Live data feed rendered into a `STREAM_SURFACE`.
    Stream,
    /// Embeds a Surface from S7.1 (typically `APP_SURFACE`).
    SurfaceEmbed,
    /// AIOS chrome element showing subject + action + evidence link.
    /// Constitutional — AI subjects cannot author (S7.2 §I5).
    SecurityIndicator,
    /// Gates an action awaiting operator decision.
    /// Constitutional — AI subjects cannot author (S7.2 §I5).
    ApprovalPrompt,
    /// Clickable link to an S3.1 evidence receipt.
    /// Constitutional — AI subjects cannot author (S7.2 §I5).
    EvidenceLink,
    /// Rich output from an AI agent; always carries `is_ai_origin = true`.
    AgentMessage,
}

/// Compile-time hint mapping a `NodeKind` to its Qt/QML primitive and GPU
/// requirements (S7.4 §4 compilation table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeKindCompilationHint {
    /// The node kind this hint describes.
    pub kind: NodeKind,
    /// The Qt/QML primitive class or component name used to render this kind.
    pub qml_primitive: &'static str,
    /// Whether this kind requires a GPU binding (wgpu/VkDevice).
    pub is_gpu_bearing: bool,
}

impl NodeKind {
    /// Number of declared `NodeKind` variants (excluding UNSPECIFIED sentinel).
    pub const LEN: usize = 19;

    /// Return the compile-time Qt/QML hint for this node kind.
    ///
    /// The mapping follows the S7.4 §4 compilation table. GPU-bearing kinds
    /// (`Visualization`, `Stream`, `SurfaceEmbed`) require a per-group
    /// `VkDevice` obtained from S8.2.
    #[must_use]
    #[allow(
        clippy::match_same_arms,
        reason = "S7.4 §4 compilation table keeps each NodeKind on its own arm for clarity, even when two map to the same Qt primitive"
    )]
    pub const fn compilation_hint(self) -> NodeKindCompilationHint {
        NodeKindCompilationHint {
            kind: self,
            qml_primitive: match self {
                Self::Container => "Item",
                Self::Divider => "Frame",
                Self::Spacer => "Item",
                Self::Text => "Label",
                Self::Heading => "Label",
                Self::InlineCode => "Label",
                Self::CodeBlock => "QPlainTextEdit",
                Self::Card => "QFrame",
                Self::List => "QListView",
                Self::Table => "QTableView",
                Self::Form => "QFormLayout",
                Self::ActionButton => "QPushButton",
                Self::Visualization => "QQuickItem",
                Self::Stream => "GStreamer/dmabuf",
                Self::SurfaceEmbed => "wl_subsurface",
                Self::SecurityIndicator => "AIOSSecurityIndicator",
                Self::ApprovalPrompt => "AIOSApprovalDialog",
                Self::EvidenceLink => "QPushButton",
                Self::AgentMessage => "AIOSAgentMessage",
            },
            is_gpu_bearing: matches!(
                self,
                Self::Visualization | Self::Stream | Self::SurfaceEmbed
            ),
        }
    }
}
