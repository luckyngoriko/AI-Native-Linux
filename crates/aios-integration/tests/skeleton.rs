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
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use aios_integration::*;
use chrono::Utc;

// ---------------------------------------------------------------------------
// DEFAULT_CODE_VERSION
// ---------------------------------------------------------------------------

#[test]
fn default_code_version_constant_is_correct() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-integration/0.1.0-T186");
}

// ---------------------------------------------------------------------------
// IntegrationLifecycleState — 6 variants + label mapping
// ---------------------------------------------------------------------------

#[test]
fn integration_lifecycle_state_has_6_variants() {
    let states: Vec<IntegrationLifecycleState> = vec![
        IntegrationLifecycleState::Proposed {
            proposer: "alice".into(),
            proposed_at: Utc::now(),
        },
        IntegrationLifecycleState::Evaluated {
            evaluator: "bob".into(),
            evaluated_at: Utc::now(),
            security_audit_passed: true,
        },
        IntegrationLifecycleState::Piloted {
            since: Utc::now(),
            profile: "DEV_RELAXED".into(),
        },
        IntegrationLifecycleState::Production { since: Utc::now() },
        IntegrationLifecycleState::Deprecated {
            since: Utc::now(),
            sunset_due: Some(Utc::now()),
        },
        IntegrationLifecycleState::Retired {
            since: Utc::now(),
            reason: "end-of-life".into(),
            data_migration_completed: true,
        },
    ];
    assert_eq!(states.len(), 6);
}

#[test]
fn integration_lifecycle_label_for_proposed() {
    let state = IntegrationLifecycleState::Proposed {
        proposer: "alice".into(),
        proposed_at: Utc::now(),
    };
    assert_eq!(state.label(), IntegrationLifecycleLabel::Proposed);
}

#[test]
fn integration_lifecycle_label_for_production() {
    let state = IntegrationLifecycleState::Production { since: Utc::now() };
    assert_eq!(state.label(), IntegrationLifecycleLabel::Production);
}

#[test]
fn integration_lifecycle_label_for_retired() {
    let state = IntegrationLifecycleState::Retired {
        since: Utc::now(),
        reason: "end-of-life".into(),
        data_migration_completed: true,
    };
    assert_eq!(state.label(), IntegrationLifecycleLabel::Retired);
}

// ---------------------------------------------------------------------------
// VendorKind + VendorTrustClass + VendorIntegrationContract
// ---------------------------------------------------------------------------

#[test]
fn vendor_kind_has_at_least_8_variants() {
    let kinds = [
        VendorKind::PackageRepository,
        VendorKind::ApplicationStore,
        VendorKind::OciRegistry,
        VendorKind::CveFeed,
        VendorKind::ComplianceProvider,
        VendorKind::MetricsExporter,
        VendorKind::IdentityProvider,
        VendorKind::OtherCertified,
    ];
    assert_eq!(kinds.len(), 8);
}

#[test]
fn vendor_trust_class_has_4_variants_including_blacklisted() {
    let classes = [
        VendorTrustClass::AiosCertifiedPartner,
        VendorTrustClass::CommunityVerified,
        VendorTrustClass::OperatorAuthorised,
        VendorTrustClass::BlacklistedDoNotAdmit,
    ];
    assert_eq!(classes.len(), 4);
    // Verify BlacklistedDoNotAdmit is present (not renamed/dropped)
    assert!(classes.contains(&VendorTrustClass::BlacklistedDoNotAdmit));
}

#[test]
fn vendor_integration_contract_serde_round_trip() {
    let contract = VendorIntegrationContract {
        contract_id: VendorContractId("VC-001".into()),
        vendor_name: "Example Corp".into(),
        vendor_kind: VendorKind::OciRegistry,
        trust_class: VendorTrustClass::AiosCertifiedPartner,
        contact_canonical_id: "alice@example.com".into(),
        rotation_cadence_days: 90,
        breach_playbook_url: "https://example.com/breach".into(),
        signer_fingerprint: "SHA256:abc123".into(),
        signature: vec![1, 2, 3, 4],
        admitted_at: Utc::now(),
    };

    let json = serde_json::to_string(&contract).unwrap();
    let round_tripped: VendorIntegrationContract = serde_json::from_str(&json).unwrap();
    assert_eq!(contract, round_tripped);
}

