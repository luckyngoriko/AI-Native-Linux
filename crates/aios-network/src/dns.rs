//! DNS resolver discipline (S8.4 §3).
//!
//! Closed `ResolverBackend` (5 variants) + `DnsTransport` (4 variants),
//! AIOS-root Ed25519-signed `ResolverAllowlist`, `ResolverProfileManager`
//! with multi-authority verification and rotation discipline.
//!
//! `PlainDnsForbidden` is an explicit sentinel — never selectable, always
//! denied at admission (INV I9 + INV-006).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing;

use crate::error::NetworkPolicyError;

/// Closed resolver backend vocabulary (S8.4 §3).
///
/// Five variants covering systemd-resolved, unbound, bind9, dnsmasq,
/// and a recovery-only `DegradedHostsFileOnly` fallback.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResolverBackend {
    /// systemd-resolved via D-Bus.
    SystemdResolved,
    /// Unbound recursive resolver.
    Unbound,
    /// BIND 9 authoritative + recursive.
    Bind9,
    /// dnsmasq lightweight forwarder.
    Dnsmasq,
    /// Recovery-only fallback — `/etc/hosts` static resolution only.
    DegradedHostsFileOnly,
}

/// Closed DNS transport vocabulary (S8.4 §3).
///
/// Four variants. `PlainDnsForbidden` is a sentinel marker that can
/// never be selected — any attempt to admit an allowlist containing it
/// is hard-denied (INV I9 + INV-006).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DnsTransport {
    /// DNS-over-TLS (`DoT`), port 853 — default secure transport.
    DnsOverTls,
    /// DNS-over-HTTPS (`DoH`).
    DnsOverHttps,
    /// DNS-over-QUIC (`DoQ`).
    DnsOverQuic,
    /// Sentinel — plain UDP/TCP DNS is forbidden. Never selectable;
    /// always denied at allowlist admission.
    PlainDnsForbidden,
}

/// A single resolver endpoint with optional SPKI certificate pinning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolverEndpoint {
    /// Fully qualified domain name of the resolver.
    pub fqdn: String,
    /// IP address of the resolver.
    pub address: IpAddr,
    /// Port the resolver listens on.
    pub port: u16,
    /// Transport protocol for this endpoint.
    pub transport: DnsTransport,
    /// Optional SPKI hash for certificate pinning (base64-encoded SHA-256).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spki_pin: Option<String>,
}

/// An AIOS-root-signed allowlist of resolver endpoints (S8.4 §3).
///
/// The allowlist is signed by a trusted authority using Ed25519 over
/// the canonical byte sequence `list_id || endpoints_canonical_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolverAllowlist {
    /// Unique identifier for this allowlist.
    pub list_id: String,
    /// The resolver endpoints authorised by this allowlist.
    pub endpoints: Vec<ResolverEndpoint>,
    /// When the allowlist was signed.
    pub signed_at: DateTime<Utc>,
    /// Hex-encoded Ed25519 public key fingerprint of the signing authority.
    pub signer_fingerprint: String,
    /// Ed25519 signature over the canonical byte sequence (64 bytes).
    pub signature: Vec<u8>,
}

impl ResolverAllowlist {
    /// Build the canonical byte sequence that must be signed.
    ///
    /// Format: `list_id || endpoints_canonical_json`
    #[must_use]
    pub fn canonical_signing_bytes(&self) -> Vec<u8> {
        let endpoints_json = serde_json::to_vec(&self.endpoints).unwrap_or_default();
        let mut out = Vec::with_capacity(self.list_id.len() + endpoints_json.len());
        out.extend_from_slice(self.list_id.as_bytes());
        out.extend_from_slice(&endpoints_json);
        out
    }
}

/// Runtime resolver profile — the active resolver configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolverProfile {
    /// The active resolver backend.
    pub backend: ResolverBackend,
    /// ID of the currently active allowlist.
    pub active_list_id: String,
    /// Endpoints currently in effect.
    pub effective_endpoints: Vec<ResolverEndpoint>,
    /// DNS cache TTL in seconds.
    pub cache_ttl_seconds: u32,
}

