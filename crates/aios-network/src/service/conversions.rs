//! Rust ↔ proto translations for gRPC `NetworkPolicyService` + `DnsVpnService` (T-160).
//!
//! Owns bidirectional translation between domain types and tonic-generated proto
//! types, plus the `network_error_to_status` mapper that translates each
//! `NetworkPolicyError` variant into the appropriate `tonic::Status` gRPC code.

#![allow(
    clippy::result_large_err,
    missing_docs,
    clippy::match_wildcard_for_single_variants,
    clippy::use_self,
    clippy::cast_possible_truncation,
    clippy::clone_on_copy,
    clippy::missing_errors_doc,
    clippy::too_many_lines,
    clippy::wildcard_imports
)]

use std::net::IpAddr;

use chrono::{DateTime, TimeZone, Utc};
use prost_types::Timestamp;
use tonic::Status;

use crate::ai_cross_origin::AICrossOriginPosture;
use crate::ai_discipline::{AiExternalCallDecision, AiExternalCallRequest, AllowedVia};
use crate::allowlist::{AllowlistEntry, AllowlistEntryKind};
use crate::connection_evaluator::{ConnectionDecisionV2, EvaluateConnectionRequestV2};
use crate::controller::PostureChangeReceipt;
use crate::dns::{
    DnsTransport, ResolverAllowlist, ResolverBackend, ResolverEndpoint, ResolverProfile,
};
use crate::error::NetworkPolicyError;
use crate::exposure_fsm::ExposureTransitionReason;
use crate::firewall::{
    FirewallAction, FirewallBackend, FirewallChain, FirewallMatch, FirewallRule, FirewallRuleset,
};
use crate::ids::{GroupId, SubjectId};
use crate::inbound::{InboundExposureClass, PortPolicy};
use crate::mdns::{MdnsAdvertisement, MdnsAdvertisementAllowlist, MdnsAvahiPosture};
use crate::outbound::OutboundDirective;
use crate::outbound_grant::{
    GrantTombstone, NetworkOutboundManifest, OutboundDirectiveKind, OutboundGrant,
};
use crate::posture::NetworkPosture;
use crate::protocol::ProtocolFamily;
use crate::service::proto;
use crate::vpn::{
    PeerKeyRotation, TunnelLifecycleLabel, VpnTunnelKind, WireGuardConfig, WireGuardPeer,
};

// ── Timestamp helpers ──────────────────────────────────────────────────────

fn datetime_to_proto(dt: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: dt.timestamp(),
        nanos: i32::try_from(dt.timestamp_subsec_nanos()).unwrap_or(0),
    }
}

fn datetime_from_proto(ts: Option<Timestamp>) -> DateTime<Utc> {
    ts.map_or_else(
        || Utc.timestamp_opt(0, 0).single().unwrap_or_default(),
        |t| {
            Utc.timestamp_opt(t.seconds, u32::try_from(t.nanos).unwrap_or(0))
                .single()
                .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap_or_default())
        },
    )
}

// ── NetworkPosture ↔ proto ────────────────────────────────────────────────

const fn posture_to_proto(p: NetworkPosture) -> i32 {
    match p {
        NetworkPosture::Airgap => proto::NetworkPostureProto::Airgap as i32,
        NetworkPosture::LoopbackOnly => proto::NetworkPostureProto::LoopbackOnly as i32,
        NetworkPosture::LanLocal => proto::NetworkPostureProto::LanLocal as i32,
        NetworkPosture::LanExposed => proto::NetworkPostureProto::LanExposed as i32,
        NetworkPosture::Public => proto::NetworkPostureProto::Public as i32,
    }
}

fn posture_from_proto(v: i32) -> Result<NetworkPosture, NetworkPolicyError> {
    let p = proto::NetworkPostureProto::try_from(v).map_err(|_| {
        NetworkPolicyError::Internal(format!("invalid NetworkPostureProto value: {v}"))
    })?;
    match p {
        proto::NetworkPostureProto::Airgap => Ok(NetworkPosture::Airgap),
        proto::NetworkPostureProto::LoopbackOnly => Ok(NetworkPosture::LoopbackOnly),
        proto::NetworkPostureProto::LanLocal => Ok(NetworkPosture::LanLocal),
        proto::NetworkPostureProto::LanExposed => Ok(NetworkPosture::LanExposed),
        proto::NetworkPostureProto::Public => Ok(NetworkPosture::Public),
        proto::NetworkPostureProto::NetworkPostureUnspecified => {
            Err(NetworkPolicyError::Internal("unspecified posture".into()))
        }
    }
}

// ── OutboundDirective ↔ proto ─────────────────────────────────────────────

fn directive_to_proto(d: &OutboundDirective) -> proto::OutboundDirectiveMsg {
    match d {
        OutboundDirective::DenyAll => proto::OutboundDirectiveMsg {
            kind: proto::OutboundDirectiveProto::DenyAll as i32,
            allowlist_id: None,
            tunnel_id: None,
        },
        OutboundDirective::AllowLoopbackOnly => proto::OutboundDirectiveMsg {
            kind: proto::OutboundDirectiveProto::AllowLoopbackOnly as i32,
            ..Default::default()
        },
        OutboundDirective::AllowListOnly { allowlist_id } => proto::OutboundDirectiveMsg {
            kind: proto::OutboundDirectiveProto::AllowListOnly as i32,
            allowlist_id: Some(allowlist_id.clone()),
            ..Default::default()
        },
        OutboundDirective::AllowVpnOnly { tunnel_id } => proto::OutboundDirectiveMsg {
            kind: proto::OutboundDirectiveProto::AllowVpnOnly as i32,
            tunnel_id: Some(tunnel_id.clone()),
            ..Default::default()
        },
        OutboundDirective::AllowInternet => proto::OutboundDirectiveMsg {
            kind: proto::OutboundDirectiveProto::AllowInternet as i32,
            ..Default::default()
        },
    }
}

