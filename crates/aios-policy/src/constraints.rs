//! Constraints + approval-requirement placeholders.
//!
//! These are **stubs** for T-016 — just enough surface to satisfy
//! [`crate::decision::PolicyDecision`]'s field types. The full S2.3 §10 constraints
//! vocabulary (11 fields: `sandbox_profile_id`, `max_runtime_seconds`, …) and the
//! S2.3 §11 / §12 / §15 approval-requirement shape land in T-017 alongside the
//! conditions parser and bundle loader.

use serde::{Deserialize, Serialize};

/// Stub for T-017 expansion — full closed constraints vocabulary per S2.3 §10.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constraints {}

/// Stub for T-017 expansion — full approval-requirement shape per S2.3 §11 / §12 / §15.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequirement {}
