use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount as EnumCountMacro, EnumIter};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCountMacro,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KeyCustody {
    TpmSealed,
    OperatorHeld,
    RecoveryEscrow,
    PerSubjectDerived,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCountMacro,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BackupSetState {
    Planned,
    Snapshotting,
    Encrypting,
    Writing,
    Verifying,
    Sealed,
    Failed,
    Expired,
    Shredded,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCountMacro,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RestoreMode {
    StagedTest,
    SelectiveObject,
    FullHostRebuild,
    CrossHostMigrate,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCountMacro,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DrScenario {
    DiskFailure,
    HostLossOrTheft,
    RansomwareOrTamper,
    ProfileCorruption,
    EvidenceTargetLoss,
    PlannedHardwareMigration,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCountMacro,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapsuleImportDecision {
    Accept,
    AcceptDegraded,
    Quarantine,
    BlockWithReason,
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::{EnumCount, IntoEnumIterator};

    #[test]
    fn key_custody_variant_count() {
        assert_eq!(KeyCustody::COUNT, 4);
    }

    #[test]
    fn backup_set_state_variant_count() {
        assert_eq!(BackupSetState::COUNT, 9);
    }

    #[test]
    fn restore_mode_variant_count() {
        assert_eq!(RestoreMode::COUNT, 4);
    }

    #[test]
    fn dr_scenario_variant_count() {
        assert_eq!(DrScenario::COUNT, 6);
    }

    #[test]
    fn capsule_import_decision_variant_count() {
        assert_eq!(CapsuleImportDecision::COUNT, 4);
    }

    #[test]
    fn key_custody_iter_all() {
        let variants: Vec<_> = KeyCustody::iter().collect();
        assert_eq!(variants.len(), 4);
    }

    #[test]
    fn serde_roundtrip_backup_set_state() {
        let states = [
            BackupSetState::Planned,
            BackupSetState::Snapshotting,
            BackupSetState::Encrypting,
            BackupSetState::Writing,
            BackupSetState::Verifying,
            BackupSetState::Sealed,
            BackupSetState::Failed,
            BackupSetState::Expired,
            BackupSetState::Shredded,
        ];
        for state in &states {
            let json = serde_json::to_string(state).expect("serialize");
            let back: BackupSetState = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*state, back);
        }
    }

    #[test]
    fn serde_roundtrip_capsule_import_decision() {
        let decisions = [
            CapsuleImportDecision::Accept,
            CapsuleImportDecision::AcceptDegraded,
            CapsuleImportDecision::Quarantine,
            CapsuleImportDecision::BlockWithReason,
        ];
        for d in &decisions {
            let json = serde_json::to_string(d).expect("serialize");
            let back: CapsuleImportDecision = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*d, back);
        }
    }
}
