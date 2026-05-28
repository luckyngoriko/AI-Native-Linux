//! Cross-crate integration shims for aios-network (T-162).
//!
//! Five bridges connecting L8 Network Policy to the rest of the AIOS stack:
//!
//! - [`policy_bridge`] — `NetworkPolicyError` → `aios_policy::PolicyDecision` denial.
//! - [`capability_bridge`] — `OutboundGrant` → typed `ActionEnvelope`.
//! - [`apps_bridge`] — `AppPackage` declared endpoints → unsigned `OutboundGrant` proposal.
//! - [`sandbox_bridge`] — `OutboundDirectiveKind` ∩ sandbox `NetworkPosture` (INV I6).
//! - [`renderer_web_bridge`] — S8.1 ↔ S7.5 exposure label alignment check.

pub mod apps_bridge;
pub mod capability_bridge;
pub mod policy_bridge;
pub mod renderer_web_bridge;
pub mod sandbox_bridge;
