//! `aios-renderer-cli` — core types for the S7.6 CLI renderer opening slice.
//!
//! T-056 intentionally stops at typed scaffolding: S7.6 closed enums,
//! [`OutputFormat`], [`RenderContext`], [`Renderable`], [`RenderError`], and
//! primitive rendering helpers. T-057 adds the format-specific renderer helpers
//! used by later cross-crate implementations. Cross-crate renderable
//! implementations, `gRPC`, and the `clap` binary land in later M7 tasks.

#![forbid(unsafe_code)]

pub mod action_render;
pub mod cli;
pub mod cli_types;
pub mod client;
pub mod error;
pub mod evidence_render;
pub mod fs_render;
pub mod json_renderer;
pub mod output_format;
pub mod policy_render;
pub mod primitives;
pub mod recovery_render;
pub mod renderable;
pub mod table_renderer;
pub mod text_renderer;
pub mod tree_renderer;
pub mod vault_render;
pub mod verification_render;

pub use aios_fs::{AiosPath, NamespaceClass, Object, Pointer, Version};
pub use aios_policy::{ApprovalRequirement, Constraints, Decision, PolicyDecision};
pub use aios_recovery::{
    BootId, CandidateId, CandidateState, FirstBootContext, FirstBootPhase, FirstBootStatus,
    KernelCandidate, KernelManifest, RecoveryMode, RecoveryState,
};
pub use aios_vault::{
    CapabilityClass, CapabilityState, KeyMaterialHandle, OverrideBinding, OverrideClass,
    VaultCapability,
};
pub use aios_verification::{
    PrimitiveResult, VerificationIntent, VerificationPrimitive, VerificationResult,
    VerificationStatus,
};
pub use cli::{
    ActionSubcommand, AiosCli, AiosCommand, EvidenceSubcommand, FsSubcommand, KernelSubcommand,
    PolicySubcommand, RecoverySubcommand, VaultSubcommand, VerificationSubcommand,
};
pub use cli_types::{
    AnsiSupportLevel, CliCompilationResult, CliEvidenceRecordKind, CliInputMode, CliRenderMode,
};
pub use client::{AiosClient, AiosEndpoints, InProcessBackend, ShutdownHandle};
pub use error::RenderError;
pub use evidence_render::EvidenceChainView;
pub use json_renderer::JsonRenderer;
pub use output_format::OutputFormat;
pub use renderable::{RenderContext, Renderable};
pub use table_renderer::{TableAlign, TableRenderer, TableSpec};
pub use text_renderer::TextRenderer;
pub use tree_renderer::{TreeNode, TreeRenderer};
pub use verification_render::VerificationPrimitiveList;
