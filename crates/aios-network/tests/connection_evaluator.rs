//! Integration tests for `ConnectionEvaluator` covering INV I3 + INV I9.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::redundant_clone,
    missing_docs,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::net::Ipv4Addr;
use std::sync::Arc;

use chrono::Utc;

use aios_network::{
    fingerprint_from_vk, generate_keypair, sign_grant, AllowlistEntry, AllowlistEntryKind,
    ConnectionDecisionV2, ConnectionEvaluator, EvaluateConnectionRequestV2, GroupId,
    NetworkPolicyError, NetworkPolicyErrorCode, OutboundDirectiveKind, OutboundGrant,
    OutboundGrantRegistry, PortPolicy, ProtocolFamily, SubjectId,
};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

struct TestContext {
    evaluator: ConnectionEvaluator,
    registry: Arc<OutboundGrantRegistry>,
    fp: String,
    sk: ed25519_dalek::SigningKey,
}

fn build_context() -> TestContext {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut registry = OutboundGrantRegistry::new();
    registry.register_authority(&fp, vk);
    let registry = Arc::new(registry);
    let evaluator = ConnectionEvaluator::new(Arc::clone(&registry));
    TestContext {
        evaluator,
        registry,
        fp,
        sk,
    }
}

fn make_entry(kind: AllowlistEntryKind, value: &str, port_policy: PortPolicy) -> AllowlistEntry {
    AllowlistEntry {
        kind,
        value: value.into(),
        port_policy,
        protocol: ProtocolFamily::Tcp,
    }
}

fn unsigned_grant(
    id: &str,
    subject: &str,
    fp: &str,
    entries: Vec<AllowlistEntry>,
) -> OutboundGrant {
    OutboundGrant {
        grant_id: id.into(),
        subject: SubjectId(subject.into()),
        allowlist: entries,
        directive_kind: OutboundDirectiveKind::AllowListOnly,
        issued_at: Utc::now(),
        expires_at: None,
        signer_fingerprint: fp.into(),
        signature: Vec::new(),
    }
}

async fn append_signed_grant(
    registry: &OutboundGrantRegistry,
    fp: &str,
    sk: &ed25519_dalek::SigningKey,
    id: &str,
    subject: &str,
    entries: Vec<AllowlistEntry>,
) {
    let mut grant = unsigned_grant(id, subject, fp, entries);
    sign_grant(&mut grant, sk);
    registry.append_grant(grant).await.unwrap();
}

// ---------------------------------------------------------------------------
// default deny
// ---------------------------------------------------------------------------

#[tokio::test]
async fn evaluate_with_no_grant_returns_denied_default_deny() {
    let ctx = build_context();
    let req = EvaluateConnectionRequestV2 {
        subject: SubjectId("human:test".into()),
        destination_host: "example.com".into(),
        destination_port: 443,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: None,
    };
    let decision = ctx.evaluator.evaluate(req).await.unwrap();
    assert_eq!(
        decision,
        ConnectionDecisionV2::Denied {
            code: NetworkPolicyErrorCode::DefaultDeny,
            reason: "no allowlist entry matches".into(),
        }
    );
}

// ---------------------------------------------------------------------------
// FQDN matching
// ---------------------------------------------------------------------------

#[tokio::test]
async fn evaluate_with_fqdn_allowlist_match_returns_allowed_with_fqdn_kind() {
    let ctx = build_context();
    append_signed_grant(
        &ctx.registry,
        &ctx.fp,
        &ctx.sk,
        "g-1",
        "human:test",
        vec![make_entry(
            AllowlistEntryKind::HostFqdn,
            "example.com",
            PortPolicy::OperatorAssigned { port: 443 },
        )],
    )
    .await;

    let req = EvaluateConnectionRequestV2 {
        subject: SubjectId("human:test".into()),
        destination_host: "example.com".into(),
        destination_port: 443,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: None,
    };
    let decision = ctx.evaluator.evaluate(req).await.unwrap();
    assert_eq!(
        decision,
        ConnectionDecisionV2::Allowed {
            matched_rule_id: "fqdn:example.com".into(),
            allowlist_entry_kind: AllowlistEntryKind::HostFqdn,
        }
    );
}

