//! Append-only cross-segment hash chain (S3.1 §5.2).
//!
//! [`SegmentChain`] is the coarse-granularity counterpart to
//! [`crate::chain::ReceiptChain`]:
//!
//! - `ReceiptChain` links receipts within a single segment via
//!   `previous_receipt_hash` (BLAKE3-truncated, 32 hex chars).
//! - `SegmentChain` links sealed segments via
//!   `previous_segment_seal_hash` (BLAKE3-256, 64 hex chars) per §5.2 line
//!   193.
//!
//! The chain only grows: `append` is the sole mutator, and there is no
//! `remove` / `replace` / `clear` on the public surface. Verification walks
//! every link and (in strict modes) every signature.

use ed25519_dalek::VerifyingKey;

use crate::error::EvidenceError;
use crate::segment::{SealedSegment, GENESIS_PREVIOUS_SEGMENT_SEAL_HASH};

/// Append-only ordered list of [`SealedSegment`]s belonging to a single
/// evidence stream.
///
/// **Public API exposes append-only writes.** `&mut self` exists for `append`,
/// but no public method removes, replaces, or reorders segments. Read access
/// is via [`Self::segments`] returning an immutable slice.
///
/// The chain is **not** thread-safe by itself; production callers wrap it in
/// a `Mutex` or use a single-writer task — same discipline as
/// [`crate::chain::ReceiptChain`].
#[derive(Debug, Clone, Default)]
pub struct SegmentChain {
    segments: Vec<SealedSegment>,
}

impl SegmentChain {
    /// Create an empty segment chain.
    ///
    /// The next [`Self::append`] call must carry a genesis segment
    /// (`previous_segment_id = None` and `previous_segment_seal_hash = None`).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Append a sealed segment to the chain.
    ///
    /// Verifies cross-segment linkage:
    ///
    /// - For the first append: the segment MUST be a genesis segment
    ///   (`previous_segment_id = None`). Otherwise returns
    ///   [`EvidenceError::SegmentChainBroken`] with index 0.
    /// - For subsequent appends: the segment MUST carry
    ///   `previous_segment_seal_hash = Some(prior.segment_seal_hash())` **and**
    ///   `previous_segment_id = Some(prior.segment_id())`. Otherwise returns
    ///   [`EvidenceError::SegmentChainBroken`].
    ///
    /// # Errors
    ///
    /// See the variant list above.
    pub fn append(&mut self, segment: SealedSegment) -> Result<(), EvidenceError> {
        let index = self.segments.len();
        match self.segments.last() {
            None => {
                // Genesis position: previous_segment_* must both be None.
                if segment.previous_segment_id().is_some()
                    || segment.previous_segment_seal_hash().is_some()
                {
                    return Err(EvidenceError::SegmentChainBroken {
                        index: 0,
                        actual: segment
                            .previous_segment_seal_hash()
                            .unwrap_or("<missing>")
                            .to_owned(),
                        expected: "<none — genesis segment must carry no previous link>".to_owned(),
                    });
                }
            }
            Some(prior) => {
                // Subsequent segment: both previous links must match the prior.
                let expected_hash = prior.segment_seal_hash();
                let Some(actual_hash) = segment.previous_segment_seal_hash() else {
                    return Err(EvidenceError::SegmentChainBroken {
                        index,
                        actual: "<missing previous_segment_seal_hash>".to_owned(),
                        expected: expected_hash.to_owned(),
                    });
                };
                if actual_hash != expected_hash {
                    return Err(EvidenceError::SegmentChainBroken {
                        index,
                        actual: actual_hash.to_owned(),
                        expected: expected_hash.to_owned(),
                    });
                }

                let expected_id = prior.segment_id();
                let Some(actual_id) = segment.previous_segment_id() else {
                    return Err(EvidenceError::SegmentChainBroken {
                        index,
                        actual: "<missing previous_segment_id>".to_owned(),
                        expected: expected_id.as_str().to_owned(),
                    });
                };
                if actual_id != expected_id {
                    return Err(EvidenceError::SegmentChainBroken {
                        index,
                        actual: actual_id.as_str().to_owned(),
                        expected: expected_id.as_str().to_owned(),
                    });
                }
            }
        }

        self.segments.push(segment);
        Ok(())
    }

    /// Read-only borrow of every sealed segment in append order.
    #[must_use]
    pub fn segments(&self) -> &[SealedSegment] {
        &self.segments
    }

