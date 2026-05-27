#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::no_effect_underscore_binding,
    missing_docs,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_renderer_web::{default_localhost_config, GrpcWebBridge, GrpcWebClientStub};

#[test]
fn default_localhost_config_has_loopback_origins() {
    let config = default_localhost_config();
    assert_eq!(config.allowed_origins.len(), 3);
    assert!(config
        .allowed_origins
        .contains(&"https://aios.localhost".to_string()));
    assert!(config
        .allowed_origins
        .contains(&"https://recovery.localhost".to_string()));
    assert!(config
        .allowed_origins
        .contains(&"https://*.aios.localhost".to_string()));
}

#[test]
fn default_localhost_config_has_13_allowed_services() {
    let config = default_localhost_config();
    assert_eq!(config.allowed_services.len(), 13);
}

#[test]
fn default_localhost_config_max_message_4_mib() {
    let config = default_localhost_config();
    assert_eq!(config.max_message_bytes, 4_194_304);
}

#[test]
fn is_service_allowed_for_apps_service_returns_true() {
    let config = default_localhost_config();
    let bridge = GrpcWebBridge::new(config);
    assert!(bridge.is_service_allowed("aios.apps.AppsService"));
}

#[test]
fn is_service_allowed_for_unknown_service_returns_false() {
    let config = default_localhost_config();
    let bridge = GrpcWebBridge::new(config);
    assert!(!bridge.is_service_allowed("evil.service.MalwareService"));
}

#[test]
fn check_request_allowed_origin_allowed_service_succeeds() {
    let config = default_localhost_config();
    let bridge = GrpcWebBridge::new(config);
    let result = bridge.check_request("https://aios.localhost", "aios.apps.AppsService", 1024);
    assert!(result.is_ok());
}

#[test]
fn check_request_disallowed_origin_returns_origin_verification_failed() {
    let config = default_localhost_config();
    let bridge = GrpcWebBridge::new(config);
    let result = bridge.check_request("https://evil.example.com", "aios.apps.AppsService", 1024);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("origin verification failed"),
        "expected origin verification failed, got: {msg}"
    );
}

#[test]
fn check_request_disallowed_service_returns_internal_error() {
    let config = default_localhost_config();
    let bridge = GrpcWebBridge::new(config);
    let result = bridge.check_request(
        "https://aios.localhost",
        "evil.service.MalwareService",
        1024,
    );
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("service not in gRPC-Web allowlist"),
        "expected service-not-allowed error, got: {msg}"
    );
}

#[test]
fn check_request_message_too_large_returns_internal_error() {
    let config = default_localhost_config();
    let max_bytes = config.max_message_bytes;
    let bridge = GrpcWebBridge::new(config);
    let too_big = (max_bytes as usize) + 1;
    let result = bridge.check_request("https://aios.localhost", "aios.apps.AppsService", too_big);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("message exceeds gRPC-Web max size"),
        "expected message-too-large error, got: {msg}"
    );
}

#[test]
fn cors_headers_for_allowed_origin_contain_allow_origin_header() {
    let config = default_localhost_config();
    let bridge = GrpcWebBridge::new(config);
    let headers = bridge
        .cors_headers_for("https://aios.localhost")
        .expect("allowed origin should produce headers");
    let allow_origin = headers
        .iter()
        .find(|(k, _)| k == "Access-Control-Allow-Origin")
        .expect("should contain Access-Control-Allow-Origin");
    assert_eq!(allow_origin.1, "https://aios.localhost");
}

#[test]
fn cors_headers_for_allowed_origin_contain_expose_grpc_status() {
    let config = default_localhost_config();
    let bridge = GrpcWebBridge::new(config);
    let headers = bridge
        .cors_headers_for("https://aios.localhost")
        .expect("allowed origin should produce headers");
    let expose = headers
        .iter()
        .find(|(k, _)| k == "Access-Control-Expose-Headers")
        .expect("should contain Access-Control-Expose-Headers");
    assert!(expose.1.contains("grpc-status"));
    assert!(expose.1.contains("grpc-message"));
}

#[test]
fn cors_headers_for_disallowed_origin_returns_error() {
    let config = default_localhost_config();
    let bridge = GrpcWebBridge::new(config);
    let result = bridge.cors_headers_for("https://evil.example.com");
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("origin verification failed"),
        "expected origin verification failed, got: {msg}"
    );
}

#[test]
fn wildcard_origin_aios_localhost_matches_acme_aios_localhost() {
    let config = default_localhost_config();
    let bridge = GrpcWebBridge::new(config);
    let result = bridge.check_request("https://acme.aios.localhost", "aios.apps.AppsService", 1024);
    assert!(result.is_ok(), "wildcard should match subdomain");
}

#[test]
fn grpc_web_client_stub_echo_with_allowed_request_returns_payload() {
    let config = default_localhost_config();
    let bridge = GrpcWebBridge::new(config);
    let stub = GrpcWebClientStub::new(bridge);
    let payload = b"hello grpc-web".to_vec();
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let result = rt.block_on(stub.send(
        "https://aios.localhost",
        "aios.apps.AppsService",
        "ListApps",
        payload.clone(),
    ));
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), payload);
}

#[test]
fn grpc_web_client_stub_with_disallowed_origin_returns_error() {
    let config = default_localhost_config();
    let bridge = GrpcWebBridge::new(config);
    let stub = GrpcWebClientStub::new(bridge);
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let result = rt.block_on(stub.send(
        "https://evil.example.com",
        "aios.apps.AppsService",
        "ListApps",
        b"payload".to_vec(),
    ));
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("origin verification failed"),
        "expected origin error, got: {msg}"
    );
}
