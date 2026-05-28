//! Evidence emission integration tests (T-161).
//!
//! Covers 30 `NetworkRecordType` variants across 19 trait methods with
//! receipt count, payload shape, chain integrity, INV-015 secret redaction,
//! and monotonic sequence numbering assertions.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_const_for_fn,
    clippy::module_name_repetitions,
    clippy::too_many_lines,
    clippy::wildcard_imports,
    clippy::enum_glob_use,
    reason = "test code — panics are assertions, repetition is clarity, names mirror the SUT"
)]

use chrono::Utc;

use aios_network::{
    AllowlistEntryKind, ConnectionDecisionV2, EvaluateConnectionRequestV2, ExposureApprovalLabel,
    ExposureTransition, ExposureTransitionReason, FirewallBackend, GrantTombstone, GroupId,
    InMemoryNetworkEvidenceEmitter, MdnsAvahiPosture, NetworkEvidenceEmitter,
    NetworkPolicyErrorCode, PeerKeyRotation, PostureChangeReceipt, ProtocolFamily, SubjectId,
    TunnelLifecycleLabel,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn subject() -> SubjectId {
    SubjectId("agent:test".into())
}

fn group_a() -> GroupId {
    GroupId("group:a".into())
}

fn group_b() -> GroupId {
    GroupId("group:b".into())
}

fn emitter() -> InMemoryNetworkEvidenceEmitter {
    InMemoryNetworkEvidenceEmitter::new("service:aios-network-test")
}

// ---------------------------------------------------------------------------
// 1. Construction
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emitter_new_starts_empty() {
    let e = emitter();
    assert_eq!(e.receipt_count().await, 0);
}

// ---------------------------------------------------------------------------
// 2. emit_posture_changed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_posture_changed_creates_receipt_and_payload_is_correct() {
    let e = emitter();
    let receipt = PostureChangeReceipt {
        from: aios_network::NetworkPosture::LanLocal,
        to: aios_network::NetworkPosture::Airgap,
        actor: SubjectId("human:op".into()),
        at: Utc::now(),
    };
    let r = e.emit_posture_changed(&receipt).await.unwrap();
    assert!(r.record_id.starts_with("evr_"));
    assert_eq!(r.hash.len(), 64);
    assert_eq!(e.receipt_count().await, 1);

    let payload = e.get_payload(0).await.unwrap();
    assert_eq!(payload["from"], "lan-local");
    assert_eq!(payload["to"], "airgap");
    assert_eq!(payload["actor"], "human:op");
}

// ---------------------------------------------------------------------------
// 3. emit_exposure_transition
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_exposure_transition_granted_activated_revoked() {
    let e = emitter();

    // Transition to LanApproved → ExposureGranted
    let t = ExposureTransition {
        from: ExposureApprovalLabel::Loopback,
        to: ExposureApprovalLabel::LanApproved,
        transitioned_at: Utc::now(),
        reason: ExposureTransitionReason::Initial,
    };
    let _r = e.emit_exposure_transition(&t, "human:op").await.unwrap();
    assert_eq!(e.receipt_count().await, 1);
    let p = e.get_payload(0).await.unwrap();
    assert!(p["from"].as_str().unwrap().contains("Loopback"));
    assert!(p["to"].as_str().unwrap().contains("LanApproved"));

    // Transition to LanActive → ExposureActivated
    let t2 = ExposureTransition {
        from: ExposureApprovalLabel::LanApproved,
        to: ExposureApprovalLabel::LanActive,
        transitioned_at: Utc::now(),
        reason: ExposureTransitionReason::LanActivated,
    };
    e.emit_exposure_transition(&t2, "human:op").await.unwrap();
    assert_eq!(e.receipt_count().await, 2);

    // Transition to Revoked → ExposureRevoked
    let t3 = ExposureTransition {
        from: ExposureApprovalLabel::LanActive,
        to: ExposureApprovalLabel::Revoked,
        transitioned_at: Utc::now(),
        reason: ExposureTransitionReason::Revoked {
            reason: "operator decision".into(),
        },
    };
    e.emit_exposure_transition(&t3, "human:op").await.unwrap();
    assert_eq!(e.receipt_count().await, 3);
    let p3 = e.get_payload(2).await.unwrap();
    assert!(p3["reason"].as_str().unwrap().contains("Revoked"));
}