// ---------------------------------------------------------------------------
// StandardKind + StandardSubscription
// ---------------------------------------------------------------------------

#[test]
fn standard_kind_has_at_least_11_variants_including_gdpr_hipaa() {
    let kinds = [
        StandardKind::Nist80053Rev5,
        StandardKind::NistSp800218Ssdf,
        StandardKind::NistSp800207ZeroTrust,
        StandardKind::NistSp800193Firmware,
        StandardKind::DisaStig,
        StandardKind::CisControlsV8,
        StandardKind::Fips1403,
        StandardKind::Gdpr,
        StandardKind::Hipaa,
        StandardKind::Iso27001,
        StandardKind::Soc2,
    ];
    assert_eq!(kinds.len(), 11);
    assert!(kinds.contains(&StandardKind::Gdpr));
    assert!(kinds.contains(&StandardKind::Hipaa));
}

#[test]
fn standard_subscription_serde_round_trip() {
    let now = Utc::now();
    let sub = StandardSubscription {
        subscription_id: StandardSubscriptionId("SS-001".into()),
        standard: StandardKind::Nist80053Rev5,
        catalog_url: "https://nvd.nist.gov/800-53".into(),
        current_revision: "Rev.5".into(),
        last_reviewed_at: now,
        next_review_due_at: now,
        responsible_canonical_id: "bob@example.com".into(),
    };

    let json = serde_json::to_string(&sub).unwrap();
    let round_tripped: StandardSubscription = serde_json::from_str(&json).unwrap();
    assert_eq!(sub, round_tripped);
}

// ---------------------------------------------------------------------------
// CveSeverity + CveStatus + CveId
// ---------------------------------------------------------------------------

#[test]
fn cve_severity_has_4_variants() {
    let severities = [
        CveSeverity::Low,
        CveSeverity::Medium,
        CveSeverity::High,
        CveSeverity::Critical,
    ];
    assert_eq!(severities.len(), 4);
}

#[test]
fn cve_status_has_5_variants() {
    let statuses = [
        CveStatus::Open,
        CveStatus::UnderReview,
        CveStatus::Patched,
        CveStatus::Quarantined,
        CveStatus::NotApplicable,
    ];
    assert_eq!(statuses.len(), 5);
}

#[test]
fn cve_id_format_round_trip() {
    let cve_id = CveId("CVE-2024-12345".into());
    let json = serde_json::to_string(&cve_id).unwrap();
    let round_tripped: CveId = serde_json::from_str(&json).unwrap();
    assert_eq!(cve_id, round_tripped);
    assert!(round_tripped.0.starts_with("CVE-"));
}

// ---------------------------------------------------------------------------
// ServiceDependency + ComposedService + ServiceComposition
// ---------------------------------------------------------------------------

#[test]
fn composed_service_serde_round_trip() {
    let svc = ComposedService {
        service_id: "svc-a".into(),
        crate_name: "aios-action".into(),
        binding_endpoint: "grpc://localhost:50051".into(),
        depends_on: vec!["svc-b".into()],
    };

    let json = serde_json::to_string(&svc).unwrap();
    let round_tripped: ComposedService = serde_json::from_str(&json).unwrap();
    assert_eq!(svc, round_tripped);
}

#[test]
fn service_dependency_round_trip() {
    let dep = ServiceDependency {
        from_service: "svc-a".into(),
        to_service: "svc-b".into(),
        required: true,
    };

    let json = serde_json::to_string(&dep).unwrap();
    let round_tripped: ServiceDependency = serde_json::from_str(&json).unwrap();
    assert_eq!(dep, round_tripped);
}

