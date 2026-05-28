//! T-162 — M16 acceptance fixtures.
//!
//! Three E2E acceptance tests covering the 6 phases defined in T-162:
//! Phase 1 bootstrap, Phase 2 posture + exposure, Phase 3 outbound grant +
//! connection eval, Phase 4 AI cross-origin, Phase 5 DNS + VPN, Phase 6
//! mDNS + firewall. Evidence chain hash continuity is verified where emitters
//! are wired.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::wildcard_imports,
    clippy::enum_glob_use,
    clippy::module_name_repetitions,
    unused_imports,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use chrono::Utc;
use ed25519_dalek::SigningKey;
use rand_core::OsRng;

use aios_network::{
    sign_grant, AICrossOriginPosture, AiCrossOriginGate, AiExternalCallDecision,
    AiExternalCallRequest, AiSubjectClassifier, AllowedVia, AllowlistEntry, AllowlistEntryKind,
    ConnectionDecisionV2, ConnectionEvaluator, DnsTransport, EvaluateConnectionRequestV2,
    ExposureApprovalFsm, ExposureApprovalLabel, FirewallAction, FirewallBackend, FirewallChain,
    FirewallManager, FirewallMatch, FirewallRule, FirewallRulesetBuilder, GroupId,
    InMemoryNetworkEvidenceEmitter, InMemoryNetworkPolicyController, MdnsAvahiPosture, MdnsGate,
    NetworkPolicyController, NetworkPolicyErrorCode, NetworkPosture, OutboundDirective,
    OutboundGrant, OutboundGrantRegistry, PortPolicy, ProtocolFamily, ResolverAllowlist,
    ResolverBackend, ResolverEndpoint, ResolverProfile, ResolverProfileManager, SubjectId,
    VpnTunnelKind, VpnTunnelManager, WireGuardConfig, WireGuardPeer, WithEmitter,
};