#[tokio::test]
async fn evaluate_with_fqdn_allowlist_case_insensitive_match() {
    let ctx = build_context();
    append_signed_grant(
        &ctx.registry,
        &ctx.fp,
        &ctx.sk,
        "g-1",
        "human:test",
        vec![make_entry(
            AllowlistEntryKind::HostFqdn,
            "Example.COM",
            PortPolicy::OperatorAssigned { port: 443 },
        )],
    )
    .await;

    let req = EvaluateConnectionRequestV2 {
        subject: SubjectId("human:test".into()),
        destination_host: "example.com".into(),
        destination_port: 443,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: None,
    };
    let decision = ctx.evaluator.evaluate(req).await.unwrap();
    assert_eq!(
        decision,
        ConnectionDecisionV2::Allowed {
            matched_rule_id: "fqdn:Example.COM".into(),
            allowlist_entry_kind: AllowlistEntryKind::HostFqdn,
        }
    );
}

#[tokio::test]
async fn evaluate_with_fqdn_allowlist_no_match_returns_denied() {
    let ctx = build_context();
    append_signed_grant(
        &ctx.registry,
        &ctx.fp,
        &ctx.sk,
        "g-1",
        "human:test",
        vec![make_entry(
            AllowlistEntryKind::HostFqdn,
            "allowed.com",
            PortPolicy::OperatorAssigned { port: 443 },
        )],
    )
    .await;

    let req = EvaluateConnectionRequestV2 {
        subject: SubjectId("human:test".into()),
        destination_host: "evil.com".into(),
        destination_port: 443,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: None,
    };
    let decision = ctx.evaluator.evaluate(req).await.unwrap();
    assert_eq!(
        decision,
        ConnectionDecisionV2::Denied {
            code: NetworkPolicyErrorCode::DefaultDeny,
            reason: "no allowlist entry matches".into(),
        }
    );
}

// ---------------------------------------------------------------------------
// IPv4 exact matching
// ---------------------------------------------------------------------------

#[tokio::test]
async fn evaluate_with_ipv4_address_exact_match_returns_allowed() {
    let ctx = build_context();
    append_signed_grant(
        &ctx.registry,
        &ctx.fp,
        &ctx.sk,
        "g-1",
        "human:test",
        vec![make_entry(
            AllowlistEntryKind::IpV4Address,
            "192.168.1.100",
            PortPolicy::OperatorAssigned { port: 8080 },
        )],
    )
    .await;

    let req = EvaluateConnectionRequestV2 {
        subject: SubjectId("human:test".into()),
        destination_host: "192.168.1.100".into(),
        destination_port: 8080,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: None,
    };
    let decision = ctx.evaluator.evaluate(req).await.unwrap();
    assert_eq!(
        decision,
        ConnectionDecisionV2::Allowed {
            matched_rule_id: "ipv4:192.168.1.100".into(),
            allowlist_entry_kind: AllowlistEntryKind::IpV4Address,
        }
    );
}

// ---------------------------------------------------------------------------
// IPv4 CIDR matching
// ---------------------------------------------------------------------------

#[tokio::test]
async fn evaluate_with_ipv4_cidr_containment_match_returns_allowed() {
    let ctx = build_context();
    append_signed_grant(
        &ctx.registry,
        &ctx.fp,
        &ctx.sk,
        "g-1",
        "human:test",
        vec![make_entry(
            AllowlistEntryKind::IpV4Cidr,
            "10.0.0.0/8",
            PortPolicy::RegisteredEphemeral,
        )],
    )
    .await;

    let req = EvaluateConnectionRequestV2 {
        subject: SubjectId("human:test".into()),
        destination_host: "10.1.2.3".into(),
        destination_port: 9000,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: None,
    };
    let decision = ctx.evaluator.evaluate(req).await.unwrap();
    assert_eq!(
        decision,
        ConnectionDecisionV2::Allowed {
            matched_rule_id: "cidr4:10.0.0.0/8".into(),
            allowlist_entry_kind: AllowlistEntryKind::IpV4Cidr,
        }
    );
}

