//! Error types for the Evidence Log (S3.1).
//!
//! Modeled on the `aios_action::error` patterns: closed taxonomy of failure modes,
//! `thiserror` derivation, no `panic!`, no `unwrap`. Each variant maps to a concrete
//! invariant from S3.1 §2 (append-only), §5 (hash chain), §6 (receipt shape), or
//! §11 (adversarial robustness).

use thiserror::Error;

/// Failure modes for evidence receipt construction, chain append, and chain
/// integrity verification.
///
/// Every variant maps to a concrete S3.1 invariant:
///
/// - [`Self::EncodingFailed`] — JCS / serde projection failure (§5.4: BLAKE3 over
///   canonical bytes).
/// - [`Self::HashMismatch`] — recomputed content hash does not match the receipt's
///   stored `content_hash` (§5.3 step 1).
/// - [`Self::ChainBroken`] — a receipt's `previous_receipt_hash` does not match the
///   prior receipt's content hash (§2 invariant 4).
/// - [`Self::GenesisMissing`] — a non-genesis receipt has `previous_receipt_hash = None`
///   (§5.1: every non-genesis receipt MUST chain-link to its predecessor).
/// - [`Self::EmptyChain`] — chain integrity was requested on a chain with no receipts.
/// - [`Self::InvalidReceiptId`] — receipt id failed `evr_<ULID>` validation
///   (delegated to [`aios_action::IdError`]).
/// - [`Self::InvalidSubject`] — `subject_canonical_id` is empty or malformed.
/// - [`Self::SignatureMalformed`] — receipt's `signature` field could not be
///   parsed as 128 lowercase hex chars decoding to a 64-byte Ed25519 signature
///   (T-009, §5.2 / §11.3).
/// - [`Self::SignatureMissing`] — signature verification was requested on a
///   receipt sealed without a signing key.
/// - [`Self::SignatureMismatch`] — Ed25519 verification rejected the signature
///   against the supplied verifying key. Constitutional tamper indicator per
///   §28.5 (`RECEIPT_FORGERY_DETECTED`).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EvidenceError {
    /// Canonical JSON projection failed during content-hash computation.
    ///
    /// Carries the underlying `aios_action::CanonicalError` rendered as text so the
    /// public API does not leak `serde_json`'s error type.
    #[error("evidence payload canonical encoding failed: {0}")]
    EncodingFailed(String),

    /// Recomputed BLAKE3 of canonical payload bytes does not match the stored
    /// `content_hash`. Indicates either tampering or a bug in the encoder; both
    /// trigger §11.5 tamper response in the production engine.
    #[error("content hash mismatch: expected `{expected}`, computed `{computed}`")]
    HashMismatch {
        /// The hash recorded on the receipt.
        expected: String,
        /// The hash recomputed during verification.
        computed: String,
    },

    /// The receipt's `previous_receipt_hash` does not match the prior receipt's
    /// canonical content hash, breaking the §5.1 per-segment chain invariant.
    #[error(
        "chain broken at receipt index {index}: previous_receipt_hash `{actual}` \
         does not match expected `{expected}`"
    )]
    ChainBroken {
        /// 0-based index of the offending receipt within the chain.
        index: usize,
        /// The hash the receipt claims to link to.
        actual: String,
        /// The hash computed from the prior receipt.
        expected: String,
    },

    /// A non-genesis receipt was appended without a `previous_receipt_hash`.
    ///
    /// The genesis receipt is the only one allowed to carry `previous_receipt_hash =
    /// None`; every other receipt MUST link backwards.
    #[error("non-genesis receipt is missing previous_receipt_hash (S3.1 §5.1)")]
    GenesisMissing,

    /// Integrity verification was requested on an empty chain.
    ///
    /// Distinguished from a healthy single-receipt chain (which contains the
    /// genesis receipt and is trivially valid).
    #[error("evidence chain is empty; integrity check has no receipts to walk")]
    EmptyChain,

    /// A genesis receipt was appended to a non-empty chain. The chain has exactly
    /// one genesis receipt (the first one); subsequent appends MUST carry
    /// `previous_receipt_hash = Some(...)`.
    #[error(
        "attempted to append a genesis receipt (previous_receipt_hash = None) to a non-empty chain"
    )]
    DuplicateGenesis,

    /// `receipt_id` is not a valid `evr_<ULID>` string per S0.1 §3.2.1.
    #[error("invalid receipt id: {detail}")]
    InvalidReceiptId {
        /// Stringified underlying [`aios_action::IdError`] for clarity.
        detail: String,
    },

    /// `subject_canonical_id` is empty or otherwise malformed.
    ///
    /// S3.1 §3 requires every receipt to identify its emitting subject; an empty
    /// subject is a constitutional defect because the subject is the audit anchor
    /// for L4 policy attribution.
    #[error("subject_canonical_id is invalid: {detail}")]
    InvalidSubject {
        /// Reason the subject was rejected.
        detail: String,
    },

    /// Ed25519 signature is structurally invalid: not 128 lowercase hex chars,
    /// or does not decode to a 64-byte signature value.
    ///
    /// Distinct from [`Self::SignatureMismatch`], which carries a structurally
    /// valid signature that fails cryptographic verification. T-009 / S3.1
    /// §5.2 / §11.3.
    #[error("evidence signature is malformed: {detail}")]
    SignatureMalformed {
        /// Reason the signature blob was rejected (hex parse, wrong length, etc.).
        detail: String,
    },

    /// Signature verification was requested on a receipt whose `signature`
    /// field is `None`.
    ///
    /// Used to distinguish "this receipt is unsigned" from "this receipt
    /// claims a signature but the signature is bad" — the production engine
    /// emits different record types for the two cases (an unsigned receipt is
    /// a configuration / migration concern; a bad signature is `RECEIPT_FORGERY_DETECTED`).
    /// T-009 / S3.1 §5.2 / §11.3 / §28.5.
    #[error("evidence receipt carries no signature; cannot verify")]
    SignatureMissing,

    /// Ed25519 verification rejected the signature against the supplied
    /// verifying key.
    ///
    /// Constitutional tamper indicator. Production maps this to a
    /// `RECEIPT_FORGERY_DETECTED` record per S3.1 §28.5 — receipt's Ed25519
    /// signature did not verify against the vault capability bound to the
    /// claimed `subject_canonical_id`.
    #[error("evidence signature verification failed (Ed25519 reject)")]
    SignatureMismatch,

    /// `Segment::append` was called on a segment that has already been sealed.
    ///
    /// After [`crate::segment::Segment::seal`] returns a [`crate::segment::SealedSegment`],
    /// the original segment is consumed and no further receipts may be added —
    /// constitutional INV-005 extends from individual receipts to segment
    /// boundaries (T-010, S3.1 §7).
    #[error("segment is already sealed; cannot append further receipts")]
    SegmentAlreadySealed,

    /// `Segment::seal` was called on a segment containing zero receipts.
    ///
    /// S3.1 §7.1 derives the segment id from `<genesis_receipt_id> +
    /// <sealed_at_timestamp>` — an empty segment has no genesis receipt and
    /// therefore no well-defined identity. The engine seals on size or age
    /// thresholds (§7.3); both require at least one receipt to fire.
    #[error("evidence segment has no receipts; cannot seal an empty segment")]
    EmptySegment,

    /// Cross-segment hash chain is broken: the appended segment's
    /// `previous_segment_seal_hash` does not match the prior segment's
    /// `segment_seal_hash` (S3.1 §5.2 line 193).
    ///
    /// Distinguished from [`Self::ChainBroken`], which fires on per-receipt
    /// links inside a single segment.
    #[error(
        "segment chain broken at segment index {index}: previous_segment_seal_hash `{actual}` \
         does not match expected `{expected}`"
    )]
    SegmentChainBroken {
        /// 0-based index of the offending segment within the segment chain.
        index: usize,
        /// The hash the appended segment claims to link to.
        actual: String,
        /// The seal hash computed from the prior segment.
        expected: String,
    },

    /// Recomputed `segment_seal_hash` differs from the stored value on the
    /// [`crate::segment::SealedSegment`].
    ///
    /// Indicates tampering with one of the sealed segment's receipts (which
    /// would change the canonical-all-receipts hash) or with the segment
    /// metadata (receipt count, previous-seal hash). T-010 / S3.1 §5.2.
    #[error("segment seal hash mismatch: expected `{expected}`, computed `{computed}`")]
    SegmentSealMismatch {
        /// The seal hash recorded on the [`crate::segment::SealedSegment`].
        expected: String,
        /// The seal hash recomputed during verification.
        computed: String,
    },

    /// Ed25519 verification rejected the segment-level signature against the
    /// supplied verifying key.
    ///
    /// Distinct from [`Self::SignatureMismatch`], which covers per-receipt
    /// signatures. Constitutional tamper indicator at the segment boundary
    /// (T-010 / S3.1 §11.3) — production maps this to a tamper response per
    /// §11.5 and the engine enters degraded mode.
    #[error("segment signature verification failed (Ed25519 reject)")]
    SegmentSignatureMismatch,
}

