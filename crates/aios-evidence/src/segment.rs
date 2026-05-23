//! Segment sealing + per-segment Ed25519 signature (S3.1 §5.2 / §7).
//!
//! T-010 introduces the **segment** abstraction on top of the per-receipt
//! [`crate::receipt::EvidenceReceipt`] / [`crate::chain::ReceiptChain`] surface:
//!
//! - [`SegmentId`] — content-addressed `seg_<32hex>` identifier derived per
//!   S3.1 §7.1 from `BLAKE3(genesis_receipt_id + sealed_at_timestamp)`.
//! - [`Segment`] — open, append-only collection of [`EvidenceReceipt`]s for a
//!   single segment window. Becomes consumed by [`Segment::seal`].
//! - [`SealedSegment`] — immutable terminal segment shape carrying:
//!     1. every receipt (including a final `SEGMENT_SEALED` terminal receipt),
//!     2. the segment seal hash (BLAKE3-256 over canonical segment metadata),
//!     3. an Ed25519 segment-level signature over that seal hash,
//!     4. the cross-segment link (`previous_segment_id` +
//!        `previous_segment_seal_hash`).
//!
//! ## Why segments matter constitutionally
//!
//! - **Retention enforcement (S3.1 §13).** GC runs at segment granularity:
//!   `STANDARD_24M` segments expire at 24 months, `EXTENDED_60M` at 60,
//!   `FOREVER` never.
//! - **Defense-in-depth signature (§11.3).** Per-receipt signing (T-009)
//!   catches single-receipt tamper; the segment-level signature catches
//!   reordering, deletion, or substitution of entire receipts within a
//!   segment.
//! - **Cross-segment chain (§5.2).** Segments link via
//!   `previous_segment_seal_hash`, forming a second hash chain at coarser
//!   granularity. The first segment carries 64 `'0'` chars per §5.2 line 193.
//!
//! ## Production key acquisition (deferred to S5.2)
//!
//! Segment signing uses the same constitutional subject as per-receipt signing:
//! `_system:service:evidence-segment-signer` (§11.3). Production resolves the
//! signing capability via the L4.2 Vault Broker; tests use ephemeral
//! [`ed25519_dalek::SigningKey`] values. The signed surface is the
//! BLAKE3-256-hex digest of the canonical seal metadata — re-keying changes
//! key custody, not the signed bytes.

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::EvidenceError;
use crate::receipt::{EvidenceReceipt, ReceiptBuilder};
use crate::record::{RecordType, RetentionClass};

/// `previous_segment_seal_hash` carried by the first segment in the chain.
///
/// S3.1 §5.2 line 193: "The first segment's value is `\"0000...0000\"`." The
/// length matches the BLAKE3-256 hex digest (64 lowercase hex chars).
pub const GENESIS_PREVIOUS_SEGMENT_SEAL_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

// =====================================================================
// SegmentId — `seg_<32hex>` content-addressed identifier
// =====================================================================

/// Segment identifier — S3.1 §7.1.
///
/// Canonical form: `seg_` + 32 lowercase hex chars where the body is
/// `hex_lower(BLAKE3(<seed_bytes>))[:32]`. The seed bytes are
/// `<genesis_receipt_id>` concatenated with `<sealed_at_timestamp>` (RFC 3339
/// UTC) per §7.1.
///
/// Like [`aios_action::TransitionPlanId`], `SegmentId` deliberately has **no
/// random `new()` constructor**: identical canonical seed bytes MUST produce
/// byte-identical ids. The only way to mint a `SegmentId` is via
/// [`Self::from_content`], applied to the same canonical seed every replay.
///
/// ## Wave-11 constitutional rule
///
/// Colon-separated forms (`seg:abc...`) are forbidden and rejected with
/// [`EvidenceError::InvalidReceiptId`] (carrying the colon-form detail) for
/// consistency with the rest of the id surface.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SegmentId(String);

impl SegmentId {
    /// The canonical prefix including the trailing underscore.
    pub const PREFIX: &'static str = "seg_";

    /// Required body length: 32 lowercase hex chars = 128-bit BLAKE3 truncation.
    pub const BODY_LEN: usize = 32;

    /// Derive a deterministic id from already-canonicalized seed bytes.
    ///
    /// Per S3.1 §7.1 the canonical seed is
    /// `<genesis_receipt_id> + <sealed_at_timestamp>`. The caller is
    /// responsible for assembling that seed; this constructor does not
    /// canonicalize for you — that decision lives one layer up so the
    /// canonical form remains explicit at every call site.
    #[must_use]
    pub fn from_content(seed_bytes: &[u8]) -> Self {
        let body = aios_action::blake3_truncated(seed_bytes);
        Self(format!("{}{body}", Self::PREFIX))
    }

