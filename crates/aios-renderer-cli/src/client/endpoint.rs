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
    /// Verification Engine gRPC endpoint.
    pub verification: String,
    /// Recovery Service gRPC endpoint.
    pub recovery: String,
    /// Service Graph Runtime gRPC endpoint.
    pub sgr: String,
    /// Cognitive Core gRPC endpoint.
    pub cognitive: String,
    /// Sandbox Composer gRPC endpoint.
    pub sandbox: String,
    /// Apps Service gRPC endpoint.
    pub apps: String,
    /// Optional Evidence Log gRPC endpoint.
    pub evidence: Option<String>,
}

impl AiosEndpoints {
    /// Return canonical loopback endpoints for the backend service set.
    ///
    /// Evidence has a gRPC service in this repository, so the default endpoint
    /// is populated. The in-process renderer fixture leaves it `None` because
    /// it starts the eight services needed by the renderer test client.
    #[must_use]
    pub fn localhost_default() -> Self {
        Self {
            policy: "http://[::1]:50051".to_owned(),
            runtime: "http://[::1]:50052".to_owned(),
            fs: "http://[::1]:50053".to_owned(),
            vault: "http://[::1]:50054".to_owned(),
            verification: "http://[::1]:50056".to_owned(),
            recovery: "http://[::1]:50057".to_owned(),
            sgr: "http://[::1]:50058".to_owned(),
            cognitive: "http://[::1]:50059".to_owned(),
            sandbox: "http://[::1]:50060".to_owned(),
            apps: "http://[::1]:50061".to_owned(),
            evidence: Some("http://[::1]:50055".to_owned()),
        }
    }
}
