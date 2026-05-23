//! Integration test: emit the canonical M1 `aios-action` lifecycle evidence
//! chain end-to-end and verify integrity.
//!
//! Builds an `ActionEnvelope`, then produces an evidence chain that mirrors the
//! S0.1 §6 lifecycle edges:
//!
//! ```text
//!   ACTION_RECEIVED  -> POLICY_DECISION (ALLOW) -> EXECUTION_STARTED
//!   -> EXECUTION_COMPLETED -> VERIFICATION_RESULT
//! ```
//!
//! After appending all five receipts, `ReceiptChain::verify_integrity` walks the
//! chain and confirms every link hash matches.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    reason = "panic-on-failure is the idiomatic test signal in integration tests; fixture builders are intentionally long-form"
)]

use aios_action::ActionId;
use aios_evidence::{ReceiptBuilder, ReceiptChain, RecordType, RetentionClass};
use ed25519_dalek::SigningKey;
use serde_json::json;

#[test]
fn full_action_lifecycle_emits_a_verifiable_chain() {
    let action_id = ActionId::new();
    let subject = "service:capability-runtime";
    let mut chain = ReceiptChain::new();

    // 1. ACTION_RECEIVED (genesis of this segment).
    let received = ReceiptBuilder::new(
        RecordType::ActionReceived,
        RetentionClass::Standard24M,
        subject,
    )
    .with_action_id(action_id.clone())
    .with_payload(json!({
        "action_id": action_id.as_str(),
        "action": "fs.write",
        "adapter_id": "aios-fs",
    }))
    .seal(None)
    .expect("ACTION_RECEIVED seal");
    chain.append(received).expect("append ACTION_RECEIVED");

    // 2. POLICY_DECISION — ALLOW.
    let policy = ReceiptBuilder::new(
        RecordType::PolicyDecision,
        RetentionClass::Standard24M,
        "service:policy-kernel",
    )
    .with_action_id(action_id.clone())
    .with_payload(json!({
        "action_id": action_id.as_str(),
        "decision": "ALLOW",
        "reason_code": "policy.match.ok",
        "bundle_version": "v1.0.0",
    }))
    .seal(chain.receipts().last())
    .expect("POLICY_DECISION seal");
    chain.append(policy).expect("append POLICY_DECISION");

    // 3. EXECUTION_STARTED.
    let exec_started = ReceiptBuilder::new(
        RecordType::ExecutionStarted,
        RetentionClass::Standard24M,
        subject,
    )
    .with_action_id(action_id.clone())
    .with_payload(json!({
        "action_id": action_id.as_str(),
        "adapter_id": "aios-fs",
        "started_at_epoch_ms": 1_000,
    }))
    .seal(chain.receipts().last())
    .expect("EXECUTION_STARTED seal");
    chain
        .append(exec_started)
        .expect("append EXECUTION_STARTED");

    // 4. EXECUTION_COMPLETED — success.
    let exec_done = ReceiptBuilder::new(
        RecordType::ExecutionCompleted,
        RetentionClass::Standard24M,
        subject,
    )
    .with_action_id(action_id.clone())
    .with_payload(json!({
        "action_id": action_id.as_str(),
        "outcome": "SUCCESS",
        "duration_ms": 12,
    }))
    .seal(chain.receipts().last())
    .expect("EXECUTION_COMPLETED seal");
    chain.append(exec_done).expect("append EXECUTION_COMPLETED");

    // 5. VERIFICATION_RESULT — PASSED.
    let verified = ReceiptBuilder::new(
        RecordType::VerificationResult,
        RetentionClass::Standard24M,
        "service:verification-harness",
    )
    .with_action_id(action_id.clone())
    .with_payload(json!({
        "action_id": action_id.as_str(),
        "primitive_or_property": "file.exists",
        "status": "PASSED",
    }))
    .seal(chain.receipts().last())
    .expect("VERIFICATION_RESULT seal");
    chain.append(verified).expect("append VERIFICATION_RESULT");

    // ─── Acceptance ──────────────────────────────────────────────────
    assert_eq!(chain.len(), 5, "expected 5-receipt lifecycle chain");

    // Genesis carries no previous_receipt_hash.
    assert!(chain.receipts()[0].previous_receipt_hash().is_none());

    // Every non-genesis receipt carries a 32-hex-char link.
    for (i, r) in chain.receipts().iter().enumerate().skip(1) {
        let link = r
            .previous_receipt_hash()
            .unwrap_or_else(|| panic!("non-genesis receipt at index {i} must have link"));
        assert_eq!(
            link.len(),
            32,
            "link hash at index {i} must be 32 hex chars, got {link}"
        );
    }

    // Every receipt is bound to the action.
    for r in chain.receipts() {
        assert_eq!(r.action_id(), Some(&action_id));
    }

    // Content hashes are well-formed (BLAKE3-256 -> 64 hex chars).
    for (i, r) in chain.receipts().iter().enumerate() {
        assert_eq!(
            r.content_hash().len(),
            64,
            "content hash at index {i} must be 64 hex chars"
        );
        r.verify_content_hash().expect("content hash recompute");
    }

    // Final invariant: full-chain integrity.
    chain
        .verify_integrity()
        .expect("the full lifecycle chain must verify");
}

