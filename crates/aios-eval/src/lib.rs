//! `aios-eval` — S27 AI Evaluation and Model Governance.
//!
//! Provides:
//!
//! - **Enums** — closed vocabulary types for evaluation metrics, agent roles,
//!   multi‑agent coordination states, harness lifecycle states, trust levels,
//!   and verdicts.
//! - **Harness** — [`AIEvaluationHarness`] orchestrates a single benchmark run
//!   against a model under test with mandatory safety invariants.
//! - **Report** — [`ModelEvaluationReport`] collects scalar metrics and computes
//!   a verdict against configurable threshold profiles.
//! - **Thresholds** — [`VerdictThresholds`] define per‑profile accuracy,
//!   hallucination, rejection, and calibration boundaries.
//! - **Multi‑Agent** — [`MultiAgentCoordination`] enforces INV‑016 separation of
//!   duties between Planner, Executor, and Reviewer.
//! - **Marketplace** — [`SignedModelBundle`] represents a cryptographically
//!   signed model artifact with trust‑level and SLSA provenance metadata.

#![forbid(unsafe_code)]

pub mod enums;
pub mod harness;
pub mod marketplace;
pub mod multi_agent;
pub mod report;
pub mod thresholds;

pub use enums::{
    AgentRole, EvaluationHarnessState, EvaluationMetricKind, ModelBundleTrustLevel,
    MultiAgentState, Verdict,
};
pub use harness::AIEvaluationHarness;
pub use marketplace::SignedModelBundle;
pub use multi_agent::MultiAgentCoordination;
pub use report::ModelEvaluationReport;
pub use thresholds::VerdictThresholds;
