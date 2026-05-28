#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::net::{IpAddr, Ipv4Addr};

use aios_network::{
    FirewallAction, FirewallBackend, FirewallChain, FirewallManager, FirewallMatch, FirewallRule,
    FirewallRuleset, FirewallRulesetBuilder, MdnsAdvertisement, MdnsAdvertisementAllowlist,
    MdnsAvahiPosture, MdnsGate, NetworkPolicyError, NetworkPolicyErrorCode, OutboundDirective,
    ProtocolFamily, SubjectId,
};
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;
use strum::{EnumCount, IntoEnumIterator};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        write!(s, "{b:02x}").expect("write to String is infallible");
    }
    s
}

fn make_authority() -> (SigningKey, String) {
    let sk = SigningKey::generate(&mut OsRng);
    let vk = sk.verifying_key();
    let fingerprint = bytes_to_hex(vk.as_bytes());
    (sk, fingerprint)
}

fn sign_allowlist(
    sk: &SigningKey,
    allowlist_id: &str,
    advertisements: &[MdnsAdvertisement],
    fingerprint: &str,
) -> MdnsAdvertisementAllowlist {
    let signed_at = Utc::now();
    // Build the same payload MdnsAdvertisementAllowlist::signing_payload() produces.
    let mut payload = Vec::new();
    payload.extend_from_slice(allowlist_id.as_bytes());
    for ad in advertisements {
        payload.extend_from_slice(ad.advertisement_id.as_bytes());
        payload.extend_from_slice(ad.service_type.as_bytes());
        payload.extend_from_slice(ad.instance_name.as_bytes());
        payload.extend_from_slice(&ad.port.to_le_bytes());
    }
    payload.extend_from_slice(signed_at.to_rfc3339().as_bytes());
    let sig = sk.sign(&payload);
    MdnsAdvertisementAllowlist {
        allowlist_id: allowlist_id.to_owned(),
        advertisements: advertisements.to_vec(),
        signed_at,
        signer_fingerprint: fingerprint.to_owned(),
        signature: sig.to_bytes().to_vec(),
    }
}

fn make_ad(id: &str, service_type: &str, instance_name: &str, port: u16) -> MdnsAdvertisement {
    MdnsAdvertisement {
        advertisement_id: id.to_owned(),
        service_type: service_type.to_owned(),
        instance_name: instance_name.to_owned(),
        port,
        authorised_at: Utc::now(),
        authoriser: SubjectId("human:ops".into()),
    }
}

fn make_rule(
    id: &str,
    chain: FirewallChain,
    match_expr: FirewallMatch,
    action: FirewallAction,
) -> FirewallRule {
    FirewallRule {
        rule_id: id.to_owned(),
        chain,
        priority: 100,
        match_expr,
        action,
        comment: "test rule".into(),
    }
}

fn make_basic_ruleset() -> FirewallRuleset {
    FirewallRulesetBuilder::new(FirewallBackend::Nftables)
        .rule(make_rule(
            "r1",
            FirewallChain::Input,
            FirewallMatch::Interface("lo".into()),
            FirewallAction::Accept,
        ))
        .build()
}

// ---------------------------------------------------------------------------
// mDNS tests
// ---------------------------------------------------------------------------

#[test]
fn mdns_posture_default_is_deny_default() {
    let gate = MdnsGate::new();
    // We can't directly read the private field, but the behaviour confirms it:
    // check_advertisement with DenyDefault returns MdnsAdvertisementDenied("default-deny").
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(gate.check_advertisement("_http._tcp", "printer", 8080));
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), NetworkPolicyErrorCode::MdnsAdvertisementDenied);
    let msg = format!("{err}");
    assert!(msg.contains("default-deny"));
}

#[test]
fn mdns_check_advertisement_deny_default_returns_mdns_advertisement_denied() {
    let gate = MdnsGate::new();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(gate.check_advertisement("_http._tcp", "My Service", 8080));
    assert!(result.is_err());
    match result.unwrap_err() {
        NetworkPolicyError::MdnsAdvertisementDenied(reason) => {
            assert_eq!(reason, "default-deny");
        }
        other => panic!("expected MdnsAdvertisementDenied, got {other:?}"),
    }
}

