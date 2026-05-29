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
    assert_eq!(DEFAULT_CODE_VERSION, "aios-distribution/0.0.1-T187");
}

// ---------------------------------------------------------------------------
// PublisherTrustLevel — 5 variants + label + can_publish
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
// RepositoryKind — 5 variants
// ---------------------------------------------------------------------------

#[test]
fn repository_kind_has_5_variants() {
    let kinds: Vec<RepositoryKind> = vec![
        RepositoryKind::AiosOfficialRepo,
        RepositoryKind::AiosVerifiedRepo,
        RepositoryKind::AiosCommunityRepo,
        RepositoryKind::AiosRecoveryRepo,
        RepositoryKind::ExternalBridgeRepo,
    ];
    assert_eq!(kinds.len(), 5);
}

// ---------------------------------------------------------------------------
// UpdateChannel — 4 variants
// ---------------------------------------------------------------------------

#[test]
fn update_channel_has_4_variants() {
    let channels: Vec<UpdateChannel> = vec![
        UpdateChannel::Stable,
        UpdateChannel::Beta,
        UpdateChannel::RecoveryCritical,
        UpdateChannel::Edge,
    ];
    assert_eq!(channels.len(), 4);
}

// ---------------------------------------------------------------------------
// PackageKind — 9 variants
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
// InstallScope — 4 variants (including RecoveryOnly)
// ---------------------------------------------------------------------------

#[test]
fn install_scope_has_4_variants_including_recovery_only() {
    let scopes: Vec<InstallScope> = vec![
        InstallScope::SystemWide,
        InstallScope::PerGroup,
        InstallScope::PerSubject,
        InstallScope::RecoveryOnly,
    ];
    assert_eq!(scopes.len(), 4);
    assert!(scopes.contains(&InstallScope::RecoveryOnly));
}

// ---------------------------------------------------------------------------
// PackageInstallState — 10 variants
// ---------------------------------------------------------------------------

#[test]
fn package_install_state_has_10_variants() {
    let states: Vec<PackageInstallState> = vec![
        PackageInstallState::Discovered,
        PackageInstallState::Fetching,
        PackageInstallState::Fetched,
        PackageInstallState::Verifying,
        PackageInstallState::Verified,
        PackageInstallState::Staging,
        PackageInstallState::Staged,
        PackageInstallState::Activating,
        PackageInstallState::Installed,
        PackageInstallState::Failed,
    ];
    assert_eq!(states.len(), 10);
}

// ---------------------------------------------------------------------------
// PackageVerificationResult — 10 variants (including TrustChainTooDeep)
// ---------------------------------------------------------------------------

#[test]
fn package_verification_result_has_10_variants_including_trust_chain_too_deep() {
    let results: Vec<PackageVerificationResult> = vec![
        PackageVerificationResult::Valid,
        PackageVerificationResult::SignatureInvalid,
        PackageVerificationResult::TrustChainTooDeep,
        PackageVerificationResult::PublisherDeplatformed,
        PackageVerificationResult::ContentHashMismatch,
        PackageVerificationResult::ManifestMalformed,
        PackageVerificationResult::RepositoryKindMismatch,
        PackageVerificationResult::DowngradeAttempt,
        PackageVerificationResult::UnknownPublisher,
        PackageVerificationResult::RevokedKey,
    ];
    assert_eq!(results.len(), 10);
    assert!(results.contains(&PackageVerificationResult::TrustChainTooDeep));
}

// ---------------------------------------------------------------------------
// MirrorSemantic — 3 variants
// ---------------------------------------------------------------------------

#[test]
fn mirror_semantic_has_3_variants() {
    let semantics: Vec<MirrorSemantic> = vec![
        MirrorSemantic::OriginAuthoritative,
        MirrorSemantic::MirrorPassthrough,
        MirrorSemantic::MirrorCacheOnly,
    ];
    assert_eq!(semantics.len(), 3);
}

// ---------------------------------------------------------------------------
// TakedownReason — 7 variants (including SupplyChainCompromise)
// ---------------------------------------------------------------------------

#[test]
fn takedown_reason_has_7_variants_including_supply_chain_compromise() {
    let reasons: Vec<TakedownReason> = vec![
        TakedownReason::Malware,
        TakedownReason::SupplyChainCompromise,
        TakedownReason::CapabilityLieDetected,
        TakedownReason::LicenseViolation,
        TakedownReason::KeyCompromise,
        TakedownReason::AbandonedAfterInactiveTtl,
        TakedownReason::OperatorRequested,
    ];
    assert_eq!(reasons.len(), 7);
    assert!(reasons.contains(&TakedownReason::SupplyChainCompromise));
}

// ---------------------------------------------------------------------------
// Identifier serde round-trips
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
// DistributionErrorCode + DistributionError
// ---------------------------------------------------------------------------

#[test]
fn distribution_error_code_has_at_least_15_variants() {
    let codes: Vec<DistributionErrorCode> = vec![
        DistributionErrorCode::PackageNotFound,
        DistributionErrorCode::PublisherNotFound,
        DistributionErrorCode::SignatureInvalid,
        DistributionErrorCode::TrustChainTooDeep,
        DistributionErrorCode::PublisherDeplatformed,
        DistributionErrorCode::ContentHashMismatch,
        DistributionErrorCode::ManifestMalformed,
        DistributionErrorCode::RepositoryKindMismatch,
        DistributionErrorCode::DowngradeAttempt,
        DistributionErrorCode::RevokedKey,
        DistributionErrorCode::InstallStateInvalidTransition,
        DistributionErrorCode::MirrorReSignAttempt,
        DistributionErrorCode::CapabilityLieDetected,
        DistributionErrorCode::TakedownActive,
        DistributionErrorCode::Internal,
    ];
    assert_eq!(codes.len(), 15);
}

#[test]
fn distribution_error_signature_invalid_code_matches() {
    let err = DistributionError::SignatureInvalid("bad signature".into());
    assert_eq!(err.code(), DistributionErrorCode::SignatureInvalid);
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
        DistributionError::SignatureInvalid("Ed25519 verify failed".into()),
        DistributionError::TrustChainTooDeep("depth 4".into()),
        DistributionError::PublisherDeplatformed("pub:evilcorp is deplatformed".into()),
        DistributionError::ContentHashMismatch("BLAKE3 mismatch".into()),
        DistributionError::ManifestMalformed("invalid semver".into()),
        DistributionError::RepositoryKindMismatch("kernel from verified repo".into()),
        DistributionError::DowngradeAttempt("1.0.0 → 0.9.0".into()),
        DistributionError::RevokedKey("pks:example:release-2024 revoked at 2026-01-01".into()),
        DistributionError::InstallStateInvalidTransition(
            "Discovered → Installed is invalid".into(),
        ),
        DistributionError::MirrorReSignAttempt("mirror.example.com re-signed".into()),
        DistributionError::CapabilityLieDetected("observed cap not in declared set".into()),
        DistributionError::TakedownActive("pub:evilcorp takedown in grace period".into()),
        DistributionError::Internal("unexpected null pointer".into()),
    ];

    assert_eq!(errors.len(), 15);

    for err in &errors {
        let display = err.to_string();
        assert!(
            !display.is_empty(),
            "Display impl must produce a non-empty string"
        );
    }
}