#[test]
fn service_composition_with_3_services_2_deps_round_trip() {
    let comp = ServiceComposition {
        composition_id: ComposedSystemId("CS-001".into()),
        services: vec![
            ComposedService {
                service_id: "svc-a".into(),
                crate_name: "aios-action".into(),
                binding_endpoint: "grpc://localhost:50051".into(),
                depends_on: vec![],
            },
            ComposedService {
                service_id: "svc-b".into(),
                crate_name: "aios-vault".into(),
                binding_endpoint: "grpc://localhost:50052".into(),
                depends_on: vec![],
            },
            ComposedService {
                service_id: "svc-c".into(),
                crate_name: "aios-sgr".into(),
                binding_endpoint: "grpc://localhost:50053".into(),
                depends_on: vec!["svc-a".into(), "svc-b".into()],
            },
        ],
        dependencies: vec![
            ServiceDependency {
                from_service: "svc-c".into(),
                to_service: "svc-a".into(),
                required: true,
            },
            ServiceDependency {
                from_service: "svc-c".into(),
                to_service: "svc-b".into(),
                required: true,
            },
        ],
        topological_order: vec!["svc-a".into(), "svc-b".into(), "svc-c".into()],
    };

    let json = serde_json::to_string(&comp).unwrap();
    let round_tripped: ServiceComposition = serde_json::from_str(&json).unwrap();
    assert_eq!(comp, round_tripped);
    assert_eq!(round_tripped.services.len(), 3);
    assert_eq!(round_tripped.dependencies.len(), 2);
}

// ---------------------------------------------------------------------------
// IntegrationErrorCode + IntegrationError
// ---------------------------------------------------------------------------

#[test]
fn integration_error_code_has_at_least_10_variants() {
    let codes = [
        IntegrationErrorCode::LifecycleInvalidTransition,
        IntegrationErrorCode::VendorContractSignatureInvalid,
        IntegrationErrorCode::VendorBlacklisted,
        IntegrationErrorCode::StandardSubscriptionExpired,
        IntegrationErrorCode::CveFeedUnreachable,
        IntegrationErrorCode::CompositionCycleDetected,
        IntegrationErrorCode::ComposedServiceMissing,
        IntegrationErrorCode::OrchestratorBootFailed,
        IntegrationErrorCode::ConfigInvalid,
        IntegrationErrorCode::Internal,
    ];
    assert_eq!(codes.len(), 10);
}

#[test]
fn integration_error_lifecycle_invalid_transition_code_matches() {
    let err = IntegrationError::LifecycleInvalidTransition {
        from: IntegrationLifecycleLabel::Proposed,
        to: IntegrationLifecycleLabel::Production,
        reason: "skip evaluation".into(),
    };
    assert_eq!(err.code(), IntegrationErrorCode::LifecycleInvalidTransition);
}

#[test]
fn integration_error_vendor_blacklisted_code_matches() {
    let err = IntegrationError::VendorBlacklisted {
        contract_id: VendorContractId("VC-001".into()),
    };
    assert_eq!(err.code(), IntegrationErrorCode::VendorBlacklisted);
}

#[test]
fn integration_error_display_round_trip_all_variants_non_empty() {
    let errors: Vec<IntegrationError> = vec![
        IntegrationError::LifecycleInvalidTransition {
            from: IntegrationLifecycleLabel::Proposed,
            to: IntegrationLifecycleLabel::Retired,
            reason: "rejected by operator".into(),
        },
        IntegrationError::VendorContractSignatureInvalid {
            contract_id: VendorContractId("VC-001".into()),
            reason: "signature mismatch".into(),
        },
        IntegrationError::VendorBlacklisted {
            contract_id: VendorContractId("VC-002".into()),
        },
        IntegrationError::StandardSubscriptionExpired {
            subscription_id: StandardSubscriptionId("SS-001".into()),
            expired_at: Utc::now(),
        },
        IntegrationError::CveFeedUnreachable("https://nvd.nist.gov timeout".into()),
        IntegrationError::CompositionCycleDetected {
            cycle: vec!["svc-a".into(), "svc-b".into(), "svc-a".into()],
        },
        IntegrationError::ComposedServiceMissing {
            service_id: "svc-missing".into(),
            required_by: "svc-caller".into(),
        },
        IntegrationError::OrchestratorBootFailed {
            stage: "topo-sort".into(),
            reason: "cycle detected".into(),
        },
        IntegrationError::ConfigInvalid("missing composition_id".into()),
        IntegrationError::Internal("unknown failure".into()),
    ];

    assert_eq!(errors.len(), 10);

    for err in &errors {
        let display = err.to_string();
        assert!(
            !display.is_empty(),
            "Display impl must produce a non-empty string"
        );
    }
}