#[test]
fn mdns_set_posture_to_recovery_denied_blocks_even_with_allowlist() {
    let gate = MdnsGate::new();
    let rt = tokio::runtime::Runtime::new().unwrap();

    let (sk, fingerprint) = make_authority();
    rt.block_on(gate.register_authority(&fingerprint, sk.verifying_key()));

    let ad = make_ad("ad-1", "_http._tcp", "printer", 8080);
    let allowlist = sign_allowlist(&sk, "al-1", &[ad], &fingerprint);
    rt.block_on(gate.admit_allowlist(allowlist)).unwrap();

    // Set posture to OperatorAuthorised so we know the allowlist works.
    rt.block_on(gate.set_posture(MdnsAvahiPosture::OperatorAuthorised {
        allowlist_id: "al-1".into(),
    }))
    .unwrap();
    assert!(rt
        .block_on(gate.check_advertisement("_http._tcp", "printer", 8080))
        .is_ok());

    // Switch to RecoveryDenied — must hard-deny even though allowlist is registered.
    rt.block_on(gate.set_posture(MdnsAvahiPosture::RecoveryDenied))
        .unwrap();
    let result = rt.block_on(gate.check_advertisement("_http._tcp", "printer", 8080));
    assert!(result.is_err());
    match result.unwrap_err() {
        NetworkPolicyError::MdnsAdvertisementDenied(reason) => {
            assert_eq!(reason, "recovery-denied");
        }
        other => panic!("expected MdnsAdvertisementDenied(recovery-denied), got {other:?}"),
    }
}

#[test]
fn mdns_set_posture_to_airgap_denied_blocks_advertisement() {
    let gate = MdnsGate::new();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(gate.set_posture(MdnsAvahiPosture::AirgapDenied))
        .unwrap();
    let result = rt.block_on(gate.check_advertisement("_http._tcp", "printer", 631));
    assert!(result.is_err());
    match result.unwrap_err() {
        NetworkPolicyError::MdnsAdvertisementDenied(reason) => {
            assert_eq!(reason, "airgap-denied");
        }
        other => panic!("expected MdnsAdvertisementDenied(airgap-denied), got {other:?}"),
    }
}

#[test]
fn mdns_admit_allowlist_with_valid_signature_succeeds() {
    let gate = MdnsGate::new();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (sk, fingerprint) = make_authority();
    rt.block_on(gate.register_authority(&fingerprint, sk.verifying_key()));

    let ad = make_ad("ad-1", "_http._tcp", "printer", 8080);
    let allowlist = sign_allowlist(&sk, "al-1", &[ad], &fingerprint);

    let result = rt.block_on(gate.admit_allowlist(allowlist));
    assert!(result.is_ok());
}

#[test]
fn mdns_admit_allowlist_with_invalid_signature_returns_internal_error() {
    let gate = MdnsGate::new();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (sk, fingerprint) = make_authority();
    rt.block_on(gate.register_authority(&fingerprint, sk.verifying_key()));

    let ad = make_ad("ad-1", "_http._tcp", "printer", 8080);
    let mut allowlist = sign_allowlist(&sk, "al-1", &[ad], &fingerprint);
    // Corrupt the signature.
    allowlist.signature[0] ^= 0xFF;

    let result = rt.block_on(gate.admit_allowlist(allowlist));
    assert!(result.is_err());
    match result.unwrap_err() {
        NetworkPolicyError::Internal(msg) => {
            assert!(msg.contains("invalid mDNS allowlist signature"));
        }
        other => panic!("expected Internal, got {other:?}"),
    }
}

#[test]
fn mdns_check_advertisement_with_operator_authorised_matching_entry_succeeds() {
    let gate = MdnsGate::new();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (sk, fingerprint) = make_authority();
    rt.block_on(gate.register_authority(&fingerprint, sk.verifying_key()));

    let ad = make_ad("ad-1", "_http._tcp", "My App", 8080);
    let allowlist = sign_allowlist(&sk, "al-1", &[ad], &fingerprint);
    rt.block_on(gate.admit_allowlist(allowlist)).unwrap();

    rt.block_on(gate.set_posture(MdnsAvahiPosture::OperatorAuthorised {
        allowlist_id: "al-1".into(),
    }))
    .unwrap();

    let result = rt.block_on(gate.check_advertisement("_http._tcp", "My App", 8080));
    assert!(result.is_ok());
}

