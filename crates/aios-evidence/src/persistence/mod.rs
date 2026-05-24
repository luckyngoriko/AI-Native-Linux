//! Persistent storage backends for the gRPC `EvidenceLog` surface (T-012,
//! S3.1 §7.2 / §11.4).
//!
//! ## Backends
//!
//! - [`RocksDbEvidenceLog`] — production-grade column-family-backed store
//!   with WAL durability and crash-recovery on open. Implements the same
//!   [`crate::service::proto::evidence_log_server::EvidenceLog`] trait as
//!   the in-memory reference backend, so the existing
//!   [`crate::service::build_router`] bootstrap works transparently.
//!
//! ## Layout
//!
//! - [`encoding`] — column-family names + key encoders. The single source
//!   of truth for the on-disk shape. Hand-audit changes here against
//!   S3.1 §7.2 before touching the backend.
//! - [`recovery`] — startup-recovery helper types (`OpenSegmentSnapshot`).
//! - [`rocksdb_backend`] — the backend itself.
//!
//! ## Constitutional invariants enforced here
//!
//! - **INV-005 (evidence append-only).** Writes go through `WriteBatch` with
//!   `WAL_sync = true`; reads never expose `&mut` access. The
//!   `RocksDbEvidenceLog` does not surface a "delete receipt" method.
//! - **§7.2 durability.** Every mutating call passes `WriteOptions::set_sync(true)`.
//! - **§11.4 crash recovery.** On open, the sealed-segment chain is walked
//!   and every segment's Ed25519 seal signature + per-receipt signatures
//!   are re-verified; cross-segment linkage is checked end-to-end.

pub mod encoding;
pub mod recovery;
pub mod rocksdb_backend;

pub use recovery::OpenSegmentSnapshot;
pub use rocksdb_backend::RocksDbEvidenceLog;