fn directive_from_proto(
    p: &proto::OutboundDirectiveMsg,
) -> Result<OutboundDirective, NetworkPolicyError> {
    let kind = proto::OutboundDirectiveProto::try_from(p.kind).map_err(|_| {
        NetworkPolicyError::Internal(format!("invalid OutboundDirectiveProto: {}", p.kind))
    })?;
    match kind {
        proto::OutboundDirectiveProto::DenyAll => Ok(OutboundDirective::DenyAll),
        proto::OutboundDirectiveProto::AllowLoopbackOnly => {
            Ok(OutboundDirective::AllowLoopbackOnly)
        }
        proto::OutboundDirectiveProto::AllowListOnly => Ok(OutboundDirective::AllowListOnly {
            allowlist_id: p.allowlist_id.clone().unwrap_or_default(),
        }),
        proto::OutboundDirectiveProto::AllowVpnOnly => Ok(OutboundDirective::AllowVpnOnly {
            tunnel_id: p.tunnel_id.clone().unwrap_or_default(),
        }),
        proto::OutboundDirectiveProto::AllowInternet => Ok(OutboundDirective::AllowInternet),
        proto::OutboundDirectiveProto::OutboundDirectiveUnspecified => {
            Err(NetworkPolicyError::Internal("unspecified directive".into()))
        }
    }
}

// ── ProtocolFamily ↔ proto ───────────────────────────────────────────────

const fn protocol_to_proto(p: ProtocolFamily) -> i32 {
    match p {
        ProtocolFamily::Tcp => proto::ProtocolFamilyProto::Tcp as i32,
        ProtocolFamily::Udp => proto::ProtocolFamilyProto::Udp as i32,
        ProtocolFamily::Icmp => proto::ProtocolFamilyProto::Icmp as i32,
        ProtocolFamily::Quic => proto::ProtocolFamilyProto::Quic as i32,
    }
}

fn protocol_from_proto(v: i32) -> Result<ProtocolFamily, NetworkPolicyError> {
    let p = proto::ProtocolFamilyProto::try_from(v)
        .map_err(|_| NetworkPolicyError::Internal(format!("invalid ProtocolFamilyProto: {v}")))?;
    match p {
        proto::ProtocolFamilyProto::Tcp => Ok(ProtocolFamily::Tcp),
        proto::ProtocolFamilyProto::Udp => Ok(ProtocolFamily::Udp),
        proto::ProtocolFamilyProto::Icmp => Ok(ProtocolFamily::Icmp),
        proto::ProtocolFamilyProto::Quic => Ok(ProtocolFamily::Quic),
        proto::ProtocolFamilyProto::ProtocolFamilyUnspecified => Err(NetworkPolicyError::Internal(
            "unspecified protocol family".into(),
        )),
    }
}

// ── PortPolicy ↔ proto ───────────────────────────────────────────────────

fn port_policy_from_proto(
    kind: i32,
    assigned_port: Option<u32>,
) -> Result<PortPolicy, NetworkPolicyError> {
    let k = proto::PortPolicyProto::try_from(kind)
        .map_err(|_| NetworkPolicyError::Internal(format!("invalid PortPolicyProto: {kind}")))?;
    match k {
        proto::PortPolicyProto::WellKnown => Ok(PortPolicy::WellKnown),
        proto::PortPolicyProto::RegisteredEphemeral => Ok(PortPolicy::RegisteredEphemeral),
        proto::PortPolicyProto::OperatorAssigned => Ok(PortPolicy::OperatorAssigned {
            port: assigned_port.unwrap_or(0) as u16,
        }),
        proto::PortPolicyProto::PortPolicyUnspecified => Err(NetworkPolicyError::Internal(
            "unspecified port policy".into(),
        )),
    }
}

// ── AllowlistEntry ↔ proto ───────────────────────────────────────────────

fn allowlist_entry_from_proto(
    p: &proto::AllowlistEntryProto,
) -> Result<AllowlistEntry, NetworkPolicyError> {
    let kind_p = proto::AllowlistEntryKindProto::try_from(p.kind).map_err(|_| {
        NetworkPolicyError::Internal(format!("invalid AllowlistEntryKindProto: {}", p.kind))
    })?;
    let kind = match kind_p {
        proto::AllowlistEntryKindProto::HostFqdn => AllowlistEntryKind::HostFqdn,
        proto::AllowlistEntryKindProto::Ipv4Address => AllowlistEntryKind::IpV4Address,
        proto::AllowlistEntryKindProto::Ipv6Address => AllowlistEntryKind::IpV6Address,
        proto::AllowlistEntryKindProto::Ipv4Cidr => AllowlistEntryKind::IpV4Cidr,
        proto::AllowlistEntryKindProto::Ipv6Cidr => AllowlistEntryKind::IpV6Cidr,
        proto::AllowlistEntryKindProto::DnsOverTlsResolver => {
            AllowlistEntryKind::DnsOverTlsResolver
        }
        proto::AllowlistEntryKindProto::VpnPeerEndpoint => AllowlistEntryKind::VpnPeerEndpoint,
        proto::AllowlistEntryKindProto::AllowlistEntryKindUnspecified => {
            return Err(NetworkPolicyError::Internal(
                "unspecified allowlist entry kind".into(),
            ));
        }
    };
    Ok(AllowlistEntry {
        kind,
        value: p.value.clone(),
        port_policy: port_policy_from_proto(p.port_policy, p.assigned_port)?,
        protocol: protocol_from_proto(p.protocol)?,
    })
}