#[test]
fn mdns_check_advertisement_with_operator_authorised_non_matching_returns_denied() {
    let gate = MdnsGate::new();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (sk, fingerprint) = make_authority();
    rt.block_on(gate.register_authority(&fingerprint, sk.verifying_key()));

    let ad = make_ad("ad-1", "_http._tcp", "My App", 8080);
    let allowlist = sign_allowlist(&sk, "al-1", &[ad], &fingerprint);
    rt.block_on(gate.admit_allowlist(allowlist)).unwrap();

    rt.block_on(gate.set_posture(MdnsAvahiPosture::OperatorAuthorised {
        allowlist_id: "al-1".into(),
    }))
    .unwrap();

    // Different port — should not match.
    let result = rt.block_on(gate.check_advertisement("_http._tcp", "My App", 9090));
    assert!(result.is_err());
    match result.unwrap_err() {
        NetworkPolicyError::MdnsAdvertisementDenied(reason) => {
            assert_eq!(reason, "not in allowlist");
        }
        other => panic!("expected MdnsAdvertisementDenied, got {other:?}"),
    }
}

#[test]
fn mdns_advertisement_allowlist_serde_round_trip() {
    let (sk, fingerprint) = make_authority();
    let ad = make_ad("ad-1", "_http._tcp", "My App", 8080);
    let allowlist = sign_allowlist(&sk, "al-1", &[ad], &fingerprint);

    let json = serde_json::to_string(&allowlist).unwrap();
    let back: MdnsAdvertisementAllowlist = serde_json::from_str(&json).unwrap();
    assert_eq!(allowlist.allowlist_id, back.allowlist_id);
    assert_eq!(allowlist.advertisements.len(), back.advertisements.len());
    assert_eq!(allowlist.signer_fingerprint, back.signer_fingerprint);
    assert_eq!(allowlist.signature, back.signature);
}

// ---------------------------------------------------------------------------
// Firewall tests
// ---------------------------------------------------------------------------

#[test]
fn firewall_backend_has_2_variants() {
    assert_eq!(FirewallBackend::COUNT, 2);
    let variants: Vec<_> = FirewallBackend::iter().collect();
    assert!(variants.contains(&FirewallBackend::Nftables));
    assert!(variants.contains(&FirewallBackend::IptablesFallback));
}

#[test]
fn firewall_chain_has_5_variants() {
    assert_eq!(FirewallChain::COUNT, 5);
    let variants: Vec<_> = FirewallChain::iter().collect();
    assert!(variants.contains(&FirewallChain::Input));
    assert!(variants.contains(&FirewallChain::Output));
    assert!(variants.contains(&FirewallChain::Forward));
    assert!(variants.contains(&FirewallChain::Prerouting));
    assert!(variants.contains(&FirewallChain::Postrouting));
}

#[test]
fn firewall_action_has_5_variants() {
    assert_eq!(FirewallAction::COUNT, 5);
    let variants: Vec<_> = FirewallAction::iter().collect();
    assert!(variants.contains(&FirewallAction::Accept));
    assert!(variants.contains(&FirewallAction::Drop));
    assert!(variants.contains(&FirewallAction::Reject));
    assert!(variants.contains(&FirewallAction::Log));
    assert!(variants.contains(&FirewallAction::Return));
}

#[test]
fn firewall_ruleset_builder_produces_ruleset_with_built_at_set() {
    let ruleset = FirewallRulesetBuilder::new(FirewallBackend::Nftables)
        .rule(make_rule(
            "r1",
            FirewallChain::Output,
            FirewallMatch::DestPort {
                port: 443,
                protocol: ProtocolFamily::Tcp,
            },
            FirewallAction::Accept,
        ))
        .build();

    assert_eq!(ruleset.backend, FirewallBackend::Nftables);
    assert_eq!(ruleset.rules.len(), 1);
    assert!(ruleset.generation > 0, "generation should be set");
}

#[test]
fn firewall_manager_apply_ruleset_stores_as_active() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mgr = FirewallManager::new();
    let ruleset = make_basic_ruleset();

    rt.block_on(mgr.apply_ruleset(ruleset.clone())).unwrap();
    let active = rt.block_on(mgr.active_ruleset()).unwrap();

    assert_eq!(active.rules.len(), 1);
    assert_eq!(active.backend, FirewallBackend::Nftables);
    assert_eq!(active.generation, ruleset.generation);
}

#[test]
fn firewall_manager_apply_ruleset_with_iptables_fallback_flips_fallback_active() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mgr = FirewallManager::new();
    assert!(!rt.block_on(mgr.is_in_fallback()));

    let ruleset = FirewallRulesetBuilder::new(FirewallBackend::IptablesFallback)
        .rule(make_rule(
            "r1",
            FirewallChain::Input,
            FirewallMatch::All,
            FirewallAction::Accept,
        ))
        .build();

    rt.block_on(mgr.apply_ruleset(ruleset)).unwrap();
    assert!(rt.block_on(mgr.is_in_fallback()));
}

