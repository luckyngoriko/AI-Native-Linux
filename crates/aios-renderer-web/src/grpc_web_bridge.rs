//! gRPC-Web bridge transport (S7.5 gRPC-Web boundary policy).
//!
//! Translates browser-side gRPC-Web calls into in-process tonic gRPC calls
//! against the AIOS backend services. Implements the service allowlist,
//! origin allowlist with `*.` wildcard matching, max-message ceiling,
//! and CORS header builder. Actual hyper wiring lands in T-147.

use crate::error::WebRendererError;

/// gRPC-Web bridge enforcing the S7.5 boundary policy.
///
/// Holds the origin allowlist, service allowlist, and max message size.
/// Every incoming gRPC-Web request must pass all three gates before
/// being forwarded to a backend tonic service.
#[derive(Debug, Clone)]
pub struct GrpcWebBridge {
    allowed_origins: Vec<String>,
    allowed_services: Vec<String>,
    max_message_bytes: u32,
}

/// Configuration for [`GrpcWebBridge`].
#[derive(Debug, Clone)]
pub struct GrpcWebBridgeConfig {
    /// Origins permitted to issue gRPC-Web requests (supports `*.` wildcards).
    pub allowed_origins: Vec<String>,
    /// Fully-qualified gRPC service names permitted through the bridge.
    pub allowed_services: Vec<String>,
    /// Hard ceiling on incoming message size in bytes.
    pub max_message_bytes: u32,
}

impl GrpcWebBridge {
    /// Create a new bridge from the given configuration.
    #[must_use]
    pub fn new(config: GrpcWebBridgeConfig) -> Self {
        Self {
            allowed_origins: config.allowed_origins,
            allowed_services: config.allowed_services,
            max_message_bytes: config.max_message_bytes,
        }
    }

    /// Return a [`GrpcWebBridgeConfig`] suitable for localhost-only
    /// development and recovery-mode operation.
    ///
    /// Origins: `https://aios.localhost`, `https://recovery.localhost`,
    /// and `https://*.aios.localhost`.
    ///
    /// Services: 13 AIOS backend services (`AppsService`, `PolicyKernel`,
    /// `EvidenceLog`, `AiosFs`, `VaultBroker`, `CapabilityRuntime`,
    /// `SgrService`, `SandboxService`, `CognitiveCore`,
    /// `VerificationEngine`, `RecoveryService`,
    /// `KdeRendererService`, `WebRendererService`).
    ///
    /// Max message size: 4 MiB.
    #[must_use]
    pub fn default_localhost_config() -> GrpcWebBridgeConfig {
        GrpcWebBridgeConfig {
            allowed_origins: vec![
                "https://aios.localhost".to_string(),
                "https://recovery.localhost".to_string(),
                "https://*.aios.localhost".to_string(),
            ],
            allowed_services: vec![
                "aios.apps.AppsService".to_string(),
                "aios.policy.PolicyKernel".to_string(),
                "aios.evidence.EvidenceLog".to_string(),
                "aios.fs.AiosFs".to_string(),
                "aios.vault.VaultBroker".to_string(),
                "aios.runtime.CapabilityRuntime".to_string(),
                "aios.sgr.SgrService".to_string(),
                "aios.sandbox.SandboxService".to_string(),
                "aios.cognitive.CognitiveCore".to_string(),
                "aios.verification.VerificationEngine".to_string(),
                "aios.recovery.RecoveryService".to_string(),
                "aios.renderer.kde.KdeRendererService".to_string(),
                "aios.renderer.web.WebRendererService".to_string(),
            ],
            max_message_bytes: 4_194_304,
        }
    }

    /// Check whether the given fully-qualified service name is in the allowlist.
    #[must_use]
    pub fn is_service_allowed(&self, service_fqn: &str) -> bool {
        self.allowed_services.iter().any(|s| s == service_fqn)
    }

    /// Validate an incoming request against all three gates.
    ///
    /// Returns `Ok(())` if the origin is allowed, the service is in the
    /// allowlist, and the message size is within bounds.
    ///
    /// # Errors
    ///
    /// * [`WebRendererError::OriginVerificationFailed`] — origin not in
    ///   [`allowed_origins`](Self::allowed_origins).
    /// * [`WebRendererError::Internal`] — service not in allowlist or
    ///   message exceeds [`max_message_bytes`](Self::max_message_bytes).
    pub fn check_request(
        &self,
        origin: &str,
        service: &str,
        message_size: usize,
    ) -> Result<(), WebRendererError> {
        if !origin_matches_any(origin, &self.allowed_origins) {
            return Err(WebRendererError::OriginVerificationFailed {
                expected_group_id: "grpc-web-allowed-origins".to_string(),
                presented_origin: origin.to_string(),
            });
        }

        if !self.is_service_allowed(service) {
            return Err(WebRendererError::Internal(format!(
                "service not in gRPC-Web allowlist: {service}"
            )));
        }

        if message_size > self.max_message_bytes as usize {
            return Err(WebRendererError::Internal(format!(
                "message exceeds gRPC-Web max size: {message_size} > {max}",
                max = self.max_message_bytes
            )));
        }

        Ok(())
    }

