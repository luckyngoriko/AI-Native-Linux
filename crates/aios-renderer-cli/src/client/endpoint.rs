//! Endpoint configuration for renderer gRPC clients.

/// Backend service endpoints consumed by [`crate::AiosClient`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiosEndpoints {
    /// Policy Kernel gRPC endpoint.
    pub policy: String,
    /// Capability Runtime gRPC endpoint.
    pub runtime: String,
    /// AIOS-FS gRPC endpoint.
    pub fs: String,
    /// Vault Broker gRPC endpoint.
    pub vault: String,
    /// Optional Evidence Log gRPC endpoint.
    pub evidence: Option<String>,
}

impl AiosEndpoints {
    /// Return canonical loopback endpoints for the backend service set.
    ///
    /// Evidence has a gRPC service in this repository, so the default endpoint
    /// is populated. The in-process renderer fixture leaves it `None` because
    /// T-061 only starts policy/runtime/fs/vault servers.
    #[must_use]
    pub fn localhost_default() -> Self {
        Self {
            policy: "http://[::1]:50051".to_owned(),
            runtime: "http://[::1]:50052".to_owned(),
            fs: "http://[::1]:50053".to_owned(),
            vault: "http://[::1]:50054".to_owned(),
            evidence: Some("http://[::1]:50055".to_owned()),
        }
    }
}
