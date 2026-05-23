//! `aios-evidence` — Evidence Log foundation (S3.1, schema `aios.evidence.v1alpha1`).
//!
//! This crate implements the **wire-format-agnostic core** of the Evidence Log
//! defined in `L9_Observability_Admin_Operations/01_evidence_log.md`. It is the
//! **first consumer of [`aios_action`]** and the place where INV-005 (evidence is
//! append-only) is encoded at the Rust type level.
//!
//! ## Scope of T-007
//!
//! - [`EvidenceReceipt`] envelope per S3.1 §3 (T-007 subset: receipt id, server
//!   timestamp, closed record type, closed retention class, subject canonical id,
//!   optional bound action id, append-only hash-chain pointer, BLAKE3-256 content
//!   hash, opaque JSON payload, Ed25519 signature placeholder).
//! - [`RetentionClass`] closed enum (3 values).
//! - [`RecordType`] closed enum — **30+ variant subset** of the 427-entry
//!   Wave-13-reconciled vocabulary. Full vocabulary queued for T-008.
//! - [`ReceiptBuilder`] consuming-builder that yields immutable receipts.
//! - [`ReceiptChain`] append-only ordered list with link-hash verification.
//! - [`EvidenceError`] closed failure taxonomy.
//! - [`Sealed`] marker trait — type-level encoding of the immutability rule.
//!
//! ## Constitutional invariants enforced here
//!
//! - **INV-005 (evidence append-only).** Sealed receipts have all fields private
//!   and only `&self` accessors. The chain exposes append-only writes.
//! - **No `unsafe`, no `panic!`, no `unwrap`/`expect`** outside test blocks
//!   (workspace lints forbid them).
//! - **BLAKE3 + JCS pinning.** Content hash and link hash both delegate to the
//!   [`aios_action::canonical`] helpers — no fresh hashing primitives are
//!   introduced here, so cross-crate determinism (S0.1 §8.5) holds.
//!
//! ## Deferred to T-008+
//!
//! - Full 428-entry `RecordType` vocabulary (S3.1 Appendix A Wave 13 IDL).
//! - Per-`RecordType` payload schemas (S3.1 §29.6: payload messages deferred to
//!   Wave 14+).
//! - Ed25519 signing path (S3.1 §5.2 / §11.3).
//! - Segment sealing + cross-segment linkage (S3.1 §5.2 / §7).
//! - WAL / `RocksDB` persistence (S3.1 §7.2).
//! - gRPC `EvidenceLog` surface (S3.1 §9 / §17).
//! - Subscription / streaming / query path (S3.1 §9 / §10).
//! - Redaction profiles (S3.1 §14).

#![forbid(unsafe_code)]

pub mod chain;
pub mod error;
pub mod receipt;
pub mod record;
pub mod sealed;
pub mod segment;
pub mod segment_chain;

pub use chain::ReceiptChain;
pub use error::EvidenceError;
pub use receipt::{EvidenceReceipt, ReceiptBuilder};
pub use record::{RecordType, RetentionClass};
pub use sealed::Sealed;
pub use segment::{SealedSegment, Segment, SegmentId};
pub use segment_chain::SegmentChain;