// ---------------------------------------------------------------------------
// Acceptance test 1 — E2E full pipeline (Phases 1–4)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn m16_e2e_bootstrap_posture_grant_connection_ai_gate() {
    // ── Phase 1: Bootstrap ──────────────────────────────────────────────
    let controller = InMemoryNetworkPolicyController::new();
    assert_eq!(
        controller.current_posture().await,
        NetworkPosture::LanLocal,
        "default posture must be LanLocal"
    );

    // ── Phase 2: Posture + Exposure ─────────────────────────────────────
    // Walk the full posture spectrum.
    let actor = SubjectId("human:op".into());
    for posture in [
        NetworkPosture::Airgap,
        NetworkPosture::LoopbackOnly,
        NetworkPosture::LanLocal,
        NetworkPosture::LanExposed,
    ] {
        let receipt = controller
            .set_posture(posture, actor.clone())
            .await
            .unwrap();
        assert_eq!(receipt.to, posture);
    }
    assert_eq!(controller.posture_history().await.len(), 4);

    // Exposure FSM — full LAN lifecycle (Loopback → LanPending → LanApproved → LanActive).
    let fsm = ExposureApprovalFsm::new();
    assert_eq!(fsm.current().await.label(), ExposureApprovalLabel::Loopback);
    fsm.request_lan(SubjectId("human:op".into())).await.unwrap();
    fsm.apply_lan_policy_decision("poldec_lan").await.unwrap();
    fsm.activate_lan().await.unwrap();
    assert_eq!(
        fsm.current().await.label(),
        ExposureApprovalLabel::LanActive
    );
    // Heartbeat record.
    fsm.record_lan_heartbeat().await.unwrap();
    assert_eq!(fsm.history().await.len(), 4);

    // ── Phase 3: Outbound grant + connection eval ───────────────────────
    let signing_key = SigningKey::generate(&mut OsRng);
    let vk = signing_key.verifying_key();
    let fingerprint = aios_network::fingerprint_from_vk(&vk);

    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fingerprint, vk);

    let mut grant = OutboundGrant {
        grant_id: "grt_e2e_01".into(),
        subject: SubjectId("agent:test".into()),
        allowlist: vec![
            AllowlistEntry {
                kind: AllowlistEntryKind::HostFqdn,
                value: "api.example.com".into(),
                port_policy: PortPolicy::RegisteredEphemeral,
                protocol: ProtocolFamily::Tcp,
            },
            AllowlistEntry {
                kind: AllowlistEntryKind::IpV4Address,
                value: "10.0.0.55".into(),
                port_policy: PortPolicy::OperatorAssigned { port: 8443 },
                protocol: ProtocolFamily::Tcp,
            },
        ],
        directive_kind: aios_network::OutboundDirectiveKind::AllowListOnly,
        issued_at: Utc::now(),
        expires_at: None,
        signer_fingerprint: fingerprint.clone(),
        signature: vec![],
    };
    sign_grant(&mut grant, &signing_key);
    registry.append_grant(grant).await.unwrap();

    // Verify effective allowlist has the entries.
    let entries = registry
        .effective_allowlist(&SubjectId("agent:test".into()))
        .await;
    assert_eq!(entries.len(), 2);

    // Connection evaluator — same-group allowed match.
    let evaluator = ConnectionEvaluator::new(Arc::new(registry));
    evaluator
        .register_subject_group(SubjectId("agent:test".into()), GroupId("group:a".into()))
        .await
        .unwrap();

    let decision = evaluator
        .evaluate(EvaluateConnectionRequestV2 {
            subject: SubjectId("agent:test".into()),
            destination_host: "api.example.com".into(),
            destination_port: 9000,
            protocol: ProtocolFamily::Tcp,
            destination_group_hint: None,
        })
        .await
        .unwrap();
    assert!(matches!(decision, ConnectionDecisionV2::Allowed { .. }));

    // IP-based match.
    let decision2 = evaluator
        .evaluate(EvaluateConnectionRequestV2 {
            subject: SubjectId("agent:test".into()),
            destination_host: "10.0.0.55".into(),
            destination_port: 8443,
            protocol: ProtocolFamily::Tcp,
            destination_group_hint: None,
        })
        .await
        .unwrap();
    assert!(matches!(decision2, ConnectionDecisionV2::Allowed { .. }));

    // Deny when no allowlist entry matches.
    let decision3 = evaluator
        .evaluate(EvaluateConnectionRequestV2 {
            subject: SubjectId("agent:test".into()),
            destination_host: "evil.com".into(),
            destination_port: 443,
            protocol: ProtocolFamily::Tcp,
            destination_group_hint: None,
        })
        .await
        .unwrap();
    assert_eq!(
        decision3,
        ConnectionDecisionV2::Denied {
            code: NetworkPolicyErrorCode::DefaultDeny,
            reason: "no allowlist entry matches".into(),
        }
    );

    // ── Phase 4: AI cross-origin ────────────────────────────────────────
    let mut ai_gate = AiCrossOriginGate::new(AiSubjectClassifier::new());

    // Register a Vault broker so brokered calls can succeed.
    let broker_handle = "vault://primary".to_string();
    ai_gate.register_broker(broker_handle.clone(), "fp-primary".into());

    // Set AI posture to VaultBrokeredOnly.
    ai_gate
        .set_posture(
            SubjectId("agent:test".into()),
            AICrossOriginPosture::VaultBrokeredOnly {
                broker_handle: broker_handle.clone(),
            },
            SubjectId("human:op".into()),
        )
        .await
        .unwrap();

    // Non-AI subject bypasses gate.
    let non_ai_decision = ai_gate
        .evaluate_external_call(AiExternalCallRequest {
            subject: SubjectId("human:lucky".into()),
            endpoint: "https://example.com".into(),
            broker_handle: None,
            operator_approval_id: None,
        })
        .await
        .unwrap();
    assert_eq!(
        non_ai_decision,
        AiExternalCallDecision::Allowed {
            via: AllowedVia::BypassedNonAi,
        }
    );

    // AI subject with valid broker handle succeeds.
    let ai_decision = ai_gate
        .evaluate_external_call(AiExternalCallRequest {
            subject: SubjectId("agent:test".into()),
            endpoint: "https://api.openai.com/v1".into(),
            broker_handle: Some(broker_handle.clone()),
            operator_approval_id: None,
        })
        .await
        .unwrap();
    assert!(matches!(
        ai_decision,
        AiExternalCallDecision::Allowed {
            via: AllowedVia::VaultBroker { .. }
        }
    ));

    // AI subject without broker handle is denied.
    let err = ai_gate
        .evaluate_external_call(AiExternalCallRequest {
            subject: SubjectId("agent:test".into()),
            endpoint: "https://evil.com".into(),
            broker_handle: None,
            operator_approval_id: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), NetworkPolicyErrorCode::AiDirectInternetDenied);
}

