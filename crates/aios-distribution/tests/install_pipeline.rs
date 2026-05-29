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
    clippy::redundant_clone,
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::collections::HashMap;

use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;

use aios_distribution::*;

// ============================================================================
// Shared test helpers
// ============================================================================

/// Generates a fresh Ed25519 keypair.
fn make_keypair() -> (SigningKey, ed25519_dalek::VerifyingKey) {
    let mut csprng = OsRng;
    let signing = SigningKey::generate(&mut csprng);
    let verifying = signing.verifying_key();
    (signing, verifying)
}

/// Builds a fully signed manifest with a 3-hop trust chain and corresponding
/// verifier.
///
/// Returns the manifest, verifier, link signatures, all owned values needed to
/// keep references alive, and the fetched content hash.
#[allow(clippy::type_complexity)]
fn build_setup(
    trust_level: PublisherTrustLevel,
    kind: PackageKind,
    scope: InstallScope,
) -> (
    PackageManifest,
    TrustChainVerifier<'static>,
    LinkSignature,
    LinkSignature,
    AiosRootKey,
    PublisherCatalog,
    HashMap<String, SigningKeyCatalog>,
    PackageSigningKey,
    PublisherRoot,
    String,
) {
    let now = Utc::now();

    // Tier 1 — AIOS root
    let (aios_sk, aios_vk) = make_keypair();
    let aios_root = AiosRootKey::new(aios_vk);

    // Tier 2 — publisher root
    let (pub_sk, pub_vk) = make_keypair();
    let publisher_root_id = PublisherRootId("pub:testsuite".into());
    let publisher_root = PublisherRoot {
        publisher_root_id: publisher_root_id.clone(),
        public_key: pub_vk,
        trust_level,
        onboarding_evidence_pointer: Some("evid://test/onboard-mf-001".into()),
        activated_at: now - Duration::days(30),
        retired_at: None,
    };
    let pub_root_sig = LinkSignature(
        aios_sk
            .sign(&publisher_root.canonical_entry_bytes())
            .to_bytes()
            .to_vec(),
    );
    let publisher_catalog = PublisherCatalog::new(vec![publisher_root.clone()]);

    // Tier 3 — package signing key
    let (sign_sk, sign_vk) = make_keypair();
    let signing_key_id = PackageSigningKeyId("pks:testsuite:release".into());
    let signing_key = PackageSigningKey {
        package_signing_key_id: signing_key_id.clone(),
        public_key: sign_vk,
        valid_from: now - Duration::days(7),
        valid_until: Some(now + Duration::days(365)),
        revoked_at: None,
    };
    let sign_key_sig = LinkSignature(
        pub_sk
            .sign(&signing_key.canonical_entry_bytes())
            .to_bytes()
            .to_vec(),
    );

    let mut signing_catalogs = HashMap::new();
    signing_catalogs.insert(
        "testsuite".to_string(),
        SigningKeyCatalog::new("testsuite".into(), vec![signing_key.clone()]),
    );

    // Build manifest
    let content_bytes = b"test-package-content-v1.0.0";
    let content_hash_val = canonical::content_hash(content_bytes);
    let fetched_content_hash = content_hash_val.clone();

    let caps = if kind == PackageKind::Theme {
        vec![]
    } else {
        vec!["filesystem.read".into(), "network.outbound".into()]
    };

    let repo = if trust_level == PublisherTrustLevel::AiosRoot {
        RepositoryKind::AiosRootRepo
    } else {
        RepositoryKind::AiosVerifiedRepo
    };

    let mut manifest = PackageManifest {
        package_id: "pkg:testsuite:testapp".into(),
        version: "1.0.0".into(),
        kind,
        publisher_trust: trust_level,
        publisher_root_id: publisher_root_id.clone(),
        package_signing_key_id: signing_key_id.clone(),
        content_hash: content_hash_val,
        manifest_canonical_hash: String::new(),
        ed25519_signature: Vec::new(),
        installable_scope: scope,
        required_sandbox: SandboxProfileRef("default-profile".into()),
        declared_capabilities: caps,
        network_manifest: NetworkManifestRef("default-net".into()),
        issued_at: now - Duration::hours(1),
        eol_at: None,
        channel: UpdateChannel::Stable,
        originating_repository: repo,
        mirror_url: "https://mirror.example.com".into(),
        mirror_semantic: MirrorSemantic::Cached,
    };

    // Compute canonical hash and sign
    let ch = canonical::manifest_canonical_hash(&manifest);
    manifest.manifest_canonical_hash.clone_from(&ch);
    let payload = canonical::signing_payload(&ch);
    manifest.ed25519_signature = sign_sk.sign(payload).to_bytes().to_vec();

    // Build verifier with leaked refs
    let aios_root_leaked: &'static AiosRootKey = Box::leak(Box::new(aios_root.clone()));
    let pubcat_leaked: &'static PublisherCatalog = Box::leak(Box::new(publisher_catalog.clone()));
    let sigcats_leaked: &'static HashMap<String, SigningKeyCatalog> =
        Box::leak(Box::new(signing_catalogs.clone()));
    let verifier = TrustChainVerifier::new(aios_root_leaked, pubcat_leaked, sigcats_leaked);

    (
        manifest,
        verifier,
        pub_root_sig,
        sign_key_sig,
        aios_root,
        publisher_catalog,
        signing_catalogs,
        signing_key,
        publisher_root,
        fetched_content_hash,
    )
}

