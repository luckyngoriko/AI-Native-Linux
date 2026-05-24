//! Key encoding helpers for the `RocksDB` evidence-log backend (T-012).
//!
//! All column-family keys and a couple of metadata sentinel keys live in one
//! place so the storage layout can be audited from a single file. The column
//! family that owns each key is documented inline.
//!
//! ## Layout summary
//!
//! | Column family            | Key encoding                                          | Value encoding                |
//! | ------------------------ | ----------------------------------------------------- | ----------------------------- |
//! | `receipts`               | raw `evr_<ULID>` bytes (ULID is lex-sortable)         | JSON `EvidenceReceipt`        |
//! | `segments`               | raw `seg_<32hex>` bytes                               | JSON `SealedSegment`          |
//! | `current_segment`        | sentinel constant [`CURRENT_SEGMENT_KEY`]             | JSON `OpenSegmentSnapshot`    |
//! | `metadata`               | sentinel constants ([`METADATA_*`])                   | sentinel-specific             |
//! | `chain_index`            | big-endian `u64` chain position (8 bytes)             | raw `seg_<32hex>` (ASCII)     |
//! | `record_type_index`      | `<rt_u16_be><nanos_i64_be><evr_ulid>` (10 + 26 bytes) | empty                         |
//!
//! ULID is lexicographically sortable by encoding-time so the `receipts` CF
//! is naturally ordered by recording time. Big-endian `u64` puts the
//! chain-position rows in numeric order in the `chain_index` CF so a
//! `prefix_iterator` walks them efficiently. The composite key in
//! `record_type_index` is `(record_type, recorded_at_nanos, receipt_id)` so
//! a per-record-type, time-range scan is one key-prefix sweep.

use aios_action::EvidenceReceiptId;
use chrono::{DateTime, Utc};

use crate::record::RecordType;
use crate::segment::SegmentId;

// ---------------------------------------------------------------------------
// Column-family names. Kept in one place so a misspelling at open time is
// caught by `cargo check` rather than at runtime.
// ---------------------------------------------------------------------------

/// `receipts` column family: stores the full receipt JSON keyed by its
/// `evr_<ULID>` id.
pub const CF_RECEIPTS: &str = "receipts";

/// `segments` column family: stores sealed-segment JSON keyed by its
/// `seg_<32hex>` id.
pub const CF_SEGMENTS: &str = "segments";

/// `current_segment` column family: a single key whose value is the
/// JSON-serialized open (in-progress) segment snapshot.
pub const CF_CURRENT_SEGMENT: &str = "current_segment";

/// `metadata` column family: chain-head pointer, schema version, signing key
/// metadata, and other backend-wide configuration items.
pub const CF_METADATA: &str = "metadata";

/// `chain_index` column family: maps the chain position (`u64` big-endian)
/// to a `seg_<32hex>` segment id. Enables O(log n) chain walks.
pub const CF_CHAIN_INDEX: &str = "chain_index";

/// `record_type_index` column family: composite-key index for fast time-range
/// queries by record type. Value is empty (the receipt is fetched from
/// [`CF_RECEIPTS`] via the trailing receipt-id bytes).
pub const CF_RECORD_TYPE_INDEX: &str = "record_type_index";

/// The six column families opened by [`crate::persistence::rocksdb_backend`].
/// Order matters only for descriptor construction — `RocksDB` enumerates them
/// by name internally.
pub const ALL_COLUMN_FAMILIES: [&str; 6] = [
    CF_RECEIPTS,
    CF_SEGMENTS,
    CF_CURRENT_SEGMENT,
    CF_METADATA,
    CF_CHAIN_INDEX,
    CF_RECORD_TYPE_INDEX,
];

// ---------------------------------------------------------------------------
// Sentinel keys (single-row column families).
// ---------------------------------------------------------------------------

/// The sole row of the `current_segment` CF.
pub const CURRENT_SEGMENT_KEY: &[u8] = b"current";

/// `metadata` row holding the current chain head as a `u64` big-endian byte
/// sequence (`0` when the chain is empty).
pub const METADATA_CHAIN_HEAD: &[u8] = b"chain_head";

/// `metadata` row holding the schema version string the backend was opened
/// against (currently always `aios.evidence.v1alpha1`).
pub const METADATA_SCHEMA_VERSION: &[u8] = b"schema_version";