    /// Validate and adopt an externally supplied string.
    ///
    /// # Errors
    ///
    /// Returns [`EvidenceError::InvalidReceiptId`] (re-used here as the
    /// generic "id is malformed" channel) when:
    /// - the input is empty,
    /// - the input uses the forbidden colon separator (`seg:abc...`),
    /// - the prefix is not `seg_`,
    /// - the body is not exactly 32 lowercase hex characters.
    pub fn parse(input: &str) -> Result<Self, EvidenceError> {
        if input.is_empty() {
            return Err(EvidenceError::InvalidReceiptId {
                detail: "segment id is empty".to_owned(),
            });
        }

        if input.starts_with("seg:") {
            return Err(EvidenceError::InvalidReceiptId {
                detail: format!("colon-separated segment id form is forbidden: `{input}`"),
            });
        }

        let Some(body) = input.strip_prefix(Self::PREFIX) else {
            return Err(EvidenceError::InvalidReceiptId {
                detail: format!("expected `seg_` prefix, got `{input}`"),
            });
        };

        if body.len() != Self::BODY_LEN {
            return Err(EvidenceError::InvalidReceiptId {
                detail: format!(
                    "expected {}-char lowercase hex body, got {} chars",
                    Self::BODY_LEN,
                    body.len()
                ),
            });
        }

        if !body
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        {
            return Err(EvidenceError::InvalidReceiptId {
                detail: format!("segment id body must be lowercase hex [0-9a-f] only: `{body}`"),
            });
        }

        Ok(Self(input.to_owned()))
    }

    /// Borrow the canonical `seg_<32hex>` string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for SegmentId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for SegmentId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// =====================================================================
// Segment — open, append-only collection prior to sealing
// =====================================================================

/// Open evidence segment.
///
/// A `Segment` holds an in-order list of [`EvidenceReceipt`]s plus the
/// segment-wide [`RetentionClass`]. Receipts are appended via
/// [`Self::append`] until [`Self::seal`] consumes the segment and produces an
/// immutable [`SealedSegment`].
///
/// The segment does **not** itself enforce hash-chain linkage between
/// receipts — callers are expected to use [`ReceiptBuilder::seal`] /
/// [`ReceiptBuilder::seal_signed`] with the previous receipt before appending,
/// exactly as with [`crate::chain::ReceiptChain`]. The segment's role is the
/// coarser-granularity ceremony: terminal `SEGMENT_SEALED` receipt + segment
/// signature + cross-segment chain.
#[derive(Debug, Clone)]
pub struct Segment {
    retention_class: RetentionClass,
    receipts: Vec<EvidenceReceipt>,
    is_sealed: bool,
}

impl Segment {
    /// Create a fresh, empty, open segment with the given retention class.
    ///
    /// The retention class is fixed at creation and applies to the entire
    /// segment per S3.1 §13. Once sealed, the [`SealedSegment`] carries this
    /// class forward as the unit of garbage collection.
    #[must_use]
    pub const fn new(retention_class: RetentionClass) -> Self {
        Self {
            retention_class,
            receipts: Vec::new(),
            is_sealed: false,
        }
    }

    /// Append a sealed receipt to this open segment.
    ///
    /// # Errors
    ///
    /// Returns [`EvidenceError::SegmentAlreadySealed`] if [`Self::seal`] has
    /// already been called on this segment (cannot happen via the public API
    /// today because `seal` consumes `self`, but the check is preserved for
    /// defence-in-depth against future internal-mutation code paths).
    pub fn append(&mut self, receipt: EvidenceReceipt) -> Result<(), EvidenceError> {
        if self.is_sealed {
            return Err(EvidenceError::SegmentAlreadySealed);
        }
        self.receipts.push(receipt);
        Ok(())
    }

    /// Number of receipts currently in the segment (excluding the future
    /// terminal `SEGMENT_SEALED` receipt that [`Self::seal`] will append).
    #[must_use]
    pub const fn len(&self) -> usize {
        self.receipts.len()
    }

    /// True if the segment has zero receipts.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.receipts.is_empty()
    }

    /// True if [`Self::seal`] has already been called. Always false in the
    /// current public API (seal consumes self), but available for defensive
    /// callers introspecting an internal `Segment` reference.
    #[must_use]
    pub const fn is_sealed(&self) -> bool {
        self.is_sealed
    }

    /// Read-only view of the receipts currently in the segment.
    #[must_use]
    pub fn receipts(&self) -> &[EvidenceReceipt] {
        &self.receipts
    }

    /// Retention class for the entire segment (S3.1 §13).
    #[must_use]
    pub const fn retention_class(&self) -> RetentionClass {
        self.retention_class
    }

