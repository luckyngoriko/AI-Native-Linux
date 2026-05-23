//! Bootstrap helpers for serving the `EvidenceLog` gRPC service (T-011).
//!
//! These helpers are intentionally thin — they exist so that downstream
//! integrators (the §22 MVP harness, the integration test under
//! `tests/grpc_roundtrip.rs`, future production binaries) do not duplicate
//! tonic plumbing. All transport configuration (TLS, compression,
//! interceptors) is left to the caller via the `Router` returned by
//! [`build_router`].
//!
//! ## Discipline
//!
//! - The server takes ownership of the backend (`InMemoryEvidenceLog` is
//!   `Clone`, so callers may keep a copy for direct in-process inspection
//!   in tests).
//! - There is no built-in TLS bootstrap here; the production binary will
//!   call [`build_router`] and chain `.serve_with_incoming(...)` with the
//!   appropriate `tls_config(...)` from the operator's certificate store.
//! - We do not start a Tokio runtime here. Callers are expected to be on
//!   an existing `tokio::main` or `#[tokio::test]` runtime.

use std::net::SocketAddr;

use tonic::transport::server::{Router, Server};

use crate::service::impl_inmemory::InMemoryEvidenceLog;
use crate::service::proto::evidence_log_server::EvidenceLogServer;

/// Build a `tonic::transport::server::Router` with the `EvidenceLog`
/// service mounted.
///
/// The returned router is fluent — callers may chain `.add_service(...)`
/// for additional services (e.g. health, reflection) before calling
/// `.serve(addr)` or `.serve_with_incoming(...)`.
#[must_use]
pub fn build_router(backend: InMemoryEvidenceLog) -> Router {
    Server::builder().add_service(EvidenceLogServer::new(backend))
}

/// Convenience helper: bind to `addr` and serve until the future is dropped.
///
/// This blocks the calling task indefinitely (until the server is shut
/// down by some other signal). For graceful shutdown, prefer
/// `Server::builder().add_service(...).serve_with_shutdown(addr, signal)`
/// directly — the integration test uses an `oneshot` cancellation channel
/// for that purpose.
///
/// # Errors
///
/// Returns the underlying [`tonic::transport::Error`] when the bind / listen
/// loop fails (port in use, permission denied, etc.).
pub async fn serve(
    backend: InMemoryEvidenceLog,
    addr: SocketAddr,
) -> Result<(), tonic::transport::Error> {
    build_router(backend).serve(addr).await
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[tokio::test]
    async fn build_router_compiles_and_accepts_evidence_log_service() {
        // Exercise the public surface — we cannot meaningfully assert on a
        // `Router` value beyond constructing it (tonic does not expose its
        // internals). The compile-time check that `EvidenceLogServer` is a
        // valid `tonic::server::NamedService` is the real assertion.
        let sk = SigningKey::from_bytes(&[5u8; 32]);
        let backend = InMemoryEvidenceLog::new(sk);
        let _router = build_router(backend);
    }
}
