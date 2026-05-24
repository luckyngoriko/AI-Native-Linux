//! `RocksDbEvidenceLog` — persistent backend for the gRPC `EvidenceLog`
//! surface (T-012, S3.1 §7.2 / §11.4).
//!
//! The backend implements the same
//! [`crate::service::proto::evidence_log_server::EvidenceLog`] trait as the
//! in-memory reference backend, so the existing tonic server bootstrap +
//! integration tests work without modification — only the constructor
//! differs.
//!
//! ## On-disk layout (§7.2)
//!
//! Six `RocksDB` column families, each owning a slice of the persistence
//! contract:
//!
//! - `receipts` — every appended receipt, JSON-encoded, keyed by raw
//!   `evr_<ULID>` bytes (ULID is lex-sortable so the CF is naturally
//!   time-ordered).
//! - `segments` — sealed segment JSON, keyed by `seg_<32hex>`.
//! - `current_segment` — single row holding the in-progress segment
//!   snapshot ([`crate::persistence::recovery::OpenSegmentSnapshot`]);
//!   rewritten atomically on every successful append.
//! - `metadata` — chain-head pointer, schema version, log id.
//! - `chain_index` — `u64` big-endian chain position → `seg_<32hex>` for
//!   O(log n) walks of the sealed segment chain.
//! - `record_type_index` — composite (record type, recorded-at nanos,
//!   receipt id) keys for fast time-range scans.
//!
//! See [`crate::persistence::encoding`] for the exact byte layout of each
//! key.
//!
//! ## Durability (§7.2)
//!
//! - `WAL_sync = true` on every write — receipts MUST survive power loss.
//! - `WAL_recovery_mode = AbsoluteConsistency` — refuse to start on WAL
//!   corruption; the operator runs the recovery toolkit (separate task).
//!
//! ## Crash recovery (§11.4)
//!
//! On open we:
//!
//! 1. Open all six column families. Missing CFs in older on-disk databases
//!    are auto-created (this also covers the fresh-database path).
//! 2. Walk `chain_index` from position `0..chain_head`, fetching each segment
//!    from the `segments` CF and verifying its seal hash + Ed25519
//!    signature against the supplied verifying key. Cross-segment chain
//!    linkage (§5.2 line 193) is checked between adjacent segments.
//! 3. Restore the open-segment snapshot from `current_segment::current`, if
//!    present, and run [`crate::chain::ReceiptChain::verify_integrity`] over
//!    its receipts.
//! 4. Surface any verification failure as the corresponding
//!    [`crate::EvidenceError`] variant. The on-disk state is **not** modified
//!    by the recovery walk.
//!
//! ## Atomic writes
//!
//! Append is one `WriteBatch` touching three CFs:
//! - `receipts::<id>` ← JSON-encoded receipt
//! - `record_type_index::<composite-key>` ← empty
//! - `current_segment::current` ← updated snapshot
//!
//! Seal is one `WriteBatch` touching four CFs:
//! - `segments::<seg_id>` ← JSON sealed segment
//! - `chain_index::<position_be>` ← seg id bytes
//! - `metadata::chain_head` ← new position (`u64` big-endian)
//! - `current_segment::current` ← cleared (empty snapshot for the next segment)
//!
//! `WriteOptions::set_sync(true)` is passed to every `db.write_opt(...)`
//! call so an `fsync` follows the WAL append.

use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rocksdb::{
    ColumnFamilyDescriptor, DBCompressionType, IteratorMode, Options, ReadOptions, WriteBatch,
    WriteOptions, DB,
};
use tokio::sync::{broadcast, RwLock};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status};

use crate::chain::ReceiptChain;
use crate::persistence::encoding::{
    chain_position_key, receipt_key, record_type_index_key, segment_key, ALL_COLUMN_FAMILIES,
    CF_CHAIN_INDEX, CF_CURRENT_SEGMENT, CF_METADATA, CF_RECEIPTS, CF_RECORD_TYPE_INDEX,
    CF_SEGMENTS, CURRENT_SEGMENT_KEY, METADATA_CHAIN_HEAD, METADATA_LOG_ID,
    METADATA_SCHEMA_VERSION, SCHEMA_VERSION_V1ALPHA1,
};
use crate::persistence::recovery::OpenSegmentSnapshot;
use crate::receipt::{EvidenceReceipt, ReceiptBuilder};
use crate::record::RecordType;
use crate::segment::{Segment, SegmentId};
use crate::service::conversions::{
    receipt_to_proto, record_type_from_proto_i32, DEFAULT_RETENTION,
};
use crate::service::proto;
use crate::EvidenceError;

/// Default broadcast channel capacity for `Subscribe`. Mirrors the in-memory
/// backend (§9.3 per-subscriber buffer).
const DEFAULT_SUBSCRIBE_BROADCAST_CAPACITY: usize = 1024;

/// Default per-response `Query` page size (proto3 default treated as 1000).
const DEFAULT_QUERY_LIMIT: u32 = 1000;

/// Persistent RocksDB-backed `EvidenceLog` backend.
///
/// Cloning is cheap (`Arc<DB>` + `Arc<RwLock<...>>`). The in-memory caches
/// (`current_snapshot`, `chain_head`) are kept in sync with the on-disk
/// state by holding the write half of the lock for the duration of every
/// mutating operation.
#[derive(Clone)]
pub struct RocksDbEvidenceLog {
    db: Arc<DB>,
    state: Arc<RwLock<BackendState>>,
    signing_key: Arc<SigningKey>,
    live_tx: broadcast::Sender<proto::EvidenceReceipt>,
    started_at: chrono::DateTime<Utc>,
    log_id: String,
}

impl core::fmt::Debug for RocksDbEvidenceLog {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // RocksDB's `DB` is not `Debug`; render the operationally useful
        // bits only (log id + started_at + live-subscriber count).
        f.debug_struct("RocksDbEvidenceLog")
            .field("log_id", &self.log_id)
            .field("started_at", &self.started_at)
            .field("live_subscribers", &self.live_tx.receiver_count())
            .finish_non_exhaustive()
    }
}

