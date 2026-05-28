//! Integration tests for the gRPC `NetworkPolicyService` + `DnsVpnService` surfaces (T-160).
//!
//! Each test boots an in-process tonic server backed by in-memory implementations,
//! connects via a TCP listener, and exercises one RPC path.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::no_effect_underscore_binding,
    clippy::doc_markdown,
    clippy::significant_drop_tightening,
    clippy::wildcard_imports,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use ed25519_dalek::SigningKey;
use tonic::Request;

use aios_network::ai_discipline::{AiCrossOriginGate, AiSubjectClassifier};
use aios_network::connection_evaluator::ConnectionEvaluator;
use aios_network::controller::InMemoryNetworkPolicyController;
use aios_network::dns::{
    fingerprint_from_vk, generate_keypair as dns_generate_keypair, sign_allowlist, DnsTransport,
    ResolverAllowlist, ResolverBackend, ResolverEndpoint, ResolverProfile, ResolverProfileManager,
};
use aios_network::exposure_fsm::ExposureApprovalFsm;
use aios_network::firewall::{
    FirewallAction, FirewallBackend, FirewallChain, FirewallManager, FirewallMatch, FirewallRule,
    FirewallRulesetBuilder,
};
use aios_network::grant_registry::OutboundGrantRegistry;
use aios_network::mdns::MdnsGate;
use aios_network::service::proto::dns_vpn_service_client::DnsVpnServiceClient;
use aios_network::service::proto::network_policy_service_client::NetworkPolicyServiceClient;
use aios_network::service::proto::{
    self, ApplyFirewallRulesetRequest, CheckMdnsAdvertisementRequest, EvaluateConnectionRequest,
    GetSubjectDirectiveRequest, GetSubjectManifestRequest, ProposeVpnTunnelRequest,
    RegisterSubjectGroupRequest, ResolverAllowlistProto, ResolverEndpointProto,
    RevokeSubjectDirectiveRequest, SetMdnsPostureRequest, SetSubjectDirectiveRequest,
};
use aios_network::service::{
    build_dnsvpn_router, build_network_router, DnsVpnServer, NetworkPolicyServer,
};
use aios_network::vpn::VpnTunnelManager;

// ── NetworkPolicy test harness ─────────────────────────────────────────────

struct NetworkTestHarness {
    client: NetworkPolicyServiceClient<tonic::transport::Channel>,
}