// ---------------------------------------------------------------------------
// 4. emit_outbound_grant_issued / revoked
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_outbound_grant_issued_and_revoked_round_trip() {
    let e = emitter();
    let subj = subject();

    // Issue
    let _r = e
        .emit_outbound_grant_issued("grant-01", &subj)
        .await
        .unwrap();
    assert_eq!(e.receipt_count().await, 1);
    let p = e.get_payload(0).await.unwrap();
    assert_eq!(p["grant_id"], "grant-01");
    assert_eq!(p["subject"], "agent:test");

    // Revoke
    let tombstone = GrantTombstone {
        revoked_grant_id: "grant-01".into(),
        revoked_at: Utc::now(),
        revoker: SubjectId("human:op".into()),
        reason: "revoked by operator".into(),
    };
    e.emit_outbound_grant_revoked(&tombstone).await.unwrap();
    assert_eq!(e.receipt_count().await, 2);
    let p2 = e.get_payload(1).await.unwrap();
    assert_eq!(p2["revoked_grant_id"], "grant-01");
    assert_eq!(p2["revoker"], "human:op");
    assert_eq!(p2["reason"], "revoked by operator");
}

// ---------------------------------------------------------------------------
// 5. emit_connection_decision
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_connection_decision_allowed_and_denied() {
    let e = emitter();

    // Allowed
    let req = EvaluateConnectionRequestV2 {
        subject: subject(),
        destination_host: "example.com".into(),
        destination_port: 443,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: None,
    };
    let decision = ConnectionDecisionV2::Allowed {
        matched_rule_id: "fqdn:example.com".into(),
        allowlist_entry_kind: AllowlistEntryKind::HostFqdn,
    };
    e.emit_connection_decision(&req, &decision).await.unwrap();
    assert_eq!(e.receipt_count().await, 1);
    let p = e.get_payload(0).await.unwrap();
    assert_eq!(p["destination_host"], "example.com");
    assert_eq!(p["allowed"], true);

    // Denied
    let decision2 = ConnectionDecisionV2::Denied {
        code: NetworkPolicyErrorCode::DefaultDeny,
        reason: "no allowlist entry matches".into(),
    };
    e.emit_connection_decision(&req, &decision2).await.unwrap();
    assert_eq!(e.receipt_count().await, 2);
    let p2 = e.get_payload(1).await.unwrap();
    assert_eq!(p2["allowed"], false);
    assert!(p2["details"].as_str().unwrap().contains("DefaultDeny"));
}

// ---------------------------------------------------------------------------
// 6. emit_cross_group_forbidden
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_cross_group_forbidden_captures_source_and_dest() {
    let e = emitter();
    e.emit_cross_group_forbidden(&group_a(), &group_b())
        .await
        .unwrap();
    assert_eq!(e.receipt_count().await, 1);
    let p = e.get_payload(0).await.unwrap();
    assert_eq!(p["source_group"], "group:a");
    assert_eq!(p["dest_group"], "group:b");
}

// ---------------------------------------------------------------------------
// 7. emit_fqdn_fanout_exceeded
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_fqdn_fanout_exceeded_reports_count() {
    let e = emitter();
    e.emit_fqdn_fanout_exceeded("cdn.example.com", 42)
        .await
        .unwrap();
    assert_eq!(e.receipt_count().await, 1);
    let p = e.get_payload(0).await.unwrap();
    assert_eq!(p["fqdn"], "cdn.example.com");
    assert_eq!(p["resolved_count"], 42);
}

// ---------------------------------------------------------------------------
// 8. emit_ai_direct_internet_denied
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_ai_direct_internet_denied_forever() {
    let e = emitter();
    e.emit_ai_direct_internet_denied(&subject(), "https://api.openai.com/v1")
        .await
        .unwrap();
    assert_eq!(e.receipt_count().await, 1);
    let p = e.get_payload(0).await.unwrap();
    assert_eq!(p["subject"], "agent:test");
    assert_eq!(p["attempted_endpoint"], "https://api.openai.com/v1");
}

// ---------------------------------------------------------------------------
// 9. emit_ai_external_call_brokered
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_ai_external_call_brokered_through_vault() {
    let e = emitter();
    e.emit_ai_external_call_brokered(&subject(), "vault-broker-01")
        .await
        .unwrap();
    assert_eq!(e.receipt_count().await, 1);
    let p = e.get_payload(0).await.unwrap();
    assert_eq!(p["subject"], "agent:test");
    assert_eq!(p["broker_handle"], "vault-broker-01");
}

// ---------------------------------------------------------------------------
// 10. emit_raw_socket_bypass_attempted
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_raw_socket_bypass_attempted_forever() {
    let e = emitter();
    e.emit_raw_socket_bypass_attempted(&subject())
        .await
        .unwrap();
    assert_eq!(e.receipt_count().await, 1);
    let p = e.get_payload(0).await.unwrap();
    assert_eq!(p["subject"], "agent:test");
}

// ---------------------------------------------------------------------------
// 11. emit_firewall_fallback_activated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_firewall_fallback_activated_forever() {
    let e = emitter();
    e.emit_firewall_fallback_activated(FirewallBackend::IptablesFallback)
        .await
        .unwrap();
    assert_eq!(e.receipt_count().await, 1);
    let p = e.get_payload(0).await.unwrap();
    assert_eq!(p["backend"], "IPTABLES_FALLBACK");
}

