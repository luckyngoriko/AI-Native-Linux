//! HTTPS server + self-signed cert (S7.5 INV I9 + INV-006).
//!
//! Wire‑boundary types and helpers — certificate generation, loopback‑only
//! bind‑address enforcement, plain‑HTTP rejection metadata. The actual
//! `hyper` run loop and gRPC‑Web bridge land in T‑145.

use chrono::{DateTime, Utc};
use std::net::{IpAddr, SocketAddr};

use crate::error::WebRendererError;
use crate::exposure::ExposureLevelLabel;

// ── HttpsServerConfig ─────────────────────────────────────────────────────

/// Configuration for the HTTPS listener.
///
/// Bundles bind addresses, PEM‑encoded certificate chain and private key,
/// and the SAN host list for the certificate.
#[derive(Debug, Clone)]
pub struct HttpsServerConfig {
    /// Socket addresses the server binds to.
    pub bind_addrs: Vec<SocketAddr>,
    /// PEM‑encoded certificate chain (leaf cert first).
    pub cert_chain_pem: Vec<u8>,
    /// PEM‑encoded private key.
    pub key_pem: Vec<u8>,
    /// Subject Alternative Names present in the certificate.
    pub san_hosts: Vec<String>,
}

// ── GeneratedCert ──────────────────────────────────────────────────────────

/// A freshly‑generated self‑signed certificate with its metadata.
#[derive(Debug, Clone)]
pub struct GeneratedCert {
    /// PEM‑encoded certificate chain.
    pub cert_chain_pem: Vec<u8>,
    /// PEM‑encoded private key.
    pub key_pem: Vec<u8>,
    /// SANs included in the certificate.
    pub san_hosts: Vec<String>,
    /// UTC timestamp of generation.
    pub generated_at: DateTime<Utc>,
}

// ── Certificate generation ─────────────────────────────────────────────────

/// Generate a self‑signed certificate for loopback + AIOS‑internal domains.
///
/// The SAN list always includes `localhost`, `127.0.0.1`, `::1`, and
/// `*.aios.localhost` (INV I9). Extra SANs are appended for recovery or
/// operator‑override paths (e.g. `recovery.localhost`).
///
/// Key type is Ed25519 if the backend supports it, falling back to
/// ECDSA‑P256. Certificate validity follows `rcgen` defaults (typically
/// 365 days for self‑signed).
///
/// # Errors
///
/// Returns [`WebRendererError::CertificateVerificationFailed`] if key
/// generation or self‑signing fails.
pub fn generate_self_signed_loopback_cert(
    extra_sans: &[&str],
) -> Result<GeneratedCert, WebRendererError> {
    let mut san_hosts: Vec<String> = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
        "*.aios.localhost".to_string(),
    ];
    for extra in extra_sans {
        san_hosts.push((*extra).to_string());
    }

    let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)
        .or_else(|_| rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256))
        .map_err(|e| {
            WebRendererError::CertificateVerificationFailed(format!("key generation failed: {e}"))
        })?;

    let params = rcgen::CertificateParams::new(san_hosts.clone()).map_err(|e| {
        WebRendererError::CertificateVerificationFailed(format!("certificate params invalid: {e}"))
    })?;

    let cert_der = params.self_signed(&key_pair).map_err(|e| {
        WebRendererError::CertificateVerificationFailed(format!("self‑sign failed: {e}"))
    })?;

    Ok(GeneratedCert {
        cert_chain_pem: cert_der.pem().into_bytes(),
        key_pem: key_pair.serialize_pem().into_bytes(),
        san_hosts,
        generated_at: Utc::now(),
    })
}

// ── Bind‑address helpers ───────────────────────────────────────────────────

/// Return the loopback‑only bind addresses for a given TCP port (INV‑006).
///
/// Always returns `[127.0.0.1:port, [::1]:port]` — the dual‑stack loopback
/// pair. `0.0.0.0` and wildcard addresses are intentionally excluded.
#[must_use]
pub fn loopback_only_bind_addrs(port: u16) -> Vec<SocketAddr> {
    vec![
        SocketAddr::new(IpAddr::from([127, 0, 0, 1]), port),
        SocketAddr::new(IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]), port),
    ]
}

/// Return bind addresses including loopback **plus** extra interface IPs.
///
/// Used later by T‑144 for the exposure FSM when transitioning to LAN‑active.
/// The loopback pair is always included regardless of the extra list.
#[must_use]
pub fn lan_bind_addrs(port: u16, additional_interfaces: &[IpAddr]) -> Vec<SocketAddr> {
    let mut addrs = loopback_only_bind_addrs(port);
    for ip in additional_interfaces {
        addrs.push(SocketAddr::new(*ip, port));
    }
    addrs
}