/// Volatile, in-memory state mirroring the persisted view. Rebuilt from disk
/// on every [`RocksDbEvidenceLog::open`].
struct BackendState {
    /// Open (in-progress) segment receipts loaded from `current_segment::current`.
    /// The vector is the canonical source of truth for `Subscribe` replay and
    /// `Query` scans; the on-disk row is rewritten on every mutation.
    current: OpenSegmentSnapshot,
    /// Sealed-segment count = next chain position.
    chain_head: u64,
}

impl RocksDbEvidenceLog {
    /// Open (or create) the persistent evidence log at `path`.
    ///
    /// The directory is created if missing. All six column families are
    /// auto-created on first open. Crash recovery is run as part of this
    /// call (S3.1 §11.4); see the module-level doc for the procedure.
    ///
    /// # Errors
    ///
    /// - [`EvidenceError::EncodingFailed`] when `RocksDB` returns an I/O
    ///   error from the open / iteration calls, when JSON deserialization
    ///   of a persisted receipt or segment fails, or when the on-disk
    ///   schema version is unknown.
    /// - [`EvidenceError::ChainBroken`] / [`EvidenceError::SegmentChainBroken`]
    ///   / [`EvidenceError::SegmentSealMismatch`] /
    ///   [`EvidenceError::SegmentSignatureMismatch`] when the persisted state
    ///   fails cryptographic verification (constitutional tamper signals per
    ///   §11.5).
    pub fn open(path: impl AsRef<Path>, signing_key: SigningKey) -> Result<Self, EvidenceError> {
        let db = Self::open_raw_db(path.as_ref())?;
        Self::validate_or_stamp_schema_version(&db)?;
        let log_id = Self::restore_or_mint_log_id(&db)?;
        let chain_head = Self::read_chain_head(&db)?;

        // Recovery walk over the sealed segment chain. Per-segment seal
        // signatures, per-receipt signatures, cross-segment linkage —
        // defence-in-depth per §11.3.
        let vk = signing_key.verifying_key();
        Self::recover_sealed_chain(&db, chain_head, &vk)?;

        // Restore the open-segment snapshot, if any.
        let current = Self::restore_open_snapshot(&db)?;

        let (live_tx, _live_rx) = broadcast::channel(DEFAULT_SUBSCRIBE_BROADCAST_CAPACITY);

        Ok(Self {
            db: Arc::new(db),
            state: Arc::new(RwLock::new(BackendState {
                current,
                chain_head,
            })),
            signing_key: Arc::new(signing_key),
            live_tx,
            started_at: Utc::now(),
            log_id,
        })
    }