// ---------------------------------------------------------------------------
// 12. emit_dns_query_audit (question-only per S8.4 §3)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_dns_query_audit_question_only_no_answers() {
    let e = emitter();
    e.emit_dns_query_audit(
        "example.com",
        aios_network::ResolverBackend::SystemdResolved,
        "resolved",
    )
    .await
    .unwrap();
    assert_eq!(e.receipt_count().await, 1);
    let p = e.get_payload(0).await.unwrap();
    assert_eq!(p["fqdn"], "example.com");
    assert_eq!(p["outcome"], "resolved");
    // INV-015: no answer set, no raw signature field
    assert!(p.get("answers").is_none());
    assert!(p.get("signature").is_none());
    assert!(p.get("sig_bytes").is_none());
}

// ---------------------------------------------------------------------------
// 13. emit_resolver_list_admitted / rotated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_resolver_list_admitted_and_rotated() {
    let e = emitter();

    e.emit_resolver_list_admitted("rl-01", "fp:abc123")
        .await
        .unwrap();
    assert_eq!(e.receipt_count().await, 1);
    let p = e.get_payload(0).await.unwrap();
    assert_eq!(p["list_id"], "rl-01");
    assert_eq!(p["signer_fingerprint"], "fp:abc123");

    e.emit_resolver_list_rotated("rl-01", "rl-02")
        .await
        .unwrap();
    assert_eq!(e.receipt_count().await, 2);
    let p2 = e.get_payload(1).await.unwrap();
    assert_eq!(p2["from_list_id"], "rl-01");
    assert_eq!(p2["to_list_id"], "rl-02");
}

// ---------------------------------------------------------------------------
// 14. emit_plain_dns_blocked
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_plain_dns_blocked_forever() {
    let e = emitter();
    e.emit_plain_dns_blocked("UDP:53 from agent:test")
        .await
        .unwrap();
    assert_eq!(e.receipt_count().await, 1);
    let p = e.get_payload(0).await.unwrap();
    assert_eq!(p["attempt_context"], "UDP:53 from agent:test");
}

// ---------------------------------------------------------------------------
// 15. emit_vpn_tunnel_event — lifecycle labels
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_vpn_tunnel_event_full_lifecycle() {
    let e = emitter();

    let labels = [
        TunnelLifecycleLabel::Proposed,
        TunnelLifecycleLabel::Approved,
        TunnelLifecycleLabel::Active,
        TunnelLifecycleLabel::Failed,
        TunnelLifecycleLabel::Revoked,
    ];
    for (i, label) in labels.iter().enumerate() {
        e.emit_vpn_tunnel_event("tun-01", *label).await.unwrap();
        assert_eq!(e.receipt_count().await, i + 1);
        let p = e.get_payload(i).await.unwrap();
        assert_eq!(p["tunnel_id"], "tun-01");
        assert!(!p["label"].as_str().unwrap().is_empty());
    }
}

// ---------------------------------------------------------------------------
// 16. emit_vpn_peer_key_rotated — INV-015: NO raw signature in payload
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_vpn_peer_key_rotated_no_raw_signature_in_payload() {
    let e = emitter();
    let rotation = PeerKeyRotation {
        tunnel_id: "tun-01".into(),
        old_pubkey: [0xAAu8; 32],
        new_pubkey: [0xBBu8; 32],
        rotated_at: Utc::now(),
        authority_fingerprint: "fp:auth01".into(),
        signature: vec![0x01, 0x02, 0x03],
    };
    e.emit_vpn_peer_key_rotated(&rotation).await.unwrap();
    assert_eq!(e.receipt_count().await, 1);
    let p = e.get_payload(0).await.unwrap();
    // INV-015: NO raw signature bytes in the payload
    assert!(p.get("signature").is_none());
    assert!(p.get("sig_bytes").is_none());
    assert_eq!(p["tunnel_id"], "tun-01");
    assert_eq!(p["authority_fingerprint"], "fp:auth01");
    // Hex-encoded pubkey fingerprints (not raw bytes)
    let old_fp = p["old_pubkey_fingerprint"].as_str().unwrap();
    let new_fp = p["new_pubkey_fingerprint"].as_str().unwrap();
    assert_eq!(old_fp.len(), 64);
    assert_eq!(new_fp.len(), 64);
}

