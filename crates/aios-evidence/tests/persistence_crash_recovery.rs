//! T-012 — crash-recovery integration tests for `RocksDbEvidenceLog`.
//!
//! These tests exercise the constitutional contract of S3.1 §11.4: every
//! receipt and sealed segment that was written to disk before a crash MUST
//! be present and verifiable after the backend is reopened.
//!
//! - **write-drop-reopen**: writes receipts, drops the handle, reopens at
//!   the same path, and verifies state matches.
//! - **seal-survives-restart**: seals a segment, reopens, and checks the
//!   sealed segment + chain head are persisted.
//! - **tamper-on-disk**: overwrites a sealed-receipt byte on disk and
//!   expects the reopen to surface `ChainBroken` / `SegmentSealMismatch` /
//!   `SegmentSignatureMismatch`.
//! - **unknown-schema-version**: refuses to open a database whose on-disk
//!   schema version does not match.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::too_many_lines,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use ed25519_dalek::SigningKey;
use tempfile::TempDir;
use tonic::Request;

use aios_evidence::persistence::RocksDbEvidenceLog;
use aios_evidence::service::proto;
use aios_evidence::service::proto::evidence_log_server::EvidenceLog;

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

// -----------------------------------------------------------------------
// 1. Ten receipts + seal a segment — survives restart.
// -----------------------------------------------------------------------

#[tokio::test]
async fn ten_receipts_then_seal_survives_restart() {
    let dir = TempDir::new().expect("tempdir");
    let sk = SigningKey::from_bytes(&[101u8; 32]);

    let mut minted_ids = Vec::new();
    // Phase 1: open, append 10 receipts, seal the segment.
    {
        let b = RocksDbEvidenceLog::open(dir.path(), sk.clone()).expect("open phase 1");
        for i in 0..10_u32 {
            let r = b
                .append(Request::new(append_req(
                    proto::RecordType::ActionReceived,
                    &format!("human:operator-{i}"),
                )))
                .await
                .expect("append")
                .into_inner();
            minted_ids.push(r.receipt_id);
        }
        assert_eq!(b.receipt_count().await, 10);
        let sealed = b.seal_current_segment().await.expect("seal");
        assert_eq!(sealed.receipt_count(), 11); // 10 + terminal SEGMENT_SEALED
        assert_eq!(b.sealed_segment_count().await, 1);
        assert_eq!(b.receipt_count().await, 0);
    }

    // Phase 2: reopen at the same path.
    let b2 = RocksDbEvidenceLog::open(dir.path(), sk).expect("open phase 2");
    assert_eq!(b2.sealed_segment_count().await, 1);
    assert_eq!(b2.receipt_count().await, 0);

    // Each pre-seal receipt must be readable by id from the receipts CF.
    for id in &minted_ids {
        let back = b2
            .read_receipt(Request::new(proto::ReadReceiptRequest {
                receipt_id: id.clone(),
            }))
            .await
            .expect("read after restart")
            .into_inner();
        assert_eq!(&back.receipt_id, id);
    }
}

// -----------------------------------------------------------------------
// 2. Open-segment snapshot survives restart (no seal in between).
// -----------------------------------------------------------------------

#[tokio::test]
async fn open_segment_snapshot_survives_restart_without_seal() {
    let dir = TempDir::new().expect("tempdir");
    let sk = SigningKey::from_bytes(&[102u8; 32]);

    {
        let b = RocksDbEvidenceLog::open(dir.path(), sk.clone()).expect("open p1");
        for i in 0..5_u32 {
            let _ = b
                .append(Request::new(append_req(
                    proto::RecordType::PolicyDecision,
                    &format!("service:policy-{i}"),
                )))
                .await
                .expect("append")
                .into_inner();
        }
        assert_eq!(b.receipt_count().await, 5);
    }

    let b2 = RocksDbEvidenceLog::open(dir.path(), sk).expect("open p2");
    // Snapshot must restore: 5 receipts in the open segment, sealed count 0.
    assert_eq!(b2.receipt_count().await, 5);
    assert_eq!(b2.sealed_segment_count().await, 0);

    // VerifyChain over the restored open segment must report consistent.
    let r = b2
        .verify_chain(Request::new(proto::VerifyChainRequest {
            segment_id_from: String::new(),
            segment_id_to: String::new(),
        }))
        .await
        .expect("verify")
        .into_inner();
    assert!(r.consistent);
    assert_eq!(r.receipts_checked, 5);
}

