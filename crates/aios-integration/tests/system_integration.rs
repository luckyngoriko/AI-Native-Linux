#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::doc_markdown,
    clippy::significant_drop_tightening,
    clippy::wildcard_imports,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    clippy::needless_collect,
    clippy::format_collect,
    clippy::missing_const_for_fn,
    clippy::too_many_arguments,
    clippy::float_cmp,
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{Duration, Utc};

use aios_integration::*;

// ---------------------------------------------------------------------------
// helpers
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

fn make_keypair() -> (ed25519_dalek::SigningKey, ed25519_dalek::VerifyingKey) {
    use rand_core::OsRng;
    let signing = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let verifying = signing.verifying_key();
    (signing, verifying)
}

#[allow(clippy::too_many_arguments)]
fn build_signed_contract(
    contract_id: &str,
    vendor_name: &str,
    vendor_kind: VendorKind,
    trust_class: VendorTrustClass,
    signing_key: &ed25519_dalek::SigningKey,
    fingerprint: &str,
) -> VendorIntegrationContract {
    use ed25519_dalek::Signer;
    let mut contract = VendorIntegrationContract {
        contract_id: VendorContractId(contract_id.into()),
        vendor_name: vendor_name.into(),
        vendor_kind,
        trust_class,
        contact_canonical_id: "test@example.com".into(),
        rotation_cadence_days: 90,
        breach_playbook_url: "https://example.com/breach".into(),
        signer_fingerprint: fingerprint.into(),
        signature: Vec::new(),
        admitted_at: Utc::now(),
    };
    let canonical = canonical_contract_bytes(&contract);
    let sig = signing_key.sign(&canonical);
    contract.signature = sig.to_bytes().to_vec();
    contract
}

fn make_invariant(id: &str, name: &str, layer: &str) -> AiosInvariant {
    AiosInvariant {
        invariant_id: id.into(),
        name: name.into(),
        layer: layer.into(),
    }
}