    /// Open the underlying `RocksDB` handle with the AIOS-specific options
    /// (WAL sync, `AbsoluteConsistency` recovery, LZ4 compression).
    fn open_raw_db(path: &Path) -> Result<DB, EvidenceError> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);
        // §7.2 WAL discipline: refuse to start on WAL corruption. The default
        // `PointInTime` mode silently drops the tail of a corrupted WAL —
        // that would violate the evidence-survives-power-loss invariant.
        db_opts.set_wal_recovery_mode(rocksdb::DBRecoveryMode::AbsoluteConsistency);
        // Crate features select `snappy` + `lz4`; LZ4 is the per-CF default —
        // well balanced for JSON receipt payloads.
        db_opts.set_compression_type(DBCompressionType::Lz4);

        let cf_descriptors: Vec<ColumnFamilyDescriptor> = ALL_COLUMN_FAMILIES
            .iter()
            .map(|name| {
                let mut cf_opts = Options::default();
                cf_opts.set_compression_type(DBCompressionType::Lz4);
                ColumnFamilyDescriptor::new(*name, cf_opts)
            })
            .collect();

        DB::open_cf_descriptors(&db_opts, path, cf_descriptors)
            .map_err(|e| EvidenceError::EncodingFailed(format!("rocksdb open: {e}")))
    }

    /// Validate the persisted schema version row, or stamp the current one
    /// on first open.
    fn validate_or_stamp_schema_version(db: &DB) -> Result<(), EvidenceError> {
        let cf_metadata = db
            .cf_handle(CF_METADATA)
            .ok_or_else(|| EvidenceError::EncodingFailed("metadata CF missing".to_owned()))?;
        let existing_schema = db
            .get_cf(&cf_metadata, METADATA_SCHEMA_VERSION)
            .map_err(|e| EvidenceError::EncodingFailed(format!("read schema_version: {e}")))?;
        if let Some(bytes) = existing_schema {
            let s = std::str::from_utf8(&bytes)
                .map_err(|e| EvidenceError::EncodingFailed(format!("schema_version utf8: {e}")))?;
            if s != SCHEMA_VERSION_V1ALPHA1 {
                return Err(EvidenceError::EncodingFailed(format!(
                    "unknown on-disk schema version `{s}`; expected `{SCHEMA_VERSION_V1ALPHA1}`"
                )));
            }
            return Ok(());
        }
        let mut wopts = WriteOptions::default();
        wopts.set_sync(true);
        db.put_cf_opt(
            &cf_metadata,
            METADATA_SCHEMA_VERSION,
            SCHEMA_VERSION_V1ALPHA1.as_bytes(),
            &wopts,
        )
        .map_err(|e| EvidenceError::EncodingFailed(format!("write schema_version: {e}")))?;
        Ok(())
    }

    /// Read the persisted `log_id` or mint a fresh one (first open) and
    /// persist it so subsequent opens see the same value.
    fn restore_or_mint_log_id(db: &DB) -> Result<String, EvidenceError> {
        let cf_metadata = db
            .cf_handle(CF_METADATA)
            .ok_or_else(|| EvidenceError::EncodingFailed("metadata CF missing".to_owned()))?;
        let existing = db
            .get_cf(&cf_metadata, METADATA_LOG_ID)
            .map_err(|e| EvidenceError::EncodingFailed(format!("read log_id: {e}")))?;
        if let Some(bytes) = existing {
            return String::from_utf8(bytes)
                .map_err(|e| EvidenceError::EncodingFailed(format!("log_id utf8: {e}")));
        }
        let id = format!(
            "aios-evidence-log/{}",
            aios_action::EvidenceReceiptId::new().as_str()
        );
        let mut wopts = WriteOptions::default();
        wopts.set_sync(true);
        db.put_cf_opt(&cf_metadata, METADATA_LOG_ID, id.as_bytes(), &wopts)
            .map_err(|e| EvidenceError::EncodingFailed(format!("write log_id: {e}")))?;
        Ok(id)
    }

    /// Read the chain head (count of sealed segments) from metadata. Returns
    /// `0` when no row exists (fresh database).
    fn read_chain_head(db: &DB) -> Result<u64, EvidenceError> {
        let cf_metadata = db
            .cf_handle(CF_METADATA)
            .ok_or_else(|| EvidenceError::EncodingFailed("metadata CF missing".to_owned()))?;
        let bytes_opt = db
            .get_cf(&cf_metadata, METADATA_CHAIN_HEAD)
            .map_err(|e| EvidenceError::EncodingFailed(format!("read chain_head: {e}")))?;
        match bytes_opt {
            Some(bytes) if bytes.len() == 8 => {
                let mut buf = [0u8; 8];
                buf.copy_from_slice(&bytes);
                Ok(u64::from_be_bytes(buf))
            }
            Some(_) => Err(EvidenceError::EncodingFailed(
                "chain_head value has unexpected length".to_owned(),
            )),
            None => Ok(0_u64),
        }
    }

    /// Restore the open-segment snapshot from `current_segment::current`.
    /// Validates the in-progress chain integrity before returning.
    fn restore_open_snapshot(db: &DB) -> Result<OpenSegmentSnapshot, EvidenceError> {
        let cf_current = db.cf_handle(CF_CURRENT_SEGMENT).ok_or_else(|| {
            EvidenceError::EncodingFailed("current_segment CF missing".to_owned())
        })?;
        let bytes_opt = db
            .get_cf(&cf_current, CURRENT_SEGMENT_KEY)
            .map_err(|e| EvidenceError::EncodingFailed(format!("read current_segment: {e}")))?;
        let Some(bytes) = bytes_opt else {
            return Ok(OpenSegmentSnapshot::new(DEFAULT_RETENTION));
        };
        let snap: OpenSegmentSnapshot = serde_json::from_slice(&bytes)
            .map_err(|e| EvidenceError::EncodingFailed(format!("current_segment JSON: {e}")))?;
        if !snap.receipts.is_empty() {
            let mut chain = ReceiptChain::new();
            for r in &snap.receipts {
                chain.append(r.clone())?;
            }
            chain.verify_integrity()?;
        }
        Ok(snap)
    }

    /// Walk the persisted sealed-segment chain and verify every signature +
    /// cross-segment link. Called from [`Self::open`].
    fn recover_sealed_chain(
        db: &DB,
        chain_head: u64,
        verifying_key: &VerifyingKey,
    ) -> Result<(), EvidenceError> {
        if chain_head == 0 {
            return Ok(());
        }
        let cf_chain_index = db
            .cf_handle(CF_CHAIN_INDEX)
            .ok_or_else(|| EvidenceError::EncodingFailed("chain_index CF missing".to_owned()))?;
        let cf_segments = db
            .cf_handle(CF_SEGMENTS)
            .ok_or_else(|| EvidenceError::EncodingFailed("segments CF missing".to_owned()))?;

        let mut prev_seal_hash: Option<String> = None;
        let mut prev_segment_id: Option<SegmentId> = None;

        for pos in 0..chain_head {
            let key = chain_position_key(pos);
            let seg_id_bytes = db
                .get_cf(&cf_chain_index, key)
                .map_err(|e| {
                    EvidenceError::EncodingFailed(format!("read chain_index[{pos}]: {e}"))
                })?
                .ok_or_else(|| EvidenceError::SegmentChainBroken {
                    index: usize::try_from(pos).unwrap_or(usize::MAX),
                    actual: "<missing chain_index row>".to_owned(),
                    expected: "<sealed segment id>".to_owned(),
                })?;
            let seg_id_str = std::str::from_utf8(&seg_id_bytes).map_err(|e| {
                EvidenceError::EncodingFailed(format!("chain_index[{pos}] utf8: {e}"))
            })?;
            let segment_id = SegmentId::parse(seg_id_str)?;
            let seg_bytes = db
                .get_cf(&cf_segments, segment_id.as_str().as_bytes())
                .map_err(|e| {
                    EvidenceError::EncodingFailed(format!("read segments[{seg_id_str}]: {e}"))
                })?
                .ok_or_else(|| EvidenceError::SegmentChainBroken {
                    index: usize::try_from(pos).unwrap_or(usize::MAX),
                    actual: format!("<missing segments row for `{seg_id_str}`>"),
                    expected: "<sealed segment JSON>".to_owned(),
                })?;
            let sealed: crate::SealedSegment = serde_json::from_slice(&seg_bytes).map_err(|e| {
                EvidenceError::EncodingFailed(format!("segments[{seg_id_str}] JSON: {e}"))
            })?;

            // Cross-segment chain check.
            match (pos, sealed.previous_segment_seal_hash(), &prev_seal_hash) {
                (0, None, _) => {
                    // Genesis segment — must carry no previous link.
                }
                (0, Some(_), _) => {
                    return Err(EvidenceError::SegmentChainBroken {
                        index: 0,
                        actual: "<unexpected previous_segment_seal_hash on genesis>".to_owned(),
                        expected: "<none>".to_owned(),
                    });
                }
                (_, Some(claimed), Some(expected)) if claimed == expected => {
                    // Linked correctly.
                }
                (i, claimed, expected) => {
                    return Err(EvidenceError::SegmentChainBroken {
                        index: usize::try_from(i).unwrap_or(usize::MAX),
                        actual: claimed.unwrap_or("<missing>").to_owned(),
                        expected: expected.clone().unwrap_or_else(|| "<missing>".to_owned()),
                    });
                }
            }
            if pos > 0 {
                // The persisted previous_segment_id must equal our running prev_segment_id.
                match (sealed.previous_segment_id(), &prev_segment_id) {
                    (Some(p), Some(expected)) if p == expected => {}
                    _ => {
                        return Err(EvidenceError::SegmentChainBroken {
                            index: usize::try_from(pos).unwrap_or(usize::MAX),
                            actual: sealed
                                .previous_segment_id()
                                .map_or_else(|| "<missing>".to_owned(), |s| s.as_str().to_owned()),
                            expected: prev_segment_id
                                .as_ref()
                                .map_or_else(|| "<missing>".to_owned(), |s| s.as_str().to_owned()),
                        });
                    }
                }
            }

            // Per-segment seal + per-receipt signature verification.
            sealed.verify_full(verifying_key)?;

            prev_seal_hash = Some(sealed.segment_seal_hash().to_owned());
            prev_segment_id = Some(sealed.segment_id().clone());
        }
        Ok(())
    }

    /// The Ed25519 verifying key derived from this backend's signing key.
    /// Used by tests and operators to verify receipts produced here.
    #[must_use]
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Stable identifier of this evidence log instance. Persisted across
    /// restarts.
    #[must_use]
    pub fn log_id(&self) -> &str {
        &self.log_id
    }

    /// Read a `RocksDB` property string (e.g. `"rocksdb.stats"`). Useful
    /// for telemetry / `GetLogInfo`-style introspection. Returns `None`
    /// when the property is unknown or the call fails.
    #[must_use]
    pub fn rocksdb_property(&self, property: &str) -> Option<String> {
        self.db.property_value(property).ok().flatten()
    }

    /// Total number of receipts currently in the open (in-progress) segment.
    /// Sealed-segment receipts are not counted (they are persisted under
    /// their segment id and not in the open snapshot).
    pub async fn receipt_count(&self) -> usize {
        self.state.read().await.current.len()
    }

    /// Number of sealed segments persisted on disk.
    pub async fn sealed_segment_count(&self) -> u64 {
        self.state.read().await.chain_head
    }

    // ----- Internal helpers -----

    /// Persist a freshly minted receipt atomically: write the receipt row,
    /// the record-type index row, and the updated open-segment snapshot in
    /// one [`WriteBatch`] with `WAL_sync = true`.
    fn persist_append(
        &self,
        receipt: &EvidenceReceipt,
        snapshot: &OpenSegmentSnapshot,
    ) -> Result<(), EvidenceError> {
        let cf_receipts = self
            .db
            .cf_handle(CF_RECEIPTS)
            .ok_or_else(|| EvidenceError::EncodingFailed("receipts CF missing".to_owned()))?;
        let cf_idx = self.db.cf_handle(CF_RECORD_TYPE_INDEX).ok_or_else(|| {
            EvidenceError::EncodingFailed("record_type_index CF missing".to_owned())
        })?;
        let cf_current = self.db.cf_handle(CF_CURRENT_SEGMENT).ok_or_else(|| {
            EvidenceError::EncodingFailed("current_segment CF missing".to_owned())
        })?;

        let receipt_bytes = serde_json::to_vec(receipt)
            .map_err(|e| EvidenceError::EncodingFailed(format!("receipt JSON: {e}")))?;
        let snapshot_bytes = serde_json::to_vec(snapshot)
            .map_err(|e| EvidenceError::EncodingFailed(format!("snapshot JSON: {e}")))?;
        let idx_key = record_type_index_key(
            receipt.record_type(),
            receipt.recorded_at(),
            receipt.receipt_id(),
        );

        let mut batch = WriteBatch::default();
        batch.put_cf(
            &cf_receipts,
            receipt_key(receipt.receipt_id()),
            &receipt_bytes,
        );
        batch.put_cf(&cf_idx, &idx_key, []);
        batch.put_cf(&cf_current, CURRENT_SEGMENT_KEY, &snapshot_bytes);

        let mut wopts = WriteOptions::default();
        wopts.set_sync(true);
        self.db
            .write_opt(batch, &wopts)
            .map_err(|e| EvidenceError::EncodingFailed(format!("rocksdb write_opt append: {e}")))
    }

    /// Atomically persist a freshly sealed segment: write the segment row,
    /// the chain-index row, the updated chain head, and clear the open
    /// snapshot.
    fn persist_seal(
        &self,
        sealed: &crate::SealedSegment,
        new_chain_head: u64,
        cleared_snapshot: &OpenSegmentSnapshot,
    ) -> Result<(), EvidenceError> {
        let cf_segments = self
            .db
            .cf_handle(CF_SEGMENTS)
            .ok_or_else(|| EvidenceError::EncodingFailed("segments CF missing".to_owned()))?;
        let cf_chain = self
            .db
            .cf_handle(CF_CHAIN_INDEX)
            .ok_or_else(|| EvidenceError::EncodingFailed("chain_index CF missing".to_owned()))?;
        let cf_meta = self
            .db
            .cf_handle(CF_METADATA)
            .ok_or_else(|| EvidenceError::EncodingFailed("metadata CF missing".to_owned()))?;
        let cf_current = self.db.cf_handle(CF_CURRENT_SEGMENT).ok_or_else(|| {
            EvidenceError::EncodingFailed("current_segment CF missing".to_owned())
        })?;

        let seg_bytes = serde_json::to_vec(sealed)
            .map_err(|e| EvidenceError::EncodingFailed(format!("sealed segment JSON: {e}")))?;
        let snap_bytes = serde_json::to_vec(cleared_snapshot)
            .map_err(|e| EvidenceError::EncodingFailed(format!("snapshot JSON: {e}")))?;
        // chain_index stores the chain_head-1 row (the position the new
        // segment was just appended at).
        let pos = new_chain_head.saturating_sub(1);
        let pos_key = chain_position_key(pos);

        let mut batch = WriteBatch::default();
        batch.put_cf(&cf_segments, segment_key(sealed.segment_id()), &seg_bytes);
        batch.put_cf(&cf_chain, pos_key, sealed.segment_id().as_str().as_bytes());
        batch.put_cf(&cf_meta, METADATA_CHAIN_HEAD, new_chain_head.to_be_bytes());
        batch.put_cf(&cf_current, CURRENT_SEGMENT_KEY, &snap_bytes);

        let mut wopts = WriteOptions::default();
        wopts.set_sync(true);
        self.db
            .write_opt(batch, &wopts)
            .map_err(|e| EvidenceError::EncodingFailed(format!("rocksdb write_opt seal: {e}")))
    }

    /// Seal the in-memory open segment, persist the resulting `SealedSegment`,
    /// advance the chain head, and clear the open-segment snapshot.
    ///
    /// Cross-segment linkage is read from the previous sealed segment when
    /// present (we look up `chain_index[chain_head - 1]` and pull its seal
    /// hash + id from the `segments` CF).
    ///
    /// # Errors
    ///
    /// - [`EvidenceError::EmptySegment`] when the open segment carries no
    ///   receipts.
    /// - [`EvidenceError::EncodingFailed`] on `RocksDB` I/O or JSON
    ///   projection failure.
    /// - Any error returned by [`Segment::seal`].
    #[allow(
        clippy::significant_drop_tightening,
        reason = "the write guard is held intentionally across the seal+persist+\
                  in-memory-state-update cycle to keep the on-disk and in-memory \
                  views atomic w.r.t. concurrent appends"
    )]
    pub async fn seal_current_segment(&self) -> Result<crate::SealedSegment, EvidenceError> {
        let mut guard = self.state.write().await;
        if guard.current.receipts.is_empty() {
            return Err(EvidenceError::EmptySegment);
        }
        // Build a transient Segment from the snapshot.
        let retention = guard.current.retention_class;
        let mut segment = Segment::new(retention);
        for r in &guard.current.receipts {
            segment.append(r.clone())?;
        }
        // Resolve the previous segment's id + seal hash via the on-disk view.
        let (prev_id, prev_seal): (Option<SegmentId>, Option<String>) = if guard.chain_head == 0 {
            (None, None)
        } else {
            let cf_chain = self.db.cf_handle(CF_CHAIN_INDEX).ok_or_else(|| {
                EvidenceError::EncodingFailed("chain_index CF missing".to_owned())
            })?;
            let cf_segments = self
                .db
                .cf_handle(CF_SEGMENTS)
                .ok_or_else(|| EvidenceError::EncodingFailed("segments CF missing".to_owned()))?;
            let prev_pos = chain_position_key(guard.chain_head - 1);
            let id_bytes = self
                .db
                .get_cf(&cf_chain, prev_pos)
                .map_err(|e| EvidenceError::EncodingFailed(format!("read chain_index[prev]: {e}")))?
                .ok_or_else(|| {
                    EvidenceError::EncodingFailed(
                        "chain_index[prev] row missing during seal".to_owned(),
                    )
                })?;
            let id_str = std::str::from_utf8(&id_bytes).map_err(|e| {
                EvidenceError::EncodingFailed(format!("chain_index[prev] utf8: {e}"))
            })?;
            let prev_segment_id = SegmentId::parse(id_str)?;
            let seg_bytes = self
                .db
                .get_cf(&cf_segments, prev_segment_id.as_str().as_bytes())
                .map_err(|e| EvidenceError::EncodingFailed(format!("read segments[prev]: {e}")))?
                .ok_or_else(|| {
                    EvidenceError::EncodingFailed(
                        "segments[prev] row missing during seal".to_owned(),
                    )
                })?;
            let prev_sealed: crate::SealedSegment = serde_json::from_slice(&seg_bytes)
                .map_err(|e| EvidenceError::EncodingFailed(format!("segments[prev] JSON: {e}")))?;
            (
                Some(prev_segment_id),
                Some(prev_sealed.segment_seal_hash().to_owned()),
            )
        };

        let sealed = segment.seal(prev_id.as_ref(), prev_seal.as_deref(), &self.signing_key)?;

        let new_head = guard.chain_head.saturating_add(1);
        let cleared = OpenSegmentSnapshot::new(retention);
        self.persist_seal(&sealed, new_head, &cleared)?;
        guard.chain_head = new_head;
        guard.current = cleared;
        Ok(sealed)
    }
}

