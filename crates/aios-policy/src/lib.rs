//! `aios-policy` — core types for the AIOS Policy Kernel (S2.3, schema `aios.policy.v1alpha1`).
//!
//! This crate implements the **wire-format-agnostic core data model** for the Policy Kernel
//! defined in `002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/01_policy_kernel.md`. It is
//! consumed by L3 (Capability Runtime), L4 (identity / vault), and L9 (evidence linkage).
//!
//! ## Scope of T-016 (skeleton) + T-017 (pipeline)
//!
//! - [`Decision`] enum and [`PolicyDecision`] result struct per S2.3 §4.
//! - [`HardDenyClass`] enum (the 10 constitutional hard denies) per S2.3 §6.
//! - [`HydratedSubject`] + [`SubjectType`] per S2.3 §7.
//! - **T-020**: Full [`Constraints`] vocabulary per S2.3 §10 (11 fields) +
//!   [`ApprovalRequirement`] per §11.2 / §15, with closed enums
//!   ([`EvidenceGrade`], [`NetworkPolicy`], [`SessionClass`], [`ApprovalScope`],
//!   [`ApproverClass`]) and [`Constraints::validate`] enforcing §13.2 TTL bounds
//!   and §10 non-zero budget invariants.
//! - [`PolicyError`] taxonomy for the decision pipeline short-circuits (§3, §7, §8).
//! - **T-017**: [`PolicyKernel`] async trait + [`PolicyContext`] + [`EnrichmentSnapshot`]
//!   stub (S2.3 §3 / §8 / §20).
//! - **T-017**: [`DecisionPipeline`] — the 12-step pipeline driver per S2.3 §3, with
//!   steps 1 (schema), 2 (subject pass-through), 9 (default deny floor), 11 (decision
//!   emission) REAL, and steps 3, 4, 5, 6, 7, 8, 10, 12 stubbed for T-018..T-025.
//! - **T-017**: [`RulePrecedence`] — the 7-tier fixed precedence ladder per S2.3 §5.
//! - **T-017**: [`InMemoryPolicyKernel`] — the in-process harness used by tests and by
//!   T-018..T-025 to attach the real implementations of the stubbed steps.
//!
//! Bundle loading, hard-deny enforcement, conditions parser, gRPC IDL, caching, and
//! integration with `aios-evidence` are **explicitly out of scope** for T-017 and are
//! queued for T-018..T-025.
//!
//! ## Constitutional invariants enforced here
//!
//! - **No `unsafe`, no `panic!`, no `unwrap`/`expect`, no `todo!`/`unimplemented!`** — workspace
//!   lints forbid them; every fallible path returns a typed `Result`.
//! - **`HardDenyClass` enumerates exactly the 10 rows of S2.3 §6** — `EnumCount` provides a
//!   compile-time anchor and the round-trip tests assert the count.
//! - **`Decision::Unspecified` is reserved for proto wire compatibility** and never participates
//!   in policy logic — the three active variants (`Allow`, `RequireApproval`, `Deny`) cover
//!   every pipeline outcome.

#![forbid(unsafe_code)]

pub mod constraints;
pub mod decision;
pub mod error;
pub mod hard_deny;
pub mod hard_deny_engine;
pub mod kernel;
pub mod pipeline;
pub mod precedence;
pub mod subject;

pub use constraints::{
    ApprovalRequirement, ApprovalScope, ApproverClass, Constraints, EvidenceGrade, NetworkPolicy,
    SandboxProfileId, SessionClass, VaultCapabilityId,
};
pub use decision::{Decision, PolicyDecision};
pub use error::PolicyError;
pub use hard_deny::HardDenyClass;
pub use hard_deny_engine::{
    has_recovery_override_path, reason_code_for, reason_message_for, HardDenyEngine,
    HardDenyEngineConfig,
};
pub use kernel::{EnrichmentSnapshot, InMemoryPolicyKernel, PolicyContext, PolicyKernel};
pub use pipeline::{reason_code, DecisionPipeline, PipelineState};
pub use precedence::RulePrecedence;
pub use subject::{HydratedSubject, SubjectType};
