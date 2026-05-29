//! First-run capability-lie audit per S11.1 §9 (§9.1–§9.4).
//!
//! # Pipeline step 17 (§6.17)
//!
//! Within the first 60 seconds of a package's runtime, every capability
//! invocation by a subject acting for the package is observed from four sources
//! (L3 Capability Runtime, L4 Vault Broker, L8 Network Policy, L7 Renderer).
//! At window end, declared vs. observed capabilities are compared:
//!
//! - `observed ⊆ declared` → audit PASSES; package stays `Active`.
//! - under-declaration (`observed ⊄ declared`) → audit FAILS;
//!   `drift = observed − declared`; package transitions
//!   `Active → Quarantined`; result is [`CapabilityLie`](crate::install_state::PackageVerificationResult::CapabilityLie).
//!
//! # §9.3 edges
//!
//! - An empty observed set passes by default (subset trivially).
//! - One-shot: an observation after the window does NOT re-trigger the audit.
//! - Per-install (not per-package-id): a fresh audit per install.
//! - Over-declaration (declaring MORE than observed) is fine.
//!
//! # §9.4 no-re-audit-release rule
//!
//! A `Quarantined`-via-lie package CANNOT be released by re-audit. Release
//! requires operator review + uninstall + new manifest version + fresh install.
//! The audit is deterministic — there is no "false-positive" release path.
//!
//! # Time injection
//!
//! All time is injected via `DateTime<Utc>` parameters. No real wall-clock or
//! sleeps are used anywhere in this module.

use chrono::{DateTime, Duration, Utc};
use std::collections::BTreeSet;

use crate::ids::PackageId;
use crate::install_fsm;
use crate::install_state::{PackageInstallState, PackageVerificationResult};

// ---------------------------------------------------------------------------
// ObservationSource — the four observation surfaces (§9.1)
// ---------------------------------------------------------------------------

/// The four observation sources that feed the first-run capability-lie audit
/// per S11.1 §9.1.
///
/// Each source observes capability invocations from its domain during the
/// 60-second audit window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObservationSource {
    /// Observations from the L3 Capability Runtime (typed action execution).
    L3CapabilityRuntime,
    /// Observations from the L4 Vault Broker (secret/capability access).
    L4VaultBroker,
    /// Observations from the L8 Network Policy engine (outbound requests).
    L8NetworkPolicy,
    /// Observations from the L7 Renderer layer (UI-requested capabilities).
    L7Renderer,
}

// ---------------------------------------------------------------------------
// CapabilityObservation — a single observed invocation
// ---------------------------------------------------------------------------

/// A single capability invocation observed by one of the four observation
/// sources during the first-run audit window (§9.1).
///
/// The `at` timestamp must be within `[opened_at, opened_at + window)` for
/// the observation to be recorded by [`FirstRunAudit::record`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityObservation {
    /// The capability identifier as declared in the package manifest.
    pub capability_id: String,
    /// Which observation source reported this invocation.
    pub source: ObservationSource,
    /// When the invocation was observed.
    pub at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// AuditOutcome — the binary result of the audit
// ---------------------------------------------------------------------------

/// The binary outcome of the first-run capability-lie audit (§9.2).
///
/// - `Passed` — `observed ⊆ declared`; package stays `Active`.
/// - `Failed { drift }` — under-declaration detected; `drift = observed − declared`
///   (sorted, deterministic); package must be quarantined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditOutcome {
    /// Audit passed; all observed capabilities are in the declared set.
    Passed,
    /// Audit failed; the `drift` contains the observed-but-undeclared capabilities
    /// (sorted alphabetically for determinism).
    Failed {
        /// The set of capability IDs that were observed but not declared,
        /// sorted alphabetically.
        drift: Vec<String>,
    },
}

// ---------------------------------------------------------------------------
// FirstRunAudit — the per-install audit accumulator (§9.2)
// ---------------------------------------------------------------------------

