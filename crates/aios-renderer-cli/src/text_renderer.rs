//! Human-readable text renderer helpers.

use crate::{OutputFormat, RenderContext, RenderError, Renderable};

/// Text renderer for S7.6 human-readable terminal output.
#[derive(Debug, Clone)]
pub struct TextRenderer {
    ctx: RenderContext,
}

impl TextRenderer {
    /// Builds a text renderer with the supplied rendering context.
    #[must_use]
    pub const fn new(ctx: RenderContext) -> Self {
        Self { ctx }
    }

    /// Renders any [`Renderable`] value in [`OutputFormat::Text`].
    ///
    /// # Errors
    ///
    /// Returns the underlying [`RenderError`] from the value-specific renderer.
    pub fn render<R: Renderable>(&self, value: &R) -> Result<String, RenderError> {
        value.render(OutputFormat::Text, &self.ctx)
    }

    /// Renders a single text key-value line.
    #[must_use]
    pub fn render_kv(&self, key: &str, value: &str) -> String {
        let rendered_value = if self.ctx.redact_secrets && is_secret_key(key) {
            "<redacted>"
        } else {
            value
        };

        format!("{key}: {rendered_value}")
    }

    /// Renders a titled section followed by zero or more body lines.
    #[must_use]
    pub fn render_section(&self, title: &str, lines: &[String]) -> String {
        let title = if self.ctx.color {
            format!("\u{1b}[1m{title}\u{1b}[0m")
        } else {
            title.to_owned()
        };

        if lines.is_empty() {
            format!("{title}\n")
        } else {
            format!("{title}\n{}", lines.join("\n"))
        }
    }

    /// Renders an unordered bullet list.
    #[must_use]
    pub fn render_list(&self, items: &[String]) -> String {
        let bullet = if self.ctx.color && locale_supports_utf8(&self.ctx.locale) {
            "• "
        } else {
            "* "
        };

        items
            .iter()
            .map(|item| format!("{bullet}{item}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Applies a small ANSI state color mapping when color is enabled.
    #[must_use]
    pub fn color_for_state(&self, state: &str) -> String {
        if !self.ctx.color {
            return state.to_owned();
        }

        let color = match state.to_ascii_lowercase().as_str() {
            "active" | "ok" | "secure" | "success" | "succeeded" => "32",
            "degraded" | "pending" | "warning" => "33",
            "denied" | "destructive" | "error" | "failed" | "failure" => "31",
            _ => "36",
        };

        format!("\u{1b}[{color}m{state}\u{1b}[0m")
    }
}

fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    ["key_material", "secret", "password", "token"]
        .iter()
        .any(|needle| key.contains(needle))
}

fn locale_supports_utf8(locale: &str) -> bool {
    let locale = locale.to_ascii_lowercase();
    locale.contains("utf-8") || locale.contains("utf8")
}
