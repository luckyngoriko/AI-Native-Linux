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

use aios_integration::service::proto::integration_service_client::IntegrationServiceClient;
use aios_integration::service::proto::*;
use aios_integration::service::{build_router, IntegrationServer};
use aios_integration::*;
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

struct Harness {
    client: IntegrationServiceClient<tonic::transport::Channel>,
    _shutdown: oneshot::Sender<()>,
    _handle: tokio::task::JoinHandle<()>,
}

impl Harness {
    async fn new(server: IntegrationServer) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let (tx, rx) = oneshot::channel::<()>();
        let router = build_router(server);
        let handle = tokio::spawn(async move {
            router
                .serve_with_shutdown(addr, async {
                    let _ = rx.await;
                })
                .await
                .unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let client = IntegrationServiceClient::connect(format!("http://{addr}"))
            .await
            .unwrap();

        Self {
            client,
            _shutdown: tx,
            _handle: handle,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn make_keypair() -> SigningKey {
    SigningKey::generate(&mut OsRng)
}

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

// ---------------------------------------------------------------------------
// Vendor contract management (3 tests)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admit_contract_and_get_contract_round_trip() {
    let sk = make_keypair();
    let vk = sk.verifying_key();
    let fingerprint = "fp:vendor-test-1";

    let contract = build_signed_contract(
        "VC-TEST-001",
        "Test Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::AiosCertifiedPartner,
        "test@example.com",
        90,
        "https://example.com/breach",
        &sk,
        fingerprint,
    );

    let mut vendor_registry = VendorIntegrationRegistry::new();
    vendor_registry.register_authority(fingerprint, vk);

    let server = IntegrationServer::new(
        Arc::new(vendor_registry),
        Arc::new(ExternalStandardRegistry::new()),
        Arc::new(CveFeedShape::new()),
        Arc::new(ExternalBridgeRegistry::new()),
        Arc::new(Orchestrator::from_default_composition().unwrap()),
        Arc::new(ControlMapRegistry::new()),
    );

    let mut harness = Harness::new(server).await;

    // Admit
    let proto_contract = vendor_contract_to_proto_test(&contract);
    let _ = harness
        .client
        .admit_contract(AdmitContractRequest {
            contract: Some(proto_contract),
        })
        .await
        .unwrap();

    // Get
    let resp = harness
        .client
        .get_contract(GetContractRequest {
            contract_id: "VC-TEST-001".into(),
        })
        .await
        .unwrap();

    let got = resp.into_inner().contract.unwrap();
    assert_eq!(got.contract_id, "VC-TEST-001");
    assert_eq!(got.vendor_name, "Test Corp");
}

#[tokio::test]
async fn list_contracts_after_two_admissions_returns_two() {
    let sk = make_keypair();
    let vk = sk.verifying_key();

    let mut vendor_registry = VendorIntegrationRegistry::new();
    vendor_registry.register_authority("fp:list-test", vk);

    let contract_a = build_signed_contract(
        "VC-LIST-A",
        "Vendor A",
        VendorKind::PackageRepository,
        VendorTrustClass::CommunityVerified,
        "a@example.com",
        90,
        "https://a.example/breach",
        &sk,
        "fp:list-test",
    );
    let contract_b = build_signed_contract(
        "VC-LIST-B",
        "Vendor B",
        VendorKind::ApplicationStore,
        VendorTrustClass::OperatorAuthorised,
        "b@example.com",
        60,
        "https://b.example/breach",
        &sk,
        "fp:list-test",
    );

    let server = IntegrationServer::new(
        Arc::new(vendor_registry),
        Arc::new(ExternalStandardRegistry::new()),
        Arc::new(CveFeedShape::new()),
        Arc::new(ExternalBridgeRegistry::new()),
        Arc::new(Orchestrator::from_default_composition().unwrap()),
        Arc::new(ControlMapRegistry::new()),
    );

    let mut harness = Harness::new(server).await;

    // We need to use the registry directly to admit (since the server's vendor_registry
    // is in the Arc). Actually, let's admit through the RPC instead.
    // But first, the RPC needs the proto conversion. Let's use vendor_contract_to_proto.
    // Actually, we need the proto conversion function from conversions.
    // Let's use the server's AdmitContract RPC directly.

    // Admit both through RPC
    for contract in [&contract_a, &contract_b] {
        let proto = vendor_contract_to_proto_test(contract);
        let _ = harness
            .client
            .admit_contract(AdmitContractRequest {
                contract: Some(proto),
            })
            .await
            .unwrap();
    }

    let resp = harness
        .client
        .list_contracts(ListContractsRequest {})
        .await
        .unwrap();

    let list = resp.into_inner().contracts;
    assert_eq!(list.len(), 2);
}

#[tokio::test]
async fn get_contract_unknown_returns_not_found() {
    let server = IntegrationServer::new(
        Arc::new(VendorIntegrationRegistry::new()),
        Arc::new(ExternalStandardRegistry::new()),
        Arc::new(CveFeedShape::new()),
        Arc::new(ExternalBridgeRegistry::new()),
        Arc::new(Orchestrator::from_default_composition().unwrap()),
        Arc::new(ControlMapRegistry::new()),
    );

    let mut harness = Harness::new(server).await;

    let err = harness
        .client
        .get_contract(GetContractRequest {
            contract_id: "NONEXISTENT".into(),
        })
        .await
        .unwrap_err();

    assert_eq!(err.code(), tonic::Code::NotFound);
}

// ---------------------------------------------------------------------------
// Standards subscription (2 tests)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn subscribe_and_get_status_round_trip() {
    let server = IntegrationServer::new(
        Arc::new(VendorIntegrationRegistry::new()),
        Arc::new(ExternalStandardRegistry::new()),
        Arc::new(CveFeedShape::new()),
        Arc::new(ExternalBridgeRegistry::new()),
        Arc::new(Orchestrator::from_default_composition().unwrap()),
        Arc::new(ControlMapRegistry::new()),
    );

    let mut harness = Harness::new(server).await;

    let now = Utc::now();
    let future = now + chrono::Duration::days(90);
    let sub = StandardSubscriptionProto {
        subscription_id: "SS-TEST-001".into(),
        standard: StandardKindProto::Nist80053Rev5 as i32,
        catalog_url: "https://nvd.nist.gov/800-53".into(),
        current_revision: "Rev.5".into(),
        last_reviewed_at: Some(prost_types::Timestamp {
            seconds: now.timestamp(),
            nanos: now.timestamp_subsec_nanos() as i32,
        }),
        next_review_due_at: Some(prost_types::Timestamp {
            seconds: future.timestamp(),
            nanos: future.timestamp_subsec_nanos() as i32,
        }),
        responsible_canonical_id: "bob@example.com".into(),
    };

    let _ = harness
        .client
        .subscribe(SubscribeRequest {
            subscription: Some(sub),
        })
        .await
        .unwrap();

    let resp = harness
        .client
        .get_subscription_status(GetSubscriptionStatusRequest {
            subscription_id: "SS-TEST-001".into(),
        })
        .await
        .unwrap();

    let inner = resp.into_inner();
    assert_eq!(inner.subscription_id, "SS-TEST-001");
    assert_eq!(inner.status, SubscriptionStatusProto::Current as i32);
}

#[tokio::test]
async fn list_subscriptions_after_two_returns_two() {
    let server = IntegrationServer::new(
        Arc::new(VendorIntegrationRegistry::new()),
        Arc::new(ExternalStandardRegistry::new()),
        Arc::new(CveFeedShape::new()),
        Arc::new(ExternalBridgeRegistry::new()),
        Arc::new(Orchestrator::from_default_composition().unwrap()),
        Arc::new(ControlMapRegistry::new()),
    );

    let mut harness = Harness::new(server).await;

    let now = Utc::now();
    let ts = Some(prost_types::Timestamp {
        seconds: now.timestamp(),
        nanos: now.timestamp_subsec_nanos() as i32,
    });

    for id in ["SS-LIST-A", "SS-LIST-B"] {
        let sub = StandardSubscriptionProto {
            subscription_id: id.into(),
            standard: StandardKindProto::Iso27001 as i32,
            catalog_url: "https://iso.example/27001".into(),
            current_revision: "2022".into(),
            last_reviewed_at: ts,
            next_review_due_at: ts,
            responsible_canonical_id: "ops@example.com".into(),
        };
        let _ = harness
            .client
            .subscribe(SubscribeRequest {
                subscription: Some(sub),
            })
            .await
            .unwrap();
    }

    let resp = harness
        .client
        .list_subscriptions(ListSubscriptionsRequest {})
        .await
        .unwrap();

    assert_eq!(resp.into_inner().subscriptions.len(), 2);
}

// ---------------------------------------------------------------------------
// CVE feed (3 tests)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ingest_cve_record_and_get_by_id_round_trip() {
    let server = IntegrationServer::new(
        Arc::new(VendorIntegrationRegistry::new()),
        Arc::new(ExternalStandardRegistry::new()),
        Arc::new(CveFeedShape::new()),
        Arc::new(ExternalBridgeRegistry::new()),
        Arc::new(Orchestrator::from_default_composition().unwrap()),
        Arc::new(ControlMapRegistry::new()),
    );

    let mut harness = Harness::new(server).await;

    let now = Utc::now();
    let ts = Some(prost_types::Timestamp {
        seconds: now.timestamp(),
        nanos: now.timestamp_subsec_nanos() as i32,
    });

    let record = CveRecordProto {
        cve_id: "CVE-2024-12345".into(),
        published_at: ts,
        last_modified_at: ts,
        cvss_v3_score: 7.5,
        severity: CveSeverityProto::High as i32,
        summary: "Test vulnerability".into(),
        affected_cpe_uris: vec!["cpe:2.3:*:*:*:*:*:*:*:*:*:*".into()],
    };

    let _ = harness
        .client
        .ingest_cve_record(IngestCveRecordRequest {
            record: Some(record),
        })
        .await
        .unwrap();

    let resp = harness
        .client
        .get_cve_record(GetCveRecordRequest {
            cve_id: "CVE-2024-12345".into(),
        })
        .await
        .unwrap();

    let got = resp.into_inner().record.unwrap();
    assert_eq!(got.cve_id, "CVE-2024-12345");
    assert_eq!(got.cvss_v3_score, 7.5);
}

#[tokio::test]
async fn list_cve_records_after_two_ingestions_returns_two() {
    let server = IntegrationServer::new(
        Arc::new(VendorIntegrationRegistry::new()),
        Arc::new(ExternalStandardRegistry::new()),
        Arc::new(CveFeedShape::new()),
        Arc::new(ExternalBridgeRegistry::new()),
        Arc::new(Orchestrator::from_default_composition().unwrap()),
        Arc::new(ControlMapRegistry::new()),
    );

    let mut harness = Harness::new(server).await;

    let now = Utc::now();
    let ts = Some(prost_types::Timestamp {
        seconds: now.timestamp(),
        nanos: now.timestamp_subsec_nanos() as i32,
    });

    for (id, score) in [("CVE-2024-10001", 3.0_f32), ("CVE-2024-20002", 8.0_f32)] {
        let record = CveRecordProto {
            cve_id: id.into(),
            published_at: ts,
            last_modified_at: ts,
            cvss_v3_score: score,
            severity: if score >= 7.0 {
                CveSeverityProto::High as i32
            } else {
                CveSeverityProto::Low as i32
            },
            summary: format!("Test {id}"),
            affected_cpe_uris: vec![],
        };
        let _ = harness
            .client
            .ingest_cve_record(IngestCveRecordRequest {
                record: Some(record),
            })
            .await
            .unwrap();
    }

    let resp = harness
        .client
        .list_cve_records(ListCveRecordsRequest {})
        .await
        .unwrap();

    assert_eq!(resp.into_inner().records.len(), 2);
}

#[tokio::test]
async fn bind_cve_to_package_round_trip() {
    let server = IntegrationServer::new(
        Arc::new(VendorIntegrationRegistry::new()),
        Arc::new(ExternalStandardRegistry::new()),
        Arc::new(CveFeedShape::new()),
        Arc::new(ExternalBridgeRegistry::new()),
        Arc::new(Orchestrator::from_default_composition().unwrap()),
        Arc::new(ControlMapRegistry::new()),
    );

    let mut harness = Harness::new(server).await;

    // First ingest a CVE record
    let now = Utc::now();
    let ts = Some(prost_types::Timestamp {
        seconds: now.timestamp(),
        nanos: now.timestamp_subsec_nanos() as i32,
    });

    let record = CveRecordProto {
        cve_id: "CVE-2024-55555".into(),
        published_at: ts,
        last_modified_at: ts,
        cvss_v3_score: 5.0,
        severity: CveSeverityProto::Medium as i32,
        summary: "Test bind".into(),
        affected_cpe_uris: vec![],
    };
    let _ = harness
        .client
        .ingest_cve_record(IngestCveRecordRequest {
            record: Some(record),
        })
        .await
        .unwrap();

    // Bind to a package
    let binding = PackageCveBindingProto {
        binding_id: "B-TEST-001".into(),
        cve_id: "CVE-2024-55555".into(),
        package_id: "PKG-TEST".into(),
        status: CveStatusProto::Unresolved as i32,
        bound_at: ts,
        matched_via_cpe: String::new(),
        mitigated_by: String::new(),
    };

    let _ = harness
        .client
        .bind_cve_to_package(BindCveToPackageRequest {
            binding: Some(binding),
        })
        .await
        .unwrap();

    // Verify via list
    let resp = harness
        .client
        .list_cve_bindings(ListCveBindingsRequest {})
        .await
        .unwrap();

    let bindings = resp.into_inner().bindings;
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].binding_id, "B-TEST-001");
    assert_eq!(bindings[0].package_id, "PKG-TEST");
}

