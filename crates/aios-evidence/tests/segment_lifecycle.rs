//! Integration test (T-010): full M1 action lifecycle → 5-receipt segment →
//! seal → second 5-receipt segment → seal → cross-segment chain verifies.
//!
//! Pins the end-to-end public surface introduced by T-010:
//! [`Segment`], [`SealedSegment`], [`SegmentChain`] and the
//! [`Segment::seal`] consuming-seal contract over Ed25519.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    reason = "panic-on-failure is the idiomatic test signal in integration tests; lifecycle fixtures are long-form"
)]

use aios_action::ActionId;
use aios_evidence::{
    EvidenceReceipt, ReceiptBuilder, RecordType, RetentionClass, SealedSegment, Segment,
    SegmentChain,
};
use ed25519_dalek::SigningKey;
use serde_json::json;

/// Build one canonical 5-receipt action lifecycle inside a fresh `Segment`,
/// each receipt Ed25519-signed.
fn build_lifecycle_segment(sk: &SigningKey) -> Segment {
    let action_id = ActionId::new();
    let subject = "service:capability-runtime";
    let mut seg = Segment::new(RetentionClass::Standard24M);

    // 1. ACTION_RECEIVED (genesis of this segment).
    let received = ReceiptBuilder::new(
        RecordType::ActionReceived,
        RetentionClass::Standard24M,
        subject,
    )
    .with_action_id(action_id.clone())
    .with_payload(json!({"action": "fs.write", "adapter_id": "aios-fs"}))
    .seal_signed(None, sk)
    .expect("ACTION_RECEIVED seal_signed");
    seg.append(received).expect("append ACTION_RECEIVED");

    // 2. POLICY_DECISION.
    let policy = ReceiptBuilder::new(
        RecordType::PolicyDecision,
        RetentionClass::Standard24M,
        "service:policy-kernel",
    )
    .with_action_id(action_id.clone())
    .with_payload(json!({"decision": "ALLOW"}))
    .seal_signed(seg.receipts().last(), sk)
    .expect("POLICY_DECISION seal_signed");
    seg.append(policy).expect("append POLICY_DECISION");

    // 3. EXECUTION_STARTED.
    let started = ReceiptBuilder::new(
        RecordType::ExecutionStarted,
        RetentionClass::Standard24M,
        subject,
    )
    .with_action_id(action_id.clone())
    .with_payload(json!({"adapter_id": "aios-fs"}))
    .seal_signed(seg.receipts().last(), sk)
    .expect("EXECUTION_STARTED seal_signed");
    seg.append(started).expect("append EXECUTION_STARTED");

    // 4. EXECUTION_COMPLETED.
    let done = ReceiptBuilder::new(
        RecordType::ExecutionCompleted,
        RetentionClass::Standard24M,
        subject,
    )
    .with_action_id(action_id.clone())
    .with_payload(json!({"outcome": "SUCCESS"}))
    .seal_signed(seg.receipts().last(), sk)
    .expect("EXECUTION_COMPLETED seal_signed");
    seg.append(done).expect("append EXECUTION_COMPLETED");

    // 5. VERIFICATION_RESULT.
    let verified = ReceiptBuilder::new(
        RecordType::VerificationResult,
        RetentionClass::Standard24M,
        "service:verification-harness",
    )
    .with_action_id(action_id)
    .with_payload(json!({"status": "PASSED"}))
    .seal_signed(seg.receipts().last(), sk)
    .expect("VERIFICATION_RESULT seal_signed");
    seg.append(verified).expect("append VERIFICATION_RESULT");

    assert_eq!(seg.len(), 5, "lifecycle must contribute exactly 5 receipts");
    seg
}