    /// Seal the segment with an Ed25519 signature.
    ///
    /// Steps (T-010, S3.1 §5.2 / §7.3):
    ///
    /// 1. Reject empty segments with [`EvidenceError::EmptySegment`].
    /// 2. Compute the canonical-all-receipts hash:
    ///    `BLAKE3_256_HEX(JCS(<all receipts in order>))`.
    /// 3. Derive the [`SegmentId`] per §7.1:
    ///    `seg_` + `BLAKE3_truncated(<genesis_receipt_id_bytes> +
    ///    <sealed_at_rfc3339_bytes>)[:32]`.
    /// 4. Build the terminal `SEGMENT_SEALED` receipt via [`ReceiptBuilder`],
    ///    chaining to the last receipt currently in the segment, and Ed25519-
    ///    sign it with the same `signing_key`. Append it to the receipts vec.
    /// 5. Compute the **segment seal hash**:
    ///    `BLAKE3_256_HEX(JCS(<canonical seal metadata>))` where the metadata
    ///    is a closed struct containing the segment id, retention class,
    ///    receipt count, previous-segment-seal hash, and the canonical-all-
    ///    receipts hash from step 2.
    /// 6. Ed25519-sign the segment seal hash hex bytes (mirroring the T-009
    ///    per-receipt signing pattern: signature is over the hex digest
    ///    string, not the raw 32 bytes).
    ///
    /// `previous_segment_id` and `previous_segment_seal_hash` are `None` for
    /// the genesis segment of the chain; subsequent segments MUST supply both.
    /// If they disagree (`Some(id) + None` or vice-versa) the call fails with
    /// [`EvidenceError::SegmentChainBroken`].
    ///
    /// # Errors
    ///
    /// - [`EvidenceError::EmptySegment`] if the segment has no receipts.
    /// - [`EvidenceError::SegmentChainBroken`] if only one of
    ///   `previous_segment_id` / `previous_segment_seal_hash` is supplied.
    /// - [`EvidenceError::EncodingFailed`] on JCS projection failure.
    /// - [`EvidenceError::InvalidSubject`] is impossible here (subject is a
    ///   fixed constant) but is in the propagated error set for clarity.
    pub fn seal(
        mut self,
        previous_segment_id: Option<&SegmentId>,
        previous_segment_seal_hash: Option<&str>,
        signing_key: &SigningKey,
    ) -> Result<SealedSegment, EvidenceError> {
        // 1. Reject empty segments.
        if self.receipts.is_empty() {
            return Err(EvidenceError::EmptySegment);
        }

        // Validate the cross-segment linkage shape: both Some or both None.
        match (previous_segment_id, previous_segment_seal_hash) {
            (Some(_), Some(_)) | (None, None) => {}
            (Some(_), None) | (None, Some(_)) => {
                return Err(EvidenceError::SegmentChainBroken {
                    index: 0,
                    actual: previous_segment_seal_hash.unwrap_or("<missing>").to_owned(),
                    expected: previous_segment_id
                        .map_or_else(|| "<missing>".to_owned(), |id| id.as_str().to_owned()),
                });
            }
        }

        // 2. Canonical-all-receipts hash (over receipts BEFORE the terminal
        //    SEGMENT_SEALED record — the terminal record is the *witness* of
        //    the seal, not part of the content it witnesses).
        let canonical_receipts = aios_action::jcs_canonicalize(&self.receipts)?;
        let all_receipts_canonical_hash = aios_action::blake3_hash(canonical_receipts.as_bytes());

        // 3. Mint the segment id.
        let sealed_at = Utc::now();
        // S3.1 §7.1: id seed is `<genesis_receipt_id> + <sealed_at_timestamp>`.
        // `genesis_receipt_id` is the receipt-id of the segment's first non-
        // seal receipt. RFC 3339 is the canonical timestamp encoding used
        // throughout S0.1 / S3.1.
        let genesis_receipt = self
            .receipts
            .first()
            .ok_or(EvidenceError::EmptySegment)?
            .receipt_id();
        let mut seed = Vec::with_capacity(64);
        seed.extend_from_slice(genesis_receipt.as_str().as_bytes());
        seed.extend_from_slice(sealed_at.to_rfc3339().as_bytes());
        let segment_id = SegmentId::from_content(&seed);

        // Resolve the previous-segment-seal hash carried INTO the seal
        // metadata. The cross-segment chain uses 64-zero hex for the genesis
        // segment per S3.1 §5.2 line 193.
        let prev_seal_hash_for_metadata = previous_segment_seal_hash
            .unwrap_or(GENESIS_PREVIOUS_SEGMENT_SEAL_HASH)
            .to_owned();

        // 4. Append the terminal SEGMENT_SEALED receipt.
        //
        // The terminal receipt is itself per-receipt signed by the same key
        // (single signing capability across the whole evidence stream per
        // T-009). Its payload carries the segment-id-bound metadata so a
        // reader walking just the receipt stream can locate every segment
        // boundary without needing the SealedSegment envelope.
        let terminal_payload = SegmentSealedPayload {
            segment_id: segment_id.as_str().to_owned(),
            retention_class: self.retention_class.as_wire_str().to_owned(),
            receipt_count: u32::try_from(self.receipts.len()).unwrap_or(u32::MAX),
            previous_segment_id: previous_segment_id.map(|id| id.as_str().to_owned()),
            previous_segment_seal_hash: previous_segment_seal_hash.map(str::to_owned),
        };
        let terminal_payload_json = serde_json::to_value(&terminal_payload).map_err(|e| {
            EvidenceError::EncodingFailed(format!("terminal SEGMENT_SEALED payload: {e}"))
        })?;

        // Subject for system-emitted seal records per S3.1 §11.3.
        let segment_signer_subject = "_system:service:evidence-segment-signer";
        let terminal_builder = ReceiptBuilder::new(
            RecordType::SegmentSealed,
            RetentionClass::Forever,
            segment_signer_subject,
        )
        .with_payload(terminal_payload_json);

        let terminal_receipt = terminal_builder.seal_signed(self.receipts.last(), signing_key)?;
        self.receipts.push(terminal_receipt);
        self.is_sealed = true;

        // 5. Segment seal hash over canonical metadata.
        let seal_meta = SegmentSealMetadata {
            segment_id: segment_id.as_str(),
            retention_class: self.retention_class.as_wire_str(),
            receipt_count: u64::try_from(self.receipts.len()).unwrap_or(u64::MAX),
            previous_segment_seal_hash: &prev_seal_hash_for_metadata,
            all_receipts_canonical_hash: &all_receipts_canonical_hash,
        };
        let canonical_meta = aios_action::jcs_canonicalize(&seal_meta)?;
        let segment_seal_hash = aios_action::blake3_hash(canonical_meta.as_bytes());

        // 6. Ed25519-sign the segment seal hash hex string bytes.
        let signature: Signature = signing_key.sign(segment_seal_hash.as_bytes());
        let segment_signature_hex = encode_signature_hex_64(&signature.to_bytes());

        Ok(SealedSegment {
            segment_id,
            retention_class: self.retention_class,
            receipts: self.receipts,
            segment_seal_hash,
            segment_signature: segment_signature_hex,
            previous_segment_id: previous_segment_id.cloned(),
            previous_segment_seal_hash: previous_segment_seal_hash.map(str::to_owned),
            sealed_at,
        })
    }
}