/// Calls `run_install` with all-success deps and the pre-built chain.
fn run_happy(
    manifest: &PackageManifest,
    verifier: &TrustChainVerifier<'_>,
    fetched_ch: &str,
    pub_root_sig: &LinkSignature,
    sign_key_sig: &LinkSignature,
) -> InstallOutcome {
    let deps = InMemoryPipelineDeps {
        fetched_content_hash: fetched_ch.to_string(),
        ..Default::default()
    };
    run_install(
        manifest,
        verifier,
        &deps,
        pub_root_sig,
        sign_key_sig,
        Utc::now(),
    )
}

// ============================================================================
// 01 — Happy path: VERIFIED publisher → Active
// ============================================================================

#[test]
fn happy_path_verified_publisher_returns_active() {
    let (m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::Either,
    );

    let outcome = run_happy(&m, &verifier, &fch, &pub_sig, &key_sig);

    assert_eq!(outcome.final_state, PackageInstallState::Active);
    assert_eq!(outcome.result, PackageVerificationResult::VerifiedPublisher);
    assert!(outcome.failed_step.is_none());
}

// ============================================================================
// 02 — Happy path: AIOS_ROOT publisher → Active with VerifiedAiosRoot
// ============================================================================

#[test]
fn happy_path_aios_root_returns_verified_aios_root() {
    let (m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::AiosRoot,
        PackageKind::InvariantBundle,
        InstallScope::SystemOnly,
    );

    let deps = InMemoryPipelineDeps {
        fetched_content_hash: fch.clone(),
        recovery_active: true, // required for InvariantBundle
        ..Default::default()
    };

    let outcome = run_install(&m, &verifier, &deps, &pub_sig, &key_sig, Utc::now());

    assert_eq!(outcome.final_state, PackageInstallState::Active);
    assert_eq!(outcome.result, PackageVerificationResult::VerifiedAiosRoot);
    assert!(outcome.failed_step.is_none());
}

// ============================================================================
// 03 — Step 2: Signature failure → InstallFailed
// ============================================================================

#[test]
fn step2_signature_failure_returns_signature_failed() {
    let (mut m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::Either,
    );

    // Tamper the signature
    if !m.ed25519_signature.is_empty() {
        m.ed25519_signature[0] ^= 0xFF;
    }

    let outcome = run_happy(&m, &verifier, &fch, &pub_sig, &key_sig);

    assert_eq!(outcome.final_state, PackageInstallState::InstallFailed);
    assert_eq!(outcome.result, PackageVerificationResult::SignatureFailed);
    assert_eq!(outcome.failed_step, Some(PipelineStep::SignatureVerify));
}

// ============================================================================
// 04 — Step 3: Trust chain too deep → InstallFailed
// ============================================================================

#[test]
fn step3_trust_chain_too_deep() {
    // Build ONE setup — all keys and catalogs must match.
    let (
        m,
        _verifier,
        pub_sig,
        key_sig,
        aios_root,
        publisher_catalog,
        signing_catalogs,
        _sk,
        _pr,
        fch,
    ) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::Either,
    );

    // Construct a verifier with max_depth=2 using the SAME catalogs.
    let aios_root_leaked: &'static AiosRootKey = Box::leak(Box::new(aios_root));
    let pubcat_leaked: &'static PublisherCatalog = Box::leak(Box::new(publisher_catalog));
    let sigcats_leaked: &'static HashMap<String, SigningKeyCatalog> =
        Box::leak(Box::new(signing_catalogs));
    let deep_verifier =
        TrustChainVerifier::with_max_depth(aios_root_leaked, pubcat_leaked, sigcats_leaked, 2);

    let deps = InMemoryPipelineDeps {
        fetched_content_hash: fch.clone(),
        ..Default::default()
    };

    let outcome = run_install(&m, &deep_verifier, &deps, &pub_sig, &key_sig, Utc::now());

    assert_eq!(outcome.final_state, PackageInstallState::InstallFailed);
    assert_eq!(outcome.result, PackageVerificationResult::TrustChainTooDeep);
    assert_eq!(outcome.failed_step, Some(PipelineStep::TrustChainVerify));
}

