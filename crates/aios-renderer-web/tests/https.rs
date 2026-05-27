//! T-143 HTTPS server + self-signed cert tests — 14 tests.
//!
//! Verifies cert generation, SAN list invariants, bind-address helpers,
//! `HttpsListener` construction rules, and plain-HTTP rejection metadata.
//! No actual TCP port binding — pure config + metadata layer.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::no_effect_underscore_binding
)]

use std::net::{IpAddr, SocketAddr};

use aios_renderer_web::{
    generate_self_signed_loopback_cert, lan_bind_addrs, loopback_only_bind_addrs,
    plain_http_rejection_response_body, ExposureLevelLabel, HttpsListener, HttpsServerConfig,
    PLAIN_HTTP_REJECTION_STATUS,
};

// ── Helpers ─────────────────────────────────────────────────────────────────

fn dummy_loopback_config(port: u16) -> HttpsServerConfig {
    HttpsServerConfig {
        bind_addrs: loopback_only_bind_addrs(port),
        cert_chain_pem: b"TEST-CERT-PEM".to_vec(),
        key_pem: b"TEST-KEY-PEM".to_vec(),
        san_hosts: vec!["localhost".into()],
    }
}

fn non_loopback_config() -> HttpsServerConfig {
    HttpsServerConfig {
        bind_addrs: vec![SocketAddr::new(IpAddr::from([0, 0, 0, 0]), 8443)],
        cert_chain_pem: b"TEST-CERT-PEM".to_vec(),
        key_pem: b"TEST-KEY-PEM".to_vec(),
        san_hosts: vec!["localhost".into()],
    }
}

// ── Cert generation ─────────────────────────────────────────────────────────

#[test]
fn generate_self_signed_loopback_cert_succeeds() {
    let cert = generate_self_signed_loopback_cert(&[]).unwrap();
    assert!(!cert.cert_chain_pem.is_empty());
    assert!(!cert.key_pem.is_empty());
    assert!(cert.san_hosts.len() >= 4);
}

#[test]
fn generated_cert_san_includes_localhost() {
    let cert = generate_self_signed_loopback_cert(&[]).unwrap();
    assert!(cert.san_hosts.iter().any(|h| h == "localhost"));
}

#[test]
fn generated_cert_san_includes_aios_localhost_wildcard() {
    let cert = generate_self_signed_loopback_cert(&[]).unwrap();
    assert!(cert.san_hosts.iter().any(|h| h == "*.aios.localhost"));
}

#[test]
fn generated_cert_san_includes_extra_san() {
    let cert = generate_self_signed_loopback_cert(&["recovery.localhost"]).unwrap();
    assert!(cert.san_hosts.iter().any(|h| h == "recovery.localhost"));
}

// ── Bind-address helpers ────────────────────────────────────────────────────

#[test]
fn loopback_only_bind_addrs_has_two_entries() {
    let addrs = loopback_only_bind_addrs(8443);
    assert_eq!(addrs.len(), 2);
    assert!(addrs.iter().any(|a| a.ip().to_string() == "127.0.0.1"));
    assert!(addrs.iter().any(|a| a.ip().to_string() == "::1"));
}

#[test]
fn loopback_only_bind_addrs_does_not_include_zero_zero_zero_zero() {
    let addrs = loopback_only_bind_addrs(8443);
    assert!(!addrs.iter().any(|a| a.ip().to_string() == "0.0.0.0"));
}

#[test]
fn lan_bind_addrs_includes_loopback_plus_extras() {
    let extra_ip = IpAddr::from([192, 168, 1, 100]);
    let addrs = lan_bind_addrs(8443, &[extra_ip]);
    assert!(addrs.len() >= 3);
    assert!(addrs.iter().any(|a| a.ip().to_string() == "127.0.0.1"));
    assert!(addrs.iter().any(|a| a.ip().to_string() == "192.168.1.100"));
}

// ── HttpsListener construction ─────────────────────────────────────────────

#[test]
fn https_listener_new_with_loopback_addrs_succeeds() {
    let config = dummy_loopback_config(8443);
    let listener = HttpsListener::new(config).unwrap();
    assert_eq!(listener.bound_addrs().len(), 2);
}

#[test]
fn https_listener_new_with_non_loopback_addr_returns_error() {
    let config = non_loopback_config();
    let result = HttpsListener::new(config);
    assert!(result.is_err());
}

#[test]
fn https_listener_new_with_exposure_lan_active_allows_non_loopback() {
    let config = non_loopback_config();
    let listener = HttpsListener::new_with_exposure(config, ExposureLevelLabel::LanActive).unwrap();
    assert_eq!(listener.bound_addrs().len(), 1);
}

#[test]
fn https_listener_new_with_exposure_localhost_blocks_non_loopback() {
    let config = non_loopback_config();
    let result = HttpsListener::new_with_exposure(config, ExposureLevelLabel::Localhost);
    assert!(result.is_err());
}

// ── Plain-HTTP rejection metadata ───────────────────────────────────────────

#[test]
fn plain_http_rejection_status_is_421() {
    assert_eq!(PLAIN_HTTP_REJECTION_STATUS, 421);
}

#[test]
fn plain_http_rejection_body_mentions_https() {
    let body = plain_http_rejection_response_body();
    assert!(body.to_lowercase().contains("https"));
}

#[test]
fn plain_http_rejection_headers_include_strict_transport_security() {
    let headers = aios_renderer_web::https::PLAIN_HTTP_REJECTION_HEADERS;
    let has_hsts = headers
        .iter()
        .any(|(k, _)| *k == "Strict-Transport-Security");
    assert!(has_hsts);
}