/// Schema version string written by the current backend code path. Read at
/// open time so future migrations can detect older on-disk formats.
pub const SCHEMA_VERSION_V1ALPHA1: &str = "aios.evidence.v1alpha1";

/// `metadata` row holding the stable per-instance log identifier. Persists
/// across restarts so `GetLogInfo` returns a stable id for the same on-disk
/// directory.
pub const METADATA_LOG_ID: &[u8] = b"log_id";

/// Prefix for per-segment compaction-state rows in the `metadata` CF.
///
/// The full key is `compacted_at::<segment_id>` (ASCII); the value is the
/// compaction RFC 3339 timestamp as UTF-8 bytes. The presence of the row
/// is the "this segment has been compacted" predicate; absence means the
/// segment is still fully present in the warm tier. T-015 / S3.1 §12.
///
/// Keeping the compaction state out of the `segments` CF preserves the
/// segment row's byte-identical layout — cryptographic recovery (§11.4)
/// continues to round-trip through `serde_json::from_slice` without any
/// special-case handling.
pub const METADATA_COMPACTED_AT_PREFIX: &[u8] = b"compacted_at::";

/// Build the full `metadata` key for a segment's compacted-at row.
#[must_use]
pub fn metadata_compacted_at_key(segment_id: &SegmentId) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(METADATA_COMPACTED_AT_PREFIX.len() + segment_id.as_str().len());
    out.extend_from_slice(METADATA_COMPACTED_AT_PREFIX);
    out.extend_from_slice(segment_id.as_str().as_bytes());
    out
}

// ---------------------------------------------------------------------------
// Key constructors
// ---------------------------------------------------------------------------

/// Build the [`CF_RECEIPTS`] key for a given receipt id. The raw ASCII bytes
/// of `evr_<ULID>` are used directly; ULID is lex-sortable so the CF is
/// naturally ordered by recording time.
#[must_use]
pub fn receipt_key(id: &EvidenceReceiptId) -> Vec<u8> {
    id.as_str().as_bytes().to_vec()
}

/// Build the [`CF_SEGMENTS`] key for a given segment id. Raw ASCII bytes of
/// `seg_<32hex>`.
#[must_use]
pub fn segment_key(id: &SegmentId) -> Vec<u8> {
    id.as_str().as_bytes().to_vec()
}

/// Encode a chain position as 8 big-endian bytes. Big-endian guarantees
/// numeric order matches lexical order under `RocksDB`'s default comparator.
#[must_use]
pub const fn chain_position_key(position: u64) -> [u8; 8] {
    position.to_be_bytes()
}

/// Decode a [`CF_CHAIN_INDEX`] key back to the chain position. Returns
/// `None` if the key length is wrong (defensive against on-disk corruption).
#[must_use]
pub const fn chain_position_from_key(key: &[u8]) -> Option<u64> {
    if key.len() != 8 {
        return None;
    }
    let mut buf = [0u8; 8];
    let mut i = 0;
    while i < 8 {
        buf[i] = key[i];
        i += 1;
    }
    Some(u64::from_be_bytes(buf))
}

/// Composite-key for [`CF_RECORD_TYPE_INDEX`]:
/// `<record_type:u16_be><recorded_at_nanos:i64_be><receipt_id:utf8 ascii>`.
///
/// `record_type` is the wire-ID per S3.1 §29 (1..=427). Storing the wire-ID
/// as `u16` big-endian sorts all entries of the same record type together —
/// a prefix scan with the first 2 bytes is a per-record-type sweep.
/// `recorded_at_nanos` is the Unix-nanos UTC timestamp as big-endian `i64`;
/// negative values (pre-1970) sort _before_ positive values which matches
/// the chronological intuition. The trailing receipt id ensures uniqueness
/// across multiple receipts produced in the same nanosecond.
#[must_use]
pub fn record_type_index_key(
    record_type: RecordType,
    recorded_at: DateTime<Utc>,
    receipt_id: &EvidenceReceiptId,
) -> Vec<u8> {
    let rt_id = record_type.wire_id();
    let nanos = recorded_at
        .timestamp_nanos_opt()
        .unwrap_or_else(|| recorded_at.timestamp().saturating_mul(1_000_000_000));
    let id_bytes = receipt_id.as_str().as_bytes();
    let mut out = Vec::with_capacity(2 + 8 + id_bytes.len());
    out.extend_from_slice(&rt_id.to_be_bytes());
    out.extend_from_slice(&nanos.to_be_bytes());
    out.extend_from_slice(id_bytes);
    out
}

