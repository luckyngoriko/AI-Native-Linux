//! S7.6 §3 closed CLI renderer enum vocabulary.

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Renderer session mode selected by S7.6 §3.1.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CliRenderMode {
    /// Proto3 zero-value sentinel; rejected for active sessions.
    CliRenderModeUnspecified,
    /// Human at a `TTY`; full feature set, styling, and prompts permitted.
    NormalInteractive,
    /// Structured `JSON` output for tooling callers.
    Scripting,
    /// Recovery boot console surface.
    RecoveryTty,
    /// ANSI unavailable or terminal hostile; structural plain fallback.
    DegradedNoColor,
}

/// Result code emitted by S7.6 §3.2 compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CliCompilationResult {
    /// Proto3 zero-value sentinel; rejected for real render results.
    CliCompilationResultUnspecified,
    /// Full ANSI styling and UTF-8 box drawing applied.
    CompiledRich,
    /// Structural compilation succeeded without styling.
    CompiledPlain,
    /// Recovery `TTY` compilation succeeded.
    CompiledRecovery,
    /// Tree compiled with one or more CLI placeholders.
    DegradedPartial,
    /// Required node kind cannot render in CLI.
    FailedNodeKindUnsupported,
    /// S7.2 tree signature failed.
    FailedTreeSignatureInvalid,
    /// Recovery session encountered a rejected kind or surface.
    FailedRecoveryKindRejected,
    /// ANSI sanitizer rejected injected content.
    FailedAnsiInjectionBlocked,
    /// UI tree exceeded the CLI size bound.
    FailedTreeTooLarge,
    /// Renderer-internal failure.
    FailedRendererInternal,
}

/// Input mode selected for a CLI renderer invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CliInputMode {
    /// Proto3 zero-value sentinel; rejected for active sessions.
    CliInputModeUnspecified,
    /// `stdin` is a `TTY`; prompts are allowed.
    InteractiveTty,
    /// `stdin` is a pipe carrying structured input.
    ScriptPiped,
    /// No interactive input is available.
    NonInteractive,
    /// No controlling `TTY` exists.
    NoTty,
    /// Read-only caller assertion; approval prompts are rejected.
    ReadOnlyQuery,
}

/// Terminal ANSI capability level detected by S7.6 §3.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AnsiSupportLevel {
    /// Proto3 zero-value sentinel; rejected for active sessions.
    AnsiSupportLevelUnspecified,
    /// 24-bit color support.
    Truecolor,
    /// 256-color ANSI support.
    Color256,
    /// Basic 16-color ANSI support.
    Color16,
    /// No color; ASCII structural fallback.
    Monochrome,
}

/// CLI renderer evidence record kinds queued by S7.6 §3.5 / §10.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CliEvidenceRecordKind {
    /// Proto3 zero-value sentinel; rejected for real evidence records.
    CliEvidenceRecordKindUnspecified,
    /// Session started and mode selected.
    CliRenderStarted,
    /// Render failed.
    CliRenderFailed,
    /// Node kind unsupported in CLI.
    CliNodeKindUnsupported,
    /// Recovery mode rejected a node or surface kind.
    CliRecoveryKindRejected,
    /// Piped or auto approval attempt rejected.
    CliAutoConfirmRejected,
    /// ANSI injection attempt blocked.
    CliAnsiInjectionBlocked,
    /// Session degraded because no usable `TTY` exists.
    CliDegradedNoTty,
    /// Scripting mode invoked.
    CliScriptingModeInvoked,
    /// Recovery operator authenticated.
    CliOperatorAuthenticated,
    /// Trust indicator ordering invariant failed.
    CliTrustIndicatorReordered,
}