// ---------------------------------------------------------------------------
// Acceptance test 2 — Cross-group evaluation + evidence chain (Phase 3 deep)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn m16_cross_group_evidence_chain_continuity() {
    // ── Set up grant registry with a signed grant ───────────────────────
    let signing_key = SigningKey::generate(&mut OsRng);
    let vk = signing_key.verifying_key();
    let fingerprint = aios_network::fingerprint_from_vk(&vk);

    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fingerprint, vk);

    let mut grant = OutboundGrant {
        grant_id: "grt_accept_02".into(),
        subject: SubjectId("agent:service".into()),
        allowlist: vec![AllowlistEntry {
            kind: AllowlistEntryKind::HostFqdn,
            value: "internal.api".into(),
            port_policy: PortPolicy::RegisteredEphemeral,
            protocol: ProtocolFamily::Tcp,
        }],
        directive_kind: aios_network::OutboundDirectiveKind::AllowListOnly,
        issued_at: Utc::now(),
        expires_at: None,
        signer_fingerprint: fingerprint.clone(),
        signature: vec![],
    };
    sign_grant(&mut grant, &signing_key);
    registry.append_grant(grant).await.unwrap();

    // ── Wire evidence emitter ───────────────────────────────────────────
    let emitter = Arc::new(InMemoryNetworkEvidenceEmitter::new("service:e2e-test"));
    let evaluator =
        ConnectionEvaluator::new(Arc::new(registry)).with_emitter(Some(emitter.clone()));

    // Register two subjects in different groups.
    evaluator
        .register_subject_group(SubjectId("agent:service".into()), GroupId("group:a".into()))
        .await
        .unwrap();
    evaluator
        .register_subject_group(SubjectId("agent:other".into()), GroupId("group:b".into()))
        .await
        .unwrap();

    // ── Same-group: allowed ─────────────────────────────────────────────
    let decision = evaluator
        .evaluate(EvaluateConnectionRequestV2 {
            subject: SubjectId("agent:service".into()),
            destination_host: "internal.api".into(),
            destination_port: 9000,
            protocol: ProtocolFamily::Tcp,
            destination_group_hint: None,
        })
        .await
        .unwrap();
    assert!(matches!(decision, ConnectionDecisionV2::Allowed { .. }));

    // ── Cross-group: denied (INV I3) ────────────────────────────────────
    let cross_decision = evaluator
        .evaluate(EvaluateConnectionRequestV2 {
            subject: SubjectId("agent:service".into()),
            destination_host: "internal.api".into(),
            destination_port: 9000,
            protocol: ProtocolFamily::Tcp,
            destination_group_hint: Some(GroupId("group:b".into())),
        })
        .await
        .unwrap();
    assert_eq!(
        cross_decision,
        ConnectionDecisionV2::Denied {
            code: NetworkPolicyErrorCode::CrossGroupAccessForbidden,
            reason: "source group group:a cannot reach destination group group:b".into(),
        }
    );

    // ── Evidence chain: emitter captured receipts ───────────────────────
    let count = emitter.receipt_count().await;
    assert!(
        count >= 2,
        "emitter must have at least 2 receipts, got {count}"
    );

    // Verify full hash-chain integrity (BLAKE3).
    emitter.verify_chain().await.unwrap();

    // Each receipt has a non-empty payload.
    for i in 0..count {
        let payload = emitter.get_payload(i).await;
        assert!(payload.is_some(), "receipt {i} must have a payload");
    }

    // ── FQDN fan-out bound (INV I9) ─────────────────────────────────────
    let addresses: Vec<IpAddr> = (0..17)
        .map(|i| IpAddr::V4(Ipv4Addr::new(10, 0, 0, i)))
        .collect();
    let result = evaluator.resolve_fqdn_bounded("fanout.example.com", addresses);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code(),
        NetworkPolicyErrorCode::AllowlistFqdnFanoutExceeded,
    );

    // ── Manifest shrink rejection (INV I8) ──────────────────────────────
    let mut shrink_grant = OutboundGrant {
        grant_id: "grt_accept_02_v2".into(),
        subject: SubjectId("agent:service".into()),
        allowlist: vec![AllowlistEntry {
            kind: AllowlistEntryKind::HostFqdn,
            value: "internal.api".into(),
            port_policy: PortPolicy::RegisteredEphemeral,
            protocol: ProtocolFamily::Tcp,
        }],
        directive_kind: aios_network::OutboundDirectiveKind::AllowListOnly,
        issued_at: Utc::now(),
        expires_at: Some(Utc::now()), // shrink: perpetual → now
        signer_fingerprint: fingerprint.clone(),
        signature: vec![],
    };
    sign_grant(&mut shrink_grant, &signing_key);

    let mut registry2 = OutboundGrantRegistry::new();
    registry2.register_authority(&fingerprint, vk);

    // Re-register first grant.
    let mut first = OutboundGrant {
        grant_id: "grt_accept_02".into(),
        subject: SubjectId("agent:service".into()),
        allowlist: vec![AllowlistEntry {
            kind: AllowlistEntryKind::HostFqdn,
            value: "internal.api".into(),
            port_policy: PortPolicy::RegisteredEphemeral,
            protocol: ProtocolFamily::Tcp,
        }],
        directive_kind: aios_network::OutboundDirectiveKind::AllowListOnly,
        issued_at: Utc::now(),
        expires_at: None,
        signer_fingerprint: fingerprint.clone(),
        signature: vec![],
    };
    sign_grant(&mut first, &signing_key);
    registry2.append_grant(first).await.unwrap();

    let shrink_err = registry2.append_grant(shrink_grant).await.unwrap_err();
    assert_eq!(
        shrink_err.code(),
        NetworkPolicyErrorCode::ManifestMutationForbidden,
    );
}