// ── OutboundGrant ↔ proto ────────────────────────────────────────────────

fn grant_to_proto(g: &OutboundGrant) -> proto::OutboundGrantProto {
    let allowlist: Vec<proto::AllowlistEntryProto> = g
        .allowlist
        .iter()
        .map(|e| proto::AllowlistEntryProto {
            kind: allowlist_entry_kind_to_proto(e.kind),
            value: e.value.clone(),
            port_policy: port_policy_to_proto(e.port_policy),
            protocol: protocol_to_proto(e.protocol),
            assigned_port: assigned_port_for_policy(e.port_policy),
        })
        .collect();

    let dk = match &g.directive_kind {
        OutboundDirectiveKind::AllowListOnly => {
            proto::OutboundDirectiveKindProto::GrantAllowListOnly as i32
        }
        OutboundDirectiveKind::AllowVpnOnly { .. } => {
            proto::OutboundDirectiveKindProto::GrantAllowVpnOnly as i32
        }
    };

    proto::OutboundGrantProto {
        grant_id: g.grant_id.clone(),
        subject: g.subject.0.clone(),
        allowlist,
        directive_kind: dk,
        issued_at: Some(datetime_to_proto(g.issued_at)),
        expires_at: g.expires_at.map(datetime_to_proto),
        signer_fingerprint: g.signer_fingerprint.clone(),
        signature: g.signature.clone(),
    }
}

const fn allowlist_entry_kind_to_proto(k: AllowlistEntryKind) -> i32 {
    match k {
        AllowlistEntryKind::HostFqdn => proto::AllowlistEntryKindProto::HostFqdn as i32,
        AllowlistEntryKind::IpV4Address => proto::AllowlistEntryKindProto::Ipv4Address as i32,
        AllowlistEntryKind::IpV6Address => proto::AllowlistEntryKindProto::Ipv6Address as i32,
        AllowlistEntryKind::IpV4Cidr => proto::AllowlistEntryKindProto::Ipv4Cidr as i32,
        AllowlistEntryKind::IpV6Cidr => proto::AllowlistEntryKindProto::Ipv6Cidr as i32,
        AllowlistEntryKind::DnsOverTlsResolver => {
            proto::AllowlistEntryKindProto::DnsOverTlsResolver as i32
        }
        AllowlistEntryKind::VpnPeerEndpoint => {
            proto::AllowlistEntryKindProto::VpnPeerEndpoint as i32
        }
    }
}

const fn port_policy_to_proto(p: PortPolicy) -> i32 {
    match p {
        PortPolicy::WellKnown => proto::PortPolicyProto::WellKnown as i32,
        PortPolicy::RegisteredEphemeral => proto::PortPolicyProto::RegisteredEphemeral as i32,
        PortPolicy::OperatorAssigned { .. } => proto::PortPolicyProto::OperatorAssigned as i32,
    }
}

fn assigned_port_for_policy(p: PortPolicy) -> Option<u32> {
    match p {
        PortPolicy::OperatorAssigned { port } => Some(u32::from(port)),
        _ => None,
    }
}

// ── Grant tombstone ↔ proto ──────────────────────────────────────────────

fn tombstone_to_proto(t: &GrantTombstone) -> proto::GrantTombstoneProto {
    proto::GrantTombstoneProto {
        revoked_grant_id: t.revoked_grant_id.clone(),
        revoked_at: Some(datetime_to_proto(t.revoked_at)),
        revoker: t.revoker.0.clone(),
        reason: t.reason.clone(),
    }
}

// ── NetworkOutboundManifest ↔ proto ──────────────────────────────────────

fn manifest_to_proto(m: &NetworkOutboundManifest) -> proto::NetworkOutboundManifestProto {
    proto::NetworkOutboundManifestProto {
        subject: m.subject.0.clone(),
        grants: m.grants.iter().map(grant_to_proto).collect(),
        manifest_id: m.manifest_id.clone(),
        created_at: Some(datetime_to_proto(m.created_at)),
        last_appended_at: Some(datetime_to_proto(m.last_appended_at)),
    }
}

// ── PostureChangeReceipt → proto ─────────────────────────────────────────

fn receipt_to_proto(r: &PostureChangeReceipt) -> proto::PostureChangeReceiptProto {
    proto::PostureChangeReceiptProto {
        from: posture_to_proto(r.from),
        to: posture_to_proto(r.to),
        actor: r.actor.0.clone(),
        at: Some(datetime_to_proto(r.at)),
    }
}

// ── ConnectionDecisionV2 → proto ─────────────────────────────────────────

fn connection_decision_to_proto(d: &ConnectionDecisionV2) -> proto::ConnectionDecisionProto {
    match d {
        ConnectionDecisionV2::Allowed {
            matched_rule_id,
            allowlist_entry_kind,
        } => proto::ConnectionDecisionProto {
            decision: proto::connection_decision_proto::Decision::Allowed as i32,
            matched_rule_id: Some(matched_rule_id.clone()),
            entry_kind: Some(allowlist_entry_kind_to_proto(*allowlist_entry_kind)),
            ..Default::default()
        },
        ConnectionDecisionV2::Denied { code, reason } => proto::ConnectionDecisionProto {
            decision: proto::connection_decision_proto::Decision::Denied as i32,
            error_code: Some(format!("{code:?}")),
            reason: Some(reason.clone()),
            ..Default::default()
        },
    }
}

