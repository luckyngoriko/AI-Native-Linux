//! `aios-policy` — core types for the AIOS Policy Kernel (S2.3, schema `aios.policy.v1alpha1`).
//!
//! This crate implements the **wire-format-agnostic core data model** for the Policy Kernel
//! defined in `002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/01_policy_kernel.md`. It is
//! consumed by L3 (Capability Runtime), L4 (identity / vault), and L9 (evidence linkage).
//!
//! ## Scope of T-016 (skeleton)
//!
//! - [`Decision`] enum and [`PolicyDecision`] result struct per S2.3 §4.
//! - [`HardDenyClass`] enum (the 10 constitutional hard denies) per S2.3 §6.
//! - [`HydratedSubject`] + [`SubjectType`] per S2.3 §7.
//! - Stub [`Constraints`] + [`ApprovalRequirement`] per S2.3 §10 (full vocabulary deferred to
//!   T-017+).
//! - [`PolicyError`] taxonomy for the decision pipeline short-circuits (§3, §7, §8).
//!
//! The trait surface (`PolicyKernel::EvaluatePolicy`), bundle loading, rule precedence
//! evaluation, conditions parser, gRPC IDL, and integration with `aios-evidence` are
//! **explicitly out of scope** for T-016 and are queued for T-017 and later.
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
pub mod subject;

pub use constraints::{ApprovalRequirement, Constraints};
pub use decision::{Decision, PolicyDecision};
pub use error::PolicyError;
pub use hard_deny::HardDenyClass;
pub use subject::{HydratedSubject, SubjectType};
