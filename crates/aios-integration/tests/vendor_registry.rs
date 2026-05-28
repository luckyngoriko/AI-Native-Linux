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
    clippy::missing_const_for_fn,
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use aios_integration::*;
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn canonical_contract_bytes(contract: &VendorIntegrationContract) -> Vec<u8> {
    let mut s = String::new();
    s.push_str(&contract.contract_id.0);
    s.push('\n');
    s.push_str(&contract.vendor_name);
    s.push('\n');
    s.push_str(contract.vendor_kind.label());
    s.push('\n');
    s.push_str(contract.trust_class.label());
    s.push('\n');
    s.push_str(&contract.contact_canonical_id);
    s.push('\n');
    s.push_str(&contract.rotation_cadence_days.to_string());
    s.push('\n');
    s.push_str(&contract.breach_playbook_url);
    s.into_bytes()
}

#[allow(clippy::too_many_arguments)]
fn build_signed_contract(
    contract_id: &str,
    vendor_name: &str,
    vendor_kind: VendorKind,
    trust_class: VendorTrustClass,
    contact: &str,
    rotation: u32,
    breach_url: &str,
    signing_key: &SigningKey,
    fingerprint: &str,
) -> VendorIntegrationContract {
    let mut contract = VendorIntegrationContract {
        contract_id: VendorContractId(contract_id.into()),
        vendor_name: vendor_name.into(),
        vendor_kind,
        trust_class,
        contact_canonical_id: contact.into(),
        rotation_cadence_days: rotation,
        breach_playbook_url: breach_url.into(),
        signer_fingerprint: fingerprint.into(),
        signature: Vec::new(),
        admitted_at: Utc::now(),
    };
    let canonical = canonical_contract_bytes(&contract);
    let sig = signing_key.sign(&canonical);
    contract.signature = sig.to_bytes().to_vec();
    contract
}

fn make_keypair() -> (SigningKey, VerifyingKey) {
    let signing = SigningKey::generate(&mut OsRng);
    let verifying = signing.verifying_key();
    (signing, verifying)
}

// ---------------------------------------------------------------------------
// admit_contract — valid signature
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admit_contract_with_valid_signature_succeeds() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let contract = build_signed_contract(
        "VC-01",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:acme",
    );

    let result = registry.admit_contract(contract).await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

// ---------------------------------------------------------------------------
// admit_contract — invalid signature
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admit_contract_with_invalid_signature_returns_vendor_contract_signature_invalid() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let mut contract = build_signed_contract(
        "VC-02",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:acme",
    );
    // Corrupt the signature
    if !contract.signature.is_empty() {
        contract.signature[0] ^= 0xFF;
    }

    let result = registry.admit_contract(contract).await;
    assert!(matches!(
        result,
        Err(IntegrationError::VendorContractSignatureInvalid { ref reason, .. })
            if reason == "ed25519 verify failed"
    ));
}

// ---------------------------------------------------------------------------
// admit_contract — unknown authority
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admit_contract_with_unknown_authority_returns_signature_invalid() {
    let registry = VendorIntegrationRegistry::new();
    let (sk, _vk) = make_keypair();

    let contract = build_signed_contract(
        "VC-03",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:unknown",
    );

    let result = registry.admit_contract(contract).await;
    assert!(matches!(
        result,
        Err(IntegrationError::VendorContractSignatureInvalid { ref reason, .. })
            if reason == "unknown authority"
    ));
}

// ---------------------------------------------------------------------------
// admit_contract — blacklisted trust class
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admit_contract_with_blacklisted_trust_class_returns_vendor_blacklisted() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:evil", vk);

    let contract = build_signed_contract(
        "VC-04",
        "Evil Corp",
        VendorKind::ApplicationStore,
        VendorTrustClass::BlacklistedDoNotAdmit,
        "evil@example.com",
        30,
        "https://evil.example/breach",
        &sk,
        "fp:evil",
    );

    let result = registry.admit_contract(contract).await;
    assert!(matches!(
        result,
        Err(IntegrationError::VendorBlacklisted { .. })
    ));
}

// ---------------------------------------------------------------------------
// admit_contract — blacklisted vendor name
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admit_contract_with_blacklisted_vendor_name_returns_vendor_blacklisted() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    registry.add_to_blacklist("BlockedVendor").await.unwrap();

    let contract = build_signed_contract(
        "VC-05",
        "BlockedVendor",
        VendorKind::MetricsExporter,
        VendorTrustClass::OperatorAuthorised,
        "blocked@example.com",
        60,
        "https://blocked.example/breach",
        &sk,
        "fp:acme",
    );

    let result = registry.admit_contract(contract).await;
    assert!(matches!(
        result,
        Err(IntegrationError::VendorBlacklisted { .. })
    ));
}

