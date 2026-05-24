//! `aios-fs` — core types for the AIOS-FS object model and namespace layout.
//!
//! This crate implements the **wire-format-agnostic core data model** for the
//! L2 AIOS-FS contracts defined in
//! `002.AI-OS.NET--SPECREV.2/L2_AIOS_FS/01_object_model.md` and
//! `002.AI-OS.NET--SPECREV.2/L2_AIOS_FS/05_namespace_layout.md`.
//!
//! ## Scope of T-036 (M5 opener — types-only skeleton)
//!
//! - [`Object`], [`Version`], [`Chunk`], [`Pointer`], and [`Transaction`] records
//!   per S1.3 §5..§9.
//! - Closed enums for [`ObjectKind`], [`PrivacyClass`], [`VersionState`],
//!   [`PointerKind`], [`LifecycleState`], [`TransactionState`], and
//!   [`ConsistencyClass`].
//! - [`AiosPath`] + [`NamespaceClass`] path classifier for the S4.1 namespace
//!   vocabulary.
//! - [`FsError`] taxonomy for the future reader/writer and transaction driver
//!   tasks.
//!
//! Trait surface, persistence, RPC/proto codegen, transaction execution,
//! quarantine operations, garbage collection, and POSIX/FUSE projection are
//! explicitly out of scope for T-036 and queued for T-037..T-045.

#![forbid(unsafe_code)]

pub mod chunk;
pub mod error;
mod id;
pub mod lifecycle;
pub mod namespace;
pub mod object;
pub mod pointer;
pub mod transaction;
pub mod version;

pub use chunk::{Chunk, ChunkId, ChunkRef};
pub use error::FsError;
pub use lifecycle::LifecycleState;
pub use namespace::{AiosPath, NamespaceClass};
pub use object::{
    Object, ObjectId, ObjectInit, ObjectKind, ObjectMetadata, PrivacyClass, ScopeBinding,
    ScopeKind, SubjectRef,
};
pub use pointer::{Pointer, PointerId, PointerKind};
pub use transaction::{
    ConsistencyClass, PointerMoveOp, Transaction, TransactionId, TransactionState, WriteOp,
};
pub use version::{Version, VersionId, VersionState};
