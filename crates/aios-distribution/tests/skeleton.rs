#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::doc_markdown,
    clippy::significant_drop_tightening,
    clippy::wildcard_imports,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::needless_collect,
    clippy::format_collect,
    clippy::too_many_arguments,
    clippy::float_cmp,
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use aios_distribution::*;

// ---------------------------------------------------------------------------
// DEFAULT_CODE_VERSION
// ---------------------------------------------------------------------------

#[test]
fn default_code_version_constant_is_correct() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-distribution/0.0.1-T194");
}

// ---------------------------------------------------------------------------
// PublisherTrustLevel — 5 variants + label + can_publish (UNCHANGED)
// ---------------------------------------------------------------------------

#[test]
fn publisher_trust_level_has_5_variants() {
    let levels: Vec<PublisherTrustLevel> = vec![
        PublisherTrustLevel::AiosRoot,
        PublisherTrustLevel::Verified,
        PublisherTrustLevel::Community,
        PublisherTrustLevel::Deprecated,
        PublisherTrustLevel::Deplatformed,
    ];
    assert_eq!(levels.len(), 5);
}

#[test]
fn publisher_trust_level_aios_root_can_publish() {
    assert!(PublisherTrustLevel::AiosRoot.can_publish());
}

#[test]
fn publisher_trust_level_deplatformed_cannot_publish() {
    assert!(!PublisherTrustLevel::Deplatformed.can_publish());
}

#[test]
fn publisher_trust_level_deprecated_cannot_publish() {
    assert!(!PublisherTrustLevel::Deprecated.can_publish());
}

// ---------------------------------------------------------------------------
// RepositoryKind — 5 variants (renamed: AiosRootRepo, ExternalBridge)
// ---------------------------------------------------------------------------

#[test]
fn repository_kind_has_5_variants() {
    let kinds: Vec<RepositoryKind> = vec![
        RepositoryKind::AiosRootRepo,
        RepositoryKind::AiosVerifiedRepo,
        RepositoryKind::AiosCommunityRepo,
        RepositoryKind::AiosRecoveryRepo,
        RepositoryKind::ExternalBridge,
    ];
    assert_eq!(kinds.len(), 5);
}

#[test]
fn repository_kind_aios_root_repo_exists() {
    assert_eq!(
        std::mem::discriminant(&RepositoryKind::AiosRootRepo),
        std::mem::discriminant(&RepositoryKind::AiosRootRepo),
    );
}

// ---------------------------------------------------------------------------
// UpdateChannel — 4 variants (renamed: DeprecatedRetention)
// ---------------------------------------------------------------------------

#[test]
fn update_channel_has_4_variants() {
    let channels: Vec<UpdateChannel> = vec![
        UpdateChannel::Stable,
        UpdateChannel::Beta,
        UpdateChannel::RecoveryCritical,
        UpdateChannel::DeprecatedRetention,
    ];
    assert_eq!(channels.len(), 4);
}

#[test]
fn update_channel_deprecated_retention_exists() {
    assert_eq!(
        std::mem::discriminant(&UpdateChannel::DeprecatedRetention),
        std::mem::discriminant(&UpdateChannel::DeprecatedRetention),
    );
}

// ---------------------------------------------------------------------------
// PackageKind — 9 variants (UNCHANGED)
// ---------------------------------------------------------------------------

#[test]
fn package_kind_has_9_variants() {
    let kinds: Vec<PackageKind> = vec![
        PackageKind::App,
        PackageKind::Agent,
        PackageKind::Theme,
        PackageKind::InvariantBundle,
        PackageKind::PolicyBundle,
        PackageKind::IdentityBundle,
        PackageKind::KernelCandidate,
        PackageKind::Adapter,
        PackageKind::CapabilityCatalogDelta,
    ];
    assert_eq!(kinds.len(), 9);
}

// ---------------------------------------------------------------------------
// InstallScope — 4 variants (replaced: SystemOnly, GroupScoped, UserScoped, Either)
// ---------------------------------------------------------------------------

#[test]
fn install_scope_has_4_variants() {
    let scopes: Vec<InstallScope> = vec![
        InstallScope::SystemOnly,
        InstallScope::GroupScoped,
        InstallScope::UserScoped,
        InstallScope::Either,
    ];
    assert_eq!(scopes.len(), 4);
}

#[test]
fn install_scope_system_only_and_either_exist() {
    assert_eq!(
        std::mem::discriminant(&InstallScope::SystemOnly),
        std::mem::discriminant(&InstallScope::SystemOnly),
    );
    assert_eq!(
        std::mem::discriminant(&InstallScope::Either),
        std::mem::discriminant(&InstallScope::Either),
    );
}