impl From<aios_action::CanonicalError> for EvidenceError {
    fn from(err: aios_action::CanonicalError) -> Self {
        Self::EncodingFailed(err.to_string())
    }
}

impl From<aios_action::IdError> for EvidenceError {
    fn from(err: aios_action::IdError) -> Self {
        Self::InvalidReceiptId {
            detail: err.to_string(),
        }
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

    #[test]
    fn display_renders_each_variant_with_useful_context() {
        let e = EvidenceError::EncodingFailed("boom".to_owned());
        assert!(e.to_string().contains("boom"));

        let e = EvidenceError::HashMismatch {
            expected: "aaaa".to_owned(),
            computed: "bbbb".to_owned(),
        };
        assert!(e.to_string().contains("aaaa"));
        assert!(e.to_string().contains("bbbb"));

        let e = EvidenceError::ChainBroken {
            index: 3,
            actual: "xx".to_owned(),
            expected: "yy".to_owned(),
        };
        assert!(e.to_string().contains('3'));

        assert!(EvidenceError::GenesisMissing
            .to_string()
            .contains("genesis"));
        assert!(EvidenceError::EmptyChain.to_string().contains("empty"));
        assert!(EvidenceError::DuplicateGenesis
            .to_string()
            .contains("genesis"));

        let e = EvidenceError::SignatureMalformed {
            detail: "expected 128 hex chars, got 64".to_owned(),
        };
        assert!(e.to_string().contains("malformed"));
        assert!(e.to_string().contains("128 hex chars"));

        assert!(EvidenceError::SignatureMissing
            .to_string()
            .contains("no signature"));

        assert!(EvidenceError::SignatureMismatch
            .to_string()
            .contains("Ed25519"));
    }

    #[test]
    fn from_id_error_carries_text() {
        let id_err = aios_action::IdError::Empty;
        let e: EvidenceError = id_err.clone().into();
        match e {
            EvidenceError::InvalidReceiptId { detail } => {
                assert_eq!(detail, id_err.to_string());
            }
            other => panic!("expected InvalidReceiptId, got {other:?}"),
        }
    }
}
