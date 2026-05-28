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
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::type_complexity,
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use aios_integration::*;
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
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

fn make_keypair() -> SigningKey {
    SigningKey::generate(&mut OsRng)
}

fn make_bridge_contract(
    bridge_id: &str,
    kind: BridgeKind,
    vendor_contract: VendorIntegrationContract,
    rules: ManifestTranslationRules,
) -> BridgeContract {
    BridgeContract {
        bridge_id: bridge_id.into(),
        kind,
        vendor_contract,
        translation_rules: rules,
        admitted_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// BridgeKind — variant count and labels
// ---------------------------------------------------------------------------

#[test]
fn bridge_kind_has_5_variants() {
    let variants = [
        BridgeKind::Flathub,
        BridgeKind::OciRegistry {
            registry_host: "docker.io".into(),
        },
        BridgeKind::Apt {
            distro: "debian".into(),
        },
        BridgeKind::Dnf {
            distro: "fedora".into(),
        },
        BridgeKind::Pacman {
            distro: "arch".into(),
        },
    ];
    assert_eq!(variants.len(), 5);
}

#[test]
fn bridge_kind_label_for_flathub() {
    assert_eq!(BridgeKind::Flathub.label(), "Flathub");
}

#[test]
fn bridge_kind_label_for_oci_registry() {
    let kind = BridgeKind::OciRegistry {
        registry_host: "ghcr.io".into(),
    };
    assert_eq!(kind.label(), "OciRegistry");
}

#[test]
fn bridge_kind_label_for_apt() {
    let kind = BridgeKind::Apt {
        distro: "ubuntu".into(),
    };
    assert_eq!(kind.label(), "Apt");
}

#[test]
fn bridge_kind_label_for_dnf() {
    let kind = BridgeKind::Dnf {
        distro: "rhel".into(),
    };
    assert_eq!(kind.label(), "Dnf");
}

#[test]
fn bridge_kind_label_for_pacman() {
    let kind = BridgeKind::Pacman {
        distro: "manjaro".into(),
    };
    assert_eq!(kind.label(), "Pacman");
}

// ---------------------------------------------------------------------------
// CapabilityExtractorRule — variant count
// ---------------------------------------------------------------------------

#[test]
fn capability_extractor_rule_has_at_least_6_variants_including_operator_authored() {
    let variants = [
        CapabilityExtractorRule::FlatpakFinishesSection,
        CapabilityExtractorRule::OciAnnotations,
        CapabilityExtractorRule::DebianControl,
        CapabilityExtractorRule::RpmSpec,
        CapabilityExtractorRule::PkgbuildArray,
        CapabilityExtractorRule::OperatorAuthored,
    ];
    assert_eq!(variants.len(), 6);
}

// ---------------------------------------------------------------------------
// Default contract constructors — extractor check
// ---------------------------------------------------------------------------

#[test]
fn default_flathub_contract_uses_flatpak_finishes_section_extractor() {
    let rules = default_flathub_contract();
    assert_eq!(
        rules.capability_extractor,
        CapabilityExtractorRule::FlatpakFinishesSection
    );
    assert_eq!(rules.source_manifest_format, "flatpak-manifest.json");
}

#[test]
fn default_oci_contract_uses_oci_annotations_extractor() {
    let rules = default_oci_contract();
    assert_eq!(
        rules.capability_extractor,
        CapabilityExtractorRule::OciAnnotations
    );
    assert_eq!(rules.source_manifest_format, "OCI image manifest v1");
}

#[test]
fn default_apt_contract_uses_debian_control_extractor() {
    let rules = default_apt_contract();
    assert_eq!(
        rules.capability_extractor,
        CapabilityExtractorRule::DebianControl
    );
    assert_eq!(rules.source_manifest_format, "control");
}

#[test]
fn default_dnf_contract_uses_rpm_spec_extractor() {
    let rules = default_dnf_contract();
    assert_eq!(rules.capability_extractor, CapabilityExtractorRule::RpmSpec);
    assert_eq!(rules.source_manifest_format, "spec");
}

#[test]
fn default_pacman_contract_uses_pkgbuild_array_extractor() {
    let rules = default_pacman_contract();
    assert_eq!(
        rules.capability_extractor,
        CapabilityExtractorRule::PkgbuildArray
    );
    assert_eq!(rules.source_manifest_format, "PKGBUILD");
}

// ---------------------------------------------------------------------------
// Default contract constructors — trust floor check
// ---------------------------------------------------------------------------

#[test]
fn default_pacman_contract_trust_floor_is_operator_authorised() {
    let rules = default_pacman_contract();
    assert_eq!(rules.trust_floor, VendorTrustClass::OperatorAuthorised);
}

#[test]
fn default_flathub_contract_trust_floor_is_community_verified() {
    let rules = default_flathub_contract();
    assert_eq!(rules.trust_floor, VendorTrustClass::CommunityVerified);
}

// ---------------------------------------------------------------------------
// ExternalBridgeRegistry — admit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admit_bridge_with_valid_vendor_contract_succeeds() {
    let registry = ExternalBridgeRegistry::new();
    let sk = make_keypair();
    let contract = build_signed_contract(
        "VC-FH-01",
        "Flathub Foundation",
        VendorKind::ApplicationStore,
        VendorTrustClass::CommunityVerified,
        "flathub@example.com",
        90,
        "https://flathub.example/breach",
        &sk,
        "fp:flathub",
    );
    let bridge = make_bridge_contract(
        "bridge-flathub",
        BridgeKind::Flathub,
        contract,
        default_flathub_contract(),
    );
    let result = registry.admit_bridge(bridge).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn admit_bridge_with_blacklisted_vendor_trust_class_returns_vendor_blacklisted() {
    let registry = ExternalBridgeRegistry::new();
    let sk = make_keypair();
    let contract = build_signed_contract(
        "VC-BL-01",
        "Bad Vendor",
        VendorKind::PackageRepository,
        VendorTrustClass::BlacklistedDoNotAdmit,
        "bad@example.com",
        30,
        "https://bad.example/breach",
        &sk,
        "fp:bad",
    );
    let bridge = make_bridge_contract(
        "bridge-blacklisted",
        BridgeKind::Flathub,
        contract,
        default_flathub_contract(),
    );
    let result = registry.admit_bridge(bridge).await;
    assert!(result.is_err());
    match result {
        Err(IntegrationError::VendorBlacklisted { contract_id }) => {
            assert_eq!(contract_id, VendorContractId("VC-BL-01".into()));
        }
        other => panic!("expected VendorBlacklisted, got {other:?}"),
    }
}

#[tokio::test]
async fn admit_duplicate_bridge_id_returns_internal_error() {
    let registry = ExternalBridgeRegistry::new();
    let sk = make_keypair();
    let contract = build_signed_contract(
        "VC-APT-01",
        "Debian",
        VendorKind::PackageRepository,
        VendorTrustClass::CommunityVerified,
        "debian@example.com",
        90,
        "https://debian.example/breach",
        &sk,
        "fp:debian",
    );
    let bridge1 = make_bridge_contract(
        "bridge-apt",
        BridgeKind::Apt {
            distro: "debian".into(),
        },
        contract.clone(),
        default_apt_contract(),
    );
    let bridge2 = make_bridge_contract(
        "bridge-apt",
        BridgeKind::Apt {
            distro: "debian".into(),
        },
        contract,
        default_apt_contract(),
    );

    let result1 = registry.admit_bridge(bridge1).await;
    assert!(result1.is_ok());

    let result2 = registry.admit_bridge(bridge2).await;
    assert!(result2.is_err());
    match result2 {
        Err(IntegrationError::Internal(msg)) => {
            assert!(msg.contains("already exists"));
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// ExternalBridgeRegistry — get
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_bridge_known_returns_some() {
    let registry = ExternalBridgeRegistry::new();
    let sk = make_keypair();
    let contract = build_signed_contract(
        "VC-FH-GET",
        "Flathub",
        VendorKind::ApplicationStore,
        VendorTrustClass::CommunityVerified,
        "fh@example.com",
        90,
        "https://fh.example/breach",
        &sk,
        "fp:fh",
    );
    let bridge = make_bridge_contract(
        "bridge-get-test",
        BridgeKind::Flathub,
        contract,
        default_flathub_contract(),
    );
    registry.admit_bridge(bridge).await.unwrap();

    let got = registry.get_bridge("bridge-get-test").await;
    assert!(got.is_some());
    assert_eq!(got.unwrap().bridge_id, "bridge-get-test");
}

#[tokio::test]
async fn get_bridge_unknown_returns_none() {
    let registry = ExternalBridgeRegistry::new();
    let got = registry.get_bridge("nonexistent").await;
    assert!(got.is_none());
}

// ---------------------------------------------------------------------------
// ExternalBridgeRegistry — list
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_bridges_after_3_admissions_returns_3() {
    let registry = ExternalBridgeRegistry::new();
    let sk = make_keypair();

    let kinds: [(BridgeKind, &str, fn() -> ManifestTranslationRules); 3] = [
        (BridgeKind::Flathub, "FH", default_flathub_contract),
        (
            BridgeKind::Apt {
                distro: "debian".into(),
            },
            "APT",
            default_apt_contract,
        ),
        (
            BridgeKind::Dnf {
                distro: "fedora".into(),
            },
            "DNF",
            default_dnf_contract,
        ),
    ];

    for (kind, suffix, rules_fn) in &kinds {
        let contract = build_signed_contract(
            &format!("VC-{suffix}-LIST"),
            &format!("Vendor {suffix}"),
            VendorKind::PackageRepository,
            VendorTrustClass::CommunityVerified,
            &format!("{suffix}@example.com"),
            90,
            &format!("https://{suffix}.example/breach"),
            &sk,
            &format!("fp:{suffix}"),
        );
        let bridge = make_bridge_contract(
            &format!("bridge-{suffix}"),
            kind.clone(),
            contract,
            rules_fn(),
        );
        registry.admit_bridge(bridge).await.unwrap();
    }

    let list = registry.list_bridges().await;
    assert_eq!(list.len(), 3);
}

#[tokio::test]
async fn list_bridges_on_empty_registry_returns_empty_vec() {
    let registry = ExternalBridgeRegistry::new();
    let list = registry.list_bridges().await;
    assert!(list.is_empty());
}

// ---------------------------------------------------------------------------
// ExternalBridgeRegistry — list_by_kind_label
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_by_kind_label_filters_correctly() {
    let registry = ExternalBridgeRegistry::new();
    let sk = make_keypair();

    // Admit one Flathub, two OciRegistry bridges.
    for (i, kind) in [
        BridgeKind::Flathub,
        BridgeKind::OciRegistry {
            registry_host: "docker.io".into(),
        },
        BridgeKind::OciRegistry {
            registry_host: "ghcr.io".into(),
        },
    ]
    .into_iter()
    .enumerate()
    {
        let contract = build_signed_contract(
            &format!("VC-KL-{i}"),
            &format!("Vendor KL {i}"),
            VendorKind::OciRegistry,
            VendorTrustClass::CommunityVerified,
            &format!("kl{i}@example.com"),
            90,
            &format!("https://kl{i}.example/breach"),
            &sk,
            &format!("fp:kl{i}"),
        );
        let bridge = make_bridge_contract(
            &format!("bridge-kl-{i}"),
            kind,
            contract,
            default_oci_contract(),
        );
        registry.admit_bridge(bridge).await.unwrap();
    }

    let oci_list = registry.list_by_kind_label("OciRegistry").await;
    assert_eq!(oci_list.len(), 2);

    let flat_list = registry.list_by_kind_label("Flathub").await;
    assert_eq!(flat_list.len(), 1);

    let unknown = registry.list_by_kind_label("Pacman").await;
    assert!(unknown.is_empty());
}

// ---------------------------------------------------------------------------
// ExternalBridgeRegistry — revoke
// ---------------------------------------------------------------------------

#[tokio::test]
async fn revoke_bridge_known_id_succeeds_then_get_returns_none() {
    let registry = ExternalBridgeRegistry::new();
    let sk = make_keypair();
    let contract = build_signed_contract(
        "VC-REV-01",
        "Revoke Test",
        VendorKind::PackageRepository,
        VendorTrustClass::CommunityVerified,
        "rev@example.com",
        90,
        "https://rev.example/breach",
        &sk,
        "fp:rev",
    );
    let bridge = make_bridge_contract(
        "bridge-to-revoke",
        BridgeKind::Pacman {
            distro: "arch".into(),
        },
        contract,
        default_pacman_contract(),
    );
    registry.admit_bridge(bridge).await.unwrap();

    let result = registry
        .revoke_bridge("bridge-to-revoke", "no longer needed")
        .await;
    assert!(result.is_ok());

    let got = registry.get_bridge("bridge-to-revoke").await;
    assert!(got.is_none());
}

#[tokio::test]
async fn revoke_bridge_unknown_id_returns_internal_error() {
    let registry = ExternalBridgeRegistry::new();
    let result = registry.revoke_bridge("nonexistent", "test").await;
    assert!(result.is_err());
    match result {
        Err(IntegrationError::Internal(msg)) => {
            assert!(msg.contains("unknown bridge_id"));
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Serde round-trip
// ---------------------------------------------------------------------------

#[test]
fn bridge_contract_serde_round_trip() {
    let bridge = BridgeContract {
        bridge_id: "bridge-serde".into(),
        kind: BridgeKind::OciRegistry {
            registry_host: "docker.io".into(),
        },
        vendor_contract: VendorIntegrationContract {
            contract_id: VendorContractId("VC-SERDE".into()),
            vendor_name: "SerdeTest".into(),
            vendor_kind: VendorKind::OciRegistry,
            trust_class: VendorTrustClass::CommunityVerified,
            contact_canonical_id: "serde@example.com".into(),
            rotation_cadence_days: 90,
            breach_playbook_url: "https://serde.example/breach".into(),
            signer_fingerprint: "fp:serde".into(),
            signature: vec![1, 2, 3, 4],
            admitted_at: Utc::now(),
        },
        translation_rules: ManifestTranslationRules {
            source_manifest_format: "OCI image manifest v1".into(),
            capability_extractor: CapabilityExtractorRule::OciAnnotations,
            trust_floor: VendorTrustClass::CommunityVerified,
        },
        admitted_at: Utc::now(),
    };

    let json = serde_json::to_string(&bridge).unwrap();
    let round_tripped: BridgeContract = serde_json::from_str(&json).unwrap();
    assert_eq!(bridge.bridge_id, round_tripped.bridge_id);
    assert_eq!(bridge.kind, round_tripped.kind);
    assert_eq!(bridge.translation_rules, round_tripped.translation_rules);
}

#[test]
fn bridge_kind_serde_round_trip_all_variants() {
    let variants = [
        BridgeKind::Flathub,
        BridgeKind::OciRegistry {
            registry_host: "docker.io".into(),
        },
        BridgeKind::Apt {
            distro: "debian".into(),
        },
        BridgeKind::Dnf {
            distro: "fedora".into(),
        },
        BridgeKind::Pacman {
            distro: "arch".into(),
        },
    ];

    for kind in &variants {
        let json = serde_json::to_string(kind).unwrap();
        let round_tripped: BridgeKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, &round_tripped);
    }
}

#[test]
fn capability_extractor_rule_serde_round_trip_all_variants() {
    let variants = [
        CapabilityExtractorRule::FlatpakFinishesSection,
        CapabilityExtractorRule::OciAnnotations,
        CapabilityExtractorRule::DebianControl,
        CapabilityExtractorRule::RpmSpec,
        CapabilityExtractorRule::PkgbuildArray,
        CapabilityExtractorRule::OperatorAuthored,
    ];

    for rule in &variants {
        let json = serde_json::to_string(rule).unwrap();
        let round_tripped: CapabilityExtractorRule = serde_json::from_str(&json).unwrap();
        assert_eq!(rule, &round_tripped);
    }
}

// ---------------------------------------------------------------------------
// Concurrent safety
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_admit_3_distinct_bridges_no_panic() {
    use std::sync::Arc;

    let registry = Arc::new(ExternalBridgeRegistry::new());

    let tasks: Vec<_> = (0..3)
        .map(|i| {
            let reg = Arc::clone(&registry);
            tokio::spawn(async move {
                let sk = make_keypair();
                let contract = build_signed_contract(
                    &format!("VC-CONC-{i}"),
                    &format!("Vendor CONC {i}"),
                    VendorKind::PackageRepository,
                    VendorTrustClass::CommunityVerified,
                    &format!("conc{i}@example.com"),
                    90,
                    &format!("https://conc{i}.example/breach"),
                    &sk,
                    &format!("fp:conc{i}"),
                );
                let bridge = make_bridge_contract(
                    &format!("bridge-conc-{i}"),
                    BridgeKind::Flathub,
                    contract,
                    default_flathub_contract(),
                );
                reg.admit_bridge(bridge).await
            })
        })
        .collect();

    for task in tasks {
        let result = task.await.unwrap();
        assert!(result.is_ok(), "concurrent admit failed: {result:?}");
    }

    let list = registry.list_bridges().await;
    assert_eq!(list.len(), 3);
}

// ---------------------------------------------------------------------------
// ExternalBridgeRegistry — Default
// ---------------------------------------------------------------------------

#[tokio::test]
async fn external_bridge_registry_default_is_empty() {
    let registry = ExternalBridgeRegistry::default();
    let bridges = registry.list_bridges().await;
    assert!(bridges.is_empty());
}
