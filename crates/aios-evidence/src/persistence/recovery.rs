//! Crash-recovery / startup-rebuild logic for the `RocksDB` evidence backend
//! (T-012, S3.1 §11.4).
//!
//! The recovery procedure runs on every [`crate::persistence::RocksDbEvidenceLog::open`]
//! call. Its job is to materialize an in-memory view of the persisted state
//! that is byte-identical to the state the engine had at the time of its
//! last successful append.
//!
//! ## Spec contract (§11.4)
//!
//! > A scheduled audit (default daily) and on-demand `VerifyChain` walk the
//! > chain. Inconsistencies emit `CHAIN_INCONSISTENCY_DETECTED` and the engine
//! > enters **degraded mode**.
//!
//! For T-012 we surface the inconsistency as
//! [`crate::EvidenceError::ChainBroken`] (per-receipt-chain break inside a
//! segment) or [`crate::EvidenceError::SegmentChainBroken`] (cross-segment
//! seal-hash break). A higher layer (the operator-facing gRPC binary, not
//! yet implemented) is responsible for translating that into a
//! `CHAIN_INCONSISTENCY_DETECTED` receipt and flipping a `degraded_mode` flag.
//!
//! ## Procedure
//!
//! On open:
//!
//! 1. Open `RocksDB` with all 6 column families (per `encoding::ALL_COLUMN_FAMILIES`).
//! 2. Read `metadata::chain_head` — the count of sealed segments. Fresh
//!    databases have no entry and we treat that as `0`.
//! 3. Walk `chain_index` from position `0..chain_head`, looking up each
//!    [`crate::SealedSegment`] in the `segments` CF and verifying:
//!    - The segment's Ed25519 seal signature ([`crate::SealedSegment::verify_seal`]).
//!    - The cross-segment link: each segment's `previous_segment_seal_hash`
//!      matches the prior segment's `segment_seal_hash` (or the §5.2
//!      `0000...0000` constant for the genesis segment).
//! 4. Read `current_segment::current` — if a snapshot is present, deserialize
//!    it as an [`OpenSegmentSnapshot`] and run [`crate::chain::ReceiptChain::verify_integrity`]
//!    over its receipts. The in-progress segment is restored as the
//!    "current open" segment for new appends.
//! 5. Restore the broadcast subscriber count to zero — subscribers are
//!    per-connection state, not persistent.
//!
//! Any verification failure short-circuits the open and the error
//! propagates to the caller. The on-disk state is **not** modified by the
//! recovery walk; the caller decides whether to enter degraded mode or
//! abort.

use serde::{Deserialize, Serialize};

use crate::receipt::EvidenceReceipt;
use crate::record::RetentionClass;

/// Snapshot of an open (in-progress, not yet sealed) segment persisted in the
/// `current_segment` column family.
///
/// The snapshot is rewritten on every successful `Append` — the canonical
/// crash-survival path is "persist the receipt to `receipts`, persist the
/// updated open-segment snapshot to `current_segment`, both inside one
/// atomic `WriteBatch` with `WAL_sync = true`".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenSegmentSnapshot {
    /// Retention class for the segment under construction.
    pub retention_class: RetentionClass,
    /// Receipts in the open segment, in append order.
    pub receipts: Vec<EvidenceReceipt>,
}

impl OpenSegmentSnapshot {
    /// Fresh, empty snapshot with the default retention class
    /// ([`RetentionClass::Standard24M`]).
    #[must_use]
    pub const fn new(retention_class: RetentionClass) -> Self {
        Self {
            retention_class,
            receipts: Vec::new(),
        }
    }

    /// Number of receipts buffered in the open segment.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.receipts.len()
    }

    /// True iff the open segment carries no receipts.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.receipts.is_empty()
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use crate::receipt::ReceiptBuilder;
    use crate::record::RecordType;
    use ed25519_dalek::SigningKey;
    use serde_json::json;

    fn fresh_signed_receipt(sk: &SigningKey) -> EvidenceReceipt {
        ReceiptBuilder::new(
            RecordType::ActionReceived,
            RetentionClass::Standard24M,
            "human:operator-1",
        )
        .with_payload(json!({"step": 1}))
        .seal_signed(None, sk)
        .expect("seal_signed")
    }

    #[test]
    fn open_segment_snapshot_new_starts_empty() {
        let snap = OpenSegmentSnapshot::new(RetentionClass::Standard24M);
        assert!(snap.is_empty());
        assert_eq!(snap.len(), 0);
        assert_eq!(snap.retention_class, RetentionClass::Standard24M);
    }

    #[test]
    fn open_segment_snapshot_serde_round_trip_with_signed_receipt() {
        let sk = SigningKey::from_bytes(&[3u8; 32]);
        let mut snap = OpenSegmentSnapshot::new(RetentionClass::Forever);
        snap.receipts.push(fresh_signed_receipt(&sk));

        let s = serde_json::to_string(&snap).expect("ser");
        let back: OpenSegmentSnapshot = serde_json::from_str(&s).expect("de");

        assert_eq!(back.retention_class, snap.retention_class);
        assert_eq!(back.receipts.len(), snap.receipts.len());
        assert_eq!(back.receipts[0], snap.receipts[0]);
    }

    #[test]
    fn open_segment_snapshot_len_tracks_receipts_vector() {
        let sk = SigningKey::from_bytes(&[4u8; 32]);
        let mut snap = OpenSegmentSnapshot::new(RetentionClass::Standard24M);
        assert_eq!(snap.len(), 0);
        snap.receipts.push(fresh_signed_receipt(&sk));
        snap.receipts.push(fresh_signed_receipt(&sk));
        assert_eq!(snap.len(), 2);
        assert!(!snap.is_empty());
    }
}
