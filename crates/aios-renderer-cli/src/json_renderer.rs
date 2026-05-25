//! Structured `JSON` renderer helpers.

use std::io::IsTerminal;

use serde::Serialize;
use serde_json::Value;

use crate::{RenderContext, RenderError, Renderable};

/// `JSON` renderer for scripting and machine-readable output.
#[derive(Debug, Clone)]
pub struct JsonRenderer {
    ctx: RenderContext,
    pretty: bool,
}

impl JsonRenderer {
    /// Builds a `JSON` renderer.
    ///
    /// Pretty printing is enabled for colored terminal contexts or when stdout
    /// is attached to a terminal.
    #[must_use]
    pub fn new(ctx: RenderContext) -> Self {
        let pretty = ctx.color || std::io::stdout().is_terminal();

        Self { ctx, pretty }
    }

    /// Builds a `JSON` renderer that always pretty-prints.
    #[must_use]
    pub const fn new_pretty(ctx: RenderContext) -> Self {
        Self { ctx, pretty: true }
    }

    /// Serializes a value as `JSON`, applying secret redaction when requested.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::SerializationFailed`] when `serde_json` cannot
    /// convert or serialize the supplied value.
    pub fn render<R: Renderable + Serialize>(&self, value: &R) -> Result<String, RenderError> {
        let mut value = serde_json::to_value(value)
            .map_err(|err| RenderError::SerializationFailed(err.to_string()))?;

        if self.ctx.redact_secrets {
            redact_secret_fields(&mut value);
        }

        let rendered = if self.pretty {
            serde_json::to_string_pretty(&value)
        } else {
            serde_json::to_string(&value)
        };

        rendered.map_err(|err| RenderError::SerializationFailed(err.to_string()))
    }
}

fn redact_secret_fields(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                if is_secret_key(key) {
                    *nested = Value::String("<redacted>".to_owned());
                } else {
                    redact_secret_fields(nested);
                }
            }
        }
        Value::Array(values) => {
            for nested in values {
                redact_secret_fields(nested);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    ["key_material", "secret", "password", "token"]
        .iter()
        .any(|needle| key.contains(needle))
}