// =====================================================================
// SealedSegment — terminal immutable segment shape
// =====================================================================

/// Immutable, sealed evidence segment.
///
/// **All fields are private.** The only construction path is via
/// [`Segment::seal`], which consumes the open segment by value. After that
/// point, the segment's content is cryptographically frozen — INV-005
/// extended to the segment boundary.
///
/// `SealedSegment` deliberately exposes only `&self` accessors. There is no
/// `&mut self` method. Cloning is permitted because cloned values are
/// content-identical to their source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedSegment {
    segment_id: SegmentId,
    retention_class: RetentionClass,
    receipts: Vec<EvidenceReceipt>,
    segment_seal_hash: String,
    segment_signature: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_segment_id: Option<SegmentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_segment_seal_hash: Option<String>,
    sealed_at: DateTime<Utc>,
}

impl SealedSegment {
    /// Canonical `seg_<32hex>` identity of this segment.
    #[must_use]
    pub const fn segment_id(&self) -> &SegmentId {
        &self.segment_id
    }

    /// Retention class for the entire segment.
    #[must_use]
    pub const fn retention_class(&self) -> RetentionClass {
        self.retention_class
    }

    /// All receipts in the sealed segment, in order.
    ///
    /// The last entry is the terminal `SEGMENT_SEALED` witness emitted by
    /// [`Segment::seal`].
    #[must_use]
    pub fn receipts(&self) -> &[EvidenceReceipt] {
        &self.receipts
    }

    /// BLAKE3-256 hex (64 chars) of the canonical segment seal metadata.
    ///
    /// Carrying surface for cross-segment chain linkage (§5.2): the next
    /// segment's [`Segment::seal`] call passes this value as
    /// `previous_segment_seal_hash`.
    #[must_use]
    pub fn segment_seal_hash(&self) -> &str {
        &self.segment_seal_hash
    }

    /// 128-char lowercase hex Ed25519 signature over [`Self::segment_seal_hash`].
    #[must_use]
    pub fn segment_signature(&self) -> &str {
        &self.segment_signature
    }

    /// Cross-segment chain link: the previous segment's id, or `None` for the
    /// genesis segment.
    #[must_use]
    pub const fn previous_segment_id(&self) -> Option<&SegmentId> {
        self.previous_segment_id.as_ref()
    }

    /// Cross-segment chain link: the previous segment's seal hash, or `None`
    /// for the genesis segment.
    #[must_use]
    pub fn previous_segment_seal_hash(&self) -> Option<&str> {
        self.previous_segment_seal_hash.as_deref()
    }

    /// Server-authoritative wall-clock timestamp at which the segment was
    /// sealed (S3.1 §11.2).
    #[must_use]
    pub const fn sealed_at(&self) -> DateTime<Utc> {
        self.sealed_at
    }