// ── AiExternalCallDecision → proto ───────────────────────────────────────

fn ai_decision_to_proto(d: &AiExternalCallDecision) -> proto::AiExternalCallDecisionProto {
    match d {
        AiExternalCallDecision::Allowed { via } => {
            let (via_proto, broker_handle, signer_fp, operator, approval_id) = match via {
                AllowedVia::BypassedNonAi => (
                    proto::AllowedViaProto::ViaBypassedNonAi as i32,
                    None,
                    None,
                    None,
                    None,
                ),
                AllowedVia::VaultBroker {
                    broker_handle: bh,
                    signer_fingerprint: sf,
                } => (
                    proto::AllowedViaProto::ViaVaultBroker as i32,
                    Some(bh.clone()),
                    Some(sf.clone()),
                    None,
                    None,
                ),
                AllowedVia::OperatorMediated {
                    operator: op,
                    approval_id: aid,
                } => (
                    proto::AllowedViaProto::ViaOperatorMediated as i32,
                    None,
                    None,
                    Some(op.clone()),
                    Some(aid.clone()),
                ),
            };
            proto::AiExternalCallDecisionProto {
                allowed: true,
                via: via_proto,
                broker_handle,
                signer_fingerprint: signer_fp,
                operator,
                approval_id,
            }
        }
    }
}

pub(crate) fn ai_posture_from_proto(v: i32) -> Result<AICrossOriginPosture, NetworkPolicyError> {
    let p = proto::AiCrossOriginPostureProto::try_from(v).map_err(|_| {
        NetworkPolicyError::Internal(format!("invalid AiCrossOriginPostureProto: {v}"))
    })?;
    match p {
        proto::AiCrossOriginPostureProto::AiDenyAllExternal => {
            Ok(AICrossOriginPosture::DenyAllExternal)
        }
        proto::AiCrossOriginPostureProto::AiVaultBrokeredOnly => {
            Ok(AICrossOriginPosture::VaultBrokeredOnly {
                broker_handle: String::new(),
            })
        }
        proto::AiCrossOriginPostureProto::AiOperatorMediated => {
            Ok(AICrossOriginPosture::OperatorMediated {
                operator_canonical_id: String::new(),
            })
        }
        proto::AiCrossOriginPostureProto::AiCrossOriginPostureUnspecified => Err(
            NetworkPolicyError::Internal("unspecified AI cross-origin posture".into()),
        ),
    }
}

// ── InboundExposureClass from proto ──────────────────────────────────────

fn inbound_exposure_from_proto(v: i32) -> Result<InboundExposureClass, NetworkPolicyError> {
    let p = proto::InboundExposureClassProto::try_from(v).map_err(|_| {
        NetworkPolicyError::Internal(format!("invalid InboundExposureClassProto: {v}"))
    })?;
    match p {
        proto::InboundExposureClassProto::ExposureLoopback => Ok(InboundExposureClass::Loopback),
        proto::InboundExposureClassProto::ExposureLan => Ok(InboundExposureClass::Lan),
        proto::InboundExposureClassProto::ExposurePublic => Ok(InboundExposureClass::Public),
        proto::InboundExposureClassProto::InboundExposureClassUnspecified => Err(
            NetworkPolicyError::Internal("unspecified exposure class".into()),
        ),
    }
}

// ── FirewallRuleset ↔ proto ─────────────────────────────────────────────

pub(crate) fn firewall_ruleset_from_proto(
    p: &proto::FirewallRulesetProto,
) -> Result<FirewallRuleset, NetworkPolicyError> {
    let backend = firewall_backend_from_proto(p.backend)?;
    let rules: Result<Vec<FirewallRule>, NetworkPolicyError> =
        p.rules.iter().map(firewall_rule_from_proto).collect();
    Ok(FirewallRuleset {
        backend,
        rules: rules?,
        generation: p.generation,
        built_at: datetime_from_proto(p.built_at),
    })
}

fn firewall_backend_from_proto(v: i32) -> Result<FirewallBackend, NetworkPolicyError> {
    let p = proto::FirewallBackendProto::try_from(v)
        .map_err(|_| NetworkPolicyError::Internal(format!("invalid FirewallBackendProto: {v}")))?;
    match p {
        proto::FirewallBackendProto::Nftables => Ok(FirewallBackend::Nftables),
        proto::FirewallBackendProto::IptablesFallback => Ok(FirewallBackend::IptablesFallback),
        proto::FirewallBackendProto::FirewallBackendUnspecified => Err(
            NetworkPolicyError::Internal("unspecified firewall backend".into()),
        ),
    }
}

fn firewall_chain_from_proto(v: i32) -> Result<FirewallChain, NetworkPolicyError> {
    let p = proto::FirewallChainProto::try_from(v)
        .map_err(|_| NetworkPolicyError::Internal(format!("invalid FirewallChainProto: {v}")))?;
    match p {
        proto::FirewallChainProto::FwInput => Ok(FirewallChain::Input),
        proto::FirewallChainProto::FwOutput => Ok(FirewallChain::Output),
        proto::FirewallChainProto::FwForward => Ok(FirewallChain::Forward),
        proto::FirewallChainProto::FwPrerouting => Ok(FirewallChain::Prerouting),
        proto::FirewallChainProto::FwPostrouting => Ok(FirewallChain::Postrouting),
        proto::FirewallChainProto::FirewallChainUnspecified => Err(NetworkPolicyError::Internal(
            "unspecified firewall chain".into(),
        )),
    }
}

