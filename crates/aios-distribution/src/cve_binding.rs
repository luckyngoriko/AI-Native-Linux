//! CVE binding enforcement levels — local shape mirroring M18 `aios-integration`
//! `CveFeedShape`.
//!
//! This module provides a **local** copy of the CVE enforcement shape used by
//! the distribution layer. Cross-crate reconciliation with the authoritative
//! `aios-integration` crate (re-export or mapping adapter) is deferred to
//! **T-197**.
//!
//! # Enforcement tiers
//!
//! | CVSS range        | [`CveEnforcementLevel`] | Action                          |
//! |-------------------|--------------------------|---------------------------------|
//! | `cvss < 4.0`      | `Monitor`                | Record only                     |
//! | `4.0 ≤ cvss < 7.0`| `Notify`                 | Notify operator                 |
//! | `7.0 ≤ cvss < 9.0`| `QuarantineCandidate`    | Flag for operator review        |
//! | `cvss ≥ 9.0`      | `AutoQuarantine`         | Auto-quarantine the package     |
//!
//! Only `AutoQuarantine` triggers an automatic `Active → Quarantined` state
//! transition. All other levels are informational at this layer.

use crate::ids::PackageId;
use crate::install_fsm;
use crate::install_state::PackageInstallState;

// ---------------------------------------------------------------------------
// CveEnforcementLevel — 4-tier CVSS enforcement
// ---------------------------------------------------------------------------

/// CVE enforcement tiers derived from CVSS v3.x base score.
///
/// This LOCAL enum mirrors the M18 `aios-integration` `CveFeedShape` shape.
/// T-197 will reconcile the two (re-export or adapter mapping).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CveEnforcementLevel {
    /// CVSS < 4.0 — record the binding; no operator action required.
    Monitor,
    /// 4.0 ≤ CVSS < 7.0 — notify the operator; no automatic state transition.
    Notify,
    /// 7.0 ≤ CVSS < 9.0 — flag for operator review; no automatic transition.
    QuarantineCandidate,
    /// CVSS ≥ 9.0 — automatic quarantine: `Active → Quarantined`.
    AutoQuarantine,
}

impl CveEnforcementLevel {
    /// Derives the enforcement level from a CVSS v3.x base score.
    ///
    /// # Thresholds (per S11.1 §CVE / M18 shape)
    ///
    /// - `cvss < 4.0` → [`Monitor`](Self::Monitor)
    /// - `4.0 ≤ cvss < 7.0` → [`Notify`](Self::Notify)
    /// - `7.0 ≤ cvss < 9.0` → [`QuarantineCandidate`](Self::QuarantineCandidate)
    /// - `cvss ≥ 9.0` → [`AutoQuarantine`](Self::AutoQuarantine)
    ///
    /// # Panics
    ///
    /// Never panics. All `f32` values (including NaN, infinity) are handled:
    /// NaN and negative values fall into the `Monitor` bucket (cvss < 4.0).
    #[must_use]
    pub fn from_cvss(cvss: f32) -> Self {
        if cvss >= 9.0 {
            Self::AutoQuarantine
        } else if cvss >= 7.0 {
            Self::QuarantineCandidate
        } else if cvss >= 4.0 {
            Self::Notify
        } else {
            Self::Monitor
        }
    }

    /// Returns a human-readable label for this enforcement level.
    ///
    /// All four labels are non-empty `&'static str` values suitable for
    /// logging, operator dashboards, and evidence records.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Monitor => "monitor",
            Self::Notify => "notify",
            Self::QuarantineCandidate => "quarantine_candidate",
            Self::AutoQuarantine => "auto_quarantine",
        }
    }
}

// ---------------------------------------------------------------------------
// PackageCveBinding — a package-to-CVE relationship
// ---------------------------------------------------------------------------