// ---------------------------------------------------------------------------
// admit_contract — duplicate contract_id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admit_duplicate_contract_id_returns_signature_invalid() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let c1 = build_signed_contract(
        "VC-06",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:acme",
    );
    let c2 = build_signed_contract(
        "VC-06",
        "Other Corp",
        VendorKind::ApplicationStore,
        VendorTrustClass::CommunityVerified,
        "other@example.com",
        30,
        "https://other.example/breach",
        &sk,
        "fp:acme",
    );

    registry.admit_contract(c1).await.unwrap();

    let result = registry.admit_contract(c2).await;
    assert!(matches!(
        result,
        Err(IntegrationError::VendorContractSignatureInvalid { ref reason, .. })
            if reason == "contract already admitted"
    ));
}

// ---------------------------------------------------------------------------
// admit_contract — initial lifecycle state is Proposed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admit_contract_initial_lifecycle_state_is_proposed() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let contract = build_signed_contract(
        "VC-07",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:acme",
    );

    registry.admit_contract(contract).await.unwrap();

    let state = registry
        .current_lifecycle(&VendorContractId("VC-07".into()))
        .await;
    assert!(matches!(
        state,
        Some(IntegrationLifecycleState::Proposed { .. })
    ));
}

// ---------------------------------------------------------------------------
// transition — Proposed → Evaluated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transition_proposed_to_evaluated_succeeds() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let contract = build_signed_contract(
        "VC-08",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:acme",
    );
    registry.admit_contract(contract).await.unwrap();

    let cid = VendorContractId("VC-08".into());
    let result = registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Evaluated {
                evaluator: "bob".into(),
                evaluated_at: Utc::now(),
                security_audit_passed: true,
            },
        )
        .await;

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    let state = registry.current_lifecycle(&cid).await;
    assert!(matches!(
        state,
        Some(IntegrationLifecycleState::Evaluated { .. })
    ));
}

// ---------------------------------------------------------------------------
// transition — Evaluated → Piloted (security_audit_passed=true)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transition_evaluated_to_piloted_when_security_audit_passed_succeeds() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let contract = build_signed_contract(
        "VC-09",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:acme",
    );
    let cid = VendorContractId("VC-09".into());
    registry.admit_contract(contract).await.unwrap();
    registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Evaluated {
                evaluator: "bob".into(),
                evaluated_at: Utc::now(),
                security_audit_passed: true,
            },
        )
        .await
        .unwrap();

    let result = registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Piloted {
                since: Utc::now(),
                profile: "DEV_RELAXED".into(),
            },
        )
        .await;

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    let state = registry.current_lifecycle(&cid).await;
    assert!(matches!(
        state,
        Some(IntegrationLifecycleState::Piloted { .. })
    ));
}

// ---------------------------------------------------------------------------
// transition — Piloted → Production
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transition_piloted_to_production_succeeds() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let contract = build_signed_contract(
        "VC-10",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:acme",
    );
    let cid = VendorContractId("VC-10".into());
    registry.admit_contract(contract).await.unwrap();
    registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Evaluated {
                evaluator: "bob".into(),
                evaluated_at: Utc::now(),
                security_audit_passed: true,
            },
        )
        .await
        .unwrap();
    registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Piloted {
                since: Utc::now(),
                profile: "DEV_RELAXED".into(),
            },
        )
        .await
        .unwrap();

    let result = registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Production { since: Utc::now() },
        )
        .await;

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    let state = registry.current_lifecycle(&cid).await;
    assert!(matches!(
        state,
        Some(IntegrationLifecycleState::Production { .. })
    ));
}

// ---------------------------------------------------------------------------
// transition — Production → Deprecated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transition_production_to_deprecated_succeeds() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let contract = build_signed_contract(
        "VC-11",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:acme",
    );
    let cid = VendorContractId("VC-11".into());
    registry.admit_contract(contract).await.unwrap();
    registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Evaluated {
                evaluator: "bob".into(),
                evaluated_at: Utc::now(),
                security_audit_passed: true,
            },
        )
        .await
        .unwrap();
    registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Piloted {
                since: Utc::now(),
                profile: "DEV_RELAXED".into(),
            },
        )
        .await
        .unwrap();
    registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Production { since: Utc::now() },
        )
        .await
        .unwrap();

    let result = registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Deprecated {
                since: Utc::now(),
                sunset_due: None,
            },
        )
        .await;

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    let state = registry.current_lifecycle(&cid).await;
    assert!(matches!(
        state,
        Some(IntegrationLifecycleState::Deprecated { .. })
    ));
}

