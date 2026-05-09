# Evidence Receipt Schema (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Phase tag      | S6.3                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Layer          | L0 Governance, Evidence, Safety                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Schema package | `aios.evidence.v1alpha1` (binds to S3.1's package; this contract owns the receipt envelope, S3.1 owns the log container around it)                                                                                                                                                                                                                                                                                                                                                                                                                               |
| Consumes       | **Imports vocabulary from**: S3.1 (`RecordType` closed enum + `RetentionClass` enum + segment model + hash chain algorithm — type-level shape co-defined with L9; L0 receipt envelope embeds these without requiring L9 evidence-log operational), S5.1 (Subject canonical id format — type-level string format owned by L4), S4.1 (`(scope, group_id, user_id)` triple — type-level scope shape owned by L2), S0.1 (BLAKE3 + JCS hash convention — cross-cutting). **Peer (intra-L0)**: S6.2 (`EvidenceGrade` enum), S6.4 (INV-005, INV-014, INV-015, INV-016). |
| Produces       | the canonical `EvidenceReceipt` envelope shape; four closed enums (`RedactionClass`, `ReceiptIntegrityState`, `LineageRelation`, `RedactionRule`); receipt-level integrity rules; four FOREVER-retention record types catching forgery / cycle / replay / redaction failure                                                                                                                                                                                                                                                                                      |
| Evidence       | E1 (artifact recorded; this spec file is the artifact)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |

## 1. Purpose

S3.1 (Evidence Log Architecture) defines the segment model, hash chain at segment level, retention classes, append API, and the closed `RecordType` vocabulary that now totals 205 narrative entries through Wave 7. S6.2 (Evidence Grades) defines what grades `E0..E5` mean and which receipts contribute to which grade. S6.4 names the constitutional invariants `INV-005` (evidence is append-only), `INV-014` (no proof, no completion), `INV-015` (evidence never contains secrets) that the receipt envelope must respect.

What is missing is the **canonical envelope shape every evidence receipt carries** — the fields that bind a payload to its emitter, scope, retention class, lineage, and integrity proof. Without this contract, evidence verification is per-`RecordType` ad hoc: each emitter chooses how to populate `subject`, `action_id`, `payload_hash`, signature; each verifier walks a slightly different shape; the audit pathway is not uniform across the 205 record types.

This spec fixes the canonical receipt shape. It states which fields every receipt carries, who is allowed to set each field, what is recomputed by the log on append, what is forbidden post-seal, and how an adversary's attempts to forge / replay / poison receipts are caught. It is the L0 mirror of S3.1 §3 — S3.1 says "here is a receipt envelope and here are the per-`RecordType` payloads"; this contract says "here is the constitutional discipline that makes every receipt audit-grade across the 205 record types".

## 2. Core invariants of this spec itself

- **I1 — One envelope, all record types.** Every record in the closed `RecordType` vocabulary (now 205) emits an `EvidenceReceipt` of the shape in §3. There is no second envelope, no "lightweight receipt", no "telemetry-only receipt". A producer that wants to emit something not envelope-shaped is emitting telemetry, not evidence.
- **I2 — Field authorship is split.** Some fields are emitter-set (subject id, record type, payload, action_id, parent receipt id, redaction class). Some fields are log-set (sequence number, segment id, previous receipt hash, integrity state). An emitter that tries to set a log-owned field fails closed at `Append`. A log that tries to mutate an emitter-owned field fails the chain audit at the next `VerifyChain`.
- **I3 — Receipts are immutable post-seal.** Once a receipt is appended and its containing segment is sealed (S3.1 §7), the receipt's bytes are constitutionally fixed (`INV-005`). Corrections are new receipts referencing the old ones via lineage; original receipts are never edited.
- **I4 — Receipts never carry secret material.** Per `INV-015`, the payload field is filtered through a redaction layer at emit time; the redaction outcome is recorded in `redaction_class`. Receipts that fail redaction validation are rejected at append and emit a separate `RECEIPT_REDACTION_FAILED` record (FOREVER) to make the rejection itself auditable.
- **I5 — Receipts are signed by their emitting subject.** Every receipt carries an Ed25519 signature over the canonical bytes of its non-signature fields, computed under a vault-broker capability bound to the claimed `subject_canonical_id` (S5.2). A signature that does not verify against a subject-bound capability is **a forgery**, not a "different" receipt — the field-level claim of authorship is the field-level commitment of the subject's vault key.
- **I6 — Lineage is a DAG.** Receipts may reference parent receipts via `parent_receipt_id` to build cause-and-effect lineage (`APPROVAL_GRANTED` parents `EXECUTION_SUCCEEDED`). The lineage graph is acyclic; cycle detection at audit time emits `RECEIPT_LINEAGE_CYCLE_DETECTED` (FOREVER) and quarantines the involved segment.
- **I7 — Receipt integrity is checkable per-receipt and per-segment.** A single receipt can be audited for self-consistency (signature, redaction class, retention-class match against `RecordType`); a segment is audited for chain integrity (`previous_receipt_hash` continuity, sealed-segment Ed25519 signature) per S3.1 §5.

## 3. The canonical receipt envelope

