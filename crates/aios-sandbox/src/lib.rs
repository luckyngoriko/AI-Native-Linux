//! `aios-sandbox` — L6 Sandbox Composition typed core skeleton (S3.2).
//!
//! Types-only crate; no trait, no enforcement, no gRPC, no evidence emission.

#![forbid(unsafe_code)]

/// `SandboxError` taxonomy.
pub mod error;
/// `GpuPolicy` + `GpuCapabilityClass` (S3.2 + S8.2 type-level).
pub mod gpu;
/// `IsolationKind` closed enum.
pub mod isolation;
/// `NetworkPosture` closed enum.
pub mod network;
/// `SandboxProfile` + `ProfileId`.
pub mod profile;
/// `ResourceLimits` + default factories + validation.
pub mod resources;

// Re-exports — flattened public surface
pub use error::SandboxError;
pub use gpu::{GpuCapabilityClass, GpuPolicy};
pub use isolation::IsolationKind;
pub use network::NetworkPosture;
pub use profile::{ProfileId, SandboxProfile};
pub use resources::ResourceLimits;

/// Crate version marker — bump on every semantic change.
pub const DEFAULT_CODE_VERSION: &str = "aios-sandbox/0.0.1-T106";
