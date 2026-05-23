//! Append-only [`ReceiptChain`] mechanics (S3.1 §5).
//!
//! The chain is the in-memory representation of a single segment's receipt list.
//! Concrete persistence (WAL, `RocksDB` column families, segment sealing as per
//! S3.1 §7) is deferred to later tasks; T-007 nails the append-only invariant
//! (INV-005) and the link-hash verification (§5.3 steps 1).
//!
//! ## Append discipline
//!
//! - The first append MUST be a genesis receipt (`previous_receipt_hash = None`).
//! - Every subsequent append MUST carry
//!   `previous_receipt_hash == Some(prior.link_hash())`.
//! - Inserts in the middle, deletes, and overwrites are **not exposed by the
//!   public API**. The chain only grows.
//!
//! ## Verification
//!
//! [`ReceiptChain::verify_integrity`] walks the chain and recomputes every link
//! hash. On the first mismatch it returns [`crate::EvidenceError::ChainBroken`]
//! pointing at the offending index. This is the building block S3.1 §5.3 step 1
//! `VerifyChain` requires.

use crate::error::EvidenceError;
use crate::receipt::EvidenceReceipt;

/// Append-only ordered list of [`EvidenceReceipt`]s belonging to a single
/// segment.
///
/// **Public API exposes append-only writes.** `&mut self` exists for `append`,
/// but no public method removes, replaces, or reorders receipts. Read access is
/// via [`Self::receipts`] returning an immutable slice.
///
/// The chain is **not** thread-safe by itself; production callers wrap it in a
/// `Mutex` or use a single-writer task. Concurrency policy is the engine's
/// responsibility (S3.1 §11.1 strictly-monotonic enforcement at append time).
#[derive(Debug, Clone, Default)]
pub struct ReceiptChain {
    receipts: Vec<EvidenceReceipt>,
}

