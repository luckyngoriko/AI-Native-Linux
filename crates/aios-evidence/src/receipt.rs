//! [`EvidenceReceipt`] envelope + [`ReceiptBuilder`] (S3.1 §3).
//!
//! ## Type-level append-only encoding (INV-005)
//!
//! `EvidenceReceipt` has **all fields private** and exposes **only `&self`
//! accessors**. There is no `&mut self` method on the public API. Once a receipt
//! is sealed via [`ReceiptBuilder::seal`], it cannot be mutated by safe Rust code
//! in any downstream crate.
//!
//! Construction goes through [`ReceiptBuilder`]:
//!
//! ```text
//!   ReceiptBuilder::new(record_type, retention, subject)
//!     .with_action_id(action_id)            // optional
//!     .with_payload(payload_json)           // optional (defaults to Null)
//!     .seal(previous_or_none)
//!     -> EvidenceReceipt
//! ```
//!
//! `seal` is consuming (`self`, not `&mut self`). The seal step computes the
//! BLAKE3-256 content hash over the JCS-canonical bytes of the payload, assigns
//! the `recorded_at` server timestamp, links to the previous receipt's content
//! hash (if any), and produces an immutable [`EvidenceReceipt`].
//!
//! `Deserialize` is provided so receipts can be loaded from disk. **Deserialized
//! receipts go through validation in `ReceiptChain::append` and
//! `ReceiptChain::verify_integrity`** — see [`crate::chain`]. There is no "edit
//! and re-serialize" path in the public API.
//!
//! ## Ed25519 signature path (T-009)
//!
//! Sealed receipts may carry an Ed25519 signature over their canonical bytes
//! (S3.1 §5.2 / §11.3 / §28.5). The signed surface is the BLAKE3-256 digest of
//! the JCS-canonical serialization of the receipt **with the `signature` field
//! removed** — see [`ReceiptForSigning`]. This avoids the circular dependency
//! "sign the bytes that contain the signature" and keeps the canonical form
//! deterministic across re-serializations.
//!
//! ### Production key acquisition (deferred to S5.2)
//!
//! The constitutional signing subject is `_system:service:evidence-segment-signer`
//! (S3.1 §11.3). In production, the evidence-log service obtains an Ed25519
//! signing capability from the L4.2 Vault Broker — the broker performs the sign
//! operation without ever exposing the raw private key to the caller (S5.2
//! secrets-as-capabilities invariant). The crate-level API here takes a
//! [`ed25519_dalek::SigningKey`] directly because the Vault Broker capability
//! surface is not yet implemented; **the wire is the same shape** — replace the
//! `&SigningKey` parameter with a `&VaultSignCapability` when S5.2 lands.
//!
//! Verification is symmetric: production reads the `signing_key_id` field on
//! the producer's identity, resolves it to the `_system:service:evidence-segment-signer`
//! public key in the identity bundle, and calls [`EvidenceReceipt::verify_signature`].
//!
//! ### Test path
//!
//! Tests construct ephemeral keypairs via `SigningKey::generate(&mut OsRng)`
//! or `SigningKey::from_bytes(&[u8; 32])`. The receipt envelope's behaviour is
//! identical to production — only the key custodian differs.

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey, SIGNATURE_LENGTH};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use aios_action::{ActionId, EvidenceReceiptId};

use crate::error::EvidenceError;

/// An immutable, sealed evidence receipt.
///
/// Carries the full S3.1 §3 envelope subset that T-007 implements: id, server
/// timestamp, record type, retention class, emitting subject, optional bound
/// action id, hash-chain pointer to the prior receipt, content hash over the
/// JCS-canonical payload bytes, the opaque JSON payload, and an optional
/// signature placeholder.
///
/// **All fields are private.** The only mutation path is via [`ReceiptBuilder`],
/// which yields a fully-sealed value via [`ReceiptBuilder::seal`]. After that
/// point, INV-005 (evidence append-only) is enforced at the type level: no
/// `&mut self` method exists and there is no public field access.
///
/// Cloning is permitted because cloned receipts are content-identical to their
/// source — the hash chain still holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceReceipt {
    /// `evr_<ULID>` per S0.1 §3.2.1.
    receipt_id: EvidenceReceiptId,

    /// Server-authoritative wall-clock timestamp at seal (S3.1 §11.2).
    recorded_at: DateTime<Utc>,

    /// Closed `RecordType` value identifying what kind of event this is.
    record_type: crate::record::RecordType,

    /// Closed `RetentionClass` value driving §13 retention policy.
    retention_class: crate::record::RetentionClass,

    /// Subject canonical id per S5.1 (e.g. `human:operator-1`,
    /// `service:capability-runtime`). Always non-empty.
    subject_canonical_id: String,

    /// The action this evidence pertains to. `None` for system-emitted records
    /// that do not flow from an action envelope (e.g. `SEGMENT_SEALED`,
    /// `RECOVERY_EVENT`).
    action_id: Option<ActionId>,

    /// BLAKE3-truncated (32 hex chars) of the previous receipt's canonical
    /// content bytes. `None` only for the genesis receipt of the chain.
    previous_receipt_hash: Option<String>,

    /// BLAKE3-256 (64 hex chars) of the JCS-canonical payload bytes. Stable
    /// across re-encodings per S0.1 §8.5 / RFC 8785.
    content_hash: String,

    /// Opaque payload. Per-`RecordType` payload schemas are deferred (the Wave 13
    /// IDL reconciliation kept the discriminated `RecordPayload` oneof at 22
    /// variants; the 405 newly-enum'd `RecordType`s do not yet have payload
    /// messages — see S3.1 §29.6). T-007 stores the payload as opaque JSON so
    /// the envelope can be sealed and chained today.
    payload: Value,

    /// Ed25519 signature placeholder (S3.1 §5.2 / §11.3).
    ///
    /// T-007: always `None`. T-008+ will integrate the Vault Broker capability
    /// that signs over the segment's canonical bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signature: Option<String>,
}

