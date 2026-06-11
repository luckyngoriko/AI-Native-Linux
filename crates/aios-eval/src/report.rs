use crate::enums::Verdict;
use crate::thresholds::VerdictThresholds;
use chrono::Utc;
use ulid::Ulid;

/// A model evaluation report emitted after a harness run completes.
///
/// The report collects scalar metrics from the benchmark and computes a
/// binary/ternary [`Verdict`] against a configured threshold profile.
#[derive(Debug, Clone)]
pub struct ModelEvaluationReport {
    /// Unique report identifier (prefix `"mer_"` + ULID).
    pub report_id: String,
    /// ID of the model under test.
    pub model_id: String,
    /// The harness that produced this report.
    pub harness_id: String,
    /// Identifier of the benchmark used.
    pub benchmark_id: String,
    /// Blake3 digest of the frozen benchmark content.
    pub benchmark_digest: String,
    /// UTC wall‑clock time when the evaluation was completed.
    pub evaluated_at: chrono::DateTime<Utc>,
    /// Top‑1 or exact‑match accuracy in \[0.0, 1.0\].  `None` if not measured.
    pub accuracy_score: Option<f64>,
    /// Number of test samples used for the accuracy metric.
    pub accuracy_n: Option<u64>,
    /// `true` if concept/drift was detected.  `None` if drift analysis was
    /// skipped.
    pub drift_detected: Option<bool>,
    /// Fraction of outputs flagged as confabulated.  `None` if the
    /// hallucination detector was not run.
    pub hallucination_rate: Option<f64>,
    /// Fraction of prompt‑injection probes correctly rejected.  `None` if the
    /// injection suite was not run.
    pub prompt_injection_rejection_rate: Option<f64>,
    /// Expected calibration error.  `None` if calibration was not measured.
    pub ece: Option<f64>,
    /// Computed verdict (set by [`compute_verdict`](Self::compute_verdict)).
    pub verdict: Verdict,
}

impl ModelEvaluationReport {
    /// Creates a new report pre‑filled with defaults and a fresh ULID.
    ///
    /// The report enters the world with `verdict = Verdict::Exception`; callers
    /// MUST call [`compute_verdict`](Self::compute_verdict) to compute the
    /// actual verdict after populating the metric fields.
    #[must_use]
    pub fn new(
        model_id: impl Into<String>,
        harness_id: impl Into<String>,
        benchmark_id: impl Into<String>,
        benchmark_digest: impl Into<String>,
    ) -> Self {
        Self {
            report_id: format!("mer_{}", Ulid::new()),
            model_id: model_id.into(),
            harness_id: harness_id.into(),
            benchmark_id: benchmark_id.into(),
            benchmark_digest: benchmark_digest.into(),
            evaluated_at: Utc::now(),
            accuracy_score: None,
            accuracy_n: None,
            drift_detected: None,
            hallucination_rate: None,
            prompt_injection_rejection_rate: None,
            ece: None,
            verdict: Verdict::Exception,
        }
    }

    /// Computes the verdict by comparing the report's metrics against the
    /// supplied threshold profile.
    ///
    /// # Decision logic
    ///
    /// - **Pass** — *all* of: accuracy ≥ `min_accuracy`, hallucination_rate ≤
    ///   `max_hallucination`, drift_detected ≠ `true`, rejection_rate ≥
    ///   `min_rejection`.
    /// - **Fail** — any metric falls below (or exceeds, for bounded‑above
    ///   metrics) the hard threshold.
    /// - **Warn** — all metrics meet the hard pass criteria but one or more are
    ///   within 5 % of the threshold boundary.
    /// - **Exception** — one or more metrics are `None` (benchmark did not
    ///   measure them).
    #[must_use]
    pub fn compute_verdict(&self, thresholds: &VerdictThresholds) -> Verdict {
        let accuracy = match self.accuracy_score {
            Some(v) => v,
            None => return Verdict::Exception,
        };
        let hallucination = match self.hallucination_rate {
            Some(v) => v,
            None => return Verdict::Exception,
        };
        let rejection = match self.prompt_injection_rejection_rate {
            Some(v) => v,
            None => return Verdict::Exception,
        };
        let ece = match self.ece {
            Some(v) => v,
            None => return Verdict::Exception,
        };

        // Hard‑fail gates
        if accuracy < thresholds.min_accuracy {
            return Verdict::Fail;
        }
        if hallucination > thresholds.max_hallucination {
            return Verdict::Fail;
        }
        if self.drift_detected == Some(true) {
            return Verdict::Fail;
        }
        if rejection < thresholds.min_rejection_rate {
            return Verdict::Fail;
        }
        if ece > thresholds.max_ece {
            return Verdict::Fail;
        }

        // Borderline (within 5 % of threshold boundary in the "bad" direction)
        let borderline_margin = 0.05;
        let acc_borderline = accuracy - thresholds.min_accuracy < borderline_margin;
        let hal_borderline = thresholds.max_hallucination - hallucination < borderline_margin;
        let rej_borderline = rejection - thresholds.min_rejection_rate < borderline_margin;
        let ece_borderline = thresholds.max_ece - ece < borderline_margin;

        if acc_borderline || hal_borderline || rej_borderline || ece_borderline {
            Verdict::Warn
        } else {
            Verdict::Pass
        }
    }
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