// ---------------------------------------------------------------------------
// Bridge management (1 test)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admit_bridge_and_get_round_trip() {
    let sk = make_keypair();
    let vk = sk.verifying_key();
    let fingerprint = "fp:bridge-test";

    let contract = build_signed_contract(
        "VC-BR-TEST",
        "Bridge Vendor",
        VendorKind::ApplicationStore,
        VendorTrustClass::CommunityVerified,
        "bridge@example.com",
        90,
        "https://bridge.example/breach",
        &sk,
        fingerprint,
    );

    let mut vendor_registry = VendorIntegrationRegistry::new();
    vendor_registry.register_authority(fingerprint, vk);

    let bridge_registry = ExternalBridgeRegistry::new();
    // Admit vendor contract first
    vendor_registry
        .admit_contract(contract.clone())
        .await
        .unwrap();

    let server = IntegrationServer::new(
        Arc::new(vendor_registry),
        Arc::new(ExternalStandardRegistry::new()),
        Arc::new(CveFeedShape::new()),
        Arc::new(bridge_registry),
        Arc::new(Orchestrator::from_default_composition().unwrap()),
        Arc::new(ControlMapRegistry::new()),
    );

    let mut harness = Harness::new(server).await;

    let now = Utc::now();
    let ts = Some(prost_types::Timestamp {
        seconds: now.timestamp(),
        nanos: now.timestamp_subsec_nanos() as i32,
    });

    let bridge = BridgeContractProto {
        bridge_id: "bridge-test-1".into(),
        kind: BridgeKindProto::Flathub as i32,
        vendor_contract: Some(vendor_contract_to_proto_test(&contract)),
        translation_rules: Some(ManifestTranslationRulesProto {
            source_manifest_format: "flatpak-manifest.json".into(),
            capability_extractor: CapabilityExtractorRuleProto::FlatpakFinishesSection as i32,
            trust_floor: VendorTrustClassProto::CommunityVerified as i32,
        }),
        admitted_at: ts,
    };

    let _ = harness
        .client
        .admit_bridge(AdmitBridgeRequest {
            bridge: Some(bridge),
        })
        .await
        .unwrap();

    let resp = harness
        .client
        .get_bridge(GetBridgeRequest {
            bridge_id: "bridge-test-1".into(),
        })
        .await
        .unwrap();

    let got = resp.into_inner().bridge.unwrap();
    assert_eq!(got.bridge_id, "bridge-test-1");
    assert_eq!(got.kind, BridgeKindProto::Flathub as i32);
}

