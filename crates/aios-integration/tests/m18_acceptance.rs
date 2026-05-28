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
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;

use aios_integration::control_map::{AiosInvariant, ControlFrameworkRef, ControlMapping};
use aios_integration::harness::SystemIntegrationHarness;
use aios_integration::ids::VendorContractId;
use aios_integration::lifecycle::{IntegrationLifecycleLabel, IntegrationLifecycleState};
use aios_integration::standard::StandardKind;
use aios_integration::vendor::{VendorIntegrationContract, VendorKind, VendorTrustClass};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn make_keypair() -> (SigningKey, VerifyingKey) {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
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

fn sign_contract(
    contract: &VendorIntegrationContract,
    signing_key: &SigningKey,
    fingerprint: &str,
) -> VendorIntegrationContract {
    let bytes = canonical_contract_bytes(contract);
    let sig = signing_key.sign(&bytes);
    let mut signed = contract.clone();
    signed.signer_fingerprint = fingerprint.to_string();
    signed.signature = sig.to_vec();
    signed
}

fn unsigned_contract(id: &str) -> VendorIntegrationContract {
    VendorIntegrationContract {
        contract_id: VendorContractId(id.into()),
        vendor_name: "AcceptanceVendor".into(),
        vendor_kind: VendorKind::PackageRepository,
        trust_class: VendorTrustClass::CommunityVerified,
        contact_canonical_id: "acceptance@vendor.example".into(),
        rotation_cadence_days: 90,
        breach_playbook_url: "https://vendor.example/breach-playbook".into(),
        signer_fingerprint: String::new(),
        signature: vec![],
        admitted_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// 1. Phase 1-5 E2E through SystemIntegrationHarness
//    admit contract → transition lifecycle → verify evidence chain
// ---------------------------------------------------------------------------

#[tokio::test]
async fn phase_1_5_e2e_admit_transition_verify_evidence_chain() {
    // ── bootstrap harness ─────────────────────────────────────────────────
    let (signing_key, verifying_key) = make_keypair();
    let fingerprint = "fp-acceptance-e2e";

    let mut harness = SystemIntegrationHarness::new();
    harness.register_vendor_authority(fingerprint, verifying_key);

    let vendor = harness.vendor().await;

    // ── Phase 1: Admit contract (enters Proposed) ──────────────────────────
    let contract = unsigned_contract("VC-ACCEPT-E2E");
    let signed = sign_contract(&contract, &signing_key, fingerprint);
    vendor.admit_contract(signed.clone()).await.unwrap();

    let state = vendor
        .current_lifecycle(&contract.contract_id)
        .await
        .unwrap();
    assert_eq!(state.label(), IntegrationLifecycleLabel::Proposed);

    // ── Phase 2: Transition Proposed → Evaluated ──────────────────────────
    let evaluated = IntegrationLifecycleState::Evaluated {
        evaluator: "human:auditor".into(),
        evaluated_at: Utc::now(),
        security_audit_passed: true,
    };
    vendor
        .transition_lifecycle(&contract.contract_id, evaluated)
        .await
        .unwrap();
    let state = vendor
        .current_lifecycle(&contract.contract_id)
        .await
        .unwrap();
    assert_eq!(state.label(), IntegrationLifecycleLabel::Evaluated);

    // ── Phase 3: Transition Evaluated → Piloted ───────────────────────────
    let piloted = IntegrationLifecycleState::Piloted {
        since: Utc::now(),
        profile: "DEV_RELAXED".into(),
    };
    vendor
        .transition_lifecycle(&contract.contract_id, piloted)
        .await
        .unwrap();
    let state = vendor
        .current_lifecycle(&contract.contract_id)
        .await
        .unwrap();
    assert_eq!(state.label(), IntegrationLifecycleLabel::Piloted);

    // ── Phase 4: Transition Piloted → Production ──────────────────────────
    let production = IntegrationLifecycleState::Production { since: Utc::now() };
    vendor
        .transition_lifecycle(&contract.contract_id, production)
        .await
        .unwrap();
    let state = vendor
        .current_lifecycle(&contract.contract_id)
        .await
        .unwrap();
    assert_eq!(state.label(), IntegrationLifecycleLabel::Production);

    // ── Phase 5: Revoke contract ──────────────────────────────────────────
    vendor
        .revoke_contract(&contract.contract_id, "end-of-life policy")
        .await
        .unwrap();
    let state = vendor
        .current_lifecycle(&contract.contract_id)
        .await
        .unwrap();
    assert_eq!(state.label(), IntegrationLifecycleLabel::Retired);

    // ── Verify evidence chain integrity ───────────────────────────────────
    // Should have at least 5 evidence records:
    //   admission → 3 lifecycle transitions → revocation
    let chain_len = harness.evidence_chain_length().await;
    assert!(
        chain_len >= 5,
        "expected >=5 evidence records, got {chain_len}"
    );

    harness.validate_evidence_chain().await.unwrap();
}

// ---------------------------------------------------------------------------
// 1b. Phase 1-5 E2E: Evaluated→Piloted should FAIL when
//     security_audit_passed=false (guard condition reachability)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn phase_2_to_3_rejected_without_security_audit() {
    let (signing_key, verifying_key) = make_keypair();
    let fingerprint = "fp-audit-fail";

    let mut harness = SystemIntegrationHarness::new();
    harness.register_vendor_authority(fingerprint, verifying_key);

    let vendor = harness.vendor().await;
    let contract = unsigned_contract("VC-AUDIT-FAIL");
    let signed = sign_contract(&contract, &signing_key, fingerprint);
    vendor.admit_contract(signed).await.unwrap();

    // Transition to Evaluated WITH security_audit_passed=false.
    let evaluated_no_audit = IntegrationLifecycleState::Evaluated {
        evaluator: "human:auditor".into(),
        evaluated_at: Utc::now(),
        security_audit_passed: false,
    };
    vendor
        .transition_lifecycle(&contract.contract_id, evaluated_no_audit)
        .await
        .unwrap();

    // Attempt Evaluated→Piloted — MUST FAIL (guard requires security_audit_passed=true).
    let piloted = IntegrationLifecycleState::Piloted {
        since: Utc::now(),
        profile: "DEV_RELAXED".into(),
    };
    let err = vendor
        .transition_lifecycle(&contract.contract_id, piloted)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        aios_integration::error::IntegrationError::LifecycleInvalidTransition { .. }
    ));
}