// ============================================================================
// 05 — Step 4: Deplatformed publisher → InstallFailed
// ============================================================================

#[test]
fn step4_deplatformed_publisher() {
    let (m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::Deplatformed,
        PackageKind::App,
        InstallScope::Either,
    );

    let outcome = run_happy(&m, &verifier, &fch, &pub_sig, &key_sig);

    assert_eq!(outcome.final_state, PackageInstallState::InstallFailed);
    assert_eq!(
        outcome.result,
        PackageVerificationResult::PublisherDeplatformed
    );
    assert_eq!(outcome.failed_step, Some(PipelineStep::PublisherStateCheck));
}

// ============================================================================
// 06 — Step 4: Deprecated publisher → InstallFailed
// ============================================================================

#[test]
fn step4_deprecated_publisher() {
    // Build with Deprecated from the start. The verifier only checks for
    // Deplatformed; Deprecated publishers pass verification. The pipeline's
    // step 4 catches Deprecated from the manifest's publisher_trust field.
    let (m2, verifier2, pub_sig2, key_sig2, _r2, _pc2, _sc2, _sk2, _pr2, fch2) = build_setup(
        PublisherTrustLevel::Deprecated,
        PackageKind::App,
        InstallScope::Either,
    );

    let outcome = run_happy(&m2, &verifier2, &fch2, &pub_sig2, &key_sig2);

    assert_eq!(outcome.final_state, PackageInstallState::InstallFailed);
    assert_eq!(outcome.failed_step, Some(PipelineStep::PublisherStateCheck));
}

// ============================================================================
// 07 — Step 5: Content hash mismatch → InstallFailed
// ============================================================================

#[test]
fn step5_content_hash_mismatch() {
    let (m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, _fch) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::Either,
    );

    // Pass a content hash that doesn't match the manifest
    let wrong_fch = "f".repeat(32);

    let outcome = run_happy(&m, &verifier, &wrong_fch, &pub_sig, &key_sig);

    assert_eq!(outcome.final_state, PackageInstallState::InstallFailed);
    assert_eq!(outcome.result, PackageVerificationResult::HashMismatch);
    assert_eq!(outcome.failed_step, Some(PipelineStep::ContentHashVerify));
}

// ============================================================================
// 08 — Step 6: Manifest field forged → InstallFailed
// ============================================================================

#[test]
fn step6_manifest_field_forged() {
    let (mut m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::Either,
    );

    // Tamper a field that validate_fields catches — bad package_id format
    m.package_id = "not-a-valid-package-id".into();

    let outcome = run_happy(&m, &verifier, &fch, &pub_sig, &key_sig);

    assert_eq!(outcome.final_state, PackageInstallState::InstallFailed);
    assert_eq!(outcome.result, PackageVerificationResult::ManifestForged);
    assert_eq!(
        outcome.failed_step,
        Some(PipelineStep::ManifestFieldValidation)
    );
}

// ============================================================================
// 09 — Step 7: Sandbox infeasible → InstallFailed
// ============================================================================

#[test]
fn step7_sandbox_infeasible() {
    let (m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::Either,
    );

    let deps = InMemoryPipelineDeps {
        fetched_content_hash: fch.clone(),
        sandbox_valid: false,
        sandbox_failure_reason: "SANDBOX_INFEASIBLE".into(),
        ..Default::default()
    };

    let outcome = run_install(&m, &verifier, &deps, &pub_sig, &key_sig, Utc::now());

    assert_eq!(outcome.final_state, PackageInstallState::InstallFailed);
    assert_eq!(outcome.result, PackageVerificationResult::BundleTampered);
    assert_eq!(
        outcome.failed_step,
        Some(PipelineStep::SandboxProfileValidation)
    );
}

// ============================================================================
// 10 — Step 8: Unknown capability → InstallFailed
// ============================================================================

#[test]
fn step8_unknown_capability() {
    let (m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::Either,
    );

    let deps = InMemoryPipelineDeps {
        fetched_content_hash: fch.clone(),
        capabilities_valid: false,
        capabilities_failure_reason: "UNKNOWN_CAPABILITY".into(),
        ..Default::default()
    };

    let outcome = run_install(&m, &verifier, &deps, &pub_sig, &key_sig, Utc::now());

    assert_eq!(outcome.final_state, PackageInstallState::InstallFailed);
    assert_eq!(outcome.result, PackageVerificationResult::BundleTampered);
    assert_eq!(
        outcome.failed_step,
        Some(PipelineStep::CapabilityDeclaration)
    );
}