/// Build the per-record-type prefix used by time-range scans. Two
/// big-endian bytes — exactly the leading bytes of every
/// [`record_type_index_key`] for that record type.
#[must_use]
pub const fn record_type_prefix(record_type: RecordType) -> [u8; 2] {
    record_type.wire_id().to_be_bytes()
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn chain_position_round_trips_through_u64_be_bytes() {
        for pos in [0_u64, 1, 7, 1024, 1_000_000, u64::MAX] {
            let k = chain_position_key(pos);
            let back = chain_position_from_key(&k).expect("decode");
            assert_eq!(back, pos);
        }
    }

    #[test]
    fn chain_position_from_wrong_length_returns_none() {
        assert!(chain_position_from_key(&[0u8; 4]).is_none());
        assert!(chain_position_from_key(&[]).is_none());
        assert!(chain_position_from_key(&[0u8; 16]).is_none());
    }

    #[test]
    fn chain_position_keys_sort_in_numeric_order() {
        let k1 = chain_position_key(1);
        let k2 = chain_position_key(2);
        let k10 = chain_position_key(10);
        let k256 = chain_position_key(256);
        // Lex order must match numeric order — that's why big-endian.
        assert!(k1 < k2);
        assert!(k2 < k10);
        assert!(k10 < k256);
    }

    #[test]
    fn receipt_key_is_raw_evr_ulid_bytes() {
        let id = EvidenceReceiptId::new();
        let k = receipt_key(&id);
        assert_eq!(k, id.as_str().as_bytes());
        assert!(k.starts_with(b"evr_"));
    }

    #[test]
    fn segment_key_is_raw_seg_hex_bytes() {
        let id = SegmentId::from_content(b"hello");
        let k = segment_key(&id);
        assert_eq!(k, id.as_str().as_bytes());
        assert!(k.starts_with(b"seg_"));
    }

    #[test]
    fn record_type_index_key_prefix_matches_record_type_prefix() {
        let id = EvidenceReceiptId::new();
        let now = Utc.timestamp_opt(1_700_000_000, 0).single().expect("utc");
        let k = record_type_index_key(RecordType::ActionReceived, now, &id);
        let prefix = record_type_prefix(RecordType::ActionReceived);
        assert_eq!(&k[..2], &prefix[..]);
    }

    #[test]
    fn record_type_index_keys_sort_first_by_record_type_then_by_time() {
        let id_a = EvidenceReceiptId::new();
        let id_b = EvidenceReceiptId::new();
        let t1 = Utc.timestamp_opt(1_700_000_000, 0).single().expect("utc");
        let t2 = Utc
            .timestamp_opt(1_700_000_001, 0)
            .single()
            .expect("utc-later");

        // Same record type, two timestamps.
        let k_t1 = record_type_index_key(RecordType::ActionReceived, t1, &id_a);
        let k_t2 = record_type_index_key(RecordType::ActionReceived, t2, &id_b);
        assert!(k_t1 < k_t2, "earlier time must sort before later time");

        // Two different record types — the lower wire-id sorts first.
        let k_action = record_type_index_key(RecordType::ActionReceived, t1, &id_a);
        let k_policy = record_type_index_key(RecordType::PolicyDecision, t1, &id_a);
        let action_id = RecordType::ActionReceived.wire_id();
        let policy_id = RecordType::PolicyDecision.wire_id();
        if action_id < policy_id {
            assert!(k_action < k_policy);
        } else {
            assert!(k_policy < k_action);
        }
    }

    #[test]
    fn record_type_index_key_total_length_is_predictable() {
        // 2 bytes record type + 8 bytes nanos + 30 bytes ULID-shaped id =
        // 40 bytes. `EvidenceReceiptId::new()` yields `evr_` (4) + 26 ULID
        // chars = 30 bytes.
        let id = EvidenceReceiptId::new();
        let now = Utc.timestamp_opt(0, 0).single().expect("epoch");
        let k = record_type_index_key(RecordType::ActionReceived, now, &id);
        assert_eq!(k.len(), 2 + 8 + 30);
    }

    #[test]
    fn all_column_families_contains_all_six_unique_names() {
        let mut sorted = ALL_COLUMN_FAMILIES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 6, "every CF name must be unique");
    }
}