// ---------------------------------------------------------------------------
// transition — Deprecated → Retired
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transition_deprecated_to_retired_succeeds() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let contract = build_signed_contract(
        "VC-12",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:acme",
    );
    let cid = VendorContractId("VC-12".into());
    registry.admit_contract(contract).await.unwrap();
    registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Evaluated {
                evaluator: "bob".into(),
                evaluated_at: Utc::now(),
                security_audit_passed: true,
            },
        )
        .await
        .unwrap();
    registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Piloted {
                since: Utc::now(),
                profile: "DEV_RELAXED".into(),
            },
        )
        .await
        .unwrap();
    registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Production { since: Utc::now() },
        )
        .await
        .unwrap();
    registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Deprecated {
                since: Utc::now(),
                sunset_due: None,
            },
        )
        .await
        .unwrap();

    let result = registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Retired {
                since: Utc::now(),
                reason: "end-of-life".into(),
                data_migration_completed: true,
            },
        )
        .await;

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    let state = registry.current_lifecycle(&cid).await;
    assert!(matches!(
        state,
        Some(IntegrationLifecycleState::Retired { .. })
    ));
}

// ---------------------------------------------------------------------------
// transition — Retired is terminal
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transition_retired_to_anything_returns_lifecycle_invalid_transition() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let contract = build_signed_contract(
        "VC-13",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:acme",
    );
    let cid = VendorContractId("VC-13".into());
    registry.admit_contract(contract).await.unwrap();
    // Proposed → Retired (direct, allowed)
    registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Retired {
                since: Utc::now(),
                reason: "done".into(),
                data_migration_completed: true,
            },
        )
        .await
        .unwrap();

    // Retired → anything should fail
    let result = registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Production { since: Utc::now() },
        )
        .await;

    assert!(matches!(
        result,
        Err(IntegrationError::LifecycleInvalidTransition { .. })
    ));
}

// ---------------------------------------------------------------------------
// transition — Proposed → Production is invalid (skip states)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transition_proposed_to_production_returns_lifecycle_invalid_transition() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let contract = build_signed_contract(
        "VC-14",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:acme",
    );
    let cid = VendorContractId("VC-14".into());
    registry.admit_contract(contract).await.unwrap();

    // Proposed → Production directly (should fail)
    let result = registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Production { since: Utc::now() },
        )
        .await;

    assert!(matches!(
        result,
        Err(IntegrationError::LifecycleInvalidTransition { .. })
    ));
}

// ---------------------------------------------------------------------------
// get_contract
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_contract_known_returns_some() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let contract = build_signed_contract(
        "VC-15",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:acme",
    );
    let cid = VendorContractId("VC-15".into());
    registry.admit_contract(contract).await.unwrap();

    let found = registry.get_contract(&cid).await;
    assert!(found.is_some());
    assert_eq!(found.unwrap().vendor_name, "Acme Corp");

    let not_found = registry
        .get_contract(&VendorContractId("VC-NOPE".into()))
        .await;
    assert!(not_found.is_none());
}

// ---------------------------------------------------------------------------
// current_lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn current_lifecycle_known_returns_state() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let contract = build_signed_contract(
        "VC-16",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:acme",
    );
    let cid = VendorContractId("VC-16".into());
    registry.admit_contract(contract).await.unwrap();

    let state = registry.current_lifecycle(&cid).await;
    assert!(state.is_some());
    assert_eq!(state.unwrap().label(), IntegrationLifecycleLabel::Proposed);

    let not_found = registry
        .current_lifecycle(&VendorContractId("VC-NOPE".into()))
        .await;
    assert!(not_found.is_none());
}