#[async_trait]
#[allow(
    clippy::result_large_err,
    reason = "tonic::Status is the canonical gRPC error type (176 bytes); \
              the lint is irrelevant for a generated service surface"
)]
impl proto::evidence_log_server::EvidenceLog for RocksDbEvidenceLog {
    // -----------------------------------------------------------------
    // Append
    // -----------------------------------------------------------------

    async fn append(
        &self,
        request: Request<proto::AppendRequest>,
    ) -> Result<Response<proto::EvidenceReceipt>, Status> {
        let req = request.into_inner();
        let record_type = record_type_from_proto_i32(req.record_type)?;
        let subject = req.subject;
        if subject.trim().is_empty() {
            return Err(Status::invalid_argument(
                "AppendRequest.subject must be non-empty (S5.1 canonical id)",
            ));
        }
        let mut builder = ReceiptBuilder::new(record_type, DEFAULT_RETENTION, subject);
        if !req.action_id.is_empty() {
            let action_id = aios_action::ActionId::parse(&req.action_id)
                .map_err(|e| Status::invalid_argument(format!("invalid action_id: {e}")))?;
            builder = builder.with_action_id(action_id);
        }

        let mut guard = self.state.write().await;
        let previous = guard.current.receipts.last().cloned();
        let receipt = builder
            .seal_signed(previous.as_ref(), &self.signing_key)
            .map_err(Status::from)?;
        // Append to the open snapshot first (validated via chain linkage on
        // open / VerifyChain).
        guard.current.receipts.push(receipt.clone());
        // Persist atomically. On failure, roll back the in-memory snapshot.
        if let Err(e) = self.persist_append(&receipt, &guard.current) {
            guard.current.receipts.pop();
            return Err(Status::from(e));
        }
        drop(guard);

        let wire = receipt_to_proto(&receipt);
        let _ = self.live_tx.send(wire.clone());
        Ok(Response::new(wire))
    }

