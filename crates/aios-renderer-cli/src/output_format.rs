//! Crate-local output format vocabulary for T-056 primitive rendering.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

use crate::RenderError;

/// Output shape selected by early M7 CLI flag parsing.
///
/// S7.6 does not define this enum as a wire contract. The spec's §3 closed
/// enums are represented in [`crate::cli_types`]; this helper keeps the T-056
/// primitive render tests stable until the later `clap` binary task lands.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OutputFormat {
    /// Human-readable plain text.
    #[default]
    Text,
    /// Machine-readable `JSON`.
    Json,
    /// Indented tree output.
    Tree,
    /// Row/column table output.
    Table,
}

impl OutputFormat {
    /// Parses a CLI format flag case-insensitively.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::UnknownFormat`] when `s` does not match one of the
    /// closed T-056 formats.
    #[allow(
        clippy::should_implement_trait,
        reason = "T-056 explicitly requires OutputFormat::from_str; FromStr is also implemented"
    )]
    pub fn from_str(s: &str) -> Result<Self, RenderError> {
        parse_output_format(s)
    }
}

impl FromStr for OutputFormat {
    type Err = RenderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_output_format(s)
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Text => "TEXT",
            Self::Json => "JSON",
            Self::Tree => "TREE",
            Self::Table => "TABLE",
        })
    }
}

fn parse_output_format(s: &str) -> Result<OutputFormat, RenderError> {
    match s.trim().to_ascii_lowercase().as_str() {
        "text" => Ok(OutputFormat::Text),
        "json" => Ok(OutputFormat::Json),
        "tree" => Ok(OutputFormat::Tree),
        "table" => Ok(OutputFormat::Table),
        _ => Err(RenderError::UnknownFormat(s.to_owned())),
    }
}
