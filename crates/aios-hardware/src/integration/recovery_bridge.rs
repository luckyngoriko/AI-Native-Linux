//! Recovery bridge: drift detection → recovery-mode signal.
//!
//! Translates an `EvilMaidEvidenceMarker` into a typed boolean signal indicating
//! whether the system should enter recovery mode. The actual recovery-mode flip is
//! operator-mediated; this bridge provides the typed signal.

use crate::drift::{EvilMaidEvidenceMarker, EvilMaidRecommendedAction};

/// Returns `true` iff the drift evidence marker recommends entering recovery mode.
///
/// This is the typed signal — the actual recovery-mode flip is operator-mediated.
/// The caller (e.g. the L1 bootstrap layer) uses this as a policy input.
///
/// ## Constitutional invariants
///
/// `HARDWARE_GRAPH_DRIFT_FOREVER` is the L0 constitutional signal that hardware
/// tampering was detected.  When this function returns `true`, the system MUST:
/// 1. Emit a FOREVER-retention evidence receipt.
/// 2. Enter operator-mediated recovery before any further typed actions execute.
#[must_use]
pub fn drift_should_trigger_recovery(marker: &EvilMaidEvidenceMarker) -> bool {
    marker.recommended_action == EvilMaidRecommendedAction::EnterRecoveryMode
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test"
)]
mod tests {
    use super::*;
    use crate::drift::GraphDiff;
    use crate::ids::{DeviceId, HardwareGraphId};

    fn make_marker(action: EvilMaidRecommendedAction) -> EvilMaidEvidenceMarker {
        EvilMaidEvidenceMarker {
            prior: HardwareGraphId("pg_01".into()),
            current: HardwareGraphId("cg_01".into()),
            diff: GraphDiff {
                added: vec![DeviceId("new_dev".into())],
                removed: vec![],
                modified: vec![],
                kept: 5,
            },
            detected_at: chrono::Utc::now(),
            recommended_action: action,
        }
    }

    #[test]
    fn enter_recovery_mode_triggers_recovery() {
        let marker = make_marker(EvilMaidRecommendedAction::EnterRecoveryMode);
        assert!(drift_should_trigger_recovery(&marker));
    }

    #[test]
    fn operator_investigation_does_not_trigger() {
        let marker = make_marker(EvilMaidRecommendedAction::OperatorInvestigation);
        assert!(!drift_should_trigger_recovery(&marker));
    }

    #[test]
    fn auto_quarantine_new_devices_does_not_trigger() {
        let marker = make_marker(EvilMaidRecommendedAction::AutoQuarantineNewDevices);
        assert!(!drift_should_trigger_recovery(&marker));
    }

    #[test]
    fn recovery_trigger_is_deterministic() {
        let m1 = make_marker(EvilMaidRecommendedAction::EnterRecoveryMode);
        let m2 = make_marker(EvilMaidRecommendedAction::EnterRecoveryMode);
        assert_eq!(
            drift_should_trigger_recovery(&m1),
            drift_should_trigger_recovery(&m2)
        );
    }
}