#[test]
fn chain_serde_round_trips_and_still_verifies() {
    // Build a 3-receipt chain.
    let mut chain = ReceiptChain::new();
    let action_id = ActionId::new();
    let subject = "human:operator-1";

    let g = ReceiptBuilder::new(
        RecordType::ActionReceived,
        RetentionClass::Standard24M,
        subject,
    )
    .with_action_id(action_id.clone())
    .with_payload(json!({"step": "received"}))
    .seal(None)
    .expect("genesis");
    chain.append(g).expect("append");

    let r1 = ReceiptBuilder::new(
        RecordType::PolicyDecision,
        RetentionClass::Standard24M,
        subject,
    )
    .with_action_id(action_id.clone())
    .with_payload(json!({"step": "policy"}))
    .seal(chain.receipts().last())
    .expect("r1");
    chain.append(r1).expect("append");

    let r2 = ReceiptBuilder::new(
        RecordType::ExecutionCompleted,
        RetentionClass::Standard24M,
        subject,
    )
    .with_action_id(action_id)
    .with_payload(json!({"step": "done"}))
    .seal(chain.receipts().last())
    .expect("r2");
    chain.append(r2).expect("append");

    // Round-trip every receipt through JSON.
    let mut rebuilt = ReceiptChain::new();
    for r in chain.receipts() {
        let json = serde_json::to_string(r).expect("serialize");
        let back: aios_evidence::EvidenceReceipt =
            serde_json::from_str(&json).expect("deserialize");
        rebuilt.append(back).expect("append round-tripped receipt");
    }

    rebuilt
        .verify_integrity()
        .expect("round-tripped chain must still verify");
    assert_eq!(rebuilt.len(), chain.len());
}

/// T-009: emit the canonical 5-receipt action lifecycle with **every receipt
/// Ed25519-signed**, then verify both hash-chain integrity and per-receipt
/// signatures end-to-end. This is the integration evidence that the S3.1
/// §5.2 / §11.3 signing path is wired into the public crate surface.
#[test]
fn t009_signed_lifecycle_chain_verifies_chain_and_every_signature() {
    // Deterministic test keypair. Production keys come from S5.2 Vault Broker
    // for subject `_system:service:evidence-segment-signer`.
    let seed = [7u8; 32];
    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();

    let action_id = ActionId::new();
    let subject = "service:capability-runtime";
    let mut chain = ReceiptChain::new();

    let received = ReceiptBuilder::new(
        RecordType::ActionReceived,
        RetentionClass::Standard24M,
        subject,
    )
    .with_action_id(action_id.clone())
    .with_payload(json!({"action": "fs.write", "adapter_id": "aios-fs"}))
    .seal_signed(None, &signing_key)
    .expect("ACTION_RECEIVED seal_signed");
    chain.append(received).expect("append ACTION_RECEIVED");

    let policy = ReceiptBuilder::new(
        RecordType::PolicyDecision,
        RetentionClass::Standard24M,
        "service:policy-kernel",
    )
    .with_action_id(action_id.clone())
    .with_payload(json!({"decision": "ALLOW"}))
    .seal_signed(chain.receipts().last(), &signing_key)
    .expect("POLICY_DECISION seal_signed");
    chain.append(policy).expect("append POLICY_DECISION");

    let exec_started = ReceiptBuilder::new(
        RecordType::ExecutionStarted,
        RetentionClass::Standard24M,
        subject,
    )
    .with_action_id(action_id.clone())
    .with_payload(json!({"adapter_id": "aios-fs"}))
    .seal_signed(chain.receipts().last(), &signing_key)
    .expect("EXECUTION_STARTED seal_signed");
    chain
        .append(exec_started)
        .expect("append EXECUTION_STARTED");

    let exec_done = ReceiptBuilder::new(
        RecordType::ExecutionCompleted,
        RetentionClass::Standard24M,
        subject,
    )
    .with_action_id(action_id.clone())
    .with_payload(json!({"outcome": "SUCCESS"}))
    .seal_signed(chain.receipts().last(), &signing_key)
    .expect("EXECUTION_COMPLETED seal_signed");
    chain.append(exec_done).expect("append EXECUTION_COMPLETED");

    let verified = ReceiptBuilder::new(
        RecordType::VerificationResult,
        RetentionClass::Standard24M,
        "service:verification-harness",
    )
    .with_action_id(action_id)
    .with_payload(json!({"status": "PASSED"}))
    .seal_signed(chain.receipts().last(), &signing_key)
    .expect("VERIFICATION_RESULT seal_signed");
    chain.append(verified).expect("append VERIFICATION_RESULT");

    // ─── Acceptance ──────────────────────────────────────────────────
    assert_eq!(chain.len(), 5);

    // Every receipt is signed.
    for (i, r) in chain.receipts().iter().enumerate() {
        assert!(r.is_signed(), "receipt at index {i} must be signed");
        let sig = r
            .signature()
            .unwrap_or_else(|| panic!("receipt {i} missing signature"));
        assert_eq!(
            sig.len(),
            128,
            "signature at index {i} must be 128 lowercase hex chars"
        );
    }

    // Combined integrity (hash chain + every Ed25519 signature) verifies.
    chain
        .verify_integrity_signed(&verifying_key)
        .expect("signed lifecycle chain must fully verify");
}
