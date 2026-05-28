#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_network::{
    AICrossOriginPosture, AllowlistEntry, AllowlistEntryKind, GroupId, InboundExposureClass,
    NetworkPolicyError, NetworkPolicyErrorCode, NetworkPosture, OutboundDirective, PortPolicy,
    ProtocolFamily, SubjectId, DEFAULT_CODE_VERSION,
};

#[test]
fn default_code_version_constant_is_correct() {
    assert_eq!(DEFAULT_CODE_VERSION, "aios-network/0.1.0-T162");
}

#[test]
fn network_posture_has_5_variants() {
    use strum::IntoEnumIterator;
    assert_eq!(NetworkPosture::iter().count(), 5);
}

#[test]
fn network_posture_label_for_airgap_is_airgap() {
    assert_eq!(NetworkPosture::Airgap.label(), "airgap");
}

#[test]
fn outbound_directive_has_5_variants_at_least() {
    use strum::IntoEnumIterator;
    assert!(OutboundDirective::iter().count() >= 5);
}

#[test]
fn outbound_directive_deny_all_default() {
    let directive = OutboundDirective::DenyAll;
    let json = serde_json::to_string(&directive).unwrap();
    assert!(json.contains("DENY_ALL"));
}

#[test]
fn inbound_exposure_class_loopback_lan_public() {
    let loopback = serde_json::to_string(&InboundExposureClass::Loopback).unwrap();
    assert!(loopback.contains("LOOPBACK"));

    let lan = serde_json::to_string(&InboundExposureClass::Lan).unwrap();
    assert!(lan.contains("LAN"));

    let public = serde_json::to_string(&InboundExposureClass::Public).unwrap();
    assert!(public.contains("PUBLIC"));
}

#[test]
fn port_policy_operator_assigned_round_trip() {
    let policy = PortPolicy::OperatorAssigned { port: 8443 };
    let json = serde_json::to_string(&policy).unwrap();
    let back: PortPolicy = serde_json::from_str(&json).unwrap();
    assert_eq!(policy, back);
}

#[test]
fn protocol_family_has_tcp_udp_icmp_quic() {
    use strum::IntoEnumIterator;
    let variants: Vec<_> = ProtocolFamily::iter().collect();
    assert!(variants.contains(&ProtocolFamily::Tcp));
    assert!(variants.contains(&ProtocolFamily::Udp));
    assert!(variants.contains(&ProtocolFamily::Icmp));
    assert!(variants.contains(&ProtocolFamily::Quic));
}

#[test]
fn allowlist_entry_kind_has_7_variants() {
    use strum::IntoEnumIterator;
    assert_eq!(AllowlistEntryKind::iter().count(), 7);
}

#[test]
fn allowlist_entry_serde_round_trip() {
    let entry = AllowlistEntry {
        kind: AllowlistEntryKind::HostFqdn,
        value: "example.com".into(),
        port_policy: PortPolicy::OperatorAssigned { port: 443 },
        protocol: ProtocolFamily::Tcp,
    };
    let json = serde_json::to_string(&entry).unwrap();
    let back: AllowlistEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(entry, back);
}

#[test]
fn ai_cross_origin_posture_has_3_variants() {
    use strum::IntoEnumIterator;
    assert_eq!(AICrossOriginPosture::iter().count(), 3);
}

#[test]
fn ai_cross_origin_posture_deny_all_external_carries_no_handle() {
    let posture = AICrossOriginPosture::DenyAllExternal;
    let json = serde_json::to_string(&posture).unwrap();
    assert!(json.contains("DENY_ALL_EXTERNAL"));
    assert!(!json.contains("broker_handle"));
    assert!(!json.contains("operator_canonical_id"));
}

#[test]
fn subject_id_and_group_id_serde_round_trip() {
    let sid = SubjectId("human:lucky".into());
    let json = serde_json::to_string(&sid).unwrap();
    let back: SubjectId = serde_json::from_str(&json).unwrap();
    assert_eq!(sid, back);

    let gid = GroupId("group:operators".into());
    let json = serde_json::to_string(&gid).unwrap();
    let back: GroupId = serde_json::from_str(&json).unwrap();
    assert_eq!(gid, back);
}

#[test]
fn network_policy_error_code_has_at_least_13_variants() {
    // 13 closed codes covering INVs I1, I3, I4, I7, I8, I9, I10, I11, I12
    // plus S8.4 resolver/VPN/DNS/mDNS error codes.
    let codes: &[NetworkPolicyErrorCode] = &[
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
    assert_eq!(codes.len(), 13);
}

#[test]
fn network_policy_error_default_deny_code_matches() {
    let err = NetworkPolicyError::DefaultDeny("test".into());
    assert_eq!(err.code(), NetworkPolicyErrorCode::DefaultDeny);
}

#[test]
fn network_policy_error_cross_group_access_forbidden_code_matches() {
    let err = NetworkPolicyError::CrossGroupAccessForbidden {
        source_group: GroupId("src".into()),
        dest_group: GroupId("dst".into()),
    };
    assert_eq!(
        err.code(),
        NetworkPolicyErrorCode::CrossGroupAccessForbidden
    );
}

#[test]
fn network_policy_error_ai_direct_internet_denied_code_matches() {
    let err = NetworkPolicyError::AiDirectInternetDenied {
        subject: SubjectId("agent:test".into()),
        attempted_endpoint: "https://evil.com".into(),
    };
    assert_eq!(err.code(), NetworkPolicyErrorCode::AiDirectInternetDenied);
}

#[test]
fn network_policy_error_display_round_trip_all_variants_non_empty() {
    let subject = SubjectId("subj".into());
    let ga = GroupId("ga".into());
    let gb = GroupId("gb".into());

    let variants: &[NetworkPolicyError] = &[
        NetworkPolicyError::DefaultDeny("test".into()),
        NetworkPolicyError::CrossGroupAccessForbidden {
            source_group: ga,
            dest_group: gb,
        },
        NetworkPolicyError::AiDirectInternetDenied {
            subject,
            attempted_endpoint: "https://evil.com".into(),
        },
        NetworkPolicyError::AllowlistFqdnFanoutExceeded {
            fqdn: "example.com".into(),
            resolved_count: 42,
        },
        NetworkPolicyError::ExposureEscalationDenied {
            from: "Loopback".into(),
            to: "Public".into(),
            reason: "policy".into(),
        },
        NetworkPolicyError::GrantSignatureInvalid {
            grant_id: "g1".into(),
            reason: "bad sig".into(),
        },
        NetworkPolicyError::RawSocketBypassAttempted(SubjectId("agent:x".into())),
        NetworkPolicyError::ManifestMutationForbidden("mutated".into()),
        NetworkPolicyError::ResolverSignatureInvalid("bad-resolver".into()),
        NetworkPolicyError::VpnPeerKeySignatureInvalid("bad-peer".into()),
        NetworkPolicyError::PlainDnsForbidden("dns".into()),
        NetworkPolicyError::MdnsAdvertisementDenied("mdns".into()),
        NetworkPolicyError::Internal("boom".into()),
    ];

    for err in variants {
        let msg = format!("{err}");
        assert!(!msg.is_empty(), "empty Display for {err:?}");
    }
}
