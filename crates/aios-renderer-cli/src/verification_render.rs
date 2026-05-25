//! Cross-crate renderers for S2.4 verification intents and results.
//!
//! `VerificationResult` table output is intentionally a single summary row.
//! Per-primitive rows are available by rendering [`PrimitiveResult`] directly,
//! while result text/tree output expands the full primitive list.

use serde::Serialize;

use aios_verification::{
    PrimitiveResult, VerificationIntent, VerificationPrimitive, VerificationResult,
    VerificationStatus,
};

use crate::{
    OutputFormat, RenderContext, RenderError, Renderable, TableAlign, TableRenderer, TableSpec,
    TextRenderer, TreeNode, TreeRenderer,
};

const HASH_TRUNCATE_AT: usize = 12;

/// Renderable view over the supported verification primitive vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerificationPrimitiveList {
    primitives: Vec<VerificationPrimitive>,
}

impl VerificationPrimitiveList {
    /// Builds a primitive-list view.
    #[must_use]
    pub const fn new(primitives: Vec<VerificationPrimitive>) -> Self {
        Self { primitives }
    }

    /// Read-only access to the primitive list.
    #[must_use]
    pub fn primitives(&self) -> &[VerificationPrimitive] {
        &self.primitives
    }

    /// Consumes the view and returns the primitive list.
    #[must_use]
    pub fn into_primitives(self) -> Vec<VerificationPrimitive> {
        self.primitives
    }
}

impl From<Vec<VerificationPrimitive>> for VerificationPrimitiveList {
    fn from(primitives: Vec<VerificationPrimitive>) -> Self {
        Self::new(primitives)
    }
}

impl Renderable for VerificationIntent {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_intent_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_intent_tree(self, ctx),
            OutputFormat::Table => render_intent_table(self, ctx),
        }
    }
}

impl Renderable for VerificationResult {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_result_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_result_tree(self, ctx),
            OutputFormat::Table => render_result_table(self, ctx),
        }
    }
}

impl Renderable for VerificationStatus {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let renderer = TextRenderer::new(ctx.clone());
                Ok(renderer.render_kv("status", &styled_status(*self, ctx)))
            }
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: "VerificationStatus".to_owned(),
                    children: vec![leaf(format!("status: {}", styled_status(*self, ctx)))],
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec!["status".to_owned()],
                    rows: vec![vec![self.as_wire_str().to_owned()]],
                    align: vec![TableAlign::Left],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

impl Renderable for PrimitiveResult {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_primitive_result_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_primitive_result_tree(self, ctx),
            OutputFormat::Table => render_primitive_result_table(self, ctx),
        }
    }
}

impl Renderable for VerificationPrimitive {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let renderer = TextRenderer::new(ctx.clone());
                Ok(renderer.render_kv("primitive", self.as_wire_str()))
            }
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: "VerificationPrimitive".to_owned(),
                    children: vec![leaf(self.as_wire_str())],
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec!["primitive".to_owned()],
                    rows: vec![vec![self.as_wire_str().to_owned()]],
                    align: vec![TableAlign::Left],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

impl Renderable for VerificationPrimitiveList {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_primitive_list_text(self, ctx)),
            OutputFormat::Json => render_json(&self.primitives),
            OutputFormat::Tree => render_primitive_list_tree(self, ctx),
            OutputFormat::Table => render_primitive_list_table(self, ctx),
        }
    }
}

fn render_intent_text(intent: &VerificationIntent, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("intent_id", intent.intent_id.as_str()),
        renderer.render_kv("action_id", intent.action_id.as_str()),
        renderer.render_kv("expression_hash", &truncate_hash(&intent.expression_hash)),
        renderer.render_kv("timeout_seconds", &intent.timeout_seconds.to_string()),
    ];

    renderer.render_section("VerificationIntent", &lines)
}

fn render_intent_tree(
    intent: &VerificationIntent,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("VerificationIntent {}", intent.intent_id.as_str()),
        children: vec![
            leaf(format!("action_id: {}", intent.action_id.as_str())),
            leaf(format!(
                "expression_hash: {}",
                truncate_hash(&intent.expression_hash)
            )),
            leaf(format!("timeout_seconds: {}", intent.timeout_seconds)),
        ],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_intent_table(
    intent: &VerificationIntent,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "intent_id".to_owned(),
            "action_id".to_owned(),
            "expression_hash".to_owned(),
            "timeout_seconds".to_owned(),
        ],
        rows: vec![vec![
            intent.intent_id.as_str().to_owned(),
            intent.action_id.as_str().to_owned(),
            truncate_hash(&intent.expression_hash),
            intent.timeout_seconds.to_string(),
        ]],
        align: vec![
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Right,
        ],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
}

fn render_result_text(result: &VerificationResult, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let mut lines = vec![
        format!("STATUS: {}", styled_status(result.status, ctx)),
        renderer.render_kv("result_id", &result.result_id),
        renderer.render_kv("intent_id", result.intent_id.as_str()),
        renderer.render_kv("action_id", result.action_id.as_str()),
        renderer.render_kv("duration_ms", &result.duration_ms.to_string()),
        renderer.render_kv(
            "per_primitive",
            &format!("count={}", result.per_primitive.len()),
        ),
    ];

    lines.extend(
        result
            .per_primitive
            .iter()
            .map(|primitive| format!("  {}", primitive_summary(primitive, ctx))),
    );

    renderer.render_section("VerificationResult", &lines)
}