#[tokio::test]
async fn evaluate_with_ipv4_cidr_no_containment_returns_denied() {
    let ctx = build_context();
    append_signed_grant(
        &ctx.registry,
        &ctx.fp,
        &ctx.sk,
        "g-1",
        "human:test",
        vec![make_entry(
            AllowlistEntryKind::IpV4Cidr,
            "10.0.0.0/8",
            PortPolicy::RegisteredEphemeral,
        )],
    )
    .await;

    let req = EvaluateConnectionRequestV2 {
        subject: SubjectId("human:test".into()),
        destination_host: "192.168.1.1".into(),
        destination_port: 9000,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: None,
    };
    let decision = ctx.evaluator.evaluate(req).await.unwrap();
    assert_eq!(
        decision,
        ConnectionDecisionV2::Denied {
            code: NetworkPolicyErrorCode::DefaultDeny,
            reason: "no allowlist entry matches".into(),
        }
    );
}

// ---------------------------------------------------------------------------
// port policy matching
// ---------------------------------------------------------------------------

#[tokio::test]
async fn evaluate_with_port_policy_well_known_for_port_80_succeeds() {
    let ctx = build_context();
    append_signed_grant(
        &ctx.registry,
        &ctx.fp,
        &ctx.sk,
        "g-1",
        "human:test",
        vec![make_entry(
            AllowlistEntryKind::HostFqdn,
            "example.com",
            PortPolicy::WellKnown,
        )],
    )
    .await;

    let req = EvaluateConnectionRequestV2 {
        subject: SubjectId("human:test".into()),
        destination_host: "example.com".into(),
        destination_port: 80,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: None,
    };
    let decision = ctx.evaluator.evaluate(req).await.unwrap();
    assert_eq!(
        decision,
        ConnectionDecisionV2::Allowed {
            matched_rule_id: "fqdn:example.com".into(),
            allowlist_entry_kind: AllowlistEntryKind::HostFqdn,
        }
    );
}

#[tokio::test]
async fn evaluate_with_port_policy_well_known_for_port_8080_returns_denied() {
    let ctx = build_context();
    append_signed_grant(
        &ctx.registry,
        &ctx.fp,
        &ctx.sk,
        "g-1",
        "human:test",
        vec![make_entry(
            AllowlistEntryKind::HostFqdn,
            "example.com",
            PortPolicy::WellKnown,
        )],
    )
    .await;

    let req = EvaluateConnectionRequestV2 {
        subject: SubjectId("human:test".into()),
        destination_host: "example.com".into(),
        destination_port: 8080,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: None,
    };
    let decision = ctx.evaluator.evaluate(req).await.unwrap();
    assert_eq!(
        decision,
        ConnectionDecisionV2::Denied {
            code: NetworkPolicyErrorCode::DefaultDeny,
            reason: "no allowlist entry matches".into(),
        }
    );
}

#[tokio::test]
async fn evaluate_with_port_policy_operator_assigned_exact_port_succeeds() {
    let ctx = build_context();
    append_signed_grant(
        &ctx.registry,
        &ctx.fp,
        &ctx.sk,
        "g-1",
        "human:test",
        vec![make_entry(
            AllowlistEntryKind::HostFqdn,
            "example.com",
            PortPolicy::OperatorAssigned { port: 8443 },
        )],
    )
    .await;

    let req = EvaluateConnectionRequestV2 {
        subject: SubjectId("human:test".into()),
        destination_host: "example.com".into(),
        destination_port: 8443,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: None,
    };
    let decision = ctx.evaluator.evaluate(req).await.unwrap();
    assert!(matches!(decision, ConnectionDecisionV2::Allowed { .. }));
}

// ---------------------------------------------------------------------------
// INV I3 — cross-group access
// ---------------------------------------------------------------------------

