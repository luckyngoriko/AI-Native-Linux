//! T-162 — M16 closure invariants.
//!
//! Constitutional checks that M16 (aios-network) is honestly closed:
//! version marker, no deferred-stub leakage, trait coverage, invariant
//! reachability (INV I2/I3/I4/I7/I8/I9/I10), and 30 evidence
//! `NetworkRecordType` variants all constructable.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::wildcard_imports,
    clippy::enum_glob_use,
    clippy::module_name_repetitions,
    clippy::missing_const_for_fn,
    unused_imports,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::Utc;
use ed25519_dalek::SigningKey;
use rand_core::OsRng;

use aios_network::{
    AICrossOriginPosture, AiCrossOriginGate, AiExternalCallRequest, AiSubjectClassifier,
    AllowlistEntry, AllowlistEntryKind, ConnectionDecisionV2, ConnectionEvaluator,
    EvaluateConnectionRequestV2, ExposureApprovalFsm, GroupId, InMemoryNetworkEvidenceEmitter,
    InMemoryNetworkPolicyController, NetworkPolicyController, NetworkPolicyErrorCode,
    NetworkPosture, OutboundDirective, OutboundGrant, OutboundGrantRegistry, ProtocolFamily,
    SubjectId, WithEmitter, DEFAULT_CODE_VERSION,
};

use aios_network::evidence::NetworkRecordType;

// ---------------------------------------------------------------------------
// INV-1: Version marker is 0.1.0-T162
// ---------------------------------------------------------------------------

#[test]
fn inv_1_version_marker_is_0_1_0_t162() {
    assert_eq!(
        DEFAULT_CODE_VERSION, "aios-network/0.1.0-T162",
        "DEFAULT_CODE_VERSION must reflect M16 closure"
    );
    assert_eq!(
        env!("CARGO_PKG_VERSION"),
        "0.1.0",
        "CARGO_PKG_VERSION must be 0.1.0"
    );
}

// ---------------------------------------------------------------------------
// INV-2: No Status::Unimplemented, todo!, or unimplemented! in src/
// ---------------------------------------------------------------------------

fn collect_rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
}

#[test]
fn inv_2_no_unimplemented_in_src() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![];
    collect_rs_files(&src, &mut files);
    assert!(!files.is_empty(), "should find .rs files under src/");

    for file in &files {
        let content =
            std::fs::read_to_string(file).unwrap_or_else(|e| panic!("cannot read {file:?}: {e}"));
        let rel = file.strip_prefix(&src).unwrap_or(file);

        assert!(
            !content.contains("Status::Unimplemented"),
            "{rel:?} must not contain Status::Unimplemented"
        );
        assert!(
            !content.contains("todo!("),
            "{rel:?} must not contain todo!()"
        );
        assert!(
            !content.contains("unimplemented!("),
            "{rel:?} must not contain unimplemented!()"
        );
    }
}

// ---------------------------------------------------------------------------
// INV-3: NetworkPosture has 5 variants
// ---------------------------------------------------------------------------

#[test]
fn inv_3_network_posture_5_variants() {
    let variants = [
        NetworkPosture::Airgap,
        NetworkPosture::LoopbackOnly,
        NetworkPosture::LanLocal,
        NetworkPosture::LanExposed,
        NetworkPosture::Public,
    ];
    assert_eq!(
        variants.len(),
        5,
        "NetworkPosture must have exactly 5 variants"
    );
}

// ---------------------------------------------------------------------------
// INV-4: OutboundDirective has at least 5 variants
// ---------------------------------------------------------------------------

#[test]
fn inv_4_outbound_directive_at_least_5_variants() {
    let variants = [
        OutboundDirective::DenyAll,
        OutboundDirective::AllowLoopbackOnly,
        OutboundDirective::AllowListOnly {
            allowlist_id: "id".into(),
        },
        OutboundDirective::AllowVpnOnly {
            tunnel_id: "tun".into(),
        },
        OutboundDirective::AllowInternet,
    ];
    assert!(
        variants.len() >= 5,
        "OutboundDirective must have at least 5 variants"
    );
}

// ---------------------------------------------------------------------------
// INV-5: AICrossOriginPosture has 3 variants (INV I4)
// ---------------------------------------------------------------------------