/// The first-run capability-lie audit accumulator (§9.2).
///
/// Created at install time with the package's declared capabilities and an
/// `opened_at` timestamp. Observations are fed in via [`record`](Self::record)
/// during the 60-second window. After the window closes, [`evaluate`](Self::evaluate)
/// produces the final [`AuditOutcome`].
///
/// # Per-install freshness (§9.3)
///
/// Each install creates a fresh `FirstRunAudit`. Two installs of the same
/// package produce independent audits — no state is shared.
///
/// # Example
///
/// ```ignore
/// use chrono::Utc;
/// use std::collections::BTreeSet;
/// use aios_distribution::ids::PackageId;
/// use aios_distribution::lie_audit::{FirstRunAudit, CapabilityObservation, ObservationSource, AuditOutcome};
///
/// let now = Utc::now();
/// let declared: BTreeSet<String> = ["cap.read".into(), "cap.write".into()].into();
/// let mut audit = FirstRunAudit::new(PackageId("pkg:test:app".into()), declared, now);
///
/// // Record an observation within the window
/// audit.record(CapabilityObservation {
///     capability_id: "cap.read".into(),
///     source: ObservationSource::L3CapabilityRuntime,
///     at: now + chrono::Duration::seconds(30),
/// });
///
/// // After 60s, evaluate
/// let outcome = audit.evaluate(now + chrono::Duration::seconds(61));
/// assert_eq!(outcome, AuditOutcome::Passed);
/// ```
#[derive(Debug, Clone)]
pub struct FirstRunAudit {
    /// The package being audited.
    pub package_id: PackageId,
    /// When the audit window opened (install-complete time).
    pub opened_at: DateTime<Utc>,
    /// The observation window duration (default: 60 seconds per §9).
    pub window: Duration,
    /// The set of capabilities declared in the package manifest.
    pub declared: BTreeSet<String>,
    /// The set of distinct capability IDs observed so far (within the window).
    pub observed: BTreeSet<String>,
}

impl FirstRunAudit {
    /// Creates a new first-run audit with a 60-second default observation window.
    ///
    /// `declared` is the set of capability IDs from the package manifest.
    /// `opened_at` is the timestamp when the package became `Active` (install
    /// complete time). The observation window extends from `opened_at` to
    /// `opened_at + 60s`.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(
        package_id: PackageId,
        declared: BTreeSet<String>,
        opened_at: DateTime<Utc>,
    ) -> Self {
        Self {
            package_id,
            opened_at,
            window: Duration::seconds(60),
            declared,
            observed: BTreeSet::new(),
        }
    }

    /// Records a capability observation.
    ///
    /// The observation is recorded **only** if `obs.at` falls within
    /// `[opened_at, opened_at + window)`. Observations at or after
    /// `opened_at + window` are silently ignored — the audit is one-shot
    /// per §9.3 (no retroactive re-trigger).
    ///
    /// Only the `capability_id` is stored; the `source` and exact `at` are
    /// discarded after the window check (they are available in the evidence
    /// log produced by T-196).
    pub fn record(&mut self, obs: CapabilityObservation) {
        let window_end = self.opened_at + self.window;
        if obs.at >= self.opened_at && obs.at < window_end {
            self.observed.insert(obs.capability_id);
        }
    }

    /// Evaluates the audit outcome.
    ///
    /// # Precondition
    ///
    /// Callers MUST ensure `now >= opened_at + window` (the observation window
    /// has closed) before treating the returned [`AuditOutcome`] as final.
    /// Calling [`evaluate`] before the window closes is deterministic (it checks
    /// whatever has been observed so far) but may be preliminary.
    ///
    /// # Outcome
    ///
    /// - `observed ⊆ declared` → [`AuditOutcome::Passed`].
    /// - `observed ⊄ declared` → [`AuditOutcome::Failed`] with `drift` equal
    ///   to the sorted set difference `observed − declared`.
    #[must_use]
    pub fn evaluate(&self, _now: DateTime<Utc>) -> AuditOutcome {
        if self.observed.is_subset(&self.declared) {
            AuditOutcome::Passed
        } else {
            let mut drift: Vec<String> =
                self.observed.difference(&self.declared).cloned().collect();
            drift.sort();
            AuditOutcome::Failed { drift }
        }
    }
}

// ---------------------------------------------------------------------------
// apply_audit_outcome — FSM integration
// ---------------------------------------------------------------------------