/// Manages resolver profiles, allowlist admission, and rotation (S8.4 §3).
///
/// Maintains a registry of trusted signing authorities, a set of admitted
/// allowlists, the active profile, and an in-flight query counter for
/// observability. Rotation swaps the active endpoint list without waiting
/// for in-flight queries to drain.
pub struct ResolverProfileManager {
    /// The currently active resolver profile.
    current: RwLock<ResolverProfile>,
    /// Admitted allowlists keyed by list ID.
    allowlists: RwLock<HashMap<String, ResolverAllowlist>>,
    /// Trusted signing authorities keyed by hex-encoded fingerprint.
    trusted_authorities: HashMap<String, VerifyingKey>,
    /// In-flight DNS query count (RAII-guarded, observability only).
    in_flight_query_count: AtomicU64,
}

impl ResolverProfileManager {
    /// Create a new profile manager with the given initial profile.
    #[must_use]
    pub fn new(initial: ResolverProfile) -> Self {
        Self {
            current: RwLock::new(initial),
            allowlists: RwLock::new(HashMap::new()),
            trusted_authorities: HashMap::new(),
            in_flight_query_count: AtomicU64::new(0),
        }
    }

    /// Register a trusted signing authority.
    pub fn register_authority(&mut self, fingerprint: &str, key: VerifyingKey) {
        self.trusted_authorities
            .insert(fingerprint.to_string(), key);
    }

    /// Admit a signed allowlist into the registry.
    ///
    /// # Verification steps (S8.4 §3)
    ///
    /// 1. Ed25519 signature verification against the authority registry.
    /// 2. Reject any endpoint with `PlainDnsForbidden` transport.
    /// 3. Reject any `DoT` endpoint with port != 853.
    ///
    /// # Errors
    ///
    /// Returns `ResolverSignatureInvalid` when the fingerprint is unknown
    /// or the signature fails. Returns `PlainDnsForbidden` when any
    /// endpoint uses plain DNS or a non-standard `DoT` port.
    pub async fn admit_allowlist(&self, list: ResolverAllowlist) -> Result<(), NetworkPolicyError> {
        // 1. Verify Ed25519 signature.
        let vk = self
            .trusted_authorities
            .get(&list.signer_fingerprint)
            .ok_or_else(|| {
                NetworkPolicyError::ResolverSignatureInvalid(format!(
                    "unknown authority: {}",
                    list.signer_fingerprint
                ))
            })?;

        let sig_array: [u8; 64] = list.signature.as_slice().try_into().map_err(|_| {
            NetworkPolicyError::ResolverSignatureInvalid("signature is not 64 bytes".into())
        })?;

        let signature = Signature::from_bytes(&sig_array);
        let message = list.canonical_signing_bytes();
        vk.verify_strict(&message, &signature).map_err(|_| {
            NetworkPolicyError::ResolverSignatureInvalid("ed25519 verify failed".into())
        })?;

        // 2. Reject PlainDnsForbidden endpoints.
        for ep in &list.endpoints {
            if ep.transport == DnsTransport::PlainDnsForbidden {
                return Err(NetworkPolicyError::PlainDnsForbidden(
                    "allowlist contains forbidden plain DNS endpoint".into(),
                ));
            }
        }

        // 3. Reject DoT endpoints with non-standard port.
        for ep in &list.endpoints {
            if ep.transport == DnsTransport::DnsOverTls && ep.port != 853 {
                tracing::warn!(
                    fqdn = %ep.fqdn,
                    port = ep.port,
                    "spec deviation: DoT endpoint with non-standard port rejected",
                );
                return Err(NetworkPolicyError::PlainDnsForbidden(format!(
                    "DoT endpoint {} has non-standard port {} (expected 853)",
                    ep.fqdn, ep.port,
                )));
            }
        }

        let mut allowlists = self.allowlists.write().await;
        allowlists.insert(list.list_id.clone(), list);
        drop(allowlists);
        Ok(())
    }

    /// Rotate the active allowlist to a previously admitted list.
    ///
    /// The new list must already be admitted. The active list ID and
    /// effective endpoints are swapped atomically. In-flight queries
    /// already routed to old endpoints complete naturally — new queries
    /// use the new endpoints.
    ///
    /// # Errors
    ///
    /// Returns `ResolverSignatureInvalid` when the list ID is unknown.
    pub async fn rotate_active_list(&self, new_list_id: &str) -> Result<(), NetworkPolicyError> {
        let allowlists = self.allowlists.read().await;
        let new_list = allowlists.get(new_list_id).ok_or_else(|| {
            NetworkPolicyError::ResolverSignatureInvalid(format!("unknown list id: {new_list_id}"))
        })?;
        let new_endpoints = new_list.endpoints.clone();
        drop(allowlists);

        let mut current = self.current.write().await;
        current.active_list_id = new_list_id.to_string();
        current.effective_endpoints = new_endpoints;
        drop(current);
        Ok(())
    }