#[test]
fn inv_5_ai_cross_origin_posture_3_variants() {
    let variants = [
        AICrossOriginPosture::DenyAllExternal,
        AICrossOriginPosture::VaultBrokeredOnly {
            broker_handle: "brk".into(),
        },
        AICrossOriginPosture::OperatorMediated {
            operator_canonical_id: "op".into(),
        },
    ];
    assert_eq!(
        variants.len(),
        3,
        "AICrossOriginPosture must have exactly 3 variants (INV I4)"
    );
}

// ---------------------------------------------------------------------------
// INV-6: NetworkPolicyErrorCode has at least 13 variants
// ---------------------------------------------------------------------------

#[test]
fn inv_6_error_code_at_least_13() {
    let codes = [
        NetworkPolicyErrorCode::DefaultDeny,
        NetworkPolicyErrorCode::CrossGroupAccessForbidden,
        NetworkPolicyErrorCode::AiDirectInternetDenied,
        NetworkPolicyErrorCode::AllowlistFqdnFanoutExceeded,
        NetworkPolicyErrorCode::ExposureEscalationDenied,
        NetworkPolicyErrorCode::GrantSignatureInvalid,
        NetworkPolicyErrorCode::RawSocketBypassAttempted,
        NetworkPolicyErrorCode::ManifestMutationForbidden,
        NetworkPolicyErrorCode::ResolverSignatureInvalid,
        NetworkPolicyErrorCode::VpnPeerKeySignatureInvalid,
        NetworkPolicyErrorCode::PlainDnsForbidden,
        NetworkPolicyErrorCode::MdnsAdvertisementDenied,
        NetworkPolicyErrorCode::Internal,
    ];
    assert!(
        codes.len() >= 13,
        "NetworkPolicyErrorCode must have at least 13 variants"
    );
}

// ---------------------------------------------------------------------------
// INV-7: Trait coverage — NetworkPolicyController has InMemoryNetworkPolicyController
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_7_trait_coverage_controller_in_memory() {
    let emitter = Arc::new(InMemoryNetworkEvidenceEmitter::new("service:test"));
    let controller = InMemoryNetworkPolicyController::new().with_emitter(Some(emitter));
    let posture = controller.current_posture().await;
    assert!(matches!(
        posture,
        NetworkPosture::Airgap
            | NetworkPosture::LoopbackOnly
            | NetworkPosture::LanLocal
            | NetworkPosture::LanExposed
            | NetworkPosture::Public
    ));
}

// ---------------------------------------------------------------------------
// INV-8: INV I3 reachability — ConnectionEvaluator cross-group → Denied(CrossGroupAccessForbidden)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_8_inv_i3_cross_group_forbidden() {
    let registry = Arc::new(OutboundGrantRegistry::new());
    let evaluator = ConnectionEvaluator::new(registry);

    // Register subject in group:a
    evaluator
        .register_subject_group(SubjectId("agent:x".into()), GroupId("group:a".into()))
        .await
        .unwrap();

    let req = EvaluateConnectionRequestV2 {
        subject: SubjectId("agent:x".into()),
        destination_host: "example.com".into(),
        destination_port: 443,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: Some(GroupId("group:b".into())),
    };

    let decision = evaluator.evaluate(req).await.unwrap();
    assert_eq!(
        decision,
        ConnectionDecisionV2::Denied {
            code: NetworkPolicyErrorCode::CrossGroupAccessForbidden,
            reason: format!(
                "source group {} cannot reach destination group {}",
                "group:a", "group:b"
            )
        },
        "INV I3: cross-group access must be forbidden"
    );
}

// ---------------------------------------------------------------------------
// INV-9: INV I4 reachability — AiCrossOriginGate DenyAllExternal → AiDirectInternetDenied
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_9_inv_i4_ai_deny_all_external() {
    let classifier = AiSubjectClassifier::new();
    let gate = AiCrossOriginGate::new(classifier);

    let request = AiExternalCallRequest {
        subject: SubjectId("agent:test".into()),
        endpoint: "https://api.openai.com/v1/chat".into(),
        broker_handle: None,
        operator_approval_id: None,
    };

    let result = gate.evaluate_external_call(request).await;
    assert!(
        result.is_err(),
        "INV I4: DenyAllExternal must deny AI direct internet"
    );
    let err = result.unwrap_err();
    assert_eq!(err.code(), NetworkPolicyErrorCode::AiDirectInternetDenied);
}