    /// Build CORS response headers for a preflight or actual request from
    /// the given origin.
    ///
    /// Returns the headers if the origin is in the allowlist.
    ///
    /// # Errors
    ///
    /// * [`WebRendererError::OriginVerificationFailed`] — origin not in
    ///   [`allowed_origins`](Self::allowed_origins).
    pub fn cors_headers_for(
        &self,
        origin: &str,
    ) -> Result<Vec<(String, String)>, WebRendererError> {
        if !origin_matches_any(origin, &self.allowed_origins) {
            return Err(WebRendererError::OriginVerificationFailed {
                expected_group_id: "grpc-web-allowed-origins".to_string(),
                presented_origin: origin.to_string(),
            });
        }

        Ok(vec![
            (
                "Access-Control-Allow-Origin".to_string(),
                origin.to_string(),
            ),
            (
                "Access-Control-Allow-Methods".to_string(),
                "POST, OPTIONS".to_string(),
            ),
            (
                "Access-Control-Allow-Headers".to_string(),
                "content-type, x-grpc-web, x-user-agent".to_string(),
            ),
            (
                "Access-Control-Expose-Headers".to_string(),
                "grpc-status, grpc-message".to_string(),
            ),
        ])
    }

    /// Expose the allowed origins for inspection (used by tests and TLS
    /// verifier in T-146).
    #[must_use]
    pub fn allowed_origins(&self) -> &[String] {
        &self.allowed_origins
    }

    /// Expose the max message byte ceiling.
    #[must_use]
    pub const fn max_message_bytes(&self) -> u32 {
        self.max_message_bytes
    }
}

/// Check whether `candidate` matches any pattern in `allowlist`.
///
/// Supports `*.` wildcard matching: `*.aios.localhost` matches
/// `foo.aios.localhost`, `bar.aios.localhost`, etc.  Exact matches are
/// also checked before wildcard expansion.
fn origin_matches_any(candidate: &str, allowlist: &[String]) -> bool {
    allowlist
        .iter()
        .any(|pattern| origin_matches(candidate, pattern))
}

fn origin_matches(candidate: &str, pattern: &str) -> bool {
    if candidate == pattern {
        return true;
    }
    if let Some(pos) = pattern.find("*.") {
        let prefix = &pattern[..pos];
        let suffix = &pattern[pos + 2..];
        return candidate.starts_with(prefix)
            && candidate.ends_with(suffix)
            && candidate.len() > prefix.len() + suffix.len();
    }
    false
}

/// Echo-style gRPC-Web client stub for testing without a real HTTP
/// listener.
///
/// Validates every request through the bridge's `check_request` gate and
/// echoes back the payload. Real tonic `Channel` wiring lands in T-147
/// alongside the Next.js front-end.
#[derive(Debug, Clone)]
pub struct GrpcWebClientStub {
    bridge: GrpcWebBridge,
}

impl GrpcWebClientStub {
    /// Wrap a [`GrpcWebBridge`] in an echo-style client stub.
    #[must_use]
    pub const fn new(bridge: GrpcWebBridge) -> Self {
        Self { bridge }
    }

    /// Validate the request through the bridge and echo the payload.
    ///
    /// This is an echo stub: on success the payload is returned unchanged
    /// so callers can verify the bridge gates without a real backend.
    /// The actual tonic `Channel` wiring lands in T-147.
    ///
    /// # Errors
    ///
    /// Returns the same errors as
    /// [`GrpcWebBridge::check_request`].
    #[allow(clippy::unused_async)]
    pub async fn send(
        &self,
        origin: &str,
        service: &str,
        _method: &str,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, WebRendererError> {
        self.bridge.check_request(origin, service, payload.len())?;
        Ok(payload)
    }
}

/// Convenience re-export of [`GrpcWebBridge::default_localhost_config`].
#[must_use]
pub fn default_localhost_config() -> GrpcWebBridgeConfig {
    GrpcWebBridge::default_localhost_config()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_matches_subdomain() {
        assert!(origin_matches(
            "https://acme.aios.localhost",
            "https://*.aios.localhost"
        ));
    }

    #[test]
    fn wildcard_does_not_match_bare_domain() {
        assert!(!origin_matches(
            "https://aios.localhost",
            "https://*.aios.localhost"
        ));
    }

    #[test]
    fn exact_match_works() {
        assert!(origin_matches(
            "https://aios.localhost",
            "https://aios.localhost"
        ));
    }

    #[test]
    fn wildcard_rejects_partial_match() {
        assert!(!origin_matches(
            "https://not-aios.localhost.evil",
            "https://*.aios.localhost"
        ));
    }
}