    fn perfect_report() -> ModelEvaluationReport {
        ModelEvaluationReport {
            accuracy_score: Some(1.0),
            hallucination_rate: Some(0.0),
            prompt_injection_rejection_rate: Some(1.0),
            ece: Some(0.0),
            drift_detected: Some(false),
            ..ModelEvaluationReport::new("m1", "h1", "b1", "d1")
        }
    }

    fn zero_report() -> ModelEvaluationReport {
        ModelEvaluationReport {
            accuracy_score: Some(0.0),
            hallucination_rate: Some(1.0),
            prompt_injection_rejection_rate: Some(0.0),
            ece: Some(1.0),
            drift_detected: Some(true),
            ..ModelEvaluationReport::new("m1", "h1", "b1", "d1")
        }
    }

    #[test]
    fn perfect_report_passes_all_profiles() {
        let report = perfect_report();
        assert_eq!(
            report.compute_verdict(&VerdictThresholds::for_profile("dev")),
            Verdict::Pass
        );
        assert_eq!(
            report.compute_verdict(&VerdictThresholds::for_profile("stig")),
            Verdict::Pass
        );
    }

    #[test]
    fn zero_report_fails_all_profiles() {
        let report = zero_report();
        assert_eq!(
            report.compute_verdict(&VerdictThresholds::for_profile("dev")),
            Verdict::Fail
        );
        assert_eq!(
            report.compute_verdict(&VerdictThresholds::for_profile("stig")),
            Verdict::Fail
        );
    }

    #[test]
    fn missing_metric_returns_exception() {
        let report = ModelEvaluationReport::new("m1", "h1", "b1", "d1");
        assert_eq!(
            report.compute_verdict(&VerdictThresholds::for_profile("dev")),
            Verdict::Exception
        );
    }

    #[test]
    fn accuracy_below_threshold_fails() {
        let report = ModelEvaluationReport {
            accuracy_score: Some(0.5),
            hallucination_rate: Some(0.0),
            prompt_injection_rejection_rate: Some(1.0),
            ece: Some(0.0),
            drift_detected: Some(false),
            ..ModelEvaluationReport::new("m1", "h1", "b1", "d1")
        };
        assert_eq!(
            report.compute_verdict(&VerdictThresholds::for_profile("dev")),
            Verdict::Fail
        );
    }

    #[test]
    fn drift_detected_fails() {
        let report = ModelEvaluationReport {
            accuracy_score: Some(1.0),
            hallucination_rate: Some(0.0),
            prompt_injection_rejection_rate: Some(1.0),
            ece: Some(0.0),
            drift_detected: Some(true),
            ..ModelEvaluationReport::new("m1", "h1", "b1", "d1")
        };
        assert_eq!(
            report.compute_verdict(&VerdictThresholds::for_profile("dev")),
            Verdict::Fail
        );
    }

    #[test]
    fn borderline_accuracy_triggers_warn() {
        let t = VerdictThresholds::for_profile("stig"); // min_accuracy = 0.9
        let report = ModelEvaluationReport {
            accuracy_score: Some(0.91), // just 0.01 above threshold → borderline
            hallucination_rate: Some(0.0),
            prompt_injection_rejection_rate: Some(1.0),
            ece: Some(0.0),
            drift_detected: Some(false),
            ..ModelEvaluationReport::new("m1", "h1", "b1", "d1")
        };
        assert_eq!(report.compute_verdict(&t), Verdict::Warn);
    }

    #[test]
    fn borderline_hallucination_triggers_warn() {
        let t = VerdictThresholds::for_profile("stig"); // max_hallucination = 0.05
        let report = ModelEvaluationReport {
            accuracy_score: Some(1.0),
            hallucination_rate: Some(0.03), // within 0.02 of 0.05 → borderline
            prompt_injection_rejection_rate: Some(1.0),
            ece: Some(0.0),
            drift_detected: Some(false),
            ..ModelEvaluationReport::new("m1", "h1", "b1", "d1")
        };
        assert_eq!(report.compute_verdict(&t), Verdict::Warn);
    }

    #[test]
    fn report_id_starts_with_mer_prefix() {
        let report = ModelEvaluationReport::new("m1", "h1", "b1", "d1");
        assert!(report.report_id.starts_with("mer_"));
    }
}
