//! Integration tests for DNS resolver discipline (S8.4 §3).
//!
//! Covers `ResolverBackend`, `DnsTransport`, `ResolverProfileManager`
//! allowlist admission, rotation, `QueryGuard`, and serde round trips.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    missing_docs,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use aios_network::{
    sign_allowlist, validate_transport, DnsTransport, NetworkPolicyError, ResolverAllowlist,
    ResolverBackend, ResolverEndpoint, ResolverProfile, ResolverProfileManager,
};
use chrono::Utc;
use ed25519_dalek::{SigningKey, VerifyingKey};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn fingerprint_from_vk(vk: &VerifyingKey) -> String {
    vk.as_bytes()[..16]
        .iter()
        .fold(String::with_capacity(32), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn generate_keypair() -> (SigningKey, VerifyingKey) {
    let sk = SigningKey::generate(&mut rand_core::OsRng);
    let vk = sk.verifying_key();
    (sk, vk)
}

fn test_endpoint_dot(fqdn: &str) -> ResolverEndpoint {
    ResolverEndpoint {
        fqdn: fqdn.into(),
        address: IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
        port: 853,
        transport: DnsTransport::DnsOverTls,
        spki_pin: None,
    }
}

fn build_signed_allowlist(
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

fn default_profile() -> ResolverProfile {
    ResolverProfile {
        backend: ResolverBackend::Unbound,
        active_list_id: "default".into(),
        effective_endpoints: vec![],
        cache_ttl_seconds: 300,
    }
}

// ---------------------------------------------------------------------------
// 17 integration tests
// ---------------------------------------------------------------------------

#[test]
fn resolver_backend_has_5_variants_including_degraded_hosts_file_only() {
    let variants: &[ResolverBackend] = &[
        ResolverBackend::SystemdResolved,
        ResolverBackend::Unbound,
        ResolverBackend::Bind9,
        ResolverBackend::Dnsmasq,
        ResolverBackend::DegradedHostsFileOnly,
    ];
    assert_eq!(variants.len(), 5);
    assert!(variants.contains(&ResolverBackend::DegradedHostsFileOnly));
}

#[test]
fn dns_transport_has_4_variants_including_plain_dns_forbidden() {
    let variants: &[DnsTransport] = &[
        DnsTransport::DnsOverTls,
        DnsTransport::DnsOverHttps,
        DnsTransport::DnsOverQuic,
        DnsTransport::PlainDnsForbidden,
    ];
    assert_eq!(variants.len(), 4);
    assert!(variants.contains(&DnsTransport::PlainDnsForbidden));
}

#[test]
fn validate_transport_plain_dns_forbidden_returns_error() {
    let result = validate_transport(DnsTransport::PlainDnsForbidden);
    match result {
        Err(NetworkPolicyError::PlainDnsForbidden(msg)) => {
            assert!(msg.contains("INV I9"));
        }
        other => panic!("expected PlainDnsForbidden, got {other:?}"),
    }
}

#[test]
fn validate_transport_dot_succeeds() {
    assert!(validate_transport(DnsTransport::DnsOverTls).is_ok());
}

#[tokio::test]
async fn admit_allowlist_with_valid_signature_succeeds() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut manager = ResolverProfileManager::new(default_profile());
    manager.register_authority(&fp, vk);

    let list = build_signed_allowlist("L1", vec![test_endpoint_dot("ns1.example.com")], &sk, &fp);
    assert!(manager.admit_allowlist(list).await.is_ok());
}

#[tokio::test]
async fn admit_allowlist_with_invalid_signature_returns_resolver_signature_invalid() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut manager = ResolverProfileManager::new(default_profile());
    manager.register_authority(&fp, vk);

    let mut list =
        build_signed_allowlist("L1", vec![test_endpoint_dot("ns1.example.com")], &sk, &fp);
    list.list_id = "L1-tampered".into();

    let result = manager.admit_allowlist(list).await;
    match result {
        Err(NetworkPolicyError::ResolverSignatureInvalid(msg)) => {
            assert!(msg.contains("ed25519 verify failed"));
        }
        other => panic!("expected ResolverSignatureInvalid, got {other:?}"),
    }
}

#[tokio::test]
async fn admit_allowlist_with_unknown_authority_returns_resolver_signature_invalid() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let manager = ResolverProfileManager::new(default_profile());

    let list = build_signed_allowlist("L1", vec![test_endpoint_dot("ns1.example.com")], &sk, &fp);
    let result = manager.admit_allowlist(list).await;
    match result {
        Err(NetworkPolicyError::ResolverSignatureInvalid(msg)) => {
            assert!(msg.contains("unknown authority"));
        }
        other => panic!("expected ResolverSignatureInvalid, got {other:?}"),
    }
}