/// Applies the audit outcome to the package install state.
///
/// - [`AuditOutcome::Passed`] → state stays `Active`; returns `None` (no
///   verification result).
/// - [`AuditOutcome::Failed`] → FSM transition `Active → Quarantined` via
///   [`install_fsm::apply`]; returns `Some(CapabilityLie)`.
///
/// # §9.4 no-re-audit-release rule
///
/// A `Quarantined`-via-lie package CANNOT be released by re-audit. Release
/// requires: operator review + uninstall + new manifest version + fresh
/// install. There is no "false-positive" release path — the audit is
/// deterministic.
///
/// # Returns
///
/// A tuple of `(final_state, optional_verification_result)`. On `Passed`,
/// `final_state` is the state as-is (expected to be `Active`). On `Failed`,
/// `final_state` is `Quarantined` and the verification result is
/// [`PackageVerificationResult::CapabilityLie`].
#[must_use]
pub fn apply_audit_outcome(
    state: &mut PackageInstallState,
    outcome: &AuditOutcome,
) -> (PackageInstallState, Option<PackageVerificationResult>) {
    match outcome {
        AuditOutcome::Passed => (*state, None),
        AuditOutcome::Failed { .. } => {
            // Active → Quarantined is a valid FSM transition per §3.6.
            // If the transition fails (e.g. state is already Quarantined),
            // we leave the state unchanged — the `let _` absorbs the error.
            let _ = install_fsm::apply(state, PackageInstallState::Quarantined);
            (*state, Some(PackageVerificationResult::CapabilityLie))
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::needless_collect,
    clippy::format_collect,
    clippy::too_many_arguments,
    clippy::float_cmp
)]
mod tests {
    use super::*;

    fn t0() -> DateTime<Utc> {
        DateTime::from_timestamp(0, 0).expect("UNIX epoch is valid")
    }

    fn make_declared(caps: &[&str]) -> BTreeSet<String> {
        caps.iter().map(|s| (*s).to_string()).collect()
    }

    fn make_obs(
        cap_id: &str,
        at: DateTime<Utc>,
        source: ObservationSource,
    ) -> CapabilityObservation {
        CapabilityObservation {
            capability_id: cap_id.to_string(),
            source,
            at,
        }
    }

    fn test_package_id() -> PackageId {
        PackageId("pkg:test:audit-app".into())
    }

    // ── Lie audit: evaluate ──────────────────────────────────────────────

    #[test]
    fn observed_subset_of_declared_passes() {
        let declared = make_declared(&["cap.a", "cap.b", "cap.c"]);
        let mut audit = FirstRunAudit::new(test_package_id(), declared, t0());

        audit.record(make_obs(
            "cap.a",
            t0() + Duration::seconds(10),
            ObservationSource::L3CapabilityRuntime,
        ));
        audit.record(make_obs(
            "cap.b",
            t0() + Duration::seconds(20),
            ObservationSource::L4VaultBroker,
        ));

        let outcome = audit.evaluate(t0() + Duration::seconds(61));
        assert_eq!(outcome, AuditOutcome::Passed);
    }

    #[test]
    fn undeclared_capability_causes_failed_with_drift() {
        let declared = make_declared(&["cap.a"]);
        let mut audit = FirstRunAudit::new(test_package_id(), declared, t0());

        audit.record(make_obs(
            "cap.a",
            t0() + Duration::seconds(10),
            ObservationSource::L3CapabilityRuntime,
        ));
        audit.record(make_obs(
            "cap.undeclared",
            t0() + Duration::seconds(20),
            ObservationSource::L4VaultBroker,
        ));

        let outcome = audit.evaluate(t0() + Duration::seconds(61));
        assert_eq!(
            outcome,
            AuditOutcome::Failed {
                drift: vec!["cap.undeclared".to_string()]
            }
        );
    }

    #[test]
    fn empty_observed_passes_trivially() {
        let declared = make_declared(&["cap.a", "cap.b"]);
        let audit = FirstRunAudit::new(test_package_id(), declared, t0());

        let outcome = audit.evaluate(t0() + Duration::seconds(61));
        assert_eq!(outcome, AuditOutcome::Passed);
    }

    #[test]
    fn over_declaration_passes() {
        // Declared MORE than observed — over-declaration is fine per §9.
        let declared = make_declared(&["cap.a", "cap.b", "cap.c", "cap.extra"]);
        let mut audit = FirstRunAudit::new(test_package_id(), declared, t0());

        audit.record(make_obs(
            "cap.a",
            t0() + Duration::seconds(5),
            ObservationSource::L3CapabilityRuntime,
        ));
        audit.record(make_obs(
            "cap.b",
            t0() + Duration::seconds(10),
            ObservationSource::L7Renderer,
        ));

        let outcome = audit.evaluate(t0() + Duration::seconds(61));
        assert_eq!(outcome, AuditOutcome::Passed);
    }

    #[test]
    fn observation_after_window_is_ignored() {
        let declared = make_declared(&["cap.a"]);
        let mut audit = FirstRunAudit::new(test_package_id(), declared, t0());

        // Within window
        audit.record(make_obs(
            "cap.a",
            t0() + Duration::seconds(30),
            ObservationSource::L3CapabilityRuntime,
        ));
        // After window (at 65s — should be ignored per §9.3 one-shot)
        audit.record(make_obs(
            "cap.undeclared",
            t0() + Duration::seconds(65),
            ObservationSource::L4VaultBroker,
        ));

        let outcome = audit.evaluate(t0() + Duration::seconds(70));
        assert_eq!(outcome, AuditOutcome::Passed);
    }

    #[test]
    fn evaluate_before_window_close_is_deterministic() {
        // §9 requires `now >= opened_at + window` for the final audit.
        // Calling evaluate before the window closes is deterministic
        // (checks whatever has been observed so far) but may be preliminary.
        let declared = make_declared(&["cap.a", "cap.b"]);
        let mut audit = FirstRunAudit::new(test_package_id(), declared, t0());

        audit.record(make_obs(
            "cap.a",
            t0() + Duration::seconds(10),
            ObservationSource::L3CapabilityRuntime,
        ));

        // Called at t=30s — window not yet closed, but deterministic result
        let outcome = audit.evaluate(t0() + Duration::seconds(30));
        // All observed caps are declared → Passed
        assert_eq!(outcome, AuditOutcome::Passed);
    }

    #[test]
    fn drift_is_sorted_and_deterministic() {
        let declared = make_declared(&["cap.a"]);
        let mut audit = FirstRunAudit::new(test_package_id(), declared, t0());

        // Record in non-alphabetical order
        audit.record(make_obs(
            "cap.z",
            t0() + Duration::seconds(1),
            ObservationSource::L3CapabilityRuntime,
        ));
        audit.record(make_obs(
            "cap.m",
            t0() + Duration::seconds(2),
            ObservationSource::L4VaultBroker,
        ));
        audit.record(make_obs(
            "cap.a",
            t0() + Duration::seconds(3),
            ObservationSource::L8NetworkPolicy,
        ));

        let outcome = audit.evaluate(t0() + Duration::seconds(61));
        assert_eq!(
            outcome,
            AuditOutcome::Failed {
                // drift must be sorted: cap.m before cap.z
                drift: vec!["cap.m".to_string(), "cap.z".to_string()]
            }
        );
    }

    #[test]
    fn each_observation_source_recorded_correctly() {
        let declared = make_declared(&["cap.a", "cap.b", "cap.c", "cap.d"]);
        let mut audit = FirstRunAudit::new(test_package_id(), declared, t0());

        // One observation from each of the four sources
        audit.record(make_obs(
            "cap.a",
            t0() + Duration::seconds(1),
            ObservationSource::L3CapabilityRuntime,
        ));
        audit.record(make_obs(
            "cap.b",
            t0() + Duration::seconds(2),
            ObservationSource::L4VaultBroker,
        ));
        audit.record(make_obs(
            "cap.c",
            t0() + Duration::seconds(3),
            ObservationSource::L8NetworkPolicy,
        ));
        audit.record(make_obs(
            "cap.d",
            t0() + Duration::seconds(4),
            ObservationSource::L7Renderer,
        ));

        // All declared → Passed
        let outcome = audit.evaluate(t0() + Duration::seconds(61));
        assert_eq!(outcome, AuditOutcome::Passed);

        // Verify all four were recorded
        assert!(audit.observed.contains("cap.a"));
        assert!(audit.observed.contains("cap.b"));
        assert!(audit.observed.contains("cap.c"));
        assert!(audit.observed.contains("cap.d"));
    }

    #[test]
    fn apply_audit_outcome_passed_returns_active_none() {
        let mut state = PackageInstallState::Active;
        let outcome = AuditOutcome::Passed;

        let (final_state, result) = apply_audit_outcome(&mut state, &outcome);
        assert_eq!(final_state, PackageInstallState::Active);
        assert_eq!(result, None);
        assert_eq!(state, PackageInstallState::Active);
    }

    #[test]
    fn apply_audit_outcome_failed_returns_quarantined_some_capability_lie() {
        let mut state = PackageInstallState::Active;
        let outcome = AuditOutcome::Failed {
            drift: vec!["cap.undeclared".to_string()],
        };

        let (final_state, result) = apply_audit_outcome(&mut state, &outcome);
        assert_eq!(final_state, PackageInstallState::Quarantined);
        assert_eq!(result, Some(PackageVerificationResult::CapabilityLie));
        assert_eq!(state, PackageInstallState::Quarantined);
    }

    #[test]
    fn per_install_freshness_two_instances_independent() {
        // §9.3: per-install (not per-package-id) — fresh audit each install.
        let declared = make_declared(&["cap.a"]);

        let mut audit1 = FirstRunAudit::new(test_package_id(), declared.clone(), t0());
        let mut audit2 = FirstRunAudit::new(test_package_id(), declared, t0());

        // audit1 records an undeclared capability → should fail
        audit1.record(make_obs(
            "cap.undeclared",
            t0() + Duration::seconds(10),
            ObservationSource::L3CapabilityRuntime,
        ));

        // audit2 records only declared capabilities → should pass
        audit2.record(make_obs(
            "cap.a",
            t0() + Duration::seconds(10),
            ObservationSource::L4VaultBroker,
        ));

        let outcome1 = audit1.evaluate(t0() + Duration::seconds(61));
        let outcome2 = audit2.evaluate(t0() + Duration::seconds(61));

        assert_eq!(
            outcome1,
            AuditOutcome::Failed {
                drift: vec!["cap.undeclared".to_string()]
            }
        );
        assert_eq!(outcome2, AuditOutcome::Passed);
    }
}
