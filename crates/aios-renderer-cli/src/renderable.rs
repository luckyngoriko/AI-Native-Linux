//! Renderable abstraction and non-interactive context defaults.

use std::env;

use serde::{Deserialize, Serialize};

use crate::{OutputFormat, RenderError};

/// Shared context supplied to every [`Renderable`] implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderContext {
    /// Whether ANSI color/styling is permitted.
    pub color: bool,
    /// Terminal width in columns, when known.
    pub width: Option<u16>,
    /// Whether secret-shaped content must be redacted before display.
    pub redact_secrets: bool,
    /// Whether verbose diagnostic rendering is enabled.
    pub verbose: bool,
    /// Locale identifier used for future formatting decisions.
    pub locale: String,
}

impl RenderContext {
    /// Builds defaults for a human terminal session.
    ///
    /// Detection is intentionally limited to the S7.6 inputs that do not require
    /// extra terminal crates in T-056: `TERM`, `COLUMNS`, `NO_COLOR`, `LC_ALL`,
    /// and `LANG`.
    #[must_use]
    pub fn new_terminal_defaults() -> Self {
        let no_color = env_non_empty("NO_COLOR");
        let term = env::var("TERM").unwrap_or_default();
        let color = !no_color && !term.is_empty() && term != "dumb";

        Self {
            color,
            width: terminal_columns(),
            redact_secrets: true,
            verbose: false,
            locale: locale(),
        }
    }

    /// Builds defaults for stdout/stderr piping into other tools.
    #[must_use]
    pub fn new_pipe_defaults() -> Self {
        Self {
            color: false,
            width: None,
            redact_secrets: true,
            verbose: false,
            locale: locale(),
        }
    }
}

/// Common rendering abstraction implemented by every user-facing AIOS type.
pub trait Renderable {
    /// Renders `self` into the selected output format.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] when the type cannot support the requested format
    /// or when structured serialization fails.
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError>;
}

fn terminal_columns() -> Option<u16> {
    env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|width| *width > 0)
}

fn env_non_empty(key: &str) -> bool {
    env::var_os(key).is_some_and(|value| !value.is_empty())
}

fn locale() -> String {
    env::var("LC_ALL")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| env::var("LANG").ok().filter(|value| !value.is_empty()))
        .unwrap_or_else(|| "C".to_owned())
}