fn render_result_tree(
    result: &VerificationResult,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("VerificationResult {}", result.result_id),
        children: vec![
            leaf(format!("status: {}", styled_status(result.status, ctx))),
            leaf(format!("intent_id: {}", result.intent_id.as_str())),
            leaf(format!("duration_ms: {}", result.duration_ms)),
            TreeNode {
                label: format!("per_primitive: count={}", result.per_primitive.len()),
                children: result
                    .per_primitive
                    .iter()
                    .map(|primitive| leaf(primitive_leaf_label(primitive)))
                    .collect(),
            },
        ],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_result_table(
    result: &VerificationResult,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "result_id".to_owned(),
            "intent_id".to_owned(),
            "status".to_owned(),
            "primitives".to_owned(),
            "duration_ms".to_owned(),
        ],
        rows: vec![vec![
            result.result_id.clone(),
            result.intent_id.as_str().to_owned(),
            result.status.as_wire_str().to_owned(),
            result.per_primitive.len().to_string(),
            result.duration_ms.to_string(),
        ]],
        align: vec![
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Right,
            TableAlign::Right,
        ],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
}

fn render_primitive_result_text(result: &PrimitiveResult, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        format!(
            "{} {}",
            primitive_marker(result.passed, ctx),
            result.primitive_kind.as_wire_str()
        ),
        renderer.render_kv("passed", &result.passed.to_string()),
        renderer.render_kv("elapsed_ms", &result.elapsed_ms.to_string()),
        renderer.render_kv("error", result.error.as_deref().unwrap_or("-")),
    ];

    renderer.render_section("PrimitiveResult", &lines)
}

fn render_primitive_result_tree(
    result: &PrimitiveResult,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("PrimitiveResult {}", result.primitive_kind.as_wire_str()),
        children: vec![
            leaf(format!("passed: {}", result.passed)),
            leaf(format!("elapsed_ms: {}", result.elapsed_ms)),
            leaf(format!("error: {}", result.error.as_deref().unwrap_or("-"))),
        ],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_primitive_result_table(
    result: &PrimitiveResult,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "primitive".to_owned(),
            "passed".to_owned(),
            "elapsed_ms".to_owned(),
            "error".to_owned(),
        ],
        rows: vec![vec![
            result.primitive_kind.as_wire_str().to_owned(),
            result.passed.to_string(),
            result.elapsed_ms.to_string(),
            result.error.as_deref().unwrap_or("-").to_owned(),
        ]],
        align: vec![
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Right,
            TableAlign::Left,
        ],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
}

fn render_primitive_list_text(list: &VerificationPrimitiveList, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let mut lines = vec![renderer.render_kv("primitives", &list.primitives.len().to_string())];
    lines.extend(
        list.primitives
            .iter()
            .map(|primitive| primitive.as_wire_str().to_owned()),
    );

    renderer.render_section("VerificationPrimitives", &lines)
}

fn render_primitive_list_tree(
    list: &VerificationPrimitiveList,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("VerificationPrimitives count={}", list.primitives.len()),
        children: list
            .primitives
            .iter()
            .map(|primitive| leaf(primitive.as_wire_str()))
            .collect(),
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_primitive_list_table(
    list: &VerificationPrimitiveList,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec!["index".to_owned(), "primitive".to_owned()],
        rows: list
            .primitives
            .iter()
            .enumerate()
            .map(|(index, primitive)| vec![index.to_string(), primitive.as_wire_str().to_owned()])
            .collect(),
        align: vec![TableAlign::Right, TableAlign::Left],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
}

fn primitive_summary(result: &PrimitiveResult, ctx: &RenderContext) -> String {
    format!(
        "{} {} passed={} elapsed_ms={} error={}",
        primitive_marker(result.passed, ctx),
        result.primitive_kind.as_wire_str(),
        result.passed,
        result.elapsed_ms,
        result.error.as_deref().unwrap_or("-")
    )
}

fn primitive_leaf_label(result: &PrimitiveResult) -> String {
    format!(
        "{} passed={} elapsed_ms={} error={}",
        result.primitive_kind.as_wire_str(),
        result.passed,
        result.elapsed_ms,
        result.error.as_deref().unwrap_or("-")
    )
}

fn primitive_marker(passed: bool, ctx: &RenderContext) -> String {
    let marker = if passed { "✓" } else { "X" };
    if !ctx.color {
        return marker.to_owned();
    }

    let color = if passed { "32" } else { "31" };
    format!("\u{1b}[{color}m{marker}\u{1b}[0m")
}

fn styled_status(status: VerificationStatus, ctx: &RenderContext) -> String {
    let label = status.as_wire_str();
    if !ctx.color {
        return label.to_owned();
    }

    format!("\u{1b}[{}m{label}\u{1b}[0m", status_color(status))
}

const fn status_color(status: VerificationStatus) -> &'static str {
    match status {
        VerificationStatus::Passed => "32",
        VerificationStatus::Failed => "31",
        VerificationStatus::Timeout => "33",
        VerificationStatus::ProbeError => "35",
        VerificationStatus::Skipped => "90",
    }
}

fn truncate_hash(hash: &str) -> String {
    if hash.chars().count() <= HASH_TRUNCATE_AT {
        return hash.to_owned();
    }

    let head = hash.chars().take(HASH_TRUNCATE_AT).collect::<String>();
    format!("{head}...")
}

fn render_json<T: Serialize>(value: &T) -> Result<String, RenderError> {
    serde_json::to_string(value).map_err(|err| RenderError::SerializationFailed(err.to_string()))
}

fn leaf(label: impl Into<String>) -> TreeNode {
    TreeNode {
        label: label.into(),
        children: Vec::new(),
    }
}