#[tokio::test]
async fn evaluate_cross_group_destination_with_different_source_group_returns_cross_group_access_forbidden(
) {
    let ctx = build_context();
    append_signed_grant(
        &ctx.registry,
        &ctx.fp,
        &ctx.sk,
        "g-1",
        "human:alice",
        vec![make_entry(
            AllowlistEntryKind::HostFqdn,
            "example.com",
            PortPolicy::OperatorAssigned { port: 443 },
        )],
    )
    .await;

    ctx.evaluator
        .register_subject_group(
            SubjectId("human:alice".into()),
            GroupId("group:engineering".into()),
        )
        .await
        .unwrap();

    let req = EvaluateConnectionRequestV2 {
        subject: SubjectId("human:alice".into()),
        destination_host: "example.com".into(),
        destination_port: 443,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: Some(GroupId("group:finance".into())),
    };
    let decision = ctx.evaluator.evaluate(req).await.unwrap();
    assert_eq!(
        decision,
        ConnectionDecisionV2::Denied {
            code: NetworkPolicyErrorCode::CrossGroupAccessForbidden,
            reason: "source group group:engineering cannot reach destination group group:finance"
                .into(),
        }
    );
}

#[tokio::test]
async fn evaluate_cross_group_destination_with_same_source_group_succeeds() {
    let ctx = build_context();
    append_signed_grant(
        &ctx.registry,
        &ctx.fp,
        &ctx.sk,
        "g-1",
        "human:alice",
        vec![make_entry(
            AllowlistEntryKind::HostFqdn,
            "example.com",
            PortPolicy::OperatorAssigned { port: 443 },
        )],
    )
    .await;

    ctx.evaluator
        .register_subject_group(
            SubjectId("human:alice".into()),
            GroupId("group:engineering".into()),
        )
        .await
        .unwrap();

    let req = EvaluateConnectionRequestV2 {
        subject: SubjectId("human:alice".into()),
        destination_host: "example.com".into(),
        destination_port: 443,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: Some(GroupId("group:engineering".into())),
    };
    let decision = ctx.evaluator.evaluate(req).await.unwrap();
    assert_eq!(
        decision,
        ConnectionDecisionV2::Allowed {
            matched_rule_id: "fqdn:example.com".into(),
            allowlist_entry_kind: AllowlistEntryKind::HostFqdn,
        }
    );
}

#[tokio::test]
async fn evaluate_unset_destination_group_hint_skips_cross_group_check() {
    let ctx = build_context();
    append_signed_grant(
        &ctx.registry,
        &ctx.fp,
        &ctx.sk,
        "g-1",
        "human:alice",
        vec![make_entry(
            AllowlistEntryKind::HostFqdn,
            "example.com",
            PortPolicy::OperatorAssigned { port: 443 },
        )],
    )
    .await;

    // Register in an isolated group but don't set destination_group_hint.
    ctx.evaluator
        .register_subject_group(
            SubjectId("human:alice".into()),
            GroupId("group:isolated".into()),
        )
        .await
        .unwrap();

    let req = EvaluateConnectionRequestV2 {
        subject: SubjectId("human:alice".into()),
        destination_host: "example.com".into(),
        destination_port: 443,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: None,
    };
    let decision = ctx.evaluator.evaluate(req).await.unwrap();
    assert_eq!(
        decision,
        ConnectionDecisionV2::Allowed {
            matched_rule_id: "fqdn:example.com".into(),
            allowlist_entry_kind: AllowlistEntryKind::HostFqdn,
        }
    );
}

// ---------------------------------------------------------------------------
// INV I9 — FQDN fan-out bound
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_fqdn_bounded_under_limit_returns_ok() {
    let registry = Arc::new(OutboundGrantRegistry::new());
    let evaluator = ConnectionEvaluator::new(registry);
    let addresses: Vec<std::net::IpAddr> = (1..=5)
        .map(|i| std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, i)))
        .collect();

    let result = evaluator
        .resolve_fqdn_bounded("example.com", addresses.clone())
        .unwrap();
    assert_eq!(result.fqdn, "example.com");
    assert_eq!(result.addresses.len(), 5);
    assert_eq!(result.addresses, addresses);
}