fn firewall_action_from_proto(v: i32) -> Result<FirewallAction, NetworkPolicyError> {
    let p = proto::FirewallActionProto::try_from(v)
        .map_err(|_| NetworkPolicyError::Internal(format!("invalid FirewallActionProto: {v}")))?;
    match p {
        proto::FirewallActionProto::FwAccept => Ok(FirewallAction::Accept),
        proto::FirewallActionProto::FwDrop => Ok(FirewallAction::Drop),
        proto::FirewallActionProto::FwReject => Ok(FirewallAction::Reject),
        proto::FirewallActionProto::FwLog => Ok(FirewallAction::Log),
        proto::FirewallActionProto::FwReturn => Ok(FirewallAction::Return),
        proto::FirewallActionProto::FirewallActionUnspecified => Err(NetworkPolicyError::Internal(
            "unspecified firewall action".into(),
        )),
    }
}

fn firewall_rule_from_proto(
    p: &proto::FirewallRuleProto,
) -> Result<FirewallRule, NetworkPolicyError> {
    let match_expr = firewall_match_from_proto(
        p.match_kind,
        &p.match_value,
        p.match_port,
        p.match_protocol.unwrap_or(0),
    )?;
    Ok(FirewallRule {
        rule_id: p.rule_id.clone(),
        chain: firewall_chain_from_proto(p.chain)?,
        priority: p.priority,
        match_expr,
        action: firewall_action_from_proto(p.action)?,
        comment: p.comment.clone(),
    })
}

fn firewall_match_from_proto(
    kind: i32,
    value: &str,
    port: Option<u32>,
    proto_val: i32,
) -> Result<FirewallMatch, NetworkPolicyError> {
    let p = proto::FirewallMatchProto::try_from(kind)
        .map_err(|_| NetworkPolicyError::Internal(format!("invalid FirewallMatchProto: {kind}")))?;
    match p {
        proto::FirewallMatchProto::FwSourceIp => {
            let ip: IpAddr = value
                .parse()
                .map_err(|_| NetworkPolicyError::Internal(format!("invalid IP: {value}")))?;
            Ok(FirewallMatch::SourceIp(ip))
        }
        proto::FirewallMatchProto::FwSourceCidr => Ok(FirewallMatch::SourceCidr(value.to_string())),
        proto::FirewallMatchProto::FwDestIp => {
            let ip: IpAddr = value
                .parse()
                .map_err(|_| NetworkPolicyError::Internal(format!("invalid IP: {value}")))?;
            Ok(FirewallMatch::DestIp(ip))
        }
        proto::FirewallMatchProto::FwDestCidr => Ok(FirewallMatch::DestCidr(value.to_string())),
        proto::FirewallMatchProto::FwDestPort => Ok(FirewallMatch::DestPort {
            port: port.unwrap_or(0) as u16,
            protocol: protocol_from_proto(proto_val)?,
        }),
        proto::FirewallMatchProto::FwInterface => Ok(FirewallMatch::Interface(value.to_string())),
        proto::FirewallMatchProto::FwCtState => Ok(FirewallMatch::CtState(value.to_string())),
        proto::FirewallMatchProto::FwAll => Ok(FirewallMatch::All),
        proto::FirewallMatchProto::FirewallMatchUnspecified => Err(NetworkPolicyError::Internal(
            "unspecified firewall match".into(),
        )),
    }
}

// ── ResolverAllowlist ↔ proto ────────────────────────────────────────────

pub fn resolver_allowlist_from_proto(
    p: &proto::ResolverAllowlistProto,
) -> Result<ResolverAllowlist, NetworkPolicyError> {
    let endpoints: Result<Vec<ResolverEndpoint>, NetworkPolicyError> = p
        .endpoints
        .iter()
        .map(resolver_endpoint_from_proto)
        .collect();
    Ok(ResolverAllowlist {
        list_id: p.list_id.clone(),
        endpoints: endpoints?,
        signed_at: datetime_from_proto(p.signed_at),
        signer_fingerprint: p.signer_fingerprint.clone(),
        signature: p.signature.clone(),
    })
}

fn resolver_endpoint_from_proto(
    p: &proto::ResolverEndpointProto,
) -> Result<ResolverEndpoint, NetworkPolicyError> {
    let address: IpAddr = p.address.parse().map_err(|_| {
        NetworkPolicyError::Internal(format!("invalid resolver address: {}", p.address))
    })?;
    let transport = match p.transport.as_str() {
        "DNS_OVER_TLS" => DnsTransport::DnsOverTls,
        "DNS_OVER_HTTPS" => DnsTransport::DnsOverHttps,
        "DNS_OVER_QUIC" => DnsTransport::DnsOverQuic,
        "PLAIN_DNS_FORBIDDEN" => DnsTransport::PlainDnsForbidden,
        other => {
            return Err(NetworkPolicyError::Internal(format!(
                "unknown transport: {other}"
            )))
        }
    };
    Ok(ResolverEndpoint {
        fqdn: p.fqdn.clone(),
        address,
        port: p.port as u16,
        transport,
        spki_pin: p.spki_pin.clone(),
    })
}

