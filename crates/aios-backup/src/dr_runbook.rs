use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::enums::DrScenario;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrRunbook {
    pub runbook_id: String,
    pub host_id: String,
    pub scenario: DrScenario,
    pub ai_may_execute: bool,
    pub requires_human_approver: bool,
}

impl DrRunbook {
    pub fn new(host_id: String, scenario: DrScenario) -> Self {
        let runbook_id = format!("drrb_{}", Ulid::new());
        Self {
            runbook_id,
            host_id,
            scenario,
            ai_may_execute: false,
            requires_human_approver: true,
        }
    }

    pub fn is_ai_allowed_to_execute(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_never_allowed_to_execute() {
        let runbook = DrRunbook::new("host-1".into(), DrScenario::DiskFailure);
        assert!(!runbook.is_ai_allowed_to_execute());
        assert!(!runbook.ai_may_execute);
    }

    #[test]
    fn human_approver_always_required() {
        let runbook = DrRunbook::new("host-1".into(), DrScenario::RansomwareOrTamper);
        assert!(runbook.requires_human_approver);
    }

    #[test]
    fn all_scenarios_require_human() {
        let scenarios = [
            DrScenario::DiskFailure,
            DrScenario::HostLossOrTheft,
            DrScenario::RansomwareOrTamper,
            DrScenario::ProfileCorruption,
            DrScenario::EvidenceTargetLoss,
            DrScenario::PlannedHardwareMigration,
        ];
        for sc in &scenarios {
            let runbook = DrRunbook::new("host-1".into(), *sc);
            assert!(!runbook.is_ai_allowed_to_execute());
            assert!(runbook.requires_human_approver);
        }
    }

    #[test]
    fn runbook_ids_are_unique() {
        let rb1 = DrRunbook::new("host-1".into(), DrScenario::DiskFailure);
        let rb2 = DrRunbook::new("host-1".into(), DrScenario::DiskFailure);
        assert_ne!(rb1.runbook_id, rb2.runbook_id);
    }
}