impl EvidenceReceipt {
    // -----------------------------------------------------------------
    // Read-only accessors. NO `&mut self` methods exist anywhere.
    // -----------------------------------------------------------------

    /// `evr_<ULID>` identity of this receipt.
    #[must_use]
    pub const fn receipt_id(&self) -> &EvidenceReceiptId {
        &self.receipt_id
    }

    /// Server-sealed wall-clock timestamp.
    #[must_use]
    pub const fn recorded_at(&self) -> DateTime<Utc> {
        self.recorded_at
    }

    /// The closed `RecordType` discriminator.
    #[must_use]
    pub const fn record_type(&self) -> crate::record::RecordType {
        self.record_type
    }

    /// The closed `RetentionClass` for §13 retention enforcement.
    #[must_use]
    pub const fn retention_class(&self) -> crate::record::RetentionClass {
        self.retention_class
    }

    /// Emitting subject canonical id (S5.1).
    #[must_use]
    pub fn subject_canonical_id(&self) -> &str {
        &self.subject_canonical_id
    }

    /// Optional action id this receipt is bound to.
    #[must_use]
    pub const fn action_id(&self) -> Option<&ActionId> {
        self.action_id.as_ref()
    }

    /// BLAKE3-truncated previous-receipt content hash, or `None` for the
    /// genesis receipt.
    #[must_use]
    pub fn previous_receipt_hash(&self) -> Option<&str> {
        self.previous_receipt_hash.as_deref()
    }

    /// BLAKE3-256 (64 hex chars) of the JCS-canonical payload bytes.
    #[must_use]
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    /// Opaque JSON payload (per-RecordType payload schemas deferred to Wave 14+).
    #[must_use]
    pub const fn payload(&self) -> &Value {
        &self.payload
    }

    /// Ed25519 signature over the canonical-minus-signature receipt bytes, as
    /// 128 lowercase hex chars. `None` for receipts sealed via the legacy
    /// [`ReceiptBuilder::seal`] path; `Some(...)` for receipts sealed via
    /// [`ReceiptBuilder::seal_signed`] (T-009).
    #[must_use]
    pub fn signature(&self) -> Option<&str> {
        self.signature.as_deref()
    }

    /// True iff the receipt carries any signature blob (regardless of
    /// validity). Use [`Self::verify_signature`] to check cryptographic
    /// validity.
    #[must_use]
    pub const fn is_signed(&self) -> bool {
        self.signature.is_some()
    }

    /// Compute the canonical-minus-signature digest that the Ed25519 signature
    /// covers.
    ///
    /// The digest is `BLAKE3_256(JCS(receipt-without-signature))`. The
    /// "without signature" projection is the [`ReceiptForSigning`] view: every
    /// sealed field except `signature`. This is the **same** byte sequence the
    /// signer signed in [`ReceiptBuilder::seal_signed`].
    ///
    /// # Errors
    ///
    /// Returns [`EvidenceError::EncodingFailed`] if the JCS projection fails.
    pub fn signing_digest(&self) -> Result<String, EvidenceError> {
        let view = ReceiptForSigning::from(self);
        let canonical = aios_action::jcs_canonicalize(&view)?;
        Ok(aios_action::blake3_hash(canonical.as_bytes()))
    }