```proto
syntax = "proto3";
package aios.evidence.v1alpha1;

import "google/protobuf/timestamp.proto";

// EvidenceReceipt is the constitutional shape carried by every record in the
// closed RecordType vocabulary. The fields here are authoritative; per-type
// payloads live inside `payload` (oneof in S3.1 §4) and never duplicate or
// re-encode envelope-level fields.
message EvidenceReceipt {

  // ── Identity (assigned at append) ──────────────────────────────────────────
  string receipt_id = 1;
    // Format: "recpt_" + hex_lower(BLAKE3(JCS(this without ed25519_signature
    //                                  and without log-set fields)))[:32].
    // Canonical hash convention follows S0.1 §8.5.
    // The receipt_id is computed by the Evidence Log on append, not by the
    // emitter; an emitter-supplied receipt_id is rejected as IllegalAuthorityField.

  string segment_id = 2;
    // Format: "seg_<hex>" per S3.1 §7.1. The segment into which this receipt
    // was sealed. Set by the log on append; cannot be set by the emitter.

  uint64 sequence_number = 3;
    // Strictly monotonic within `segment_id`. Set by the log on append.
    // Replay (same sequence_number twice in the same segment) is rejected at
    // append per S3.1 §11.1 and emits CHAIN_INCONSISTENCY_DETECTED.

  // ── Temporal ───────────────────────────────────────────────────────────────
  google.protobuf.Timestamp emitted_at = 10;
    // Server-authoritative wall-clock at append time (not the emitter's clock).
    // The log writes this; an emitter-supplied value in this field is overwritten
    // and ignored. (Emitter clocks may appear inside payload as advisory only.)

  string tai64n = 11;
    // Format: 24 lowercase hex characters per the TAI64N convention.
    // Set by the log on append. Used for clock-skew-resistant ordering at audit
    // time: even if `emitted_at` jitters across time-source switches, the TAI64N
    // sequence remains monotonic per the constitutional clock (TAI). Drift
    // between emitted_at and tai64n beyond ±5 s is a TIME_DRIFT_DETECTED signal.

  // ── Authorship ─────────────────────────────────────────────────────────────
  string subject_canonical_id = 20;
    // S5.1 §4.2 format: matches /^[a-z_][a-z0-9_-]{0,62}(:[a-z0-9_-]+)+$/, total
    // length cap 256 bytes. Examples:
    //   "family:alice"
    //   "_system:service:capability-runtime"
    //   "finance:workflow:quarterly-close:run-7842".
    // The subject claimed to have emitted this receipt. The Ed25519 signature
    // (field 60) must verify against a vault capability (S5.2) whose owner is
    // exactly this subject; mismatch is a forgery, not an authorship error.

  bool subject_is_ai = 21;
    // Mirror of the active identity bundle's `is_ai` flag for `subject_canonical_id`.
    // Recorded in the receipt itself so audit cannot be deflected by retroactive
    // identity-bundle edits (the bundle is versioned, but receipts pin the bit
    // they saw at emit time). Inconsistency between this bit and the bundle at
    // emit time → reject at append.

  string acting_session_id = 22;
    // The S5.1 §8 session id under which the action that produced this receipt
    // ran. Empty only for receipts emitted by the log itself (e.g.
    // SEGMENT_SEALED, CHAIN_CHECKPOINT) — those receipts always carry
    // subject_canonical_id = "_system:service:evidence-log".

  // ── Scope ──────────────────────────────────────────────────────────────────
  // Per S4.1 §12.6 / S3.1 §23.1, every receipt carries the namespace triple.
  string scope = 30;
    // Closed values: "_system" | "groups". Per S4.1.
    // "_system" for system-internal records (segment seal, chain checkpoint,
    // bundle load, recovery events). "groups" otherwise.

  string group_id = 31;
    // Empty when scope = "_system". Otherwise the S4.1 group_id matching the
    // S4.1 §7.1 regex for groups. The receipt's "owning group" — controls
    // privacy ceiling on Query (§5).

  string user_id = 32;
    // Empty for system and group-scope events. Set when the underlying record
    // is user-private (e.g. a user-private surface event under
    // /aios/groups/<G>/users/<U>/...).

  // ── Content ────────────────────────────────────────────────────────────────
  RecordType record_type = 40;
    // Closed enum from S3.1 §4 (now 205 narrative entries through Wave 7).
    // This contract does NOT redefine RecordType — it cites and binds.
    // RECORD_TYPE_UNSPECIFIED is rejected at append with InvalidRecordType.

  RetentionClass retention = 41;
    // Closed enum from S3.1 §6.4 / §13: STANDARD_24M | EXTENDED_60M | FOREVER.
    // The retention class for a given RecordType is **fixed** by S3.1's
    // narrative tables (§13, §23.3, §24, §25, §26). An emitter that supplies a
    // retention class that disagrees with the S3.1 mapping for the given
    // record_type fails closed with RetentionClassMismatch. Operator override
    // (extending retention) is a separate flow recorded in the log itself, not
    // a per-receipt field.

  RecordPayload payload = 42;
    // The discriminated oneof from S3.1 §4 / Appendix A. The variant set
    // selected MUST match record_type (e.g. record_type = POLICY_DECISION
    // requires payload.policy_decision = ...). Mismatch is rejected at append
    // with PayloadVariantMismatch.

  // ── Lineage ────────────────────────────────────────────────────────────────
  string action_id = 50;
    // S0.1 ActionId — "act_<hex>" per the canonical hash convention. Empty for
    // receipts not emitted on behalf of a typed action (segment seal, recovery
    // event, model call without a parent action). When non-empty, the log
    // cross-references `action_id` against the action lifecycle index (S10.1)
    // and rejects orphans with OrphanedActionRef.

  string parent_receipt_id = 51;
    // The immediate logical parent in the evidence DAG. Empty for ORIGIN
    // receipts. Examples:
    //   APPROVAL_GRANTED.parent_receipt_id = APPROVAL_REQUESTED.receipt_id
    //   EXECUTION_SUCCEEDED.parent_receipt_id = APPROVAL_GRANTED.receipt_id
    //   VERIFICATION_RESULT.parent_receipt_id = EXECUTION_SUCCEEDED.receipt_id
    // Cycles (a chain that revisits a node already on its lineage path) are
    // detected at audit time and emit RECEIPT_LINEAGE_CYCLE_DETECTED (§11.4).

  string previous_receipt_hash = 52;
    // S3.1 §5.1: hex_lower(BLAKE3(JCS(prior_receipt without ed25519_signature)))[:32].
    // Genesis sentinel for first receipt of a segment: 64 zero chars truncated
    // to 32 ("00000000000000000000000000000000"). Genesis receipt of each
    // non-first segment links to the prior segment's seal hash
    // (SegmentSealedPayload.final_receipt_hash) for cross-segment continuity
    // per S3.1 §5.2. Set by the log on append.

  EvidenceGrade grade = 53;
    // S6.2 closed enum E0..E5. The grade *this single receipt contributes
    // toward*, not the capability's accumulated grade. Examples:
    //   ARTIFACT_RECORDED → contributes E1
    //   BUILD_PASSED → contributes E2
    //   TEST_PASSED → contributes E3
    //   E2E_PASSED / RECOVERY_REHEARSAL_PASSED / RELEASE_GATE_PASSED → contributes E4
    //   OPERATIONAL_HEALTHY → contributes E5
    // Receipts that are not grade-promotion receipts (e.g. POLICY_DECISION,
    // EXECUTION_FAILED, TAMPER_DETECTED) carry grade = EVIDENCE_GRADE_UNSPECIFIED.

  // ── Integrity ──────────────────────────────────────────────────────────────
  bytes ed25519_signature = 60;
    // 64 bytes. Signature over JCS-canonicalized bytes of every other field of
    // this receipt EXCEPT log-set fields (receipt_id, segment_id,
    // sequence_number, emitted_at, tai64n, previous_receipt_hash) and EXCEPT
    // ed25519_signature itself. Verification requires signing_key_id (61) to
    // resolve to a vault capability (S5.2) bound to subject_canonical_id (20).

  string signing_key_id = 61;
    // The vault capability id (per S5.2) used to produce the signature. Format:
    // "vcap_<hex>". The vault catalog binds each capability id to exactly one
    // subject_canonical_id — at signature verification, the log re-reads the
    // capability's bound subject and checks identity with field 20.

  // ── Adversarial robustness ─────────────────────────────────────────────────
  RedactionClass redaction_class = 70;
    // Closed enum (§4.1). NONE for receipts with no redaction-eligible content;
    // SECRET_REDACTED when at least one secret-shaped field was dropped or
    // hashed; SUBJECT_PSEUDONYMIZED when a user-content field was replaced with
    // a pseudonym; FULL_REDACTED when the entire payload was withheld and only
    // the envelope is auditable. The redaction class is committed in the
    // signature scope, so an attacker cannot strip the marker after the fact.

  bool tamper_quarantined = 71;
    // Always false at emit time. Set to true by the log if and only if the
    // segment containing this receipt fails its integrity check at read time
    // (chain mismatch, segment Ed25519 fails, sequence non-monotonic). The
    // quarantine state is a *read-side* annotation surfaced through the gRPC
    // surface (S3.1 §17), NOT a mutation of the sealed bytes — sealed bytes
    // remain bit-identical per INV-005.
}
```

The full proto IDL for `RecordType`, `RecordPayload`, `RetentionClass`, and `EvidenceGrade` lives in S3.1 Appendix A and S6.2 §3. This contract does **not** restate them; it imports them.

## 4. Closed enums declared by this contract

This contract introduces four closed enums that did not exist before. They are owned by `aios.evidence.v1alpha1` and are referenced from the `EvidenceReceipt` envelope.

### 4.1 `RedactionClass`