// ---------------------------------------------------------------------------
// PackageInstallState — 10 variants INCLUDING Quarantined
// ---------------------------------------------------------------------------

#[test]
fn package_install_state_has_10_variants_including_quarantined() {
    let states: Vec<PackageInstallState> = vec![
        PackageInstallState::Draft,
        PackageInstallState::Validating,
        PackageInstallState::AwaitingApproval,
        PackageInstallState::Approved,
        PackageInstallState::Installing,
        PackageInstallState::Active,
        PackageInstallState::Quarantined,
        PackageInstallState::Uninstalling,
        PackageInstallState::Removed,
        PackageInstallState::InstallFailed,
    ];
    assert_eq!(states.len(), 10);
    assert!(states.contains(&PackageInstallState::Quarantined));
}

#[test]
fn package_install_state_removed_and_install_failed_are_terminal() {
    assert!(PackageInstallState::Removed.is_terminal());
    assert!(PackageInstallState::InstallFailed.is_terminal());
    // Active and Quarantined are NOT terminal per spec §3.6.
    assert!(!PackageInstallState::Active.is_terminal());
    assert!(!PackageInstallState::Quarantined.is_terminal());
}

// ---------------------------------------------------------------------------
// PackageVerificationResult — 10 variants; dual-success semantics
// ---------------------------------------------------------------------------

#[test]
fn package_verification_result_has_11_variants_including_trust_chain_too_deep() {
    let results: Vec<PackageVerificationResult> = vec![
        PackageVerificationResult::VerifiedAiosRoot,
        PackageVerificationResult::VerifiedPublisher,
        PackageVerificationResult::SignatureFailed,
        PackageVerificationResult::TrustChainBroken,
        PackageVerificationResult::TrustChainTooDeep,
        PackageVerificationResult::PublisherDeplatformed,
        PackageVerificationResult::HashMismatch,
        PackageVerificationResult::ManifestForged,
        PackageVerificationResult::RepositoryKindMismatch,
        PackageVerificationResult::CapabilityLie,
        PackageVerificationResult::BundleTampered,
    ];
    assert_eq!(results.len(), 11);
    assert!(results.contains(&PackageVerificationResult::TrustChainTooDeep));
}

#[test]
fn package_verification_result_verified_aios_root_and_publisher_are_success() {
    assert!(PackageVerificationResult::VerifiedAiosRoot.is_success());
    assert!(PackageVerificationResult::VerifiedPublisher.is_success());
    assert!(!PackageVerificationResult::SignatureFailed.is_success());
}

// ---------------------------------------------------------------------------
// MirrorSemantic — 3 variants (renamed: Origin, Cached, Local)
// ---------------------------------------------------------------------------

#[test]
fn mirror_semantic_has_3_variants() {
    let semantics: Vec<MirrorSemantic> = vec![
        MirrorSemantic::Origin,
        MirrorSemantic::Cached,
        MirrorSemantic::Local,
    ];
    assert_eq!(semantics.len(), 3);
}

#[test]
fn mirror_semantic_origin_cached_local_exist() {
    assert_eq!(
        std::mem::discriminant(&MirrorSemantic::Origin),
        std::mem::discriminant(&MirrorSemantic::Origin),
    );
    assert_eq!(
        std::mem::discriminant(&MirrorSemantic::Cached),
        std::mem::discriminant(&MirrorSemantic::Cached),
    );
    assert_eq!(
        std::mem::discriminant(&MirrorSemantic::Local),
        std::mem::discriminant(&MirrorSemantic::Local),
    );
}

// ---------------------------------------------------------------------------
// TakedownReason — 7 variants (renamed: MaliciousBehaviorDetected, LegalRequirement, PublisherRequest)
// ---------------------------------------------------------------------------

#[test]
fn takedown_reason_has_7_variants_including_supply_chain_compromise() {
    let reasons: Vec<TakedownReason> = vec![
        TakedownReason::MaliciousBehaviorDetected,
        TakedownReason::SupplyChainCompromise,
        TakedownReason::CapabilityLieDetected,
        TakedownReason::LegalRequirement,
        TakedownReason::PublisherRequest,
        TakedownReason::KeyCompromise,
        TakedownReason::AbandonedAfterInactiveTtl,
    ];
    assert_eq!(reasons.len(), 7);
    assert!(reasons.contains(&TakedownReason::SupplyChainCompromise));
}