// ---------------------------------------------------------------------------
// 2. Compliance baseline drift detection
//    add mapping → snapshot → modify → detect drift
// ---------------------------------------------------------------------------

#[tokio::test]
async fn compliance_baseline_drift_detection_through_harness() {
    let harness = SystemIntegrationHarness::new();
    let control_map = harness.control_map().await;

    // ── Add two mappings ──────────────────────────────────────────────────
    let map_a = ControlMapping {
        mapping_id: "MAP-ACCEPT-A".into(),
        invariant: AiosInvariant {
            invariant_id: "INV-001".into(),
            name: "Non-Repudiation Of Action".into(),
            layer: "L0".into(),
        },
        control_refs: vec![ControlFrameworkRef {
            framework: StandardKind::Nist80053Rev5,
            control_family: "AU".into(),
            control_id: "AU-10".into(),
        }],
        mapping_rationale: "original rationale A".into(),
        mapped_at: Utc::now(),
    };
    let map_b = ControlMapping {
        mapping_id: "MAP-ACCEPT-B".into(),
        invariant: AiosInvariant {
            invariant_id: "INV-002".into(),
            name: "Secrets Are Capabilities".into(),
            layer: "L4".into(),
        },
        control_refs: vec![ControlFrameworkRef {
            framework: StandardKind::Nist80053Rev5,
            control_family: "AC".into(),
            control_id: "AC-6".into(),
        }],
        mapping_rationale: "original rationale B".into(),
        mapped_at: Utc::now(),
    };
    control_map.add_mapping(map_a).await.unwrap();
    control_map.add_mapping(map_b).await.unwrap();

    // ── Snapshot baseline ─────────────────────────────────────────────────
    let prior = control_map
        .snapshot_baseline(
            "BL-ACCEPT-001".into(),
            "0.1.0-T186".into(),
            "human:auditor".into(),
        )
        .await
        .unwrap();
    assert_eq!(prior.mappings.len(), 2);

    // ── Modify MAP-A rationale (current deviates from baseline) ───────────
    {
        let mut guard = control_map.mappings.write().await;
        if let Some(m) = guard.get_mut("MAP-ACCEPT-A") {
            m.mapping_rationale = "updated rationale after security review".into();
        }
    }

    // ── Detect drift ──────────────────────────────────────────────────────
    let drift = control_map.detect_drift(&prior).await;
    assert_eq!(drift.modified, vec!["MAP-ACCEPT-A"]);
    assert!(drift.added.is_empty());
    assert!(drift.removed.is_empty());
    assert_eq!(drift.unchanged_count, 1); // MAP-B unchanged

    // ── Evidence should include drift event ───────────────────────────────
    let chain_len = harness.evidence_chain_length().await;
    assert!(
        chain_len >= 1,
        "expected >=1 evidence record, got {chain_len}"
    );
}

// ---------------------------------------------------------------------------
// 2b. Full drift spectrum: added + removed + modified + unchanged
// ---------------------------------------------------------------------------

