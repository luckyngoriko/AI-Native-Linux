//! `aios-recovery` — typed core skeleton for S9.1, S9.2, and S9.3.
//!
//! T-074 intentionally stops at the type surface: recovery mode state,
//! first-boot context, dedicated-kernel candidate metadata, the degraded
//! recovery bundle, and the recovery error taxonomy.

#![forbid(unsafe_code)]

pub mod boot;
pub mod bundle;
pub mod error;
pub mod kernel;
pub mod mode;

pub use boot::{BootId, BootPhase, FirstBootContext, FirstBootPhase, FirstBootStatus};
pub use bundle::RecoveryBundle;
pub use error::RecoveryError;
pub use kernel::{CandidateId, CandidateState, KernelCandidate, KernelManifest};
pub use mode::{RecoveryMode, RecoveryState};