```proto
enum RedactionClass {
  REDACTION_CLASS_UNSPECIFIED   = 0;
  REDACTION_CLASS_NONE          = 1;
  REDACTION_CLASS_SECRET_REDACTED        = 2;
  REDACTION_CLASS_SUBJECT_PSEUDONYMIZED  = 3;
  REDACTION_CLASS_FULL_REDACTED          = 4;
}
```

| Value                   | Meaning                                                                                                                                                                                                                                               |
| ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `NONE`                  | Payload contains no redaction-eligible fields, or all eligible fields tested clean against every rule in §6.                                                                                                                                          |
| `SECRET_REDACTED`       | At least one secret-shaped field (PEM block, password, API token, raw key material) was dropped or replaced with `hex_lower(BLAKE3(value))[:32]` per the rule. The receipt is auditable; the secret is not present.                                   |
| `SUBJECT_PSEUDONYMIZED` | At least one user-text field (chat message, prompt body, free-form `reason` field) was replaced with a pseudonym keyed off the subject's canonical id. Audit can correlate per-subject without revealing content.                                     |
| `FULL_REDACTED`         | The entire payload was withheld at emit time (e.g. classified privacy class, recovery-mode forensic capture). The envelope (subject, scope, record type, action id, lineage, signature) remains auditable; the payload field carries an empty marker. |

`UNSPECIFIED` is rejected at append. The redaction class is **committed in the Ed25519 signature scope** — an adversary who edits the post-seal class also breaks the signature.

### 4.2 `ReceiptIntegrityState`

A read-side state surfaced by the gRPC `ReadReceipt` / `Subscribe` / `Query` responses. Not stored inside the sealed receipt bytes; computed at read time.

```proto
enum ReceiptIntegrityState {
  RECEIPT_INTEGRITY_STATE_UNSPECIFIED = 0;
  RECEIPT_INTEGRITY_STATE_PENDING            = 1;
  RECEIPT_INTEGRITY_STATE_SEALED             = 2;
  RECEIPT_INTEGRITY_STATE_VERIFIED           = 3;
  RECEIPT_INTEGRITY_STATE_TAMPER_QUARANTINED = 4;
  RECEIPT_INTEGRITY_STATE_RETIRED            = 5;
}
```

| Value                | Meaning                                                                                                                                                                                                   |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `PENDING`            | Receipt has been appended to the active segment but the segment is not yet sealed. Hash chain is provisional; `tamper_quarantined` is unset.                                                              |
| `SEALED`             | Containing segment has been sealed (S3.1 §7.3). Segment-level Ed25519 signature exists; chain is canonical.                                                                                               |
| `VERIFIED`           | Most recent `VerifyChain` covering this receipt's segment passed. The state is bumped to VERIFIED on success and stays VERIFIED until a later audit changes it.                                           |
| `TAMPER_QUARANTINED` | Last verification of the containing segment failed; the receipt is read-fenced (the log returns the receipt with `tamper_quarantined = true`). The sealed bytes are unchanged; this is a read annotation. |
| `RETIRED`            | Containing segment has reached the end of its retention horizon and the payload bytes have been GC'd. The receipt id and previous_receipt_hash are preserved as a tombstone (S3.1 §13).                   |

### 4.3 `LineageRelation`

The relation a `parent_receipt_id` link expresses. Present only at audit / query time when walking the lineage DAG; not stored in the receipt body.

```proto
enum LineageRelation {
  LINEAGE_RELATION_UNSPECIFIED  = 0;
  LINEAGE_RELATION_ORIGIN        = 1;
  LINEAGE_RELATION_PARENT_OF     = 2;
  LINEAGE_RELATION_CHILD_OF      = 3;
  LINEAGE_RELATION_SIBLING       = 4;
  LINEAGE_RELATION_DERIVED_FROM  = 5;
}
```

| Value          | Meaning                                                                                                                                                                                                                                                                                    |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `ORIGIN`       | The receipt has `parent_receipt_id = ""`. It is the head of a lineage tree (e.g. `ACTION_RECEIVED`, `SEGMENT_SEALED`, `INVARIANT_BUNDLE_LOADED`).                                                                                                                                          |
| `PARENT_OF`    | This relation, when traversed forward, leads to a child receipt that names this one as parent.                                                                                                                                                                                             |
| `CHILD_OF`     | Inverse of `PARENT_OF`. Lineage walks backward from effect to cause.                                                                                                                                                                                                                       |
| `SIBLING`      | Two receipts share the same `parent_receipt_id` (e.g. an `APPROVAL_GRANTED` and an `APPROVAL_DENIED` derived from the same `APPROVAL_REQUESTED` — only one sibling can be present in a valid lineage; multiple siblings of mutually exclusive type are an audit-time consistency failure). |
| `DERIVED_FROM` | A retro-emitted forensic record (e.g. `OVERRIDE_REVIEW` per S5.4) that derives from one or more prior receipts without being a strict child.                                                                                                                                               |

### 4.4 `RedactionRule`

The rule applied to produce the `redaction_class`. Multiple rules may apply to a single receipt; the resulting `redaction_class` is the strongest applied rule's class.

```proto
enum RedactionRule {
  REDACTION_RULE_UNSPECIFIED              = 0;
  REDACTION_RULE_SECRET_FIELD_DROPPED     = 1;
  REDACTION_RULE_KEY_MATERIAL_HASHED      = 2;
  REDACTION_RULE_USER_TEXT_PSEUDONYMIZED  = 3;
  REDACTION_RULE_NETWORK_PAYLOAD_HASHED   = 4;
  REDACTION_RULE_FULL_PAYLOAD_WITHHELD    = 5;
}
```

| Rule                      | Trigger                                                                                                                                               | Mapped class            |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- |
| `SECRET_FIELD_DROPPED`    | Field name matches the secret-name catalog (e.g. `password`, `api_key`, `private_key`); field is dropped from payload.                                | `SECRET_REDACTED`       |
| `KEY_MATERIAL_HASHED`     | Field value matches a key-material shape (PEM block, JWK private, raw token); replaced with `hex_lower(BLAKE3(value))[:32]`.                          | `SECRET_REDACTED`       |
| `USER_TEXT_PSEUDONYMIZED` | Free-form user text (prompt body, chat message, `reason` field) replaced with a stable pseudonym `subj_<hex>` keyed off the subject id and the field. | `SUBJECT_PSEUDONYMIZED` |
| `NETWORK_PAYLOAD_HASHED`  | A captured network frame / TLS body / raw socket buffer is replaced with its content hash.                                                            | `SECRET_REDACTED`       |
| `FULL_PAYLOAD_WITHHELD`   | Payload classification is `CLASSIFIED` (S1.2 §5) or recovery forensic; the entire payload is dropped and an empty marker stored.                      | `FULL_REDACTED`         |

The rule(s) actually applied are recorded inside the receipt's payload via S3.1's existing `redaction_profile` field (already present on the envelope from S3.1 §3) — this contract narrows that field's permitted values to the `RedactionRule` enum names plus a literal `"none"` for receipts with `redaction_class = NONE`. Free-form values in `redaction_profile` are rejected at append.

## 5. Field-by-field discipline

The following table enumerates each envelope field, who sets it, whether it is part of the signature scope, and what an attacker would attempt against it.

