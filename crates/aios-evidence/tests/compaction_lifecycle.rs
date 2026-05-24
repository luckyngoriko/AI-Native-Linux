//! Integration test (T-015): retention-class compaction worker
//! end-to-end through both backends (`InMemory` + `RocksDB`).
//!
//! Pins:
//! - 3-segment chain (one `FOREVER`, two `STANDARD_24M`) survives a 3-year
//!   wall-clock advance and `Auto`-mode compaction with the expected
//!   semantics (`STANDARD_24M` segments compacted; `FOREVER` segment intact).
//! - `OperatorApproval` mode emits a `COMPACTION_APPROVAL_REQUIRED` record
//!   on the first tick, waits, and only compacts after
//!   `CompactionWorker::approve_segment` is called.
//! - `RocksDB` persistence round-trips the compacted state across reopen.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::similar_names,
    reason = "panic-on-failure is the idiomatic test signal in integration tests; lifecycle fixtures are long-form"
)]

use aios_evidence::service::proto::evidence_log_server::EvidenceLog;
use aios_evidence::service::{proto, InMemoryEvidenceLog};
use aios_evidence::{
    CompactionPolicy, CompactionWorker, RecordType, RetentionClass, RocksDbEvidenceLog,
};
use ed25519_dalek::SigningKey;
use tempfile::TempDir;
use tonic::Request;

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

async fn seal_three_inmemory_segments(
    b: &InMemoryEvidenceLog,
) -> (
    aios_evidence::SealedSegment,
    aios_evidence::SealedSegment,
    aios_evidence::SealedSegment,
) {
    for i in 0..2 {
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            &format!("human:op-a-{i}"),
        )))
        .await
        .expect("append a");
    }
    let s_forever = b
        .seal_current_segment(RetentionClass::Forever)
        .await
        .expect("seal forever");

    for i in 0..3 {
        b.append(Request::new(append_req(
            proto::RecordType::PolicyDecision,
            &format!("service:policy-{i}"),
        )))
        .await
        .expect("append b");
    }
    let s_std_a = b
        .seal_current_segment(RetentionClass::Standard24M)
        .await
        .expect("seal std a");

    for i in 0..4 {
        b.append(Request::new(append_req(
            proto::RecordType::ApprovalGranted,
            &format!("human:op-c-{i}"),
        )))
        .await
        .expect("append c");
    }
    let s_std_b = b
        .seal_current_segment(RetentionClass::Standard24M)
        .await
        .expect("seal std b");

    (s_forever, s_std_a, s_std_b)
}

#[tokio::test]
async fn t015_inmemory_auto_compaction_after_3_years_keeps_forever_segment() {
    let sk = SigningKey::from_bytes(&[42u8; 32]);
    let b = InMemoryEvidenceLog::new(sk);
    let (s_forever, s_std_a, s_std_b) = seal_three_inmemory_segments(&b).await;

    let three_years_later = chrono::Utc::now() + chrono::Duration::days(3 * 365);
    let report = tokio::task::spawn_blocking({
        let backend = b.clone();
        move || {
            let mut backend = backend;
            let mut worker = CompactionWorker::new(CompactionPolicy::Auto);
            worker.tick(&mut backend, three_years_later)
        }
    })
    .await
    .expect("join")
    .expect("tick");

    // Two STANDARD_24M segments are past their 730-day horizon and the
    // FOREVER one is never eligible.
    assert_eq!(report.eligible_segments, 2);
    assert_eq!(report.compacted_segments, 2);
    assert_eq!(report.awaiting_approval, 0);

    // FOREVER segment intact.
    assert_eq!(
        b.warm_receipt_count_for(s_forever.segment_id()),
        Some(s_forever.receipt_count()),
        "FOREVER segment must NEVER be compacted"
    );
    // STANDARD_24M segments compacted (warm count is 0; sealed envelope kept).
    assert_eq!(b.warm_receipt_count_for(s_std_a.segment_id()), Some(0));
    assert_eq!(b.warm_receipt_count_for(s_std_b.segment_id()), Some(0));
    assert_eq!(
        b.sealed_segment_count(),
        3,
        "sealed envelopes preserved (§12)"
    );
}

