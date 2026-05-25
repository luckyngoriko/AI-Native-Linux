//! Cross-crate renderers for evidence receipts and receipt chains.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use aios_evidence::{EvidenceReceipt, RecordType};

use crate::{
    OutputFormat, RenderContext, RenderError, Renderable, TableAlign, TableRenderer, TableSpec,
    TextRenderer, TreeNode, TreeRenderer,
};

const HASH_TRUNCATE_AT: usize = 12;

/// Renderable view over an ordered evidence receipt chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceChainView {
    receipts: Vec<EvidenceReceipt>,
}

impl EvidenceChainView {
    /// Builds a renderable evidence-chain view from ordered receipts.
    #[must_use]
    pub const fn new(receipts: Vec<EvidenceReceipt>) -> Self {
        Self { receipts }
    }

    /// Read-only access to the ordered receipts.
    #[must_use]
    pub fn receipts(&self) -> &[EvidenceReceipt] {
        &self.receipts
    }

    /// Consumes the view and returns the underlying receipts.
    #[must_use]
    pub fn into_receipts(self) -> Vec<EvidenceReceipt> {
        self.receipts
    }
}

impl From<Vec<EvidenceReceipt>> for EvidenceChainView {
    fn from(receipts: Vec<EvidenceReceipt>) -> Self {
        Self::new(receipts)
    }
}

impl Renderable for EvidenceReceipt {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_receipt_text(self, ctx)),
            OutputFormat::Json => render_receipt_json(self, ctx),
            OutputFormat::Tree => render_receipt_tree(self, ctx),
            OutputFormat::Table => render_receipt_table(self, ctx),
        }
    }
}

impl Renderable for RecordType {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let renderer = TextRenderer::new(ctx.clone());
                Ok(renderer.render_kv("record_type", self.as_wire_str()))
            }
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: "record_type".to_owned(),
                    children: vec![leaf(self.as_wire_str())],
                };

                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec!["record_type".to_owned()],
                    rows: vec![vec![self.as_wire_str().to_owned()]],
                    align: vec![TableAlign::Left],
                };

                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

impl Renderable for EvidenceChainView {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_chain_text(self, ctx)),
            OutputFormat::Json => render_chain_json(self, ctx),
            OutputFormat::Tree => render_chain_tree(self, ctx),
            OutputFormat::Table => render_chain_table(&self.receipts, ctx),
        }
    }
}

fn render_receipt_text(receipt: &EvidenceReceipt, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let mut lines = receipt_summary_lines(receipt, &renderer);

    lines.push(renderer.render_kv(
        "previous_receipt_hash",
        receipt.previous_receipt_hash().unwrap_or("<genesis>"),
    ));
    lines.push(renderer.render_kv("content_hash", receipt.content_hash()));
    lines.push(renderer.render_kv("payload", &payload_display(receipt, ctx)));
    lines.push(renderer.render_kv("signature", &signature_display(receipt, ctx)));

    renderer.render_section("EvidenceReceipt", &lines)
}

fn render_receipt_json(
    receipt: &EvidenceReceipt,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let value = receipt_json_value(receipt, ctx)?;
    serde_json::to_string(&value).map_err(|err| RenderError::SerializationFailed(err.to_string()))
}

fn render_receipt_tree(
    receipt: &EvidenceReceipt,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = receipt_tree_node(receipt, ctx);
    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_receipt_table(
    receipt: &EvidenceReceipt,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    render_chain_table(std::slice::from_ref(receipt), ctx)
}

fn render_chain_text(view: &EvidenceChainView, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());

    if view.receipts.is_empty() {
        return renderer.render_section(
            "EvidenceChain",
            &[renderer.render_kv("receipts", "(no receipts)")],
        );
    }

    let mut lines = vec![renderer.render_kv("receipts", &view.receipts.len().to_string())];

    for (index, receipt) in view.receipts.iter().enumerate() {
        lines.push(format!(
            "{} {} {} {}",
            receipt.receipt_id().as_str(),
            receipt.record_type().as_wire_str(),
            receipt.recorded_at().to_rfc3339(),
            receipt.subject_canonical_id()
        ));
        lines.push(format!("  {}", chain_link_line(&view.receipts, index, ctx)));
    }

    renderer.render_section("EvidenceChain", &lines)
}