// ============================================================================
// 11 — Step 9: Network manifest invalid → InstallFailed
// ============================================================================

#[test]
fn step9_network_manifest_invalid() {
    let (m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::Either,
    );

    let deps = InMemoryPipelineDeps {
        fetched_content_hash: fch.clone(),
        network_manifest_valid: false,
        network_manifest_failure_reason: "NETWORK_MANIFEST_INVALID".into(),
        ..Default::default()
    };

    let outcome = run_install(&m, &verifier, &deps, &pub_sig, &key_sig, Utc::now());

    assert_eq!(outcome.final_state, PackageInstallState::InstallFailed);
    assert_eq!(outcome.result, PackageVerificationResult::BundleTampered);
    assert_eq!(
        outcome.failed_step,
        Some(PipelineStep::NetworkManifestValidation)
    );
}

// ============================================================================
// 12 — Step 10: Policy Deny → InstallFailed
// ============================================================================

#[test]
fn step10_policy_deny() {
    let (m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::Either,
    );

    let deps = InMemoryPipelineDeps {
        fetched_content_hash: fch.clone(),
        policy_outcome: PolicyOutcome::Deny,
        ..Default::default()
    };

    let outcome = run_install(&m, &verifier, &deps, &pub_sig, &key_sig, Utc::now());

    assert_eq!(outcome.final_state, PackageInstallState::InstallFailed);
    assert_eq!(outcome.failed_step, Some(PipelineStep::PolicyDecision));
}

// ============================================================================
// 13 — Step 10: Policy HardDeny → InstallFailed
// ============================================================================

#[test]
fn step10_policy_hard_deny() {
    let (m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::Either,
    );

    let deps = InMemoryPipelineDeps {
        fetched_content_hash: fch.clone(),
        policy_outcome: PolicyOutcome::HardDeny,
        ..Default::default()
    };

    let outcome = run_install(&m, &verifier, &deps, &pub_sig, &key_sig, Utc::now());

    assert_eq!(outcome.final_state, PackageInstallState::InstallFailed);
    assert_eq!(outcome.failed_step, Some(PipelineStep::PolicyDecision));
}

// ============================================================================
// 14 — Step 11: Approval Denied → InstallFailed
// ============================================================================

#[test]
fn step11_approval_denied() {
    let (m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::Either,
    );

    let deps = InMemoryPipelineDeps {
        fetched_content_hash: fch.clone(),
        policy_outcome: PolicyOutcome::RequireApproval,
        approval_outcome: ApprovalOutcome::Denied,
        ..Default::default()
    };

    let outcome = run_install(&m, &verifier, &deps, &pub_sig, &key_sig, Utc::now());

    assert_eq!(outcome.final_state, PackageInstallState::InstallFailed);
    assert_eq!(outcome.failed_step, Some(PipelineStep::Approval));
}

// ============================================================================
// 15 — Step 11: Approval Expired → InstallFailed
// ============================================================================

#[test]
fn step11_approval_expired() {
    let (m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::Either,
    );

    let deps = InMemoryPipelineDeps {
        fetched_content_hash: fch.clone(),
        policy_outcome: PolicyOutcome::RequireApproval,
        approval_outcome: ApprovalOutcome::Expired,
        ..Default::default()
    };

    let outcome = run_install(&m, &verifier, &deps, &pub_sig, &key_sig, Utc::now());

    assert_eq!(outcome.final_state, PackageInstallState::InstallFailed);
    assert_eq!(outcome.failed_step, Some(PipelineStep::Approval));
}

// ============================================================================
// 16 — Step 12: SystemOnly scope + recovery NOT active → InstallFailed
// ============================================================================

#[test]
fn step12_system_only_without_recovery() {
    let (m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::SystemOnly,
    );

    let deps = InMemoryPipelineDeps {
        fetched_content_hash: fch.clone(),
        recovery_active: false, // NOT in recovery
        ..Default::default()
    };

    let outcome = run_install(&m, &verifier, &deps, &pub_sig, &key_sig, Utc::now());

    assert_eq!(outcome.final_state, PackageInstallState::InstallFailed);
    assert_eq!(outcome.failed_step, Some(PipelineStep::RecoveryModeGate));
}

// ============================================================================
// 17 — Step 12: Recovery active → proceeds (happy-path for SystemOnly scope)
// ============================================================================