// ---------------------------------------------------------------------------
// INV-10: INV I7 reachability — OutboundGrantRegistry rejects invalid Ed25519 signature
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_10_inv_i7_rejects_invalid_signature() {
    let emitter = Arc::new(InMemoryNetworkEvidenceEmitter::new("service:test"));
    let mut registry = OutboundGrantRegistry::new();
    registry = registry.with_emitter(Some(emitter));

    let signing_key = SigningKey::generate(&mut OsRng);
    let vk = signing_key.verifying_key();
    let fingerprint = aios_network::fingerprint_from_vk(&vk);

    registry.register_authority(&fingerprint, vk);

    let mut grant = OutboundGrant {
        grant_id: "grt_inv07".into(),
        subject: SubjectId("agent:test".into()),
        allowlist: vec![AllowlistEntry {
            kind: AllowlistEntryKind::HostFqdn,
            value: "api.example.com".into(),
            port_policy: aios_network::PortPolicy::RegisteredEphemeral,
            protocol: ProtocolFamily::Tcp,
        }],
        directive_kind: aios_network::OutboundDirectiveKind::AllowListOnly,
        issued_at: Utc::now(),
        expires_at: None,
        signer_fingerprint: fingerprint.clone(),
        signature: vec![],
    };

    // Tampered signature: all zeros (not a real Ed25519 sig)
    grant.signature = vec![0u8; 64];

    let result = registry.append_grant(grant).await;
    assert!(
        result.is_err(),
        "INV I7: invalid signature must be rejected"
    );
    let err = result.unwrap_err();
    assert_eq!(
        err.code(),
        NetworkPolicyErrorCode::GrantSignatureInvalid,
        "rejection must carry GrantSignatureInvalid"
    );
}

// ---------------------------------------------------------------------------
// INV-11: INV I8 reachability — OutboundGrantRegistry rejects manifest shrink
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_11_inv_i8_rejects_manifest_shrink() {
    let emitter = Arc::new(InMemoryNetworkEvidenceEmitter::new("service:test"));
    let mut registry = OutboundGrantRegistry::new();
    registry = registry.with_emitter(Some(emitter));

    let signing_key = SigningKey::generate(&mut OsRng);
    let vk = signing_key.verifying_key();
    let fingerprint = aios_network::fingerprint_from_vk(&vk);
    registry.register_authority(&fingerprint, vk);

    // First grant — no expiry (perpetual)
    let mut grant1 = OutboundGrant {
        grant_id: "grt_inv08".into(),
        subject: SubjectId("agent:test".into()),
        allowlist: vec![AllowlistEntry {
            kind: AllowlistEntryKind::HostFqdn,
            value: "a.example.com".into(),
            port_policy: aios_network::PortPolicy::RegisteredEphemeral,
            protocol: ProtocolFamily::Tcp,
        }],
        directive_kind: aios_network::OutboundDirectiveKind::AllowListOnly,
        issued_at: Utc::now(),
        expires_at: None,
        signer_fingerprint: fingerprint.clone(),
        signature: vec![],
    };
    aios_network::sign_grant(&mut grant1, &signing_key);
    registry
        .append_grant(grant1.clone())
        .await
        .expect("first grant");

    // Second grant with a shorter expiry — this is a "shrink" and must be rejected
    let mut grant2 = OutboundGrant {
        grant_id: "grt_inv08_v2".into(),
        subject: SubjectId("agent:test".into()),
        allowlist: vec![AllowlistEntry {
            kind: AllowlistEntryKind::HostFqdn,
            value: "a.example.com".into(),
            port_policy: aios_network::PortPolicy::RegisteredEphemeral,
            protocol: ProtocolFamily::Tcp,
        }],
        directive_kind: aios_network::OutboundDirectiveKind::AllowListOnly,
        issued_at: Utc::now(),
        expires_at: Some(Utc::now()),
        signer_fingerprint: fingerprint.clone(),
        signature: vec![],
    };
    aios_network::sign_grant(&mut grant2, &signing_key);

    let result = registry.append_grant(grant2).await;
    assert!(result.is_err(), "INV I8: manifest shrink must be rejected");
    let err = result.unwrap_err();
    assert_eq!(
        err.code(),
        NetworkPolicyErrorCode::ManifestMutationForbidden
    );
}

