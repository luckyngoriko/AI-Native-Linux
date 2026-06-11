use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::enums::RestoreMode;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestorePlan {
    pub plan_id: String,
    pub source_set_id: String,
    pub mode: RestoreMode,
    pub verify_integrity: bool,
    pub preserve_current_until_verified: bool,
}

impl RestorePlan {
    pub fn new(source_set_id: String, mode: RestoreMode) -> Self {
        let plan_id = format!("rplan_{}", Ulid::new());
        Self {
            plan_id,
            source_set_id,
            mode,
            verify_integrity: true,
            preserve_current_until_verified: true,
        }
    }

    pub fn requires_staging_sandbox(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_plan_always_verifies_integrity() {
        let plan = RestorePlan::new("bset_01".into(), RestoreMode::FullHostRebuild);
        assert!(plan.verify_integrity);
    }

    #[test]
    fn new_plan_always_preserves_current_until_verified() {
        let plan = RestorePlan::new("bset_01".into(), RestoreMode::CrossHostMigrate);
        assert!(plan.preserve_current_until_verified);
    }

    #[test]
    fn staging_sandbox_always_required() {
        let plan = RestorePlan::new("bset_01".into(), RestoreMode::StagedTest);
        assert!(plan.requires_staging_sandbox());

        let plan = RestorePlan::new("bset_01".into(), RestoreMode::SelectiveObject);
        assert!(plan.requires_staging_sandbox());
    }

    #[test]
    fn all_restore_modes_supported() {
        let modes = [
            RestoreMode::StagedTest,
            RestoreMode::SelectiveObject,
            RestoreMode::FullHostRebuild,
            RestoreMode::CrossHostMigrate,
        ];
        for mode in &modes {
            let plan = RestorePlan::new("bset_01".into(), *mode);
            assert_eq!(plan.mode, *mode);
        }
    }

    #[test]
    fn restore_plan_has_distinct_ids() {
        let plan1 = RestorePlan::new("bset_01".into(), RestoreMode::FullHostRebuild);
        let plan2 = RestorePlan::new("bset_02".into(), RestoreMode::SelectiveObject);
        assert_ne!(plan1.plan_id, plan2.plan_id);
    }
}