pub(crate) fn resolver_profile_to_proto(rp: &ResolverProfile) -> proto::ResolverProfileProto {
    let backend_str = match rp.backend {
        ResolverBackend::SystemdResolved => "SYSTEMD_RESOLVED",
        ResolverBackend::Unbound => "UNBOUND",
        ResolverBackend::Bind9 => "BIND9",
        ResolverBackend::Dnsmasq => "DNSMASQ",
        ResolverBackend::DegradedHostsFileOnly => "DEGRADED_HOSTS_FILE_ONLY",
    };
    proto::ResolverProfileProto {
        backend: backend_str.to_string(),
        active_list_id: rp.active_list_id.clone(),
        effective_endpoints: rp
            .effective_endpoints
            .iter()
            .map(|ep| proto::ResolverEndpointProto {
                fqdn: ep.fqdn.clone(),
                address: ep.address.to_string(),
                port: u32::from(ep.port),
                transport: format!("{transport:?}", transport = ep.transport).to_uppercase(),
                spki_pin: ep.spki_pin.clone(),
            })
            .collect(),
        cache_ttl_seconds: rp.cache_ttl_seconds,
    }
}

// ── VPN conversions ──────────────────────────────────────────────────────

pub fn wireguard_config_from_proto(
    p: &proto::ProposeVpnTunnelRequest,
) -> Result<(WireGuardConfig, String), NetworkPolicyError> {
    let kind = match p.kind.as_str() {
        "WIREGUARD_SPLIT_TUNNEL" => VpnTunnelKind::WireGuardSplitTunnel,
        "WIREGUARD_FULL_TUNNEL" => VpnTunnelKind::WireGuardFullTunnel,
        "RECOVERY_DISABLED" => VpnTunnelKind::RecoveryDisabled,
        "OPERATOR_DEFINED_OTHER_BLACKLISTED" => VpnTunnelKind::OperatorDefinedOtherBlacklisted,
        other => {
            return Err(NetworkPolicyError::Internal(format!(
                "unknown tunnel kind: {other}"
            )))
        }
    };

    let local_pk: [u8; 32] =
        p.local_public_key.as_slice().try_into().map_err(|_| {
            NetworkPolicyError::Internal("local public key must be 32 bytes".into())
        })?;

    let peers: Result<Vec<WireGuardPeer>, NetworkPolicyError> = p
        .peers
        .iter()
        .map(|peer_proto| {
            let pk: [u8; 32] = peer_proto.public_key.as_slice().try_into().map_err(|_| {
                NetworkPolicyError::Internal("peer public key must be 32 bytes".into())
            })?;
            Ok(WireGuardPeer {
                peer_id: peer_proto.peer_id.clone(),
                endpoint: peer_proto.endpoint.clone(),
                public_key: pk,
                allowed_ips: peer_proto.allowed_ips.clone(),
                persistent_keepalive_seconds: peer_proto.persistent_keepalive_seconds,
            })
        })
        .collect();

    let config = WireGuardConfig {
        tunnel_id: p.tunnel_id.clone(),
        kind,
        interface_name: p.interface_name.clone(),
        local_private_key_handle: p.local_private_key_handle.clone(),
        local_public_key: local_pk,
        peers: peers?,
        mtu: p.mtu,
        fwmark: p.fwmark,
    };

    Ok((config, p.requester.clone()))
}

pub fn peer_key_rotation_from_proto(
    p: &proto::PeerKeyRotationProto,
) -> Result<PeerKeyRotation, NetworkPolicyError> {
    let old_pk: [u8; 32] = p
        .old_pubkey
        .as_slice()
        .try_into()
        .map_err(|_| NetworkPolicyError::Internal("old pubkey must be 32 bytes".into()))?;
    let new_pk: [u8; 32] = p
        .new_pubkey
        .as_slice()
        .try_into()
        .map_err(|_| NetworkPolicyError::Internal("new pubkey must be 32 bytes".into()))?;
    Ok(PeerKeyRotation {
        tunnel_id: p.tunnel_id.clone(),
        old_pubkey: old_pk,
        new_pubkey: new_pk,
        rotated_at: datetime_from_proto(p.rotated_at),
        authority_fingerprint: p.authority_fingerprint.clone(),
        signature: p.signature.clone(),
    })
}

pub(crate) fn vpn_tunnel_entry_to_proto(
    id: &str,
    label: TunnelLifecycleLabel,
) -> proto::VpnTunnelEntry {
    let label_str = match label {
        TunnelLifecycleLabel::Proposed => "Proposed",
        TunnelLifecycleLabel::Approved => "Approved",
        TunnelLifecycleLabel::Active => "Active",
        TunnelLifecycleLabel::Failed => "Failed",
        TunnelLifecycleLabel::Revoked => "Revoked",
    };
    proto::VpnTunnelEntry {
        tunnel_id: id.to_string(),
        lifecycle_label: label_str.to_string(),
    }
}

// ── mDNS conversions ─────────────────────────────────────────────────────

pub fn mdns_posture_from_str(
    s: &str,
    allowlist_id: Option<&str>,
) -> Result<MdnsAvahiPosture, NetworkPolicyError> {
    match s {
        "DENY_DEFAULT" => Ok(MdnsAvahiPosture::DenyDefault),
        "RECOVERY_DENIED" => Ok(MdnsAvahiPosture::RecoveryDenied),
        "AIRGAP_DENIED" => Ok(MdnsAvahiPosture::AirgapDenied),
        "OPERATOR_AUTHORISED" => Ok(MdnsAvahiPosture::OperatorAuthorised {
            allowlist_id: allowlist_id.unwrap_or_default().to_string(),
        }),
        other => Err(NetworkPolicyError::Internal(format!(
            "unknown mDNS posture: {other}"
        ))),
    }
}