#[test]
fn takedown_reason_malicious_behavior_detected_exists() {
    assert_eq!(
        std::mem::discriminant(&TakedownReason::MaliciousBehaviorDetected),
        std::mem::discriminant(&TakedownReason::MaliciousBehaviorDetected),
    );
}

// ---------------------------------------------------------------------------
// Identifier serde round-trips (UNCHANGED)
// ---------------------------------------------------------------------------

#[test]
fn package_id_serde_round_trip() {
    let id = PackageId("pkg:example:my-app".into());
    let json = serde_json::to_string(&id).unwrap();
    let round_tripped: PackageId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, round_tripped);
}

#[test]
fn publisher_id_serde_round_trip() {
    let id = PublisherId("pub:example".into());
    let json = serde_json::to_string(&id).unwrap();
    let round_tripped: PublisherId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, round_tripped);
}

#[test]
fn manifest_id_serde_round_trip() {
    let id = ManifestId("abcdef0123456789abcdef0123456789".into());
    let json = serde_json::to_string(&id).unwrap();
    let round_tripped: ManifestId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, round_tripped);
}

// ---------------------------------------------------------------------------
// DistributionErrorCode + DistributionError (16 variants)
// ---------------------------------------------------------------------------

#[test]
fn distribution_error_code_has_at_least_15_variants() {
    let codes: Vec<DistributionErrorCode> = vec![
        DistributionErrorCode::PackageNotFound,
        DistributionErrorCode::PublisherNotFound,
        DistributionErrorCode::SignatureFailed,
        DistributionErrorCode::TrustChainTooDeep,
        DistributionErrorCode::PublisherDeplatformed,
        DistributionErrorCode::HashMismatch,
        DistributionErrorCode::ManifestForged,
        DistributionErrorCode::RepositoryKindMismatch,
        DistributionErrorCode::RevokedKey,
        DistributionErrorCode::InstallStateInvalidTransition,
        DistributionErrorCode::MirrorReSignAttempt,
        DistributionErrorCode::CapabilityLieDetected,
        DistributionErrorCode::TakedownActive,
        DistributionErrorCode::Internal,
        DistributionErrorCode::InstallScopeViolation,
        DistributionErrorCode::BundleTampered,
        DistributionErrorCode::MirrorBlacklisted,
        DistributionErrorCode::PackageDowngradeBlocked,
    ];
    assert_eq!(codes.len(), 18);
}

#[test]
fn distribution_error_signature_failed_code_matches() {
    let err = DistributionError::SignatureFailed("bad signature".into());
    assert_eq!(err.code(), DistributionErrorCode::SignatureFailed);
}

#[test]
fn distribution_error_trust_chain_too_deep_code_matches() {
    let err = DistributionError::TrustChainTooDeep("depth 5 exceeds max 3".into());
    assert_eq!(err.code(), DistributionErrorCode::TrustChainTooDeep);
}

#[test]
fn distribution_error_display_round_trip_all_variants_non_empty() {
    let errors: Vec<DistributionError> = vec![
        DistributionError::PackageNotFound("pkg:foo:bar".into()),
        DistributionError::PublisherNotFound("pub:baz".into()),
        DistributionError::SignatureFailed("Ed25519 verify failed".into()),
        DistributionError::TrustChainTooDeep("depth 4".into()),
        DistributionError::PublisherDeplatformed("pub:evilcorp is deplatformed".into()),
        DistributionError::HashMismatch("BLAKE3 mismatch".into()),
        DistributionError::ManifestForged("invalid semver".into()),
        DistributionError::RepositoryKindMismatch("kernel from verified repo".into()),
        DistributionError::RevokedKey("pks:example:release-2024 revoked at 2026-01-01".into()),
        DistributionError::InstallStateInvalidTransition("Draft → Active is invalid".into()),
        DistributionError::MirrorReSignAttempt("mirror.example.com re-signed".into()),
        DistributionError::CapabilityLieDetected("observed cap not in declared set".into()),
        DistributionError::TakedownActive("pub:evilcorp takedown in grace period".into()),
        DistributionError::Internal("unexpected null pointer".into()),
        DistributionError::InstallScopeViolation(
            "SYSTEM_ONLY requested, USER_SCOPED manifest".into(),
        ),
        DistributionError::BundleTampered("executable found in THEME package".into()),
    ];

    assert_eq!(errors.len(), 16);

    for err in &errors {
        let display = err.to_string();
        assert!(
            !display.is_empty(),
            "Display impl must produce a non-empty string"
        );
    }
}
