//! Error taxonomy for the CLI renderer skeleton.

use thiserror::Error;

use crate::OutputFormat;

/// Render failures surfaced by primitive and future format-specific renderers.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RenderError {
    /// CLI flag requested a format the renderer does not know.
    #[error("unknown output format: {0}")]
    UnknownFormat(String),
    /// The target type does not support the requested output format.
    #[error("unsupported render target `{type_name}` for format {format}")]
    Unsupported {
        /// User-facing type name.
        type_name: String,
        /// Requested output format.
        format: OutputFormat,
    },
    /// Serialization failed while rendering structured output.
    #[error("serialization failed: {0}")]
    SerializationFailed(String),
    /// Rendered content cannot fit in the available terminal width.
    #[error("terminal width overflow: needed {needed} columns, available {available}")]
    WidthOverflow {
        /// Required columns.
        needed: u32,
        /// Available terminal columns.
        available: u16,
    },
    /// Internal renderer invariant failed.
    #[error("renderer internal error: {0}")]
    Internal(String),
}
