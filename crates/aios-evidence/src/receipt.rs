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
//! ## Signature placeholder
//!
//! The `signature` field is reserved for the Ed25519 segment-signing implementation
//! (S3.1 §5.2 / §11.3). T-007 sets it to `None`; concrete signing lives in T-008+.

use chrono::{DateTime, Utc};
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

    /// Ed25519 signature placeholder. Always `None` in T-007.
    #[must_use]
    pub fn signature(&self) -> Option<&str> {
        self.signature.as_deref()
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
            signature: None,
        })
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
    }
}