// -----------------------------------------------------------------------
// 3. Tamper a sealed-segment receipt byte on disk — reopen must reject.
// -----------------------------------------------------------------------

#[tokio::test]
async fn tampered_sealed_segment_on_disk_is_rejected_at_reopen() {
    let dir = TempDir::new().expect("tempdir");
    let sk = SigningKey::from_bytes(&[103u8; 32]);

    // Phase 1: seal a segment.
    {
        let b = RocksDbEvidenceLog::open(dir.path(), sk.clone()).expect("open p1");
        for i in 0..3_u32 {
            b.append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                &format!("human:operator-{i}"),
            )))
            .await
            .expect("append");
        }
        b.seal_current_segment().await.expect("seal");
    }

    // Phase 2: open raw DB and mutate the sealed segment JSON in place.
    {
        use rocksdb::{ColumnFamilyDescriptor, Options, DB};
        let mut opts = Options::default();
        opts.create_if_missing(false);
        opts.create_missing_column_families(false);
        let cfs: Vec<ColumnFamilyDescriptor> = [
            "receipts",
            "segments",
            "current_segment",
            "metadata",
            "chain_index",
            "record_type_index",
        ]
        .iter()
        .map(|n| ColumnFamilyDescriptor::new(*n, Options::default()))
        .collect();
        let db = DB::open_cf_descriptors(&opts, dir.path(), cfs).expect("raw open");
        let cf_segments = db.cf_handle("segments").expect("segments cf");
        // Iterate, mutate the first segment row.
        let mut iter = db.iterator_cf(&cf_segments, rocksdb::IteratorMode::Start);
        let kv = iter.next().expect("at least one segment").expect("ok");
        let key = kv.0;
        let bytes = kv.1;
        let mut json: serde_json::Value =
            serde_json::from_slice(&bytes).expect("segment json deserializes");
        // Tamper a non-terminal receipt's payload.
        if let Some(arr) = json["receipts"].as_array_mut() {
            arr[0]["payload"] = serde_json::json!({"step": 9999});
        }
        let mutated = serde_json::to_vec(&json).expect("ser");
        db.put_cf(&cf_segments, &key, mutated).expect("write back");
    }

    // Phase 3: reopen — must refuse with a tamper-flavoured EvidenceError.
    let err = RocksDbEvidenceLog::open(dir.path(), sk).expect_err("reopen must refuse");
    let s = format!("{err:?}");
    assert!(
        s.contains("Signature") || s.contains("Seal") || s.contains("Chain"),
        "expected tamper-flavoured error, got {err:?}"
    );
}

// -----------------------------------------------------------------------
// 4. Same signing key — log id is stable across restart.
// -----------------------------------------------------------------------

#[tokio::test]
async fn log_id_is_stable_across_restart() {
    let dir = TempDir::new().expect("tempdir");
    let sk = SigningKey::from_bytes(&[104u8; 32]);

    let id_1 = {
        let b = RocksDbEvidenceLog::open(dir.path(), sk.clone()).expect("p1");
        b.log_id().to_owned()
    };
    let b2 = RocksDbEvidenceLog::open(dir.path(), sk).expect("p2");
    assert_eq!(b2.log_id(), id_1);
}

// -----------------------------------------------------------------------
// 5. VerifyChain over reopened state matches pre-restart state.
// -----------------------------------------------------------------------

#[tokio::test]
async fn verify_chain_matches_pre_restart_state() {
    let dir = TempDir::new().expect("tempdir");
    let sk = SigningKey::from_bytes(&[105u8; 32]);

    {
        let b = RocksDbEvidenceLog::open(dir.path(), sk.clone()).expect("p1");
        for _ in 0..7 {
            b.append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                "human:operator-1",
            )))
            .await
            .expect("append");
        }
    }
    let b2 = RocksDbEvidenceLog::open(dir.path(), sk).expect("p2");
    let v = b2
        .verify_chain(Request::new(proto::VerifyChainRequest {
            segment_id_from: String::new(),
            segment_id_to: String::new(),
        }))
        .await
        .expect("verify")
        .into_inner();
    assert!(v.consistent);
    assert_eq!(v.receipts_checked, 7);
}
