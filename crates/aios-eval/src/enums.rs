use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Kind of evaluation metric collected during a benchmark run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvaluationMetricKind {
    /// Top-1 or exact-match accuracy on a held-out test set.
    Accuracy,
    /// Data/concept drift score computed against the frozen benchmark.
    Drift,
    /// Fraction of model outputs flagged as confabulated by the hallucination
    /// detector.
    Hallucination,
    /// Rejection rate for prompt-injection probes (INV‑004 gate).
    PromptInjectionRejection,
    /// Expected calibration error (ECE) measured on the benchmark.
    Calibration,
}

/// Role an agent plays in the multi-agent coordination loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentRole {
    /// Generates the task plan from the task intent.
    Planner,
    /// Executes plan steps under supervision.
    Executor,
    /// Reviews execution output and renders a verdict.
    Reviewer,
}

/// State machine for the multi-agent coordination protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MultiAgentState {
    /// Planner is decomposing the task intent.
    Planning,
    /// Plan is ready and waiting for human / system approval.
    AwaitingApproval,
    /// Executor is running an approved plan step.
    Executing,
    /// Execution completed; reviewer is evaluating output.
    UnderReview,
    /// Reviewer accepted execution output.
    Accepted,
    /// Reviewer rejected execution output.
    Rejected,
    /// Coordination blocked because reviewer ≡ executor or reviewer ≡ planner
    /// (INV‑016 separation violation).
    BlockedSeparationViolation,
}

/// Life‑cycle state of an evaluation harness run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvaluationHarnessState {
    /// Harness record created but benchmark not yet loaded.
    Staged,
    /// Benchmark is actively executing.
    Running,
    /// Evaluation finished and the report artifact was emitted.
    ReportEmitted,
    /// Harness blocked because the benchmark digest does not match the frozen
    /// reference.
    BlockedBenchmarkUnverified,
    /// Harness exited abnormally.
    Failed,
}

/// Trust tier assigned to a signed model bundle by the marketplace governance
/// policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelBundleTrustLevel {
    /// Bundle was evaluated and signed by the AIOS verification pipeline.
    AiosVerified,
    /// Bundle carries a valid third‑party publisher signature.
    ThirdPartySigned,
    /// Bundle exists only in the local registry (no remote signature chain).
    LocalOnly,
    /// Bundle lacks any provenance or its signature chain is broken.
    Untrusted,
}

/// Atomic verdict produced by the evaluation report thresholding logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verdict {
    /// All metrics meet or exceed the active threshold profile.
    Pass,
    /// One or more metrics fall below the threshold, indicating the model must
    /// not be promoted.
    Fail,
    /// One or more metrics are borderline; human review is recommended.
    Warn,
    /// Evaluation was incomplete (e.g. harness crashed, benchmark corrupt).
    Exception,
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use strum::{EnumCount, IntoEnumIterator};

    #[test]
    fn evaluation_metric_kind_has_five_variants() {
        assert_eq!(EvaluationMetricKind::COUNT, 5);
        assert_eq!(EvaluationMetricKind::iter().count(), 5);
    }

    #[test]
    fn agent_role_has_three_variants() {
        assert_eq!(AgentRole::COUNT, 3);
        assert_eq!(AgentRole::iter().count(), 3);
    }

    #[test]
    fn multi_agent_state_has_seven_variants() {
        assert_eq!(MultiAgentState::COUNT, 7);
        assert_eq!(MultiAgentState::iter().count(), 7);
    }

    #[test]
    fn evaluation_harness_state_has_five_variants() {
        assert_eq!(EvaluationHarnessState::COUNT, 5);
        assert_eq!(EvaluationHarnessState::iter().count(), 5);
    }

    #[test]
    fn model_bundle_trust_level_has_four_variants() {
        assert_eq!(ModelBundleTrustLevel::COUNT, 4);
        assert_eq!(ModelBundleTrustLevel::iter().count(), 4);
    }

    #[test]
    fn verdict_has_four_variants() {
        assert_eq!(Verdict::COUNT, 4);
        assert_eq!(Verdict::iter().count(), 4);
    }

    #[test]
    fn all_enums_serde_round_trip() {
        for variant in EvaluationMetricKind::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: EvaluationMetricKind = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "round-trip failed for {variant:?}");
        }
        for variant in AgentRole::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: AgentRole = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "round-trip failed for {variant:?}");
        }
        for variant in MultiAgentState::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: MultiAgentState = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "round-trip failed for {variant:?}");
        }
        for variant in EvaluationHarnessState::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: EvaluationHarnessState = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "round-trip failed for {variant:?}");
        }
        for variant in ModelBundleTrustLevel::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: ModelBundleTrustLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "round-trip failed for {variant:?}");
        }
        for variant in Verdict::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: Verdict = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "round-trip failed for {variant:?}");
        }
    }

    #[test]
    fn verdict_serde_is_screaming_snake() {
        let pass_json = serde_json::to_string(&Verdict::Pass).unwrap();
        assert_eq!(pass_json, "\"PASS\"");
        let fail_json = serde_json::to_string(&Verdict::Fail).unwrap();
        assert_eq!(fail_json, "\"FAIL\"");
        let warn_json = serde_json::to_string(&Verdict::Warn).unwrap();
        assert_eq!(warn_json, "\"WARN\"");
        let exception_json = serde_json::to_string(&Verdict::Exception).unwrap();
        assert_eq!(exception_json, "\"EXCEPTION\"");
    }

    #[test]
    fn multi_agent_state_blocked_is_present() {
        let blocked = MultiAgentState::BlockedSeparationViolation;
        let json = serde_json::to_string(&blocked).unwrap();
        assert_eq!(json, "\"BLOCKED_SEPARATION_VIOLATION\"");
        let back: MultiAgentState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, MultiAgentState::BlockedSeparationViolation);
    }
}