    // -----------------------------------------------------------------
    // ReadReceipt
    // -----------------------------------------------------------------

    async fn read_receipt(
        &self,
        request: Request<proto::ReadReceiptRequest>,
    ) -> Result<Response<proto::EvidenceReceipt>, Status> {
        let id = request.into_inner().receipt_id;
        if id.is_empty() {
            return Err(Status::invalid_argument(
                "ReadReceiptRequest.receipt_id must be non-empty",
            ));
        }
        // Look up in the open snapshot first (fastest path for hot receipts).
        let hit_open = {
            let guard = self.state.read().await;
            guard
                .current
                .receipts
                .iter()
                .find(|r| r.receipt_id().as_str() == id)
                .map(receipt_to_proto)
        };
        if let Some(p) = hit_open {
            return Ok(Response::new(p));
        }
        // Fall back to the `receipts` CF on disk.
        let cf_receipts = self
            .db
            .cf_handle(CF_RECEIPTS)
            .ok_or_else(|| Status::internal("receipts CF missing"))?;
        match self
            .db
            .get_cf(&cf_receipts, id.as_bytes())
            .map_err(|e| Status::internal(format!("rocksdb read receipt: {e}")))?
        {
            Some(bytes) => {
                let r: EvidenceReceipt = serde_json::from_slice(&bytes)
                    .map_err(|e| Status::internal(format!("receipt JSON: {e}")))?;
                Ok(Response::new(receipt_to_proto(&r)))
            }
            None => Err(Status::not_found(format!(
                "no evidence receipt with id `{id}`"
            ))),
        }
    }