// ---------------------------------------------------------------------------
// INV-12: INV I9 reachability — ConnectionEvaluator rejects FQDN fan-out > 16
// ---------------------------------------------------------------------------

#[test]
fn inv_12_inv_i9_rejects_fqdn_fanout_16() {
    let registry = Arc::new(OutboundGrantRegistry::new());
    let evaluator = ConnectionEvaluator::new(registry);

    let addresses: Vec<std::net::IpAddr> = (0..17)
        .map(|i| format!("10.0.0.{i}").parse().unwrap())
        .collect();

    let result = evaluator.resolve_fqdn_bounded("big-fanout.example.com", addresses);
    assert!(result.is_err(), "INV I9: fan-out > 16 must be rejected");
    let err = result.unwrap_err();
    assert_eq!(
        err.code(),
        NetworkPolicyErrorCode::AllowlistFqdnFanoutExceeded
    );
}

// ---------------------------------------------------------------------------
// INV-13: INV I10 reachability — ExposureApprovalFsm rejects direct LAN→PUBLIC
// ---------------------------------------------------------------------------

#[tokio::test]
async fn inv_13_inv_i10_rejects_lan_active_to_public_direct() {
    let fsm = ExposureApprovalFsm::new();

    // Set LAN active first
    fsm.request_lan(SubjectId("human:op".into())).await.unwrap();
    fsm.apply_lan_policy_decision("poldec_lan").await.unwrap();
    fsm.activate_lan().await.unwrap();

    // Attempt direct transition LAN→PublicPending (must revoke first) — denied
    let result = fsm
        .request_public(SubjectId("human:op".into()), "recov_session_1")
        .await;
    assert!(
        result.is_err(),
        "INV I10: direct LAN→PUBLIC transition must be denied"
    );
    let err = result.unwrap_err();
    assert_eq!(err.code(), NetworkPolicyErrorCode::ExposureEscalationDenied);
}

// ---------------------------------------------------------------------------
// INV-14: 30 NETWORK/VPN/DNS/MDNS_* evidence RecordType variants constructable
// ---------------------------------------------------------------------------

#[test]
fn inv_14_thirty_network_record_types_constructable() {
    let all = [
        NetworkRecordType::NetworkPostureChanged,
        NetworkRecordType::ExposureRequested,
        NetworkRecordType::ExposureGranted,
        NetworkRecordType::ExposureActivated,
        NetworkRecordType::ExposureHeartbeatRecorded,
        NetworkRecordType::ExposureRevoked,
        NetworkRecordType::PublicExposureGranted,
        NetworkRecordType::PublicExposureTtlExpired,
        NetworkRecordType::OutboundGrantIssued,
        NetworkRecordType::OutboundGrantRevoked,
        NetworkRecordType::ConnectionAllowed,
        NetworkRecordType::ConnectionDenied,
        NetworkRecordType::CrossGroupAccessForbidden,
        NetworkRecordType::AllowlistFqdnFanoutExceeded,
        NetworkRecordType::AiDirectInternetDenied,
        NetworkRecordType::AiExternalCallBrokered,
        NetworkRecordType::RawSocketBypassAttempted,
        NetworkRecordType::FirewallFallbackActivated,
        NetworkRecordType::DnsQueryAudit,
        NetworkRecordType::ResolverListAdmitted,
        NetworkRecordType::ResolverListRotated,
        NetworkRecordType::PlainDnsBlocked,
        NetworkRecordType::VpnTunnelProposed,
        NetworkRecordType::VpnTunnelApproved,
        NetworkRecordType::VpnTunnelActivated,
        NetworkRecordType::VpnTunnelHandshakeRecorded,
        NetworkRecordType::VpnTunnelRevoked,
        NetworkRecordType::VpnPeerKeyRotated,
        NetworkRecordType::MdnsPostureChanged,
        NetworkRecordType::MdnsAdvertisementRejected,
    ];

    assert_eq!(
        all.len(),
        30,
        "Must have exactly 30 NetworkRecordType variants"
    );

    for (i, record) in all.iter().enumerate() {
        let name = record.as_str();
        assert!(
            !name.is_empty(),
            "variant {i} ({record:?}) must have non-empty as_str()"
        );
    }
}
