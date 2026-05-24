//! S4.1 namespace layout and mutation admission coverage.

use aios_fs::{AiosPath, FsError, NamespaceClass, NamespacePolicy, SubjectRef};
use strum::{EnumCount, IntoEnumIterator};

fn subject(id: &str) -> SubjectRef {
    SubjectRef(id.to_owned())
}

#[test]
fn namespace_class_count_matches_s4_1_wave8_catalog_plus_bare_scopes() {
    assert_eq!(NamespaceClass::COUNT, 52);
}

#[test]
fn every_namespace_class_variant_is_iterable() {
    let variants: Vec<NamespaceClass> = NamespaceClass::iter().collect();

    assert_eq!(variants.len(), NamespaceClass::COUNT);
    assert_eq!(
        variants,
        vec![
            NamespaceClass::System,
            NamespaceClass::SystemApps,
            NamespaceClass::SystemAgents,
            NamespaceClass::SystemPolicy,
            NamespaceClass::SystemCapabilities,
            NamespaceClass::SystemEvidence,
            NamespaceClass::SystemVault,
            NamespaceClass::SystemRuntime,
            NamespaceClass::SystemRecovery,
            NamespaceClass::SystemBoot,
            NamespaceClass::SystemFirstboot,
            NamespaceClass::SystemGovernance,
            NamespaceClass::SystemIdentity,
            NamespaceClass::SystemKernel,
            NamespaceClass::SystemHardware,
            NamespaceClass::SystemDrivers,
            NamespaceClass::SystemFirmware,
            NamespaceClass::SystemNetwork,
            NamespaceClass::SystemSgr,
            NamespaceClass::SystemUnits,
            NamespaceClass::SystemRunbooks,
            NamespaceClass::SystemThemes,
            NamespaceClass::SystemRenderers,
            NamespaceClass::SystemWeb,
            NamespaceClass::SystemDistribution,
            NamespaceClass::Groups,
            NamespaceClass::Group,
            NamespaceClass::GroupApps,
            NamespaceClass::GroupAgents,
            NamespaceClass::GroupUsers,
            NamespaceClass::GroupShared,
            NamespaceClass::GroupProjects,
            NamespaceClass::GroupDatasets,
            NamespaceClass::GroupInbox,
            NamespaceClass::GroupPolicy,
            NamespaceClass::GroupEvidence,
            NamespaceClass::GroupVault,
            NamespaceClass::GroupAudit,
            NamespaceClass::GroupServices,
            NamespaceClass::GroupSystem,
            NamespaceClass::User,
            NamespaceClass::UserHome,
            NamespaceClass::UserAgents,
            NamespaceClass::UserPrefs,
            NamespaceClass::UserDesktop,
            NamespaceClass::UserInbox,
            NamespaceClass::UserOutbox,
            NamespaceClass::UserDrafts,
            NamespaceClass::UserTrust,
            NamespaceClass::UserApps,
            NamespaceClass::UserRuntime,
            NamespaceClass::UserExports,
        ]
    );
}

#[test]
fn classifies_system_namespace() {
    let path = AiosPath::new("/aios/system/policy/bundle.aios");

    assert_eq!(path.namespace_class(), Some(NamespaceClass::SystemPolicy));
}

#[test]
fn classifies_group_namespace() {
    let path = AiosPath::new("/aios/groups/family/shared/photos");

    assert_eq!(path.namespace_class(), Some(NamespaceClass::GroupShared));
}

#[test]
fn classifies_work_group_namespace() {
    let path = AiosPath::new("/aios/groups/work/projects/roadmap");

    assert_eq!(path.namespace_class(), Some(NamespaceClass::GroupProjects));
}

#[test]
fn rejects_foreign_path() {
    let path = AiosPath::new("/etc/passwd");

    assert_eq!(path.namespace_class(), None);
}

#[test]
fn classifies_wave8_system_group_and_user_paths() {
    assert_eq!(
        AiosPath::new("/aios/system/firmware/counter").namespace_class(),
        Some(NamespaceClass::SystemFirmware)
    );
    assert_eq!(
        AiosPath::new("/aios/groups/homelab/services/dns").namespace_class(),
        Some(NamespaceClass::GroupServices)
    );
    assert_eq!(
        AiosPath::new("/aios/groups/family/users/alice/runtime/wine").namespace_class(),
        Some(NamespaceClass::UserRuntime)
    );
}

#[test]
fn recovery_only_mutation_classes_match_system_boundary() {
    for class in [
        NamespaceClass::SystemPolicy,
        NamespaceClass::SystemCapabilities,
        NamespaceClass::SystemVault,
        NamespaceClass::SystemRecovery,
        NamespaceClass::SystemBoot,
        NamespaceClass::SystemFirstboot,
        NamespaceClass::SystemGovernance,
        NamespaceClass::SystemIdentity,
        NamespaceClass::SystemKernel,
        NamespaceClass::SystemFirmware,
    ] {
        assert!(class.is_recovery_only_mutation(), "{class:?}");
    }
}