    /// Number of segments in the chain.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.segments.len()
    }

    /// True if the chain is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Verify cross-segment hash chain integrity (no signature pass).
    ///
    /// Walks every adjacent pair `(prev, curr)` and confirms
    /// `curr.previous_segment_seal_hash == Some(prev.segment_seal_hash)` and
    /// `curr.previous_segment_id == Some(prev.segment_id)`. The genesis
    /// segment at index 0 must carry no previous link.
    ///
    /// # Errors
    ///
    /// - [`EvidenceError::SegmentChainBroken`] on the first link mismatch.
    pub fn verify_chain(&self) -> Result<(), EvidenceError> {
        // Genesis must carry no previous link.
        if let Some(first) = self.segments.first() {
            if first.previous_segment_id().is_some()
                || first
                    .previous_segment_seal_hash()
                    .is_some_and(|h| h != GENESIS_PREVIOUS_SEGMENT_SEAL_HASH)
            {
                return Err(EvidenceError::SegmentChainBroken {
                    index: 0,
                    actual: first
                        .previous_segment_seal_hash()
                        .unwrap_or("<missing>")
                        .to_owned(),
                    expected: "<none — genesis segment must carry no previous link>".to_owned(),
                });
            }
        }

        // Walk pairs.
        for (i, window) in self.segments.windows(2).enumerate() {
            let prev = &window[0];
            let curr = &window[1];
            let curr_index = i + 1;

            let Some(claimed_hash) = curr.previous_segment_seal_hash() else {
                return Err(EvidenceError::SegmentChainBroken {
                    index: curr_index,
                    actual: "<missing previous_segment_seal_hash>".to_owned(),
                    expected: prev.segment_seal_hash().to_owned(),
                });
            };
            if claimed_hash != prev.segment_seal_hash() {
                return Err(EvidenceError::SegmentChainBroken {
                    index: curr_index,
                    actual: claimed_hash.to_owned(),
                    expected: prev.segment_seal_hash().to_owned(),
                });
            }

            let Some(claimed_id) = curr.previous_segment_id() else {
                return Err(EvidenceError::SegmentChainBroken {
                    index: curr_index,
                    actual: "<missing previous_segment_id>".to_owned(),
                    expected: prev.segment_id().as_str().to_owned(),
                });
            };
            if claimed_id != prev.segment_id() {
                return Err(EvidenceError::SegmentChainBroken {
                    index: curr_index,
                    actual: claimed_id.as_str().to_owned(),
                    expected: prev.segment_id().as_str().to_owned(),
                });
            }
        }

        Ok(())
    }

    /// Verify cross-segment chain integrity **and** every segment's seal
    /// signature against `verifying_key`. Does NOT verify per-receipt
    /// signatures — use [`Self::verify_full`] for the all-up walk.
    ///
    /// # Errors
    ///
    /// - Everything [`Self::verify_chain`] returns, plus:
    /// - [`EvidenceError::SegmentSealMismatch`] / [`EvidenceError::SegmentSignatureMismatch`]
    ///   from any segment's seal recomputation / signature reject.
    pub fn verify_chain_signed(&self, verifying_key: &VerifyingKey) -> Result<(), EvidenceError> {
        self.verify_chain()?;
        for seg in &self.segments {
            seg.verify_seal(verifying_key)?;
        }
        Ok(())
    }

    /// Verify **everything**: cross-segment chain + every segment seal + every
    /// per-receipt signature in every segment.
    ///
    /// # Errors
    ///
    /// Union of [`Self::verify_chain_signed`] and per-receipt
    /// [`crate::receipt::EvidenceReceipt::verify_signature`].
    pub fn verify_full(&self, verifying_key: &VerifyingKey) -> Result<(), EvidenceError> {
        self.verify_chain()?;
        for seg in &self.segments {
            seg.verify_full(verifying_key)?;
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
    use crate::receipt::{EvidenceReceipt, ReceiptBuilder};
    use crate::record::{RecordType, RetentionClass};
    use crate::segment::Segment;
    use ed25519_dalek::SigningKey;
    use serde_json::json;

    fn test_keypair() -> (SigningKey, VerifyingKey) {
        let seed = [55u8; 32];
        let sk = SigningKey::from_bytes(&seed);
        let vk = sk.verifying_key();
        (sk, vk)
    }

    fn build_signed_segment(n: usize, sk: &SigningKey) -> Segment {
        let mut seg = Segment::new(RetentionClass::Standard24M);
        let mut prev: Option<EvidenceReceipt> = None;
        for i in 0..n {
            let r = ReceiptBuilder::new(
                RecordType::ActionReceived,
                RetentionClass::Standard24M,
                "service:capability-runtime",
            )
            .with_payload(json!({"step": i}))
            .seal_signed(prev.as_ref(), sk)
            .expect("seal_signed");
            seg.append(r.clone()).expect("append");
            prev = Some(r);
        }
        seg
    }

    #[test]
    fn new_segment_chain_is_empty() {
        let chain = SegmentChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
        assert!(chain.segments().is_empty());
    }

    #[test]
    fn segment_chain_append_three_genesis_then_linked_segments_verifies() {
        let (sk, vk) = test_keypair();
        let mut chain = SegmentChain::new();

        let seg1 = build_signed_segment(3, &sk);
        let sealed1 = seg1.seal(None, None, &sk).expect("seg1 seal");
        let prev_id = sealed1.segment_id().clone();
        let prev_hash = sealed1.segment_seal_hash().to_owned();
        chain.append(sealed1).expect("append seg1");

        let seg2 = build_signed_segment(3, &sk);
        let sealed2 = seg2
            .seal(Some(&prev_id), Some(&prev_hash), &sk)
            .expect("seg2 seal");
        let prev_id = sealed2.segment_id().clone();
        let prev_hash = sealed2.segment_seal_hash().to_owned();
        chain.append(sealed2).expect("append seg2");

        let seg3 = build_signed_segment(3, &sk);
        let sealed3 = seg3
            .seal(Some(&prev_id), Some(&prev_hash), &sk)
            .expect("seg3 seal");
        chain.append(sealed3).expect("append seg3");

        assert_eq!(chain.len(), 3);
        chain.verify_chain().expect("verify_chain happy path");
        chain
            .verify_chain_signed(&vk)
            .expect("verify_chain_signed happy path");
        chain
            .verify_full(&vk)
            .expect("verify_full happy path over 3 segments");
    }

    #[test]
    fn segment_chain_append_rejects_non_genesis_first_segment() {
        let (sk, _vk) = test_keypair();

        // Build a segment sealed WITH a fake previous link.
        let seg = build_signed_segment(2, &sk);
        let dummy_id = crate::segment::SegmentId::from_content(b"dummy-prev");
        let dummy_hash = "f".repeat(64);
        let sealed = seg
            .seal(Some(&dummy_id), Some(&dummy_hash), &sk)
            .expect("seal with fake link");

        let mut chain = SegmentChain::new();
        match chain.append(sealed) {
            Err(EvidenceError::SegmentChainBroken { index, .. }) => {
                assert_eq!(index, 0);
            }
            other => panic!("expected SegmentChainBroken at index 0, got {other:?}"),
        }
    }

    #[test]
    fn segment_chain_append_rejects_mismatched_previous_seal_hash() {
        let (sk, _vk) = test_keypair();
        let mut chain = SegmentChain::new();

        let seg1 = build_signed_segment(2, &sk);
        let sealed1 = seg1.seal(None, None, &sk).expect("seg1");
        chain.append(sealed1).expect("append seg1");

        // Build seg2 claiming a wrong previous seal hash.
        let seg2 = build_signed_segment(2, &sk);
        let real_prev_id = chain.segments()[0].segment_id().clone();
        let bogus_prev_hash = "0".repeat(64);
        let sealed2 = seg2
            .seal(Some(&real_prev_id), Some(&bogus_prev_hash), &sk)
            .expect("seg2 seal");

        match chain.append(sealed2) {
            Err(EvidenceError::SegmentChainBroken {
                index,
                actual,
                expected,
            }) => {
                assert_eq!(index, 1);
                assert_eq!(actual, bogus_prev_hash);
                assert_ne!(actual, expected);
            }
            other => panic!("expected SegmentChainBroken at index 1, got {other:?}"),
        }
    }

    #[test]
    fn segment_chain_verify_chain_signed_detects_tampered_segment() {
        let (sk, vk) = test_keypair();
        let mut chain = SegmentChain::new();

        let seg1 = build_signed_segment(2, &sk);
        let sealed1 = seg1.seal(None, None, &sk).expect("seg1");
        let prev_id = sealed1.segment_id().clone();
        let prev_hash = sealed1.segment_seal_hash().to_owned();
        chain.append(sealed1).expect("append seg1");

        let seg2 = build_signed_segment(2, &sk);
        let sealed2 = seg2
            .seal(Some(&prev_id), Some(&prev_hash), &sk)
            .expect("seg2");
        chain.append(sealed2).expect("append seg2");

        // Tamper: corrupt a receipt inside segment 0 via deserialize.
        let mut v = serde_json::to_value(chain.segments()).expect("ser");
        v[0]["receipts"][0]["payload"] = json!({"step": 9999});
        let mutated: Vec<SealedSegment> = serde_json::from_value(v).expect("de");
        let tampered = SegmentChain { segments: mutated };

        match tampered.verify_chain_signed(&vk) {
            Err(
                EvidenceError::SegmentSealMismatch { .. }
                | EvidenceError::SegmentSignatureMismatch
                | EvidenceError::SegmentChainBroken { .. },
            ) => {}
            other => panic!("expected tamper detection, got {other:?}"),
        }
    }

    #[test]
    fn segment_chain_default_equals_new() {
        let a = SegmentChain::default();
        let b = SegmentChain::new();
        assert_eq!(a.len(), b.len());
        assert_eq!(a.is_empty(), b.is_empty());
    }
}