fn render_chain_json(view: &EvidenceChainView, ctx: &RenderContext) -> Result<String, RenderError> {
    if view.receipts.is_empty() {
        let mut object = Map::new();
        object.insert("receipts".to_owned(), Value::Array(Vec::new()));
        object.insert(
            "summary".to_owned(),
            Value::String("(no receipts)".to_owned()),
        );
        return serde_json::to_string(&Value::Object(object))
            .map_err(|err| RenderError::SerializationFailed(err.to_string()));
    }

    let receipts = view
        .receipts
        .iter()
        .map(|receipt| receipt_json_value(receipt, ctx))
        .collect::<Result<Vec<_>, _>>()?;
    let mut object = Map::new();
    object.insert("receipts".to_owned(), Value::Array(receipts));

    serde_json::to_string(&Value::Object(object))
        .map_err(|err| RenderError::SerializationFailed(err.to_string()))
}

fn render_chain_tree(view: &EvidenceChainView, ctx: &RenderContext) -> Result<String, RenderError> {
    let children = if view.receipts.is_empty() {
        vec![leaf("(no receipts)")]
    } else {
        view.receipts
            .iter()
            .enumerate()
            .map(|(index, receipt)| {
                let mut node = receipt_tree_node(receipt, ctx);
                node.children
                    .push(leaf(chain_link_line(&view.receipts, index, ctx)));
                node
            })
            .collect()
    };

    let root = TreeNode {
        label: "evidence_chain".to_owned(),
        children,
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_chain_table(
    receipts: &[EvidenceReceipt],
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    if receipts.is_empty() {
        let spec = TableSpec {
            headers: vec!["receipt_id".to_owned()],
            rows: vec![vec!["(no receipts)".to_owned()]],
            align: vec![TableAlign::Left],
        };
        return TableRenderer::new(ctx.clone()).render(&spec);
    }

    let spec = TableSpec {
        headers: vec![
            "receipt_id".to_owned(),
            "type".to_owned(),
            "timestamp".to_owned(),
            "subject".to_owned(),
        ],
        rows: receipts
            .iter()
            .map(|receipt| {
                vec![
                    receipt.receipt_id().as_str().to_owned(),
                    receipt.record_type().as_wire_str().to_owned(),
                    receipt.recorded_at().to_rfc3339(),
                    receipt.subject_canonical_id().to_owned(),
                ]
            })
            .collect(),
        align: vec![
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
        ],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
}

fn receipt_tree_node(receipt: &EvidenceReceipt, ctx: &RenderContext) -> TreeNode {
    TreeNode {
        label: format!("receipt {}", receipt.receipt_id().as_str()),
        children: vec![
            leaf(format!(
                "record_type: {}",
                receipt.record_type().as_wire_str()
            )),
            leaf(format!("timestamp: {}", receipt.recorded_at().to_rfc3339())),
            leaf(format!("subject: {}", receipt.subject_canonical_id())),
            leaf(format!(
                "previous_receipt_hash: {}",
                receipt.previous_receipt_hash().unwrap_or("<genesis>")
            )),
            leaf(format!("content_hash: {}", receipt.content_hash())),
            leaf(format!("payload: {}", payload_display(receipt, ctx))),
            leaf(format!("signature: {}", signature_display(receipt, ctx))),
        ],
    }
}

fn receipt_summary_lines(receipt: &EvidenceReceipt, renderer: &TextRenderer) -> Vec<String> {
    vec![
        renderer.render_kv("receipt_id", receipt.receipt_id().as_str()),
        renderer.render_kv("record_type", receipt.record_type().as_wire_str()),
        renderer.render_kv("timestamp", &receipt.recorded_at().to_rfc3339()),
        renderer.render_kv("subject", receipt.subject_canonical_id()),
    ]
}

fn receipt_json_value(
    receipt: &EvidenceReceipt,
    ctx: &RenderContext,
) -> Result<Value, RenderError> {
    let mut value = serde_json::to_value(receipt)
        .map_err(|err| RenderError::SerializationFailed(err.to_string()))?;
    scrub_key_material(&mut value);

    if ctx.redact_secrets {
        let payload_marker = payload_byte_marker(receipt)?;
        let signature_marker = receipt.signature().map_or_else(
            || Value::String("<none>".to_owned()),
            |signature| Value::String(byte_marker(signature_hex_bytes(signature))),
        );

        let object = value.as_object_mut().ok_or_else(|| {
            RenderError::Internal("evidence receipt JSON did not serialize as object".to_owned())
        })?;
        object.insert("payload".to_owned(), Value::String(payload_marker));
        object.insert("signature".to_owned(), signature_marker);
    }

    Ok(value)
}

fn payload_display(receipt: &EvidenceReceipt, ctx: &RenderContext) -> String {
    if ctx.redact_secrets {
        return payload_byte_marker(receipt).unwrap_or_else(|_| "<payload bytes>".to_owned());
    }

    let payload = sanitized_payload(receipt);
    let payload_bytes =
        aios_action::jcs_canonicalize(&payload).unwrap_or_else(|_| payload.to_string());
    format!("0x{}", hex_encode(payload_bytes.as_bytes()))
}

fn payload_byte_marker(receipt: &EvidenceReceipt) -> Result<String, RenderError> {
    let payload = sanitized_payload(receipt);
    let payload_bytes = aios_action::jcs_canonicalize(&payload)
        .map_err(|err| RenderError::SerializationFailed(err.to_string()))?;
    Ok(byte_marker(payload_bytes.len()))
}

fn signature_display(receipt: &EvidenceReceipt, ctx: &RenderContext) -> String {
    match receipt.signature() {
        Some(signature) if ctx.redact_secrets => byte_marker(signature_hex_bytes(signature)),
        Some(signature) => signature.to_owned(),
        None => "<none>".to_owned(),
    }
}

fn chain_link_line(receipts: &[EvidenceReceipt], index: usize, ctx: &RenderContext) -> String {
    let Some(receipt) = receipts.get(index) else {
        return "prev_receipt_hash: <missing receipt>".to_owned();
    };

    if index == 0 {
        return receipt.previous_receipt_hash().map_or_else(
            || "prev_receipt_hash: <genesis>".to_owned(),
            |actual| {
                let label = format!("→ {}", truncate_hash(actual));
                format!(
                    "prev_receipt_hash: {}",
                    color_link_label(&label, false, ctx)
                )
            },
        );
    }

    let actual = receipt.previous_receipt_hash();
    let expected = receipts
        .get(index.saturating_sub(1))
        .and_then(|previous| previous.link_hash().ok());
    let valid = actual
        .zip(expected.as_deref())
        .is_some_and(|(actual, expected)| actual == expected);
    let label = actual.map_or_else(
        || "→ <missing>".to_owned(),
        |actual| format!("→ {}", truncate_hash(actual)),
    );

    format!(
        "prev_receipt_hash: {}",
        color_link_label(&label, valid, ctx)
    )
}

fn color_link_label(label: &str, valid: bool, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let state = if valid { "ok" } else { "error" };
    let colored_state = renderer.color_for_state(state);

    if colored_state == state {
        label.to_owned()
    } else {
        colored_state.replace(state, label)
    }
}

fn truncate_hash(hash: &str) -> String {
    hash.chars().take(HASH_TRUNCATE_AT).collect()
}

fn sanitized_payload(receipt: &EvidenceReceipt) -> Value {
    let mut payload = receipt.payload().clone();
    scrub_key_material(&mut payload);
    payload
}

fn scrub_key_material(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                if key.eq_ignore_ascii_case("key_material") {
                    *nested = Value::String("<redacted>".to_owned());
                } else {
                    scrub_key_material(nested);
                }
            }
        }
        Value::Array(values) => {
            for nested in values {
                scrub_key_material(nested);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn signature_hex_bytes(signature: &str) -> usize {
    if signature.len().is_multiple_of(2) && signature.chars().all(|c| c.is_ascii_hexdigit()) {
        signature.len() / 2
    } else {
        signature.len()
    }
}

fn byte_marker(byte_count: usize) -> String {
    format!("<{byte_count} bytes>")
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

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }

    out
}