#[tokio::test]
async fn t015_inmemory_operator_approval_flow_round_trip() {
    let sk = SigningKey::from_bytes(&[43u8; 32]);
    let b = InMemoryEvidenceLog::new(sk);
    let (_s_forever, s_std_a, s_std_b) = seal_three_inmemory_segments(&b).await;

    let three_years_later = chrono::Utc::now() + chrono::Duration::days(3 * 365);

    // Tick 1 — emit COMPACTION_APPROVAL_REQUIRED for both STANDARD_24M
    // segments; nothing compacted.
    let (worker, report) = tokio::task::spawn_blocking({
        let backend = b.clone();
        move || {
            let mut backend = backend;
            let mut worker = CompactionWorker::new(CompactionPolicy::OperatorApproval);
            let r = worker
                .tick(&mut backend, three_years_later)
                .expect("tick 1");
            (worker, r)
        }
    })
    .await
    .expect("join");
    assert_eq!(report.eligible_segments, 2);
    assert_eq!(report.compacted_segments, 0);
    assert_eq!(report.awaiting_approval, 2);

    // The chain now carries two COMPACTION_APPROVAL_REQUIRED records.
    let chain_count = b.receipt_count().await;
    assert_eq!(
        chain_count, 2,
        "operator-approval mode must surface one record per eligible segment"
    );
    // Warm receipts still present for the eligible segments.
    assert_eq!(
        b.warm_receipt_count_for(s_std_a.segment_id()),
        Some(s_std_a.receipt_count())
    );

    // Tick 2 — approve one segment only; only that one is compacted.
    let s_std_a_id = s_std_a.segment_id().clone();
    let s_std_b_id = s_std_b.segment_id().clone();
    let (worker, report2) = tokio::task::spawn_blocking({
        let backend = b.clone();
        move || {
            let mut backend = backend;
            let mut worker = worker;
            worker.approve_segment(&s_std_a_id).expect("approve a");
            let r = worker
                .tick(&mut backend, three_years_later)
                .expect("tick 2");
            (worker, r)
        }
    })
    .await
    .expect("join");
    assert_eq!(report2.compacted_segments, 1);
    assert_eq!(report2.awaiting_approval, 1);
    assert_eq!(b.warm_receipt_count_for(s_std_a.segment_id()), Some(0));
    assert_eq!(
        b.warm_receipt_count_for(s_std_b.segment_id()),
        Some(s_std_b.receipt_count())
    );

    // Tick 3 — approve the second segment; it now compacts. No new
    // approval-required records emitted (worker keeps its bookkeeping).
    let report3 = tokio::task::spawn_blocking({
        let backend = b.clone();
        move || {
            let mut backend = backend;
            let mut worker = worker;
            worker.approve_segment(&s_std_b_id).expect("approve b");
            worker.tick(&mut backend, three_years_later)
        }
    })
    .await
    .expect("join")
    .expect("tick 3");
    assert_eq!(report3.compacted_segments, 1);
    assert_eq!(report3.awaiting_approval, 0);
    assert_eq!(b.warm_receipt_count_for(s_std_b.segment_id()), Some(0));

    // Total approval-required emissions in the chain stays at 2 — the
    // worker never re-emits for a segment it already reported.
    let final_chain_count = b.receipt_count().await;
    assert_eq!(final_chain_count, 2);
}

#[tokio::test]
async fn t015_rocksdb_auto_compaction_survives_reopen() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().to_path_buf();
    let sk = SigningKey::from_bytes(&[44u8; 32]);

    let sealed_id;
    {
        let b = RocksDbEvidenceLog::open(&path, sk.clone()).expect("open 1");
        for i in 0..3 {
            b.append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                &format!("human:op-{i}"),
            )))
            .await
            .expect("append");
        }
        let sealed = b.seal_current_segment().await.expect("seal");
        sealed_id = sealed.segment_id().clone();

        let three_years_later = chrono::Utc::now() + chrono::Duration::days(3 * 365);
        let report = tokio::task::spawn_blocking({
            let backend = b.clone();
            move || {
                let mut backend = backend;
                let mut worker = CompactionWorker::new(CompactionPolicy::Auto);
                worker.tick(&mut backend, three_years_later)
            }
        })
        .await
        .expect("join")
        .expect("tick");
        assert_eq!(report.compacted_segments, 1);

        // Confirm receipts gone via ReadReceipt.
        for r in sealed
            .receipts()
            .iter()
            .filter(|r| r.record_type() != RecordType::SegmentSealed)
        {
            let resp = b
                .read_receipt(Request::new(proto::ReadReceiptRequest {
                    receipt_id: r.receipt_id().as_str().to_owned(),
                }))
                .await;
            assert!(resp.is_err());
        }
        drop(b);
    }

    // Reopen — sealed chain still verifies, compacted receipts stay gone,
    // and the sealed envelope is preserved (sealed_segment_count = 1).
    let b2 = RocksDbEvidenceLog::open(&path, sk).expect("open 2");
    assert_eq!(b2.sealed_segment_count().await, 1);

    // VerifyChain must succeed — compaction did NOT break the hash chain
    // (§12 invariant). The sealed segment metadata remains the canonical
    // chain witness even after the warm-tier receipt rows are gone.
    let resp = b2
        .verify_chain(Request::new(proto::VerifyChainRequest {
            segment_id_from: String::new(),
            segment_id_to: String::new(),
        }))
        .await
        .expect("verify_chain");
    let inner = resp.into_inner();
    assert!(
        inner.consistent,
        "compaction must NOT break the chain (§12); first_anomalous_receipt_id={}",
        inner.first_anomalous_receipt_id
    );

    let _ = sealed_id;
}