// ---------------------------------------------------------------------------
// Composition / orchestrator (2 tests)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_boot_order_returns_17_entries() {
    let server = IntegrationServer::new(
        Arc::new(VendorIntegrationRegistry::new()),
        Arc::new(ExternalStandardRegistry::new()),
        Arc::new(CveFeedShape::new()),
        Arc::new(ExternalBridgeRegistry::new()),
        Arc::new(Orchestrator::from_default_composition().unwrap()),
        Arc::new(ControlMapRegistry::new()),
    );

    let mut harness = Harness::new(server).await;

    let resp = harness
        .client
        .get_boot_order(GetBootOrderRequest {})
        .await
        .unwrap();

    let order = resp.into_inner().boot_order;
    assert_eq!(order.len(), 17);
}

#[tokio::test]
async fn health_summary_returns_17_entries() {
    let server = IntegrationServer::new(
        Arc::new(VendorIntegrationRegistry::new()),
        Arc::new(ExternalStandardRegistry::new()),
        Arc::new(CveFeedShape::new()),
        Arc::new(ExternalBridgeRegistry::new()),
        Arc::new(Orchestrator::from_default_composition().unwrap()),
        Arc::new(ControlMapRegistry::new()),
    );

    let mut harness = Harness::new(server).await;

    let resp = harness
        .client
        .health_summary(HealthSummaryRequest {})
        .await
        .unwrap();

    let summaries = resp.into_inner().summaries;
    assert_eq!(summaries.len(), 17);
}