#[test]
fn non_recovery_only_mutation_classes_remain_normal_mode_mutable() {
    for class in [
        NamespaceClass::SystemApps,
        NamespaceClass::SystemAgents,
        NamespaceClass::GroupShared,
        NamespaceClass::UserHome,
    ] {
        assert!(!class.is_recovery_only_mutation(), "{class:?}");
    }
}

#[test]
fn ai_locked_classes_are_read_only_for_ai() {
    for class in [
        NamespaceClass::System,
        NamespaceClass::SystemApps,
        NamespaceClass::SystemAgents,
        NamespaceClass::SystemPolicy,
        NamespaceClass::SystemBoot,
        NamespaceClass::GroupSystem,
    ] {
        assert!(class.is_read_only_for_ai(), "{class:?}");
    }
}

#[test]
fn normal_user_space_is_not_read_only_for_ai_at_namespace_layer() {
    for class in [NamespaceClass::GroupShared, NamespaceClass::UserHome] {
        assert!(!class.is_read_only_for_ai(), "{class:?}");
    }
}

#[test]
fn human_subject_can_mutate_user_space_path() {
    let path = AiosPath::new("/aios/groups/family/users/alice/home/notes.md");

    assert_eq!(
        NamespacePolicy::can_mutate(&path, &subject("family:alice"), false, false),
        Ok(())
    );
}

#[test]
fn ai_subject_cannot_mutate_ai_locked_path() -> Result<(), String> {
    let path = AiosPath::new("/aios/system/apps/evidence-viewer");

    let Err(err) = NamespacePolicy::can_mutate(&path, &subject("agent:coder"), false, true) else {
        return Err("AI system mutation must be denied".to_owned());
    };

    assert!(matches!(
        err,
        FsError::NamespaceMutationDenied { path, reason }
            if path == "/aios/system/apps/evidence-viewer"
                && reason.contains("AI subjects cannot mutate")
    ));

    Ok(())
}

#[test]
fn non_recovery_subject_cannot_mutate_recovery_only_path() -> Result<(), String> {
    let path = AiosPath::new("/aios/system/policy/active.bundle");

    let Err(err) = NamespacePolicy::can_mutate(&path, &subject("family:alice"), false, false)
    else {
        return Err("normal-mode policy mutation must be denied".to_owned());
    };

    assert!(matches!(
        err,
        FsError::NamespaceMutationDenied { path, reason }
            if path == "/aios/system/policy/active.bundle"
                && reason.contains("recovery mode required")
    ));

    Ok(())
}

#[test]
fn recovery_subject_can_mutate_recovery_only_path() {
    let path = AiosPath::new("/aios/system/policy/active.bundle");

    assert_eq!(
        NamespacePolicy::can_mutate(&path, &subject("_system:recovery:operator"), true, false),
        Ok(())
    );
}

#[test]
fn evidence_grade_floor_is_valid_for_every_namespace_class() {
    for class in NamespaceClass::iter() {
        let floor = class.evidence_grade_floor();
        assert!(
            matches!(floor.as_str(), "E3" | "E4"),
            "{class:?} returned {}",
            floor.as_str()
        );
    }
}

#[test]
fn evidence_grade_floor_distinguishes_recovery_critical_paths() {
    assert_eq!(
        NamespaceClass::SystemPolicy.evidence_grade_floor().as_str(),
        "E4"
    );
    assert_eq!(
        NamespaceClass::UserHome.evidence_grade_floor().as_str(),
        "E3"
    );
}

#[test]
fn inv_022_enforcement_denies_ai_boot_config_mutation_envelope() -> Result<(), String> {
    struct MutationEnvelope {
        target_path: AiosPath,
        subject: SubjectRef,
        recovery_mode: bool,
        is_ai: bool,
    }

    let envelope = MutationEnvelope {
        target_path: AiosPath::new("/aios/system/boot-config"),
        subject: subject("agent:planner"),
        recovery_mode: false,
        is_ai: true,
    };

    let Err(err) = NamespacePolicy::can_mutate(
        &envelope.target_path,
        &envelope.subject,
        envelope.recovery_mode,
        envelope.is_ai,
    ) else {
        return Err("AI boot-config mutation must be denied".to_owned());
    };

    assert!(matches!(
        err,
        FsError::NamespaceMutationDenied { path, reason }
            if path == "/aios/system/boot-config"
                && reason.contains("AI subjects cannot mutate")
    ));

    Ok(())
}

#[test]
fn path_classification_and_policy_compose_for_allowed_mutation() {
    let path = AiosPath::new("/aios/groups/family/users/alice/apps/editor");

    assert_eq!(path.namespace_class(), Some(NamespaceClass::UserApps));
    assert_eq!(
        NamespacePolicy::can_mutate(&path, &subject("family:alice"), false, false),
        Ok(())
    );
}