#[test]
fn t010_two_lifecycle_segments_seal_and_chain_full_verify() {
    let seed = [11u8; 32];
    let sk = SigningKey::from_bytes(&seed);
    let vk = sk.verifying_key();

    // ─── Segment 1: 5-receipt lifecycle → seal ──────────────────────
    let seg1 = build_lifecycle_segment(&sk);
    let sealed1 = seg1.seal(None, None, &sk).expect("seg1 seal");

    // After seal: 5 originals + 1 terminal SEGMENT_SEALED.
    assert_eq!(sealed1.receipt_count(), 6);
    assert_eq!(sealed1.retention_class(), RetentionClass::Standard24M);
    assert!(sealed1.previous_segment_id().is_none());
    assert!(sealed1.previous_segment_seal_hash().is_none());

    // The terminal receipt is RecordType::SegmentSealed and FOREVER retention.
    let terminal = sealed1.receipts().last().expect("terminal");
    assert_eq!(terminal.record_type(), RecordType::SegmentSealed);
    assert_eq!(terminal.retention_class(), RetentionClass::Forever);
    assert!(terminal.is_signed(), "terminal receipt must be signed");

    // ─── Segment 2: another 5-receipt lifecycle → seal w/ prev link ─
    let seg2 = build_lifecycle_segment(&sk);
    let prev_id = sealed1.segment_id().clone();
    let prev_hash = sealed1.segment_seal_hash().to_owned();
    let sealed2 = seg2
        .seal(Some(&prev_id), Some(&prev_hash), &sk)
        .expect("seg2 seal");
    assert_eq!(sealed2.receipt_count(), 6);
    assert_eq!(sealed2.previous_segment_id(), Some(&prev_id));
    assert_eq!(
        sealed2.previous_segment_seal_hash(),
        Some(prev_hash.as_str())
    );

    // ─── SegmentChain: append both, verify_full ─────────────────────
    let mut chain = SegmentChain::new();
    chain.append(sealed1).expect("append sealed1");
    chain.append(sealed2).expect("append sealed2");

    assert_eq!(chain.len(), 2);
    chain.verify_chain().expect("verify_chain ok");
    chain
        .verify_chain_signed(&vk)
        .expect("verify_chain_signed ok");
    chain.verify_full(&vk).expect("verify_full ok");

    // ─── Serde round-trip preserves verification ────────────────────
    let serialized = serde_json::to_string(chain.segments()).expect("serialize");
    let segments: Vec<SealedSegment> = serde_json::from_str(&serialized).expect("deserialize");
    let mut rebuilt = SegmentChain::new();
    for seg in segments {
        rebuilt.append(seg).expect("re-append");
    }
    rebuilt
        .verify_full(&vk)
        .expect("round-tripped segment chain must verify");
}

#[test]
fn t010_segment_chain_three_segments_verify_full() {
    let seed = [203u8; 32];
    let sk = SigningKey::from_bytes(&seed);
    let vk = sk.verifying_key();

    let mut chain = SegmentChain::new();

    // Build and chain 3 lifecycle segments.
    let seg1 = build_lifecycle_segment(&sk);
    let sealed1 = seg1.seal(None, None, &sk).expect("seg1");
    let prev_id = sealed1.segment_id().clone();
    let prev_hash = sealed1.segment_seal_hash().to_owned();
    chain.append(sealed1).expect("append seg1");

    let seg2 = build_lifecycle_segment(&sk);
    let sealed2 = seg2
        .seal(Some(&prev_id), Some(&prev_hash), &sk)
        .expect("seg2");
    let prev_id = sealed2.segment_id().clone();
    let prev_hash = sealed2.segment_seal_hash().to_owned();
    chain.append(sealed2).expect("append seg2");

    let seg3 = build_lifecycle_segment(&sk);
    let sealed3 = seg3
        .seal(Some(&prev_id), Some(&prev_hash), &sk)
        .expect("seg3");
    chain.append(sealed3).expect("append seg3");

    // 3 segments × 6 receipts each (5 lifecycle + 1 terminal).
    assert_eq!(chain.len(), 3);
    let total_receipts: usize = chain
        .segments()
        .iter()
        .map(SealedSegment::receipt_count)
        .sum();
    assert_eq!(total_receipts, 18);

    chain
        .verify_full(&vk)
        .expect("3-segment chain must verify_full");

    // Bonus: sanity-check that every non-terminal receipt is signed.
    for (i, seg) in chain.segments().iter().enumerate() {
        for (j, r) in seg.receipts().iter().enumerate() {
            assert!(r.is_signed(), "segment {i} receipt {j} must be signed");
        }
    }
    // Silence the unused-EvidenceReceipt warning.
    let _: &EvidenceReceipt = &chain.segments()[0].receipts()[0];
}
