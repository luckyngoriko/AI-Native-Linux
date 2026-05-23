//! `aios-action` — core types for the AIOS Action Envelope (S0.1, schema `aios.action.v1alpha1`).
//!
//! This crate implements the **wire-format-agnostic core data model** for the typed action
//! envelope defined in `XX_Cross_Cutting/01_action_envelope_lifecycle.md`. It is consumed by
//! L3 (Capability Runtime), L4 (Policy Kernel client), L5 (Cognitive Core), and L9 (Evidence Log).
//!
//! ## Scope of T-001
//!
//! - Top-level envelope partition (`identity`, `request`, `execution`, `trace`) per S0.1 §2.
//! - Prefix-namespaced ULID identifiers per S0.1 §3.2 (with the Wave-11 underscore-only rule).
//! - Closed `ActionPhase` enum + 6-transition FSM per S0.1 §4 / §6.
//! - Closed `DryRunMode` enum (`Live` default) per S0.1 §6.
//! - Subset of the canonical `PascalCase` error taxonomy per S0.1 §7.
//!
//! BLAKE3 canonical hashing (S0.1 §8), full error coverage, and proto-level serialization
//! belong to follow-up tasks T-002 / T-003 / T-005.
//!
//! ## Constitutional invariants enforced here
//!
//! - **No `unsafe`, no `panic!`, no `unwrap`/`expect`, no `todo!`/`unimplemented!`** — workspace
//!   lints forbid them; every fallible path returns a typed `Result`.
//! - **ID parser REJECTS colon-separated forms** (`act:01H...`) per S0.1 §3.2 — colon forms are a
//!   sentinel for legacy/illegal input.
//! - **Terminal phases are terminal** — `ActionPhase::can_transition_to` returns `false` from any
//!   terminal state.

#![forbid(unsafe_code)]

pub mod canonical;
pub mod envelope;
pub mod error;
pub mod execution;
pub mod id;
pub mod identity;
pub mod phase;
pub mod request;
pub mod trace;

pub use canonical::{blake3_hash, blake3_truncated, jcs_canonicalize, CanonicalError};
pub use envelope::{ActionEnvelope, SCHEMA_VERSION};
pub use error::{ActionError, IdError};
pub use execution::{Condition, ConditionStatus, Execution};
pub use id::{
    ActionId, ApprovalId, CorrelationId, EvidenceReceiptId, IntentId, PlanId, PolicyDecisionId,
    PolicyRequestId,
};
pub use identity::Identity;
pub use phase::ActionPhase;
pub use request::{DryRunMode, Request};
pub use trace::Trace;