/// A binding between a package and a known CVE.
///
/// Created when the CVE feed matches a package version. The `enforcement`
/// field is derived from `cvss` via [`CveEnforcementLevel::from_cvss`].
#[derive(Debug, Clone, PartialEq)]
pub struct PackageCveBinding {
    /// The package affected by the CVE.
    pub package_id: PackageId,
    /// The CVE identifier (e.g. `CVE-2026-12345`).
    pub cve_id: String,
    /// The CVSS v3.x base score for this CVE.
    pub cvss: f32,
    /// The enforcement level derived from `cvss`.
    pub enforcement: CveEnforcementLevel,
}

impl PackageCveBinding {
    /// Creates a new CVE binding, deriving the [`CveEnforcementLevel`] from
    /// the CVSS score via [`CveEnforcementLevel::from_cvss`].
    #[must_use]
    pub fn new(package_id: PackageId, cve_id: String, cvss: f32) -> Self {
        let enforcement = CveEnforcementLevel::from_cvss(cvss);
        Self {
            package_id,
            cve_id,
            cvss,
            enforcement,
        }
    }
}

// ---------------------------------------------------------------------------
// CveAction — the action taken in response to a CVE binding
// ---------------------------------------------------------------------------

/// The action taken by [`apply_cve_binding`] in response to a
/// [`PackageCveBinding`].
///
/// Only [`Quarantined`](Self::Quarantined) triggers a state transition
/// (`Active → Quarantined`). All other actions are informational.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CveAction {
    /// CVE recorded in the evidence log; no state change.
    Recorded,
    /// Operator notified; no state change.
    Notified,
    /// Package flagged for operator review; no automatic state change.
    QuarantineCandidate,
    /// Package auto-quarantined (`Active → Quarantined` via FSM).
    Quarantined,
}

// ---------------------------------------------------------------------------
// apply_cve_binding — FSM integration
// ---------------------------------------------------------------------------