    // -----------------------------------------------------------------
    // Subscribe (server-streaming)
    // -----------------------------------------------------------------

    type SubscribeStream =
        Pin<Box<dyn Stream<Item = Result<proto::EvidenceReceipt, Status>> + Send + 'static>>;

    async fn subscribe(
        &self,
        request: Request<proto::SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let req = request.into_inner();
        let record_filter: Vec<RecordType> = req
            .record_types_filter
            .iter()
            .map(|v| record_type_from_proto_i32(*v))
            .collect::<Result<Vec<_>, _>>()
            .map_err(Status::from)?;
        let subject_filter = req.subject_filter.clone();
        let correlation_filter = req.correlation_id_filter.clone();
        let resume_from = req.resume_from_receipt_id.clone();

        // Replay from the open snapshot (T-012 keeps the sealed-segment
        // replay deferred until a dedicated `Subscribe`-over-sealed-history
        // task lands; the §22 MVP path subscribes to the open chain only).
        let guard = self.state.read().await;
        let mut replay: Vec<proto::EvidenceReceipt> = Vec::new();
        let mut replaying = resume_from.is_empty();
        for r in &guard.current.receipts {
            if !replaying {
                if r.receipt_id().as_str() == resume_from {
                    replaying = true;
                }
                continue;
            }
            let wire = receipt_to_proto(r);
            if subscribe_filters_pass(&wire, &record_filter, &subject_filter, &correlation_filter) {
                replay.push(wire);
            }
        }
        drop(guard);

        let rx = self.live_tx.subscribe();
        let live = BroadcastStream::new(rx).filter_map(move |item| {
            item.ok().and_then(|wire| {
                if subscribe_filters_pass(
                    &wire,
                    &record_filter,
                    &subject_filter,
                    &correlation_filter,
                ) {
                    Some(Ok(wire))
                } else {
                    None
                }
            })
        });
        let combined = tokio_stream::iter(replay.into_iter().map(Ok)).chain(live);
        Ok(Response::new(Box::pin(combined)))
    }

    // -----------------------------------------------------------------
    // Query (server-streaming)
    // -----------------------------------------------------------------

    type QueryStream =
        Pin<Box<dyn Stream<Item = Result<proto::EvidenceReceipt, Status>> + Send + 'static>>;

    async fn query(
        &self,
        request: Request<proto::QueryRequest>,
    ) -> Result<Response<Self::QueryStream>, Status> {
        let req = request.into_inner();
        let record_filter: Vec<RecordType> = req
            .record_types_filter
            .iter()
            .map(|v| record_type_from_proto_i32(*v))
            .collect::<Result<Vec<_>, _>>()
            .map_err(Status::from)?;
        let limit = if req.limit == 0 {
            DEFAULT_QUERY_LIMIT
        } else {
            req.limit
        };

        let guard = self.state.read().await;
        let mut hits: Vec<proto::EvidenceReceipt> = Vec::new();
        for r in &guard.current.receipts {
            if !req.action_id_filter.is_empty() {
                match r.action_id() {
                    Some(a) if a.as_str() == req.action_id_filter => {}
                    _ => continue,
                }
            }
            if !req.subject_filter.is_empty() && r.subject_canonical_id() != req.subject_filter {
                continue;
            }
            if !record_filter.is_empty() && !record_filter.contains(&r.record_type()) {
                continue;
            }
            if let Some(from) = req.from_time.as_ref() {
                let from_dt = crate::service::conversions::timestamp_to_datetime(from);
                if r.recorded_at() < from_dt {
                    continue;
                }
            }
            if let Some(to) = req.to_time.as_ref() {
                let to_dt = crate::service::conversions::timestamp_to_datetime(to);
                if r.recorded_at() > to_dt {
                    continue;
                }
            }
            hits.push(receipt_to_proto(r));
            if u32::try_from(hits.len()).unwrap_or(u32::MAX) >= limit {
                break;
            }
        }
        drop(guard);

        let stream = tokio_stream::iter(hits.into_iter().map(Ok));
        Ok(Response::new(Box::pin(stream)))
    }

    // -----------------------------------------------------------------
    // VerifyChain
    // -----------------------------------------------------------------

    async fn verify_chain(
        &self,
        request: Request<proto::VerifyChainRequest>,
    ) -> Result<Response<proto::VerifyChainResponse>, Status> {
        let _req = request.into_inner();
        let guard = self.state.read().await;
        // Build a transient chain and run the link walk.
        let mut chain = ReceiptChain::new();
        for r in &guard.current.receipts {
            if let Err(e) = chain.append(r.clone()) {
                // Append failure on a recorded receipt is a constitutional
                // tamper signal — surface it via the DataLoss mapping.
                return Err(Status::from(e));
            }
        }
        let walked = chain.len();
        let result = if walked == 0 {
            Ok(())
        } else {
            chain.verify_integrity()
        };
        drop(guard);
        match result {
            Ok(()) => Ok(Response::new(proto::VerifyChainResponse {
                consistent: true,
                receipts_checked: walked as u64,
                first_anomalous_receipt_id: String::new(),
                detection_method: "rocksdb_walk_link_hash".to_owned(),
            })),
            Err(EvidenceError::ChainBroken {
                index,
                actual,
                expected,
            }) => {
                let guard = self.state.read().await;
                let first_anomalous = guard
                    .current
                    .receipts
                    .get(index)
                    .map(|r| r.receipt_id().as_str().to_owned())
                    .unwrap_or_default();
                drop(guard);
                Ok(Response::new(proto::VerifyChainResponse {
                    consistent: false,
                    receipts_checked: walked as u64,
                    first_anomalous_receipt_id: first_anomalous,
                    detection_method: format!(
                        "chain_broken_link_at_{index}: actual=`{actual}` expected=`{expected}`"
                    ),
                }))
            }
            Err(other) => Err(Status::from(other)),
        }
    }

    // -----------------------------------------------------------------
    // RebuildIndex
    // -----------------------------------------------------------------