#[test]
fn step12_system_only_with_recovery_proceeds() {
    let (m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::SystemOnly,
    );

    let deps = InMemoryPipelineDeps {
        fetched_content_hash: fch.clone(),
        recovery_active: true,
        ..Default::default()
    };

    let outcome = run_install(&m, &verifier, &deps, &pub_sig, &key_sig, Utc::now());

    assert_eq!(outcome.final_state, PackageInstallState::Active);
    assert!(outcome.failed_step.is_none());
}

// ============================================================================
// 18 — Step 14: Atomic install failure → InstallFailed
// ============================================================================

#[test]
fn step14_atomic_install_failure() {
    let (m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::Either,
    );

    let deps = InMemoryPipelineDeps {
        fetched_content_hash: fch.clone(),
        atomic_install_success: false,
        atomic_install_failure_reason: "ATOMIC_INSTALL_FAILED".into(),
        ..Default::default()
    };

    let outcome = run_install(&m, &verifier, &deps, &pub_sig, &key_sig, Utc::now());

    assert_eq!(outcome.final_state, PackageInstallState::InstallFailed);
    assert_eq!(outcome.failed_step, Some(PipelineStep::AtomicInstall));
}

// ============================================================================
// 19 — Active → Quarantined transition valid
// ============================================================================

#[test]
fn active_to_quarantined_valid() {
    assert!(can_transition(
        PackageInstallState::Active,
        PackageInstallState::Quarantined
    ));
}

// ============================================================================
// 20 — Active → Uninstalling → Removed valid
// ============================================================================

#[test]
fn active_uninstalling_removed_valid() {
    assert!(can_transition(
        PackageInstallState::Active,
        PackageInstallState::Uninstalling
    ));
    assert!(can_transition(
        PackageInstallState::Uninstalling,
        PackageInstallState::Removed
    ));
}

// ============================================================================
// 21 — Quarantined → Uninstalling valid
// ============================================================================

#[test]
fn quarantined_to_uninstalling_valid() {
    assert!(can_transition(
        PackageInstallState::Quarantined,
        PackageInstallState::Uninstalling
    ));
}

// ============================================================================
// 22 — Pipeline STOPS at Active (does not run FirstRunCapabilityLieAudit)
// ============================================================================

#[test]
fn pipeline_stops_at_active_does_not_run_first_run_audit() {
    let (m, verifier, pub_sig, key_sig, _root, _pubcat, _sigcats, _sk, _pr, fch) = build_setup(
        PublisherTrustLevel::Verified,
        PackageKind::App,
        InstallScope::Either,
    );

    let outcome = run_happy(&m, &verifier, &fch, &pub_sig, &key_sig);

    // The pipeline stops at Active — step 17 is never executed.
    assert_eq!(outcome.final_state, PackageInstallState::Active);
    // failed_step is None, proving step 17 was never reached
    assert!(outcome.failed_step.is_none());
    // still Active — not Quarantined or anything step 17 might produce
    assert_ne!(outcome.final_state, PackageInstallState::InstallFailed);
}

// ============================================================================
// 23 — PipelineStep::label() non-empty for all 17
// ============================================================================

#[test]
fn pipeline_step_label_all_non_empty() {
    let steps = [
        PipelineStep::Fetch,
        PipelineStep::SignatureVerify,
        PipelineStep::TrustChainVerify,
        PipelineStep::PublisherStateCheck,
        PipelineStep::ContentHashVerify,
        PipelineStep::ManifestFieldValidation,
        PipelineStep::SandboxProfileValidation,
        PipelineStep::CapabilityDeclaration,
        PipelineStep::NetworkManifestValidation,
        PipelineStep::PolicyDecision,
        PipelineStep::Approval,
        PipelineStep::RecoveryModeGate,
        PipelineStep::MarkApprovedInstalling,
        PipelineStep::AtomicInstall,
        PipelineStep::CapabilityBinding,
        PipelineStep::MarkActive,
        PipelineStep::FirstRunCapabilityLieAudit,
    ];
    let mut labels = std::collections::HashSet::new();
    for step in &steps {
        let label = step.label();
        assert!(!label.is_empty(), "step {step:?} label is empty");
        assert!(
            labels.insert(label),
            "step {step:?} has duplicate label '{label}'"
        );
    }
    assert_eq!(labels.len(), 17);
}

// ============================================================================
// 24 — DEFAULT_CODE_VERSION → aios-distribution/0.0.1-T194
// ============================================================================

#[test]
fn default_code_version_constant_is_t194() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-distribution/0.0.1-T194");
}