// ---------------------------------------------------------------------------
// list_by_kind
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_by_kind_returns_only_matching() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let c_oci = build_signed_contract(
        "VC-17a",
        "Acme Registry",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://a.example/breach",
        &sk,
        "fp:acme",
    );
    let c_app = build_signed_contract(
        "VC-17b",
        "App Store Inc",
        VendorKind::ApplicationStore,
        VendorTrustClass::CommunityVerified,
        "app@example.com",
        60,
        "https://b.example/breach",
        &sk,
        "fp:acme",
    );

    registry.admit_contract(c_oci).await.unwrap();
    registry.admit_contract(c_app).await.unwrap();

    let oci_list = registry.list_by_kind(VendorKind::OciRegistry).await;
    assert_eq!(oci_list.len(), 1);
    assert_eq!(oci_list[0].vendor_name, "Acme Registry");

    let app_list = registry.list_by_kind(VendorKind::ApplicationStore).await;
    assert_eq!(app_list.len(), 1);
    assert_eq!(app_list[0].vendor_name, "App Store Inc");

    let cve_list = registry.list_by_kind(VendorKind::CveFeed).await;
    assert!(cve_list.is_empty());
}

// ---------------------------------------------------------------------------
// list_in_state
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_in_state_returns_only_matching_label() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let c1 = build_signed_contract(
        "VC-18a",
        "Vendor A",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "a@example.com",
        90,
        "https://a.example/breach",
        &sk,
        "fp:acme",
    );
    let c2 = build_signed_contract(
        "VC-18b",
        "Vendor B",
        VendorKind::ApplicationStore,
        VendorTrustClass::CommunityVerified,
        "b@example.com",
        60,
        "https://b.example/breach",
        &sk,
        "fp:acme",
    );

    registry.admit_contract(c1).await.unwrap();
    registry.admit_contract(c2).await.unwrap();

    // Both in Proposed
    let proposed_ids = registry
        .list_in_state(IntegrationLifecycleLabel::Proposed)
        .await;
    assert_eq!(proposed_ids.len(), 2);

    // Move VC-18a to Retired
    registry
        .transition_lifecycle(
            &VendorContractId("VC-18a".into()),
            IntegrationLifecycleState::Retired {
                since: Utc::now(),
                reason: "done".into(),
                data_migration_completed: true,
            },
        )
        .await
        .unwrap();

    let proposed_after = registry
        .list_in_state(IntegrationLifecycleLabel::Proposed)
        .await;
    assert_eq!(proposed_after.len(), 1);

    let retired = registry
        .list_in_state(IntegrationLifecycleLabel::Retired)
        .await;
    assert_eq!(retired.len(), 1);
}

// ---------------------------------------------------------------------------
// add_to_blacklist + admit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn add_to_blacklist_then_admit_same_vendor_rejects() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    registry.add_to_blacklist("BadVendor").await.unwrap();

    assert!(registry.is_blacklisted("BadVendor").await);
    assert!(!registry.is_blacklisted("GoodVendor").await);

    let contract = build_signed_contract(
        "VC-19",
        "BadVendor",
        VendorKind::MetricsExporter,
        VendorTrustClass::OperatorAuthorised,
        "bad@example.com",
        60,
        "https://bad.example/breach",
        &sk,
        "fp:acme",
    );

    let result = registry.admit_contract(contract).await;
    assert!(matches!(
        result,
        Err(IntegrationError::VendorBlacklisted { .. })
    ));
}

// ---------------------------------------------------------------------------
// revoke_contract
// ---------------------------------------------------------------------------

#[tokio::test]
async fn revoke_contract_forces_transition_to_retired() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let contract = build_signed_contract(
        "VC-20",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:acme",
    );
    let cid = VendorContractId("VC-20".into());
    registry.admit_contract(contract).await.unwrap();

    // Forward through to Production first
    registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Evaluated {
                evaluator: "bob".into(),
                evaluated_at: Utc::now(),
                security_audit_passed: true,
            },
        )
        .await
        .unwrap();
    registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Piloted {
                since: Utc::now(),
                profile: "DEV_RELAXED".into(),
            },
        )
        .await
        .unwrap();
    registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Production { since: Utc::now() },
        )
        .await
        .unwrap();

    // Revoke (bypasses transition table)
    let result = registry.revoke_contract(&cid, "security breach").await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    let state = registry.current_lifecycle(&cid).await;
    assert!(matches!(
        state,
        Some(IntegrationLifecycleState::Retired { .. })
    ));

    // Revoke unknown contract
    let unknown = registry
        .revoke_contract(&VendorContractId("VC-NOPE".into()), "nope")
        .await;
    assert!(matches!(unknown, Err(IntegrationError::Internal(_))));
}