// ---------------------------------------------------------------------------
// Control map (2 tests)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn add_control_mapping_and_snapshot_baseline_round_trip() {
    let server = IntegrationServer::new(
        Arc::new(VendorIntegrationRegistry::new()),
        Arc::new(ExternalStandardRegistry::new()),
        Arc::new(CveFeedShape::new()),
        Arc::new(ExternalBridgeRegistry::new()),
        Arc::new(Orchestrator::from_default_composition().unwrap()),
        Arc::new(ControlMapRegistry::new()),
    );

    let mut harness = Harness::new(server).await;

    let now = Utc::now();
    let ts = Some(prost_types::Timestamp {
        seconds: now.timestamp(),
        nanos: now.timestamp_subsec_nanos() as i32,
    });

    let mapping = ControlMappingProto {
        mapping_id: "MAP-001".into(),
        invariant: Some(AiosInvariantProto {
            invariant_id: "INV-001".into(),
            name: "Secrets Are Capabilities".into(),
            layer: "L4".into(),
        }),
        control_refs: vec![ControlFrameworkRefProto {
            framework: StandardKindProto::Nist80053Rev5 as i32,
            control_family: "AC".into(),
            control_id: "AC-3".into(),
        }],
        mapping_rationale: "access enforcement".into(),
        mapped_at: ts,
    };

    let _ = harness
        .client
        .add_control_mapping(AddControlMappingRequest {
            mapping: Some(mapping),
        })
        .await
        .unwrap();

    let resp = harness
        .client
        .snapshot_baseline(SnapshotBaselineRequest {
            baseline_id: "BL-001".into(),
            aios_version: "0.0.1".into(),
            validator_canonical_id: "auditor-1".into(),
        })
        .await
        .unwrap();

    let baseline = resp.into_inner().baseline.unwrap();
    assert_eq!(baseline.baseline_id, "BL-001");
    assert_eq!(baseline.mappings.len(), 1);
}

