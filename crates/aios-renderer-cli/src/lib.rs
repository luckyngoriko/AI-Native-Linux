//! `aios-renderer-cli` — core types for the S7.6 CLI renderer opening slice.
//!
//! T-056 intentionally stops at typed scaffolding: S7.6 closed enums,
//! [`OutputFormat`], [`RenderContext`], [`Renderable`], [`RenderError`], and
//! primitive rendering helpers. Format-specific renderers, cross-crate
//! renderable implementations, `gRPC`, and the `clap` binary land in later M7
//! tasks.

#![forbid(unsafe_code)]

pub mod cli_types;
pub mod error;
pub mod output_format;
pub mod primitives;
pub mod renderable;

pub use cli_types::{
    AnsiSupportLevel, CliCompilationResult, CliEvidenceRecordKind, CliInputMode, CliRenderMode,
};
pub use error::RenderError;
pub use output_format::OutputFormat;
pub use renderable::{RenderContext, Renderable};
