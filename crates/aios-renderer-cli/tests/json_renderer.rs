//! Tests for `JSON` renderer serialization and redaction.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::collections::BTreeMap;

use serde::Serialize;

use aios_renderer_cli::{JsonRenderer, OutputFormat, RenderContext, RenderError, Renderable};

fn ctx(color: bool, redact_secrets: bool) -> RenderContext {
    RenderContext {
        color,
        width: Some(80),
        redact_secrets,
        verbose: false,
        locale: "en_US.UTF-8".to_owned(),
    }
}

#[derive(Debug, Clone, Serialize)]
struct Probe {
    label: String,
    key_material: String,
    nested: NestedProbe,
}

#[derive(Debug, Clone, Serialize)]
struct NestedProbe {
    api_token: String,
    public_id: String,
}

impl Renderable for Probe {
    fn render(&self, format: OutputFormat, _ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
            OutputFormat::Text | OutputFormat::Tree | OutputFormat::Table => Ok(self.label.clone()),
        }
    }
}

impl Renderable for NestedProbe {
    fn render(&self, format: OutputFormat, _ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
            OutputFormat::Text | OutputFormat::Tree | OutputFormat::Table => {
                Ok(self.public_id.clone())
            }
        }
    }
}

#[test]
fn primitive_serialize_uses_json_format() {
    let renderer = JsonRenderer::new(ctx(false, true));

    assert_eq!(renderer.render(&42_i64).expect("render i64"), "42");
}

#[test]
fn new_pretty_emits_multiline_json() {
    let renderer = JsonRenderer::new_pretty(ctx(false, true));
    let mut value = BTreeMap::new();
    value.insert("count".to_owned(), 2_u64);

    let rendered = renderer.render(&value).expect("render pretty map");

    assert_eq!(rendered, "{\n  \"count\": 2\n}");
}

#[test]
fn new_with_plain_context_emits_compact_json() {
    let renderer = JsonRenderer::new(ctx(false, true));
    let mut value = BTreeMap::new();
    value.insert("count".to_owned(), 2_u64);

    let rendered = renderer.render(&value).expect("render compact map");

    assert_eq!(rendered, "{\"count\":2}");
}

#[test]
fn redact_secrets_replaces_key_material_field() {
    let renderer = JsonRenderer::new(ctx(false, true));
    let value = probe();

    let rendered = renderer.render(&value).expect("render redacted probe");

    assert!(rendered.contains("\"key_material\":\"<redacted>\""));
    assert!(!rendered.contains("private-key-bytes"));
}

#[test]
fn redact_secrets_walks_nested_token_fields() {
    let renderer = JsonRenderer::new(ctx(false, true));
    let value = probe();

    let rendered = renderer.render(&value).expect("render redacted probe");

    assert!(rendered.contains("\"api_token\":\"<redacted>\""));
    assert!(!rendered.contains("token-value"));
}

#[test]
fn redact_secrets_does_not_redact_non_secret_fields() {
    let renderer = JsonRenderer::new(ctx(false, true));
    let value = probe();

    let rendered = renderer.render(&value).expect("render redacted probe");

    assert!(rendered.contains("\"label\":\"visible\""));
    assert!(rendered.contains("\"public_id\":\"pub-1\""));
}

#[test]
fn redact_secrets_false_preserves_secret_shaped_fields() {
    let renderer = JsonRenderer::new(ctx(false, false));
    let value = probe();

    let rendered = renderer.render(&value).expect("render unredacted probe");

    assert!(rendered.contains("private-key-bytes"));
    assert!(rendered.contains("token-value"));
}

#[test]
fn render_api_requires_renderable_and_serde_serialize_bounds() {
    fn render_with_required_bounds<T>(
        renderer: &JsonRenderer,
        value: &T,
    ) -> Result<String, RenderError>
    where
        T: Renderable + Serialize,
    {
        renderer.render(value)
    }

    let renderer = JsonRenderer::new(ctx(false, true));
    let value = probe();

    assert!(render_with_required_bounds(&renderer, &value)
        .expect("render through bounded helper")
        .contains("\"label\":\"visible\""));
}

fn probe() -> Probe {
    Probe {
        label: "visible".to_owned(),
        key_material: "private-key-bytes".to_owned(),
        nested: NestedProbe {
            api_token: "token-value".to_owned(),
            public_id: "pub-1".to_owned(),
        },
    }
}