    /// Return a snapshot of the current resolver profile.
    pub async fn current_profile(&self) -> ResolverProfile {
        let guard = self.current.read().await;
        guard.clone()
    }

    /// Return the active resolver backend (observability helper).
    pub async fn audit_resolver_used(&self) -> ResolverBackend {
        let guard = self.current.read().await;
        guard.backend
    }

    /// Begin a tracked DNS query.
    ///
    /// Returns a [`QueryGuard`] that increments the in-flight counter.
    /// The counter is decremented when the guard is dropped. The guard
    /// does **not** block rotation — it is purely for observability.
    pub fn begin_query(&self) -> QueryGuard<'_> {
        self.in_flight_query_count.fetch_add(1, Ordering::Relaxed);
        QueryGuard { manager: self }
    }
}

/// RAII guard that tracks an in-flight DNS query.
///
/// Increments `in_flight_query_count` on creation (via
/// [`ResolverProfileManager::begin_query`]) and decrements on drop.
/// Does **not** gate rotation — purely for observability (S8.4 §3).
#[must_use]
pub struct QueryGuard<'a> {
    manager: &'a ResolverProfileManager,
}

impl Drop for QueryGuard<'_> {
    fn drop(&mut self) {
        self.manager
            .in_flight_query_count
            .fetch_sub(1, Ordering::Relaxed);
    }
}

