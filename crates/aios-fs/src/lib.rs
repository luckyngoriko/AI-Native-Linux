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
//! - [`AiosFs`] async trait, [`SnapshotId`], and the [`InMemoryAiosFs`] harness
//!   for SNAPSHOT-consistent reads and quarantine read denial.
//!
//! Persistence, RPC/proto codegen, transaction execution, and POSIX/FUSE
//! projection are explicitly out of scope for T-036/T-039 and queued for later
//! M5 tasks.

#![forbid(unsafe_code)]

pub mod chunk;
pub mod error;
pub mod fs_trait;
pub mod gc;
mod id;
pub mod impl_space;
pub mod in_memory;
pub mod lifecycle;
pub mod namespace;
pub mod object;
pub mod pointer;
pub mod quarantine;
pub mod query;
pub mod query_eval;
pub mod query_parser;
pub mod snapshot_id;
pub mod transaction;
pub mod version;

pub use chunk::{Chunk, ChunkId, ChunkRef};
pub use error::FsError;
pub use fs_trait::{
    AiosFs, FsContext, ObjectReadResult, ObjectWriteRequest, ObjectWriteResult, SnapshotSummary,
};
pub use gc::{GcPassDriver, GcPassReport, GcReason, VersionPurgeReason};
pub use impl_space::{
    ImplSpace, ImplSpaceBinding, ImplSpaceSource, ImplSpaceTarget, InMemoryImplSpace,
    IntegrityState,
};
pub use in_memory::InMemoryAiosFs;
pub use lifecycle::LifecycleState;
pub use namespace::{AiosPath, NamespaceClass};
pub use object::{
    Object, ObjectId, ObjectInit, ObjectKind, ObjectMetadata, PrivacyClass, ScopeBinding,
    ScopeKind, SubjectRef,
};
pub use pointer::{Pointer, PointerId, PointerKind};
pub use quarantine::{
    MutableAiosFs, QuarantineDisposition, QuarantineDriver, QuarantineReceipt, QuarantineTrigger,
};
pub use query::{Predicate, Query, QueryField, QueryNamespace, QueryOperator, QueryValue};
pub use query_eval::{
    evaluate as evaluate_query, materialize_view, ObjectRef, QueryEvalContext, QueryEvalError, View,
};
pub use query_parser::{parse as parse_query, QueryParseError};
pub use snapshot_id::SnapshotId;
pub use transaction::{
    ConsistencyClass, PointerMoveOp, Transaction, TransactionId, TransactionState, WriteOp,
};
pub use version::{Version, VersionId, VersionState};
