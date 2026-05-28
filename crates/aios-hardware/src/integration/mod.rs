//! Cross-crate integration shims for aios-hardware (T-174).
//!
//! Five bridges connecting L8 Hardware Graph + GPU Resource Model + Firmware Trust
//! to the rest of the AIOS stack:
//!
//! - [`policy_bridge`] — `HardwareError` → `aios_policy::PolicyDecision` denial.
//! - [`capability_bridge`] — `DriverBinding` → typed `ActionEnvelope`.
//! - [`sandbox_bridge`] — `GpuCapabilityBinding` → `aios_sandbox::GpuPolicy`.
//! - [`recovery_bridge`] — `EvilMaidEvidenceMarker` → recovery-mode signal.
//! - [`network_bridge`] — `HardwareGraph` → `NetworkPostureHint`.

pub mod capability_bridge;
pub mod network_bridge;
pub mod policy_bridge;
pub mod recovery_bridge;
pub mod sandbox_bridge;