    async fn rebuild_index(
        &self,
        _request: Request<proto::RebuildIndexRequest>,
    ) -> Result<Response<proto::RebuildIndexResponse>, Status> {
        // T-012 maintains the record_type_index in lock-step with every
        // append (inside the WriteBatch), so a rebuild is a verification
        // walk: count the index rows. The count is bounded by the receipts
        // currently persisted (open snapshot only — sealed segments are
        // indexed when their receipts are first appended).
        let cf_idx = self
            .db
            .cf_handle(CF_RECORD_TYPE_INDEX)
            .ok_or_else(|| Status::internal("record_type_index CF missing"))?;
        let mut ropts = ReadOptions::default();
        ropts.set_verify_checksums(true);
        let mut indexed: u64 = 0;
        let iter = self.db.iterator_cf_opt(&cf_idx, ropts, IteratorMode::Start);
        for kv in iter {
            kv.map_err(|e| Status::internal(format!("iterate record_type_index: {e}")))?;
            indexed = indexed.saturating_add(1);
        }
        Ok(Response::new(proto::RebuildIndexResponse {
            receipts_indexed: indexed,
            completed_at: Some(crate::service::conversions::datetime_to_timestamp(
                Utc::now(),
            )),
        }))
    }

    // -----------------------------------------------------------------
    // GetLogInfo
    // -----------------------------------------------------------------

    async fn get_log_info(
        &self,
        _request: Request<()>,
    ) -> Result<Response<proto::LogInfo>, Status> {
        let guard = self.state.read().await;
        let count = guard.current.receipts.len() as u64;
        drop(guard);
        Ok(Response::new(proto::LogInfo {
            log_id: self.log_id.clone(),
            supported_schema_versions: vec![SCHEMA_VERSION_V1ALPHA1.to_owned()],
            default_schema_version: SCHEMA_VERSION_V1ALPHA1.to_owned(),
            active_segment_id: String::new(),
            active_segment_record_count: count,
            degraded: false,
            started_at: Some(crate::service::conversions::datetime_to_timestamp(
                self.started_at,
            )),
        }))
    }
}