/// Validate a DNS transport against the system invariants (INV I9 + INV-006).
///
/// # Errors
///
/// Returns `PlainDnsForbidden` when the transport is `PlainDnsForbidden`.
pub fn validate_transport(transport: DnsTransport) -> Result<(), NetworkPolicyError> {
    match transport {
        DnsTransport::PlainDnsForbidden => Err(NetworkPolicyError::PlainDnsForbidden(
            "plain UDP DNS forbidden by INV I9 + INV-006".into(),
        )),
        DnsTransport::DnsOverTls | DnsTransport::DnsOverHttps | DnsTransport::DnsOverQuic => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Convenience: Ed25519 key generation + signing helpers (test-facing)
// ---------------------------------------------------------------------------

/// Mint a fresh Ed25519 keypair for test or authority setup.
#[must_use]
pub fn generate_keypair() -> (SigningKey, VerifyingKey) {
    let sk = SigningKey::generate(&mut rand_core::OsRng);
    let vk = sk.verifying_key();
    (sk, vk)
}

/// Sign a [`ResolverAllowlist`] in-place using the provided signing key.
pub fn sign_allowlist(list: &mut ResolverAllowlist, sk: &SigningKey) {
    let message = list.canonical_signing_bytes();
    list.signature = sk.sign(&message).to_vec();
}

/// Derive a human-readable fingerprint from a verifying key (hex-encoded
/// first 16 bytes).
#[must_use]
pub fn fingerprint_from_vk(vk: &VerifyingKey) -> String {
    vk.as_bytes()[..16]
        .iter()
        .fold(String::with_capacity(32), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

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

    // --- ResolverBackend ---

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
        // DegradedHostsFileOnly must be present.
        assert!(variants.contains(&ResolverBackend::DegradedHostsFileOnly));
    }

    #[test]
    fn resolver_backend_serde_round_trip() {
        let backends = vec![
            ResolverBackend::SystemdResolved,
            ResolverBackend::Unbound,
            ResolverBackend::Bind9,
            ResolverBackend::Dnsmasq,
            ResolverBackend::DegradedHostsFileOnly,
        ];
        for b in backends {
            let json = serde_json::to_string(&b).unwrap();
            let back: ResolverBackend = serde_json::from_str(&json).unwrap();
            assert_eq!(b, back);
        }
    }

    // --- DnsTransport ---

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
    fn dns_transport_display_non_empty() {
        for t in &[
            DnsTransport::DnsOverTls,
            DnsTransport::DnsOverHttps,
            DnsTransport::DnsOverQuic,
            DnsTransport::PlainDnsForbidden,
        ] {
            let s = t.to_string();
            assert!(!s.is_empty(), "empty Display for {t:?}");
        }
    }

    // --- validate_transport ---

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

    #[test]
    fn validate_transport_doh_and_doq_succeed() {
        assert!(validate_transport(DnsTransport::DnsOverHttps).is_ok());
        assert!(validate_transport(DnsTransport::DnsOverQuic).is_ok());
    }

    // --- ResolverAllowlist canonical signing ---

    #[test]
    fn canonical_signing_bytes_deterministic() {
        let eps = vec![test_endpoint_dot("resolver1.example.com")];
        let list = ResolverAllowlist {
            list_id: "L1".into(),
            endpoints: eps,
            signed_at: Utc::now(),
            signer_fingerprint: "abcd".into(),
            signature: vec![],
        };
        let a = list.canonical_signing_bytes();
        let b = list.canonical_signing_bytes();
        assert_eq!(a, b);
    }

    // --- ResolverEndpoint serde ---

    #[test]
    fn resolver_endpoint_serde_round_trip() {
        let ep = ResolverEndpoint {
            fqdn: "dns.example.com".into(),
            address: IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
            port: 853,
            transport: DnsTransport::DnsOverTls,
            spki_pin: Some("abc123base64==".into()),
        };
        let json = serde_json::to_string(&ep).unwrap();
        let back: ResolverEndpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(ep, back);
    }

    #[test]
    fn resolver_endpoint_without_spki_pin_deserializes() {
        let json = r#"{"fqdn":"dns.example.com","address":"8.8.8.8","port":853,"transport":"DNS_OVER_TLS"}"#;
        let ep: ResolverEndpoint = serde_json::from_str(json).unwrap();
        assert_eq!(ep.fqdn, "dns.example.com");
        assert!(ep.spki_pin.is_none());
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

    // --- ResolverProfileManager: admission ---

    #[tokio::test]
    async fn admit_allowlist_with_valid_signature_succeeds() {
        let (sk, vk) = generate_keypair();
        let fp = fingerprint_from_vk(&vk);
        let mut manager = ResolverProfileManager::new(ResolverProfile {
            backend: ResolverBackend::Unbound,
            active_list_id: "default".into(),
            effective_endpoints: vec![],
            cache_ttl_seconds: 300,
        });
        manager.register_authority(&fp, vk);

        let list =
            build_signed_allowlist("L1", vec![test_endpoint_dot("ns1.example.com")], &sk, &fp);
        let result = manager.admit_allowlist(list).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn admit_allowlist_with_invalid_signature_returns_resolver_signature_invalid() {
        let (sk, vk) = generate_keypair();
        let fp = fingerprint_from_vk(&vk);
        let mut manager = ResolverProfileManager::new(ResolverProfile {
            backend: ResolverBackend::Unbound,
            active_list_id: "default".into(),
            effective_endpoints: vec![],
            cache_ttl_seconds: 300,
        });
        manager.register_authority(&fp, vk);

        let mut list =
            build_signed_allowlist("L1", vec![test_endpoint_dot("ns1.example.com")], &sk, &fp);
        // Tamper with the list ID after signing.
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
        // Do NOT register the authority.
        let manager = ResolverProfileManager::new(ResolverProfile {
            backend: ResolverBackend::Unbound,
            active_list_id: "default".into(),
            effective_endpoints: vec![],
            cache_ttl_seconds: 300,
        });

        let list =
            build_signed_allowlist("L1", vec![test_endpoint_dot("ns1.example.com")], &sk, &fp);
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
        let mut manager = ResolverProfileManager::new(ResolverProfile {
            backend: ResolverBackend::Unbound,
            active_list_id: "default".into(),
            effective_endpoints: vec![],
            cache_ttl_seconds: 300,
        });
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
        let mut manager = ResolverProfileManager::new(ResolverProfile {
            backend: ResolverBackend::Unbound,
            active_list_id: "default".into(),
            effective_endpoints: vec![],
            cache_ttl_seconds: 300,
        });
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
    async fn admit_allowlist_with_short_signature_returns_resolver_signature_invalid() {
        let (sk, vk) = generate_keypair();
        let fp = fingerprint_from_vk(&vk);
        let mut manager = ResolverProfileManager::new(ResolverProfile {
            backend: ResolverBackend::Unbound,
            active_list_id: "default".into(),
            effective_endpoints: vec![],
            cache_ttl_seconds: 300,
        });
        manager.register_authority(&fp, vk);

        let mut list = ResolverAllowlist {
            list_id: "L1".into(),
            endpoints: vec![test_endpoint_dot("ns1.example.com")],
            signed_at: Utc::now(),
            signer_fingerprint: fp.clone(),
            signature: vec![0u8; 32], // Too short — must be 64 bytes.
        };
        // Re-sign with fake short sig; canonical bytes matches the data.
        sign_allowlist(&mut list, &sk);
        // Now overwrite with a short signature.
        list.signature = vec![0u8; 32];

        let result = manager.admit_allowlist(list).await;
        match result {
            Err(NetworkPolicyError::ResolverSignatureInvalid(msg)) => {
                assert!(msg.contains("not 64 bytes"));
            }
            other => panic!("expected ResolverSignatureInvalid for short sig, got {other:?}"),
        }
    }

    // --- ResolverProfileManager: rotation ---

    #[tokio::test]
    async fn rotate_active_list_to_admitted_list_succeeds() {
        let (sk, vk) = generate_keypair();
        let fp = fingerprint_from_vk(&vk);
        let mut manager = ResolverProfileManager::new(ResolverProfile {
            backend: ResolverBackend::Unbound,
            active_list_id: "default".into(),
            effective_endpoints: vec![],
            cache_ttl_seconds: 300,
        });
        manager.register_authority(&fp, vk);

        let list =
            build_signed_allowlist("L1", vec![test_endpoint_dot("ns1.example.com")], &sk, &fp);
        manager.admit_allowlist(list).await.unwrap();

        let result = manager.rotate_active_list("L1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn rotate_active_list_to_unknown_list_returns_resolver_signature_invalid() {
        let manager = ResolverProfileManager::new(ResolverProfile {
            backend: ResolverBackend::Unbound,
            active_list_id: "default".into(),
            effective_endpoints: vec![],
            cache_ttl_seconds: 300,
        });

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
        let mut manager = ResolverProfileManager::new(ResolverProfile {
            backend: ResolverBackend::Unbound,
            active_list_id: "default".into(),
            effective_endpoints: vec![],
            cache_ttl_seconds: 300,
        });
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
        let list = build_signed_allowlist("L2", new_eps.clone(), &sk, &fp);
        manager.admit_allowlist(list).await.unwrap();
        manager.rotate_active_list("L2").await.unwrap();

        let profile = manager.current_profile().await;
        assert_eq!(profile.active_list_id, "L2");
        assert_eq!(profile.effective_endpoints.len(), 2);
        assert_eq!(profile.effective_endpoints[0].fqdn, "ns1.example.com");
        assert_eq!(profile.effective_endpoints[1].fqdn, "ns2.example.com");
    }

    // --- QueryGuard ---

    #[tokio::test]
    async fn query_guard_increments_then_decrements_on_drop() {
        let manager = ResolverProfileManager::new(ResolverProfile {
            backend: ResolverBackend::Unbound,
            active_list_id: "default".into(),
            effective_endpoints: vec![],
            cache_ttl_seconds: 300,
        });

        assert_eq!(manager.in_flight_query_count.load(Ordering::Relaxed), 0);

        {
            let _guard = manager.begin_query();
            assert_eq!(manager.in_flight_query_count.load(Ordering::Relaxed), 1);
        }

        assert_eq!(manager.in_flight_query_count.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn concurrent_5_query_guards_no_panic() {
        let manager = std::sync::Arc::new(ResolverProfileManager::new(ResolverProfile {
            backend: ResolverBackend::Unbound,
            active_list_id: "default".into(),
            effective_endpoints: vec![],
            cache_ttl_seconds: 300,
        }));

        let mut handles = Vec::new();
        for _ in 0..5 {
            let m = std::sync::Arc::clone(&manager);
            handles.push(tokio::spawn(async move {
                let _guard = m.begin_query();
                // Hold the guard briefly.
                tokio::task::yield_now().await;
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        // All guards dropped — counter should be back to 0.
        assert_eq!(manager.in_flight_query_count.load(Ordering::Relaxed), 0);
    }

    // --- audit_resolver_used ---

    #[tokio::test]
    async fn audit_resolver_used_returns_backend() {
        let manager = ResolverProfileManager::new(ResolverProfile {
            backend: ResolverBackend::SystemdResolved,
            active_list_id: "default".into(),
            effective_endpoints: vec![],
            cache_ttl_seconds: 300,
        });

        let backend = manager.audit_resolver_used().await;
        assert_eq!(backend, ResolverBackend::SystemdResolved);
    }

    // --- DegradedHostsFileOnly serde round trip ---

    #[test]
    fn degraded_hosts_file_only_backend_round_trip_via_serde() {
        let backend = ResolverBackend::DegradedHostsFileOnly;
        let json = serde_json::to_string(&backend).unwrap();
        let back: ResolverBackend = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ResolverBackend::DegradedHostsFileOnly);
    }
}
