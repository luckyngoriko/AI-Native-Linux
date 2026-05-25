//! `aios-renderer-cli` — core types for the S7.6 CLI renderer opening slice.
//!
//! T-056 intentionally stops at typed scaffolding: S7.6 closed enums,
//! [`OutputFormat`], [`RenderContext`], [`Renderable`], [`RenderError`], and
//! primitive rendering helpers. T-057 adds the format-specific renderer helpers
//! used by later cross-crate implementations. Cross-crate renderable
//! implementations, `gRPC`, and the `clap` binary land in later M7 tasks.

#![forbid(unsafe_code)]

pub mod cli_types;
pub mod error;
pub mod json_renderer;
pub mod output_format;
pub mod primitives;
pub mod renderable;
pub mod table_renderer;
pub mod text_renderer;
pub mod tree_renderer;

pub use cli_types::{
    AnsiSupportLevel, CliCompilationResult, CliEvidenceRecordKind, CliInputMode, CliRenderMode,
};
pub use error::RenderError;
pub use json_renderer::JsonRenderer;
pub use output_format::OutputFormat;
pub use renderable::{RenderContext, Renderable};
pub use table_renderer::{TableAlign, TableRenderer, TableSpec};
pub use text_renderer::TextRenderer;
pub use tree_renderer::{TreeNode, TreeRenderer};