// ---------------------------------------------------------------------------
// Acceptance test 3 — DNS + VPN + mDNS + Firewall happy path (Phases 5–6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn m16_dns_vpn_mdns_firewall_happy_path() {
    // ── Phase 5a: DNS — resolver profile + allowlist ────────────────────
    let initial_profile = ResolverProfile {
        backend: ResolverBackend::Unbound,
        active_list_id: "default".into(),
        effective_endpoints: vec![],
        cache_ttl_seconds: 300,
    };
    let mut dns_mgr = ResolverProfileManager::new(initial_profile);

    // Verify plain DNS is forbidden via transport validation.
    assert!(aios_network::validate_transport(DnsTransport::DnsOverTls).is_ok());
    assert!(aios_network::validate_transport(DnsTransport::DnsOverHttps).is_ok());
    assert!(aios_network::validate_transport(DnsTransport::DnsOverQuic).is_ok());
    assert!(aios_network::validate_transport(DnsTransport::PlainDnsForbidden).is_err());

    // Register authority and admit a signed allowlist.
    let signing_key = SigningKey::generate(&mut OsRng);
    let vk = signing_key.verifying_key();
    let fingerprint = aios_network::fingerprint_from_vk(&vk);
    dns_mgr.register_authority(&fingerprint, vk);

    let mut allowlist = ResolverAllowlist {
        list_id: "dns-list-01".into(),
        endpoints: vec![ResolverEndpoint {
            fqdn: "dns.quad9.net".into(),
            address: IpAddr::V4(Ipv4Addr::new(9, 9, 9, 9)),
            port: 853,
            transport: DnsTransport::DnsOverTls,
            spki_pin: None,
        }],
        signed_at: Utc::now(),
        signer_fingerprint: fingerprint.clone(),
        signature: vec![],
    };
    aios_network::sign_allowlist(&mut allowlist, &signing_key);
    dns_mgr.admit_allowlist(allowlist).await.unwrap();

    // Rotate to the new list.
    dns_mgr.rotate_active_list("dns-list-01").await.unwrap();
    let profile = dns_mgr.current_profile().await;
    assert_eq!(profile.active_list_id, "dns-list-01");
    assert_eq!(profile.effective_endpoints.len(), 1);
    assert_eq!(profile.effective_endpoints[0].port, 853);

    // Query guard increments and decrements.
    {
        let _guard = dns_mgr.begin_query();
    }
    let backend = dns_mgr.audit_resolver_used().await;
    assert_eq!(backend, ResolverBackend::Unbound);

    // ── Phase 5b: VPN — full tunnel lifecycle ───────────────────────────
    let vpn_mgr = VpnTunnelManager::new();

    let wg_config = WireGuardConfig {
        tunnel_id: "wg-e2e-01".into(),
        kind: VpnTunnelKind::WireGuardSplitTunnel,
        interface_name: "wg0".into(),
        local_private_key_handle: "vault://wg/priv/01".into(),
        local_public_key: [0xAAu8; 32],
        peers: vec![WireGuardPeer {
            peer_id: "peer-01".into(),
            endpoint: "198.51.100.1:51820".into(),
            public_key: [0xBBu8; 32],
            allowed_ips: vec!["10.8.0.0/24".into()],
            persistent_keepalive_seconds: 25,
        }],
        mtu: Some(1420),
        fwmark: None,
    };

    // Propose → Approve → Activate → Handshake → Revoke.
    vpn_mgr
        .propose_tunnel(wg_config, SubjectId("human:op".into()))
        .await
        .unwrap();
    vpn_mgr
        .approve_tunnel("wg-e2e-01", "poldec_vpn")
        .await
        .unwrap();
    vpn_mgr.activate_tunnel("wg-e2e-01").await.unwrap();
    vpn_mgr.record_handshake("wg-e2e-01").await.unwrap();
    vpn_mgr
        .revoke_tunnel("wg-e2e-01", "operator decommission")
        .await
        .unwrap();

    // ── Phase 6a: mDNS — gate posture + advertisement check ─────────────
    let mdns_gate = MdnsGate::new();

    // Default posture is DenyDefault; check rejects everything.
    let mdns_result = mdns_gate
        .check_advertisement("_http._tcp", "My Service", 8080)
        .await;
    assert!(mdns_result.is_err());

    // Set operator-authorised posture.
    mdns_gate
        .set_posture(MdnsAvahiPosture::OperatorAuthorised {
            allowlist_id: "mdns-al-01".into(),
        })
        .await
        .unwrap();

    // RecoveryDenied hard-denies regardless of allowlist.
    mdns_gate
        .set_posture(MdnsAvahiPosture::RecoveryDenied)
        .await
        .unwrap();
    let recovery_result = mdns_gate
        .check_advertisement("_http._tcp", "Recovery Svc", 8080)
        .await;
    assert!(recovery_result.is_err());

    // ── Phase 6b: Firewall — ruleset builder + manager lifecycle ────────
    let fw_mgr = FirewallManager::new();

    // Build an nftables ruleset.
    let ruleset = FirewallRulesetBuilder::new(FirewallBackend::Nftables)
        .rule(FirewallRule {
            rule_id: "allow-loopback".into(),
            chain: FirewallChain::Output,
            priority: 0,
            match_expr: FirewallMatch::DestIp(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            action: FirewallAction::Accept,
            comment: "allow loopback".into(),
        })
        .rule(FirewallRule {
            rule_id: "drop-all".into(),
            chain: FirewallChain::Output,
            priority: i32::MAX,
            match_expr: FirewallMatch::All,
            action: FirewallAction::Drop,
            comment: "default deny".into(),
        })
        .build();

    assert_eq!(ruleset.backend, FirewallBackend::Nftables);
    assert_eq!(ruleset.rules.len(), 2);
    assert!(ruleset.generation > 0);

    // Apply → active.
    fw_mgr.apply_ruleset(ruleset).await.unwrap();
    let active = fw_mgr.active_ruleset().await.unwrap();
    assert_eq!(active.rules.len(), 2);
    assert_eq!(active.rules[0].rule_id, "allow-loopback");

    // Second ruleset pushes first to history.
    let ruleset2 = FirewallRulesetBuilder::new(FirewallBackend::Nftables)
        .rule(FirewallRule {
            rule_id: "updated-rule".into(),
            chain: FirewallChain::Input,
            priority: 1,
            match_expr: FirewallMatch::CtState("established,related".into()),
            action: FirewallAction::Accept,
            comment: "allow established".into(),
        })
        .build();
    fw_mgr.apply_ruleset(ruleset2).await.unwrap();
    assert_eq!(fw_mgr.history().await.len(), 1);
    assert_eq!(fw_mgr.active_ruleset().await.unwrap().rules.len(), 1);

    // iptables fallback flips the flag.
    let fallback_ruleset = FirewallRulesetBuilder::new(FirewallBackend::IptablesFallback)
        .rule(FirewallRule {
            rule_id: "fb-drop".into(),
            chain: FirewallChain::Output,
            priority: 0,
            match_expr: FirewallMatch::All,
            action: FirewallAction::Drop,
            comment: "fallback deny".into(),
        })
        .build();
    fw_mgr.apply_ruleset(fallback_ruleset).await.unwrap();
    assert!(fw_mgr.is_in_fallback().await);

    // Subject directive compilation.
    let rules = fw_mgr
        .enforce_subject_directive(&SubjectId("test:subj".into()), &OutboundDirective::DenyAll)
        .await;
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].action, FirewallAction::Drop);

    let loopback_rules = fw_mgr
        .enforce_subject_directive(
            &SubjectId("test:subj".into()),
            &OutboundDirective::AllowLoopbackOnly,
        )
        .await;
    assert_eq!(loopback_rules.len(), 2);
    assert_eq!(loopback_rules[0].action, FirewallAction::Accept);
    assert_eq!(loopback_rules[1].action, FirewallAction::Drop);
}