    /// Total number of receipts in the sealed segment, including the terminal
    /// `SEGMENT_SEALED` witness.
    #[must_use]
    pub const fn receipt_count(&self) -> usize {
        self.receipts.len()
    }

    /// Recompute the canonical-all-receipts hash by re-hashing every receipt
    /// **except** the terminal `SEGMENT_SEALED` witness. Returns the hex
    /// digest.
    fn recompute_all_receipts_canonical_hash(&self) -> Result<String, EvidenceError> {
        // Strip the terminal SEGMENT_SEALED receipt — it was appended *after*
        // the canonical-all-receipts hash was computed in `Segment::seal`.
        let body = self.receipts_excluding_terminal();
        let canonical = aios_action::jcs_canonicalize(&body)?;
        Ok(aios_action::blake3_hash(canonical.as_bytes()))
    }

    /// Borrow the receipts slice excluding the terminal `SEGMENT_SEALED`
    /// witness. If, somehow, the last receipt is not a `SEGMENT_SEALED` (e.g.
    /// after malformed deserialization), returns the full slice — verification
    /// will then detect the mismatch via the seal-hash recomputation.
    fn receipts_excluding_terminal(&self) -> &[EvidenceReceipt] {
        match self.receipts.last() {
            Some(last) if last.record_type() == RecordType::SegmentSealed => {
                &self.receipts[..self.receipts.len() - 1]
            }
            _ => &self.receipts,
        }
    }

    /// Verify the segment-level seal: recompute the canonical metadata hash,
    /// then re-verify the Ed25519 signature against `verifying_key`.
    ///
    /// Does **not** verify per-receipt signatures — use [`Self::verify_full`]
    /// for the all-up walk.
    ///
    /// # Errors
    ///
    /// - [`EvidenceError::EncodingFailed`] on JCS projection failure.
    /// - [`EvidenceError::SegmentSealMismatch`] if the recomputed seal hash
    ///   differs from the stored value (tamper on metadata or receipts).
    /// - [`EvidenceError::SegmentSignatureMismatch`] if Ed25519 verification
    ///   rejects the signature against the supplied key.
    /// - [`EvidenceError::SignatureMalformed`] if the segment signature blob
    ///   is not 128 lowercase hex chars decoding to a 64-byte signature.
    pub fn verify_seal(&self, verifying_key: &VerifyingKey) -> Result<(), EvidenceError> {
        // Recompute canonical-all-receipts hash (excluding the terminal
        // SEGMENT_SEALED witness).
        let all_receipts_canonical_hash = self.recompute_all_receipts_canonical_hash()?;

        // Reconstruct the seal metadata using the receipt count INCLUDING the
        // terminal witness (that's what `Segment::seal` recorded in step 5).
        let prev_hash_for_meta = self
            .previous_segment_seal_hash
            .clone()
            .unwrap_or_else(|| GENESIS_PREVIOUS_SEGMENT_SEAL_HASH.to_owned());
        let seal_meta = SegmentSealMetadata {
            segment_id: self.segment_id.as_str(),
            retention_class: self.retention_class.as_wire_str(),
            receipt_count: u64::try_from(self.receipts.len()).unwrap_or(u64::MAX),
            previous_segment_seal_hash: &prev_hash_for_meta,
            all_receipts_canonical_hash: &all_receipts_canonical_hash,
        };
        let canonical_meta = aios_action::jcs_canonicalize(&seal_meta)?;
        let computed_seal_hash = aios_action::blake3_hash(canonical_meta.as_bytes());

        if computed_seal_hash != self.segment_seal_hash {
            return Err(EvidenceError::SegmentSealMismatch {
                expected: self.segment_seal_hash.clone(),
                computed: computed_seal_hash,
            });
        }

        // Verify the Ed25519 signature over the seal hash hex string bytes.
        let sig_bytes = decode_signature_hex_64(&self.segment_signature)?;
        let signature = Signature::from_bytes(&sig_bytes);
        verifying_key
            .verify(self.segment_seal_hash.as_bytes(), &signature)
            .map_err(|_| EvidenceError::SegmentSignatureMismatch)
    }

    /// Verify per-receipt signatures **and** the segment seal in one walk.
    ///
    /// # Errors
    ///
    /// Returns the first failure encountered. Failure modes are the union of
    /// [`Self::verify_seal`] and per-receipt
    /// [`EvidenceReceipt::verify_signature`].
    pub fn verify_full(&self, verifying_key: &VerifyingKey) -> Result<(), EvidenceError> {
        // Per-receipt signature pass — short-circuit on first failure.
        for r in &self.receipts {
            r.verify_signature(verifying_key)?;
        }
        self.verify_seal(verifying_key)
    }
}

// =====================================================================
// Payload + metadata helper structs
// =====================================================================