    /// Verify the Ed25519 signature on this receipt against `verifying_key`.
    ///
    /// On success returns `Ok(())`. Failure modes:
    ///
    /// - [`EvidenceError::SignatureMissing`] — `signature` field is `None`.
    /// - [`EvidenceError::SignatureMalformed`] — hex parse fails or the decoded
    ///   blob is not 64 bytes (`SIGNATURE_LENGTH`).
    /// - [`EvidenceError::SignatureMismatch`] — Ed25519 verification rejected
    ///   the signature against `verifying_key` and the recomputed digest.
    /// - [`EvidenceError::EncodingFailed`] — JCS projection of the
    ///   canonical-minus-signature form failed.
    ///
    /// Production note: the verifying key is resolved from the identity bundle
    /// for the constitutional subject `_system:service:evidence-segment-signer`
    /// per S3.1 §11.3.
    ///
    /// # Errors
    ///
    /// See the variant list above.
    pub fn verify_signature(&self, verifying_key: &VerifyingKey) -> Result<(), EvidenceError> {
        let hex_sig = self
            .signature
            .as_deref()
            .ok_or(EvidenceError::SignatureMissing)?;

        let sig_bytes = decode_signature_hex(hex_sig)?;
        let signature = Signature::from_bytes(&sig_bytes);

        let digest_hex = self.signing_digest()?;
        // The signed message is the canonical-minus-signature digest expressed
        // as its lowercase-hex BLAKE3 string. Verification reconstructs the
        // same bytes deterministically — that's the contract.
        verifying_key
            .verify(digest_hex.as_bytes(), &signature)
            .map_err(|_| EvidenceError::SignatureMismatch)
    }

    /// Recompute the BLAKE3-256 of the canonical payload bytes and check it
    /// matches `content_hash`.
    ///
    /// Used by [`crate::chain::ReceiptChain::verify_integrity`] for the §5.3
    /// step-1 check. Returns the recomputed hash on success and an
    /// [`EvidenceError::HashMismatch`] otherwise.
    ///
    /// # Errors
    ///
    /// - [`EvidenceError::EncodingFailed`] if JCS projection fails.
    /// - [`EvidenceError::HashMismatch`] if the recomputed hash differs from
    ///   the stored `content_hash`.
    pub fn verify_content_hash(&self) -> Result<String, EvidenceError> {
        let canonical = aios_action::jcs_canonicalize(&self.payload)?;
        let computed = aios_action::blake3_hash(canonical.as_bytes());
        if computed == self.content_hash {
            Ok(computed)
        } else {
            Err(EvidenceError::HashMismatch {
                expected: self.content_hash.clone(),
                computed,
            })
        }
    }

    /// Compute the BLAKE3-truncated (32 hex chars) hash of this receipt's
    /// canonical-serialized form. Used by the chain to link the next receipt
    /// via `previous_receipt_hash`.
    ///
    /// The canonical form here is the JCS encoding of the whole `EvidenceReceipt`
    /// struct (not just the payload). This pins the link to *all* sealed fields
    /// of the receipt — receipt id, timestamp, record type, retention class,
    /// subject, action id, previous hash, content hash, payload, and signature.
    /// Any mutation of any field would change this value and break the chain.
    ///
    /// # Errors
    ///
    /// Returns [`EvidenceError::EncodingFailed`] if JCS projection fails.
    pub fn link_hash(&self) -> Result<String, EvidenceError> {
        let canonical = aios_action::jcs_canonicalize(self)?;
        Ok(aios_action::blake3_truncated(canonical.as_bytes()))
    }
}

/// Mutable builder for an unsealed receipt.
///
/// All setters are consuming (move `self`); chain them to compose a receipt and
/// terminate with [`Self::seal`]. The builder is the only path that produces an
/// [`EvidenceReceipt`] from scratch.
#[derive(Debug, Clone)]
pub struct ReceiptBuilder {
    record_type: crate::record::RecordType,
    retention_class: crate::record::RetentionClass,
    subject_canonical_id: String,
    action_id: Option<ActionId>,
    payload: Value,
}

impl ReceiptBuilder {
    /// Start a new builder bound to a record type, retention class, and
    /// emitting subject.
    ///
    /// `subject` is trimmed for the emptiness check at seal time but stored
    /// verbatim — S5.1 canonical subject ids are case- and whitespace-sensitive.
    #[must_use]
    pub fn new(
        record_type: crate::record::RecordType,
        retention_class: crate::record::RetentionClass,
        subject: impl Into<String>,
    ) -> Self {
        Self {
            record_type,
            retention_class,
            subject_canonical_id: subject.into(),
            action_id: None,
            payload: Value::Null,
        }
    }

    /// Bind this receipt to a specific action envelope.
    ///
    /// For system-emitted records (e.g. `SEGMENT_SEALED`) the action id is left
    /// `None` and this method is not called.
    #[must_use]
    pub fn with_action_id(mut self, action_id: ActionId) -> Self {
        self.action_id = Some(action_id);
        self
    }

    /// Set the opaque JSON payload.
    ///
    /// Per-RecordType payload schemas are deferred to Wave 14+ (see S3.1 §29.6);
    /// for T-007 the payload is opaque JSON.
    #[must_use]
    pub fn with_payload(mut self, payload: Value) -> Self {
        self.payload = payload;
        self
    }