#[test]
fn firewall_manager_history_after_3_applies_returns_3_prior() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mgr = FirewallManager::new();

    for i in 0..4 {
        let ruleset = FirewallRulesetBuilder::new(FirewallBackend::Nftables)
            .rule(make_rule(
                &format!("r{i}"),
                FirewallChain::Input,
                FirewallMatch::All,
                FirewallAction::Accept,
            ))
            .build();
        rt.block_on(mgr.apply_ruleset(ruleset)).unwrap();
    }

    let history = rt.block_on(mgr.history());
    // 4 applies → 3 prior in history; 1 active.
    assert_eq!(history.len(), 3);
}

#[test]
fn firewall_enforce_subject_directive_deny_all_returns_drop_rule() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mgr = FirewallManager::new();
    let subject = SubjectId("agent:test".into());
    let directive = OutboundDirective::DenyAll;

    let rules = rt.block_on(mgr.enforce_subject_directive(&subject, &directive));
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].chain, FirewallChain::Output);
    assert_eq!(rules[0].action, FirewallAction::Drop);
    assert!(matches!(rules[0].match_expr, FirewallMatch::SourceIp(_)));
}

#[test]
fn firewall_enforce_subject_directive_allow_loopback_only_returns_2_rules() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mgr = FirewallManager::new();
    let subject = SubjectId("agent:test".into());
    let directive = OutboundDirective::AllowLoopbackOnly;

    let rules = rt.block_on(mgr.enforce_subject_directive(&subject, &directive));
    assert_eq!(rules.len(), 2);
    // First rule should accept loopback.
    assert_eq!(rules[0].chain, FirewallChain::Output);
    assert_eq!(rules[0].action, FirewallAction::Accept);
    assert!(matches!(rules[0].match_expr, FirewallMatch::DestCidr(_)));
    // Second rule should drop all else.
    assert_eq!(rules[1].chain, FirewallChain::Output);
    assert_eq!(rules[1].action, FirewallAction::Drop);
    assert!(matches!(rules[1].match_expr, FirewallMatch::All));
}

#[test]
fn firewall_rule_serde_round_trip() {
    let rule = make_rule(
        "r1",
        FirewallChain::Input,
        FirewallMatch::SourceIp(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
        FirewallAction::Accept,
    );

    let json = serde_json::to_string(&rule).unwrap();
    let back: FirewallRule = serde_json::from_str(&json).unwrap();
    assert_eq!(rule.rule_id, back.rule_id);
    assert_eq!(rule.chain, back.chain);
    assert_eq!(rule.action, back.action);
}

#[test]
fn firewall_match_all_variants_serde_round_trip() {
    // Extra sanity: every FirewallMatch variant round-trips through JSON.
    let variants: &[FirewallMatch] = &[
        FirewallMatch::SourceIp(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))),
        FirewallMatch::SourceCidr("10.0.0.0/8".into()),
        FirewallMatch::DestIp(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
        FirewallMatch::DestCidr("192.168.0.0/16".into()),
        FirewallMatch::DestPort {
            port: 443,
            protocol: ProtocolFamily::Tcp,
        },
        FirewallMatch::Interface("eth0".into()),
        FirewallMatch::CtState("established,related".into()),
        FirewallMatch::All,
    ];

    for m in variants {
        let json = serde_json::to_string(m).unwrap();
        let back: FirewallMatch = serde_json::from_str(&json).unwrap();
        assert_eq!(m, &back, "round-trip failed for {m:?}");
    }
}

#[test]
fn firewall_ruleset_serde_round_trip() {
    let ruleset = FirewallRulesetBuilder::new(FirewallBackend::Nftables)
        .rule(make_rule(
            "r1",
            FirewallChain::Output,
            FirewallMatch::DestPort {
                port: 53,
                protocol: ProtocolFamily::Udp,
            },
            FirewallAction::Accept,
        ))
        .rule(make_rule(
            "r2",
            FirewallChain::Output,
            FirewallMatch::All,
            FirewallAction::Drop,
        ))
        .build();

    let json = serde_json::to_string(&ruleset).unwrap();
    let back: FirewallRuleset = serde_json::from_str(&json).unwrap();
    assert_eq!(ruleset.backend, back.backend);
    assert_eq!(ruleset.rules.len(), back.rules.len());
    assert_eq!(ruleset.generation, back.generation);
}