/// JSON payload of the terminal `SEGMENT_SEALED` evidence receipt.
///
/// Mirrors the proto `SegmentSealedPayload` shape from S3.1 §5.2 / Appendix A
/// at the JSON-payload level. The full proto message also carries
/// `genesis_receipt_id`, `final_receipt_hash`, `segment_signature`,
/// `signing_key_id`, and `sealed_at`; T-010 surfaces only the fields needed
/// for the in-receipt witness (the rest live on the [`SealedSegment`]
/// envelope and can be reconstructed deterministically).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SegmentSealedPayload {
    segment_id: String,
    retention_class: String,
    receipt_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_segment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_segment_seal_hash: Option<String>,
}

/// Canonical metadata struct whose JCS form is the byte sequence covered by
/// the segment seal hash + signature.
///
/// Kept private — the only place this is constructed is inside `Segment::seal`
/// and `SealedSegment::verify_seal`, and the two MUST produce byte-identical
/// JCS output.
#[derive(Debug, Serialize)]
struct SegmentSealMetadata<'a> {
    segment_id: &'a str,
    retention_class: &'a str,
    receipt_count: u64,
    previous_segment_seal_hash: &'a str,
    all_receipts_canonical_hash: &'a str,
}

// =====================================================================
// Internal signature hex codec — duplicated from receipt.rs to avoid
// widening the receipt module's public surface.
// =====================================================================

const SIGNATURE_LENGTH: usize = ed25519_dalek::SIGNATURE_LENGTH;