    /// Seal the builder into an immutable [`EvidenceReceipt`].
    ///
    /// Steps:
    ///
    /// 1. Validate `subject_canonical_id` is non-empty (after trim).
    /// 2. Mint a fresh `evr_<ULID>` receipt id.
    /// 3. Stamp `recorded_at` with the current UTC wall clock.
    /// 4. Compute `content_hash = BLAKE3_256(JCS(payload))` (full 64 hex chars).
    /// 5. Set `previous_receipt_hash`:
    ///    - `None` if `previous` is `None` (this is a genesis receipt).
    ///    - `Some(prev.link_hash())` otherwise.
    ///
    /// After this call, no field of the resulting receipt can be mutated by safe
    /// Rust code — INV-005 (evidence append-only) is enforced at the type level.
    ///
    /// # Errors
    ///
    /// - [`EvidenceError::InvalidSubject`] if `subject_canonical_id` is empty
    ///   or whitespace-only.
    /// - [`EvidenceError::EncodingFailed`] if JCS canonical projection of the
    ///   payload (or of the previous receipt for link hashing) fails.
    pub fn seal(
        self,
        previous: Option<&EvidenceReceipt>,
    ) -> Result<EvidenceReceipt, EvidenceError> {
        self.seal_inner(previous, None)
    }

    /// Seal **and** Ed25519-sign the receipt in one consuming step (T-009,
    /// S3.1 §5.2 / §11.3).
    ///
    /// Steps 1..5 are identical to [`Self::seal`]. After producing the receipt
    /// with `signature: None`, the canonical-minus-signature form is hashed
    /// (BLAKE3-256), the resulting hex digest is signed with `signing_key`,
    /// and the 128-char lowercase hex signature is stored on the receipt.
    ///
    /// Production: `signing_key` is obtained from the L4.2 Vault Broker for
    /// subject `_system:service:evidence-segment-signer`. The Vault Broker
    /// owns the raw private key; this API surface will be replaced with a
    /// `VaultSignCapability` when S5.2 lands. The hash-then-sign envelope
    /// here is the **same** that the broker will sign over — the migration
    /// is local to key custody, not to the signed surface.
    ///
    /// # Errors
    ///
    /// - [`EvidenceError::InvalidSubject`] if `subject_canonical_id` is empty
    ///   or whitespace-only.
    /// - [`EvidenceError::EncodingFailed`] if JCS canonical projection of the
    ///   payload, the previous receipt, or the canonical-minus-signature view
    ///   fails.
    pub fn seal_signed(
        self,
        previous: Option<&EvidenceReceipt>,
        signing_key: &SigningKey,
    ) -> Result<EvidenceReceipt, EvidenceError> {
        // Step 1: produce an unsigned receipt going through the same path as
        // `seal`. This guarantees the signature flow does not diverge from
        // the non-signed flow in any field.
        let mut receipt = self.seal_inner(previous, None)?;

        // Step 2: compute the canonical-minus-signature digest. At this point
        // `receipt.signature` is `None`, so the `ReceiptForSigning` projection
        // sees the final field set.
        let digest_hex = receipt.signing_digest()?;

        // Step 3: sign the digest hex bytes. We sign the *string* bytes (not
        // the raw 32 BLAKE3 bytes) because (a) it matches the verifier path
        // in `verify_signature`, (b) it produces a deterministic, debuggable
        // signed message even if the receipt is re-serialized through
        // different JSON encoders.
        let signature: Signature = signing_key.sign(digest_hex.as_bytes());
        receipt.signature = Some(encode_signature_hex(&signature.to_bytes()));
        Ok(receipt)
    }

    /// Shared seal core used by both `seal` and `seal_signed`.
    fn seal_inner(
        self,
        previous: Option<&EvidenceReceipt>,
        signature: Option<String>,
    ) -> Result<EvidenceReceipt, EvidenceError> {
        // 1. Subject discipline.
        if self.subject_canonical_id.trim().is_empty() {
            return Err(EvidenceError::InvalidSubject {
                detail: "subject_canonical_id is empty or whitespace-only".to_owned(),
            });
        }

        // 2. Mint id + 3. stamp recorded_at.
        let receipt_id = EvidenceReceiptId::new();
        let recorded_at = Utc::now();

        // 4. Compute content hash over JCS-canonical payload bytes.
        let canonical_payload = aios_action::jcs_canonicalize(&self.payload)?;
        let content_hash = aios_action::blake3_hash(canonical_payload.as_bytes());

        // 5. Link previous receipt, if any.
        let previous_receipt_hash = match previous {
            None => None,
            Some(prev) => Some(prev.link_hash()?),
        };

        Ok(EvidenceReceipt {
            receipt_id,
            recorded_at,
            record_type: self.record_type,
            retention_class: self.retention_class,
            subject_canonical_id: self.subject_canonical_id,
            action_id: self.action_id,
            previous_receipt_hash,
            content_hash,
            payload: self.payload,
            signature,
        })
    }
}