#[tokio::test]
async fn get_baseline_unknown_returns_not_found() {
    let server = IntegrationServer::new(
        Arc::new(VendorIntegrationRegistry::new()),
        Arc::new(ExternalStandardRegistry::new()),
        Arc::new(CveFeedShape::new()),
        Arc::new(ExternalBridgeRegistry::new()),
        Arc::new(Orchestrator::from_default_composition().unwrap()),
        Arc::new(ControlMapRegistry::new()),
    );

    let mut harness = Harness::new(server).await;

    let err = harness
        .client
        .get_baseline(GetBaselineRequest {
            baseline_id: "NONEXISTENT".into(),
        })
        .await
        .unwrap_err();

    assert_eq!(err.code(), tonic::Code::NotFound);
}

// ---------------------------------------------------------------------------
// Info (1 test)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_integration_info_returns_nonzero_counts() {
    let sk = make_keypair();
    let vk = sk.verifying_key();

    let mut vendor_registry = VendorIntegrationRegistry::new();
    vendor_registry.register_authority("fp:info-test", vk);

    let contract = build_signed_contract(
        "VC-INFO-TEST",
        "Info Corp",
        VendorKind::OciRegistry,
        VendorTrustClass::AiosCertifiedPartner,
        "info@example.com",
        90,
        "https://info.example/breach",
        &sk,
        "fp:info-test",
    );
    vendor_registry
        .admit_contract(contract.clone())
        .await
        .unwrap();

    let server = IntegrationServer::new(
        Arc::new(vendor_registry),
        Arc::new(ExternalStandardRegistry::new()),
        Arc::new(CveFeedShape::new()),
        Arc::new(ExternalBridgeRegistry::new()),
        Arc::new(Orchestrator::from_default_composition().unwrap()),
        Arc::new(ControlMapRegistry::new()),
    );

    let mut harness = Harness::new(server).await;

    let resp = harness
        .client
        .get_integration_info(GetIntegrationInfoRequest {})
        .await
        .unwrap();

    let info = resp.into_inner();
    assert_eq!(info.vendor_contract_count, 1);
    assert_eq!(info.composition_service_count, 17);
    assert!(info.code_version.starts_with("aios-integration/"));
    assert_eq!(info.schema_version, "aios.integration");
}