#[tokio::test]
async fn resolve_fqdn_bounded_exactly_at_16_returns_ok() {
    let registry = Arc::new(OutboundGrantRegistry::new());
    let evaluator = ConnectionEvaluator::new(registry);
    let addresses: Vec<std::net::IpAddr> = (1..=16)
        .map(|i| std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, i)))
        .collect();

    let result = evaluator
        .resolve_fqdn_bounded("example.com", addresses)
        .unwrap();
    assert_eq!(result.addresses.len(), 16);
}

#[tokio::test]
async fn resolve_fqdn_bounded_above_16_returns_allowlist_fqdn_fanout_exceeded() {
    let registry = Arc::new(OutboundGrantRegistry::new());
    let evaluator = ConnectionEvaluator::new(registry);
    let addresses: Vec<std::net::IpAddr> = (1..=17)
        .map(|i| std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, i)))
        .collect();

    let result = evaluator.resolve_fqdn_bounded("big.example.com", addresses);
    match result {
        Err(NetworkPolicyError::AllowlistFqdnFanoutExceeded {
            fqdn,
            resolved_count,
        }) => {
            assert_eq!(fqdn, "big.example.com");
            assert_eq!(resolved_count, 17);
        }
        other => panic!("expected AllowlistFqdnFanoutExceeded, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// register + cross-group smoke
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_subject_group_then_get_via_evaluate() {
    let ctx = build_context();
    append_signed_grant(
        &ctx.registry,
        &ctx.fp,
        &ctx.sk,
        "g-1",
        "human:bob",
        vec![make_entry(
            AllowlistEntryKind::HostFqdn,
            "internal.corp",
            PortPolicy::OperatorAssigned { port: 443 },
        )],
    )
    .await;

    ctx.evaluator
        .register_subject_group(SubjectId("human:bob".into()), GroupId("group:ops".into()))
        .await
        .unwrap();

    // Same group — allowed.
    let req = EvaluateConnectionRequestV2 {
        subject: SubjectId("human:bob".into()),
        destination_host: "internal.corp".into(),
        destination_port: 443,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: Some(GroupId("group:ops".into())),
    };
    let decision = ctx.evaluator.evaluate(req).await.unwrap();
    assert!(matches!(decision, ConnectionDecisionV2::Allowed { .. }));

    // Different group — denied.
    let req2 = EvaluateConnectionRequestV2 {
        subject: SubjectId("human:bob".into()),
        destination_host: "internal.corp".into(),
        destination_port: 443,
        protocol: ProtocolFamily::Tcp,
        destination_group_hint: Some(GroupId("group:intruders".into())),
    };
    let decision2 = ctx.evaluator.evaluate(req2).await.unwrap();
    assert!(matches!(
        decision2,
        ConnectionDecisionV2::Denied {
            code: NetworkPolicyErrorCode::CrossGroupAccessForbidden,
            ..
        }
    ));
}

// ---------------------------------------------------------------------------
// concurrency
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_evaluate_5_no_panic() {
    let ctx = build_context();
    append_signed_grant(
        &ctx.registry,
        &ctx.fp,
        &ctx.sk,
        "g-1",
        "human:concurrent",
        vec![make_entry(
            AllowlistEntryKind::HostFqdn,
            "example.com",
            PortPolicy::OperatorAssigned { port: 443 },
        )],
    )
    .await;

    let evaluator = Arc::new(ctx.evaluator);
    let mut handles = Vec::new();
    for _ in 0..5 {
        let ev = Arc::clone(&evaluator);
        handles.push(tokio::spawn(async move {
            let req = EvaluateConnectionRequestV2 {
                subject: SubjectId("human:concurrent".into()),
                destination_host: "example.com".into(),
                destination_port: 443,
                protocol: ProtocolFamily::Tcp,
                destination_group_hint: None,
            };
            ev.evaluate(req).await.unwrap()
        }));
    }

    for h in handles {
        let decision = h.await.unwrap();
        assert!(matches!(decision, ConnectionDecisionV2::Allowed { .. }));
    }
}