/// Helper: apply the three `Subscribe` filters to a wire receipt.
fn subscribe_filters_pass(
    wire: &proto::EvidenceReceipt,
    record_filter: &[RecordType],
    subject_filter: &str,
    correlation_filter: &str,
) -> bool {
    if !record_filter.is_empty() {
        match record_type_from_proto_i32(wire.record_type) {
            Ok(rt) if record_filter.contains(&rt) => {}
            _ => return false,
        }
    }
    if !subject_filter.is_empty() && wire.subject != subject_filter {
        return false;
    }
    if !correlation_filter.is_empty() && wire.correlation_id != correlation_filter {
        return false;
    }
    true
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
    use crate::service::proto::evidence_log_server::EvidenceLog as EvidenceLogService;
    use ed25519_dalek::SigningKey;
    use tempfile::TempDir;

    fn temp_backend(seed: u8) -> (RocksDbEvidenceLog, TempDir) {
        let dir = TempDir::new().expect("tempdir");
        let sk = SigningKey::from_bytes(&[seed; 32]);
        let backend = RocksDbEvidenceLog::open(dir.path(), sk).expect("open rocksdb backend");
        (backend, dir)
    }

    fn append_req(record_type: proto::RecordType, subject: &str) -> proto::AppendRequest {
        proto::AppendRequest {
            schema_version: "aios.evidence.v1alpha1".to_owned(),
            payload: None,
            record_type: i32::from(record_type),
            subject: subject.to_owned(),
            action_id: String::new(),
            correlation_id: String::new(),
            trace_id: String::new(),
            simulated: false,
        }
    }

    #[tokio::test]
    async fn open_fresh_directory_creates_all_column_families() {
        let (b, _dir) = temp_backend(1);
        assert_eq!(b.receipt_count().await, 0);
        assert_eq!(b.sealed_segment_count().await, 0);
        assert!(b.log_id().starts_with("aios-evidence-log/"));
    }

    #[tokio::test]
    async fn append_persists_and_returns_signed_receipt() {
        let (b, _dir) = temp_backend(2);
        let r = b
            .append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                "human:operator-1",
            )))
            .await
            .expect("append")
            .into_inner();
        assert!(r.receipt_id.starts_with("evr_"));
        assert_eq!(b.receipt_count().await, 1);
    }

    #[tokio::test]
    async fn append_rejects_empty_subject() {
        let (b, _dir) = temp_backend(3);
        let err = b
            .append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                "",
            )))
            .await
            .expect_err("must reject");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn append_rejects_unspecified_record_type() {
        let (b, _dir) = temp_backend(4);
        let err = b
            .append(Request::new(append_req(
                proto::RecordType::Unspecified,
                "human:operator-1",
            )))
            .await
            .expect_err("must reject");
        assert_eq!(err.code(), tonic::Code::Internal);
    }

    #[tokio::test]
    async fn read_receipt_round_trips_through_disk() {
        let (b, _dir) = temp_backend(5);
        let r = b
            .append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                "human:operator-1",
            )))
            .await
            .expect("append")
            .into_inner();

        let back = b
            .read_receipt(Request::new(proto::ReadReceiptRequest {
                receipt_id: r.receipt_id.clone(),
            }))
            .await
            .expect("read")
            .into_inner();
        assert_eq!(back.receipt_id, r.receipt_id);
        assert_eq!(back.payload_hash, r.payload_hash);
    }

    #[tokio::test]
    async fn read_receipt_returns_not_found_for_unknown_id() {
        let (b, _dir) = temp_backend(6);
        let err = b
            .read_receipt(Request::new(proto::ReadReceiptRequest {
                receipt_id: "evr_does_not_exist".to_owned(),
            }))
            .await
            .expect_err("must miss");
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn append_then_seal_then_open_recovers_sealed_chain() {
        let dir = TempDir::new().expect("tempdir");
        let sk = SigningKey::from_bytes(&[7u8; 32]);

        // Phase 1: open, append 3 receipts, seal.
        {
            let b = RocksDbEvidenceLog::open(dir.path(), sk.clone()).expect("open");
            for i in 0..3_u32 {
                b.append(Request::new(append_req(
                    proto::RecordType::ActionReceived,
                    &format!("human:operator-{i}"),
                )))
                .await
                .expect("append");
            }
            let sealed = b.seal_current_segment().await.expect("seal");
            assert_eq!(sealed.receipt_count(), 4); // 3 + terminal SEGMENT_SEALED
            assert_eq!(b.sealed_segment_count().await, 1);
            assert_eq!(b.receipt_count().await, 0);
        }

        // Phase 2: reopen. Recovery must pass; sealed-segment count is 1.
        let b2 = RocksDbEvidenceLog::open(dir.path(), sk).expect("reopen");
        assert_eq!(b2.sealed_segment_count().await, 1);
        assert_eq!(b2.receipt_count().await, 0);
    }

    #[tokio::test]
    async fn query_filters_by_record_type_and_subject() {
        let (b, _dir) = temp_backend(8);
        for _ in 0..3 {
            b.append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                "human:alice",
            )))
            .await
            .expect("append");
        }
        for _ in 0..2 {
            b.append(Request::new(append_req(
                proto::RecordType::PolicyDecision,
                "service:policy",
            )))
            .await
            .expect("append");
        }

        let stream = b
            .query(Request::new(proto::QueryRequest {
                record_types_filter: vec![i32::from(proto::RecordType::PolicyDecision)],
                subject_filter: String::new(),
                correlation_id_filter: String::new(),
                action_id_filter: String::new(),
                from_time: None,
                to_time: None,
                text_match: String::new(),
                limit: 0,
                subject: String::new(),
            }))
            .await
            .expect("query")
            .into_inner();
        let collected: Vec<_> = stream.collect::<Vec<_>>().await;
        assert_eq!(collected.len(), 2);
    }

    #[tokio::test]
    async fn verify_chain_reports_consistent_after_appends() {
        let (b, _dir) = temp_backend(9);
        for i in 0..4 {
            b.append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                &format!("human:operator-{i}"),
            )))
            .await
            .expect("append");
        }
        let r = b
            .verify_chain(Request::new(proto::VerifyChainRequest {
                segment_id_from: String::new(),
                segment_id_to: String::new(),
            }))
            .await
            .expect("verify")
            .into_inner();
        assert!(r.consistent);
        assert_eq!(r.receipts_checked, 4);
    }

    #[tokio::test]
    async fn rebuild_index_counts_record_type_index_rows() {
        let (b, _dir) = temp_backend(10);
        for _ in 0..5 {
            b.append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                "human:operator-1",
            )))
            .await
            .expect("append");
        }
        let r = b
            .rebuild_index(Request::new(proto::RebuildIndexRequest {
                include_full_text: false,
            }))
            .await
            .expect("rebuild")
            .into_inner();
        assert_eq!(r.receipts_indexed, 5);
    }

    #[tokio::test]
    async fn get_log_info_reports_persistent_log_id() {
        let dir = TempDir::new().expect("tempdir");
        let sk = SigningKey::from_bytes(&[11u8; 32]);
        let log_id_1 = {
            let b = RocksDbEvidenceLog::open(dir.path(), sk.clone()).expect("open1");
            b.log_id().to_owned()
        };
        // Reopen with the same signing key: log_id must persist.
        let b2 = RocksDbEvidenceLog::open(dir.path(), sk).expect("open2");
        assert_eq!(b2.log_id(), log_id_1);
    }

    #[tokio::test]
    async fn open_rejects_unknown_schema_version_on_disk() {
        let dir = TempDir::new().expect("tempdir");
        let sk = SigningKey::from_bytes(&[12u8; 32]);
        // Open once to stamp v1alpha1, then poke a fake version into metadata.
        {
            let _b = RocksDbEvidenceLog::open(dir.path(), sk.clone()).expect("open1");
        }
        // Manual mutation: open the DB raw and overwrite schema_version.
        {
            let mut opts = Options::default();
            opts.create_if_missing(false);
            opts.create_missing_column_families(false);
            let cfs: Vec<ColumnFamilyDescriptor> = ALL_COLUMN_FAMILIES
                .iter()
                .map(|n| ColumnFamilyDescriptor::new(*n, Options::default()))
                .collect();
            let db = DB::open_cf_descriptors(&opts, dir.path(), cfs).expect("raw open");
            let cf_meta = db.cf_handle(CF_METADATA).expect("meta cf");
            db.put_cf(&cf_meta, METADATA_SCHEMA_VERSION, b"v0.0.0-bogus")
                .expect("write bogus");
        }
        // Reopen — should refuse.
        let err = RocksDbEvidenceLog::open(dir.path(), sk).expect_err("must refuse");
        match err {
            EvidenceError::EncodingFailed(d) => {
                assert!(d.contains("v0.0.0-bogus"), "detail = `{d}`");
            }
            other => panic!("expected EncodingFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rocksdb_property_returns_value_for_known_property() {
        let (b, _dir) = temp_backend(13);
        let p = b.rocksdb_property("rocksdb.stats");
        assert!(p.is_some(), "rocksdb.stats must return a value");
    }

    #[tokio::test]
    async fn seal_on_empty_open_segment_fails_with_empty_segment() {
        let (b, _dir) = temp_backend(14);
        let r = b.seal_current_segment().await;
        match r {
            Err(EvidenceError::EmptySegment) => {}
            other => panic!("expected EmptySegment, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn two_consecutive_segments_link_correctly() {
        let dir = TempDir::new().expect("tempdir");
        let sk = SigningKey::from_bytes(&[15u8; 32]);
        let b = RocksDbEvidenceLog::open(dir.path(), sk).expect("open");
        // Segment 1.
        for i in 0..2 {
            b.append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                &format!("human:operator-{i}"),
            )))
            .await
            .expect("append");
        }
        let s1 = b.seal_current_segment().await.expect("seal1");
        // Segment 2.
        for i in 0..2 {
            b.append(Request::new(append_req(
                proto::RecordType::PolicyDecision,
                &format!("service:policy-{i}"),
            )))
            .await
            .expect("append");
        }
        let s2 = b.seal_current_segment().await.expect("seal2");

        assert_eq!(b.sealed_segment_count().await, 2);
        assert_eq!(s2.previous_segment_id(), Some(s1.segment_id()));
        assert_eq!(
            s2.previous_segment_seal_hash(),
            Some(s1.segment_seal_hash())
        );
    }
}