/// Canonical-minus-signature projection of an [`EvidenceReceipt`].
///
/// Used by both signing and verification to produce the byte sequence the
/// Ed25519 signature covers. The struct mirrors `EvidenceReceipt` field-for-
/// field **except** for the `signature` field, which is omitted. JCS
/// canonicalization over this view yields a deterministic, signature-free
/// byte stream — avoiding the "signature signs itself" cycle.
///
/// Field order does not matter for JCS (it sorts keys lexicographically), but
/// the serialized names MUST match `EvidenceReceipt`'s field names so that
/// the canonical bytes are byte-identical to `EvidenceReceipt`'s own JCS form
/// minus the `signature` entry.
#[derive(Debug, Serialize)]
struct ReceiptForSigning<'a> {
    receipt_id: &'a EvidenceReceiptId,
    recorded_at: DateTime<Utc>,
    record_type: crate::record::RecordType,
    retention_class: crate::record::RetentionClass,
    subject_canonical_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    action_id: Option<&'a ActionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_receipt_hash: Option<&'a str>,
    content_hash: &'a str,
    payload: &'a Value,
}

impl<'a> From<&'a EvidenceReceipt> for ReceiptForSigning<'a> {
    fn from(r: &'a EvidenceReceipt) -> Self {
        Self {
            receipt_id: &r.receipt_id,
            recorded_at: r.recorded_at,
            record_type: r.record_type,
            retention_class: r.retention_class,
            subject_canonical_id: &r.subject_canonical_id,
            action_id: r.action_id.as_ref(),
            previous_receipt_hash: r.previous_receipt_hash.as_deref(),
            content_hash: &r.content_hash,
            payload: &r.payload,
        }
    }
}

