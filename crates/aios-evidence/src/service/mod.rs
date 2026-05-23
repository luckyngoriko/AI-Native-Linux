//! gRPC `EvidenceLog` service surface (T-011, S3.1 §9 / §17 / Appendix A).
//!
//! This module hosts the tonic-generated server + client stubs, the wire ↔
//! Rust value-type conversions, the in-memory reference backend, and the
//! bootstrap helpers used by both the integration tests and (eventually) the
//! production binary.
//!
//! ## Layout
//!
//! - `proto` — verbatim tonic-build output (`tonic::include_proto!`).
//! - [`conversions`] — Rust ↔ proto translations for [`RecordType`],
//!   [`RetentionClass`], and the [`EvidenceReceipt`] envelope.
//! - [`impl_inmemory`] — [`InMemoryEvidenceLog`] backend used by tests and the
//!   §22 MVP golden path. Pure async, no on-disk persistence; the production
//!   backend (`RocksDB` segment writer) is deferred per S3.1 §7.2.
//! - [`server`] — `tower`-based bootstrap helpers: build a `tonic::transport::Router`,
//!   serve it on a `SocketAddr`, etc.
//!
//! ## Spec compliance
//!
//! The proto IDL is the **verbatim Appendix A** declaration. The seven RPCs
//! prescribed in §9 + §17 are all implemented:
//!
//! | RPC            | Spec ref         | Streaming | Backend support              |
//! | -------------- | ---------------- | --------- | ---------------------------- |
//! | `Append`        | §9, §17, §3      | unary     | full                         |
//! | `ReadReceipt`   | §9, §17          | unary     | full (`NotFound` on miss)    |
//! | `Subscribe`     | §9.1, §9.3, §17  | server    | replay from receipt id; live |
//! | `Query`         | §10, §17         | server    | filter + paginate            |
//! | `VerifyChain`   | §11.4, §17       | unary     | full (in-memory)             |
//! | `RebuildIndex`  | §17              | unary     | no-op (no indexes in-memory) |
//! | `GetLogInfo`    | §17              | unary     | full                         |
//!
//! ## Carry-forward
//!
//! - Per-`RecordType` payload one-of construction is **opaque** in T-011: the
//!   in-memory backend stores the JSON payload as-is and the wire `payload`
//!   one-of is left empty on round-trip (`EvidenceReceipt.payload` in the
//!   proto is set to `None`). When Wave 14 reconciles the payload schemas
//!   per S3.1 §29.6, the `conversions.rs` module is the single touch point.
//! - On-disk segment writing (`RocksDB`) is deferred per S3.1 §7.2 / §22
//!   carry-forward. The in-memory backend is the §22 MVP path.
//! - Privacy ceiling enforcement (§10 trailer count) is deferred until S2.1
//!   capability subjects are wired.

pub mod conversions;
pub mod impl_inmemory;
pub mod server;

/// Tonic-generated server/client stubs + proto messages.
///
/// The `include_proto!` macro pulls the file emitted by `build.rs` (via
/// `tonic-build`). Downstream code should depend on the public re-exports
/// below rather than reaching into `proto::*` directly.
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    missing_docs,
    unused_qualifications,
    clippy::default_trait_access,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::empty_line_after_doc_comments,
    clippy::large_enum_variant,
    clippy::missing_const_for_fn,
    clippy::missing_errors_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::needless_borrow,
    clippy::option_option,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    clippy::unused_async,
    clippy::use_self,
    clippy::wildcard_imports
)]
pub mod proto {
    tonic::include_proto!("aios.evidence.v1alpha1");
}

// Re-exports that downstream crates should use.
pub use impl_inmemory::InMemoryEvidenceLog;
pub use proto::evidence_log_client::EvidenceLogClient;
pub use proto::evidence_log_server::{EvidenceLog as EvidenceLogService, EvidenceLogServer};
pub use server::{build_router, serve};