impl ReceiptChain {
    /// Create an empty chain.
    ///
    /// The next [`Self::append`] call must carry a genesis receipt
    /// (`previous_receipt_hash = None`).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            receipts: Vec::new(),
        }
    }

    /// Append a sealed receipt to the chain.
    ///
    /// Verifies:
    ///
    /// - For the first append: the receipt MUST be a genesis receipt (have
    ///   `previous_receipt_hash = None`). Otherwise returns
    ///   [`EvidenceError::GenesisMissing`].
    /// - For subsequent appends: the receipt MUST carry
    ///   `previous_receipt_hash = Some(prior.link_hash())`. Otherwise returns
    ///   [`EvidenceError::ChainBroken`] (mismatched link) or
    ///   [`EvidenceError::GenesisMissing`] (claimed-genesis but chain is
    ///   non-empty — actually mapped to
    ///   [`EvidenceError::DuplicateGenesis`]).
    ///
    /// # Errors
    ///
    /// See the variant list above.
    pub fn append(&mut self, receipt: EvidenceReceipt) -> Result<(), EvidenceError> {
        match (self.receipts.last(), receipt.previous_receipt_hash()) {
            // Genesis append onto an empty chain.
            (None, None) => {
                self.receipts.push(receipt);
                Ok(())
            }

            // Genesis-shaped append onto a non-empty chain: forbidden.
            (Some(_), None) => Err(EvidenceError::DuplicateGenesis),

            // Non-genesis append onto an empty chain: forbidden.
            (None, Some(_)) => Err(EvidenceError::GenesisMissing),

            // Normal forward append: previous_receipt_hash must match prior.link_hash().
            (Some(prior), Some(claimed_link)) => {
                let expected = prior.link_hash()?;
                if expected == claimed_link {
                    self.receipts.push(receipt);
                    Ok(())
                } else {
                    Err(EvidenceError::ChainBroken {
                        index: self.receipts.len(),
                        actual: claimed_link.to_owned(),
                        expected,
                    })
                }
            }
        }
    }

    /// Read-only borrow of every receipt in append order.
    #[must_use]
    pub fn receipts(&self) -> &[EvidenceReceipt] {
        &self.receipts
    }

    /// Number of receipts in the chain.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.receipts.len()
    }

    /// True if the chain is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.receipts.is_empty()
    }

    /// Walk the entire chain and verify §5.3 step-1 link consistency.
    ///
    /// For each pair (prev, curr) in order, asserts that
    /// `curr.previous_receipt_hash() == Some(prev.link_hash())`. On the first
    /// mismatch returns [`EvidenceError::ChainBroken`] pointing at the
    /// offending index. The genesis receipt at index 0 must carry
    /// `previous_receipt_hash = None`.
    ///
    /// Empty chains return [`EvidenceError::EmptyChain`] — distinguished from
    /// a healthy one-receipt chain (which trivially verifies).
    ///
    /// # Errors
    ///
    /// - [`EvidenceError::EmptyChain`] when the chain is empty.
    /// - [`EvidenceError::GenesisMissing`] when the receipt at index 0 has a
    ///   non-`None` `previous_receipt_hash` (the chain was constructed via a
    ///   trusted external path, e.g. deserialization, and is malformed).
    /// - [`EvidenceError::ChainBroken`] on the first link mismatch found.
    /// - [`EvidenceError::EncodingFailed`] if a link-hash recomputation fails.
    pub fn verify_integrity(&self) -> Result<(), EvidenceError> {
        if self.receipts.is_empty() {
            return Err(EvidenceError::EmptyChain);
        }

        // Genesis must carry no previous_receipt_hash.
        if let Some(genesis) = self.receipts.first() {
            if genesis.previous_receipt_hash().is_some() {
                return Err(EvidenceError::GenesisMissing);
            }
        }

        // Walk pairs and verify link.
        for (i, window) in self.receipts.windows(2).enumerate() {
            // windows(2) yields slices of length 2; index in the chain of the
            // second element is `i + 1`.
            let prev = &window[0];
            let curr = &window[1];
            let Some(claimed) = curr.previous_receipt_hash() else {
                return Err(EvidenceError::ChainBroken {
                    index: i + 1,
                    actual: "<missing previous_receipt_hash>".to_owned(),
                    expected: prev.link_hash()?,
                });
            };
            let expected = prev.link_hash()?;
            if expected != claimed {
                return Err(EvidenceError::ChainBroken {
                    index: i + 1,
                    actual: claimed.to_owned(),
                    expected,
                });
            }
        }

        Ok(())
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
    use crate::record::{RecordType, RetentionClass};
    use serde_json::json;

    fn b(record_type: RecordType) -> ReceiptBuilder {
        ReceiptBuilder::new(record_type, RetentionClass::Standard24M, "human:operator-1")
    }

    #[test]
    fn new_chain_is_empty() {
        let chain = ReceiptChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
        assert!(chain.receipts().is_empty());
    }

    #[test]
    fn verify_integrity_on_empty_chain_errors() {
        let chain = ReceiptChain::new();
        match chain.verify_integrity() {
            Err(EvidenceError::EmptyChain) => {}
            other => panic!("expected EmptyChain, got {other:?}"),
        }
    }

    #[test]
    fn append_genesis_then_three_receipts_and_verify_integrity_ok() {
        let mut chain = ReceiptChain::new();

        let genesis = b(RecordType::ActionReceived)
            .with_payload(json!({"step": 0}))
            .seal(None)
            .expect("genesis");
        chain.append(genesis).expect("append genesis");

        let r1 = b(RecordType::PolicyDecision)
            .with_payload(json!({"step": 1}))
            .seal(chain.receipts().last())
            .expect("r1");
        chain.append(r1).expect("append r1");

        let r2 = b(RecordType::ExecutionStarted)
            .with_payload(json!({"step": 2}))
            .seal(chain.receipts().last())
            .expect("r2");
        chain.append(r2).expect("append r2");

        let r3 = b(RecordType::ExecutionCompleted)
            .with_payload(json!({"step": 3}))
            .seal(chain.receipts().last())
            .expect("r3");
        chain.append(r3).expect("append r3");

        assert_eq!(chain.len(), 4);
        chain.verify_integrity().expect("integrity must hold");
    }

    #[test]
    fn append_rejects_non_genesis_first_receipt() {
        // Build a receipt that already carries a previous_receipt_hash (by
        // sealing against a genesis we then DO NOT add to the chain).
        let fake_prev = b(RecordType::ActionReceived).seal(None).expect("fake");
        let orphan = b(RecordType::PolicyDecision)
            .seal(Some(&fake_prev))
            .expect("orphan");

        let mut chain = ReceiptChain::new();
        match chain.append(orphan) {
            Err(EvidenceError::GenesisMissing) => {}
            other => panic!("expected GenesisMissing, got {other:?}"),
        }
    }

    #[test]
    fn append_rejects_duplicate_genesis() {
        let mut chain = ReceiptChain::new();
        let g1 = b(RecordType::ActionReceived).seal(None).expect("g1");
        chain.append(g1).expect("first genesis ok");

        let g2 = b(RecordType::PolicyDecision).seal(None).expect("g2");
        match chain.append(g2) {
            Err(EvidenceError::DuplicateGenesis) => {}
            other => panic!("expected DuplicateGenesis, got {other:?}"),
        }
    }

    #[test]
    fn append_rejects_wrong_previous_receipt_hash() {
        // Build two genesis receipts. Seal r1 against the first; then attempt
        // to append r1 onto a chain whose tail is the SECOND genesis. The link
        // hash will not match.
        let g_a = b(RecordType::ActionReceived).seal(None).expect("g_a");
        let g_b = b(RecordType::ActionReceived).seal(None).expect("g_b");

        // r1 chains to g_a.
        let r1 = b(RecordType::PolicyDecision).seal(Some(&g_a)).expect("r1");

        let mut chain = ReceiptChain::new();
        chain.append(g_b).expect("g_b is genesis ok");

        match chain.append(r1) {
            Err(EvidenceError::ChainBroken {
                index,
                actual,
                expected,
            }) => {
                assert_eq!(index, 1);
                assert_ne!(actual, expected);
            }
            other => panic!("expected ChainBroken, got {other:?}"),
        }
    }

    #[test]
    fn verify_integrity_detects_tampered_middle_receipt_via_deserialization() {
        // Build a 3-receipt chain, then surgically corrupt the middle receipt
        // through the deserialization path (the only "edit" path that exists).
        // Verification must catch the resulting broken link.
        let mut chain = ReceiptChain::new();
        let g = b(RecordType::ActionReceived).seal(None).expect("g");
        chain.append(g.clone()).expect("g");
        let r1 = b(RecordType::PolicyDecision).seal(Some(&g)).expect("r1");
        chain.append(r1.clone()).expect("r1");
        let r2 = b(RecordType::ExecutionStarted).seal(Some(&r1)).expect("r2");
        chain.append(r2).expect("r2");

        // Serialize, mutate the middle receipt's payload, deserialize back.
        let mut serialized: Vec<serde_json::Value> = chain
            .receipts()
            .iter()
            .map(|r| serde_json::to_value(r).expect("serialize"))
            .collect();
        // Tamper: rewrite payload of receipt index 1. Its content_hash now no
        // longer matches but the chain link is what we verify here.
        // To force a chain break, mutate the receipt's `previous_receipt_hash`
        // field directly so the link no longer matches the genesis.
        serialized[1]["previous_receipt_hash"] = json!("0".repeat(32));

        let mutated_receipts: Vec<EvidenceReceipt> = serialized
            .into_iter()
            .map(|v| serde_json::from_value(v).expect("deserialize"))
            .collect();

        // Hand-build a chain with the mutated content. We bypass `append`
        // intentionally: tamper detection is the job of `verify_integrity`.
        let tampered = ReceiptChain {
            receipts: mutated_receipts,
        };

        match tampered.verify_integrity() {
            Err(EvidenceError::ChainBroken { index, .. }) => {
                assert_eq!(index, 1, "tamper at receipt index 1 must be detected");
            }
            other => panic!("expected ChainBroken at index 1, got {other:?}"),
        }
    }

    #[test]
    fn single_receipt_chain_verifies_trivially() {
        let mut chain = ReceiptChain::new();
        let g = b(RecordType::ActionReceived).seal(None).expect("g");
        chain.append(g).expect("g");
        chain.verify_integrity().expect("single genesis ok");
    }
}