pub fn mdns_allowlist_from_proto(
    p: &proto::MdnsAdvertisementAllowlistProto,
) -> Result<MdnsAdvertisementAllowlist, NetworkPolicyError> {
    let ads: Vec<MdnsAdvertisement> = p
        .advertisements
        .iter()
        .map(|a| MdnsAdvertisement {
            advertisement_id: a.advertisement_id.clone(),
            service_type: a.service_type.clone(),
            instance_name: a.instance_name.clone(),
            port: a.port as u16,
            authorised_at: datetime_from_proto(a.authorised_at),
            authoriser: SubjectId(a.authoriser.clone()),
        })
        .collect();
    Ok(MdnsAdvertisementAllowlist {
        allowlist_id: p.allowlist_id.clone(),
        advertisements: ads,
        signed_at: datetime_from_proto(p.signed_at),
        signer_fingerprint: p.signer_fingerprint.clone(),
        signature: p.signature.clone(),
    })
}

// ── Exposure FSM helpers ─────────────────────────────────────────────────

pub fn exposure_request_to_fsm_call(
    class: &InboundExposureClass,
    requester: &SubjectId,
    recovery_session_id: Option<&str>,
) -> Result<ExposureTransitionReason, NetworkPolicyError> {
    match class {
        InboundExposureClass::Loopback | InboundExposureClass::Lan => {
            Ok(ExposureTransitionReason::LanRequest {
                requester: requester.clone(),
            })
        }
        InboundExposureClass::Public => {
            let rsid = recovery_session_id.unwrap_or_default();
            if rsid.is_empty() {
                return Err(NetworkPolicyError::ExposureEscalationDenied {
                    from: "Loopback".into(),
                    to: "Public".into(),
                    reason: "recovery_session_id required".into(),
                });
            }
            Ok(ExposureTransitionReason::PublicRequest {
                requester: requester.clone(),
                recovery_session_id: rsid.to_string(),
            })
        }
    }
}

// ── network_error_to_status ──────────────────────────────────────────────

/// Map a [`NetworkPolicyError`] to a [`tonic::Status`] for gRPC responses.
#[must_use]
pub fn network_error_to_status(err: &NetworkPolicyError) -> Status {
    match err {
        NetworkPolicyError::DefaultDeny(msg) => {
            Status::failed_precondition(format!("default deny: {msg}"))
        }
        NetworkPolicyError::CrossGroupAccessForbidden {
            source_group,
            dest_group,
        } => Status::permission_denied(format!(
            "cross-group access forbidden: {source_group} -> {dest_group}"
        )),
        NetworkPolicyError::AiDirectInternetDenied {
            subject,
            attempted_endpoint,
        } => Status::permission_denied(format!(
            "AI direct internet denied: {subject} attempted {attempted_endpoint}"
        )),
        NetworkPolicyError::AllowlistFqdnFanoutExceeded {
            fqdn,
            resolved_count,
        } => Status::resource_exhausted(format!(
            "allowlist FQDN fan-out exceeded: {fqdn} resolved to {resolved_count} addresses"
        )),
        NetworkPolicyError::ExposureEscalationDenied { from, to, reason } => {
            Status::failed_precondition(format!(
                "exposure escalation denied: from {from} to {to}: {reason}"
            ))
        }
        NetworkPolicyError::GrantSignatureInvalid { grant_id, reason } => {
            Status::permission_denied(format!("grant signature invalid: {grant_id}: {reason}"))
        }
        NetworkPolicyError::RawSocketBypassAttempted(subject) => {
            Status::permission_denied(format!("raw socket bypass attempted by {subject}"))
        }
        NetworkPolicyError::ManifestMutationForbidden(msg) => {
            Status::failed_precondition(format!("manifest mutation forbidden: {msg}"))
        }
        NetworkPolicyError::ResolverSignatureInvalid(msg) => {
            Status::permission_denied(format!("resolver signature invalid: {msg}"))
        }
        NetworkPolicyError::VpnPeerKeySignatureInvalid(msg) => {
            Status::permission_denied(format!("VPN peer key signature invalid: {msg}"))
        }
        NetworkPolicyError::PlainDnsForbidden(msg) => {
            Status::failed_precondition(format!("plain DNS forbidden: {msg}"))
        }
        NetworkPolicyError::MdnsAdvertisementDenied(msg) => {
            Status::permission_denied(format!("mDNS advertisement denied: {msg}"))
        }
        NetworkPolicyError::Internal(msg) => {
            Status::internal(format!("internal network policy error: {msg}"))
        }
    }
}

// ── Public conversion entry points ───────────────────────────────────────
// These are the functions called from server.rs. Internal helpers above
// are intentionally module-private.

#[allow(dead_code)]
pub(crate) fn posture_from_req(
    req: &proto::SetPostureRequest,
) -> Result<(NetworkPosture, SubjectId), NetworkPolicyError> {
    let posture = posture_from_proto(req.new_posture)?;
    Ok((posture, SubjectId(req.actor.clone())))
}

#[allow(dead_code)]
#[allow(clippy::missing_const_for_fn)]
pub(crate) fn get_posture_to_resp(posture: NetworkPosture) -> proto::GetPostureResponse {
    proto::GetPostureResponse {
        posture: posture_to_proto(posture),
    }
}

#[allow(dead_code)]
pub(crate) fn receipt_to_resp(r: &PostureChangeReceipt) -> proto::PostureChangeReceiptProto {
    receipt_to_proto(r)
}