// ── Plain‑HTTP rejection metadata ──────────────────────────────────────────

/// Canonical `421 Misdirected Request` response body for plain‑HTTP requests
/// that arrive on the HTTPS port (RFC 7540 §9.1.2).
#[must_use]
pub const fn plain_http_rejection_response_body() -> &'static str {
    "421 Misdirected Request — this port speaks HTTPS only.\n\
     Please retry with `https://`.\n\
     Hint: `Strict-Transport-Security` is enforced (upgrade-insecure-requests).\n"
}

/// HTTP status code returned for plain‑HTTP requests on the HTTPS port.
pub const PLAIN_HTTP_REJECTION_STATUS: u16 = 421;

/// Headers returned with every plain‑HTTP rejection response.
///
/// Includes `Strict-Transport-Security` so that compliant user agents
/// automatically upgrade future requests to HTTPS.
pub const PLAIN_HTTP_REJECTION_HEADERS: &[(&str, &str)] = {
    const H: [(&str, &str); 3] = [
        (
            "Strict-Transport-Security",
            "max-age=31536000; includeSubDomains",
        ),
        ("Content-Type", "text/plain; charset=utf-8"),
        ("Connection", "close"),
    ];
    &H
};

// ── HttpsListener ──────────────────────────────────────────────────────────

/// In‑memory HTTPS listener marker.
///
/// Stores validated bind addresses, certificate metadata, and the current
/// exposure level. Actual `hyper` binding is deferred to T‑145 — this type
/// enforces the INV I6 (loopback‑only default) and INV I9 (TLS‑only)
/// configuration contracts at construction time.
#[derive(Debug, Clone)]
pub struct HttpsListener {
    config: HttpsServerConfig,
    #[allow(dead_code)]
    current_exposure: ExposureLevelLabel,
}

impl HttpsListener {
    /// Create a new `HttpsListener` with loopback‑only enforcement (INV‑006).
    ///
    /// Fails if any bind address is **not** a loopback address — wider
    /// exposure requires `new_with_exposure`.
    ///
    /// # Errors
    ///
    /// Returns [`WebRendererError::ExposureEscalationDenied`] if any bind
    /// address is non‑loopback.
    pub fn new(config: HttpsServerConfig) -> Result<Self, WebRendererError> {
        for addr in &config.bind_addrs {
            if !is_loopback(addr) {
                return Err(WebRendererError::ExposureEscalationDenied {
                    from: ExposureLevelLabel::Localhost,
                    to: ExposureLevelLabel::Localhost,
                    reason: format!(
                        "non‑loopback bind address {addr} requires explicit exposure escalation"
                    ),
                });
            }
        }
        Ok(Self {
            config,
            current_exposure: ExposureLevelLabel::Localhost,
        })
    }

    /// Create a `HttpsListener` with an explicit exposure level.
    ///
    /// If `current_exposure` is `Localhost`, loopback‑only enforcement is
    /// applied. For `LanActive`, `Public`, or other escalated levels,
    /// non‑loopback bind addresses are accepted.
    ///
    /// # Errors
    ///
    /// Returns [`WebRendererError::ExposureEscalationDenied`] if
    /// `current_exposure` is `Localhost` and any bind address is non‑loopback.
    pub fn new_with_exposure(
        config: HttpsServerConfig,
        current_exposure: ExposureLevelLabel,
    ) -> Result<Self, WebRendererError> {
        if current_exposure == ExposureLevelLabel::Localhost {
            for addr in &config.bind_addrs {
                if !is_loopback(addr) {
                    return Err(WebRendererError::ExposureEscalationDenied {
                        from: ExposureLevelLabel::Localhost,
                        to: ExposureLevelLabel::Localhost,
                        reason: format!(
                            "non‑loopback bind address {addr} blocked at current exposure level"
                        ),
                    });
                }
            }
        }
        Ok(Self {
            config,
            current_exposure,
        })
    }

    /// The socket addresses this listener is configured to bind to.
    #[must_use]
    pub fn bound_addrs(&self) -> &[SocketAddr] {
        &self.config.bind_addrs
    }

    /// The SAN host list extracted from the certificate.
    #[must_use]
    pub fn cert_san_hosts(&self) -> &[String] {
        &self.config.san_hosts
    }
}

// ── Internal helpers ───────────────────────────────────────────────────────

const fn is_loopback(addr: &SocketAddr) -> bool {
    match addr.ip() {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}