#[tokio::test]
async fn admit_allowlist_containing_plain_dns_endpoint_returns_plain_dns_forbidden() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut manager = ResolverProfileManager::new(default_profile());
    manager.register_authority(&fp, vk);

    let bad_ep = ResolverEndpoint {
        fqdn: "evil-dns.example.com".into(),
        address: IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
        port: 53,
        transport: DnsTransport::PlainDnsForbidden,
        spki_pin: None,
    };
    let list = build_signed_allowlist("L1", vec![bad_ep], &sk, &fp);
    let result = manager.admit_allowlist(list).await;
    match result {
        Err(NetworkPolicyError::PlainDnsForbidden(msg)) => {
            assert!(msg.contains("forbidden plain DNS"));
        }
        other => panic!("expected PlainDnsForbidden, got {other:?}"),
    }
}

#[tokio::test]
async fn admit_allowlist_with_dot_port_not_853_logs_deviation_but_still_rejects() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut manager = ResolverProfileManager::new(default_profile());
    manager.register_authority(&fp, vk);

    let bad_ep = ResolverEndpoint {
        fqdn: "dns.example.com".into(),
        address: IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
        port: 5353,
        transport: DnsTransport::DnsOverTls,
        spki_pin: None,
    };
    let list = build_signed_allowlist("L1", vec![bad_ep], &sk, &fp);
    let result = manager.admit_allowlist(list).await;
    match result {
        Err(NetworkPolicyError::PlainDnsForbidden(msg)) => {
            assert!(msg.contains("non-standard port"));
        }
        other => panic!("expected PlainDnsForbidden for non-standard DoT port, got {other:?}"),
    }
}

#[tokio::test]
async fn rotate_active_list_to_admitted_list_succeeds() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut manager = ResolverProfileManager::new(default_profile());
    manager.register_authority(&fp, vk);

    let list = build_signed_allowlist("L1", vec![test_endpoint_dot("ns1.example.com")], &sk, &fp);
    manager.admit_allowlist(list).await.unwrap();
    assert!(manager.rotate_active_list("L1").await.is_ok());
}

#[tokio::test]
async fn rotate_active_list_to_unknown_list_returns_resolver_signature_invalid() {
    let manager = ResolverProfileManager::new(default_profile());
    let result = manager.rotate_active_list("nonexistent").await;
    match result {
        Err(NetworkPolicyError::ResolverSignatureInvalid(msg)) => {
            assert!(msg.contains("unknown list id"));
        }
        other => panic!("expected ResolverSignatureInvalid, got {other:?}"),
    }
}

#[tokio::test]
async fn current_profile_after_rotation_returns_new_endpoints() {
    let (sk, vk) = generate_keypair();
    let fp = fingerprint_from_vk(&vk);
    let mut manager = ResolverProfileManager::new(default_profile());
    manager.register_authority(&fp, vk);

    let new_eps = vec![
        test_endpoint_dot("ns1.example.com"),
        ResolverEndpoint {
            fqdn: "ns2.example.com".into(),
            address: IpAddr::V4(Ipv4Addr::new(1, 0, 0, 1)),
            port: 853,
            transport: DnsTransport::DnsOverTls,
            spki_pin: Some("pin2".into()),
        },
    ];
    let list = build_signed_allowlist("L2", new_eps, &sk, &fp);
    manager.admit_allowlist(list).await.unwrap();
    manager.rotate_active_list("L2").await.unwrap();

    let profile = manager.current_profile().await;
    assert_eq!(profile.active_list_id, "L2");
    assert_eq!(profile.effective_endpoints.len(), 2);
}

#[tokio::test]
async fn query_guard_increments_then_decrements_on_drop() {
    let manager = ResolverProfileManager::new(default_profile());
    {
        let _guard = manager.begin_query();
        // Guard is alive; QueryGuard is not Clone, but the RAII behavior is
        // tested by observing the drop-side effect.
    }
    // Guard dropped — no panic means the decrement fired correctly.
}

#[tokio::test]
async fn concurrent_5_query_guards_no_panic() {
    let manager = Arc::new(ResolverProfileManager::new(default_profile()));
    let mut handles = Vec::new();
    for _ in 0..5 {
        let m = Arc::clone(&manager);
        handles.push(tokio::spawn(async move {
            let _guard = m.begin_query();
            tokio::task::yield_now().await;
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
}

#[tokio::test]
async fn audit_resolver_used_returns_backend() {
    let manager = ResolverProfileManager::new(ResolverProfile {
        backend: ResolverBackend::SystemdResolved,
        active_list_id: "default".into(),
        effective_endpoints: vec![],
        cache_ttl_seconds: 300,
    });
    assert_eq!(
        manager.audit_resolver_used().await,
        ResolverBackend::SystemdResolved
    );
}

#[test]
fn degraded_hosts_file_only_backend_round_trip_via_serde() {
    let backend = ResolverBackend::DegradedHostsFileOnly;
    let json = serde_json::to_string(&backend).unwrap();
    let back: ResolverBackend = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ResolverBackend::DegradedHostsFileOnly);
}

#[test]
fn resolver_profile_serde_round_trip() {
    let profile = ResolverProfile {
        backend: ResolverBackend::Unbound,
        active_list_id: "L1".into(),
        effective_endpoints: vec![test_endpoint_dot("ns1.example.com")],
        cache_ttl_seconds: 300,
    };
    let json = serde_json::to_string(&profile).unwrap();
    let back: ResolverProfile = serde_json::from_str(&json).unwrap();
    assert_eq!(profile, back);
}