/// Lowercase-hex encode a 64-byte Ed25519 signature.
fn encode_signature_hex_64(sig: &[u8; SIGNATURE_LENGTH]) -> String {
    let mut out = String::with_capacity(SIGNATURE_LENGTH * 2);
    for byte in sig {
        use core::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Parse a 128-char lowercase-hex Ed25519 signature into a 64-byte array.
fn decode_signature_hex_64(hex: &str) -> Result<[u8; SIGNATURE_LENGTH], EvidenceError> {
    let expected_len = SIGNATURE_LENGTH * 2;
    if hex.len() != expected_len {
        return Err(EvidenceError::SignatureMalformed {
            detail: format!(
                "expected {expected_len} lowercase hex chars on segment signature, got {} chars",
                hex.len()
            ),
        });
    }
    let bytes = hex.as_bytes();
    let mut out = [0u8; SIGNATURE_LENGTH];
    for (i, chunk) in bytes.chunks_exact(2).enumerate() {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_nibble(c: u8) -> Result<u8, EvidenceError> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        other => Err(EvidenceError::SignatureMalformed {
            detail: format!(
                "non-lowercase-hex byte 0x{other:02x} in Ed25519 segment signature blob"
            ),
        }),
    }
}

// `_value` is consumed by `serde_json::Value` field references in tests; we
// keep the alias to clarify intent at the imports level.
#[allow(dead_code)]
type _ValuePin = Value;

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_keypair() -> (SigningKey, VerifyingKey) {
        let seed = [21u8; 32];
        let sk = SigningKey::from_bytes(&seed);
        let vk = sk.verifying_key();
        (sk, vk)
    }

    fn test_keypair_other() -> (SigningKey, VerifyingKey) {
        let seed = [88u8; 32];
        let sk = SigningKey::from_bytes(&seed);
        let vk = sk.verifying_key();
        (sk, vk)
    }

    /// Build an open segment with `n` signed receipts chained head-to-tail.
    /// Returns the open `Segment`.
    fn build_open_segment(n: usize, sk: &SigningKey) -> Segment {
        let mut seg = Segment::new(RetentionClass::Standard24M);
        let mut prev: Option<EvidenceReceipt> = None;
        for i in 0..n {
            let builder = ReceiptBuilder::new(
                RecordType::ActionReceived,
                RetentionClass::Standard24M,
                "service:capability-runtime",
            )
            .with_payload(json!({"step": i}));
            let r = builder.seal_signed(prev.as_ref(), sk).expect("seal_signed");
            seg.append(r.clone()).expect("append");
            prev = Some(r);
        }
        seg
    }

    // ─── SegmentId ────────────────────────────────────────────────────

    #[test]
    fn segment_id_from_content_is_deterministic() {
        let a = SegmentId::from_content(b"hello");
        let b = SegmentId::from_content(b"hello");
        assert_eq!(a, b);
        assert!(a.as_str().starts_with("seg_"));
        assert_eq!(a.as_str().len(), "seg_".len() + 32);
    }

    #[test]
    fn segment_id_different_content_yields_different_ids() {
        let a = SegmentId::from_content(b"hello");
        let b = SegmentId::from_content(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn segment_id_parse_round_trips_derived_value() {
        let derived = SegmentId::from_content(b"some-canonical-seed");
        let reparsed = SegmentId::parse(derived.as_str()).expect("derived id must reparse");
        assert_eq!(reparsed, derived);
    }

    #[test]
    fn segment_id_parse_rejects_empty() {
        let err = SegmentId::parse("").expect_err("empty must fail");
        match err {
            EvidenceError::InvalidReceiptId { detail } => {
                assert!(detail.contains("empty"));
            }
            other => panic!("expected InvalidReceiptId, got {other:?}"),
        }
    }

    #[test]
    fn segment_id_parse_rejects_colon_separator() {
        let colon_form = "seg:0123456789abcdef0123456789abcdef";
        let err = SegmentId::parse(colon_form).expect_err("colon form must fail");
        match err {
            EvidenceError::InvalidReceiptId { detail } => {
                assert!(detail.contains("colon-separated"));
            }
            other => panic!("expected InvalidReceiptId, got {other:?}"),
        }
    }

    #[test]
    fn segment_id_parse_rejects_wrong_prefix() {
        let err = SegmentId::parse("foo_0123456789abcdef0123456789abcdef")
            .expect_err("wrong prefix must fail");
        match err {
            EvidenceError::InvalidReceiptId { detail } => {
                assert!(detail.contains("prefix"));
            }
            other => panic!("expected InvalidReceiptId, got {other:?}"),
        }
    }

    #[test]
    fn segment_id_parse_rejects_short_body() {
        let too_short = "seg_0123456789abcdef";
        let err = SegmentId::parse(too_short).expect_err("short body must fail");
        match err {
            EvidenceError::InvalidReceiptId { detail } => {
                assert!(detail.contains("32-char"));
            }
            other => panic!("expected InvalidReceiptId, got {other:?}"),
        }
    }

    #[test]
    fn segment_id_parse_rejects_uppercase_hex() {
        let upper = "seg_0123456789ABCDEF0123456789abcdef";
        let err = SegmentId::parse(upper).expect_err("uppercase must fail");
        match err {
            EvidenceError::InvalidReceiptId { detail } => {
                assert!(detail.contains("lowercase"));
            }
            other => panic!("expected InvalidReceiptId, got {other:?}"),
        }
    }

    #[test]
    fn segment_id_serde_round_trip() {
        let id = SegmentId::from_content(b"serde-test");
        let s = serde_json::to_string(&id).expect("ser");
        let back: SegmentId = serde_json::from_str(&s).expect("de");
        assert_eq!(back, id);
    }

    // ─── Segment open-state semantics ─────────────────────────────────

    #[test]
    fn segment_new_is_empty_open_and_carries_retention_class() {
        let seg = Segment::new(RetentionClass::Forever);
        assert!(seg.is_empty());
        assert_eq!(seg.len(), 0);
        assert!(!seg.is_sealed());
        assert_eq!(seg.retention_class(), RetentionClass::Forever);
        assert!(seg.receipts().is_empty());
    }

    #[test]
    fn segment_append_grows_and_preserves_order() {
        let (sk, _vk) = test_keypair();
        let seg = build_open_segment(5, &sk);
        assert_eq!(seg.len(), 5);
        assert!(!seg.is_sealed());

        // Payload order check.
        for (i, r) in seg.receipts().iter().enumerate() {
            assert_eq!(r.payload(), &json!({"step": i}));
        }
    }

    // ─── Segment seal happy path ──────────────────────────────────────

    #[test]
    fn segment_seal_produces_sealed_segment_with_correct_receipt_count() {
        let (sk, vk) = test_keypair();
        let seg = build_open_segment(5, &sk);

        let sealed = seg.seal(None, None, &sk).expect("seal");

        // 5 originals + 1 terminal SEGMENT_SEALED = 6.
        assert_eq!(sealed.receipt_count(), 6);
        assert_eq!(sealed.receipts().len(), 6);

        // Last receipt is the terminal witness.
        let terminal = sealed.receipts().last().expect("last");
        assert_eq!(terminal.record_type(), RecordType::SegmentSealed);
        assert_eq!(terminal.retention_class(), RetentionClass::Forever);

        // Segment id is well-formed.
        assert!(sealed.segment_id().as_str().starts_with("seg_"));
        assert_eq!(sealed.segment_id().as_str().len(), "seg_".len() + 32);

        // Seal hash is 64 hex chars (BLAKE3-256).
        assert_eq!(sealed.segment_seal_hash().len(), 64);

        // Signature is 128 lowercase hex chars (Ed25519).
        assert_eq!(sealed.segment_signature().len(), 128);
        assert!(sealed
            .segment_signature()
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));

        // Genesis segment carries no previous link.
        assert!(sealed.previous_segment_id().is_none());
        assert!(sealed.previous_segment_seal_hash().is_none());

        // Retention class is carried through.
        assert_eq!(sealed.retention_class(), RetentionClass::Standard24M);

        // Verify seal against the right key.
        sealed.verify_seal(&vk).expect("verify_seal happy path");
    }

    #[test]
    fn segment_seal_verify_full_passes_for_signed_segment() {
        let (sk, vk) = test_keypair();
        let seg = build_open_segment(3, &sk);
        let sealed = seg.seal(None, None, &sk).expect("seal");
        sealed.verify_full(&vk).expect("verify_full happy path");
    }

    #[test]
    fn segment_seal_with_wrong_key_fails_signature_verification() {
        let (sk_a, _vk_a) = test_keypair();
        let (_sk_b, vk_b) = test_keypair_other();
        let seg = build_open_segment(3, &sk_a);
        let sealed = seg.seal(None, None, &sk_a).expect("seal");
        match sealed.verify_seal(&vk_b) {
            Err(EvidenceError::SegmentSignatureMismatch) => {}
            other => panic!("expected SegmentSignatureMismatch, got {other:?}"),
        }
    }

    #[test]
    fn segment_seal_on_empty_segment_fails() {
        let (sk, _vk) = test_keypair();
        let seg = Segment::new(RetentionClass::Standard24M);
        match seg.seal(None, None, &sk) {
            Err(EvidenceError::EmptySegment) => {}
            other => panic!("expected EmptySegment, got {other:?}"),
        }
    }

    #[test]
    fn segment_append_after_seal_is_impossible_due_to_consuming_seal() {
        // `seal` consumes self; the only way to exercise SegmentAlreadySealed
        // is to flip the flag manually via a test-local mutation path. We do
        // that by simulating the post-seal state.
        let (sk, _vk) = test_keypair();
        let mut seg = build_open_segment(1, &sk);
        // Synthetic flip — emulates a future internal mutation path that
        // would set `is_sealed = true` without consuming the value.
        seg.is_sealed = true;

        let r = ReceiptBuilder::new(
            RecordType::PolicyDecision,
            RetentionClass::Standard24M,
            "service:policy-kernel",
        )
        .seal_signed(None, &sk)
        .expect("r");

        match seg.append(r) {
            Err(EvidenceError::SegmentAlreadySealed) => {}
            other => panic!("expected SegmentAlreadySealed, got {other:?}"),
        }
    }

    // ─── Tamper detection ─────────────────────────────────────────────

    #[test]
    fn segment_seal_detects_tampered_receipt_payload() {
        let (sk, vk) = test_keypair();
        let seg = build_open_segment(3, &sk);
        let sealed = seg.seal(None, None, &sk).expect("seal");

        // Serialize, tamper with a non-terminal receipt's payload, deserialize.
        let mut v = serde_json::to_value(&sealed).expect("ser");
        v["receipts"][1]["payload"] = json!({"step": 999});
        let tampered: SealedSegment = serde_json::from_value(v).expect("de");

        // Either the recomputed seal hash differs (caught first) or the
        // per-receipt signature fails — both are acceptable tamper signals.
        match tampered.verify_full(&vk) {
            Err(
                EvidenceError::SegmentSealMismatch { .. }
                | EvidenceError::SignatureMismatch
                | EvidenceError::SegmentSignatureMismatch,
            ) => {}
            other => panic!("expected tamper detection, got {other:?}"),
        }
    }

    // ─── Cross-segment linkage on seal() ──────────────────────────────

    #[test]
    fn segment_seal_with_previous_link_records_chain() {
        let (sk, vk) = test_keypair();

        let seg1 = build_open_segment(2, &sk);
        let sealed1 = seg1.seal(None, None, &sk).expect("seg1");

        let seg2 = build_open_segment(2, &sk);
        let sealed2 = seg2
            .seal(
                Some(sealed1.segment_id()),
                Some(sealed1.segment_seal_hash()),
                &sk,
            )
            .expect("seg2");

        assert_eq!(sealed2.previous_segment_id(), Some(sealed1.segment_id()));
        assert_eq!(
            sealed2.previous_segment_seal_hash(),
            Some(sealed1.segment_seal_hash())
        );
        sealed2.verify_full(&vk).expect("seg2 verify_full");
    }

    #[test]
    fn segment_seal_with_partial_previous_link_fails() {
        let (sk, _vk) = test_keypair();
        let seg = build_open_segment(1, &sk);
        let dummy_id = SegmentId::from_content(b"dummy");
        match seg.seal(Some(&dummy_id), None, &sk) {
            Err(EvidenceError::SegmentChainBroken { .. }) => {}
            other => panic!("expected SegmentChainBroken on partial link, got {other:?}"),
        }
    }

    // ─── Serde round-trip ─────────────────────────────────────────────

    #[test]
    fn sealed_segment_serde_round_trip_preserves_verification() {
        let (sk, vk) = test_keypair();
        let seg = build_open_segment(4, &sk);
        let sealed = seg.seal(None, None, &sk).expect("seal");

        let s = serde_json::to_string(&sealed).expect("ser");
        let back: SealedSegment = serde_json::from_str(&s).expect("de");

        assert_eq!(back, sealed);
        back.verify_full(&vk)
            .expect("round-tripped sealed segment must verify");
    }
}