/// Applies a CVE binding to the package install state.
///
/// # State transitions
///
/// | [`CveEnforcementLevel`] | State change               | Returned [`CveAction`]     |
/// |--------------------------|----------------------------|----------------------------|
/// | `AutoQuarantine`         | `Active → Quarantined`     | `Quarantined`              |
/// | `QuarantineCandidate`    | None                       | `QuarantineCandidate`      |
/// | `Notify`                 | None                       | `Notified`                 |
/// | `Monitor`                | None                       | `Recorded`                 |
///
/// Only `AutoQuarantine` triggers an automatic FSM transition. `QuarantineCandidate`
/// is flagged for operator review but does not auto-transition. `Notify` and
/// `Monitor` are purely informational at this layer.
#[must_use]
pub fn apply_cve_binding(
    state: &mut PackageInstallState,
    binding: &PackageCveBinding,
) -> CveAction {
    match binding.enforcement {
        CveEnforcementLevel::AutoQuarantine => {
            // Active → Quarantined is a valid FSM transition per §3.6.
            let _ = install_fsm::apply(state, PackageInstallState::Quarantined);
            CveAction::Quarantined
        }
        CveEnforcementLevel::QuarantineCandidate => CveAction::QuarantineCandidate,
        CveEnforcementLevel::Notify => CveAction::Notified,
        CveEnforcementLevel::Monitor => CveAction::Recorded,
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

    fn test_package_id() -> PackageId {
        PackageId("pkg:test:cve-app".into())
    }

    // ── from_cvss thresholds ─────────────────────────────────────────────

    #[test]
    fn from_cvss_3_9_is_monitor() {
        assert_eq!(
            CveEnforcementLevel::from_cvss(3.9),
            CveEnforcementLevel::Monitor
        );
    }

    #[test]
    fn from_cvss_4_0_is_notify() {
        assert_eq!(
            CveEnforcementLevel::from_cvss(4.0),
            CveEnforcementLevel::Notify
        );
    }

    #[test]
    fn from_cvss_6_9_is_notify() {
        assert_eq!(
            CveEnforcementLevel::from_cvss(6.9),
            CveEnforcementLevel::Notify
        );
    }

    #[test]
    fn from_cvss_7_0_is_quarantine_candidate() {
        assert_eq!(
            CveEnforcementLevel::from_cvss(7.0),
            CveEnforcementLevel::QuarantineCandidate
        );
    }

    #[test]
    fn from_cvss_8_9_is_quarantine_candidate() {
        assert_eq!(
            CveEnforcementLevel::from_cvss(8.9),
            CveEnforcementLevel::QuarantineCandidate
        );
    }

    #[test]
    fn from_cvss_9_0_is_auto_quarantine() {
        assert_eq!(
            CveEnforcementLevel::from_cvss(9.0),
            CveEnforcementLevel::AutoQuarantine
        );
    }

    #[test]
    fn from_cvss_10_0_is_auto_quarantine() {
        assert_eq!(
            CveEnforcementLevel::from_cvss(10.0),
            CveEnforcementLevel::AutoQuarantine
        );
    }

    // ── PackageCveBinding::new ────────────────────────────────────────────

    #[test]
    fn package_cve_binding_new_derives_enforcement_from_cvss() {
        let binding = PackageCveBinding::new(test_package_id(), "CVE-2026-12345".into(), 9.8);
        assert_eq!(binding.package_id, test_package_id());
        assert_eq!(binding.cve_id, "CVE-2026-12345");
        // cvss 9.8 ≥ 9.0 → AutoQuarantine
        assert_eq!(binding.enforcement, CveEnforcementLevel::AutoQuarantine);
        // Verify the float is stored as-is
        assert!((binding.cvss - 9.8).abs() < 1e-6);
    }

    // ── apply_cve_binding ─────────────────────────────────────────────────

    #[test]
    fn apply_cve_binding_auto_quarantine_transitions_to_quarantined() {
        let mut state = PackageInstallState::Active;
        let binding = PackageCveBinding::new(test_package_id(), "CVE-2026-CRITICAL".into(), 9.5);

        let action = apply_cve_binding(&mut state, &binding);
        assert_eq!(action, CveAction::Quarantined);
        assert_eq!(state, PackageInstallState::Quarantined);
    }

    #[test]
    fn apply_cve_binding_quarantine_candidate_no_transition() {
        let mut state = PackageInstallState::Active;
        let binding = PackageCveBinding::new(test_package_id(), "CVE-2026-HIGH".into(), 7.5);

        let action = apply_cve_binding(&mut state, &binding);
        assert_eq!(action, CveAction::QuarantineCandidate);
        assert_eq!(state, PackageInstallState::Active);
    }

    #[test]
    fn apply_cve_binding_notify_returns_notified() {
        let mut state = PackageInstallState::Active;
        let binding = PackageCveBinding::new(test_package_id(), "CVE-2026-MEDIUM".into(), 5.0);

        let action = apply_cve_binding(&mut state, &binding);
        assert_eq!(action, CveAction::Notified);
        assert_eq!(state, PackageInstallState::Active);
    }

    #[test]
    fn apply_cve_binding_monitor_returns_recorded() {
        let mut state = PackageInstallState::Active;
        let binding = PackageCveBinding::new(test_package_id(), "CVE-2026-LOW".into(), 2.0);

        let action = apply_cve_binding(&mut state, &binding);
        assert_eq!(action, CveAction::Recorded);
        assert_eq!(state, PackageInstallState::Active);
    }

    // ── CveEnforcementLevel::label ────────────────────────────────────────

    #[test]
    fn cve_enforcement_level_label_non_empty_for_all() {
        let levels = [
            CveEnforcementLevel::Monitor,
            CveEnforcementLevel::Notify,
            CveEnforcementLevel::QuarantineCandidate,
            CveEnforcementLevel::AutoQuarantine,
        ];
        for level in &levels {
            let label = level.label();
            assert!(!label.is_empty(), "label for {level:?} is empty");
        }
    }
}