// ---------------------------------------------------------------------------
// concurrent admit — 3 distinct contracts no panic
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_admit_3_distinct_contracts_no_panic() {
    use std::sync::Arc;

    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);
    let registry = Arc::new(registry);

    let c1 = build_signed_contract(
        "VC-21a",
        "Vendor A",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "a@example.com",
        90,
        "https://a.example/breach",
        &sk,
        "fp:acme",
    );
    let c2 = build_signed_contract(
        "VC-21b",
        "Vendor B",
        VendorKind::ApplicationStore,
        VendorTrustClass::CommunityVerified,
        "b@example.com",
        60,
        "https://b.example/breach",
        &sk,
        "fp:acme",
    );
    let c3 = build_signed_contract(
        "VC-21c",
        "Vendor C",
        VendorKind::IdentityProvider,
        VendorTrustClass::AiosCertifiedPartner,
        "c@example.com",
        180,
        "https://c.example/breach",
        &sk,
        "fp:acme",
    );

    let r1 = registry.clone();
    let r2 = registry.clone();
    let r3 = registry.clone();

    let (res1, res2, res3) = tokio::join!(
        r1.admit_contract(c1),
        r2.admit_contract(c2),
        r3.admit_contract(c3),
    );

    assert!(res1.is_ok(), "c1 failed: {res1:?}");
    assert!(res2.is_ok(), "c2 failed: {res2:?}");
    assert!(res3.is_ok(), "c3 failed: {res3:?}");

    let all = registry.list_contracts().await;
    assert_eq!(all.len(), 3);
}

// ---------------------------------------------------------------------------
// Evaluated with security_audit_passed=false should NOT allow Piloted
// ---------------------------------------------------------------------------

#[tokio::test]
async fn evaluated_with_failed_audit_denies_piloted() {
    let mut registry = VendorIntegrationRegistry::new();
    let (sk, vk) = make_keypair();
    registry.register_authority("fp:acme", vk);

    let contract = build_signed_contract(
        "VC-22",
        "Acme Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        "acme@example.com",
        90,
        "https://acme.example/breach",
        &sk,
        "fp:acme",
    );
    let cid = VendorContractId("VC-22".into());
    registry.admit_contract(contract).await.unwrap();
    registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Evaluated {
                evaluator: "bob".into(),
                evaluated_at: Utc::now(),
                security_audit_passed: false,
            },
        )
        .await
        .unwrap();

    let result = registry
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Piloted {
                since: Utc::now(),
                profile: "DEV_RELAXED".into(),
            },
        )
        .await;

    assert!(matches!(
        result,
        Err(IntegrationError::LifecycleInvalidTransition { .. })
    ));
}

// ---------------------------------------------------------------------------
// Vendorkind + VendorTrustClass + IntegrationLifecycleLabel labels
// ---------------------------------------------------------------------------

#[test]
fn vendor_kind_label_non_empty_and_unique() {
    use std::collections::HashSet;
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
    let mut labels = HashSet::new();
    for k in &kinds {
        let label = k.label();
        assert!(!label.is_empty(), "label must not be empty for {k:?}");
        assert!(labels.insert(label), "duplicate label for {k:?}");
    }
    assert_eq!(labels.len(), kinds.len());
}

#[test]
fn vendor_trust_class_label_non_empty_and_unique() {
    use std::collections::HashSet;
    let classes = [
        VendorTrustClass::AiosCertifiedPartner,
        VendorTrustClass::CommunityVerified,
        VendorTrustClass::OperatorAuthorised,
        VendorTrustClass::BlacklistedDoNotAdmit,
    ];
    let mut labels = HashSet::new();
    for c in &classes {
        let label = c.label();
        assert!(!label.is_empty(), "label must not be empty for {c:?}");
        assert!(labels.insert(label), "duplicate label for {c:?}");
    }
    assert_eq!(labels.len(), classes.len());
}

#[test]
fn lifecycle_label_label_non_empty_and_unique() {
    use std::collections::HashSet;
    let labels = [
        IntegrationLifecycleLabel::Proposed,
        IntegrationLifecycleLabel::Evaluated,
        IntegrationLifecycleLabel::Piloted,
        IntegrationLifecycleLabel::Production,
        IntegrationLifecycleLabel::Deprecated,
        IntegrationLifecycleLabel::Retired,
    ];
    let mut label_strs = HashSet::new();
    for l in &labels {
        let s = l.label();
        assert!(!s.is_empty(), "label must not be empty for {l:?}");
        assert!(label_strs.insert(s), "duplicate label for {l:?}");
    }
    assert_eq!(label_strs.len(), labels.len());
}
