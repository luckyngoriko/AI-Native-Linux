//! `aios-recovery` — typed core skeleton for S9.1, S9.2, and S9.3.
//!
//! T-074 intentionally stops at the type surface: recovery mode state,
//! first-boot context, dedicated-kernel candidate metadata, the degraded
//! recovery bundle, and the recovery error taxonomy.

#![forbid(unsafe_code)]

pub mod boot;
pub mod boundary;
pub mod bundle;
pub mod error;
pub mod first_boot;
pub mod in_memory_boundary;
pub mod kernel;
pub mod kernel_pipeline;
pub mod mode;

pub use boot::{BootId, BootPhase, FirstBootContext, FirstBootPhase, FirstBootStatus};
pub use boundary::{EnterRecoveryRequest, RecoveryBoundary};
pub use bundle::RecoveryBundle;
pub use error::RecoveryError;
pub use first_boot::FirstBootDriver;
pub use in_memory_boundary::InMemoryRecoveryBoundary;
pub use kernel::{CandidateId, CandidateState, KernelCandidate, KernelManifest};
pub use kernel_pipeline::KernelPipelineDriver;
pub use mode::{RecoveryMode, RecoveryState};
