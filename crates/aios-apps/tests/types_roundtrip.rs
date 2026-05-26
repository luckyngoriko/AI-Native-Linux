#![allow(clippy::expect_used, clippy::panic)]

//! Round-trip tests for all closed enums in the aios-apps crate.
//!
//! Every test serializes each enum variant to JSON, deserializes back,
//! and asserts equality. This validates the `SCREAMING_SNAKE_CASE` wire
//! format and the closed variant set.
//!
//! Conventions:
//! - `test_NN_name` ordering matches the spec files (S12.1, S12.2, S12.3, S12.4, S6.5).
//! - Each test body serializes every variant explicitly so a new variant
//!   addition forces a test update (closed-enum invariant).

use aios_apps::*;
use strum::{EnumCount, IntoEnumIterator};

// ============================================================================
// S12.1 App Runtime Model
// ============================================================================

#[test]
fn test_01_ecosystem_runtime_variants() {
    let expected: usize = EcosystemRuntime::COUNT;
    let iterated: Vec<_> = EcosystemRuntime::iter().collect();
    assert_eq!(iterated.len(), expected, "variant count mismatch");
    assert_eq!(
        expected, 12,
        "EcosystemRuntime must have exactly 12 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: EcosystemRuntime = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_02_ecosystem_honesty_class_variants() {
    let expected: usize = EcosystemHonestyClass::COUNT;
    let iterated: Vec<_> = EcosystemHonestyClass::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 4,
        "EcosystemHonestyClass must have exactly 4 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: EcosystemHonestyClass = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_03_manifest_translation_strategy_variants() {
    let expected: usize = ManifestTranslationStrategy::COUNT;
    let iterated: Vec<_> = ManifestTranslationStrategy::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 8,
        "ManifestTranslationStrategy must have exactly 8 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: ManifestTranslationStrategy = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_04_recipe_trust_class_variants() {
    let expected: usize = RecipeTrustClass::COUNT;
    let iterated: Vec<_> = RecipeTrustClass::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(expected, 4, "RecipeTrustClass must have exactly 4 variants");

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: RecipeTrustClass = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_05_manifest_delta_outcome_variants() {
    let expected: usize = ManifestDeltaOutcome::COUNT;
    let iterated: Vec<_> = ManifestDeltaOutcome::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 4,
        "ManifestDeltaOutcome must have exactly 4 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: ManifestDeltaOutcome = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

// ============================================================================
// S12.2 Package Object Model
// ============================================================================

#[test]
fn test_06_package_object_kind_variants() {
    let expected: usize = PackageObjectKind::COUNT;
    let iterated: Vec<_> = PackageObjectKind::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 8,
        "PackageObjectKind must have exactly 8 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: PackageObjectKind = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_07_package_content_kind_variants() {
    let expected: usize = PackageContentKind::COUNT;
    let iterated: Vec<_> = PackageContentKind::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 10,
        "PackageContentKind must have exactly 10 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: PackageContentKind = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_08_package_object_state_variants() {
    let expected: usize = PackageObjectState::COUNT;
    let iterated: Vec<_> = PackageObjectState::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 8,
        "PackageObjectState must have exactly 8 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: PackageObjectState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_09_rollback_kind_variants() {
    let expected: usize = RollbackKind::COUNT;
    let iterated: Vec<_> = RollbackKind::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(expected, 4, "RollbackKind must have exactly 4 variants");

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: RollbackKind = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_10_package_record_roundtrip() {
    let record = PackageRecord {
        package_id: PackageId("pkg_01JQEXAMPLE0000000000000000".into()),
        kind: PackageObjectKind::InstalledPackage,
        content_kinds: vec![
            PackageContentKind::CodeBinaries,
            PackageContentKind::DataAssets,
            PackageContentKind::SandboxProfile,
        ],
        state: PackageObjectState::Active,
        rollback_kind: RollbackKind::MultiVersion,
        installed_at: chrono::Utc::now(),
        state_changed_at: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&record).expect("serialize");
    let back: PackageRecord = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(record.package_id, back.package_id);
    assert_eq!(record.kind, back.kind);
    assert_eq!(record.state, back.state);
    assert_eq!(record.rollback_kind, back.rollback_kind);
}

#[test]
fn test_11_package_id_newtype_serde() {
    let id = PackageId("pkg_01JQEXAMPLE0000000000000000".into());
    let json = serde_json::to_string(&id).expect("serialize");
    let back: PackageId = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(id, back);
}

// ============================================================================
// S12.3 Compatibility Runtime
// ============================================================================

#[test]
fn test_12_orchestration_kind_variants() {
    let expected: usize = OrchestrationKind::COUNT;
    let iterated: Vec<_> = OrchestrationKind::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 8,
        "OrchestrationKind must have exactly 8 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: OrchestrationKind = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_13_launch_outcome_variants() {
    let expected: usize = LaunchOutcome::COUNT;
    let iterated: Vec<_> = LaunchOutcome::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(expected, 7, "LaunchOutcome must have exactly 7 variants");

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: LaunchOutcome = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_14_wine_prefix_kind_variants() {
    let expected: usize = WinePrefixKind::COUNT;
    let iterated: Vec<_> = WinePrefixKind::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(expected, 3, "WinePrefixKind must have exactly 3 variants");

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: WinePrefixKind = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_15_waydroid_isolation_level_variants() {
    let expected: usize = WaydroidIsolationLevel::COUNT;
    let iterated: Vec<_> = WaydroidIsolationLevel::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 3,
        "WaydroidIsolationLevel must have exactly 3 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: WaydroidIsolationLevel = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_16_vm_fallback_kind_variants() {
    let expected: usize = VMFallbackKind::COUNT;
    let iterated: Vec<_> = VMFallbackKind::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(expected, 4, "VMFallbackKind must have exactly 4 variants");

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: VMFallbackKind = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

// ============================================================================
// S12.4 Compatibility Knowledge
// ============================================================================

#[test]
fn test_17_compatibility_rating_variants() {
    let expected: usize = CompatibilityRating::COUNT;
    let iterated: Vec<_> = CompatibilityRating::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 5,
        "CompatibilityRating must have exactly 5 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: CompatibilityRating = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_18_rating_dimension_variants() {
    let expected: usize = RatingDimension::COUNT;
    let iterated: Vec<_> = RatingDimension::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(expected, 8, "RatingDimension must have exactly 8 variants");

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: RatingDimension = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_19_evidence_level_variants() {
    let expected: usize = EvidenceLevel::COUNT;
    let iterated: Vec<_> = EvidenceLevel::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(expected, 4, "EvidenceLevel must have exactly 4 variants");

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: EvidenceLevel = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_20_profile_visibility_variants() {
    let expected: usize = ProfileVisibility::COUNT;
    let iterated: Vec<_> = ProfileVisibility::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 3,
        "ProfileVisibility must have exactly 3 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: ProfileVisibility = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_21_profile_retired_reason_variants() {
    let expected: usize = ProfileRetiredReason::COUNT;
    let iterated: Vec<_> = ProfileRetiredReason::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 6,
        "ProfileRetiredReason must have exactly 6 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: ProfileRetiredReason = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_22_known_issue_class_variants() {
    let expected: usize = KnownIssueClass::COUNT;
    let iterated: Vec<_> = KnownIssueClass::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 11,
        "KnownIssueClass must have exactly 11 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: KnownIssueClass = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_23_app_profile_roundtrip() {
    let profile = AppProfile {
        app_id: "app:factorio".into(),
        ecosystem_runtime: EcosystemRuntime::RuntimeWindowsProton,
        current_recipe_trust_class: RecipeTrustClass::RecipeAiosCurated,
        headline_rating: CompatibilityRating::Platinum,
        headline_evidence_level: EvidenceLevel::MultiOperatorCorroborated,
        worst_dimension: RatingDimension::SaveStateCorrectness,
        ecosystem_honesty_class: EcosystemHonestyClass::PartiallySupported,
    };
    let json = serde_json::to_string(&profile).expect("serialize");
    let back: AppProfile = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(profile.app_id, back.app_id);
    assert_eq!(profile.ecosystem_runtime, back.ecosystem_runtime);
    assert_eq!(profile.headline_rating, back.headline_rating);
    assert_eq!(profile.worst_dimension, back.worst_dimension);
}

// ============================================================================
// S6.5 Session Container Model
// ============================================================================

#[test]
fn test_24_session_container_mode_variants() {
    let expected: usize = SessionContainerMode::COUNT;
    let iterated: Vec<_> = SessionContainerMode::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 2,
        "SessionContainerMode must have exactly 2 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: SessionContainerMode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_25_session_container_state_variants() {
    let expected: usize = SessionContainerState::COUNT;
    let iterated: Vec<_> = SessionContainerState::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 5,
        "SessionContainerState must have exactly 5 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: SessionContainerState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_26_session_container_runtime_variants() {
    let expected: usize = SessionContainerRuntime::COUNT;
    let iterated: Vec<_> = SessionContainerRuntime::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 2,
        "SessionContainerRuntime must have exactly 2 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: SessionContainerRuntime = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_27_stream_protocol_variants() {
    let expected: usize = StreamProtocol::COUNT;
    let iterated: Vec<_> = StreamProtocol::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(expected, 2, "StreamProtocol must have exactly 2 variants");

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: StreamProtocol = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_28_session_failure_class_variants() {
    let expected: usize = SessionFailureClass::COUNT;
    let iterated: Vec<_> = SessionFailureClass::iter().collect();
    assert_eq!(iterated.len(), expected);
    assert_eq!(
        expected, 8,
        "SessionFailureClass must have exactly 8 variants"
    );

    for variant in &iterated {
        let json = serde_json::to_string(variant).expect("serialize");
        let back: SessionFailureClass = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, back, "round-trip failed for {variant}");
    }
}

#[test]
fn test_29_session_record_roundtrip() {
    let record = SessionRecord {
        session_id: SessionId("sess_01JQEXAMPLE0000000000000000".into()),
        group_id: "family".into(),
        mode: SessionContainerMode::FullDesktop,
        state: SessionContainerState::Active,
        runtime: SessionContainerRuntime::Podman,
        stream_protocol: StreamProtocol::Websocket,
        created_at: chrono::Utc::now(),
        state_changed_at: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&record).expect("serialize");
    let back: SessionRecord = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(record.session_id, back.session_id);
    assert_eq!(record.mode, back.mode);
    assert_eq!(record.state, back.state);
    assert_eq!(record.runtime, back.runtime);
    assert_eq!(record.stream_protocol, back.stream_protocol);
}

#[test]
fn test_30_session_id_newtype_serde() {
    let id = SessionId("sess_01JQEXAMPLE0000000000000000".into());
    let json = serde_json::to_string(&id).expect("serialize");
    let back: SessionId = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(id, back);
}

// ============================================================================
// Wire format: SCREAMING_SNAKE_CASE
// ============================================================================

#[test]
fn test_31_wire_format_screaming_snake_case() {
    // Verify the serialized wire form uses SCREAMING_SNAKE_CASE
    let json = serde_json::to_string(&PackageObjectKind::InstalledPackage).expect("serialize");
    assert_eq!(json, r#""INSTALLED_PACKAGE""#);

    let json = serde_json::to_string(&CompatibilityRating::Platinum).expect("serialize");
    assert_eq!(json, r#""PLATINUM""#);

    let json = serde_json::to_string(&SessionContainerState::Active).expect("serialize");
    assert_eq!(json, r#""ACTIVE""#);

    let json = serde_json::to_string(&OrchestrationKind::WinePrefixNew).expect("serialize");
    assert_eq!(json, r#""WINE_PREFIX_NEW""#);

    let json = serde_json::to_string(&EcosystemRuntime::RuntimeLinuxNative).expect("serialize");
    assert_eq!(json, r#""RUNTIME_LINUX_NATIVE""#);
}

// ============================================================================
// deny_unknown_fields
// ============================================================================

#[test]
fn test_32_package_record_rejects_unknown_fields() {
    let json = r#"{"package_id":"pkg_01","kind":"INSTALLED_PACKAGE","content_kinds":[],"state":"ACTIVE","rollback_kind":"MULTI_VERSION","installed_at":"2026-05-09T00:00:00Z","state_changed_at":"2026-05-09T00:00:00Z","extra_field":"nope"}"#;
    let result: Result<PackageRecord, _> = serde_json::from_str(json);
    assert!(result.is_err(), "should reject unknown fields");
}

#[test]
fn test_33_session_record_rejects_unknown_fields() {
    let json = r#"{"session_id":"sess_01","group_id":"g","mode":"FULL_DESKTOP","state":"ACTIVE","runtime":"PODMAN","stream_protocol":"WEBSOCKET","created_at":"2026-05-09T00:00:00Z","state_changed_at":"2026-05-09T00:00:00Z","evil":"true"}"#;
    let result: Result<SessionRecord, _> = serde_json::from_str(json);
    assert!(result.is_err(), "should reject unknown fields");
}

#[test]
fn test_34_app_profile_rejects_unknown_fields() {
    let json = r#"{"app_id":"app:x","ecosystem_runtime":"RUNTIME_LINUX_NATIVE","current_recipe_trust_class":"RECIPE_AIOS_CURATED","headline_rating":"PLATINUM","headline_evidence_level":"MULTI_OPERATOR_CORROBORATED","worst_dimension":"LAUNCH_RELIABILITY","ecosystem_honesty_class":"FULLY_SUPPORTED","bonus_field":42}"#;
    let result: Result<AppProfile, _> = serde_json::from_str(json);
    assert!(result.is_err(), "should reject unknown fields");
}

// ============================================================================
// CompatibilityRating ordinal ordering
// ============================================================================

#[test]
fn test_35_compatibility_rating_ordinal() {
    assert!(CompatibilityRating::Platinum > CompatibilityRating::Gold);
    assert!(CompatibilityRating::Gold > CompatibilityRating::Silver);
    assert!(CompatibilityRating::Silver > CompatibilityRating::Bronze);
    assert!(CompatibilityRating::Bronze > CompatibilityRating::Borked);
}

// ============================================================================
// EvidenceLevel ordinal ordering
// ============================================================================

#[test]
fn test_36_evidence_level_ordinal() {
    assert!(EvidenceLevel::VerifiedPublisher > EvidenceLevel::MultiOperatorCorroborated);
    assert!(EvidenceLevel::MultiOperatorCorroborated > EvidenceLevel::SingleOperatorObserved);
    assert!(EvidenceLevel::SingleOperatorObserved > EvidenceLevel::SelfReported);
}