// ---------------------------------------------------------------------------
// 17. emit_mdns_posture_changed / advertisement_rejected
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_mdns_posture_changed_all_variants() {
    let e = emitter();

    e.emit_mdns_posture_changed(&MdnsAvahiPosture::DenyDefault)
        .await
        .unwrap();
    e.emit_mdns_posture_changed(&MdnsAvahiPosture::RecoveryDenied)
        .await
        .unwrap();
    e.emit_mdns_posture_changed(&MdnsAvahiPosture::OperatorAuthorised {
        allowlist_id: "mdns-al-01".into(),
    })
    .await
    .unwrap();
    e.emit_mdns_posture_changed(&MdnsAvahiPosture::AirgapDenied)
        .await
        .unwrap();
    assert_eq!(e.receipt_count().await, 4);
}

#[tokio::test]
async fn emit_mdns_advertisement_rejected() {
    let e = emitter();
    e.emit_mdns_advertisement_rejected("_http._tcp", "My Printer", 8080, "not in allowlist")
        .await
        .unwrap();
    assert_eq!(e.receipt_count().await, 1);
    let p = e.get_payload(0).await.unwrap();
    assert_eq!(p["service_type"], "_http._tcp");
    assert_eq!(p["instance_name"], "My Printer");
    assert_eq!(p["port"], 8080);
    assert_eq!(p["reason"], "not in allowlist");
}

// ---------------------------------------------------------------------------
// 18. Chain integrity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn chain_integrity_verification_is_empty_chain_error() {
    let e = emitter();
    // Empty chain → verify_integrity returns error
    let result = e.verify_chain().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn chain_integrity_passes_after_many_emissions() {
    let e = emitter();
    let subj = subject();

    // Emit 8 receipts across different methods
    e.emit_posture_changed(&PostureChangeReceipt {
        from: aios_network::NetworkPosture::LanLocal,
        to: aios_network::NetworkPosture::LoopbackOnly,
        actor: subj.clone(),
        at: Utc::now(),
    })
    .await
    .unwrap();
    e.emit_outbound_grant_issued("g-1", &subj).await.unwrap();
    e.emit_outbound_grant_revoked(&GrantTombstone {
        revoked_grant_id: "g-1".into(),
        revoked_at: Utc::now(),
        revoker: subj.clone(),
        reason: "test".into(),
    })
    .await
    .unwrap();
    e.emit_cross_group_forbidden(&group_a(), &group_b())
        .await
        .unwrap();
    e.emit_ai_direct_internet_denied(&subj, "https://evil.com")
        .await
        .unwrap();
    e.emit_firewall_fallback_activated(FirewallBackend::IptablesFallback)
        .await
        .unwrap();
    e.emit_dns_query_audit(
        "example.com",
        aios_network::ResolverBackend::Unbound,
        "resolved",
    )
    .await
    .unwrap();
    e.emit_mdns_advertisement_rejected("_http._tcp", "svc", 80, "test")
        .await
        .unwrap();

    assert_eq!(e.receipt_count().await, 8);
    // Chain integrity should pass after multiple emissions
    let result = e.verify_chain().await;
    assert!(result.is_ok(), "chain integrity failed: {result:?}");
}

// ---------------------------------------------------------------------------
// 19. Sequence numbers are monotonic
// ---------------------------------------------------------------------------

#[tokio::test]
async fn receipt_sequence_numbers_are_monotonic_zero_based() {
    let e = emitter();
    let subj = subject();

    let r0 = e
        .emit_outbound_grant_issued("g-seq-0", &subj)
        .await
        .unwrap();
    let r1 = e
        .emit_outbound_grant_issued("g-seq-1", &subj)
        .await
        .unwrap();
    let r2 = e
        .emit_outbound_grant_revoked(&GrantTombstone {
            revoked_grant_id: "g-seq-0".into(),
            revoked_at: Utc::now(),
            revoker: subj.clone(),
            reason: "test".into(),
        })
        .await
        .unwrap();

    assert_eq!(r0.sequence, 0);
    assert_eq!(r1.sequence, 1);
    assert_eq!(r2.sequence, 2);
}

// ---------------------------------------------------------------------------
// 20. ConnectionDecisionV2 fields preserved in payload
// ---------------------------------------------------------------------------

#[tokio::test]
async fn connection_decision_payload_preserves_destination_port_and_protocol() {
    let e = emitter();
    let req = EvaluateConnectionRequestV2 {
        subject: subject(),
        destination_host: "10.0.0.1".into(),
        destination_port: 8443,
        protocol: ProtocolFamily::Udp,
        destination_group_hint: Some(group_a()),
    };
    let decision = ConnectionDecisionV2::Allowed {
        matched_rule_id: "ipv4:10.0.0.1".into(),
        allowlist_entry_kind: AllowlistEntryKind::IpV4Address,
    };
    e.emit_connection_decision(&req, &decision).await.unwrap();
    let p = e.get_payload(0).await.unwrap();
    assert_eq!(p["destination_port"], 8443);
    assert_eq!(p["protocol"], "UDP");
}