fn make_mapping(id: &str, inv: &AiosInvariant) -> ControlMapping {
    ControlMapping {
        mapping_id: id.into(),
        invariant: inv.clone(),
        control_refs: vec![ControlFrameworkRef {
            framework: StandardKind::Nist80053Rev5,
            control_family: "AC".into(),
            control_id: "AC-3".into(),
        }],
        mapping_rationale: "access enforcement".into(),
        mapped_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// test 1 — boot order sanity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn harness_boot_order_starts_with_aios_action_and_ends_with_aios_hardware() {
    let harness = SystemIntegrationHarness::new();
    let order = harness.boot_topological_order().await;
    assert_eq!(order.len(), 17);
    assert_eq!(order[0], "aios-action");
    assert_eq!(order[16], "aios-hardware");
}

// ---------------------------------------------------------------------------
// test 2 — vendor lifecycle to production emits chain
// ---------------------------------------------------------------------------

#[tokio::test]
async fn harness_admit_vendor_then_lifecycle_to_production_emits_chain() {
    let mut harness = SystemIntegrationHarness::new();
    let (sk, vk) = make_keypair();
    harness.register_vendor_authority("fp:lifecycle-test", vk);

    let vendor = harness.vendor().await;
    let contract = build_signed_contract(
        "VC-LIFECYCLE",
        "LifecycleVendor",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        &sk,
        "fp:lifecycle-test",
    );
    let cid = contract.contract_id.clone();

    // 1 — admit → INTEGRATION_PROPOSED (seq 0)
    vendor.admit_contract(contract).await.unwrap();
    assert_eq!(harness.evidence_chain_length().await, 1);

    // 2 — transition to Evaluated (seq 1)
    vendor
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Evaluated {
                evaluator: "auditor-1".into(),
                evaluated_at: Utc::now(),
                security_audit_passed: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(harness.evidence_chain_length().await, 2);

    // 3 — transition to Piloted (seq 2)
    vendor
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Piloted {
                since: Utc::now(),
                profile: "DEV_RELAXED".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(harness.evidence_chain_length().await, 3);

    // 4 — transition to Production (seq 3)
    vendor
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Production { since: Utc::now() },
        )
        .await
        .unwrap();
    assert_eq!(harness.evidence_chain_length().await, 4);

    // Verify final state and chain integrity
    let state = vendor.current_lifecycle(&cid).await.unwrap();
    assert_eq!(state.label(), IntegrationLifecycleLabel::Production);

    harness.validate_evidence_chain().await.unwrap();
}

// ---------------------------------------------------------------------------
// test 3 — standard subscribe → revise → STANDARD_UPDATE_AVAILABLE
// ---------------------------------------------------------------------------

#[tokio::test]
async fn harness_standard_subscribe_revise_emits_update_available() {
    let harness = SystemIntegrationHarness::new();
    let standards = harness.standards().await;
    let now = Utc::now();

    let sub = StandardSubscription {
        subscription_id: StandardSubscriptionId("SUB-NIST-001".into()),
        standard: StandardKind::Nist80053Rev5,
        catalog_url: "https://csrc.nist.gov/pubs/sp/800/53/r5/upd1/final".into(),
        current_revision: "Rev.5".into(),
        last_reviewed_at: now,
        next_review_due_at: now + Duration::days(90),
        responsible_canonical_id: "human:operator".into(),
    };

    standards.subscribe(sub).await.unwrap();
    assert_eq!(harness.evidence_chain_length().await, 0);

    standards
        .revise(
            &StandardSubscriptionId("SUB-NIST-001".into()),
            "Rev.5.1".into(),
            "human:auditor".into(),
            "mid-cycle update".into(),
        )
        .await
        .unwrap();

    assert_eq!(harness.evidence_chain_length().await, 1);

    // Check payload shape
    let payload = harness.evidence_emitter().get_payload(0).await.unwrap();
    assert_eq!(payload["subscription_id"], "SUB-NIST-001");
    assert_eq!(payload["new_revision"], "Rev.5.1");

    harness.validate_evidence_chain().await.unwrap();
}

// ---------------------------------------------------------------------------
// test 4 — CVE ingest → bind → PACKAGE_HAS_KNOWN_CVE with Critical severity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn harness_cve_ingest_bind_emits_package_has_known_cve_with_critical_severity() {
    let harness = SystemIntegrationHarness::new();
    let feed = harness.cve().await;

    // Ingest a CVE with CVSS 9.5 (Critical)
    let cve_record = CveRecord {
        cve_id: CveId("CVE-2024-99999".into()),
        published_at: Utc::now(),
        last_modified_at: Utc::now(),
        cvss_v3_score: 9.5,
        severity: CveSeverity::Critical,
        summary: "remote code execution in firefox".into(),
        affected_cpe_uris: vec!["cpe:2.3:a:mozilla:firefox:*".into()],
    };
    feed.ingest_record(cve_record).await.unwrap();
    assert_eq!(harness.evidence_chain_length().await, 0);

    // Enforcement level for CVSS 9.5 must be AutoQuarantine
    let enforcement = feed
        .enforcement_level_for(&CveId("CVE-2024-99999".into()))
        .await
        .unwrap();
    assert_eq!(enforcement, CveEnforcementLevel::AutoQuarantine);

    // Bind to package "firefox"
    let binding = PackageCveBinding {
        binding_id: "BIND-FIREFOX-001".into(),
        cve_id: CveId("CVE-2024-99999".into()),
        package_id: "firefox".into(),
        status: CveStatus::Open,
        bound_at: Utc::now(),
        matched_via_cpe: Some("cpe:2.3:a:mozilla:firefox:*".into()),
        mitigated_by: None,
    };
    feed.bind_to_package(binding).await.unwrap();

    assert_eq!(harness.evidence_chain_length().await, 1);

    // Verify payload carries severity Critical
    let payload = harness.evidence_emitter().get_payload(0).await.unwrap();
    assert_eq!(payload["package_id"], "firefox");
    assert_eq!(payload["cve_id"], "CVE-2024-99999");
    assert!(payload["severity"].as_str().unwrap().contains("Critical"));

    harness.validate_evidence_chain().await.unwrap();
}

// ---------------------------------------------------------------------------
// test 5 — bridge admit → BRIDGE_ADMITTED
// ---------------------------------------------------------------------------

#[tokio::test]
async fn harness_admit_flathub_bridge_then_emits_bridge_admitted() {
    let harness = SystemIntegrationHarness::new();
    let bridges = harness.bridge().await;

    let (sk, _vk) = make_keypair();
    let vendor_contract = build_signed_contract(
        "VC-FLATHUB",
        "Flathub",
        VendorKind::ApplicationStore,
        VendorTrustClass::CommunityVerified,
        &sk,
        "fp:flathub",
    );

    let bridge = BridgeContract {
        bridge_id: "BRIDGE-FLATHUB-001".into(),
        kind: BridgeKind::Flathub,
        vendor_contract,
        translation_rules: default_flathub_contract(),
        admitted_at: Utc::now(),
    };

    bridges.admit_bridge(bridge).await.unwrap();
    assert_eq!(harness.evidence_chain_length().await, 1);

    let payload = harness.evidence_emitter().get_payload(0).await.unwrap();
    assert_eq!(payload["bridge_id"], "BRIDGE-FLATHUB-001");
    assert_eq!(payload["kind"], "Flathub");

    harness.validate_evidence_chain().await.unwrap();
}

// ---------------------------------------------------------------------------
// test 6 — compliance baseline snapshot + drift detection emits two record types
// ---------------------------------------------------------------------------

#[tokio::test]
async fn harness_compliance_baseline_then_drift_detection_emits_two_records() {
    let harness = SystemIntegrationHarness::new();
    let cm = harness.control_map().await;

    // Add first mapping
    let inv1 = make_invariant("INV-001", "Action Lifecycle Integrity", "L0");
    cm.add_mapping(make_mapping("MAP-001", &inv1))
        .await
        .unwrap();
    assert_eq!(harness.evidence_chain_length().await, 0);

    // Snapshot baseline 1 → COMPLIANCE_BASELINE_SNAPSHOT (seq 0)
    let baseline1 = cm
        .snapshot_baseline("BL-1".into(), "0.0.1".into(), "v1".into())
        .await
        .unwrap();
    assert_eq!(harness.evidence_chain_length().await, 1);

    // Verify baseline snapshot payload
    let bl1_payload = harness.evidence_emitter().get_payload(0).await.unwrap();
    assert_eq!(bl1_payload["baseline_id"], "BL-1");
    assert_eq!(bl1_payload["mapping_count"], 1);

    // Add second mapping
    let inv2 = make_invariant("INV-002", "Evidence Append Only", "L0");
    cm.add_mapping(make_mapping("MAP-002", &inv2))
        .await
        .unwrap();

    // Snapshot baseline 2 → COMPLIANCE_BASELINE_SNAPSHOT (seq 1)
    let _baseline2 = cm
        .snapshot_baseline("BL-2".into(), "0.0.1".into(), "v1".into())
        .await
        .unwrap();
    assert_eq!(harness.evidence_chain_length().await, 2);

    // Detect drift against baseline1 → CONTROL_MAP_DRIFT_DETECTED (seq 2)
    let drift = cm.detect_drift(&baseline1).await;
    assert!(!drift.added.is_empty());

    let drift_payload = harness.evidence_emitter().get_payload(2).await.unwrap();
    assert!(drift_payload["prior_baseline_id"].as_str().is_some());

    // Two distinct record types present: COMPLIANCE_BASELINE_SNAPSHOT and
    // CONTROL_MAP_DRIFT_DETECTED
    assert_eq!(harness.evidence_chain_length().await, 3);
    harness.validate_evidence_chain().await.unwrap();
}

// ---------------------------------------------------------------------------
// test 7 — full lifecycle E2E covering all 8 evidence types
// ---------------------------------------------------------------------------

#[tokio::test]
async fn harness_full_lifecycle_e2e_covers_all_8_evidence_types() {
    let mut harness = SystemIntegrationHarness::new();
    let (sk, vk) = make_keypair();
    harness.register_vendor_authority("fp:e2e-full", vk);

    // ── 1. INTEGRATION_PROPOSED — admit vendor ────────────────────────
    let vendor = harness.vendor().await;
    let contract = build_signed_contract(
        "VC-E2E-ALL",
        "E2eVendor",
        VendorKind::OciRegistry,
        VendorTrustClass::OperatorAuthorised,
        &sk,
        "fp:e2e-full",
    );
    let cid = contract.contract_id.clone();
    vendor.admit_contract(contract).await.unwrap();

    // ── 2. INTEGRATION_LIFECYCLE_TRANSITIONED — Proposed → Evaluated ─
    vendor
        .transition_lifecycle(
            &cid,
            IntegrationLifecycleState::Evaluated {
                evaluator: "aud".into(),
                evaluated_at: Utc::now(),
                security_audit_passed: true,
            },
        )
        .await
        .unwrap();

    // ── 3. STANDARD_UPDATE_AVAILABLE — subscribe + revise standard ──
    let standards = harness.standards().await;
    let sub = StandardSubscription {
        subscription_id: StandardSubscriptionId("SUB-E2E".into()),
        standard: StandardKind::Nist80053Rev5,
        catalog_url: "https://example.com/nist".into(),
        current_revision: "Rev.5".into(),
        last_reviewed_at: Utc::now(),
        next_review_due_at: Utc::now() + Duration::days(90),
        responsible_canonical_id: "human:op".into(),
    };
    standards.subscribe(sub).await.unwrap();
    standards
        .revise(
            &StandardSubscriptionId("SUB-E2E".into()),
            "Rev.5.1".into(),
            "human:rev".into(),
            "rev note".into(),
        )
        .await
        .unwrap();

    // ── 4. PACKAGE_HAS_KNOWN_CVE — ingest CVE + bind to package ──────
    let feed = harness.cve().await;
    feed.ingest_record(CveRecord {
        cve_id: CveId("CVE-2024-99998".into()),
        published_at: Utc::now(),
        last_modified_at: Utc::now(),
        cvss_v3_score: 8.2,
        severity: CveSeverity::High,
        summary: "E2E CVE test".into(),
        affected_cpe_uris: vec![],
    })
    .await
    .unwrap();
    feed.bind_to_package(PackageCveBinding {
        binding_id: "BIND-E2E".into(),
        cve_id: CveId("CVE-2024-99998".into()),
        package_id: "e2e-pkg".into(),
        status: CveStatus::Open,
        bound_at: Utc::now(),
        matched_via_cpe: None,
        mitigated_by: None,
    })
    .await
    .unwrap();

    // ── 5. BRIDGE_ADMITTED — admit Flathub bridge ────────────────────
    let bridges = harness.bridge().await;
    let bridge_vc = build_signed_contract(
        "VC-BRIDGE-E2E",
        "FlathubE2E",
        VendorKind::ApplicationStore,
        VendorTrustClass::CommunityVerified,
        &sk,
        "fp:e2e-full",
    );
    bridges
        .admit_bridge(BridgeContract {
            bridge_id: "BRIDGE-E2E".into(),
            kind: BridgeKind::Flathub,
            vendor_contract: bridge_vc,
            translation_rules: default_flathub_contract(),
            admitted_at: Utc::now(),
        })
        .await
        .unwrap();

    // ── 6. COMPLIANCE_BASELINE_SNAPSHOT — snapshot baseline ──────────
    let cm = harness.control_map().await;
    let inv = make_invariant("INV-E2E", "E2E Invariant", "L5");
    cm.add_mapping(make_mapping("MAP-E2E", &inv)).await.unwrap();
    let baseline = cm
        .snapshot_baseline("BL-E2E".into(), "0.0.1".into(), "v1".into())
        .await
        .unwrap();

    // ── 7. CONTROL_MAP_DRIFT_DETECTED ────────────────────────────────
    cm.add_mapping(make_mapping("MAP-E2E-2", &inv))
        .await
        .unwrap();
    let _drift = cm.detect_drift(&baseline).await;

    // ── 8. VENDOR_CONTRACT_REVOKED — revoke contract ─────────────────
    vendor.revoke_contract(&cid, "end-of-life").await.unwrap();

    // ── assertions ───────────────────────────────────────────────────
    let chain_len = harness.evidence_chain_length().await;
    assert!(chain_len >= 8, "expected >= 8 records, got {chain_len}");

    // Verify every payload field signature corresponds to a unique record type.
    let mut types_seen: HashSet<String> = HashSet::new();
    for i in 0..chain_len as usize {
        let payload = harness.evidence_emitter().get_payload(i).await.unwrap();
        if payload.get("contract_id").is_some() && payload.get("vendor_name").is_some() {
            types_seen.insert("INTEGRATION_PROPOSED".into());
        } else if payload.get("subscription_id").is_some() && payload.get("new_revision").is_some()
        {
            types_seen.insert("STANDARD_UPDATE_AVAILABLE".into());
        } else if payload.get("cve_id").is_some() && payload.get("package_id").is_some() {
            types_seen.insert("PACKAGE_HAS_KNOWN_CVE".into());
        } else if payload.get("from").is_some() && payload.get("to").is_some() {
            types_seen.insert("INTEGRATION_LIFECYCLE_TRANSITIONED".into());
        } else if payload.get("contract_id").is_some()
            && payload.get("reason").is_some()
            && payload.get("vendor_name").is_none()
        {
            types_seen.insert("VENDOR_CONTRACT_REVOKED".into());
        } else if payload.get("bridge_id").is_some() && payload.get("kind").is_some() {
            types_seen.insert("BRIDGE_ADMITTED".into());
        } else if payload.get("baseline_id").is_some() && payload.get("mapping_count").is_some() {
            types_seen.insert("COMPLIANCE_BASELINE_SNAPSHOT".into());
        } else if payload.get("prior_baseline_id").is_some() && payload.get("added_count").is_some()
        {
            types_seen.insert("CONTROL_MAP_DRIFT_DETECTED".into());
        }
    }
    assert_eq!(
        types_seen.len(),
        8,
        "expected all 8 record type names, saw {types_seen:?}"
    );

    harness.validate_evidence_chain().await.unwrap();
}

// ---------------------------------------------------------------------------
// test 8 — concurrent emits produce a deterministic chain-validated stream
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn harness_evidence_chain_validate_after_concurrent_emits_is_consistent() {
    let mut harness = SystemIntegrationHarness::new();
    let (sk, vk) = make_keypair();
    harness.register_vendor_authority("fp:concurrent", vk);

    let harness = Arc::new(harness);

    let mut handles = Vec::new();

    // Task A: admit vendor contract
    {
        let h = Arc::clone(&harness);
        let sk = ed25519_dalek::SigningKey::from_bytes(&sk.to_bytes());
        handles.push(tokio::spawn(async move {
            let vendor = h.vendor().await;
            let c = build_signed_contract(
                "VC-CONC-A",
                "VendorA",
                VendorKind::OciRegistry,
                VendorTrustClass::OperatorAuthorised,
                &sk,
                "fp:concurrent",
            );
            vendor.admit_contract(c).await.unwrap();
        }));
    }

    // Task B: revise standard
    {
        let h = Arc::clone(&harness);
        handles.push(tokio::spawn(async move {
            let standards = h.standards().await;
            let sub = StandardSubscription {
                subscription_id: StandardSubscriptionId("SUB-CONC-B".into()),
                standard: StandardKind::CisControlsV8,
                catalog_url: "https://example.com/cis".into(),
                current_revision: "v8".into(),
                last_reviewed_at: Utc::now(),
                next_review_due_at: Utc::now() + Duration::days(90),
                responsible_canonical_id: "human:test".into(),
            };
            standards.subscribe(sub).await.unwrap();
            standards
                .revise(
                    &StandardSubscriptionId("SUB-CONC-B".into()),
                    "v8.1".into(),
                    "human:rev".into(),
                    "note".into(),
                )
                .await
                .unwrap();
        }));
    }

    // Task C: CVE bind
    {
        let h = Arc::clone(&harness);
        handles.push(tokio::spawn(async move {
            let feed = h.cve().await;
            feed.ingest_record(CveRecord {
                cve_id: CveId("CVE-2024-77777".into()),
                published_at: Utc::now(),
                last_modified_at: Utc::now(),
                cvss_v3_score: 6.5,
                severity: CveSeverity::Medium,
                summary: "concurrent CVE".into(),
                affected_cpe_uris: vec![],
            })
            .await
            .unwrap();
            feed.bind_to_package(PackageCveBinding {
                binding_id: "BIND-CONC-C".into(),
                cve_id: CveId("CVE-2024-77777".into()),
                package_id: "pkg-c".into(),
                status: CveStatus::Open,
                bound_at: Utc::now(),
                matched_via_cpe: None,
                mitigated_by: None,
            })
            .await
            .unwrap();
        }));
    }

    // Task D: bridge admit
    {
        let h = Arc::clone(&harness);
        let sk = ed25519_dalek::SigningKey::from_bytes(&sk.to_bytes());
        handles.push(tokio::spawn(async move {
            let bridges = h.bridge().await;
            let vc = build_signed_contract(
                "VC-CONC-D",
                "BridgeVendor",
                VendorKind::ApplicationStore,
                VendorTrustClass::CommunityVerified,
                &sk,
                "fp:concurrent",
            );
            bridges
                .admit_bridge(BridgeContract {
                    bridge_id: "BRIDGE-CONC-D".into(),
                    kind: BridgeKind::Flathub,
                    vendor_contract: vc,
                    translation_rules: default_flathub_contract(),
                    admitted_at: Utc::now(),
                })
                .await
                .unwrap();
        }));
    }

    // Task E: compliance baseline snapshot
    {
        let h = Arc::clone(&harness);
        handles.push(tokio::spawn(async move {
            let cm = h.control_map().await;
            let inv = make_invariant("INV-CONC-E", "Concurrent invariant", "L6");
            cm.add_mapping(make_mapping("MAP-CONC-E", &inv))
                .await
                .unwrap();
            cm.snapshot_baseline("BL-CONC-E".into(), "0.0.1".into(), "v1".into())
                .await
                .unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // 5 tasks, each emitting exactly 1 evidence record.
    assert_eq!(harness.evidence_chain_length().await, 5);

    // Chain integrity must hold under concurrent writes.
    harness.validate_evidence_chain().await.unwrap();
}