// ---------------------------------------------------------------------------
// Proto conversion helpers (inline to avoid crate visibility issues)
// ---------------------------------------------------------------------------

fn vendor_contract_to_proto_test(c: &VendorIntegrationContract) -> VendorIntegrationContractProto {
    VendorIntegrationContractProto {
        contract_id: c.contract_id.0.clone(),
        vendor_name: c.vendor_name.clone(),
        vendor_kind: vendor_kind_to_proto_test(c.vendor_kind),
        trust_class: vendor_trust_class_to_proto_test(c.trust_class),
        contact_canonical_id: c.contact_canonical_id.clone(),
        rotation_cadence_days: c.rotation_cadence_days,
        breach_playbook_url: c.breach_playbook_url.clone(),
        signer_fingerprint: c.signer_fingerprint.clone(),
        signature: c.signature.clone(),
        admitted_at: Some(prost_types::Timestamp {
            seconds: c.admitted_at.timestamp(),
            nanos: c.admitted_at.timestamp_subsec_nanos() as i32,
        }),
    }
}

fn vendor_kind_to_proto_test(k: VendorKind) -> i32 {
    match k {
        VendorKind::PackageRepository => VendorKindProto::PackageRepository as i32,
        VendorKind::ApplicationStore => VendorKindProto::ApplicationStore as i32,
        VendorKind::OciRegistry => VendorKindProto::OciRegistry as i32,
        VendorKind::CveFeed => VendorKindProto::CveFeed as i32,
        VendorKind::ComplianceProvider => VendorKindProto::ComplianceProvider as i32,
        VendorKind::MetricsExporter => VendorKindProto::MetricsExporter as i32,
        VendorKind::IdentityProvider => VendorKindProto::IdentityProvider as i32,
        VendorKind::OtherCertified => VendorKindProto::OtherCertified as i32,
    }
}

fn vendor_trust_class_to_proto_test(t: VendorTrustClass) -> i32 {
    match t {
        VendorTrustClass::AiosCertifiedPartner => {
            VendorTrustClassProto::AiosCertifiedPartner as i32
        }
        VendorTrustClass::CommunityVerified => VendorTrustClassProto::CommunityVerified as i32,
        VendorTrustClass::OperatorAuthorised => VendorTrustClassProto::OperatorAuthorised as i32,
        VendorTrustClass::BlacklistedDoNotAdmit => {
            VendorTrustClassProto::BlacklistedDoNotAdmit as i32
        }
    }
}