#[allow(dead_code)]
pub(crate) fn subject_directive_to_resp(
    d: &OutboundDirective,
) -> proto::GetSubjectDirectiveResponse {
    proto::GetSubjectDirectiveResponse {
        directive: Some(directive_to_proto(d)),
    }
}

#[allow(dead_code)]
pub(crate) fn set_directive_from_req(
    req: &proto::SetSubjectDirectiveRequest,
) -> Result<(SubjectId, OutboundDirective, SubjectId), NetworkPolicyError> {
    let directive = req.directive.as_ref().map_or_else(
        || {
            Err(NetworkPolicyError::Internal(
                "directive field required".into(),
            ))
        },
        directive_from_proto,
    )?;
    Ok((
        SubjectId(req.subject.clone()),
        directive,
        SubjectId(req.actor.clone()),
    ))
}

#[allow(dead_code)]
pub(crate) fn list_directives_to_resp(
    entries: &[(SubjectId, OutboundDirective)],
) -> proto::ListDirectivesResponse {
    proto::ListDirectivesResponse {
        entries: entries
            .iter()
            .map(|(subj, dir)| proto::DirectiveEntry {
                subject: subj.0.clone(),
                directive: Some(directive_to_proto(dir)),
            })
            .collect(),
    }
}

#[allow(dead_code)]
pub(crate) fn grant_from_req(
    req: &proto::OutboundGrantProto,
) -> Result<OutboundGrant, NetworkPolicyError> {
    let allowlist: Result<Vec<AllowlistEntry>, NetworkPolicyError> = req
        .allowlist
        .iter()
        .map(allowlist_entry_from_proto)
        .collect();
    let directive_kind = match proto::OutboundDirectiveKindProto::try_from(req.directive_kind) {
        Ok(proto::OutboundDirectiveKindProto::GrantAllowListOnly) => {
            OutboundDirectiveKind::AllowListOnly
        }
        Ok(proto::OutboundDirectiveKindProto::GrantAllowVpnOnly) => {
            OutboundDirectiveKind::AllowVpnOnly {
                tunnel_id: String::new(),
            }
        }
        _ => {
            return Err(NetworkPolicyError::Internal(
                "invalid directive kind".into(),
            ))
        }
    };
    Ok(OutboundGrant {
        grant_id: req.grant_id.clone(),
        subject: SubjectId(req.subject.clone()),
        allowlist: allowlist?,
        directive_kind,
        issued_at: datetime_from_proto(req.issued_at),
        expires_at: req.expires_at.map(|ts| datetime_from_proto(Some(ts))),
        signer_fingerprint: req.signer_fingerprint.clone(),
        signature: req.signature.clone(),
    })
}

#[allow(dead_code)]
pub(crate) fn tombstone_to_resp(t: &GrantTombstone) -> proto::GrantTombstoneProto {
    tombstone_to_proto(t)
}

#[allow(dead_code)]
pub(crate) fn manifest_to_resp(m: &NetworkOutboundManifest) -> proto::NetworkOutboundManifestProto {
    manifest_to_proto(m)
}

#[allow(dead_code)]
pub(crate) fn eval_req_from_proto(
    p: &proto::EvaluateConnectionRequest,
) -> Result<EvaluateConnectionRequestV2, NetworkPolicyError> {
    let dest_group = p
        .destination_group_hint
        .as_ref()
        .map(|g| GroupId(g.clone()));
    Ok(EvaluateConnectionRequestV2 {
        subject: SubjectId(p.subject.clone()),
        destination_host: p.destination_host.clone(),
        destination_port: p.destination_port as u16,
        protocol: protocol_from_proto(p.protocol)?,
        destination_group_hint: dest_group,
    })
}

#[allow(dead_code)]
pub(crate) fn eval_decision_to_resp(d: &ConnectionDecisionV2) -> proto::ConnectionDecisionProto {
    connection_decision_to_proto(d)
}

#[allow(dead_code)]
pub(crate) fn ai_eval_req_from_proto(
    p: &proto::EvaluateAiExternalCallRequest,
) -> AiExternalCallRequest {
    AiExternalCallRequest {
        subject: SubjectId(p.subject.clone()),
        endpoint: p.endpoint.clone(),
        broker_handle: p.broker_handle.clone(),
        operator_approval_id: p.operator_approval_id.clone(),
    }
}

#[allow(dead_code)]
pub(crate) fn ai_decision_to_resp(
    d: &AiExternalCallDecision,
) -> proto::AiExternalCallDecisionProto {
    ai_decision_to_proto(d)
}

#[allow(dead_code)]
pub(crate) fn inbound_exposure_from_req(
    req: &proto::RequestExposureRequest,
) -> Result<(InboundExposureClass, SubjectId, Option<String>), NetworkPolicyError> {
    let class = inbound_exposure_from_proto(req.class)?;
    Ok((
        class,
        SubjectId(req.requester.clone()),
        req.recovery_session_id.clone(),
    ))
}

#[allow(dead_code)]
pub(crate) fn dir_from_str(s: &str) -> Result<OutboundDirective, NetworkPolicyError> {
    match s {
        "DENY_ALL" => Ok(OutboundDirective::DenyAll),
        "ALLOW_LOOPBACK_ONLY" => Ok(OutboundDirective::AllowLoopbackOnly),
        "ALLOW_INTERNET" => Ok(OutboundDirective::AllowInternet),
        other => Err(NetworkPolicyError::Internal(format!(
            "unknown directive: {other}"
        ))),
    }
}