| Field                        | Set by                         | In signature scope?            | Adversarial concern                                                                               | Defense                                                                                                                                  |
| ---------------------------- | ------------------------------ | ------------------------------ | ------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `receipt_id` (1)             | Log                            | No                             | Emitter sets a chosen id to collide with a future canonical id.                                   | Log computes; emitter-supplied value rejected at append (`IllegalAuthorityField`).                                                       |
| `segment_id` (2)             | Log                            | No                             | Emitter claims a different segment to break chain queries.                                        | Log assigns; emitter-supplied value rejected.                                                                                            |
| `sequence_number` (3)        | Log                            | No                             | Replay (same seq twice) to inflate evidence count.                                                | S3.1 §11.1: rejected at append; emits `CHAIN_INCONSISTENCY_DETECTED`.                                                                    |
| `emitted_at` (10)            | Log                            | No                             | Emitter forges a timestamp to fit a backdated narrative.                                          | Log writes server clock; emitter clocks live in payload, advisory only.                                                                  |
| `tai64n` (11)                | Log                            | No                             | Emitter forges TAI64N to defeat skew detection.                                                   | Log writes TAI64N from constitutional clock; drift > 5 s vs `emitted_at` emits `TIME_DRIFT_DETECTED`.                                    |
| `subject_canonical_id` (20)  | Emitter                        | Yes                            | Forge another subject's id to attribute an action falsely.                                        | Ed25519 signature must verify against vault capability bound to this exact subject; mismatch is `RECEIPT_FORGERY_DETECTED`.              |
| `subject_is_ai` (21)         | Emitter                        | Yes                            | Lie about AI-ness to bypass `INV-016` (AI cannot self-grade).                                     | Identity bundle cross-check at append; mismatch rejected with `IdentityBundleMismatch`.                                                  |
| `acting_session_id` (22)     | Emitter                        | Yes                            | Forge a session id to attach receipt to a session not actually held.                              | Identity service cross-checks `(subject_canonical_id, session_id)` exists and is active at append.                                       |
| `scope` (30)                 | Emitter                        | Yes                            | Mismatch with action target to bypass cross-group privacy ceiling.                                | S4.1 cross-check against the resolved `target.scope` for `action_id`-bound receipts; rejected with `NamespaceScopeMismatch`.             |
| `group_id` (31)              | Emitter                        | Yes                            | Same as above.                                                                                    | Same as above.                                                                                                                           |
| `user_id` (32)               | Emitter                        | Yes                            | Same as above.                                                                                    | Same as above.                                                                                                                           |
| `record_type` (40)           | Emitter                        | Yes                            | Pick a permissive record type that does not match the payload.                                    | Append authority table (S3.1 §17, §24.3, §25.x, §26.x): only specific subjects may emit specific record types; mismatch rejected.        |
| `retention` (41)             | Emitter                        | Yes                            | Downgrade retention to make a forensic event compactable.                                         | Retention-class-by-record-type mapping is fixed by S3.1; mismatch rejected with `RetentionClassMismatch`.                                |
| `payload` (42)               | Emitter                        | Yes                            | Embed a secret in payload; embed a payload variant not matching record_type.                      | Redaction validation (§6); variant cross-check (`PayloadVariantMismatch`).                                                               |
| `action_id` (50)             | Emitter                        | Yes                            | Reference a non-existent action to claim a receipt belongs to a fictional flow.                   | Action lifecycle index cross-check; orphan rejected with `OrphanedActionRef`.                                                            |
| `parent_receipt_id` (51)     | Emitter                        | Yes                            | Reference a receipt that does not exist or that lives in a quarantined segment to inject lineage. | Cross-check at append: parent must exist, must be sealed or in active segment, must not be in `TAMPER_QUARANTINED` state.                |
| `previous_receipt_hash` (52) | Log                            | No (computed from prior bytes) | Pre-seed the field to break chain audit.                                                          | Log overwrites with computed value at append; emitter-supplied value ignored. Mismatch at audit triggers `CHAIN_INCONSISTENCY_DETECTED`. |
| `grade` (53)                 | Emitter                        | Yes                            | AI emits `BUILD_PASSED` with `grade = E2` for AI-authored code.                                   | `INV-016` (S6.2 §10.6) check at append; rejected with `ProducerCannotSelfGrade`.                                                         |
| `ed25519_signature` (60)     | Emitter                        | N/A (the signature itself)     | Forge by signing with attacker's key.                                                             | Verification against vault-bound capability per (61).                                                                                    |
| `signing_key_id` (61)        | Emitter                        | Yes                            | Reference a capability bound to a different subject.                                              | Vault catalog lookup at append: capability's `bound_subject` must equal field 20; mismatch rejected with `KeySubjectMismatch`.           |
| `redaction_class` (70)       | Emitter (computed by redactor) | Yes                            | Strip the marker after the fact.                                                                  | Committed in signature scope; post-seal edit breaks signature.                                                                           |
| `tamper_quarantined` (71)    | Log (read side)                | N/A (always false at emit)     | Set to false to suppress quarantine.                                                              | Always false at emit; the read-side annotation is computed at read time from segment audit state.                                        |

For every emitter-set field that is in the signature scope, post-seal modification of the bytes breaks the signature, breaks the `previous_receipt_hash` of the next receipt, and is detected by `VerifyChain`. For every log-set field, the log is the only authority and the emitter has no surface to influence it.

## 6. The "no secrets in evidence" rule

`INV-015` requires that evidence records never carry secret values. This contract operationalizes the rule.

### 6.1 The redaction layer at emit time

Every `Append` request entering the Evidence Log passes through a redaction layer **before** the receipt is sealed into a segment. The layer:

1. Walks the payload's proto fields by name.
2. Matches each field name against the **secret-name catalog** (managed by L4 vault, see also S1.1 §17.2.6). Catalog entries are exact field names: `password`, `passphrase`, `api_key`, `secret`, `token`, `auth_token`, `bearer_token`, `private_key`, `client_secret`, etc.
3. Matches each field value against the **secret-shape catalog**: PEM block patterns (`-----BEGIN ... PRIVATE KEY-----`), JWK private key shapes, raw token regex (`/^[A-Za-z0-9_\\-]{32,}$/` plus length-and-entropy heuristic), API key shapes from the registered providers, etc.
4. Applies the rule from §4.4 corresponding to the strongest match.
5. Records the rule in `redaction_profile` (S3.1 envelope field) and the resulting class in `redaction_class` (this contract's field 70).

### 6.2 What "fail closed" means here

If the redaction layer cannot complete (catalog unavailable, redactor process failed, redaction rule registry corrupted, etc.), the `Append` request is **rejected**. The log emits a separate forensic record:

| Record type                | Retention | Source spec | Purpose                                                                                                        |
| -------------------------- | --------- | ----------- | -------------------------------------------------------------------------------------------------------------- |
| `RECEIPT_REDACTION_FAILED` | `FOREVER` | S6.3 §6.2   | A receipt that contained secret-shaped content was rejected at emit time because redaction could not complete. |

The `RECEIPT_REDACTION_FAILED` record carries (a) the rejected receipt's `record_type`, (b) the calling subject id, (c) the redaction-rule registry version, (d) the failure reason class. It does **not** carry the rejected receipt's payload — by construction, the payload contained secret-shaped content.

### 6.3 Why rejection rather than silent stripping

Silent stripping leaves no signal that a receipt was emitted with secret content. An audit reading the log later would see a redacted receipt and have no way to know whether the redaction was clean or whether the redactor merely failed and dropped the content. Rejection-with-forensic-record means: the log knows a secret-bearing emit was attempted, the operator knows, and the calling subject's behavior is auditable.

### 6.4 Privacy class and `FULL_REDACTED`

For payloads classified `CLASSIFIED` (S1.2 §5) or for recovery-mode forensic capture, the redaction layer applies `REDACTION_RULE_FULL_PAYLOAD_WITHHELD`: the entire payload field is replaced with an empty marker, and `redaction_class = FULL_REDACTED`. The envelope (subject, scope, record type, action id, lineage, signature) is unchanged and remains auditable. The log preserves the envelope; the payload bytes are not stored.

This is the difference between "we recorded that this happened, redacting content" and "we did not record that this happened" — the former is `FULL_REDACTED`, the latter is a constitutional violation of `INV-005` (`evidence is append-only`, including for sensitive events).

## 7. Lineage discipline

### 7.1 The lineage DAG

Receipts form a directed acyclic graph (DAG) where nodes are receipts and edges are `parent_receipt_id` references. A receipt with `parent_receipt_id = ""` is an **origin**; otherwise it is a **derived** receipt.

Common lineage shapes:

```text
Action lifecycle (S0.1 + S10.1):
  ACTION_RECEIVED  ──parent──▶  ACTION_VALIDATED
                                        │
                                        ▼
                              ACTION_POLICY_DECISION
                                        │
           ┌────────────────────────────┼────────────────────────────┐
           ▼                            ▼                            ▼
  APPROVAL_REQUESTED          ACTION_DISPATCHED           POLICY_DECISION (DENY)
           │                            │
           ▼                            ▼
  APPROVAL_GRANTED            EXECUTION_SUCCEEDED
                                        │
                                        ▼
                              VERIFICATION_RESULT

Segment lifecycle (S3.1):
  SEGMENT_SEALED   ──derived──▶  CHAIN_CHECKPOINT  (next segment's audit)

Forensic derivation (S5.4):
  OVERRIDE_REQUESTED  ──parent──▶  OVERRIDE_QUORUM_RECEIVED  ──parent──▶  OVERRIDE_GRANTED
                                                                                  │
                                                                                  ▼
                                                                        OVERRIDE_CONSUMED
                                                                                  │
                                                                                  ▼
                                                                      OVERRIDE_REVIEW (DERIVED_FROM)
```

### 7.2 Walking lineage

**Forward walk (cause to effect):** start from an origin receipt; for each receipt, query the index `by_parent_receipt_id` (a new index added to S3.1 §8 by this contract) for its children; recurse.

**Backward walk (effect to cause):** start from a leaf receipt; follow `parent_receipt_id` to its parent; recurse until `parent_receipt_id = ""`.

Both walks are bounded: at audit time, a depth budget (default 1024 hops) protects against pathological lineage; exceeding the budget emits a `RECEIPT_LINEAGE_DEPTH_EXCEEDED` advisory record (`STANDARD_24M`) and the audit truncates with a marker.

### 7.3 Cycle detection

A cycle in the lineage DAG is a constitutional fault. Cycles can arise only from:

- A bug in an emitter that names a parent that itself names this receipt (forward in time, only possible if an emitter sees a receipt that has not yet been appended — i.e., never).
- A tamper that rewrites a sealed receipt's `parent_receipt_id` to point at a child (broken by signature check).
- A retro-emit (`OVERRIDE_REVIEW`, `LINEAGE_RELATION_DERIVED_FROM`) that is mis-classified as a strict parent and creates a cycle through the DERIVED edge.

Cycle detection runs (a) at every `parent_receipt_id` lookup at append (constant-time check that the named parent is not in the calling subject's open lineage frame for the current correlation), and (b) as a scheduled audit (default daily) walking the DAG with a visited set. Detection emits:

| Record type                      | Retention | Source spec | Purpose                                                                                                               |
| -------------------------------- | --------- | ----------- | --------------------------------------------------------------------------------------------------------------------- |
| `RECEIPT_LINEAGE_CYCLE_DETECTED` | `FOREVER` | S6.3 §7.3   | A cycle in the lineage DAG was detected at append or audit time. Carries the involved receipt ids and the cycle span. |

The detection itself causes the involved receipts' segments to be marked `TAMPER_QUARANTINED` on read; the sealed bytes are not modified.

## 8. Per-grade evidence quality requirements

Receipt-level requirements per `EvidenceGrade` (cited from S6.2; this section binds the grade-relevant fields).

| Grade | Receipt fields required to be populated                                                                                                          | Lineage requirement                                                                                                                      |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `E0`  | The receipt itself counts as null evidence. No special envelope discipline beyond §3.                                                            | None.                                                                                                                                    |
| `E1`  | `payload` non-empty for record types `ARTIFACT_RECORDED` (and the equivalents from S6.2 §3.2). `redaction_class` set; `signing_key_id` resolves. | Origin acceptable; no parent required.                                                                                                   |
| `E2`  | `record_type = BUILD_PASSED` (per S6.2 §3.3); payload carries build-system id, exit code, `aiosfs_pointer` of the build log artifact.            | Should reference an `ARTIFACT_RECORDED` parent for the source artifact; not strictly required.                                           |
| `E3`  | `record_type = TEST_PASSED`; payload carries test-runner id, test-name list, exit code, capability ids covered.                                  | Lineage to a `BUILD_PASSED` parent for the same capability id is **required** at audit; missing parent → grade does not promote.         |
| `E4`  | `record_type ∈ {E2E_PASSED, RECOVERY_REHEARSAL_PASSED, RELEASE_GATE_PASSED}`; payload carries scenario id and action chain reference.            | Lineage chain to `BUILD_PASSED` and `TEST_PASSED` for the same capability id required.                                                   |
| `E5`  | Rolling window of `OPERATIONAL_HEALTHY` receipts (S6.2 §3.6): at least 7 receipts in the last 14 days; the most recent within 24 h.              | Each `OPERATIONAL_HEALTHY` receipt should reference its capability's most recent `E2E_PASSED` ancestor as a `DERIVED_FROM` lineage edge. |

A receipt with `grade != EVIDENCE_GRADE_UNSPECIFIED` whose payload type does not match the grade's required record type is rejected at append with `GradeRecordTypeMismatch`.

## 9. Adversarial robustness

### 9.1 Forged subject id

**Attack:** Adversary crafts a receipt with `subject_canonical_id = "family:alice"` claiming Alice as the actor, signs with the adversary's own Ed25519 key, and `Append`s.

**Defense:** At append, the log:

1. Resolves `signing_key_id` (61) in the L4 vault catalog.
2. Reads the `bound_subject` of that capability.
3. Compares `bound_subject` with `subject_canonical_id` (20).
4. On mismatch, rejects with `KeySubjectMismatch` and emits `RECEIPT_FORGERY_DETECTED` (FOREVER, see §13).

The adversary cannot mount a successful forgery without compromising Alice's vault key — and per `INV-018`, raw key material does not leave the vault broker.

### 9.2 Forged hash chain link

**Attack:** Adversary edits a sealed receipt's `previous_receipt_hash` to repoint at a different prior receipt, hoping `VerifyChain` follows the new link.

**Defense:** `VerifyChain` recomputes `previous_receipt_hash` from the actual prior receipt's canonical bytes; mismatch produces `CHAIN_INCONSISTENCY_DETECTED` (S3.1 §11.4) and the segment is moved to `TAMPER_QUARANTINED` integrity state. The receipt's signature also breaks because `previous_receipt_hash`, while log-set, is not in the signature scope; the chain check itself is the defense, and the segment-level Ed25519 segment seal (S3.1 §7) provides a second independent integrity layer.

### 9.3 Replay of an old receipt as new

**Attack:** Adversary captures a sealed receipt, recomputes nothing, and `Append`s the same bytes into the active segment.

**Defense:** Sequence numbers are monotonic per segment (S3.1 §11.1); the active segment has its own sequence space. The replayed receipt arrives with a different `segment_id` and `sequence_number` than the captured one — the log assigns those at append. The receipt thus has a **different `receipt_id`** because the receipt id is `BLAKE3` over JCS of the receipt's bytes (which now include new log-set fields). The replayed receipt is a different evidence record, recorded once. The original sealed receipt is still in its original segment.

This is not a security failure — it is the system working: an attacker who replays a receipt creates a new receipt and signs nothing valuable. To make a replay observable as malicious, the audit cross-references receipts whose payloads have a `payload_hash` (S3.1 envelope §3 field 12) seen in a prior segment; collisions on `payload_hash` across distant segments emit `RECEIPT_PAYLOAD_DUPLICATE_OBSERVED` (`STANDARD_24M`, advisory) — not a tamper, but a flag for the human reviewer.

### 9.4 Out-of-order timestamps via clock manipulation

**Attack:** Adversary races the operator's clock (NTP poisoning, virtual machine clock skew) to backdate or forward-date receipts.

**Defense:** Two layers. First, `emitted_at` is server-authoritative — the log writes its own clock, not the emitter's (§5). Second, `tai64n` (11) is sourced from the constitutional clock (TAI), which is a separate time source; drift between `emitted_at` and `tai64n` beyond ±5 s emits `TIME_DRIFT_DETECTED` (`EXTENDED_60M`). For audit ordering, TAI64N is the canonical source — it is monotonic by construction across UTC leap-second events and across NTP step-corrections.

### 9.5 Receipt with embedded secret in payload

**Attack:** Emitter sends a `MODEL_CALL` payload that contains the API key for the external provider in the `prompt_body` field.

**Defense:** Redaction layer at emit time (§6). The secret-name catalog matches `api_key` and any field whose value matches the API-key shape; secret-shape catalog catches PEM blocks and high-entropy tokens even in unnamed fields. On match, `REDACTION_RULE_KEY_MATERIAL_HASHED` (or `REDACTION_RULE_SECRET_FIELD_DROPPED`) is applied; the receipt's `redaction_class = SECRET_REDACTED`. If redaction cannot complete, the receipt is rejected and `RECEIPT_REDACTION_FAILED` (FOREVER) is emitted.

### 9.6 Receipt claiming nonexistent action_id

**Attack:** Adversary references `action_id = "act_<hex>"` for an action that was never validated, hoping to insert a fake receipt that looks legitimate when queried by action id.

**Defense:** Action lifecycle index cross-check at append (§5). The Capability Runtime maintains the canonical action-id index (S10.1); the log calls into the index at append; orphan rejected with `OrphanedActionRef` and emits `RECEIPT_ORPHAN_ACTION_REF_DETECTED` (`EXTENDED_60M`).

### 9.7 Forged lineage parent

**Attack:** Adversary references `parent_receipt_id` for a receipt that does not exist or that lives in a tamper-quarantined segment.

**Defense:** Parent existence cross-check at append (§5). Missing parent rejected with `OrphanedParentRef`; quarantined-parent rejected with `QuarantinedParentRef` (the rejection itself is logged but does not propagate the quarantine — the calling subject sees the failure and re-emits without the bad parent reference, or the human investigates).

### 9.8 Out-of-order sequence at append

**Attack:** Concurrent emitters race to append, hoping a delayed append slips into a sequence position out of order.

**Defense:** S3.1 §11.1 strict monotonicity at append; the log serializes appends per active segment. A receipt that arrives while a higher sequence number is already assigned is given the next sequence number, not the one it expected. The new forensic record:

| Record type                     | Retention | Source spec | Purpose                                                                                                               |
| ------------------------------- | --------- | ----------- | --------------------------------------------------------------------------------------------------------------------- |
| `RECEIPT_SEQUENCE_OUT_OF_ORDER` | `FOREVER` | S6.3 §9.8   | A sequence-ordering anomaly was detected at audit time (e.g. recovered WAL replay produced a non-monotonic sequence). |

This is distinct from `CHAIN_INCONSISTENCY_DETECTED`, which covers chain-hash mismatch. `RECEIPT_SEQUENCE_OUT_OF_ORDER` covers the bare sequence-number inversion case.

## 10. Worked examples

### 10.1 POLICY_DECISION receipt (full field-by-field walkthrough)

A typed action `act_a1b2c3d4` flows through the Policy Kernel; a `POLICY_DECISION` is emitted.

```text
Setting:
  Action: act_a1b2c3d4 — file write under /aios/groups/family/users/alice/private/
  Subject: family:family-assistant (is_ai = true)
  Decision: REQUIRE_APPROVAL (action targets user-private space; AI subject)
  Bundle version: polb_5e6f7g8h9i0j1k2l3m4n5o6p7q8r9s0t

Receipt assembled by the Policy Kernel:

  EvidenceReceipt {
    // Identity (log will fill on append)
    receipt_id: ""
    segment_id: ""
    sequence_number: 0

    // Temporal (log will fill)
    emitted_at: <unset>
    tai64n: ""

    // Authorship (emitter sets)
    subject_canonical_id: "_system:service:policy-kernel"
    subject_is_ai: false
    acting_session_id: "sess_pk_root_2026-05-09T08:14:23Z"

    // Scope
    scope: "groups"
    group_id: "family"
    user_id: "alice"

    // Content
    record_type: POLICY_DECISION
    retention: STANDARD_24M           // per S3.1 §13 default
    payload.policy_decision: PolicyDecisionPayload {
      policy_decision_id: "pdec_4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d"
      action_id: "act_a1b2c3d4..."
      decision: "REQUIRE_APPROVAL"
      reason_code: "ai_subject_user_private_target"
      bundle_version: "polb_5e6f7g8h..."
      enrichment_snapshot_id: "enr_..."
      rules_consulted: 3
    }

    // Lineage
    action_id: "act_a1b2c3d4..."
    parent_receipt_id: "recpt_<hash of ACTION_VALIDATED for the same action>"
    previous_receipt_hash: ""         // log will fill
    grade: EVIDENCE_GRADE_UNSPECIFIED // POLICY_DECISION is not a grade-promotion record

    // Integrity
    ed25519_signature: <bytes>
    signing_key_id: "vcap_policy_kernel_signing_2026-05"

    // Adversarial robustness
    redaction_class: NONE
    tamper_quarantined: false
  }

Append flow:
  1. Log validates field authority (no emitter-set log fields).
  2. Log resolves vcap_policy_kernel_signing_2026-05; bound_subject =
     "_system:service:policy-kernel" — matches field 20. ✓
  3. Log verifies ed25519_signature over JCS-canonical bytes excluding log-set
     fields and signature. ✓
  4. Log validates record_type = POLICY_DECISION → only emitter authorized
     per S3.1 §17. ✓ ("_system:service:policy-kernel" is on the allowlist.)
  5. Log validates retention = STANDARD_24M matches the S3.1 §13 mapping for
     POLICY_DECISION. ✓
  6. Log validates payload variant matches record_type. ✓
  7. Redaction layer walks payload: no secret-name fields, no secret-shape
     values. redaction_class remains NONE. ✓
  8. Log validates parent_receipt_id exists, is sealed, not quarantined. ✓
  9. Log assigns:
       receipt_id = "recpt_" + hex_lower(BLAKE3(JCS(<receipt without sig and without log-set>)))[:32]
                  = "recpt_8f3a2b1c4d5e6f7a8b9c0d1e2f3a4b5c"
       segment_id = "seg_<active>"
       sequence_number = <next>
       emitted_at = 2026-05-09T08:14:23.471Z
       tai64n = "@4000000064000000abcd"
       previous_receipt_hash = hex_lower(BLAKE3(JCS(prev_receipt without sig)))[:32]
                             = "1f3e9a7c..."
  10. Log appends; receipt is durable; SUBSCRIBE consumers get the receipt.

Result:
  receipt_id = recpt_8f3a2b1c4d5e6f7a8b9c0d1e2f3a4b5c
  Forward lineage (later in this action's flow):
    APPROVAL_REQUESTED.parent_receipt_id = recpt_8f3a2b1c...
    APPROVAL_GRANTED.parent_receipt_id   = APPROVAL_REQUESTED.receipt_id
    EXECUTION_SUCCEEDED.parent_receipt_id = APPROVAL_GRANTED.receipt_id
```

The receipt is now part of the chain. An audit walking forward from `recpt_8f3a2b1c...` finds the approval pair and the execution result. An audit walking backward from `EXECUTION_SUCCEEDED` reaches this receipt and continues to `ACTION_VALIDATED`.

### 10.2 Lineage trace (DAG walk)

Continuing example 10.1, an auditor invokes "show me everything that derives from `act_a1b2c3d4`".

```text
Step 1: Query by action_id_filter = "act_a1b2c3d4...".
Step 2: Log returns receipts in (segment_id, sequence_number) order:
  R1: ACTION_RECEIVED              (origin)
  R2: ACTION_VALIDATED              (parent: R1)
  R3: POLICY_DECISION               (parent: R2) [example 10.1]
  R4: APPROVAL_REQUESTED            (parent: R3)
  R5: APPROVAL_GRANTED              (parent: R4)
  R6: ACTION_DISPATCHED             (parent: R5)
  R7: EXECUTION_SUCCEEDED           (parent: R6)
  R8: VERIFICATION_RESULT           (parent: R7)

Step 3: Forward walk from R1:
  R1 → R2 → R3 → R4 → R5 → R6 → R7 → R8

Step 4: Backward walk from R8:
  R8 → R7 → R6 → R5 → R4 → R3 → R2 → R1

Step 5: DAG check:
  Visited set: {R1, R2, R3, R4, R5, R6, R7, R8}
  Each node visited exactly once → no cycle ✓

Step 6: Verify each receipt's signature individually:
  For each Ri, look up signing_key_id, verify Ed25519 signature ✓

Step 7: Verify chain hashes:
  Each Ri.previous_receipt_hash matches BLAKE3(JCS(R{i-1} without sig))[:32] ✓
```

The eight-receipt lineage is the constitutional record of one action's lifecycle. Every receipt is independently audited (signature) and chain-audited (hash continuity). An attacker who attempts to remove R3 or replace R5 with a forged "GRANTED" must (a) forge the policy kernel's or approval manager's vault capability (hard), (b) re-compute and re-sign every receipt downstream (impossible without the same vault keys), and (c) make the segment Ed25519 seal verify (impossible without the operator key).

### 10.3 Forged subject id (rejection trace)

A malicious adapter `_system:service:rogue-adapter` attempts to emit a receipt claiming `subject_canonical_id = "family:family-assistant"`.

```text
Setting:
  Adversary: _system:service:rogue-adapter (compromised adapter binary)
  Adversary's vault capability: vcap_rogue_adapter_signing_2026-05
    bound_subject = "_system:service:rogue-adapter"
  Receipt forged:
    subject_canonical_id: "family:family-assistant"
    signing_key_id: "vcap_rogue_adapter_signing_2026-05"
    ed25519_signature: <signed with rogue adapter's key>
    record_type: APPROVAL_GRANTED   // attempt to fake an AI grant
    payload.approval_granted: ApprovalGrantedPayload {
      approval_receipt_id: "rcpt_synthetic_grant"
      approver_subject: "family:family-assistant"
      action_id: "act_d4e5f6..."
    }

Append validation:
  1. Log resolves signing_key_id = "vcap_rogue_adapter_signing_2026-05".
  2. Vault returns: bound_subject = "_system:service:rogue-adapter"
  3. Compare: bound_subject ("_system:service:rogue-adapter")
              !=
              subject_canonical_id ("family:family-assistant")
  4. Reject with KeySubjectMismatch.
  5. Emit RECEIPT_FORGERY_DETECTED (FOREVER):
     - calling_subject: "_system:service:rogue-adapter"
     - claimed_subject: "family:family-assistant"
     - signing_key_id: "vcap_rogue_adapter_signing_2026-05"
     - record_type_attempted: APPROVAL_GRANTED
     - rejection_reason: KeySubjectMismatch
     - emitted_at: 2026-05-09T08:14:25.092Z

Operator alert (per S6.4 §3 INV-005 violation pattern):
  - Rogue adapter is moved to ADAPTER_DEGRADED (S10.1)
  - Investigation triggered

Resulting state:
  - The forged receipt is never appended.
  - The forgery attempt itself is permanent forensic evidence.
  - No lineage is corrupted; APPROVAL_GRANTED for act_d4e5f6...
    requires a real approval from a human approver per INV-002 + INV-010.
```

The forgery defense is not "we detect the bad bytes after the fact" — it is "we never seal the bad bytes". The forensic record of the _attempt_ is sealed, with FOREVER retention, so the operational pattern of attempts is auditable.

## 11. Cross-spec dependencies

| Spec        | Direction | What this contract consumes / contributes                                                                                                                                                                                                                                                                                                      |
| ----------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S3.1        | consumer  | RecordType vocabulary (closed enum, 205 narrative entries); RetentionClass enum; segment model (segment_id, sealed Ed25519 signature, hash chain at segment level per §5); RecordPayload oneof; the existing envelope fields (payload_hash, payload_ref, redaction_profile, simulated). This contract refines and binds; it does not redefine. |
| S6.2        | consumer  | EvidenceGrade enum (E0..E5); the per-grade required record types (ARTIFACT_RECORDED, BUILD_PASSED, TEST_PASSED, E2E_PASSED, RECOVERY_REHEARSAL_PASSED, RELEASE_GATE_PASSED, OPERATIONAL_HEALTHY); the producer-cannot-self-grade rule (INV-016 enforcement at append).                                                                         |
| S6.4        | consumer  | INV-005 (evidence append-only — receipts immutable post-seal); INV-014 (no proof, no completion — receipts are the proof artifacts); INV-015 (evidence never contains secrets — redaction layer); INV-016 (AI cannot self-grade — grade-receipt producer check).                                                                               |
| S5.1        | consumer  | Subject canonical id format `^[a-z_][a-z0-9_-]{0,62}(:[a-z0-9_-]+)+$`; immutability of canonical id across the evidence trail; identity bundle cross-check at append for `subject_is_ai`.                                                                                                                                                      |
| S5.2        | consumer  | Vault capability binding for `signing_key_id`; the `bound_subject` lookup that defends against forgery (§9.1).                                                                                                                                                                                                                                 |
| S4.1        | consumer  | The `(scope, group_id, user_id)` triple semantics (envelope fields 30/31/32); namespace-scope cross-check for action-bound receipts.                                                                                                                                                                                                           |
| S0.1        | consumer  | BLAKE3 hash convention `hex_lower(BLAKE3(JCS(...)))[:32]`; ActionId format and the action lifecycle index (orphan-action defense).                                                                                                                                                                                                             |
| L4.2        | consumer  | The vault broker that serves `signing_key_id` lookups and produces the Ed25519 signatures (raw private keys never leave the broker).                                                                                                                                                                                                           |
| S2.4        | producer  | Four new properties contributed for L0 verification consolidation (queued narratively here; written to S2.4 by the orchestrator at consolidation): `RECEIPT_SIGNATURE_VERIFIED`, `RECEIPT_REDACTION_VALID`, `RECEIPT_LINEAGE_DAG`, `RECEIPT_RETENTION_MATCHES_TYPE`.                                                                           |
| L9.1 (S3.1) | producer  | Four new record types contributed to the closed `RecordType` vocabulary (queued narratively; full IDL reconciliation deferred to a subsequent S3.1 sweep): `RECEIPT_REDACTION_FAILED` FOREVER, `RECEIPT_INTEGRITY_QUARANTINED` FOREVER, `RECEIPT_LINEAGE_CYCLE_DETECTED` FOREVER, `RECEIPT_SEQUENCE_OUT_OF_ORDER` FOREVER.                     |

## 12. Acceptance criteria

- [ ] `EvidenceReceipt` envelope exists with the exact fields enumerated in §3 (identity, temporal, authorship, scope, content, lineage, integrity, adversarial robustness).
- [ ] Four new closed enums declared (`RedactionClass`, `ReceiptIntegrityState`, `LineageRelation`, `RedactionRule`); each non-`UNSPECIFIED` value has a row in §4's tables.
- [ ] Field authorship table (§5) enumerates every field; emitter-set fields in the signature scope; log-set fields rejected if emitter-supplied.
- [ ] Redaction layer at emit time (§6); rejection emits `RECEIPT_REDACTION_FAILED` (FOREVER); silent stripping is forbidden.
- [ ] Lineage discipline (§7): DAG only; cycle detection emits `RECEIPT_LINEAGE_CYCLE_DETECTED` (FOREVER); depth budget on walks.
- [ ] Per-grade receipt-field requirements per §8; grade-record-type mismatch rejected at append.
- [ ] Eight adversarial defenses (§9) each name the attack and the field-level defense.
- [ ] Three worked examples (§10): POLICY_DECISION receipt full walkthrough, lineage DAG trace, forged-subject rejection.
- [ ] Cross-spec dependency table (§11) is bidirectional: this contract consumes from S3.1 / S6.2 / S5.1 / S5.2 / S4.1 / S0.1 / L4.2 / S6.4; produces to S2.4 / S3.1.
- [ ] Four new record types (`RECEIPT_REDACTION_FAILED`, `RECEIPT_INTEGRITY_QUARANTINED`, `RECEIPT_LINEAGE_CYCLE_DETECTED`, `RECEIPT_SEQUENCE_OUT_OF_ORDER`) contributed to S3.1, all FOREVER retention.

## 13. Evidence record types this contract introduces

These are the record types added to the closed `RecordType` vocabulary by this S6.3 contract. Per the §23 / §24 / §25 / §26 narrative-only declaration pattern in S3.1, this contract does **not** edit S3.1 Appendix A; full IDL reconciliation is deferred to a subsequent S3.1 sweep. After this contract the **`RecordType` vocabulary now totals 209 entries narratively** (205 prior + 4 from S6.3). Append authority for all four is the Evidence Log itself; emission attempts from any other subject are hard-denied at the engine surface and emit `TAMPER_DETECTED` per S3.1 §11.5.

| RecordType                       | Retention | Source spec | Purpose                                                                                                                                                                                                                                                                                                                      |
| -------------------------------- | --------- | ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `RECEIPT_REDACTION_FAILED`       | `FOREVER` | S6.3 §6.2   | Emit-time redaction validation could not complete and the receipt was rejected because its payload contained secret-shaped content. Carries calling subject, redaction-rule registry version, failure-reason class, and the rejected receipt's `record_type`. Never carries the rejected payload.                            |
| `RECEIPT_INTEGRITY_QUARANTINED`  | `FOREVER` | S6.3 §4.2   | A segment-integrity audit at read time found a chain-hash mismatch or a segment-Ed25519-signature failure; the affected segment is marked `TAMPER_QUARANTINED` for read-side surfacing. Carries segment id, first anomalous receipt id, detection method, and audit timestamp. The sealed bytes are unchanged per `INV-005`. |
| `RECEIPT_LINEAGE_CYCLE_DETECTED` | `FOREVER` | S6.3 §7.3   | A scheduled lineage audit found a cycle in the receipt DAG. Carries the cycle's receipt ids, the detection method, and the receipts whose `parent_receipt_id` participates in the cycle. The involved segment is moved to `TAMPER_QUARANTINED`.                                                                              |
| `RECEIPT_SEQUENCE_OUT_OF_ORDER`  | `FOREVER` | S6.3 §9.8   | A sequence-ordering anomaly was detected at audit time (e.g. WAL replay produced a non-monotonic sequence). Carries segment id, anomalous receipt id, expected sequence range.                                                                                                                                               |

Distribution: 4 new FOREVER record types. Cumulative narrative FOREVER entries through S6.3: 65 (post-Wave 7) + 4 (S6.3) = **69 narrative FOREVER entries**. The §20 per-`record_type` cardinality reservation in S3.1 is bumped from 205 to 209 entries narratively. Existing histogram and counter labels remain valid; subject, group, and channel ids are never labels.

## 14. Open deferrals

- **Receipt-level encryption-at-rest** — orthogonal to the redaction layer (§6). Disk-level encryption (LUKS / dm-crypt / ZFS native) covers the at-rest threat model in Rev.2 per S2.2 §10. Per-receipt encryption keyed off the owning subject's vault is deferred to a future L4.2 vault sub-spec sweep.
- **Cross-host receipt federation** — when AIOS becomes multi-host, receipts emitted on host A must be queryable from host B with chain integrity preserved. The federation protocol (signed segment exchange, cross-host TAI64N consensus, federated `VerifyChain`) is deferred to a future operational sub-spec.
- **Cryptographic notarization** — anchoring the segment seal hash to an external time-stamping authority (RFC 3161) for non-repudiation against a dishonest operator. Optional future enhancement; out of scope for this contract.
- **Receipt-level retention overrides** — extending retention for a single receipt above its `RecordType` default. Currently retention is fixed by record type in S3.1 §13; per-receipt extension is deferred to an operator-authoring sub-spec.
- **Lineage-bounded query primitives** — query operators that return "all receipts within N hops of receipt X" or "the longest chain from origin to leaf containing receipt X". S2.1 has the query DSL; lineage-aware operators are deferred to a S2.1 refinement.
- **`RECEIPT_FORGERY_DETECTED` formal naming** — §9.1 references this name; in this Wave it lives narratively. Promotion to a formal closed-enum entry alongside the four §13 entries is deferred to the next S3.1 record-type consolidation sweep, when the synthetic forgery and key-subject mismatch surfaces are catalogued together with `RECEIPT_PAYLOAD_DUPLICATE_OBSERVED`, `RECEIPT_LINEAGE_DEPTH_EXCEEDED`, `RECEIPT_ORPHAN_ACTION_REF_DETECTED`, and the S2.4 properties listed in §11.

## 15. See also

- [S3.1 — Evidence Log Architecture](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S6.2 — Evidence Grades](02_evidence_grades.md)
- [S6.4 — Constitutional Invariants](04_invariants.md)
- [S5.1 — Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)
- [S5.2 — Vault Broker](../L4_Policy_Identity_Vault/02_vault_broker.md)
- [S4.1 — Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S2.4 — Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)
- [S6.1 — Status Taxonomy](01_status_taxonomy.md)
- [Rev.1 §7 — Governance and Evidence](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [L0 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

---

Status: REAL
Evidence: E1