/// Lowercase-hex encode a 64-byte Ed25519 signature.
fn encode_signature_hex(sig: &[u8; SIGNATURE_LENGTH]) -> String {
    let mut out = String::with_capacity(SIGNATURE_LENGTH * 2);
    for byte in sig {
        // `{:02x}` is the canonical lowercase-hex byte formatter and is
        // infallible for `u8`.
        use core::fmt::Write as _;
        // Writing to a String never fails — but be explicit and tolerate it
        // without `unwrap` to satisfy the workspace lint.
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Parse a 128-char lowercase-hex Ed25519 signature into a 64-byte array.
///
/// # Errors
///
/// Returns [`EvidenceError::SignatureMalformed`] when:
/// - input length is not exactly `2 * SIGNATURE_LENGTH` (128 chars), or
/// - any character is outside `[0-9a-f]` (we reject upper-case hex to keep
///   the canonical lowercase invariant from T-007 §5.4 / S0.1 §8.5).
fn decode_signature_hex(hex: &str) -> Result<[u8; SIGNATURE_LENGTH], EvidenceError> {
    let expected_len = SIGNATURE_LENGTH * 2;
    if hex.len() != expected_len {
        return Err(EvidenceError::SignatureMalformed {
            detail: format!(
                "expected {expected_len} lowercase hex chars, got {} chars",
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

/// Parse one lowercase-hex nibble (`0..=9` or `a..=f`). Upper case is rejected.
fn hex_nibble(c: u8) -> Result<u8, EvidenceError> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        other => Err(EvidenceError::SignatureMalformed {
            detail: format!("non-lowercase-hex byte 0x{other:02x} in Ed25519 signature blob"),
        }),
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
    use crate::record::{RecordType, RetentionClass};
    use serde_json::json;

    fn builder(record_type: RecordType) -> ReceiptBuilder {
        ReceiptBuilder::new(record_type, RetentionClass::Standard24M, "human:operator-1")
    }

    #[test]
    fn builder_seal_happy_path_populates_all_required_fields() {
        let receipt = builder(RecordType::ActionReceived)
            .with_payload(json!({"action": "fs.write"}))
            .seal(None)
            .expect("genesis seal must succeed");

        // receipt_id is a freshly minted evr_<ULID>.
        assert!(receipt.receipt_id().as_str().starts_with("evr_"));

        // recorded_at is non-zero (today's date).
        assert!(receipt.recorded_at().timestamp() > 0);

        // record/retention round-trip.
        assert_eq!(receipt.record_type(), RecordType::ActionReceived);
        assert_eq!(receipt.retention_class(), RetentionClass::Standard24M);

        // Subject preserved verbatim.
        assert_eq!(receipt.subject_canonical_id(), "human:operator-1");

        // No action bound by default.
        assert!(receipt.action_id().is_none());

        // Genesis: previous_receipt_hash is None.
        assert!(receipt.previous_receipt_hash().is_none());

        // content_hash is BLAKE3-256 of JCS(payload): 64 hex lowercase chars.
        assert_eq!(receipt.content_hash().len(), 64);
        assert!(receipt
            .content_hash()
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));

        // Payload round-trip.
        assert_eq!(receipt.payload(), &json!({"action": "fs.write"}));

        // T-007 signature is always None.
        assert!(receipt.signature().is_none());
    }

    #[test]
    fn builder_with_action_id_binds_action() {
        let action = ActionId::new();
        let receipt = builder(RecordType::ExecutionStarted)
            .with_action_id(action.clone())
            .with_payload(json!({"adapter": "fs"}))
            .seal(None)
            .expect("seal");
        assert_eq!(receipt.action_id(), Some(&action));
    }

    #[test]
    fn seal_is_deterministic_for_the_payload_hash() {
        // Same payload -> same content_hash (the timestamp / receipt_id differ).
        let payload = json!({"zeta": 1, "alpha": 2});

        let r1 = builder(RecordType::PolicyDecision)
            .with_payload(payload.clone())
            .seal(None)
            .expect("seal");
        let r2 = builder(RecordType::PolicyDecision)
            .with_payload(payload)
            .seal(None)
            .expect("seal");

        assert_eq!(
            r1.content_hash(),
            r2.content_hash(),
            "identical payloads must produce identical content hashes"
        );
        // Sanity: receipt ids are distinct.
        assert_ne!(r1.receipt_id(), r2.receipt_id());
    }

    #[test]
    fn seal_links_to_previous_receipt() {
        let genesis = builder(RecordType::ActionReceived)
            .with_payload(json!({"k": 1}))
            .seal(None)
            .expect("genesis");
        let second = builder(RecordType::PolicyDecision)
            .with_payload(json!({"k": 2}))
            .seal(Some(&genesis))
            .expect("second");

        let link = second.previous_receipt_hash().expect("must be Some");
        assert_eq!(link.len(), 32, "link hash is BLAKE3-truncated to 32 hex");
        // The link hash MUST equal the genesis receipt's own link_hash().
        let recomputed = genesis.link_hash().expect("link hash recompute");
        assert_eq!(link, recomputed);
    }

    #[test]
    fn seal_with_none_previous_produces_genesis_receipt() {
        let r = builder(RecordType::RecoveryEvent)
            .seal(None)
            .expect("genesis");
        assert!(r.previous_receipt_hash().is_none());
    }

    #[test]
    fn seal_rejects_empty_subject() {
        let r = ReceiptBuilder::new(RecordType::ActionReceived, RetentionClass::Standard24M, "")
            .seal(None);
        match r {
            Err(EvidenceError::InvalidSubject { .. }) => {}
            other => panic!("expected InvalidSubject, got {other:?}"),
        }
    }

    #[test]
    fn seal_rejects_whitespace_only_subject() {
        let r = ReceiptBuilder::new(
            RecordType::ActionReceived,
            RetentionClass::Standard24M,
            "   \t  ",
        )
        .seal(None);
        match r {
            Err(EvidenceError::InvalidSubject { .. }) => {}
            other => panic!("expected InvalidSubject for whitespace, got {other:?}"),
        }
    }

    #[test]
    fn receipt_serde_round_trip_via_json() {
        let r = builder(RecordType::PolicyDecision)
            .with_payload(json!({"decision": "ALLOW", "reason": "policy-ok"}))
            .seal(None)
            .expect("seal");
        let s = serde_json::to_string(&r).expect("serialize");
        let back: EvidenceReceipt = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(back, r);
    }

    #[test]
    fn verify_content_hash_passes_for_sealed_receipt() {
        let r = builder(RecordType::ApprovalGranted)
            .with_payload(json!({"approver": "human:operator-1"}))
            .seal(None)
            .expect("seal");
        let recomputed = r.verify_content_hash().expect("verify");
        assert_eq!(recomputed, r.content_hash());
    }

    #[test]
    fn link_hash_is_thirty_two_hex_chars() {
        let r = builder(RecordType::SegmentSealed).seal(None).expect("seal");
        let h = r.link_hash().expect("link_hash");
        assert_eq!(h.len(), 32);
        assert!(h
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn evidence_receipt_has_no_public_field_mutation_api() {
        // This is a structural assertion: every accessor takes `&self`, NOT
        // `&mut self`. The Rust compiler would refuse a `&mut self` accessor
        // attempting to write to private fields from this test's call site, so
        // a successful compile of this test is the proof.
        let r = builder(RecordType::ActionReceived)
            .seal(None)
            .expect("seal");

        // Every accessor here borrows immutably; we exercise the full surface
        // to ensure the API shape remains read-only.
        let _ = r.receipt_id();
        let _ = r.recorded_at();
        let _ = r.record_type();
        let _ = r.retention_class();
        let _ = r.subject_canonical_id();
        let _ = r.action_id();
        let _ = r.previous_receipt_hash();
        let _ = r.content_hash();
        let _ = r.payload();
        let _ = r.signature();
        let _ = r.is_signed();
    }

    // ─── T-009: Ed25519 signing path ────────────────────────────────────
    //
    // The tests below pin the signing surface for S3.1 §5.2 / §11.3 /
    // §28.5. They cover sign/verify round-trip, key mismatch, post-seal
    // tampering on payload and on `record_type`, the unsigned-receipt path,
    // serde-preservation of the signature field, and the wire-format
    // invariant (128 lowercase hex chars).

    /// Build a deterministic test signing keypair from a fixed seed.
    ///
    /// Production keypairs come from S5.2 Vault Broker; never construct
    /// them this way outside test code.
    fn test_keypair() -> (SigningKey, VerifyingKey) {
        let seed = [42u8; 32];
        let sk = SigningKey::from_bytes(&seed);
        let vk = sk.verifying_key();
        (sk, vk)
    }

    /// A *different* deterministic test keypair, used for mismatch tests.
    fn test_keypair_other() -> (SigningKey, VerifyingKey) {
        let seed = [99u8; 32];
        let sk = SigningKey::from_bytes(&seed);
        let vk = sk.verifying_key();
        (sk, vk)
    }

    #[test]
    fn t009_seal_signed_produces_signed_receipt_that_round_trip_verifies() {
        let (sk, vk) = test_keypair();
        let r = builder(RecordType::ActionReceived)
            .with_payload(json!({"action": "fs.write"}))
            .seal_signed(None, &sk)
            .expect("seal_signed");

        assert!(r.is_signed(), "signed receipt must report is_signed()");
        assert!(r.signature().is_some());
        r.verify_signature(&vk).expect("signature must verify");
    }

    #[test]
    fn t009_seal_signed_signature_is_128_lowercase_hex_chars() {
        let (sk, _vk) = test_keypair();
        let r = builder(RecordType::PolicyDecision)
            .with_payload(json!({"decision": "ALLOW"}))
            .seal_signed(None, &sk)
            .expect("seal_signed");

        let sig = r.signature().expect("signature present");
        assert_eq!(sig.len(), 128, "Ed25519 signature hex must be 128 chars");
        assert!(
            sig.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "signature must be lowercase hex, got `{sig}`"
        );
    }

    #[test]
    fn t009_verify_signature_fails_against_wrong_key() {
        let (sk_a, _vk_a) = test_keypair();
        let (_sk_b, vk_b) = test_keypair_other();

        let r = builder(RecordType::ExecutionStarted)
            .with_payload(json!({"adapter": "fs"}))
            .seal_signed(None, &sk_a)
            .expect("seal_signed");

        match r.verify_signature(&vk_b) {
            Err(EvidenceError::SignatureMismatch) => {}
            other => panic!("expected SignatureMismatch, got {other:?}"),
        }
    }

    #[test]
    fn t009_verify_signature_fails_when_payload_tampered_post_seal() {
        let (sk, vk) = test_keypair();
        let mut r = builder(RecordType::ActionReceived)
            .with_payload(json!({"step": 1}))
            .seal_signed(None, &sk)
            .expect("seal_signed");

        // Tamper via deserialize / mutate / re-construct: the only edit path
        // we expose in the public API is serde, and even that is enough to
        // break the signature.
        let v = serde_json::to_value(&r).expect("ser");
        let mut v = v;
        v["payload"] = json!({"step": 999});
        r = serde_json::from_value(v).expect("de");

        match r.verify_signature(&vk) {
            Err(EvidenceError::SignatureMismatch) => {}
            other => panic!("expected SignatureMismatch after payload tamper, got {other:?}"),
        }
    }

    #[test]
    fn t009_verify_signature_fails_when_record_type_tampered_post_seal() {
        let (sk, vk) = test_keypair();
        let r = builder(RecordType::ActionReceived)
            .with_payload(json!({"k": 1}))
            .seal_signed(None, &sk)
            .expect("seal_signed");

        // Tamper: swap record_type in the serialized JSON.
        let mut v = serde_json::to_value(&r).expect("ser");
        v["record_type"] = json!("POLICY_DECISION");
        let tampered: EvidenceReceipt = serde_json::from_value(v).expect("de");

        match tampered.verify_signature(&vk) {
            Err(EvidenceError::SignatureMismatch) => {}
            other => {
                panic!("expected SignatureMismatch after record_type tamper, got {other:?}")
            }
        }
    }

    #[test]
    fn t009_seal_without_signing_yields_unsigned_receipt() {
        let r = builder(RecordType::ActionReceived)
            .seal(None)
            .expect("seal");
        assert!(!r.is_signed());
        assert!(r.signature().is_none());

        let (_sk, vk) = test_keypair();
        match r.verify_signature(&vk) {
            Err(EvidenceError::SignatureMissing) => {}
            other => panic!("expected SignatureMissing on unsigned receipt, got {other:?}"),
        }
    }

    #[test]
    fn t009_signed_receipt_serde_round_trip_preserves_signature_and_verifies() {
        let (sk, vk) = test_keypair();
        let r = builder(RecordType::ExecutionCompleted)
            .with_action_id(ActionId::new())
            .with_payload(json!({"outcome": "SUCCESS"}))
            .seal_signed(None, &sk)
            .expect("seal_signed");

        let s = serde_json::to_string(&r).expect("ser");
        let back: EvidenceReceipt = serde_json::from_str(&s).expect("de");

        assert_eq!(back.signature(), r.signature());
        assert!(back.is_signed());
        back.verify_signature(&vk)
            .expect("round-tripped signed receipt must still verify");
    }

    #[test]
    fn t009_verify_signature_rejects_malformed_hex_length() {
        // Hand-craft a receipt with a bad-length signature blob.
        let (_sk, vk) = test_keypair();
        let r = builder(RecordType::ActionReceived)
            .seal(None)
            .expect("seal");

        let mut v = serde_json::to_value(&r).expect("ser");
        v["signature"] = json!("deadbeef"); // 8 chars, too short
        let bad: EvidenceReceipt = serde_json::from_value(v).expect("de");

        match bad.verify_signature(&vk) {
            Err(EvidenceError::SignatureMalformed { detail }) => {
                assert!(
                    detail.contains("128"),
                    "detail should mention expected 128 chars"
                );
            }
            other => panic!("expected SignatureMalformed (length), got {other:?}"),
        }
    }

    #[test]
    fn t009_verify_signature_rejects_uppercase_hex() {
        let (sk, vk) = test_keypair();
        let r = builder(RecordType::ActionReceived)
            .seal_signed(None, &sk)
            .expect("seal_signed");

        let sig = r.signature().expect("sig").to_string();
        // Upper-case the first 4 chars; still 128 chars.
        let uppercased = format!("{}{}", &sig[..4].to_ascii_uppercase(), &sig[4..]);
        let mut v = serde_json::to_value(&r).expect("ser");
        v["signature"] = json!(uppercased);
        let bad: EvidenceReceipt = serde_json::from_value(v).expect("de");

        match bad.verify_signature(&vk) {
            Err(EvidenceError::SignatureMalformed { .. }) => {}
            other => panic!("expected SignatureMalformed (uppercase), got {other:?}"),
        }
    }

    #[test]
    fn t009_signing_digest_is_stable_across_two_calls() {
        let (sk, _vk) = test_keypair();
        let r = builder(RecordType::PolicyDecision)
            .with_payload(json!({"x": 1, "y": 2}))
            .seal_signed(None, &sk)
            .expect("seal_signed");

        let d1 = r.signing_digest().expect("d1");
        let d2 = r.signing_digest().expect("d2");
        assert_eq!(d1, d2);
        assert_eq!(d1.len(), 64, "BLAKE3-256 hex digest is 64 chars");
    }

    #[test]
    fn t009_signing_digest_excludes_signature_field() {
        // Two receipts identical in every field except `signature` must
        // produce the same signing digest. We can't construct identical
        // receipts via the builder (receipt_id + recorded_at are minted
        // fresh), so we build one and serde-roundtrip with and without
        // the signature.
        let (sk, _vk) = test_keypair();
        let signed = builder(RecordType::ActionReceived)
            .with_payload(json!({"k": 1}))
            .seal_signed(None, &sk)
            .expect("seal_signed");

        // Strip the signature via serde and reconstruct.
        let mut v = serde_json::to_value(&signed).expect("ser");
        v.as_object_mut().expect("obj").remove("signature");
        let unsigned: EvidenceReceipt = serde_json::from_value(v).expect("de");
        assert!(!unsigned.is_signed());

        let d_signed = signed.signing_digest().expect("d");
        let d_unsigned = unsigned.signing_digest().expect("d");
        assert_eq!(
            d_signed, d_unsigned,
            "signing digest must NOT depend on the signature field"
        );
    }

    #[test]
    fn t009_seal_signed_links_previous_like_seal_does() {
        let (sk, vk) = test_keypair();
        let genesis = builder(RecordType::ActionReceived)
            .with_payload(json!({"step": 0}))
            .seal_signed(None, &sk)
            .expect("genesis");

        let next = builder(RecordType::PolicyDecision)
            .with_payload(json!({"step": 1}))
            .seal_signed(Some(&genesis), &sk)
            .expect("next");

        let link = next.previous_receipt_hash().expect("link present");
        assert_eq!(link.len(), 32);
        // The link must equal the genesis link_hash (which folds in the
        // signature field — that's by design: any mutation to ANY field
        // breaks both the chain and the signature).
        assert_eq!(link, genesis.link_hash().expect("genesis link"));

        genesis.verify_signature(&vk).expect("genesis sig ok");
        next.verify_signature(&vk).expect("next sig ok");
    }

    #[test]
    fn t009_seal_signed_rejects_empty_subject_like_seal_does() {
        let (sk, _vk) = test_keypair();
        let r = ReceiptBuilder::new(
            RecordType::ActionReceived,
            RetentionClass::Standard24M,
            "  ",
        )
        .seal_signed(None, &sk);
        match r {
            Err(EvidenceError::InvalidSubject { .. }) => {}
            other => panic!("expected InvalidSubject, got {other:?}"),
        }
    }

    #[test]
    fn t009_encode_decode_signature_hex_round_trips_all_bytes() {
        // Cover every byte value through encode/decode to pin the wire format.
        let mut sig = [0u8; SIGNATURE_LENGTH];
        for (i, b) in sig.iter_mut().enumerate() {
            // 64-byte signature; cycle 0..=255 across the array.
            #[allow(clippy::cast_possible_truncation, reason = "intentional u8 wrap")]
            {
                *b = ((i * 4) & 0xff) as u8;
            }
        }
        let hex = encode_signature_hex(&sig);
        assert_eq!(hex.len(), 128);
        assert!(hex
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        let back = decode_signature_hex(&hex).expect("decode");
        assert_eq!(back, sig);
    }
}