impl NetworkTestHarness {
    async fn new() -> Self {
        let controller = Arc::new(InMemoryNetworkPolicyController::new());
        let exposure_fsm = Arc::new(ExposureApprovalFsm::new());
        let grant_registry = Arc::new(OutboundGrantRegistry::new());
        let evaluator = Arc::new(ConnectionEvaluator::new(Arc::clone(&grant_registry)));
        let ai_gate = Arc::new(AiCrossOriginGate::new(AiSubjectClassifier::new()));
        let firewall = Arc::new(FirewallManager::new());

        let svc = NetworkPolicyServer::new(
            controller as Arc<dyn aios_network::controller::NetworkPolicyController>,
            exposure_fsm,
            grant_registry,
            evaluator,
            ai_gate,
            firewall,
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let router = build_network_router(svc);

        tokio::spawn(async move {
            router
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = NetworkPolicyServiceClient::connect(format!("http://{addr}"))
            .await
            .unwrap();

        Self { client }
    }
}

// ── DnsVpn test harness ───────────────────────────────────────────────────

struct DnsVpnTestHarness {
    client: DnsVpnServiceClient<tonic::transport::Channel>,
    resolver_sk: SigningKey,
    resolver_fp: String,
    _mdns_sk: SigningKey,
    _mdns_fp: String,
}

impl DnsVpnTestHarness {
    async fn new() -> Self {
        let profile = ResolverProfile {
            backend: ResolverBackend::Unbound,
            active_list_id: "default".into(),
            effective_endpoints: vec![],
            cache_ttl_seconds: 300,
        };
        let mut resolver_mgr = ResolverProfileManager::new(profile);
        let (resolver_sk, resolver_vk) = dns_generate_keypair();
        let resolver_fp = fingerprint_from_vk(&resolver_vk);
        resolver_mgr.register_authority(&resolver_fp, resolver_vk);

        let vpn_mgr = Arc::new(VpnTunnelManager::new());
        let mdns_gate = Arc::new(MdnsGate::new());

        // Register an mDNS authority for allowlist tests
        let (mdns_sk, mdns_vk) = dns_generate_keypair();
        let mdns_fp = fingerprint_from_vk(&mdns_vk);
        mdns_gate.register_authority(&mdns_fp, mdns_vk).await;

        let svc = DnsVpnServer::new(Arc::new(resolver_mgr), vpn_mgr, mdns_gate);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let router = build_dnsvpn_router(svc);

        tokio::spawn(async move {
            router
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = DnsVpnServiceClient::connect(format!("http://{addr}"))
            .await
            .unwrap();

        Self {
            client,
            resolver_sk,
            resolver_fp,
            _mdns_sk: mdns_sk,
            _mdns_fp: mdns_fp,
        }
    }
}

// ── Helper: build a signed ResolverAllowlist ──────────────────────────────

fn build_signed_resolver_allowlist(
    list_id: &str,
    endpoints: Vec<ResolverEndpoint>,
    sk: &SigningKey,
    fp: &str,
) -> ResolverAllowlist {
    let mut list = ResolverAllowlist {
        list_id: list_id.into(),
        endpoints,
        signed_at: Utc::now(),
        signer_fingerprint: fp.into(),
        signature: Vec::new(),
    };
    sign_allowlist(&mut list, sk);
    list
}

fn dot_endpoint(fqdn: &str, ip: &str) -> ResolverEndpoint {
    ResolverEndpoint {
        fqdn: fqdn.into(),
        address: ip.parse().unwrap(),
        port: 853,
        transport: DnsTransport::DnsOverTls,
        spki_pin: None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NetworkPolicyService tests
// ═══════════════════════════════════════════════════════════════════════════

// ── Test 1: network server boots ──────────────────────────────────────────

#[tokio::test]
async fn network_server_boots() {
    let _harness = NetworkTestHarness::new().await;
}

// ── Test 2: GetPosture returns LanLocal default ───────────────────────────

#[tokio::test]
async fn get_posture_returns_lan_local_default() {
    let mut harness = NetworkTestHarness::new().await;
    let resp = harness
        .client
        .get_posture(Request::new(proto::GetPostureRequest {}))
        .await
        .unwrap();
    let inner = resp.into_inner();
    assert_eq!(inner.posture, proto::NetworkPostureProto::LanLocal as i32);
}

// ── Test 3: SetPosture to LoopbackOnly returns receipt ────────────────────

#[tokio::test]
async fn set_posture_to_loopback_only_returns_receipt() {
    let mut harness = NetworkTestHarness::new().await;
    let resp = harness
        .client
        .set_posture(Request::new(proto::SetPostureRequest {
            new_posture: proto::NetworkPostureProto::LoopbackOnly as i32,
            actor: "human:op".into(),
        }))
        .await
        .unwrap();
    let receipt = resp.into_inner();
    assert_eq!(receipt.from, proto::NetworkPostureProto::LanLocal as i32);
    assert_eq!(receipt.to, proto::NetworkPostureProto::LoopbackOnly as i32);
    assert_eq!(receipt.actor, "human:op");
}

// ── Test 4: SetSubjectDirective then GetSubjectDirective round-trip ───────

#[tokio::test]
async fn set_then_get_subject_directive_roundtrip() {
    let mut harness = NetworkTestHarness::new().await;
    let msg = proto::OutboundDirectiveMsg {
        kind: proto::OutboundDirectiveProto::AllowInternet as i32,
        allowlist_id: None,
        tunnel_id: None,
    };
    harness
        .client
        .set_subject_directive(Request::new(SetSubjectDirectiveRequest {
            subject: "human:test".into(),
            directive: Some(msg),
            actor: "human:op".into(),
        }))
        .await
        .unwrap();

    let resp = harness
        .client
        .get_subject_directive(Request::new(GetSubjectDirectiveRequest {
            subject: "human:test".into(),
        }))
        .await
        .unwrap();
    let inner = resp.into_inner();
    let dir = inner.directive.unwrap();
    assert_eq!(
        dir.kind,
        proto::OutboundDirectiveProto::AllowInternet as i32
    );
}

// ── Test 5: EvaluateConnection with no grant returns DefaultDeny ──────────

#[tokio::test]
async fn evaluate_connection_no_grant_returns_default_deny() {
    let mut harness = NetworkTestHarness::new().await;
    let resp = harness
        .client
        .evaluate_connection(Request::new(EvaluateConnectionRequest {
            subject: "unknown:subj".into(),
            destination_host: "example.com".into(),
            destination_port: 443,
            protocol: proto::ProtocolFamilyProto::Tcp as i32,
            destination_group_hint: None,
        }))
        .await
        .unwrap();
    let decision = resp.into_inner();
    assert_eq!(
        decision.decision,
        proto::connection_decision_proto::Decision::Denied as i32
    );
    assert_eq!(decision.error_code.as_deref(), Some("DefaultDeny"));
}

// ── Test 6: cross-group EvaluateConnection returns PermissionDenied ───────

#[tokio::test]
async fn evaluate_connection_cross_group_returns_permission_denied() {
    let mut harness = NetworkTestHarness::new().await;
    // Register subject in group-a
    harness
        .client
        .register_subject_group(Request::new(RegisterSubjectGroupRequest {
            subject: "agent:srv".into(),
            group: "group-a".into(),
        }))
        .await
        .unwrap();

    let resp = harness
        .client
        .evaluate_connection(Request::new(EvaluateConnectionRequest {
            subject: "agent:srv".into(),
            destination_host: "10.0.0.1".into(),
            destination_port: 443,
            protocol: proto::ProtocolFamilyProto::Tcp as i32,
            destination_group_hint: Some("group-b".into()),
        }))
        .await
        .unwrap();
    let decision = resp.into_inner();
    assert_eq!(
        decision.decision,
        proto::connection_decision_proto::Decision::Denied as i32
    );
    assert_eq!(
        decision.error_code.as_deref(),
        Some("CrossGroupAccessForbidden")
    );
}

// ── Test 7: ApplyFirewallRuleset with nftables backend succeeds ───────────

#[tokio::test]
async fn apply_firewall_ruleset_nftables_succeeds() {
    let mut harness = NetworkTestHarness::new().await;
    let ruleset = FirewallRulesetBuilder::new(FirewallBackend::Nftables)
        .rule(FirewallRule {
            rule_id: "allow-loopback".into(),
            chain: FirewallChain::Output,
            priority: 100,
            match_expr: FirewallMatch::DestCidr("127.0.0.0/8".into()),
            action: FirewallAction::Accept,
            comment: "allow loopback".into(),
        })
        .build();

    let backend_proto = proto::FirewallBackendProto::Nftables as i32;
    let rule_proto = proto::FirewallRuleProto {
        rule_id: ruleset.rules[0].rule_id.clone(),
        chain: proto::FirewallChainProto::FwOutput as i32,
        priority: ruleset.rules[0].priority,
        match_kind: proto::FirewallMatchProto::FwDestCidr as i32,
        match_value: "127.0.0.0/8".into(),
        match_port: None,
        match_protocol: None,
        action: proto::FirewallActionProto::FwAccept as i32,
        comment: ruleset.rules[0].comment.clone(),
    };

    harness
        .client
        .apply_firewall_ruleset(Request::new(ApplyFirewallRulesetRequest {
            ruleset: Some(proto::FirewallRulesetProto {
                backend: backend_proto,
                rules: vec![rule_proto],
                generation: ruleset.generation,
                built_at: Some(prost_types::Timestamp {
                    seconds: ruleset.built_at.timestamp(),
                    nanos: ruleset.built_at.timestamp_subsec_nanos() as i32,
                }),
            }),
        }))
        .await
        .unwrap();
}

// ── Test 8: GetFirewallStatus reports fallback flag ───────────────────────

#[tokio::test]
async fn get_firewall_status_reports_fallback_flag() {
    let mut harness = NetworkTestHarness::new().await;
    let resp = harness
        .client
        .get_firewall_status(Request::new(proto::GetFirewallStatusRequest {}))
        .await
        .unwrap();
    let inner = resp.into_inner();
    // No iptables fallback applied yet — fallback should be false.
    assert!(!inner.fallback_active);
    assert_eq!(inner.history_count, 0);
}

// ── Test 9: RevokeSubjectDirective then Get returns DenyAll default ───────

#[tokio::test]
async fn revoke_subject_directive_then_get_returns_deny_all() {
    let mut harness = NetworkTestHarness::new().await;
    let msg = proto::OutboundDirectiveMsg {
        kind: proto::OutboundDirectiveProto::AllowInternet as i32,
        allowlist_id: None,
        tunnel_id: None,
    };
    harness
        .client
        .set_subject_directive(Request::new(SetSubjectDirectiveRequest {
            subject: "human:tmp".into(),
            directive: Some(msg),
            actor: "human:op".into(),
        }))
        .await
        .unwrap();

    harness
        .client
        .revoke_subject_directive(Request::new(RevokeSubjectDirectiveRequest {
            subject: "human:tmp".into(),
            actor: "human:op".into(),
        }))
        .await
        .unwrap();

    let resp = harness
        .client
        .get_subject_directive(Request::new(GetSubjectDirectiveRequest {
            subject: "human:tmp".into(),
        }))
        .await
        .unwrap();
    let dir = resp.into_inner().directive.unwrap();
    assert_eq!(dir.kind, proto::OutboundDirectiveProto::DenyAll as i32);
}

// ── Test 10: GetSubjectManifest with no grants returns NotFound ───────────

#[tokio::test]
async fn get_subject_manifest_no_grants_returns_not_found() {
    let mut harness = NetworkTestHarness::new().await;
    let status = harness
        .client
        .get_subject_manifest(Request::new(GetSubjectManifestRequest {
            subject: "never:seen".into(),
        }))
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

// ═══════════════════════════════════════════════════════════════════════════
// DnsVpnService tests
// ═══════════════════════════════════════════════════════════════════════════

// ── Test 11: DnsVpn server boots ──────────────────────────────────────────

#[tokio::test]
async fn dnsvpn_server_boots() {
    let _harness = DnsVpnTestHarness::new().await;
}

// ── Test 12: AdmitResolverAllowlist with valid signature succeeds ─────────

#[tokio::test]
async fn admit_resolver_allowlist_valid_signature_succeeds() {
    let harness = DnsVpnTestHarness::new().await;
    let list = build_signed_resolver_allowlist(
        "L1",
        vec![dot_endpoint("ns1.example.com", "1.1.1.1")],
        &harness.resolver_sk,
        &harness.resolver_fp,
    );

    let proto_list = ResolverAllowlistProto {
        list_id: list.list_id.clone(),
        endpoints: list
            .endpoints
            .iter()
            .map(|ep| ResolverEndpointProto {
                fqdn: ep.fqdn.clone(),
                address: ep.address.to_string(),
                port: u32::from(ep.port),
                transport: "DNS_OVER_TLS".into(),
                spki_pin: ep.spki_pin.clone(),
            })
            .collect(),
        signed_at: Some(prost_types::Timestamp {
            seconds: list.signed_at.timestamp(),
            nanos: list.signed_at.timestamp_subsec_nanos() as i32,
        }),
        signer_fingerprint: list.signer_fingerprint.clone(),
        signature: list.signature.clone(),
    };

    let mut client = harness.client;
    client
        .admit_resolver_allowlist(Request::new(proto_list))
        .await
        .unwrap();
}

// ── Test 13: AdmitResolverAllowlist with invalid signature returns PermissionDenied ──

#[tokio::test]
async fn admit_resolver_allowlist_invalid_signature_returns_permission_denied() {
    let harness = DnsVpnTestHarness::new().await;
    let list = build_signed_resolver_allowlist(
        "L1",
        vec![dot_endpoint("ns1.example.com", "1.1.1.1")],
        &harness.resolver_sk,
        &harness.resolver_fp,
    );

    // Tamper with the list ID after signing.
    let proto_list = ResolverAllowlistProto {
        list_id: "L1-tampered".into(),
        endpoints: list
            .endpoints
            .iter()
            .map(|ep| ResolverEndpointProto {
                fqdn: ep.fqdn.clone(),
                address: ep.address.to_string(),
                port: u32::from(ep.port),
                transport: "DNS_OVER_TLS".into(),
                spki_pin: ep.spki_pin.clone(),
            })
            .collect(),
        signed_at: Some(prost_types::Timestamp {
            seconds: list.signed_at.timestamp(),
            nanos: list.signed_at.timestamp_subsec_nanos() as i32,
        }),
        signer_fingerprint: list.signer_fingerprint.clone(),
        signature: list.signature.clone(),
    };

    let mut client = harness.client;
    let status = client
        .admit_resolver_allowlist(Request::new(proto_list))
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

// ── Test 14: ProposeVpnTunnel → ApproveVpnTunnel → ActivateVpnTunnel happy path ──

#[tokio::test]
async fn propose_approve_activate_vpn_tunnel_happy_path() {
    let harness = DnsVpnTestHarness::new().await;
    let mut client = harness.client;

    let local_pk = [1u8; 32];
    let peer_pk = [2u8; 32];

    // 1. Propose
    client
        .propose_vpn_tunnel(Request::new(ProposeVpnTunnelRequest {
            tunnel_id: "wg-home".into(),
            kind: "WIREGUARD_SPLIT_TUNNEL".into(),
            interface_name: "wg0".into(),
            local_private_key_handle: "vault://wg-home-key".into(),
            local_public_key: local_pk.to_vec(),
            peers: vec![proto::WireGuardPeerProto {
                peer_id: "peer-1".into(),
                endpoint: "203.0.113.1:51820".into(),
                public_key: peer_pk.to_vec(),
                allowed_ips: vec!["10.0.0.0/24".into()],
                persistent_keepalive_seconds: 25,
            }],
            mtu: None,
            fwmark: None,
            requester: "human:op".into(),
        }))
        .await
        .unwrap();

    // 2. Approve
    client
        .approve_vpn_tunnel(Request::new(proto::ApproveVpnTunnelRequest {
            tunnel_id: "wg-home".into(),
            decision_id: "dec-001".into(),
        }))
        .await
        .unwrap();

    // 3. Activate
    client
        .activate_vpn_tunnel(Request::new(proto::ActivateVpnTunnelRequest {
            tunnel_id: "wg-home".into(),
        }))
        .await
        .unwrap();
}

// ── Test 15: ProposeVpnTunnel with blacklisted kind returns Internal ──────

#[tokio::test]
async fn propose_vpn_tunnel_blacklisted_kind_returns_internal() {
    let harness = DnsVpnTestHarness::new().await;
    let mut client = harness.client;

    let local_pk = [1u8; 32];
    let status = client
        .propose_vpn_tunnel(Request::new(ProposeVpnTunnelRequest {
            tunnel_id: "wg-bad".into(),
            kind: "OPERATOR_DEFINED_OTHER_BLACKLISTED".into(),
            interface_name: "wg0".into(),
            local_private_key_handle: "vault://wg-bad-key".into(),
            local_public_key: local_pk.to_vec(),
            peers: vec![],
            mtu: None,
            fwmark: None,
            requester: "human:op".into(),
        }))
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Internal);
}

// ── Test 16: SetMdnsPosture then CheckMdnsAdvertisement enforces posture ──

#[tokio::test]
async fn set_mdns_posture_then_check_advertisement_enforces_posture() {
    let harness = DnsVpnTestHarness::new().await;
    let mut client = harness.client;

    // Default posture DenyDefault — should deny
    let status = client
        .check_mdns_advertisement(Request::new(CheckMdnsAdvertisementRequest {
            service_type: "_http._tcp".into(),
            instance_name: "My Service".into(),
            port: 8080,
        }))
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);

    // Set to RecoveryDenied — should also deny
    client
        .set_mdns_posture(Request::new(SetMdnsPostureRequest {
            posture: "RECOVERY_DENIED".into(),
            allowlist_id: None,
        }))
        .await
        .unwrap();

    let status = client
        .check_mdns_advertisement(Request::new(CheckMdnsAdvertisementRequest {
            service_type: "_http._tcp".into(),
            instance_name: "My Service".into(),
            port: 8080,
        }))
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}

// ── Test 17: ListVpnTunnels returns empty initially ───────────────────────

#[tokio::test]
async fn list_vpn_tunnels_returns_empty_initially() {
    let harness = DnsVpnTestHarness::new().await;
    let mut client = harness.client;

    let resp = client
        .list_vpn_tunnels(Request::new(proto::ListVpnTunnelsRequest {}))
        .await
        .unwrap();
    assert!(resp.into_inner().tunnels.is_empty());
}

// ── Test 18: GetResolverProfile returns default profile ───────────────────

#[tokio::test]
async fn get_resolver_profile_returns_default_profile() {
    let harness = DnsVpnTestHarness::new().await;
    let mut client = harness.client;

    let resp = client
        .get_resolver_profile(Request::new(proto::GetResolverProfileRequest {}))
        .await
        .unwrap();
    let profile = resp.into_inner();
    assert_eq!(profile.backend, "UNBOUND");
    assert_eq!(profile.active_list_id, "default");
}

// ── Test 19: SetSubjectDirective to AllowLoopbackOnly then ListDirectives ──

#[tokio::test]
async fn list_directives_after_setting_one_returns_one() {
    let mut harness = NetworkTestHarness::new().await;
    let msg = proto::OutboundDirectiveMsg {
        kind: proto::OutboundDirectiveProto::AllowLoopbackOnly as i32,
        allowlist_id: None,
        tunnel_id: None,
    };
    harness
        .client
        .set_subject_directive(Request::new(SetSubjectDirectiveRequest {
            subject: "human:a".into(),
            directive: Some(msg),
            actor: "human:op".into(),
        }))
        .await
        .unwrap();

    let resp = harness
        .client
        .list_directives(Request::new(proto::ListDirectivesRequest {}))
        .await
        .unwrap();
    let entries = resp.into_inner().entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].subject, "human:a");
}

// ── Test 20: RecordVpnHandshake on unknown tunnel returns Internal ────────

#[tokio::test]
async fn record_vpn_handshake_unknown_tunnel_returns_internal() {
    let harness = DnsVpnTestHarness::new().await;
    let mut client = harness.client;

    let status = client
        .record_vpn_handshake(Request::new(proto::RecordVpnHandshakeRequest {
            tunnel_id: "nonexistent".into(),
        }))
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Internal);
}