#[tokio::test]
async fn compliance_drift_full_spectrum_added_removed_modified_unchanged() {
    let harness = SystemIntegrationHarness::new();
    let control_map = harness.control_map().await;

    // Pre-load 3 mappings for the baseline.
    let mk = |id: &str, inv_id: &str, rationale: &str| ControlMapping {
        mapping_id: id.into(),
        invariant: AiosInvariant {
            invariant_id: inv_id.into(),
            name: format!("Invariant {inv_id}"),
            layer: "L4".into(),
        },
        control_refs: vec![ControlFrameworkRef {
            framework: StandardKind::Nist80053Rev5,
            control_family: "AC".into(),
            control_id: "AC-3".into(),
        }],
        mapping_rationale: rationale.into(),
        mapped_at: Utc::now(),
    };
    control_map
        .add_mapping(mk("DRIFT-KEEP", "INV-001", "unchanged"))
        .await
        .unwrap();
    control_map
        .add_mapping(mk("DRIFT-MOD", "INV-002", "before modification"))
        .await
        .unwrap();
    control_map
        .add_mapping(mk("DRIFT-DEL", "INV-003", "will be removed"))
        .await
        .unwrap();

    let prior = control_map
        .snapshot_baseline(
            "BL-FULL-SPECTRUM".into(),
            "0.1.0-T186".into(),
            "human:auditor".into(),
        )
        .await
        .unwrap();

    // After snapshot: modify DRIFT-MOD, delete DRIFT-DEL, add DRIFT-NEW.
    {
        let mut guard = control_map.mappings.write().await;
        if let Some(m) = guard.get_mut("DRIFT-MOD") {
            m.mapping_rationale = "after modification".into();
        }
        guard.remove("DRIFT-DEL");
    }
    control_map
        .add_mapping(mk("DRIFT-NEW", "INV-004", "newly added"))
        .await
        .unwrap();

    let drift = control_map.detect_drift(&prior).await;
    assert_eq!(drift.added, vec!["DRIFT-NEW"]);
    assert_eq!(drift.removed, vec!["DRIFT-DEL"]);
    assert_eq!(drift.modified, vec!["DRIFT-MOD"]);
    assert_eq!(drift.unchanged_count, 1);
}

// ---------------------------------------------------------------------------
// 3. 17-crate orchestrator boot verification
//    verify boot_order has 17 entries in valid topological order
// ---------------------------------------------------------------------------

#[tokio::test]
async fn orchestrator_boot_17_crate_topological_order_valid() {
    let harness = SystemIntegrationHarness::new();
    let boot_order = harness.boot_topological_order().await;

    assert_eq!(boot_order.len(), 17, "expected 17 crates in boot order");

    // Verify every dependency appears before its dependent (topological order).
    // Dependencies from the spec:
    // aios-action is first (no deps)
    // aios-evidence depends on aios-action
    // aios-policy depends on aios-action + aios-evidence
    // aios-sgr depends on aios-action
    // aios-capability-runtime depends on aios-action + aios-policy
    // etc.
    let pos = |name: &str| -> usize {
        boot_order
            .iter()
            .position(|s| s == name)
            .unwrap_or_else(|| panic!("{name} not in boot order"))
    };

    // Core dependency invariants.
    assert!(pos("aios-action") < pos("aios-evidence"));
    assert!(pos("aios-action") < pos("aios-policy"));
    assert!(pos("aios-evidence") < pos("aios-policy"));
    assert!(pos("aios-action") < pos("aios-capability-runtime"));
    assert!(pos("aios-policy") < pos("aios-capability-runtime"));
    assert!(pos("aios-action") < pos("aios-sgr"));
    assert!(pos("aios-action") < pos("aios-fs"));
    assert!(pos("aios-policy") < pos("aios-vault"));
    assert!(pos("aios-capability-runtime") < pos("aios-renderer-cli"));
    assert!(pos("aios-renderer-cli") < pos("aios-renderer-kde"));

    // First and last are fixed.
    assert_eq!(boot_order[0], "aios-action");
    assert_eq!(boot_order[16], "aios-hardware");

    // All 17 must be unique.
    let mut sorted = boot_order.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 17, "duplicate crate names in boot order");
}

// ---------------------------------------------------------------------------
// 3b. Orchestrator health summary: all 17 crates ScaffoldReady
// ---------------------------------------------------------------------------

#[tokio::test]
async fn orchestrator_health_summary_all_17_scaffold_ready() {
    use aios_integration::orchestrator::Orchestrator;
    use aios_integration::orchestrator::ServiceScaffoldStatus;

    let orch = Orchestrator::from_default_composition().expect("default composition must be valid");
    let health = orch.health_summary().await;

    assert_eq!(health.len(), 17);
    for s in &health {
        assert_eq!(
            s.status,
            ServiceScaffoldStatus::ScaffoldReady,
            "{} status is not ScaffoldReady",
            s.service_id
        );
        assert!(s.topological_index < 17);
    }
}
