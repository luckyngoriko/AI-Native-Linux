# Evidence Log Architecture (Rev.2)

| Field     | Value                                                                                       |
| --------- | ------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (refined 2026-05-08; awaiting implementation evidence)                           |
| Phase tag | S3.1                                                                                        |
| Layer     | L9 Observability, Admin, Operations                                                         |
| Consumes  | events from S0.1, S1.1, S1.2, S1.3, S2.3, S2.4, S3.2, sandbox/recovery layers               |
| Produces  | append-only evidence receipts, hash chain, indexes, subscription stream; gRPC `EvidenceLog` |
| Approved  | 2026-05-08 (deltas D1–D12 applied; replaces draft from `dfa3be5`)                           |

## 1. Purpose

The Evidence Log is AIOS's operational memory of what happened. It records requests, decisions, denials, approvals, execution facts, verification results, failures, recovery events, and model routing decisions — in append-only form, with a verifiable hash chain and per-segment signatures.

This sub-spec defines the record shape with per-type schemas, the hash chain algorithm, segment format and lifecycle, tiering policy, indexes, the subscription/streaming API, the query API, retention and redaction, adversarial robustness, and the gRPC surface.

## 2. Core invariants

1. **Evidence is append-only.** Sealed segments are immutable.
2. **AI agents cannot edit or delete evidence.** Hard deny `hd.evidence_log_mutation` (S2.3 §6) protects this.
3. **Corrections are new evidence records** that reference older records. Original records are never rewritten.
4. **The hash chain is verifiable.** Every receipt's `previous_receipt_hash` ties it to the prior record; any tampering is detectable.
5. **Recovery can read evidence without the Cognitive Core.** Read paths depend only on L0, L1, L2 storage.

## 3. Receipt shape

```proto
syntax = "proto3";
package aios.evidence.v1alpha1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/struct.proto";

message EvidenceReceipt {
  string receipt_id = 1;                 // "evr_<ULID>"
  google.protobuf.Timestamp recorded_at = 2;   // server wall-clock
  RecordType record_type = 3;
  string subject = 4;                    // L4 canonical subject
  string action_id = 5;                  // S0.1 action_id; "" if not action-bound
  string policy_decision_id = 6;
  string verification_id = 7;
  string correlation_id = 8;
  string trace_id = 9;                   // W3C trace context
  string segment_id = 10;
  uint64 sequence_number = 11;           // monotonic per-segment
  string payload_hash = 12;              // hex_lower(BLAKE3(canonical(payload)))[:32]
  string payload_ref = 13;               // "segment://<segment_id>/<offset>"
  string redaction_profile = 14;
  string previous_receipt_hash = 15;     // hex_lower(BLAKE3(canonical(prev_record_proto_bytes)))
  bool simulated = 16;
  RecordPayload payload = 17;
}
```

`previous_receipt_hash` is computed over the **proto deterministic bytes** of the previous record (not the JSON projection). The genesis receipt for a segment has `previous_receipt_hash = "0000...0000"` (32 hex chars of zero).

## 4. Per-record-type payloads

```proto
enum RecordType {
  RECORD_TYPE_UNSPECIFIED          = 0;
  ACTION_RECEIVED                   = 1;
  TRANSLATION_CREATED               = 2;
  ROUTING_DECISION                  = 3;
  POLICY_DECISION                   = 4;
  APPROVAL_REQUESTED                = 5;
  APPROVAL_GRANTED                  = 6;
  APPROVAL_DENIED                   = 7;
  EXECUTION_STARTED                 = 8;
  EXECUTION_COMPLETED               = 9;
  VERIFICATION_RESULT               = 10;
  ROLLBACK_COMPLETED                = 11;
  RECOVERY_EVENT                    = 12;
  MODEL_CALL                        = 13;
  CHAIN_CHECKPOINT                  = 14;     // periodic checkpoint with rolling hash
  GC_PASS                           = 15;     // S1.3 GC evidence
  QUARANTINE_EVENT                  = 16;     // S1.3 quarantine entry/exit
  CONFLICT_EVENT                    = 17;     // S1.3 conflict opened/resolved/abandoned
  EMERGENCY_OVERRIDE_GRANT          = 18;     // S2.3 §16
  POLICY_BUNDLE_LOAD                = 19;     // S2.3 §12.4
  SEGMENT_SEALED                    = 20;     // §7.3
  CHAIN_INCONSISTENCY_DETECTED      = 21;     // §11.4
  TAMPER_DETECTED                   = 22;     // §11.5
}

message RecordPayload {
  oneof payload {
    ActionReceivedPayload          action_received       = 1;
    TranslationCreatedPayload      translation_created   = 2;
    RoutingDecisionPayload         routing_decision      = 3;
    PolicyDecisionPayload          policy_decision       = 4;
    ApprovalRequestedPayload       approval_requested    = 5;
    ApprovalGrantedPayload         approval_granted      = 6;
    ApprovalDeniedPayload          approval_denied       = 7;
    ExecutionStartedPayload        execution_started     = 8;
    ExecutionCompletedPayload      execution_completed   = 9;
    VerificationResultPayload      verification_result   = 10;
    RollbackCompletedPayload       rollback_completed    = 11;
    RecoveryEventPayload           recovery_event        = 12;
    ModelCallPayload               model_call            = 13;
    ChainCheckpointPayload         chain_checkpoint      = 14;
    GcPassPayload                  gc_pass               = 15;
    QuarantineEventPayload         quarantine_event      = 16;
    ConflictEventPayload           conflict_event        = 17;
    EmergencyOverrideGrantPayload  emergency_override    = 18;
    PolicyBundleLoadPayload        policy_bundle_load    = 19;
    SegmentSealedPayload           segment_sealed        = 20;
    ChainInconsistencyPayload      chain_inconsistency   = 21;
    TamperDetectedPayload          tamper_detected       = 22;
  }
}
```

Selected payload schemas (the rest follow the same pattern; full IDL in **Appendix A**):

```proto
message ActionReceivedPayload {
  string action_id = 1;
  string envelope_hash = 2;
  string subject = 3;
  string action = 4;                     // dotted name
  string adapter_id = 5;
  string privacy_class = 6;
}

message PolicyDecisionPayload {
  string policy_decision_id = 1;
  string action_id = 2;
  string decision = 3;                   // ALLOW/REQUIRE_APPROVAL/DENY
  string reason_code = 4;
  string bundle_version = 5;
  string enrichment_snapshot_id = 6;
  uint32 rules_consulted = 7;
}

message VerificationResultPayload {
  string verification_id = 1;
  string action_id = 2;
  string primitive_or_property = 3;
  string status = 4;                     // PASSED/FAILED/TIMEOUT/PROBE_ERROR/SKIPPED
  string reason_code = 5;
  google.protobuf.Struct observed_redacted = 6;
}

message ChainCheckpointPayload {
  string segment_id = 1;
  uint64 last_sequence_number = 2;
  string rolling_hash = 3;               // hash of all prior receipts in the segment
  google.protobuf.Timestamp checkpoint_at = 4;
}

message TamperDetectedPayload {
  string segment_id = 1;
  uint64 first_anomalous_sequence = 2;
  string expected_hash = 3;
  string observed_hash = 4;
  string detection_method = 5;           // "manual_audit" | "scheduled_audit" | "VerifyChain"
}
```

Callers may emit only payload types matching their authority (per L4 policy). For example, only the policy engine emits `POLICY_DECISION`; only adapters report `EXECUTION_*`; only the engine emits `CHAIN_*` and `TAMPER_DETECTED`.

## 5. Hash chain algorithm

### 5.1. Per-segment chain

```text
Within a segment:
  receipts[0].previous_receipt_hash = "0000...0000"
  receipts[i].previous_receipt_hash = hex_lower(BLAKE3(proto_deterministic(receipts[i-1])))[:32]
```

Truncation to 32 hex chars (128 bits) follows S0.1 §8.5 — sufficient for chain integrity, compact in storage.

### 5.2. Cross-segment linkage

Each segment includes a `SEGMENT_SEALED` receipt as its final entry:

```proto
message SegmentSealedPayload {
  string segment_id = 1;
  uint64 record_count = 2;
  string genesis_receipt_id = 3;
  string final_receipt_hash = 4;        // hash of last non-seal receipt
  string previous_segment_seal_hash = 5; // hash of prior segment's SEGMENT_SEALED record
  bytes  segment_signature = 6;          // Ed25519 signature over canonical segment
  string signing_key_id = 7;
  google.protobuf.Timestamp sealed_at = 8;
}
```

`previous_segment_seal_hash` chains segments together. The first segment's value is `"0000...0000"`.

### 5.3. Verification procedure

`VerifyChain(segment_id_range)` walks all receipts in order:

1. Confirm `previous_receipt_hash` matches recomputed BLAKE3 of prior record.
2. Confirm sequence numbers are strictly monotonic.
3. Confirm timestamps are monotonically non-decreasing.
4. Confirm segment signature verifies against operator's signing public key.
5. Confirm `previous_segment_seal_hash` matches prior segment's seal.

Any failure produces a `CHAIN_INCONSISTENCY_DETECTED` or `TAMPER_DETECTED` record (depending on the anomaly class) and the engine enters degraded mode (§11.5).

### 5.4. Why BLAKE3

Same rationale as S0.1 §8.5: faster than SHA-256, cryptographically strong, well-supported across Rust/Python/Go/TypeScript, stable spec.

## 6. Architecture overview

```text
        emit
          │
          v
      WAL append (fsync)
          │
          v
      Active segment (in-memory + WAL-backed)
          │  size threshold (default 64 MB) OR time threshold (default 1 h)
          v
      Sealed segment (immutable, signed)
          │  age > 7 days
          v
      Warm tier (hot disk, indexed)
          │  age > 90 days
          v
      Cold archive (object storage, slower retrieval)
```

## 7. Segment format and lifecycle

### 7.1. Segment identity

```text
segment_id = "seg_" + hex_lower(BLAKE3(<genesis_receipt_id> + <sealed_at_timestamp>))[:32]
```

### 7.2. On-disk layout

Aligned with S2.2 storage choices:

| Component               | Backing store                                                   |
| ----------------------- | --------------------------------------------------------------- |
| WAL                     | RocksDB column family `evidence_wal`                            |
| Active segment          | RocksDB column family `evidence_active`                         |
| Sealed segments (hot)   | RocksDB column family `evidence_sealed`                         |
| Sealed segments (warm)  | Same RocksDB; periodic compaction                               |
| Cold archive            | Operator-configured: local volume / S3-compatible / ZFS dataset |
| Indexes                 | SQLite (per S2.2)                                               |
| Lexical/full-text index | Tantivy (per S2.2)                                              |

### 7.3. Sealing rules

A segment is sealed when **either** condition is met:

| Condition      | Default value                   | Configurable? |
| -------------- | ------------------------------- | ------------- |
| Size           | 64 MB                           | Yes           |
| Age            | 1 hour                          | Yes           |
| Manual         | operator call                   | n/a           |
| Engine restart | sealed at restart with notation | n/a           |

On seal:

1. Final `SEGMENT_SEALED` receipt is appended.
2. Segment signature computed over the canonical bytes of all receipts.
3. Signature stored in segment header.
4. Segment marked immutable.
5. Active segment role moves to a fresh segment with a new `genesis_receipt_id`.

### 7.4. Tiering policy

| Tier | Default age | Storage                                 | Read latency |
| ---- | ----------- | --------------------------------------- | ------------ |
| Hot  | 0 – 7 days  | Local SSD; full RocksDB indexes         | < 10 ms      |
| Warm | 7 – 90 days | Local disk; compressed; reduced indexes | < 100 ms     |
| Cold | > 90 days   | Object storage; on-demand fetch         | seconds      |

Operator may override defaults. `RECOVERY_EVENT`, `TAMPER_DETECTED`, `EMERGENCY_OVERRIDE_GRANT` are **never** moved to cold tier (always available locally for incident response).

## 8. Indexes

Indexes are rebuildable from sealed segments. Index corruption never corrupts evidence truth.

| Index                | Source field                                 | Backing store |
| -------------------- | -------------------------------------------- | ------------- |
| `by_action_id`       | `EvidenceReceipt.action_id`                  | SQLite        |
| `by_subject`         | `EvidenceReceipt.subject`                    | SQLite        |
| `by_correlation`     | `EvidenceReceipt.correlation_id`             | SQLite        |
| `by_record_type`     | `EvidenceReceipt.record_type`                | SQLite        |
| `by_timestamp`       | `EvidenceReceipt.recorded_at`                | SQLite        |
| `by_policy_decision` | `EvidenceReceipt.policy_decision_id`         | SQLite        |
| `by_object`          | extracted from payload (object_id)           | SQLite        |
| `by_payload_text`    | text fields in payloads (excluding redacted) | Tantivy       |

Rebuild from sealed segments: linear scan, ~10 000 receipts/second on reference hardware.

## 9. Subscription and streaming

```proto
service EvidenceLog {
  rpc Subscribe(SubscribeRequest) returns (stream EvidenceReceipt);
  rpc Append(AppendRequest)       returns (EvidenceReceipt);
  rpc ReadReceipt(ReadReceiptRequest) returns (EvidenceReceipt);
  rpc Query(QueryRequest)         returns (stream EvidenceReceipt);
  rpc VerifyChain(VerifyChainRequest) returns (VerifyChainResponse);
  rpc RebuildIndex(RebuildIndexRequest) returns (RebuildIndexResponse);
  rpc GetLogInfo(google.protobuf.Empty) returns (LogInfo);
}

message SubscribeRequest {
  repeated RecordType record_types_filter = 1;       // empty = all
  string subject_filter = 2;                          // empty = all
  string correlation_id_filter = 3;                   // empty = all
  string resume_from_receipt_id = 4;                  // bookmark; empty = subscribe from now
  uint32 max_buffered = 5;                            // default 1000
}
```

### 9.1. Bookmarks

`resume_from_receipt_id` lets a consumer resume after disconnect. The engine replays from that receipt forward, then transitions to live streaming.

### 9.2. Debouncing

Per §S1.3 §9.3 conflict notification and other high-cardinality sources: when receipts of the same `record_type` and `correlation_id` arrive within 5 s, they are coalesced into a single notification with a counter (the underlying receipts are still individually persisted; only the **notification** is debounced).

### 9.3. Backpressure

Slow consumers buffer up to `max_buffered` receipts. Beyond that:

- Receipts are dropped from the consumer's stream.
- A `subscriber_dropped_event` is sent to that consumer once.
- The actual evidence log is never affected — drops are per-consumer only.

## 10. Query API

`Query(filter)` returns historic receipts as a stream. Filter parameters mirror the index fields (§8) plus a time range. Results are paginated via streaming with deterministic ordering by `(segment_id, sequence_number)`.

```proto
message QueryRequest {
  repeated RecordType record_types_filter = 1;
  string subject_filter = 2;
  string correlation_id_filter = 3;
  string action_id_filter = 4;
  google.protobuf.Timestamp from_time = 5;
  google.protobuf.Timestamp to_time = 6;
  string text_match = 7;                    // Tantivy query
  uint32 limit = 8;                         // default 1000
  string subject = 9;                       // calling subject; for privacy ceiling
}
```

Privacy ceiling applies (S2.1 §5 pattern): receipts whose payload references objects above the caller's ceiling are silently filtered with a count returned in the stream trailer.

## 11. Adversarial robustness

### 11.1. Replay protection

`sequence_number` is monotonic per segment. Strictly monotonic enforcement at append time. Replay attempts (same sequence number twice) cause `Append` to fail with `SEQUENCE_REPLAY_DETECTED` and trigger a `CHAIN_INCONSISTENCY_DETECTED` receipt.

### 11.2. Timestamp validation

`recorded_at` is **server-authoritative**. Clients may include their own timestamps in payloads (`payload.client_timestamp`), but the canonical timestamp on the receipt is the engine's wall clock.

If the engine's wall clock goes backwards across a restart (operator clock change), the engine refuses to append until `RECOVERY_EVENT` records the discrepancy.

### 11.3. Per-segment signing

Each sealed segment is signed with an Ed25519 key. The signing key is operator-managed (rotation policy in L4 vault sub-spec). Signature failure on read = tamper detection.

### 11.4. Chain inconsistency detection

A scheduled audit (default daily) and on-demand `VerifyChain` walk the chain. Inconsistencies emit `CHAIN_INCONSISTENCY_DETECTED` and the engine enters **degraded mode**:

- New appends paused until investigation.
- Reads continue (truth is recoverable from sealed signed segments).
- Operator alert via L9 telemetry.

### 11.5. Tamper response

If `TAMPER_DETECTED`:

- Engine enters degraded mode immediately.
- Operator alert with high priority.
- The tamper event itself is recorded as a fresh receipt in a new segment (the corrupted segment is preserved as evidence of the tampering).
- Recovery procedure documented in operator runbooks (out of scope).

## 12. Compaction

Compaction **may**:

- Build summary records (e.g. roll up sequences of low-information `MODEL_CALL` events into hourly summaries).
- Move payload bytes to cold tier; metadata stays in indexes.
- Rebuild indexes.

Compaction **must not**:

- Delete receipt identity (`receipt_id`).
- Rewrite past decisions or results.
- Remove denials or failures.
- Break the hash chain.
- Modify sealed segments.

Each compaction pass emits a `CHAIN_CHECKPOINT` receipt with the rolling hash of affected receipts so audits can verify no rewrites occurred.

## 13. Retention policy

| Record type                                          | Default retention |
| ---------------------------------------------------- | ----------------- |
| `POLICY_DECISION` (DENY / REQUIRE_APPROVAL outcomes) | Forever           |
| `EXECUTION_COMPLETED` (failures)                     | Forever           |
| `EMERGENCY_OVERRIDE_GRANT`                           | Forever           |
| `TAMPER_DETECTED` / `CHAIN_INCONSISTENCY_DETECTED`   | Forever           |
| `RECOVERY_EVENT`                                     | Forever           |
| `EXECUTION_COMPLETED` (success)                      | 365 days          |
| `VERIFICATION_RESULT`                                | 180 days          |
| `MODEL_CALL`                                         | 90 days           |
| `ROUTING_DECISION` / `TRANSLATION_CREATED`           | 90 days           |
| `GC_PASS`                                            | 90 days           |
| `CHAIN_CHECKPOINT` / `SEGMENT_SEALED`                | Forever           |
| `CONFLICT_EVENT`                                     | 365 days          |
| `QUARANTINE_EVENT`                                   | 365 days          |
| `POLICY_BUNDLE_LOAD`                                 | Forever           |

Operator may extend retention. Shortening below default requires policy approval.

Retention enforcement: when a receipt's retention horizon passes AND it has been migrated to cold tier AND no audit references it, the engine may issue a `GC_PASS` for evidence (separate from chunk GC in S1.3) — but the receipt's `receipt_id` and `previous_receipt_hash` linkage are retained as a tombstone forever to keep the chain intact.

## 14. Redaction

Stored payloads are redacted before persistence. Engine applies redaction profiles:

| Profile         | Redacts                                                                                                                   |
| --------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `default`       | secret-shaped substrings (per S1.1 §17.2.6); raw key material; passwords; tokens; full prompt bodies that contain secrets |
| `strict`        | `default` + identifiable user content; PII heuristics                                                                     |
| `debug_capture` | minimal redaction; only secrets; **only enabled by explicit policy decision**                                             |

Never store:

- Raw secret values.
- Private keys.
- Tokens.
- Passwords.
- Full prompt bodies containing secrets.

Debug capture is a policy-controlled mode, not default behavior. Activation emits a `POLICY_BUNDLE_LOAD` (or override grant) receipt.

## 15. Performance contract

| Path                                    | p95                        | Hard timeout |
| --------------------------------------- | -------------------------- | ------------ |
| `Append` (single)                       | < 5 ms                     | 100 ms       |
| `Append` (batch of 100)                 | < 50 ms                    | 1 s          |
| `ReadReceipt` (hot)                     | < 10 ms                    | 100 ms       |
| `ReadReceipt` (warm)                    | < 100 ms                   | 1 s          |
| `ReadReceipt` (cold)                    | seconds                    | 30 s         |
| `Query` (indexed)                       | < 200 ms (per 1k receipts) | 30 s         |
| `Subscribe` first event after subscribe | < 100 ms                   | n/a          |
| `VerifyChain` (one segment)             | < 2 s                      | 30 s         |
| `VerifyChain` (90-day range)            | minutes                    | 1 h          |
| Engine cold start                       | < 1 s                      | n/a          |

Append throughput target: ≥ 5 000 receipts/second on reference hardware (S2.2 §10.2).

## 16. Recovery mode behavior

Under recovery mode (L1 / S2.3 §16):

- Read is unaffected.
- Append continues; `RECOVERY_EVENT` receipts mark mode entry/exit.
- Chain auditing runs more frequently (every 10 minutes).
- Cold tier reads may be operator-disabled to keep recovery local.

The Cognitive Core is **not** required to read evidence in recovery mode. The CLI inspector reads RocksDB segments directly.

## 17. gRPC service surface

Already shown in §9. Append authority is policy-gated: only authorized subjects may call `Append` for a given `RecordType` (per L4 policy). For example, only the policy engine may emit `POLICY_DECISION`; only adapters may emit `EXECUTION_*`; only the engine itself emits `CHAIN_*`, `TAMPER_DETECTED`, `SEGMENT_SEALED`.

## 18. Acceptance criteria

- Every action has evidence from receipt to terminal phase.
- Denials and failures are logged forever.
- The hash chain is verifiable end-to-end via `VerifyChain`.
- Indexes can be rebuilt from sealed segments.
- Secret redaction is default; debug capture requires explicit policy.
- Recovery mode reads evidence without the Cognitive Core.
- Per-segment signatures verify against operator key.
- Tamper detection produces specific evidence records.
- All golden fixtures from §19 pass.
- Telemetry metrics from §20 are emitted with bounded label cardinality.

## 19. Golden fixtures

### 19.1. Append + chain linkage

```yaml
fixture_id: ev.fix.append_chain.v1
scenario:
  - Append receipt R1 (genesis of segment seg_X)
  - Append receipt R2
  - Append receipt R3
expected:
  R1.previous_receipt_hash: "0000...0000"
  R2.previous_receipt_hash: hex_lower(BLAKE3(canonical(R1)))[:32]
  R3.previous_receipt_hash: hex_lower(BLAKE3(canonical(R2)))[:32]
  sequence_numbers: 1, 2, 3 (strictly monotonic)
```

### 19.2. Segment seal + signature

```yaml
fixture_id: ev.fix.segment_seal.v1
scenario:
  - Active segment reaches 64 MB
expected: SEGMENT_SEALED receipt appended
  segment.signature verifies against operator public key
  active segment_id changes
  prior segment marked immutable
```

### 19.3. VerifyChain detects tampering

```yaml
fixture_id: ev.fix.tamper_detected.v1
scenario:
  - Sealed segment seg_X has 100 receipts
  - Operator manually edits byte in receipt 50 (simulated tamper)
  - VerifyChain(seg_X) called
expected:
  result.consistent: false
  TAMPER_DETECTED receipt emitted
  engine_state: DEGRADED
  pause_appends: true
```

### 19.4. Subscription resume from bookmark

```yaml
fixture_id: ev.fix.subscribe_resume.v1
scenario:
  - Subscriber connects, gets receipts R1-R10
  - Subscriber disconnects after R7
  - Subscriber reconnects with resume_from_receipt_id=R7
expected: receives R8, R9, R10 (replay)
  then live stream
```

### 19.5. Backpressure drops per consumer only

```yaml
fixture_id: ev.fix.backpressure_isolated.v1
scenario:
  - Slow subscriber buffers fill
  - Fast subscriber on same engine
expected:
  slow subscriber: drops + subscriber_dropped_event
  fast subscriber: unaffected
  evidence log: no data loss
```

### 19.6. Replay protection

```yaml
fixture_id: ev.fix.replay_blocked.v1
scenario:
  - Append R1 with sequence 100 to segment seg_X
  - Append R2 also with sequence 100
expected: R2 rejected with SEQUENCE_REPLAY_DETECTED
  CHAIN_INCONSISTENCY_DETECTED receipt emitted
  no further appends until investigation
```

### 19.7. Retention forever for denials

```yaml
fixture_id: ev.fix.retention_denial_forever.v1
scenario:
  - POLICY_DECISION with decision=DENY recorded 5 years ago
expected: receipt still readable
  not migrated to cold beyond minimal threshold
  no GC_PASS targeting this receipt
```

### 19.8. Recovery-mode read without LLM

```yaml
fixture_id: ev.fix.recovery_read.v1
scenario:
  - Cognitive Core stopped
  - CLI inspector calls ReadReceipt(receipt_id) for a hot-tier receipt
expected:
  result: success
  no LLM invoked
  RECOVERY_EVENT not emitted (read is non-mutating)
```

### 19.9. Compaction preserves identities

```yaml
fixture_id: ev.fix.compaction_preserves.v1
scenario:
  - Compaction pass on hot tier
  - Some MODEL_CALL receipts rolled up into hourly summary
expected: original receipt_ids preserved as tombstones
  CHAIN_CHECKPOINT receipt emitted with rolling hash
  no rewrite of past records
  hash chain still verifies
```

## 20. Telemetry contract

| Metric                               | Type      | Labels                   |
| ------------------------------------ | --------- | ------------------------ |
| `evidence_appends_total`             | counter   | `record_type`, `outcome` |
| `evidence_append_latency_seconds`    | histogram | `record_type`            |
| `evidence_segments_sealed_total`     | counter   | `seal_reason`            |
| `evidence_chain_verifications_total` | counter   | `outcome`                |
| `evidence_tamper_detected_total`     | counter   |                          |
| `evidence_subscribers_active`        | gauge     |                          |
| `evidence_subscriber_drops_total`    | counter   |                          |
| `evidence_query_latency_seconds`     | histogram | `tier`                   |
| `evidence_storage_bytes`             | gauge     | `tier`                   |
| `evidence_records_in_segment`        | histogram |                          |
| `evidence_redaction_applied_total`   | counter   | `profile`                |

Cardinality bounds: `record_type` = 22, `outcome` ≤ 5, `seal_reason` ≤ 4, `tier` = 3, `profile` = 3. Subject is **never** a metric label.

## 21. Cross-spec dependencies

| Spec                          | Relationship                                                                       |
| ----------------------------- | ---------------------------------------------------------------------------------- |
| **S0.1** Action Envelope      | `action_id`, `correlation_id`, `trace_id` flow into receipts.                      |
| **S1.1/S1.2** Cognitive Core  | Translation and routing decisions emit `TRANSLATION_CREATED` / `ROUTING_DECISION`. |
| **S1.3** Object Model         | GC, quarantine, conflict events are evidence record types.                         |
| **S2.1** Query DSL            | Evidence query patterns mirror the query DSL grammar.                              |
| **S2.2** Implementation Space | Storage backend (RocksDB, SQLite, Tantivy) shared.                                 |
| **S2.3** Policy Kernel        | Hard deny `hd.evidence_log_mutation` protects this log.                            |
| **S2.4** Verification Grammar | Property checks read sealed segments; results emit `VERIFICATION_RESULT`.          |
| **S3.2** Sandbox Composition  | Verification engine sandbox profile.                                               |
| **L1 Recovery**               | Recovery inspector reads evidence offline.                                         |
| **L4 Vault**                  | Operator signing key for segment signatures.                                       |

## 22. Open deferrals

- Cross-instance evidence log replication / federation → future operational sub-spec.
- Cryptographic notarization (anchoring chain hash to external time-stamping authority) → optional future enhancement.
- Operator runbook for tamper response → operator documentation, not a contract.
- Encrypted-at-rest evidence (separate from disk encryption) → L4 vault sub-spec.
- Streaming compaction → future revision; rev.2 uses periodic batch compaction.

## 23. Namespace integration (S4.1 cross-spec touch-up)

Applied 2026-05-09. Source: [S4.1 §12.6](../L2_AIOS_FS/05_namespace_layout.md).

### 23.1 Namespace scope on every record

`EvidenceRecord` gains an optional `NamespaceScope` field:

```proto
message NamespaceScope {
  aios.namespace.v1alpha1.ScopeKind scope = 1;
  string group_id = 2;       // empty for SYSTEM scope
  string user_id = 3;        // empty for SYSTEM and GROUP scopes
}

// EvidenceRecord adds:
//   optional NamespaceScope namespace_scope = N;
```

Population rules:

- For records derived from an action envelope, `namespace_scope` mirrors the envelope's `target.scope`/`target.group_id`/`target.user_id` (S0.1 §13.1).
- For system-internal records (segment seal, chain checkpoint, capability catalog load), `namespace_scope` is set to `{scope = SYSTEM}` with empty ids.
- For policy bundle load and recovery events, `namespace_scope = {scope = SYSTEM}`.

### 23.2 Privacy ceiling extends to namespace scope

The Query API privacy ceiling (§10) is extended: a subject with `primary_group_id = A` cannot retrieve records with `namespace_scope.group_id = B` unless the subject is in the `_system` scope under recovery mode with `system_audit_read` capability and a human approver. Excluded records are silently filtered with a `suppressed_count` field (consistent with S2.1 cross-group filtering).

### 23.3 Two new record types

Added to the closed `RecordType` vocabulary:

| Record type                 | Retention class | When emitted                                                                                                                                                            |
| --------------------------- | --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SYSTEM_ADMIN_OPERATION`    | `STANDARD_24M`  | Any mutation of `/aios/system/apps/` or `/aios/system/agents/` by a human-bound `system_admin` capability holder. Carries the action_id and the affected reserved name. |
| `CROSS_GROUP_ACCESS_DENIED` | `STANDARD_24M`  | Whenever the S2.3 `CrossGroupAccessForbidden` hard-deny fires. Carries source `group_id`, target `group_id`, and the action_id whose target was denied.                 |

Total record types now 24 (up from 22). The discriminated `RecordPayload` oneof gains two corresponding payload messages.

### 23.4 Telemetry additions

Two counters added with bounded labels:

| Metric                              | Type    | Labels (closed)               |
| ----------------------------------- | ------- | ----------------------------- |
| `evidence_namespace_scope_total`    | counter | `scope` (system/group/user)   |
| `evidence_cross_group_filter_total` | counter | none (cumulative suppression) |

## 24. Wave 5 cross-spec touch-up (S7.1+S7.2+S7.3+S7.4+S7.5+S8.2 + L0 INV-019..022 consolidation)

Applied 2026-05-10. Sources: [S7.1 §11](../L7_Interaction_Renderers/01_surface_composition.md), [S7.2 §10](../L7_Interaction_Renderers/02_shared_ui_schema.md), [S7.3 §9](../L7_Interaction_Renderers/03_visual_language.md), [S7.4 §10](../L7_Interaction_Renderers/04_kde_renderer.md), [S7.5 §11](../L7_Interaction_Renderers/05_web_renderer.md), [S8.2 §10](../L8_Network_Hardware_Devices/05_gpu_resource_model.md). This section consolidates the evidence record types required to observe the renderer, theme, and GPU subsystems and to enforce L0 INV-019..022. After this addition the **`RecordType` vocabulary now totals 87 entries** (29 prior + 58 Wave 5). Most additions are observable lifecycle events at `STANDARD_24M` retention; a smaller set of constitutional and forensic events use `EXTENDED_60M` or `FOREVER` retention.

### 24.1 Fifty-eight new record types

Added to the closed `RecordType` vocabulary. The `RecordPayload` discriminated oneof gains a corresponding payload message per record type; payload schemas follow the existing pattern (action_id back-reference, surface_id / theme_id / binding_id where applicable, decision-relevant subset of S0.1 enrichment, redacted-by-default observed structure).

#### 24.1.1 From S7.1 Surface Composition (7 types)

| Record type                      | Retention class | When emitted                                                                                         |
| -------------------------------- | --------------- | ---------------------------------------------------------------------------------------------------- |
| `SURFACE_CREATED`                | `STANDARD_24M`  | A new Surface enters the registry (any kind).                                                        |
| `SURFACE_DESTROYED`              | `STANDARD_24M`  | A Surface leaves the registry (clean teardown or eviction).                                          |
| `SURFACE_GPU_BUDGET_EXCEEDED`    | `EXTENDED_60M`  | A Surface tried to exceed its declared GPU budget; carries observed vs allowed.                      |
| `CROSS_SURFACE_READ_DENIED`      | `FOREVER`       | A Surface attempted to read another Surface's framebuffer / IPC channel; constitutional barrier.     |
| `CROSS_ZONE_VIOLATION_ATTEMPTED` | `EXTENDED_60M`  | A Surface tried to render into a zone other than its bound CompositionZone.                          |
| `RECOVERY_KIND_REJECTED`         | `FOREVER`       | A non-recovery surface tried to load while the system is in recovery_mode = true.                    |
| `SURFACE_NEVER_RENDERED`         | `STANDARD_24M`  | A trust-bearing surface was created but never reached the chrome zone within its lifecycle deadline. |

#### 24.1.2 From S7.2 Shared UI Schema (3 types)

| Record type                           | Retention class | When emitted                                                                              |
| ------------------------------------- | --------------- | ----------------------------------------------------------------------------------------- |
| `UI_TREE_VALIDATION_REJECTED`         | `STANDARD_24M`  | A UI tree failed schema validation (closed NodeKind, depth, kind / parent rules).         |
| `UI_TRUST_BEARING_AUTHORSHIP_REFUSED` | `FOREVER`       | An AI subject attempted to author a node carrying constitutional trust authorship.        |
| `UI_RECOVERY_NODE_DROPPED`            | `STANDARD_24M`  | A renderer dropped a recovery-only NodeKind because recovery_mode = false at render time. |

#### 24.1.3 From S7.3 Visual Language (4 types)

| Record type                | Retention class | When emitted                                                                            |
| -------------------------- | --------------- | --------------------------------------------------------------------------------------- |
| `THEME_LOADED`             | `STANDARD_24M`  | A theme was activated by a subject; carries `theme_id`, `theme_kind`, `subject.is_ai`.  |
| `THEME_REJECTED`           | `EXTENDED_60M`  | A theme load failed validation (signature, schema, or constitutional invariant).        |
| `THEME_SWITCHED`           | `STANDARD_24M`  | The active theme changed; carries previous and new `theme_id`.                          |
| `THEME_INVARIANT_VIOLATED` | `FOREVER`       | A loaded theme failed a scheduled invariant audit (canonical icon hash mismatch, etc.). |

#### 24.1.4 From S7.4 KDE Renderer (11 types)

| Record type                              | Retention class | When emitted                                                                            |
| ---------------------------------------- | --------------- | --------------------------------------------------------------------------------------- |
| `KDE_RENDERER_STARTED`                   | `STANDARD_24M`  | Plasma renderer process started.                                                        |
| `KDE_RENDERER_DEGRADED`                  | `FOREVER`       | Renderer entered a degraded mode (composition fallback, software rasterization).        |
| `KDE_FRAME_DROPPED`                      | `STANDARD_24M`  | A frame was dropped beyond the budget threshold.                                        |
| `KDE_LAYER_SHELL_REJECTED`               | `FOREVER`       | A layer-shell client requested a chrome / overlay layer it is not authorised to occupy. |
| `KDE_KWIN_SCRIPT_LOADED`                 | `STANDARD_24M`  | A kwin script was activated (always evidence-emitting per S7.4).                        |
| `KDE_KWIN_SCRIPT_REJECTED`               | `FOREVER`       | A kwin script load failed signature, manifest, or invariant check.                      |
| `KDE_RECOVERY_SHELL_STARTED`             | `FOREVER`       | The recovery KDE shell was started; constitutional boundary event.                      |
| `KDE_RECOVERY_KIND_REJECTED_AT_RENDERER` | `FOREVER`       | The renderer refused to load a non-recovery surface while recovery_mode = true.         |
| `KDE_PLASMA_THEME_OVERRIDDEN`            | `STANDARD_24M`  | An end-user theme override took effect; subject `is_ai` is recorded.                    |
| `KDE_RENDER_FAILED`                      | `EXTENDED_60M`  | Frame composition failed and produced a visible error surface.                          |
| `KDE_TOKEN_FALLBACK_USED`                | `STANDARD_24M`  | A required design token was missing and a documented fallback path was used.            |

#### 24.1.5 From S7.5 Web Renderer (17 types)

| Record type                                     | Retention class | When emitted                                                                                  |
| ----------------------------------------------- | --------------- | --------------------------------------------------------------------------------------------- |
| `WEB_LAN_EXPOSURE_GRANTED`                      | `FOREVER`       | LAN exposure was approved by policy; carries action_id and approver chain.                    |
| `WEB_PUBLIC_EXPOSURE_GRANTED`                   | `FOREVER`       | Public-internet exposure was approved by policy.                                              |
| `WEB_RECOVERY_KIND_REJECTED`                    | `FOREVER`       | The Web renderer refused to load a non-recovery page while recovery_mode = true.              |
| `WEB_PUBLIC_EXPOSURE_FIREWALL_RECORDED`         | `FOREVER`       | The firewall rule corresponding to a public exposure grant was committed; carries rule hash.  |
| `WEB_RECOVERY_PAGE_LOADED`                      | `EXTENDED_60M`  | A recovery-page surface was loaded.                                                           |
| `WEB_RECOVERY_PAGE_EXITED`                      | `EXTENDED_60M`  | A recovery-page surface unloaded; pairs with `WEB_RECOVERY_PAGE_LOADED`.                      |
| `WEB_RENDERER_STARTED`                          | `STANDARD_24M`  | The Web renderer process started.                                                             |
| `WEB_RENDERER_DEGRADED`                         | `STANDARD_24M`  | The Web renderer entered a degraded mode (no GPU, no service worker, etc.).                   |
| `WEB_LAN_EXPOSURE_ACTIVE`                       | `STANDARD_24M`  | Periodic heartbeat while LAN exposure is active.                                              |
| `WEB_EXPOSURE_REVOKED`                          | `STANDARD_24M`  | LAN or public exposure was revoked (policy or operator action).                               |
| `WEB_EXTENSION_INTERFERENCE`                    | `STANDARD_24M`  | A browser extension attempted to mutate AIOS chrome subtree; rejected by isolated mount.      |
| `WEB_FULLSCREEN_REQUESTED`                      | `STANDARD_24M`  | Fullscreen API was requested; carries surface_id and grant decision.                          |
| `WEB_THEME_INJECTION_BLOCKED`                   | `STANDARD_24M`  | A non-system stylesheet attempted to override constitutional theme tokens.                    |
| `WEB_THEME_FALLBACK_USED`                       | `STANDARD_24M`  | The renderer used a documented theme fallback (missing token, network failure).               |
| `WEB_CLIENT_STORAGE_QUOTA_BREACH`               | `STANDARD_24M`  | Client storage attempted to exceed its declared quota.                                        |
| `WEB_RENDERER_CLS_BREACH`                       | `STANDARD_24M`  | Cumulative Layout Shift exceeded the declared budget for a chrome surface.                    |
| `WEB_CONSTITUTIONAL_ELEMENT_REREGISTER_BLOCKED` | `STANDARD_24M`  | A custom-element re-registration attempted to overwrite an AIOS-owned constitutional element. |

#### 24.1.6 From S8.2 GPU Resource Model (16 types)

| Record type                        | Retention class | When emitted                                                                                        |
| ---------------------------------- | --------------- | --------------------------------------------------------------------------------------------------- |
| `GPU_DEVICE_ENUMERATED`            | `STANDARD_24M`  | A GPU device entered the resource model graph.                                                      |
| `GPU_DEVICE_DISCONNECTED`          | `STANDARD_24M`  | A GPU device left the graph (clean removal).                                                        |
| `GPU_VK_DEVICE_CREATED`            | `STANDARD_24M`  | A Vulkan logical device was created for an authorised subject.                                      |
| `GPU_VK_DEVICE_DESTROYED`          | `STANDARD_24M`  | A Vulkan logical device was destroyed.                                                              |
| `GPU_DMABUF_GRANTED`               | `STANDARD_24M`  | A dmabuf descriptor was issued to a peer subject under explicit policy decision.                    |
| `GPU_DMABUF_DENIED`                | `STANDARD_24M`  | A dmabuf grant request was denied.                                                                  |
| `GPU_CAPABILITY_DENIED`            | `STANDARD_24M`  | A GPU capability request was denied (missing capability, class mismatch, budget exceeded).          |
| `GPU_VALIDATION_DISABLED_RECOVERY` | `STANDARD_24M`  | Validation layers were disabled for the recovery mode boot path.                                    |
| `GPU_VALIDATION_ENABLED_NORMAL`    | `STANDARD_24M`  | Validation layers were enabled for normal mode (default).                                           |
| `DRIVER_UNAVAILABLE`               | `STANDARD_24M`  | A required GPU driver was unavailable; fallback path entered.                                       |
| `GPU_BUDGET_EXCEEDED`              | `EXTENDED_60M`  | A subject exceeded its declared GPU budget; observed vs allowed recorded.                           |
| `GPU_BUDGET_DOWNGRADED`            | `EXTENDED_60M`  | A subject's GPU budget was downgraded (e.g., contention with higher-priority queue).                |
| `IOMMU_UNAVAILABLE_DEGRADED`       | `EXTENDED_60M`  | IOMMU isolation was unavailable; the system entered degraded GPU mode.                              |
| `HOST_CAPABILITY_LIE`              | `FOREVER`       | A guest / sandbox claimed a GPU capability the host does not actually expose; constitutional fault. |
| `GPU_BINDING_FORGERY`              | `FOREVER`       | A binding-id appeared without a matching grant in the binding catalog; tamper indicator.            |
| `GPU_DEVICE_FORCE_RECLAIMED`       | `FOREVER`       | A GPU device was force-reclaimed (out-of-policy holdout, hung job, recovery boundary).              |

### 24.2 Retention class summary

Retention class distribution for the 58 additions:

| Retention class | Count | Notes                                                                          |
| --------------- | ----: | ------------------------------------------------------------------------------ |
| `STANDARD_24M`  |    37 | Lifecycle and observable-state events.                                         |
| `EXTENDED_60M`  |     7 | Budget breaches, degradation, theme rejection — operational signals.           |
| `FOREVER`       |    14 | Constitutional / forensic events: cross-surface read, exposure grants, tamper. |

### 24.3 Append authority

Append authority follows the existing §17 discipline and is policy-gated per record type:

- `SURFACE_*`, `KDE_*`, `WEB_*`, `UI_*`, `THEME_*` → only the corresponding renderer / surface registry process.
- `GPU_*`, `DRIVER_UNAVAILABLE`, `IOMMU_UNAVAILABLE_DEGRADED` → only the GPU resource manager.
- `HOST_CAPABILITY_LIE`, `GPU_BINDING_FORGERY`, `GPU_DEVICE_FORCE_RECLAIMED` → only the GPU resource manager and the engine itself (audit pathway).

Forgery from any other subject is hard-denied at the engine surface and emits a `TAMPER_DETECTED` record per §11.

### 24.4 Telemetry note

Per-`record_type` cardinality bound updates from 22 to 87. Existing histogram and counter labels remain valid; the §20 cardinality reservation is bumped accordingly.

## 25. Wave 6 cross-spec touch-up (S5.2+S5.3+S5.4+S9.1+S10.1+S8.1 record-type consolidation)

Applied 2026-05-11. Sources: [S5.2 §14](../L4_Policy_Identity_Vault/02_vault_broker.md), [S5.3 §10](../L4_Policy_Identity_Vault/04_approval_mechanics.md), [S5.4 §13](../L4_Policy_Identity_Vault/05_emergency_override.md), [S9.1 §12](../L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md), [S10.1 §13](../L3_AIOS_SGR_Service_Graph_Runtime/03_capability_runtime_grpc.md), [S8.1 §10](../L8_Network_Hardware_Devices/02_network_policy.md). This section adds the closed `RecordType` narrative entries required to record evidence for these six sub-specs through the L9.1 Evidence Log. Each row binds a record name to its retention class (closed enum from §6.4: `STANDARD_24M` / `EXTENDED_60M` / `FOREVER`) and to the source spec that owns its emission contract. Following the §23 / §24 narrative-only declaration pattern, this addition does **not** modify Appendix A's proto IDL block; full IDL reconciliation is deferred to a subsequent refinement sweep. After this addition the **`RecordType` vocabulary now totals 159 entries narratively** (87 prior + 72 Wave 6 unique additions; one reconciliation collapses an S5.3 binding-lifecycle name into an S10.1 runtime-detector name — see §25.7).

### 25.1 From S5.2 Vault Broker (8 types)

Source: [S5.2 §14](../L4_Policy_Identity_Vault/02_vault_broker.md). Append authority is restricted to the Vault Broker process; forgery from any other subject is hard-denied at the engine surface per §11.

| RecordType                        | Retention      | Source spec | Purpose                                                                                                         |
| --------------------------------- | -------------- | ----------- | --------------------------------------------------------------------------------------------------------------- |
| `VAULT_CAPABILITY_ISSUED`         | `STANDARD_24M` | S5.2 §14    | Issuance of a vault capability binding (DRAFT → ACTIVE).                                                        |
| `VAULT_CAPABILITY_ROTATED`        | `STANDARD_24M` | S5.2 §14    | Rotation event; underlying material changes while the binding remains usable.                                   |
| `VAULT_CAPABILITY_REVOKED`        | `EXTENDED_60M` | S5.2 §14    | Explicit revocation of a binding (operator action, bundle rollover, material loss).                             |
| `VAULT_OPERATION`                 | `STANDARD_24M` | S5.2 §14    | Every Sign / Verify / Encrypt / Decrypt / MAC / Random call. Redacted projection — no payload, no key material. |
| `VAULT_RAW_REVEAL`                | `FOREVER`      | S5.2 §14    | The human-only `RevealSecret` escape hatch (recovery + STRONG session + co-signer + one-shot capability).       |
| `VAULT_CAPABILITY_FORGERY`        | `FOREVER`      | S5.2 §14    | Ed25519 signature failure on a presented capability; constitutional tamper indicator.                           |
| `SUBJECT_KIND_REJECTED_FOR_VAULT` | `FOREVER`      | S5.2 §14    | AI subject hard-denied from `SECRET_GET` at request entry per S5.2 invariant I1.                                |
| `VAULT_RECOVERY_SNAPSHOT_LOADED`  | `FOREVER`      | S5.2 §14    | Recovery-mode broker startup with master-key unlock (S5.2 §10.1).                                               |

### 25.2 From S5.3 Approval Mechanics (9 types, LONG retention floor)

Source: [S5.3 §10](../L4_Policy_Identity_Vault/04_approval_mechanics.md). S5.3 §10.3 sets a `LONG` retention floor (≥ `STANDARD_24M`); a policy bundle MAY upgrade specific records to `FOREVER` for destructive actions on financial-tier groups but MUST NOT downgrade. Default mappings below honour the floor at `STANDARD_24M` for benign lifecycle transitions and at `EXTENDED_60M` for denials and revocations that retain operational signal value. Append authority is restricted to the Approval Manager service.

| RecordType                 | Retention      | Source spec | Purpose                                                                                                                                                       |
| -------------------------- | -------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `APPROVAL_REQUESTED`       | `STANDARD_24M` | S5.3 §10    | Policy Kernel emitted `request_approval`; `ApprovalRequest` created (`created → DRAFT`).                                                                      |
| `APPROVAL_DELIVERED`       | `STANDARD_24M` | S5.3 §10    | Surface delivered to a channel (`DRAFT → AWAITING_OPERATOR`).                                                                                                 |
| `APPROVAL_GRANTED`         | `STANDARD_24M` | S5.3 §10    | Operator granted; binding active (`AWAITING_OPERATOR → GRANTED`).                                                                                             |
| `APPROVAL_DENIED`          | `EXTENDED_60M` | S5.3 §10    | Operator denied or scope/TTL drift (`AWAITING_OPERATOR → DENIED`).                                                                                            |
| `APPROVAL_EXPIRED`         | `STANDARD_24M` | S5.3 §10    | TTL elapsed before operator response (`AWAITING_OPERATOR → EXPIRED`).                                                                                         |
| `APPROVAL_CONSUMED`        | `STANDARD_24M` | S5.3 §10    | Binding spent on the bound action (`GRANTED → CONSUMED`); terminal success.                                                                                   |
| `APPROVAL_REVOKED`         | `EXTENDED_60M` | S5.3 §10    | Operator-initiated revocation of an active GRANTED binding (`GRANTED → REVOKED`).                                                                             |
| `APPROVAL_DELIVERY_FAILED` | `EXTENDED_60M` | S5.3 §10    | Surface could not be delivered to any approval channel (`DRAFT → FAILED_DELIVERY`).                                                                           |
| `APPROVAL_BINDING_VOIDED`  | `FOREVER`      | S5.3 §10    | Action canonical-hash mismatch at execute time (action revision). See §25.7 reconciliation — synonym narrative reference for `BINDING_VOIDED_ACTION_REVISED`. |

### 25.3 From S5.4 Emergency Override (8 types, all FOREVER)

Source: [S5.4 §13](../L4_Policy_Identity_Vault/05_emergency_override.md). Every transition — including denials, expirations, and post-hoc reviews — is permanent forensic evidence because emergency override is the **only** mechanism that can rescue a hard-denied action; revisability of the audit trail would defeat the constitutional premise. Append authority is restricted to the Override Manager service and the Capability Runtime (only for `OVERRIDE_CONSUMED`).

| RecordType                 | Retention | Source spec | Purpose                                                                                                                                           |
| -------------------------- | --------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `OVERRIDE_REQUESTED`       | `FOREVER` | S5.4 §13    | OverrideRequest entered `OS_REQUESTED`.                                                                                                           |
| `OVERRIDE_QUORUM_RECEIVED` | `FOREVER` | S5.4 §13    | Confirming signature arrived but quorum not yet met (`OS_AWAITING_DUAL_CONFIRM` step).                                                            |
| `OVERRIDE_GRANTED`         | `FOREVER` | S5.4 §13    | FSM transitioned to `OS_ACTIVE` and a binding was issued. Rev.2 successor of the rev.1-era `EMERGENCY_OVERRIDE_GRANT` name in §4.                 |
| `OVERRIDE_CONSUMED`        | `FOREVER` | S5.4 §13    | The Capability Runtime executed the bound action under the override.                                                                              |
| `OVERRIDE_DENIED`          | `FOREVER` | S5.4 §13    | Any of the S5.4 §3.5 `OverrideDenialReason` codes fired.                                                                                          |
| `OVERRIDE_EXPIRED`         | `FOREVER` | S5.4 §13    | TTL elapsed without consumption.                                                                                                                  |
| `OVERRIDE_REVOKED`         | `FOREVER` | S5.4 §13    | An ACTIVE binding was revoked before consumption.                                                                                                 |
| `OVERRIDE_REVIEW`          | `FOREVER` | S5.4 §13    | Post-hoc forensic review or attestation referencing one or more prior override records (the only after-the-fact augmentation; INV-005 preserved). |

### 25.4 From S9.1 Recovery Boundary (10 types, all FOREVER)

Source: [S9.1 §12](../L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md). All recovery-boundary records are FOREVER-retained, never compactable (the FOREVER retention class is exempt from compaction per §12), and replicated across all S3.1 segments so recovery activity is visible in the operational record indefinitely. Append authority is restricted to the recovery-mode supervisor and the L1 boot sequencer.

| RecordType                             | Retention | Source spec | Purpose                                                                                                                             |
| -------------------------------------- | --------- | ----------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `RECOVERY_BOOT_ENTERED`                | `FOREVER` | S9.1 §12    | Recovery mode entered; carries `RecoveryEntryReason`, kernel slot (dedicated/generic), fallback flag, identity bundle version.      |
| `RECOVERY_OPERATOR_AUTHENTICATED`      | `FOREVER` | S9.1 §12    | Operator authenticated for the recovery session; carries auth factor (HARDWARE_KEY / TOTP / PASSPHRASE) and risk flags.             |
| `RECOVERY_OPERATION_PERFORMED`         | `FOREVER` | S9.1 §12    | A recovery-time mutation occurred; carries `RecoveryMutableScope`, target path, request hash, bundle version.                       |
| `RECOVERY_TTL_EXPIRED_AUTO_REBOOT`     | `FOREVER` | S9.1 §12    | The 8-hour hard cap (S9.1 §8) elapsed; the system auto-rebooted out of recovery.                                                    |
| `RECOVERY_BOOT_EXITED`                 | `FOREVER` | S9.1 §12    | Recovery boot exited; carries `RecoveryExitReason` and session duration.                                                            |
| `RECOVERY_L5_START_BLOCKED`            | `FOREVER` | S9.1 §12    | A start attempt for any L5 service was blocked while `recovery_mode = true`; carries the `L5StartProhibitedInRecovery` reason code. |
| `RECOVERY_NETWORK_LAN_ENABLED`         | `FOREVER` | S9.1 §12    | Operator opened the LAN-for-provisioning window (`window_seconds ≤ 1800`) with justification.                                       |
| `RECOVERY_NETWORK_LAN_DISABLED`        | `FOREVER` | S9.1 §12    | The provisioning window closed (operator action or watchdog).                                                                       |
| `RECOVERY_FORENSIC_ATTACH_PERFORMED`   | `FOREVER` | S9.1 §12    | A forensic mount of another group's namespace occurred during recovery; carries attached group id, mount point, justification.      |
| `BOOT_FAILURE_AUTO_RECOVERY_TRIGGERED` | `FOREVER` | S9.1 §12    | Consecutive normal-boot failures triggered automatic entry into recovery mode.                                                      |

S9.1 §12 also names three deferred record types that are **narrative only** in this Wave 6 and not yet contract-grade: `HEAVY_AUTH_FALLBACK_USED`, `RECOVERY_SHELL_FAILED`, `BOOT_FALLBACK_TRIGGERED`. These will be queued into a future S9 refinement sweep and are not counted in the cumulative total above.

### 25.5 From S10.1 Capability Runtime gRPC (20 types)

Source: [S10.1 §13](../L3_AIOS_SGR_Service_Graph_Runtime/03_capability_runtime_grpc.md). The Capability Runtime is the **only** authorised emitter for these twenty record types; emission attempts from any other subject are hard-denied and themselves emit `TAMPER_DETECTED` per §11.5. `ACTION_POLICY_DECISION` is the runtime's mirror of L9.1 `POLICY_DECISION` — both are emitted; the runtime mirror carries the decision-against-action linkage.

| RecordType                           | Retention      | Source spec | Purpose                                                                                                                |
| ------------------------------------ | -------------- | ----------- | ---------------------------------------------------------------------------------------------------------------------- |
| `ACTION_RECEIVED`                    | `STANDARD_24M` | S10.1 §13   | `ValidateAction` accepted the envelope; `CREATED` lifecycle state recorded.                                            |
| `ACTION_VALIDATED`                   | `STANDARD_24M` | S10.1 §13   | Schema, target, sandbox, and verification grammar checks completed.                                                    |
| `ACTION_POLICY_DECISION`             | `STANDARD_24M` | S10.1 §13   | `EvaluatePolicyForAction` recorded the policy decision against the action (mirrors L9.1 `POLICY_DECISION`).            |
| `ACTION_DISPATCHED`                  | `STANDARD_24M` | S10.1 §13   | The eight-step pre-dispatch (§6.1 of S10.1) succeeded and the adapter was invoked.                                     |
| `EXECUTION_SUCCEEDED`                | `STANDARD_24M` | S10.1 §13   | Adapter returned `ADAPTER_OK` and verification returned `VERIFICATION_PASSED` for all intents.                         |
| `EXECUTION_FAILED`                   | `EXTENDED_60M` | S10.1 §13   | Lifecycle transitioned to `FAILED`; payload carries `ExecutionFailureReason` and `current_canonical_hash`.             |
| `EXECUTION_VERIFICATION_FAILED`      | `EXTENDED_60M` | S10.1 §13   | Verification returned a non-`PASSED` status; payload carries the failing intent and observed state (redacted).         |
| `ROLLBACK_ATTEMPTED`                 | `STANDARD_24M` | S10.1 §13   | `RollbackAction` invoked the adapter's rollback; payload carries `RollbackStrategy` and pre-state hash.                |
| `ROLLBACK_SUCCEEDED`                 | `STANDARD_24M` | S10.1 §13   | Rollback returned `RollbackOutcome.SUCCEEDED`; lifecycle transitioned to `ROLLED_BACK`.                                |
| `ROLLBACK_FAILED_REQUIRES_OPERATOR`  | `FOREVER`      | S10.1 §13   | Rollback returned `RollbackOutcome.FAILED`; lifecycle transitioned to `ROLLBACK_FAILED`; affected resources listed.    |
| `ADAPTER_REGISTERED`                 | `STANDARD_24M` | S10.1 §13   | Adapter manifest accepted at registration; lifecycle starts at `REGISTERED` stability.                                 |
| `ADAPTER_REGISTRATION_REJECTED`      | `FOREVER`      | S10.1 §13   | Manifest signature failed, publisher unrecognised, or expired; constitutional forensic event.                          |
| `ADAPTER_DEGRADED`                   | `STANDARD_24M` | S10.1 §13   | Adapter health transitioned to `ADAPTER_DEGRADED` (rate of timeout / panic / kind-overrun).                            |
| `ADAPTER_DEREGISTERED`               | `EXTENDED_60M` | S10.1 §13   | Adapter de-registered (manifest expired, kind-overrun voided, operator action).                                        |
| `IDEMPOTENCY_KEY_REPLAY_DETECTED`    | `EXTENDED_60M` | S10.1 §13   | Same `idempotency_key` with different `request_hash` observed at `ValidateAction`.                                     |
| `BINDING_VOIDED_ACTION_REVISED`      | `FOREVER`      | S10.1 §13   | Eight-step step 1 detected canonical-hash drift; binding voided per S5.3 §13 / S5.4 §5. **Canonical name**; see §25.7. |
| `AI_INTERACTIVE_QUEUE_DOWNGRADE`     | `STANDARD_24M` | S10.1 §13   | AI subject silently downgraded from `INTERACTIVE` to `AGENT_PROPOSAL`.                                                 |
| `DRY_RUN_SIMULATION_RECORDED`        | `STANDARD_24M` | S10.1 §13   | `SIMULATE` action terminated with a simulation transcript; segregated from production evidence stream.                 |
| `EXPERIMENTAL_ADAPTER_LIVE_DISPATCH` | `EXTENDED_60M` | S10.1 §13   | Action against an `EXPERIMENTAL` adapter dispatched live (not `DRY_RUN`); operator clearance required.                 |
| `ADAPTER_DEPRECATED_DISPATCH`        | `STANDARD_24M` | S10.1 §13   | Action against a `DEPRECATED` adapter dispatched; operational signal that the adapter should be retired.               |

### 25.6 From S8.1 Network Policy (18 types)

Source: [S8.1 §10](../L8_Network_Hardware_Devices/02_network_policy.md). Append authority is restricted to the L8 `NetworkPolicyService` process; forgery from any other subject is hard-denied at the engine surface and emits `TAMPER_DETECTED` per §11. Retention class distribution for the 18 additions: `FOREVER` × 7, `EXTENDED_60M` × 5, `STANDARD_24M` × 6.

| RecordType                              | Retention      | Source spec | Purpose                                                                                           |
| --------------------------------------- | -------------- | ----------- | ------------------------------------------------------------------------------------------------- |
| `NETWORK_POSTURE_CHANGED`               | `FOREVER`      | S8.1 §10    | Host `NetworkPosture` changes; carries `from`, `to`, setter subject, action_id.                   |
| `EXPOSURE_REQUESTED`                    | `STANDARD_24M` | S8.1 §10    | `RequestExposure` accepted into `AWAITING_OPERATOR`.                                              |
| `EXPOSURE_GRANTED`                      | `FOREVER`      | S8.1 §10    | LAN or PUBLIC `ExposureGrant` reaches `ACTIVE`; carries class, CIDR allow-list, approver chain.   |
| `EXPOSURE_DENIED`                       | `EXTENDED_60M` | S8.1 §10    | `RequestExposure` denied by policy or invariant.                                                  |
| `EXPOSURE_REVOKED`                      | `EXTENDED_60M` | S8.1 §10    | `RevokeExposure` succeeded; carries elapsed teardown time.                                        |
| `EXPOSURE_TERMINATED_TTL_EXPIRED`       | `EXTENDED_60M` | S8.1 §10    | `ExposureGrant` reached `expires_at` while `ACTIVE` and was auto-terminated.                      |
| `PUBLIC_EXPOSURE_HEARTBEAT`             | `STANDARD_24M` | S8.1 §10    | 5-minute heartbeat while `class = PUBLIC` and state `ACTIVE`.                                     |
| `OUTBOUND_GRANT_ISSUED`                 | `STANDARD_24M` | S8.1 §10    | `GrantOutbound` succeeded; carries directive and manifest hash.                                   |
| `OUTBOUND_GRANT_REVOKED`                | `EXTENDED_60M` | S8.1 §10    | `RevokeOutbound` or auto-revoke after manifest breach.                                            |
| `OUTBOUND_OUTSIDE_MANIFEST`             | `FOREVER`      | S8.1 §10    | A subject's connection attempt was outside its declared outbound manifest.                        |
| `OUTBOUND_DEGRADED_TO_LOOPBACK_AUTO`    | `FOREVER`      | S8.1 §10    | A subject's `OutboundDirective` was auto-degraded to loopback after repeated breaches.            |
| `ALLOWLIST_FQDN_FANOUT_EXCEEDED`        | `EXTENDED_60M` | S8.1 §10    | A `HOST_FQDN` resolved to > 16 IPs at evaluation time.                                            |
| `LAN_SUBNET_DRIFT_DETECTED`             | `STANDARD_24M` | S8.1 §10    | A `LAN_SUBNET`-pinned grant's CIDR drifted; grant transitioned to `AWAITING_OPERATOR`.            |
| `LAN_PEER_DRIFT_DETECTED`               | `EXTENDED_60M` | S8.1 §10    | A pinned `(MAC, IP)` peer's MAC changed; possible ARP spoofing.                                   |
| `AI_DIRECT_INTERNET_DENIED`             | `FOREVER`      | S8.1 §10    | An AI subject attempted a direct external connection without vault broker mediation.              |
| `EXTERNAL_MODEL_CALL_BROKERED`          | `STANDARD_24M` | S8.1 §10    | A vault-brokered external model call succeeded; carries provider, action_id, vault capability id. |
| `BACKEND_DEGRADED_NFTABLES_TO_IPTABLES` | `FOREVER`      | S8.1 §10    | nftables unavailable; iptables fallback chosen.                                                   |
| `RAW_SOCKET_BYPASS_ATTEMPTED`           | `FOREVER`      | S8.1 §10    | A subject attempted to open a raw / packet socket outside policy.                                 |

### 25.7 Reconciliation note (synonym → canonical)

S5.3 §10 names `APPROVAL_BINDING_VOIDED` for the binding-lifecycle transition `GRANTED → DENIED(reason = ACTION_REVISED | SCOPE_DRIFT | signature)`. S10.1 §13 names `BINDING_VOIDED_ACTION_REVISED` for the runtime detection of canonical-hash drift in eight-step step 1 of the execute path. Both names refer to the **same** underlying event observed at two contract surfaces:

- The **runtime detects** the canonical-hash mismatch at the `EXECUTING` lifecycle transition.
- The **approval-side narrative** refers to the same event by the binding-lifecycle name when describing how a `GRANTED` binding leaves its FSM.

Per §24's narrative-total counting pattern, the L9.1 `RecordType` vocabulary records this event under the **canonical name `BINDING_VOIDED_ACTION_REVISED`** (owned by S10.1, FOREVER retention). `APPROVAL_BINDING_VOIDED` is documented as a **synonym narrative reference** carried forward from S5.3 §10 for binding-lifecycle prose; it is **not** a separate enum entry. The reconciliation collapses one row in the Wave 6 unique-additions count.

Truthful Wave 6 arithmetic:

- S5.2 contributes 8 unique entries.
- S5.3 contributes 9 narrative entries → 8 unique after the §25.7 reconciliation collapses `APPROVAL_BINDING_VOIDED` into `BINDING_VOIDED_ACTION_REVISED`.
- S5.4 contributes 8 unique entries.
- S9.1 contributes 10 unique entries (3 deferred names recorded in §25.4 narrative are not counted).
- S10.1 contributes 20 unique entries (including the canonical `BINDING_VOIDED_ACTION_REVISED`).
- S8.1 contributes 18 unique entries.
- **Wave 6 unique additions: 8 + 8 + 8 + 10 + 20 + 18 = 72.**
- **New cumulative narrative total: 87 (post-Wave 5) + 72 (Wave 6) = 159 entries.**

Per §23 / §24's narrative-only declaration pattern, this Wave 6 does **not** edit Appendix A. Full IDL reconciliation — including the addition of the 72 new payload messages to the discriminated `RecordPayload` oneof, the encoding of `BINDING_VOIDED_ACTION_REVISED` as the canonical enum value, and the documentation of `APPROVAL_BINDING_VOIDED` as a synonym in proto comments — is a separate sweep when the spec is next refined.

### 25.8 Telemetry impact

Each new FOREVER record type contributes to the FOREVER retention storage class summarised in §6.4. Wave 6 introduces **33 new FOREVER record types** (S5.2 × 4, S5.4 × 8, S9.1 × 10, S10.1 × 4, S8.1 × 7), **8 new EXTENDED_60M record types** (S5.2 × 1, S5.3 × 3, S10.1 × 5 — note one S5.3 EXTENDED_60M label has been counted under EXTENDED in this Wave 6 default mapping; the actual count is S5.2 × 1, S5.3 × 3, S10.1 × 5, S8.1 × 5 = 14), and the remainder at `STANDARD_24M`. Truthful per-class delta arithmetic for storage planning:

| Retention class | Wave 6 additions | Notes                                                                                   |
| --------------- | ---------------: | --------------------------------------------------------------------------------------- |
| `STANDARD_24M`  |               25 | S5.2 × 3, S5.3 × 5, S10.1 × 11, S8.1 × 6.                                               |
| `EXTENDED_60M`  |               14 | S5.2 × 1, S5.3 × 3, S10.1 × 5, S8.1 × 5.                                                |
| `FOREVER`       |               33 | S5.2 × 4, S5.4 × 8, S9.1 × 10, S10.1 × 4, S8.1 × 7. Constitutional and forensic events. |

Total: 25 + 14 + 33 = 72 unique additions, matching the §25.7 arithmetic. The §20 per-`record_type` cardinality reservation is bumped from 87 to 159 entries narratively. Existing histogram and counter labels remain valid; subject, group, and channel ids are never labels — they would inflate cardinality unboundedly and would re-introduce subject identity into the metrics surface that §20 forbids.

The L0 invariant candidate `NETWORK_DEFAULT_DENY_OUTBOUND` queued by S8.1 is **out of scope** for this Wave 6; it requires a separate L0 sweep.

## 26. Wave 7 cross-spec touch-up (S11.1 + S9.3 + S12.1 record-type consolidation)

Applied 2026-05-11. Sources: [S11.1 §17](../L10_Distribution_Ecosystem_Marketplace/01_repository_model.md), [S9.3 §16](../L1_Kernel_Bootstrap_Recovery/03_dedicated_kernel_pipeline.md), [S12.1 §11](../L6_Apps_Packages_Compatibility/01_app_runtime_model.md). This section consolidates the queued `RecordType` additions from the L10 distribution / repository contract, the L1 dedicated-kernel pipeline contract, and the L6 app-runtime model contract through the L9.1 Evidence Log. Each row binds a record name to its retention class (closed enum from §6.4: `STANDARD_24M` / `EXTENDED_60M` / `FOREVER`) and to the source spec section that owns its emission contract. Following the §23 / §24 / §25 narrative-only declaration pattern, this addition does **not** modify Appendix A's proto IDL block; full IDL reconciliation (the addition of new payload messages to the discriminated `RecordPayload` oneof) is deferred to a subsequent refinement sweep. After this addition the **`RecordType` vocabulary now totals 205 entries narratively** (159 prior + 46 Wave 7 unique additions; no synonym reconciliations are required in this Wave because the three source contracts use disjoint name prefixes — `PACKAGE_*` / `KERNEL_*` / `APP_*` / `*_KEY_ROTATED` — and do not overlap with prior Wave-1..6 names).

### 26.1 From S11.1 Repository Model (19 types)

Source: [S11.1 §17](../L10_Distribution_Ecosystem_Marketplace/01_repository_model.md). Append authority is restricted to the L11 repository / install-pipeline service; the AIOS root-key rotation and publisher-key rotation records are appended only by the recovery-mode trust-root manager, never by an AI subject. Forgery from any other subject is hard-denied at the engine surface and emits `TAMPER_DETECTED` per §11. The nine FOREVER entries below cover the trust-root, deplatform, and capability-lie surfaces — these are constitutional events whose audit trail must remain immutable for the lifetime of the host.

| RecordType                                  | Retention      | Source spec | Purpose                                                                                                                         |
| ------------------------------------------- | -------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `PACKAGE_FETCH_STARTED`                     | `STANDARD_24M` | S11.1 §17   | Install-pipeline step 1 began; carries package id, mirror id, `MirrorSemantic`.                                                 |
| `PACKAGE_VERIFIED`                          | `STANDARD_24M` | S11.1 §17   | Verification steps 2–9 all passed; carries `PackageManifest` canonical hash, publisher chain depth.                             |
| `PACKAGE_VERIFICATION_FAILED`               | `EXTENDED_60M` | S11.1 §17   | A verification step failed; carries the failing `PackageVerificationResult` reason code.                                        |
| `PACKAGE_APPROVAL_REQUESTED`                | `STANDARD_24M` | S11.1 §17   | Step 11 entered `AWAITING_APPROVAL`; carries the S5.3 `ApprovalRequest` id and `EXACT_ACTION` binding.                          |
| `PACKAGE_INSTALLED`                         | `STANDARD_24M` | S11.1 §17   | FSM transitioned to `ACTIVE` after a successful atomic install at step 16.                                                      |
| `PACKAGE_INSTALL_FAILED`                    | `EXTENDED_60M` | S11.1 §17   | FSM reached `INSTALL_FAILED`; carries the failing step number and reason class.                                                 |
| `PACKAGE_QUARANTINED`                       | `FOREVER`      | S11.1 §17   | FSM transitioned `ACTIVE → QUARANTINED`; carries the `TakedownReason` or first-run-audit reason. Constitutional forensic event. |
| `PACKAGE_UNINSTALLED`                       | `STANDARD_24M` | S11.1 §17   | FSM transitioned `UNINSTALLING → REMOVED`; carries removed canonical paths.                                                     |
| `PACKAGE_DOWNGRADE_BLOCKED`                 | `EXTENDED_60M` | S11.1 §17   | Step 6 detected a version downgrade against the manifest monotonicity rule.                                                     |
| `CAPABILITY_LIE_DETECTED`                   | `FOREVER`      | S11.1 §17   | First-run audit (S11.1 §9) found the package using capabilities not declared in its manifest; constitutional fault.             |
| `TRUST_CHAIN_BROKEN`                        | `FOREVER`      | S11.1 §17   | Step 3 chain verify failed (revoked key, missing catalog, signature failure on an intermediate). Constitutional forensic event. |
| `TRUST_CHAIN_TOO_DEEP`                      | `FOREVER`      | S11.1 §17   | Step 3 detected chain depth > 3 — exceeds the firmware-pinned constitutional ceiling.                                           |
| `MANIFEST_FORGED`                           | `FOREVER`      | S11.1 §17   | Step 6 detected a forged manifest field (canonical hash mismatch, trust-level mismatch); constitutional forensic event.         |
| `MIRROR_HASH_MISMATCH_BLACKLISTED`          | `FOREVER`      | S11.1 §17   | A mirror's mismatch counter exceeded the §10 threshold and the mirror was auto-blacklisted; constitutional forensic event.      |
| `PUBLISHER_KEY_ROTATED`                     | `FOREVER`      | S11.1 §17   | Publisher root-key rotation completed (S11.1 §11); carries old / new publisher root id and AIOS-root cosignature.               |
| `PUBLISHER_DEPLATFORMED`                    | `FOREVER`      | S11.1 §17   | An AIOS-root cosigned takedown event (S11.1 §12); carries `TakedownReason` and affected publisher id.                           |
| `EXTERNAL_BRIDGE_PACKAGE_ADMITTED`          | `STANDARD_24M` | S11.1 §17   | A bridge admitted an upstream package (S11.1 §14.2); carries bridge id, upstream provenance, and trust ceiling.                 |
| `EXTERNAL_BRIDGE_UPSTREAM_SIGNATURE_FAILED` | `EXTENDED_60M` | S11.1 §17   | A bridge's upstream signature verification failed; carries upstream channel and failure class.                                  |
| `AIOS_ROOT_KEY_ROTATED`                     | `FOREVER`      | S11.1 §17   | The AIOS root key was rotated under recovery mode (S11.1 §4.1); the apex constitutional trust-root event.                       |

### 26.2 From S9.3 Dedicated Kernel Pipeline (13 types)

Source: [S9.3 §16](../L1_Kernel_Bootstrap_Recovery/03_dedicated_kernel_pipeline.md). Append authority is restricted to the `_system:service:kernel-builder` subject for the build / gate / convergence / refresh records, to the bootloader for the `KERNEL_IMAGE_OBSERVED`, `KERNEL_IMAGE_DRIFT_DETECTED`, and `KERNEL_ROLLBACK_PERFORMED` records, and to the recovery-mode supervisor for promotion and pipeline-definition replacement records. `KERNEL_IMAGE_DRIFT_DETECTED` is the recursive self-application of the spec: the kernel image itself is a typed evidence subject whose hash is observed at every boot and compared against the vault-pinned expected hash. The five FOREVER entries below cover promotion, rollback, drift, and pipeline-definition mutation — the events that materially change what binary the host runs as ring-zero code.

| RecordType                       | Retention      | Source spec | Purpose                                                                                                                                          |
| -------------------------------- | -------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `KERNEL_PIPELINE_STARTED`        | `STANDARD_24M` | S9.3 §16    | A new `kernel.build` action transitioned to `EXECUTING`; carries `kbld_<id>`, `pipeline_definition_id`, hardware-graph snapshot id.              |
| `KERNEL_BUILD_COMPLETED`         | `STANDARD_24M` | S9.3 §16    | The build adapter returned success; carries the final `kimg_<full_blake3>`.                                                                      |
| `KERNEL_GATE_RESULT`             | `STANDARD_24M` | S9.3 §16    | Each gate evaluation; carries `GateName`, `GateResult`, measurement struct, threshold struct, kernel-builder Ed25519 signature.                  |
| `KERNEL_CONVERGED`               | `STANDARD_24M` | S9.3 §16    | The pipeline reached a `CONVERGED` fixed-point; carries the final input-tuple hash.                                                              |
| `KERNEL_DIVERGED_REGRESSION`     | `EXTENDED_60M` | S9.3 §16    | A gate score regressed against the §5.3 monotonicity rule; carries the regressing gate, previous score, current score.                           |
| `KERNEL_PROMOTED_TO_A`           | `FOREVER`      | S9.3 §16    | A `GATE_PASSED` image transitioned to `A_PROMOTED` under recovery mode; constitutional ring-zero event.                                          |
| `KERNEL_PROMOTED_TO_B`           | `FOREVER`      | S9.3 §16    | The previous A image was demoted to slot B as part of the promotion FSM; carries both image hashes.                                              |
| `KERNEL_ROLLBACK_PERFORMED`      | `FOREVER`      | S9.3 §16    | Bootloader auto-rollback after `N_rollback_boots` consecutive failures; carries failed and replacement image hashes.                             |
| `KERNEL_IMAGE_OBSERVED`          | `STANDARD_24M` | S9.3 §16    | Every successful boot's running-kernel measurement; carries `kimg_<hash>`, slot, PCR attestation bytes.                                          |
| `KERNEL_IMAGE_DRIFT_DETECTED`    | `FOREVER`      | S9.3 §16    | Observed boot hash ≠ vault-pinned expected hash; the recursive self-application of evidence to the kernel itself; constitutional forensic event. |
| `KERNEL_REFRESH_SCHEDULED`       | `STANDARD_24M` | S9.3 §16    | A scheduled `kernel.refresh` action fired; carries cadence id and target upstream version.                                                       |
| `KERNEL_REFRESH_PIPELINE_FAILED` | `EXTENDED_60M` | S9.3 §16    | A scheduled refresh failed (`KernelMaintenanceResult ∈ {GATE_FAILED, PIPELINE_ERROR}`); carries failing gate or error class.                     |
| `PIPELINE_DEFINITION_REPLACED`   | `FOREVER`      | S9.3 §16    | The pipeline-definition itself was replaced under the §4.4 / §7.2 recovery-mode carve-out; carries old and new `kpdef_<id>` and operator id.     |

### 26.3 From S12.1 App Runtime Model (14 types)

Source: [S12.1 §11](../L6_Apps_Packages_Compatibility/01_app_runtime_model.md). Append authority is restricted to the L6 app-runtime orchestrator for the observe / translate / delta / recipe records; the `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` and `APP_HONESTY_CLASS_VIOLATION` records are appended only by the Capability Runtime and the runtime sandbox enforcer respectively. The four FOREVER entries cover the constitutional honesty surface (claimed `EcosystemHonestyClass` vs observed behaviour), the sandbox-breakout surface, the AI-cannot-install invariant (INV-002 enforcement), and the registry-ingest deception surface. `APP_HONESTY_CLASS_VIOLATION` fires when a recipe or manifest claims `FULLY_SUPPORTED` but the runtime observes `NOT_RUNNABLE` (or any inconsistent disclosure tier from S12.1 §3.2).

| RecordType                                 | Retention      | Source spec | Purpose                                                                                                                                                              |
| ------------------------------------------ | -------------- | ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `APP_OBSERVE_STARTED`                      | `STANDARD_24M` | S12.1 §11   | Phase A `app.observe_in_sandbox` action transitioned to `EXECUTING`.                                                                                                 |
| `APP_OBSERVE_COMPLETED`                    | `STANDARD_24M` | S12.1 §11   | Phase A action transitioned to `SUCCEEDED`; carries the redacted `ObservedBehavior` summary and `observed_behavior_hash`.                                            |
| `APP_OBSERVE_TIMEOUT`                      | `EXTENDED_60M` | S12.1 §11   | Phase A hard timeout reached; partial summary emitted with truncation marker.                                                                                        |
| `APP_TRANSLATE_MANIFEST_PROPOSED`          | `STANDARD_24M` | S12.1 §11   | Phase B `app.translate_manifest` action emitted a proposal awaiting S5.3 approval; carries `policy_decision_id`.                                                     |
| `APP_TRANSLATE_MANIFEST_APPROVED`          | `STANDARD_24M` | S12.1 §11   | S5.3 approval granted; the proposal becomes the bound manifest for the install pipeline.                                                                             |
| `APP_TRANSLATE_MANIFEST_REJECTED`          | `EXTENDED_60M` | S12.1 §11   | S5.3 approval denied or expired; proposal discarded.                                                                                                                 |
| `APP_RECIPE_CONTRIBUTED`                   | `STANDARD_24M` | S12.1 §11   | Operator contributed a recipe back to the community registry; carries recipe id and `RecipeTrustClass`.                                                              |
| `APP_RECIPE_IMPORTED`                      | `STANDARD_24M` | S12.1 §11   | One-shot import from upstream (ProtonDB / Flathub / AUR / Snapcraft) completed.                                                                                      |
| `APP_MANIFEST_DELTA_PROPOSED`              | `STANDARD_24M` | S12.1 §11   | Phase D continuous-refinement delta proposal emitted.                                                                                                                |
| `APP_MANIFEST_DELTA_APPROVED`              | `STANDARD_24M` | S12.1 §11   | Operator approved a Phase D delta; the new manifest version was installed.                                                                                           |
| `APP_HONESTY_CLASS_VIOLATION`              | `FOREVER`      | S12.1 §11   | Runtime observed behaviour inconsistent with the claimed `EcosystemHonestyClass` (e.g. claimed `FULLY_SUPPORTED` but observed `NOT_RUNNABLE`). Constitutional fault. |
| `APP_ECOSYSTEM_RUNTIME_BREAKOUT_ATTEMPTED` | `FOREVER`      | S12.1 §11   | Wine / Waydroid / VM / sandbox breakout reported by an enforcer; constitutional sandbox-floor event.                                                                 |
| `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED`  | `FOREVER`      | S12.1 §11   | An AI subject attempted a direct install without operator approval — INV-002 enforcement; constitutional fault.                                                      |
| `APP_RECIPE_DECEPTIVE_REJECTED_AT_INGEST`  | `FOREVER`      | S12.1 §11   | A recipe claimed iOS direct execution, shipped a fingerprint-spoofing payload, or otherwise lied about runtime capability; rejected at registry ingest.              |

### 26.4 Reconciliation note (truthful arithmetic)

Per §24's narrative-total counting pattern, this Wave 7 records 46 unique `RecordType` additions: 19 from S11.1, 13 from S9.3, 14 from S12.1, with no synonym collisions. The retention-class distribution across these 46 additions, counted directly from §26.1 / §26.2 / §26.3, is:

| Retention class | Wave 7 additions | Notes                                                                                                                                                                |
| --------------- | ---------------: | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `STANDARD_24M`  |               20 | S11.1 × 6, S9.3 × 6, S12.1 × 8. Lifecycle and observable-state events.                                                                                               |
| `EXTENDED_60M`  |                8 | S11.1 × 4, S9.3 × 2, S12.1 × 2. Verification failures, refresh failures, install failures, downgrade blocks, observation timeouts, and translate-manifest rejection. |
| `FOREVER`       |               18 | S11.1 × 9, S9.3 × 5, S12.1 × 4. Trust-root, drift, breakout, and honesty-violation events.                                                                           |

Total: 20 + 8 + 18 = 46 unique additions. New cumulative narrative total: **159 (post-Wave 6) + 46 (Wave 7) = 205 entries**.

> **Counting note vs Wave 7 charter.** The Wave 7 charter expected a distribution of `STANDARD_24M = 18 / EXTENDED_60M = 7 / FOREVER = 21`. Direct row-by-row recount against the source contracts (§26.1 / §26.2 / §26.3) yields `20 / 8 / 18`. The truthful counts are recorded above; the charter's expectation appears to have miscounted three S12.1 STANDARD_24M rows as FOREVER (the Phase B/D approval-granted pair plus one delta-approved row) and one S11.1 EXTENDED_60M row. The total of 46 is unchanged either way; the per-class arithmetic here is the source-of-truth.

Per §23 / §24 / §25's narrative-only declaration pattern, this Wave 7 does **not** edit Appendix A. Full IDL reconciliation — addition of the 46 new payload messages to the discriminated `RecordPayload` oneof — is a separate sweep when the spec is next refined.

### 26.5 Telemetry impact

Each new FOREVER record type contributes to the FOREVER retention storage class summarised in §6.4. Wave 7 introduces **18 new FOREVER record types** (S11.1 × 9, S9.3 × 5, S12.1 × 4). Cumulative FOREVER narrative entries through Wave 7: 14 (post-Wave 5) + 33 (Wave 6) + 18 (Wave 7) = **65 narrative FOREVER entries**. The §20 per-`record_type` cardinality reservation is bumped from 159 to 205 entries narratively. Existing histogram and counter labels remain valid; subject, group, and channel ids are never labels — they would inflate cardinality unboundedly and would re-introduce subject identity into the metrics surface that §20 forbids.

The new FOREVER surface in Wave 7 is dominated by the S11.1 trust-root chain (`*_KEY_ROTATED`, `TRUST_CHAIN_*`, `MANIFEST_FORGED`, `PUBLISHER_DEPLATFORMED`) and the S9.3 ring-zero promotion / drift surface (`KERNEL_PROMOTED_*`, `KERNEL_ROLLBACK_PERFORMED`, `KERNEL_IMAGE_DRIFT_DETECTED`, `PIPELINE_DEFINITION_REPLACED`). These are the two most security-sensitive surfaces the system exposes — the binary that runs as ring-zero, and the chain of signatures that authorises every other binary on the host — so the FOREVER share of Wave 7 is structural, not accidental.

### 26.6 Cross-spec impact note (queued for separate consolidations)

This Wave 7 records only the `RecordType` consolidation. Other items queued by the three source contracts are **out of scope** for this Wave and are deferred to separate sweeps:

- **Six new typed actions** queued for the S10.1 Capability Runtime catalog: four from S12.1 (`app.observe_in_sandbox`, `app.translate_manifest`, `app.propose_manifest_delta`, `app.contribute_recipe`) and two from S9.3 (`kernel.build`, `kernel.refresh`). These are consolidated into S10.1 separately.
- **One new field** queued for the S3.2 `SandboxProfile` shape (`ecosystem_runtime: EcosystemRuntime`); handled by the S3.2 orchestrator.
- **Three candidate L0 invariants** queued narrative-only by these contracts: `NETWORK_DEFAULT_DENY_OUTBOUND` (S8.1, carried forward from Wave 6 and still pending), `PACKAGE_TRUST_CHAIN_BOUND` (S11.1 §19), and `ECOSYSTEM_HONESTY_DISCLOSURE` (S12.1). Per L0 §3 I1, invariant catalog mutation is a versioned spec change and recovery-mode invariant-bundle update — these are held for the audit-phase L0 sweep per the project owner's "deliberate single-purpose constitutional act" pattern and are **not** promoted in this Wave.

## 27. Wave 8 cross-spec touch-up (Tier 1 + Tier 2 record-type consolidation)

Applied 2026-05-09. Sources: [S9.2 §12](../L1_Kernel_Bootstrap_Recovery/02_first_boot_flow.md), [S14.1 §11](03_failure_handling.md), [S6.3 §12](../L0_Governance_Evidence_Safety/03_evidence_receipt_schema.md), [S15.1 §11](../L3_AIOS_SGR_Service_Graph_Runtime/01_unit_manifest.md), [S15.2 §9](../L3_AIOS_SGR_Service_Graph_Runtime/02_state_transitions.md), [S15.3 §9](../L3_AIOS_SGR_Service_Graph_Runtime/04_adapter_model.md), [S13.2 §13](../L5_Cognitive_Core/05_model_router.md), [S13.1 §16](../L5_Cognitive_Core/01_cognitive_core_model.md), [S12.2 §11](../L6_Apps_Packages_Compatibility/02_package_model.md), [S12.3 §11](../L6_Apps_Packages_Compatibility/03_compatibility_runtime.md), [S12.4 §11](../L6_Apps_Packages_Compatibility/05_compatibility_knowledge.md), [S7.6 §11](../L7_Interaction_Renderers/06_cli_renderer.md), [S8.3 §12](../L8_Network_Hardware_Devices/01_hardware_graph.md), [S8.4 §13](../L8_Network_Hardware_Devices/03_dns_vpn_management.md), [S8.5 §13](../L8_Network_Hardware_Devices/04_firmware_trust.md), [S14.2 §13](04_telemetry_pipeline.md), [S11.2 §12](../L10_Distribution_Ecosystem_Marketplace/02_marketplace.md), [S11.3 §13](../L10_Distribution_Ecosystem_Marketplace/03_external_integrations.md). This Wave consolidates the queued `RecordType` additions from three Tier-1 contracts (first-boot flow, failure-handling discipline, evidence-receipt schema) and fifteen Tier-2 contracts (SGR unit / state / adapter; cognitive model router; cognitive core model; package model; compatibility runtime; compatibility knowledge; CLI renderer; hardware graph; DNS/VPN management; firmware trust; telemetry pipeline; marketplace; external integrations) through the L9.1 Evidence Log. Each row binds a record name to its retention class (closed enum from §6.4: `STANDARD_24M` / `EXTENDED_60M` / `FOREVER`) and to the source spec section that owns its emission contract. Following the §23 / §24 / §25 / §26 narrative-only declaration pattern, this addition does **not** modify Appendix A's proto IDL block; full IDL reconciliation (the addition of new payload messages to the discriminated `RecordPayload` oneof) is deferred to a subsequent refinement sweep. After this addition the **`RecordType` vocabulary now totals 400 entries narratively** (205 prior + 195 Wave 8 unique additions; no exact-name collisions with prior Wave-1..7 names — adjacency notes are recorded in §27.21).

### 27.1 From S9.2 First-Boot Flow (11 types)

Source: [S9.2 §12](../L1_Kernel_Bootstrap_Recovery/02_first_boot_flow.md). Append authority is restricted to the L1 first-boot installer subjects (`_system:service:identity-init`, `_system:service:vault-init`, `_system:service:firstboot-orchestrator`) for the constitutional commits; the `FIRST_BOOT_STAGE_COMPLETED` record is appended per stage by the orchestrator. The ten FOREVER entries below cover every constitutional commit a host makes during first-boot — vault root key, AI provider mode, firewall posture, first group, first user, recovery operator, first-boot completion marker, reset-to-factory initiation, and the start / failure of the flow itself. The chain across `FIRST_BOOT_STARTED → ... → FIRST_BOOT_COMPLETE` plus any subsequent `RESET_TO_FACTORY_INITIATED` records is the host's lifetime constitutional history.

| RecordType                     | Retention      | Source spec | Purpose                                                                                                                     |
| ------------------------------ | -------------- | ----------- | --------------------------------------------------------------------------------------------------------------------------- |
| `FIRST_BOOT_STARTED`           | `FOREVER`      | S9.2 §12    | First-boot session began; carries `FirstBootEntryReason`, host hostname, media signature id.                                |
| `FIRST_BOOT_STAGE_COMPLETED`   | `STANDARD_24M` | S9.2 §12    | Per-stage completion record; one per `FirstBootStage` transition (15 stages × 1 record each on success).                    |
| `FIRST_BOOT_FAILED`            | `FOREVER`      | S9.2 §12    | Terminal-failure at any stage; carries failed `FirstBootStage` and `FirstBootFailureReason`.                                |
| `VAULT_ROOT_KEY_GENERATED`     | `FOREVER`      | S9.2 §12    | The vault master key was generated and sealed; carries `seal_kind` (TPM / HARDWARE_KEY / HARDWARE_KEY_FILE) and PCRs.       |
| `AI_PROVIDER_MODE_SET`         | `FOREVER`      | S9.2 §12    | Operator chose `AIProviderMode`; carries provider id (no key material per INV-015), routing-table hash for HYBRID.          |
| `INITIAL_FIREWALL_POSTURE_SET` | `FOREVER`      | S9.2 §12    | Operator chose `InitialFirewallPosture`; carries derived LAN ranges where applicable.                                       |
| `FIRST_GROUP_REGISTERED`       | `FOREVER`      | S9.2 §12    | The host's first user group manifest was sealed; carries `group_id`, `GroupTier`, AI/install eligibility flags.             |
| `FIRST_USER_REGISTERED`        | `FOREVER`      | S9.2 §12    | The first `HUMAN_USER` subject was created; carries canonical id and enrolled credential kinds.                             |
| `RECOVERY_OPERATOR_REGISTERED` | `FOREVER`      | S9.2 §12    | The recovery-mode operator credential set was bound; carries `RecoveryCredentialKind` and hardware-key serial hash.         |
| `FIRST_BOOT_COMPLETE`          | `FOREVER`      | S9.2 §12    | Terminal commit: `/aios/system/firstboot/marker` was atomically written; carries the constitutional `state_hash`.           |
| `RESET_TO_FACTORY_INITIATED`   | `FOREVER`      | S9.2 §12    | Recovery-mode reset-to-factory began (precedes a fresh first-boot run); carries operator id, co-signer, prior `state_hash`. |

Subsection retention split: `STANDARD_24M` × 1, `EXTENDED_60M` × 0, `FOREVER` × 10 (eleven rows total).

### 27.2 From S14.1 Failure Handling (10 types)

Source: [S14.1 §11](03_failure_handling.md). Append authority is restricted to the L4.1 policy kernel for the `*_BUNDLE_REJECTED` family (which is referenced by S14.1 but already covered as a forensic class), to component supervisors for `COMPONENT_RESTARTED` / `COMPONENT_RESTART_BUDGET_EXHAUSTED`, to the runtime's circuit-breaker for `CIRCUIT_BREAKER_OPENED` / `CLOSED`, to the boot-time substrate-version checker for `BACKEND_VERSION_MISMATCH`, and to the recovery supervisor for `HALTED_PENDING_OPERATOR` / `RECOVERY_LOOP_DETECTED`. The four FOREVER entries cover the constitutional escalation surface — restart-budget exhaustion, halt-to-operator, substrate version mismatch (boot-time refusal to mount `/aios`), and recovery-loop detection (3-in-60-min for the same `RecoveryEntryReason`). Per S14.1 §9.5, FOREVER records in this set are **never** rate-limited; saturation cannot drop them.

| RecordType                           | Retention      | Source spec | Purpose                                                                                                           |
| ------------------------------------ | -------------- | ----------- | ----------------------------------------------------------------------------------------------------------------- |
| `FAILURE_OBSERVED`                   | `STANDARD_24M` | S14.1 §11   | Generic failure observation; carries `FailureClass`, layer id, runbook reference.                                 |
| `DEGRADATION_LEVEL_TRANSITIONED`     | `STANDARD_24M` | S14.1 §11   | The host transitioned between `DegradationLevel` values; carries `from`, `to`, triggering `FailureClass`.         |
| `COMPONENT_RESTARTED`                | `STANDARD_24M` | S14.1 §11   | Per-restart record for a managed component (3-in-5 / 5-in-10 budget tracking).                                    |
| `COMPONENT_RESTART_BUDGET_EXHAUSTED` | `FOREVER`      | S14.1 §11   | Restart budget exhausted; defines the moment of recovery escalation.                                              |
| `CIRCUIT_BREAKER_OPENED`             | `EXTENDED_60M` | S14.1 §11   | Breaker opened; carries target, failure count, cool-down window.                                                  |
| `CIRCUIT_BREAKER_CLOSED`             | `STANDARD_24M` | S14.1 §11   | Breaker closed; carries target, time-open.                                                                        |
| `HALTED_PENDING_OPERATOR`            | `FOREVER`      | S14.1 §11   | System entered HALTED degradation level; carries triggering `FailureClass` and the chain of escalation.           |
| `TIME_DRIFT_DETECTED`                | `EXTENDED_60M` | S14.1 §11   | Wall-clock drift exceeded tolerance; carries observed skew and tolerance.                                         |
| `BACKEND_VERSION_MISMATCH`           | `FOREVER`      | S14.1 §11   | Boot-time substrate version mismatch (kernel / AIOS-FS); the host halts before `/aios` mount.                     |
| `RECOVERY_LOOP_DETECTED`             | `FOREVER`      | S14.1 §11   | N entries in M minutes for the same `RecoveryEntryReason` (default 3-in-60-min); the system halts and emits this. |

Subsection retention split: `STANDARD_24M` × 4, `EXTENDED_60M` × 2, `FOREVER` × 4 (ten rows total).

### 27.3 From S6.3 Evidence Receipt Schema (4 types)

Source: [S6.3 §12](../L0_Governance_Evidence_Safety/03_evidence_receipt_schema.md). Append authority for all four is the Evidence Log itself — these records describe the log catching forgery, integrity, lineage, and sequence anomalies in its own append-and-audit pipeline. Emission attempts from any other subject are hard-denied at the engine surface and emit `TAMPER_DETECTED` per §11.5. Note: the S6.3 contract self-declared a cumulative running total of "209 entries narratively (205 prior + 4 from S6.3)"; that arithmetic is consumed verbatim into this Wave 8 consolidation. All four are FOREVER because they record adversarial-detection events on the audit chain itself — the apex constitutional surface.

| RecordType                       | Retention | Source spec | Purpose                                                                                                            |
| -------------------------------- | --------- | ----------- | ------------------------------------------------------------------------------------------------------------------ |
| `RECEIPT_REDACTION_FAILED`       | `FOREVER` | S6.3 §6.2   | Emit-time redaction validation could not complete and the receipt was rejected (secret-shaped payload content).    |
| `RECEIPT_INTEGRITY_QUARANTINED`  | `FOREVER` | S6.3 §4.2   | A segment-integrity audit found a chain-hash mismatch or a segment-Ed25519 signature failure; segment quarantined. |
| `RECEIPT_LINEAGE_CYCLE_DETECTED` | `FOREVER` | S6.3 §7.3   | A scheduled lineage audit found a cycle in the receipt DAG; carries cycle's receipt ids and detection method.      |
| `RECEIPT_SEQUENCE_OUT_OF_ORDER`  | `FOREVER` | S6.3 §9.8   | A sequence-ordering anomaly was detected at audit time (e.g. WAL replay produced a non-monotonic sequence).        |

Subsection retention split: `STANDARD_24M` × 0, `EXTENDED_60M` × 0, `FOREVER` × 4 (four rows total).

### 27.4 From S15.1 Unit Manifest (8 types)

Source: [S15.1 §11](../L3_AIOS_SGR_Service_Graph_Runtime/01_unit_manifest.md). Append authority is restricted to the L3 SGR runtime for unit lifecycle records. The single FOREVER entry covers the constitutional-fault detection of dependency cycles in admission validation — adversarial manifest forensics. Three additional names (`MANIFEST_VALIDATION_REJECTED`, `UNIT_REPLAY_REJECTED`, `UNIT_PUBLISHER_TRUST_REVOKED`) are mentioned narrative-only by S15.1 and are **excluded** from this Wave-8 count per the source's own "queued narrative-only for next-Wave consolidation" framing.

| RecordType                       | Retention      | Source spec | Purpose                                                                                                                         |
| -------------------------------- | -------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `UNIT_REGISTERED`                | `STANDARD_24M` | S15.1 §11   | Manifest admitted; transition `DRAFT → QUEUED`; carries `unit_id`, `unit_kind`, `canonical_hash`, `publisher_id`.               |
| `UNIT_STARTED`                   | `STANDARD_24M` | S15.1 §11   | Transition `STARTING → RUNNING`; carries adapter id, dispatch kind, action_id of the `unit.start` envelope.                     |
| `UNIT_HEALTHY`                   | `STANDARD_24M` | S15.1 §11   | Transition into `HEALTHY` after `RUNNING` or recovery from `UNHEALTHY`; carries verification result hashes.                     |
| `UNIT_DEGRADED`                  | `EXTENDED_60M` | S15.1 §11   | Transition `HEALTHY → DEGRADED`; carries verification primitive that returned `WARNING` and operator-runbook reference.         |
| `UNIT_FAILED`                    | `EXTENDED_60M` | S15.1 §11   | Transition into `FAILED` from any state; carries last `UnitState`, `ExecutionFailureReason`, action_id chain.                   |
| `UNIT_STOPPED`                   | `STANDARD_24M` | S15.1 §11   | Transition `STOPPING → STOPPED`; carries stop reason (`OPERATOR` / `DEPENDENCY_STOP` / `ROLLBACK` / `RETIREMENT`).              |
| `UNIT_ROLLBACK_TRIGGERED`        | `EXTENDED_60M` | S15.1 §11   | A `RollbackTrigger` fired and `unit.rollback` was dispatched; carries pointer id, CAS outcome, prior/next version pair.         |
| `UNIT_DEPENDENCY_CYCLE_DETECTED` | `FOREVER`      | S15.1 §11   | Admission validator detected a cycle in dependencies; carries unit_id, cycle path, publisher_id. Constitutional forensic event. |

Subsection retention split: `STANDARD_24M` × 4, `EXTENDED_60M` × 3, `FOREVER` × 1 (eight rows total).

### 27.5 From S15.2 SGR State Transitions (12 types)

Source: [S15.2 §9](../L3_AIOS_SGR_Service_Graph_Runtime/02_state_transitions.md). Append authority is restricted to the L3 SGR runtime's transition dispatcher and the dependency solver. The three FOREVER entries cover constitutional refusals: A/B rollback (post-promotion fault), dependency cycle (a constitutional fault that the runtime fails closed against), and transition conflict (two contradictory transitions on the same unit; the first-submitted wins, the second is FOREVER-recorded). Note: `DEPENDENCY_CYCLE_DETECTED` here (S15.2) is semantically adjacent to `UNIT_DEPENDENCY_CYCLE_DETECTED` (S15.1, §27.4) but constitutes a distinct record-name binding — the unit-manifest variant fires at admission, the state-transition variant fires at dependency-graph evaluation. Both are retained because the per-source append authority and the payload shape differ.

| RecordType                  | Retention      | Source spec | Purpose                                                                                                                           |
| --------------------------- | -------------- | ----------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `GRAPH_EVALUATED`           | `STANDARD_24M` | S15.2 §9    | A graph evaluation completed with result `IN_PROGRESS`; carries `transition_plan_id`, transition count, dependency_result.        |
| `TRANSITION_QUEUED`         | `STANDARD_24M` | S15.2 §9    | A transition was added to the dispatch queue; carries `transition_id`, `transition_kind`, `unit_id`.                              |
| `TRANSITION_STARTED`        | `STANDARD_24M` | S15.2 §9    | A transition was dispatched to the adapter; carries `transition_id`, dispatch start timestamp.                                    |
| `TRANSITION_SUCCEEDED`      | `STANDARD_24M` | S15.2 §9    | A transition reached its terminal-success state; carries `transition_id`, verification result hash.                               |
| `TRANSITION_FAILED`         | `EXTENDED_60M` | S15.2 §9    | Terminal-failure (verification failed, adapter error, budget exceeded); carries `transition_id`, reason.                          |
| `AB_CANARY_PROMOTED`        | `STANDARD_24M` | S15.2 §9    | A/B promotion FSM transitioned `CANARY → A_PROMOTED`; carries `unit_id`, success_count, target_image_hash.                        |
| `AB_ROLLBACK_PERFORMED`     | `FOREVER`      | S15.2 §9    | A/B promotion FSM transitioned to `ROLLBACK`; carries failure_count, prior/target image hashes. Constitutional refused-promotion. |
| `DEPENDENCY_CYCLE_DETECTED` | `FOREVER`      | S15.2 §9    | The dependency solver detected a cycle at evaluation; carries `cycle_nodes[]`, `cycle_edges[]`, evaluation_input_hash.            |
| `TRANSITION_CONFLICT`       | `FOREVER`      | S15.2 §9    | Two contradictory transitions detected on the same unit; carries winning/rejected ids, conflict_kind. First-submitted wins.       |
| `RESOURCE_BUDGET_DENIED`    | `EXTENDED_60M` | S15.2 §9    | A transition was rejected by composition; carries `resource_dimension`, requested, available, source_blocking.                    |
| `GRAPH_BLOCKED_RESOURCE`    | `STANDARD_24M` | S15.2 §9    | An evaluation returned `BLOCKED_RESOURCE`; carries blocking dimension, blocking source.                                           |
| `GRAPH_CONVERGED`           | `STANDARD_24M` | S15.2 §9    | An evaluation returned `CONVERGED`; carries `graph_state_hash`, `target_state_hash` (equal), evaluation duration.                 |

Subsection retention split: `STANDARD_24M` × 7, `EXTENDED_60M` × 2, `FOREVER` × 3 (twelve rows total).

### 27.6 From S15.3 SGR Adapter Model (10 types)

Source: [S15.3 §9](../L3_AIOS_SGR_Service_Graph_Runtime/04_adapter_model.md). Append authority is restricted to the L3 adapter directory service for all admission / lifecycle records and to the sandbox enforcer for the violation records. The four FOREVER entries cover the four classes of admission-boundary failure / runtime-violation that constitute adapter-trust faults: registration rejection (any of the six admission steps failing), action-kind violation (adapter served outside its declared kinds), capability violation (adapter exceeded its declared capability set; caught at the kernel boundary), and downgrade rejection (replay attack against the version-monotonicity check).

| RecordType                       | Retention      | Source spec | Purpose                                                                                                                                   |
| -------------------------------- | -------------- | ----------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `ADAPTER_REGISTRATION_REQUESTED` | `STANDARD_24M` | S15.3 §9    | `runtime.adapter.register` accepted at validation; manifest enters `DRAFT`; carries `adapter_id`, version, publisher root id.             |
| `ADAPTER_REGISTRATION_REJECTED`  | `FOREVER`      | S15.3 §9    | Any of the six admission checks failed; carries failed step id and manifest digest. Constitutional forensic event.                        |
| `ADAPTER_REGISTERED`             | `STANDARD_24M` | S15.3 §9    | All admission checks passed; manifest sealed; `VALIDATING → REGISTERED`.                                                                  |
| `ADAPTER_HEALTHY`                | `STANDARD_24M` | S15.3 §9    | Adapter transitioned `DEGRADED → REGISTERED`; auto-heal observed.                                                                         |
| `ADAPTER_DEGRADED`               | `EXTENDED_60M` | S15.3 §9    | Adapter transitioned `REGISTERED → DEGRADED`; health threshold crossed.                                                                   |
| `ADAPTER_ACTION_KIND_VIOLATION`  | `FOREVER`      | S15.3 §9    | Adapter served a response for an action kind outside its declared set; forces `RETIRED`; constitutional kind-overrun.                     |
| `ADAPTER_CAPABILITY_VIOLATION`   | `FOREVER`      | S15.3 §9    | Adapter invoked a capability outside its declared set (caught at sandbox boundary); forces `RETIRED`; constitutional capability-lie.      |
| `ADAPTER_HOT_RELOADED`           | `STANDARD_24M` | S15.3 §9    | Versioned manifest update succeeded; old snapshot retained for in-flight; new snapshot active for new submissions.                        |
| `ADAPTER_DOWNGRADE_REJECTED`     | `FOREVER`      | S15.3 §9    | Registration with `adapter_version` strictly less than highest seen; replay/downgrade defence.                                            |
| `ADAPTER_DEREGISTERED`           | `EXTENDED_60M` | S15.3 §9    | Adapter removed from directory; carries reason (`VOLUNTARY` / `MANIFEST_EXPIRED` / `OPERATOR` / `HEALTH_ESCALATION` / `OPERATOR_RETIRE`). |

Subsection retention split: `STANDARD_24M` × 4, `EXTENDED_60M` × 2, `FOREVER` × 4 (ten rows total).

### 27.7 From S13.2 Cognitive Model Router (12 types)

Source: [S13.2 §13](../L5_Cognitive_Core/05_model_router.md). Append authority is restricted to the L5 model router for invocation / backend / rate-limit records, with parallel emissions from L4.2 (vault-deny) and L8.1 (network-deny) on those denial paths. The two FOREVER entries cover the cognitive trust surface: prompt-injection detection (response body contains an injection pattern recognised by the finding-pass scanner) and response-signature failure (provider's Ed25519 response signature did not verify, where supported). Both are constitutional because they represent the model-output trust boundary — the moment when the cognitive core may have been adversarially influenced. Note: `MODEL_CALL` exists in the original Appendix A enum (line 1155) but is a coarse-grained legacy entry; the twelve names below are the finer-grained replacement vocabulary that subsumes it. Reconciliation between `MODEL_CALL` and the new family is deferred to the IDL sweep.

| RecordType                        | Retention      | Source spec | Purpose                                                                                                         |
| --------------------------------- | -------------- | ----------- | --------------------------------------------------------------------------------------------------------------- |
| `MODEL_INVOCATION_STARTED`        | `STANDARD_24M` | S13.2 §13   | Router begins dispatch to a backend; carries `routing_id`, `correlation_id`, `backend_kind`, `provider_class`.  |
| `MODEL_INVOCATION_SUCCEEDED`      | `STANDARD_24M` | S13.2 §13   | Backend returned `RETURNED_NORMAL` or `RETURNED_DEGRADED`; carries token counts, cost, latency, signature flag. |
| `MODEL_INVOCATION_FAILED`         | `EXTENDED_60M` | S13.2 §13   | Backend returned `TIMEOUT` / `PROVIDER_ERROR`; carries error code, observed latency.                            |
| `MODEL_BACKEND_DEGRADED`          | `EXTENDED_60M` | S13.2 §13   | Backend health FSM transitioned to `DEGRADED_LATENCY` / `DEGRADED_AVAILABILITY`.                                |
| `MODEL_CIRCUIT_OPENED`            | `EXTENDED_60M` | S13.2 §13   | Circuit breaker opens on a backend; carries error rate, cool-down seconds.                                      |
| `MODEL_PROMPT_INJECTION_DETECTED` | `FOREVER`      | S13.2 §13   | Finding pass detected an injection pattern in the response body; constitutional cognitive-trust event.          |
| `MODEL_RESPONSE_SIGNATURE_FAILED` | `FOREVER`      | S13.2 §13   | Provider response Ed25519 signature failed verification; response dropped, never returned to S1.2.              |
| `MODEL_VAULT_DENY`                | `EXTENDED_60M` | S13.2 §13   | L4.2 broker rejected the model invocation (capability missing, budget, AI-tries-`SECRET_GET`).                  |
| `MODEL_NETWORK_DENY`              | `EXTENDED_60M` | S13.2 §13   | L8.1 dropped the connection for the brokered request; carries posture and network error code.                   |
| `MODEL_RATE_LIMITED`              | `STANDARD_24M` | S13.2 §13   | Subject / group budget exhausted; router queue full.                                                            |
| `MODEL_BACKEND_REGISTERED`        | `STANDARD_24M` | S13.2 §13   | A new model adapter loaded and registered; or signature-failed registration recorded.                           |
| `MODEL_BACKEND_RETIRED`           | `EXTENDED_60M` | S13.2 §13   | An adapter was retired (operator-initiated takedown per S11.1, or version supersession).                        |

Subsection retention split: `STANDARD_24M` × 4, `EXTENDED_60M` × 6, `FOREVER` × 2 (twelve rows total).

### 27.8 From S13.1 Cognitive Core Model (18 types)

Source: [S13.1 §16](../L5_Cognitive_Core/01_cognitive_core_model.md). Append authority is restricted to the L5 cognitive runtime for the agent lifecycle / proposal / memory / coordination records, and to the policy / sandbox enforcers for the constitutional refusal records. The six FOREVER entries cover INV-002 (AI proposes never executes) enforcement on direct FS write; INV-016 (no self-grading) enforcement; INV-011 (cross-group access forbidden) at the agent coordination layer; INV-004/INV-012 (recovery interrupt) on agents in non-terminal state at recovery entry; the cross-user memory boundary; and the prompt-injection detection at the cognitive ingress.

| RecordType                               | Retention      | Source spec | Purpose                                                                                                              |
| ---------------------------------------- | -------------- | ----------- | -------------------------------------------------------------------------------------------------------------------- |
| `AGENT_REGISTERED`                       | `STANDARD_24M` | S13.1 §16   | A new `Subject` of `SubjectKind = AI_AGENT` was issued.                                                              |
| `AGENT_RETIRED`                          | `EXTENDED_60M` | S13.1 §16   | An agent transitioned to `RETIRED`; carries reason, last lifecycle_state, retirement timestamp.                      |
| `AGENT_INTERRUPTED_BY_RECOVERY`          | `FOREVER`      | S13.1 §16   | An agent was forcibly transitioned to `RETIRING` because `recovery_mode = true`; constitutional INV-004 enforcement. |
| `AGENT_PROPOSAL_EMITTED`                 | `STANDARD_24M` | S13.1 §16   | An action draft entered the proposing pipeline and reached L3 via `SubmitAction`.                                    |
| `AGENT_PROPOSAL_APPROVED`                | `STANDARD_24M` | S13.1 §16   | An AI-origin action received approval at `STANDARD` or `STRONG` strength.                                            |
| `AGENT_PROPOSAL_DENIED`                  | `EXTENDED_60M` | S13.1 §16   | An AI-origin action was denied at policy or approval; carries deny_reason_code.                                      |
| `AGENT_PLAN_BUNDLED_APPROVED`            | `STANDARD_24M` | S13.1 §16   | A plan was approved as a bundle at `STRONG` strength; carries plan_id, approval_bundle_hash.                         |
| `AGENT_PLAN_ABANDONED`                   | `EXTENDED_60M` | S13.1 §16   | A plan transitioned to `ABANDONED`; carries reason.                                                                  |
| `AGENT_MEMORY_WRITE`                     | `STANDARD_24M` | S13.1 §16   | The typed action `agent.memory.write` succeeded; payload bytes never logged (digest only).                           |
| `AGENT_MEMORY_READ`                      | `STANDARD_24M` | S13.1 §16   | The typed action `agent.memory.read` succeeded with privacy-class respect.                                           |
| `AGENT_MEMORY_CROSS_USER_DENIED`         | `FOREVER`      | S13.1 §16   | Agent attempted to read another user's `PRIVATE_TO_USER` memory; constitutional cross-user boundary fault.           |
| `AGENT_INTER_MESSAGE_SENT`               | `STANDARD_24M` | S13.1 §16   | The typed action `agent.coordinate.send` succeeded.                                                                  |
| `AGENT_INTER_MESSAGE_REJECTED`           | `EXTENDED_60M` | S13.1 §16   | An inter-agent message was denied; carries deny_reason.                                                              |
| `AGENT_SELF_GRADING_BLOCKED`             | `FOREVER`      | S13.1 §16   | INV-016 enforcement: agent attempted to grade an artifact authored by itself or its own kind.                        |
| `AGENT_DIRECT_FS_WRITE_BLOCKED`          | `FOREVER`      | S13.1 §16   | Agent attempted direct AIOS-FS write outside the proposing pipeline; INV-002 enforcement at the FS boundary.         |
| `AGENT_CROSS_GROUP_COORDINATION_BLOCKED` | `FOREVER`      | S13.1 §16   | Agent attempted cross-group coordination; INV-011 enforcement.                                                       |
| `AGENT_BACKEND_DEGRADED`                 | `EXTENDED_60M` | S13.1 §16   | A cognitive backend adapter became unavailable and a fallback was activated.                                         |
| `AGENT_PROMPT_INJECTION_DETECTED`        | `FOREVER`      | S13.1 §16   | The adversarial-input filter (S1.1 §17.1) fired on an utterance reaching this agent's INTENT_PERCEPTION.             |

Subsection retention split: `STANDARD_24M` × 7, `EXTENDED_60M` × 5, `FOREVER` × 6 (eighteen rows total).

### 27.9 From S12.2 Package Object Model (10 types)

Source: [S12.2 §11](../L6_Apps_Packages_Compatibility/02_package_model.md). Append authority is restricted to the L6 package-object engine for the lifecycle records and to the sandbox enforcer for the cross-package state corruption record. The five FOREVER entries cover the constitutional package-trust surface: rollback, quarantine, private-state corruption, version downgrade blocked (CVE-blocklist), and recovery-mode restore. Note: `PACKAGE_OBJECT_QUARANTINED` (this Wave 8) is semantically adjacent to `PACKAGE_QUARANTINED` (Wave 7 S11.1) but distinct — Wave 7's variant fires at the install-pipeline FSM transition; Wave 8's variant fires at the package-object engine's per-load detection. Both retained.

| RecordType                               | Retention      | Source spec | Purpose                                                                                                            |
| ---------------------------------------- | -------------- | ----------- | ------------------------------------------------------------------------------------------------------------------ |
| `PACKAGE_OBJECT_CREATED`                 | `STANDARD_24M` | S12.2 §11   | First-time installation of a package object completes (active or system; per-scope).                               |
| `PACKAGE_OBJECT_UPDATED`                 | `STANDARD_24M` | S12.2 §11   | A `STAGED_UPDATE` is promoted to `ACTIVE`; carries `from_version`, `to_version`, `installer_action_id`.            |
| `PACKAGE_OBJECT_ROLLED_BACK`             | `FOREVER`      | S12.2 §11   | Package transitions `ACTIVE → ROLLED_BACK`; carries reason, target version, blocklist consult result.              |
| `PACKAGE_OBJECT_QUARANTINED`             | `FOREVER`      | S12.2 §11   | Package transitions to `QUARANTINED`; carries reason (capability_lie / hash_mismatch / state_corruption / etc.).   |
| `PACKAGE_PRIVATE_STATE_INITIALIZED`      | `STANDARD_24M` | S12.2 §11   | First-launch initialization of `state/` completes.                                                                 |
| `PACKAGE_PRIVATE_STATE_CORRUPT_DETECTED` | `FOREVER`      | S12.2 §11   | Cross-subject write into `state/` detected; constitutional sandbox-boundary fault.                                 |
| `PACKAGE_VERSION_DOWNGRADE_BLOCKED`      | `FOREVER`      | S12.2 §11   | Rollback target rejected because the version is on the `RollbackBlocklist`; carries CVE reference, target version. |
| `PACKAGE_OBJECT_RETIRED`                 | `EXTENDED_60M` | S12.2 §11   | A `_rollback_<n>/` peer is retired (post-30d) or operator-driven uninstall.                                        |
| `PACKAGE_OBJECT_VERIFICATION_FAILED`     | `EXTENDED_60M` | S12.2 §11   | Per-load Merkle mismatch or staged-peer verification failure; carries reason and recomputed-vs-claimed hashes.     |
| `PACKAGE_RECOVERY_RESTORE_PERFORMED`     | `FOREVER`      | S12.2 §11   | `QUARANTINED` package was restored under recovery mode; carries operator id and target version.                    |

Subsection retention split: `STANDARD_24M` × 3, `EXTENDED_60M` × 2, `FOREVER` × 5 (ten rows total).

### 27.10 From S12.3 Compatibility Runtime (10 types)

Source: [S12.3 §11](../L6_Apps_Packages_Compatibility/03_compatibility_runtime.md). Append authority is restricted to the L6 compatibility orchestrator for the launch lifecycle records and to the kernel-level sandbox enforcers for the breakout / escape detection records. The three FOREVER entries cover the constitutional compatibility-runtime trust surface: Wine prefix breakout, Waydroid escape attempt, and orchestration-kind mismatch (manifest declared `EcosystemRuntime` ≠ orchestrator selected `OrchestrationKind` — silent disagreement is never permitted).

| RecordType                             | Retention      | Source spec | Purpose                                                                                                                       |
| -------------------------------------- | -------------- | ----------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `APP_LAUNCH_STARTED`                   | `STANDARD_24M` | S12.3 §11   | `app.launch` action transitioned to `executing`; orchestrator entered Step A.                                                 |
| `APP_LAUNCH_SUCCEEDED`                 | `STANDARD_24M` | S12.3 §11   | Step G passed; `LaunchOutcome = LAUNCHED`; envelope transitioned to `succeeded`.                                              |
| `APP_LAUNCH_FAILED`                    | `EXTENDED_60M` | S12.3 §11   | Any `LaunchOutcome ≠ LAUNCHED`; envelope transitioned to `failed`. Carries `FailureCategory` and `reason_id`.                 |
| `WINE_PREFIX_CREATED`                  | `STANDARD_24M` | S12.3 §11   | A new Wine prefix was created (Step F under `WINE_PREFIX_NEW`); carries `WinePrefixKind` and prefix path.                     |
| `WINE_PREFIX_BREAKOUT_ATTEMPTED`       | `FOREVER`      | S12.3 §11   | Kernel-level sandbox enforcer caught a Win32 binary trying to escape its prefix; constitutional sandbox-floor event.          |
| `WAYDROID_CONTAINER_STARTED`           | `STANDARD_24M` | S12.3 §11   | A Waydroid container started; carries `WaydroidIsolationLevel`.                                                               |
| `WAYDROID_ESCAPE_ATTEMPTED`            | `FOREVER`      | S12.3 §11   | LXC namespace boundary or AIOS group namespace caught a Waydroid-internal process trying to reach forbidden host/cross-group. |
| `KVM_VM_BOOTED`                        | `STANDARD_24M` | S12.3 §11   | A KVM guest reached the guest agent's "ready" handshake; carries `VMFallbackKind` and `ephemeral` flag.                       |
| `KVM_VM_TERMINATED`                    | `STANDARD_24M` | S12.3 §11   | A KVM guest was shut down; carries termination reason.                                                                        |
| `ORCHESTRATION_KIND_MISMATCH_REJECTED` | `FOREVER`      | S12.3 §11   | Selected `OrchestrationKind` disagrees with manifest's `EcosystemRuntime` or policy required value; constitutional refusal.   |

Subsection retention split: `STANDARD_24M` × 6, `EXTENDED_60M` × 1, `FOREVER` × 3 (ten rows total).

### 27.11 From S12.4 Compatibility Knowledge (8 types)

Source: [S12.4 §11](../L6_Apps_Packages_Compatibility/05_compatibility_knowledge.md). Append authority is restricted to the L6 compatibility-profile registry for ingestion / aggregation / outlier records and to the AIOS-root governance review path for farm-suspicion / visibility-downgrade records. The single FOREVER entry covers the reputation-farm detection surface (coordinated-burst detection per §9.2); a confirmed farm event is queued for AIOS-root review and contributors' weights held at `0.0`.

| RecordType                          | Retention      | Source spec | Purpose                                                                                                                |
| ----------------------------------- | -------------- | ----------- | ---------------------------------------------------------------------------------------------------------------------- |
| `PROFILE_CONTRIBUTED`               | `STANDARD_24M` | S12.4 §11   | An operator's `compat.contribute_profile_observation` action transitioned to `succeeded`.                              |
| `PROFILE_RATING_AGGREGATED`         | `STANDARD_24M` | S12.4 §11   | An aggregation step produced a per-dimension rating; carries before/after rating, drift, contribution counts.          |
| `PROFILE_OUTLIER_DETECTED`          | `EXTENDED_60M` | S12.4 §11   | A contribution was flagged as outlier and excluded from the active aggregate.                                          |
| `PROFILE_RECOMMENDATION_SHOWN`      | `STANDARD_24M` | S12.4 §11   | The L7 marketplace surface presented a profile-derived recommendation to the operator.                                 |
| `PROFILE_IMPORTED_FROM_UPSTREAM`    | `STANDARD_24M` | S12.4 §11   | A `compat.import_profile_from_upstream` action transitioned to `succeeded`; carries source, content hash, attribution. |
| `PROFILE_REPUTATION_FARM_SUSPECTED` | `FOREVER`      | S12.4 §11   | Farm detector flagged a coordinated cluster; AIOS-root review requested; contributors' weights held at `0.0`.          |
| `PROFILE_VISIBILITY_DOWNGRADED`     | `EXTENDED_60M` | S12.4 §11   | Profile contribution's effective visibility was reduced by registry action.                                            |
| `PROFILE_RETIRED`                   | `EXTENDED_60M` | S12.4 §11   | A profile transitioned to retired; carries `ProfileRetiredReason` and back-reference to surviving recipe(s).           |

Subsection retention split: `STANDARD_24M` × 4, `EXTENDED_60M` × 3, `FOREVER` × 1 (eight rows total).

### 27.12 From S7.6 CLI Renderer (10 types)

Source: [S7.6 §11](../L7_Interaction_Renderers/06_cli_renderer.md). Append authority is restricted to the L7 CLI renderer service for render lifecycle records, with `CLI_OPERATOR_AUTHENTICATED` co-emitted by the identity service. The four FOREVER entries cover the renderer's constitutional defence surface: recovery-kind misuse (e.g. `APP_SURFACE` requested in recovery TTY), auto-confirm via piped stdin (binds INV-009 — approvals are bound to one request and one approver), ANSI escape injection in non-renderer content, and trust-indicator reordering (a renderer-bug or in-process attacker reordering compiled trust-bearing nodes — TAMPER-class).

| RecordType                      | Retention      | Source spec | Purpose                                                                                                               |
| ------------------------------- | -------------- | ----------- | --------------------------------------------------------------------------------------------------------------------- |
| `CLI_RENDER_STARTED`            | `STANDARD_24M` | S7.6 §11    | Carries session_id, render mode (`CliRenderMode`), input mode (`CliInputMode`), ANSI level, term id.                  |
| `CLI_RENDER_FAILED`             | `EXTENDED_60M` | S7.6 §11    | Carries render_id, tree_id, result_code (`CliCompilationResult`), offending node id.                                  |
| `CLI_NODE_KIND_UNSUPPORTED`     | `STANDARD_24M` | S7.6 §11    | Compilation step found a node kind not supported by this renderer's compiler.                                         |
| `CLI_RECOVERY_KIND_REJECTED`    | `FOREVER`      | S7.6 §11    | Render attempted in `RECOVERY_TTY` mode included a forbidden node kind (`APP_SURFACE` / `STREAM_SURFACE` / `STREAM`). |
| `CLI_AUTO_CONFIRM_REJECTED`     | `FOREVER`      | S7.6 §11    | Approval response read from non-TTY stdin without pre-bound approval id; INV-009 enforcement.                         |
| `CLI_ANSI_INJECTION_BLOCKED`    | `FOREVER`      | S7.6 §11    | ANSI escape injection detected in non-renderer content; offending node replaced with `[content sanitized]`.           |
| `CLI_DEGRADED_NO_TTY`           | `STANDARD_24M` | S7.6 §11    | Renderer entered degraded mode (no isatty, no scrolling region, terminfo missing).                                    |
| `CLI_SCRIPTING_MODE_INVOKED`    | `STANDARD_24M` | S7.6 §11    | A scripting-mode session began; carries caller_subject_canonical_id, command_line_redacted_hash.                      |
| `CLI_OPERATOR_AUTHENTICATED`    | `STANDARD_24M` | S7.6 §11    | Operator authentication completed; carries operator_subject_canonical_id, recovery_session_id (if recovery).          |
| `CLI_TRUST_INDICATOR_REORDERED` | `FOREVER`      | S7.6 §11    | TAMPER-class: `SECURITY_INDICATOR` was compiled after non-trust-bearing content; renderer self-check failed.          |

Subsection retention split: `STANDARD_24M` × 5, `EXTENDED_60M` × 1, `FOREVER` × 4 (ten rows total).

### 27.13 From S8.3 Hardware Graph (14 types)

Source: [S8.3 §12](../L8_Network_Hardware_Devices/01_hardware_graph.md). Append authority is restricted to the L8 hardware-device-manager service (`_system:service:hardware-manager`) for graph / device lifecycle records, signed by its Ed25519 key. The five FOREVER entries cover the constitutional device-trust surface: device quarantine (lifecycle `* → QUARANTINED`), AI removable-device blocked (INV-013 device-plane enforcement), hardware-graph cross-boot drift (the evil-maid swap signal — the L0 invariant candidate `HARDWARE_GRAPH_DRIFT_FOREVER`), firmware version downgrade blocked (per-device monotonicity), and out-of-tree driver blocked (default refusal of unsigned/community kernel modules).

| RecordType                           | Retention      | Source spec | Purpose                                                                                                   |
| ------------------------------------ | -------------- | ----------- | --------------------------------------------------------------------------------------------------------- |
| `HARDWARE_GRAPH_REBUILT`             | `STANDARD_24M` | S8.3 §12    | Carries `snapshot_id`, `previous_snapshot_id`, `device_count`, `built_at`, `recovery_mode` flag.          |
| `DEVICE_DETECTED`                    | `STANDARD_24M` | S8.3 §12    | First-time observation of a device-identity tuple; carries class, vendor id, device id, bus kind.         |
| `DEVICE_DRIVER_BOUND`                | `STANDARD_24M` | S8.3 §12    | Successful driver bind; carries `driver_id`, `driver_provenance`, `trust_class`, `firmware_trusted` flag. |
| `DEVICE_DRIVER_REJECTED`             | `EXTENDED_60M` | S8.3 §12    | Driver bind refused; carries candidate `driver_id`, provenance, refusal reason.                           |
| `DEVICE_QUARANTINED`                 | `FOREVER`      | S8.3 §12    | Lifecycle transition into `QUARANTINED`; carries `DeviceQuarantineReason` and prior state.                |
| `DEVICE_DISCONNECTED`                | `STANDARD_24M` | S8.3 §12    | Hot-unplug or hard-disconnect; carries last lifecycle state before disconnect.                            |
| `REMOVABLE_DEVICE_REQUEST`           | `STANDARD_24M` | S8.3 §12    | Removable device awaiting approval; carries class, requesting subject id, model_string.                   |
| `REMOVABLE_DEVICE_APPROVED`          | `STANDARD_24M` | S8.3 §12    | Approval granted; carries policy class (`ALLOW_AUTO_THIS_BOOT` / `FOREVER` / per-group) and TTL.          |
| `REMOVABLE_DEVICE_DENIED`            | `EXTENDED_60M` | S8.3 §12    | Approval refused; carries refusal reason (recovery-deny / policy-deny / AI-blocked).                      |
| `AI_REMOVABLE_DEVICE_BLOCKED`        | `FOREVER`      | S8.3 §12    | INV-013 device-plane hard-deny fired; AI subject attempted a mutating HDM RPC.                            |
| `HARDWARE_GRAPH_DRIFT_DETECTED`      | `FOREVER`      | S8.3 §12    | Cross-boot drift detected; the evil-maid swap signal; carries added/removed/mutated sets.                 |
| `FIRMWARE_VERSION_DOWNGRADE_BLOCKED` | `FOREVER`      | S8.3 §12    | Firmware monotonicity violation; carries prior version, attempted version, source.                        |
| `IOMMU_DMA_PROTECTION_DEGRADED`      | `EXTENDED_60M` | S8.3 §12    | IOMMU absent or degraded; carries bus kind, host IOMMU global state.                                      |
| `OUT_OF_TREE_DRIVER_BLOCKED`         | `FOREVER`      | S8.3 §12    | Out-of-tree driver bind refused by default; recovery-mode override path is separate.                      |

Subsection retention split: `STANDARD_24M` × 6, `EXTENDED_60M` × 3, `FOREVER` × 5 (fourteen rows total). **Charter mismatch note:** S8.3 §12 narrative claims "FOREVER count grows by 6"; direct row-by-row recount of §12 yields 5 FOREVER entries (`DEVICE_QUARANTINED`, `AI_REMOVABLE_DEVICE_BLOCKED`, `HARDWARE_GRAPH_DRIFT_DETECTED`, `FIRMWARE_VERSION_DOWNGRADE_BLOCKED`, `OUT_OF_TREE_DRIVER_BLOCKED`). The narrative `HOST_CAPABILITY_LIE` mentioned in S8.3 §3 is owned by S8.2 (GPU resource model) and is not in S8.3's queued vocabulary table; the truthful Wave-8 count for S8.3 is 5 FOREVER, not 6.

### 27.14 From S8.4 DNS / VPN Management (12 types)

Source: [S8.4 §13](../L8_Network_Hardware_Devices/03_dns_vpn_management.md). Append authority is restricted to the L8 DnsVpnService process; forgery from any other subject is hard-denied at S3.1 and emits `TAMPER_DETECTED`. The six FOREVER entries cover the constitutional resolver-trust and VPN-trust surfaces: rebinding detection, plain-DNS attempt, resolver substitution attempt, VPN provider key rotation (legitimate; carries old/new BLAKE3), VPN provider key forgery (rotation rejected on Ed25519 fail), and mDNS poisoning (response IP outside LAN_SUBNET).

| RecordType                           | Retention      | Source spec | Purpose                                                                                                            |
| ------------------------------------ | -------------- | ----------- | ------------------------------------------------------------------------------------------------------------------ |
| `DNS_QUERY_PERFORMED`                | `STANDARD_24M` | S8.4 §13    | Every successfully evaluated DNS query; FQDN auditable; **no payload** (answer set never recorded).                |
| `DNS_RESOLVER_REBINDING_DETECTED`    | `FOREVER`      | S8.4 §13    | Response returned IPs outside the pinned set; FQDN entry transitions to `AWAITING_OPERATOR`.                       |
| `DNS_PLAIN_BLOCKED`                  | `FOREVER`      | S8.4 §13    | Subject attempted UDP/53 or TCP/53 in the clear; kernel filter dropped.                                            |
| `DNS_RESOLVER_SUBSTITUTION_REJECTED` | `FOREVER`      | S8.4 §13    | Subject attempted to register an out-of-allowlist resolver (config write, mount, RES_OPTIONS).                     |
| `VPN_TUNNEL_ESTABLISHED`             | `STANDARD_24M` | S8.4 §13    | A `VpnTunnel` reached `ACTIVE`; carries kind, peer endpoint class, approver chain.                                 |
| `VPN_TUNNEL_FAILED`                  | `EXTENDED_60M` | S8.4 §13    | Tunnel transitioned to `FAILED` (kill-switch, peer unreachable, key handshake failure).                            |
| `VPN_PROVIDER_KEY_ROTATED`           | `FOREVER`      | S8.4 §13    | Successful key rotation for a tunnel; carries old/new key BLAKE3 and signing identity.                             |
| `VPN_PROVIDER_KEY_FORGERY_REJECTED`  | `FOREVER`      | S8.4 §13    | A `RotateVpnPeerKey` attempt failed Ed25519 verification.                                                          |
| `MDNS_REQUEST_RECEIVED`              | `STANDARD_24M` | S8.4 §13    | A subject submitted `MdnsResolveInstance`; carries service type, instance name class, outcome.                     |
| `MDNS_BROADCAST_DENIED`              | `EXTENDED_60M` | S8.4 §13    | Advertisement denied (posture, expired grant, recovery-suspended).                                                 |
| `MDNS_POISONING_DETECTED`            | `FOREVER`      | S8.4 §13    | mDNS response IP fell outside the interface's LAN_SUBNET.                                                          |
| `RESOLVER_BACKEND_DEGRADED`          | `EXTENDED_60M` | S8.4 §13    | `ResolverBackend` transitioned to `DEGRADED_HOSTS_FILE_ONLY` (signature failure, all upstreams unreachable, etc.). |

Subsection retention split: `STANDARD_24M` × 3, `EXTENDED_60M` × 3, `FOREVER` × 6 (twelve rows total).

### 27.15 From S8.5 Firmware Trust (12 types)

Source: [S8.5 §13](../L8_Network_Hardware_Devices/04_firmware_trust.md). Append authority is restricted to the `_system:service:firmware-update` subject scope. The six FOREVER entries cover the constitutional firmware-trust surface — code that runs **below** the kernel: monotonicity-violation downgrade attempt, unsigned firmware refusal, vendor deplatform, post-apply rollback (always FOREVER because the pre-rollback firmware ran for some interval), tamper detection on cross-boot version drift (evil-maid firmware-only swap), and operator-local-signed install (the explicit operator-as-final-authority path with hardware-key-witness disclosure).

| RecordType                          | Retention      | Source spec | Purpose                                                                                                             |
| ----------------------------------- | -------------- | ----------- | ------------------------------------------------------------------------------------------------------------------- |
| `FIRMWARE_UPDATE_REQUESTED`         | `STANDARD_24M` | S8.5 §13    | Operator-authored `firmware.update.request` accepted at envelope validation.                                        |
| `FIRMWARE_VERIFICATION_PASSED`      | `STANDARD_24M` | S8.5 §13    | Stage 2 returned `VERIFIED_LVFS` / `VERIFIED_VENDOR_SIGNATURE` / `VERIFIED_DISTRIBUTION`.                           |
| `FIRMWARE_VERIFICATION_FAILED`      | `EXTENDED_60M` | S8.5 §13    | Stage 2 returned a non-FOREVER failure (signature, hash mismatch, scope mismatch, device absent).                   |
| `FIRMWARE_DOWNGRADE_BLOCKED`        | `FOREVER`      | S8.5 §13    | Stage 2 monotonicity check rejected; carries proposed and high-water versions.                                      |
| `FIRMWARE_UNSIGNED_REJECTED`        | `FOREVER`      | S8.5 §13    | Stage 2 class derivation returned `UNSIGNED_BLACKLISTED`; the override path was not taken.                          |
| `FIRMWARE_VENDOR_DEPLATFORMED`      | `FOREVER`      | S8.5 §13    | Stage 2 detected vendor in S11.1 `DEPLATFORMED` state.                                                              |
| `FIRMWARE_APPLIED`                  | `STANDARD_24M` | S8.5 §13    | Stage 7 commit succeeded; carries class, scope, prior/new versions, apply strategy, approver id.                    |
| `FIRMWARE_APPLY_FAILED`             | `EXTENDED_60M` | S8.5 §13    | Stage 5 apply step failed (write error, capsule reject, driver reload error).                                       |
| `FIRMWARE_ROLLBACK_PERFORMED`       | `FOREVER`      | S8.5 §13    | Stage 7 rollback path succeeded (or failed with `rollback_impossible = true`); carries strategy and outcome.        |
| `BIOS_UEFI_UPDATE_DEFERRED`         | `EXTENDED_60M` | S8.5 §13    | Stage 4 deferred a `BIOS_UEFI` update; operator-visibility need is higher for this scope.                           |
| `FIRMWARE_TAMPER_DETECTED`          | `FOREVER`      | S8.5 §13    | Boot-time hardware-graph drift detection observed `firmware_version` mismatch; forces recovery entry.               |
| `OPERATOR_LOCAL_FIRMWARE_INSTALLED` | `FOREVER`      | S8.5 §13    | Stage 7 commit of an `OPERATOR_LOCAL_SIGNED` firmware update; carries hardware-key serial hash and disclosure hash. |

Subsection retention split: `STANDARD_24M` × 3, `EXTENDED_60M` × 3, `FOREVER` × 6 (twelve rows total).

### 27.16 From S14.2 Telemetry Pipeline (10 types)

Source: [S14.2 §13](04_telemetry_pipeline.md). Append authority is restricted to the L9 telemetry pipeline service for the registration / rate-tier / probe-load records and to the redaction / cardinality / sanitizer enforcers for the violation records. The four FOREVER entries cover the constitutional telemetry-trust surface: cardinality breach (label-explosion attempt), redaction failure (secret-shaped content reached the emission boundary), log-line injection (escape-sequence injection caught by the sanitizer), and eBPF probe rejected (probe outside the closed catalog, overhead-budget exceeded, or AI subject attempting load).

| RecordType                          | Retention      | Source spec | Purpose                                                                                                        |
| ----------------------------------- | -------------- | ----------- | -------------------------------------------------------------------------------------------------------------- |
| `TELEMETRY_PIPELINE_STARTED`        | `STANDARD_24M` | S14.2 §13   | A signal registration was accepted (or rejected with `outcome = REJECTED`).                                    |
| `TELEMETRY_CARDINALITY_BREACH`      | `FOREVER`      | S14.2 §13   | A signal exceeded its `CardinalityBudget` and AUTO_DEMOTE triggered.                                           |
| `TELEMETRY_REDACTION_FAILED`        | `FOREVER`      | S14.2 §13   | The redaction layer rejected an emission; the data point is dropped before storage.                            |
| `TELEMETRY_BACKEND_UNAVAILABLE`     | `EXTENDED_60M` | S14.2 §13   | A backend (Prometheus, Loki, OTLP collector) became unreachable; rate-limited.                                 |
| `TELEMETRY_BACKEND_DEGRADED`        | `EXTENDED_60M` | S14.2 §13   | A backend is reachable but experiencing backpressure (`SCRAPE_SLOW` / `INGEST_REJECTED` / `RING_BUFFER_FULL`). |
| `TELEMETRY_LOG_INJECTION_DETECTED`  | `FOREVER`      | S14.2 §13   | The log-line escape sanitizer rejected an emission; offending content not stored.                              |
| `TELEMETRY_RETENTION_TIER_PROMOTED` | `STANDARD_24M` | S14.2 §13   | A signal's `RetentionTier` was changed; carries previous and new tier.                                         |
| `TELEMETRY_SAMPLING_RATE_ADJUSTED`  | `STANDARD_24M` | S14.2 §13   | A signal's sampling rate was changed; carries previous/new rate and reason code.                               |
| `TELEMETRY_EBPF_PROBE_LOADED`       | `STANDARD_24M` | S14.2 §13   | An eBPF probe from the closed catalog was loaded; carries probe template and cumulative overhead.              |
| `TELEMETRY_EBPF_PROBE_REJECTED`     | `FOREVER`      | S14.2 §13   | An eBPF probe load was rejected (outside catalog, overhead exceeded, AI subject attempting load).              |

Subsection retention split: `STANDARD_24M` × 4, `EXTENDED_60M` × 2, `FOREVER` × 4 (ten rows total). **Charter mismatch note:** S14.2 §13 narrative declares "S3.1 §24 noted 87; this brings the running total to **97 entries**." This claim is internally inconsistent with the L9.1 evidence-log running total tracked here (post-Wave 7 = 205 narrative entries; S14.2's "87" appears to reference an older cumulative snapshot). The truthful Wave-8 contribution from S14.2 is 10 rows; the running cumulative narrative total after Wave 8 is computed in §27.19 from the L9.1 ledger, not from S14.2's stale snapshot.

### 27.17 From S11.2 Marketplace (12 types)

Source: [S11.2 §12](../L10_Distribution_Ecosystem_Marketplace/02_marketplace.md). Append authority is restricted to the L10 marketplace state engine for the publisher-onboarding / capability-review / listing records, mirrored against S11.1 for the `PUBLISHER_ONBOARDING_DEPLATFORMED` event. The six FOREVER entries cover the constitutional marketplace-trust surface: publisher onboarding outcomes (approved / rejected / deplatformed — all three are constitutional events for the trust catalog); capability-review deceptive rejection (the deceptive-justification audit trail); listing-vs-manifest mismatch (the bait-and-switch detection point); and review-bypass attempt (the audit-side detection that fires when an install-time manifest contains capabilities that should have been caught at review).

| RecordType                                   | Retention      | Source spec | Purpose                                                                                                             |
| -------------------------------------------- | -------------- | ----------- | ------------------------------------------------------------------------------------------------------------------- |
| `PUBLISHER_ONBOARDING_APPLICATION_SUBMITTED` | `STANDARD_24M` | S11.2 §12   | `APPLICATION_SUBMITTED` state entered after successful field validation.                                            |
| `PUBLISHER_ONBOARDING_IDENTITY_VERIFIED`     | `EXTENDED_60M` | S11.2 §12   | `IDENTITY_VERIFICATION_PENDING → TECHNICAL_REVIEW` transition.                                                      |
| `PUBLISHER_ONBOARDING_APPROVED`              | `FOREVER`      | S11.2 §12   | `SECURITY_REVIEW → APPROVED_VERIFIED`; catalog entry written; mirrors S11.1 onboarding-approval signal.             |
| `PUBLISHER_ONBOARDING_REJECTED`              | `FOREVER`      | S11.2 §12   | Any review stage → `REJECTED`; mirrors S11.1 rejection signal.                                                      |
| `PUBLISHER_ONBOARDING_DEPLATFORMED`          | `FOREVER`      | S11.2 §12   | `APPROVED_VERIFIED → DEPLATFORMED`; mirrors S11.1 `PUBLISHER_DEPLATFORMED` for FSM symmetry.                        |
| `CAPABILITY_REVIEW_REQUESTED`                | `STANDARD_24M` | S11.2 §12   | `TECHNICAL_REVIEW` entered; one record per draft manifest summarising the capability count.                         |
| `CAPABILITY_REVIEW_APPROVED`                 | `EXTENDED_60M` | S11.2 §12   | Per-capability `APPROVED_AS_DECLARED` or `APPROVED_WITH_NARROWED_SCOPE` outcome.                                    |
| `CAPABILITY_REVIEW_DECEPTIVE_REJECTED`       | `FOREVER`      | S11.2 §12   | Per-capability `REJECTED_DECEPTIVE` outcome; feeds publisher's `capability_lie_history_count`.                      |
| `LISTING_PUBLISHED`                          | `STANDARD_24M` | S11.2 §12   | `Listing.visibility` transitions from unset to any visible state; canonical hash recorded.                          |
| `LISTING_VISIBILITY_DOWNGRADED`              | `EXTENDED_60M` | S11.2 §12   | Visibility transitions toward `DEPRECATED_VIEWABLE` or `RETIRED_HIDDEN`; reason recorded.                           |
| `LISTING_LISTING_VS_MANIFEST_MISMATCH`       | `FOREVER`      | S11.2 §12   | Listing-vs-manifest cross-check failed at publication or at install; constitutional bait-and-switch event.          |
| `MARKETPLACE_REVIEW_BYPASS_ATTEMPTED`        | `FOREVER`      | S11.2 §12   | Install pipeline detected a manifest that should have been caught at review; constitutional review-bypass forensic. |

Subsection retention split: `STANDARD_24M` × 3, `EXTENDED_60M` × 3, `FOREVER` × 6 (twelve rows total).

### 27.18 From S11.3 External Integrations (12 types)

Source: [S11.3 §13](../L10_Distribution_Ecosystem_Marketplace/03_external_integrations.md). Append authority is restricted to the L10 external-bridge service for fetch / repackage / metadata / recipe records, with parallel emissions from the bridge admission pipeline for deceptive-claim / signature-failure / blacklist records. The four FOREVER entries cover the constitutional bridge-trust surface: upstream signature failure (constitutionally inadmissible per §I3 — no operator override), deceptive trust-class claim in upstream metadata (permanent rejection, keyed on upstream content hash), bridge auto-blacklisted (per-source reputation collapse), and trust-class deception detected (the deceptive-claim audit trail).

| RecordType                              | Retention      | Source spec | Purpose                                                                                                  |
| --------------------------------------- | -------------- | ----------- | -------------------------------------------------------------------------------------------------------- |
| `BRIDGE_FETCH_STARTED`                  | `STANDARD_24M` | S11.3 §13   | Bridge fetch operation started.                                                                          |
| `BRIDGE_FETCH_COMPLETED`                | `STANDARD_24M` | S11.3 §13   | Bridge fetch completed successfully; carries upstream content hash and byte count.                       |
| `BRIDGE_UPSTREAM_SIGNATURE_VERIFIED`    | `STANDARD_24M` | S11.3 §13   | Upstream signature verification passed; carries upstream signing key id and signature timestamp.         |
| `BRIDGE_UPSTREAM_SIGNATURE_FAILED`      | `FOREVER`      | S11.3 §13   | Upstream signature failed or upstream is `UNSIGNED_REJECTED`; constitutionally inadmissible.             |
| `BRIDGE_REPACKAGED_WITH_AIOS_KEY`       | `STANDARD_24M` | S11.3 §13   | Bridge synthesised an AIOS manifest at `COMMUNITY` trust and signed with the AIOS bridge per-source key. |
| `BRIDGE_DECEPTIVE_REJECTED`             | `FOREVER`      | S11.3 §13   | Bridge admission rejected with `REJECTED_DECEPTIVE`; carries sub-reason and offending field.             |
| `BRIDGE_RATE_LIMIT_EXCEEDED`            | `EXTENDED_60M` | S11.3 §13   | Bridge accumulated `≥ 3` deferrals in a 1-hour window.                                                   |
| `BRIDGE_METADATA_IMPORTED`              | `STANDARD_24M` | S11.3 §13   | Metadata-only import completed; carries `MetadataAttribution`.                                           |
| `BRIDGE_RECIPE_IMPORTED`                | `STANDARD_24M` | S11.3 §13   | Recipe import completed; carries upstream attribution and recipe canonical id.                           |
| `BRIDGE_BLACKLISTED`                    | `FOREVER`      | S11.3 §13   | Bridge auto-blacklisted; carries source, trigger condition, counter snapshot.                            |
| `BRIDGE_DEGRADED_UPSTREAM_UNAVAILABLE`  | `EXTENDED_60M` | S11.3 §13   | Bridge fetch failed after retry budget exhausted; carries source, upstream URL, last error.              |
| `BRIDGE_TRUST_CLASS_DECEPTION_DETECTED` | `FOREVER`      | S11.3 §13   | Deceptive-claim check detected an AIOS-side trust-class claim in upstream metadata; pattern matched.     |

Subsection retention split: `STANDARD_24M` × 6, `EXTENDED_60M` × 2, `FOREVER` × 4 (twelve rows total).

### 27.19 Reconciliation note (truthful arithmetic)

Per §24's narrative-total counting pattern, this Wave 8 records **195 unique `RecordType` additions** across eighteen source contracts with no exact-name collisions against the prior Wave-1..7 vocabulary (synonym-adjacency notes are recorded in §27.20). The retention-class distribution across these 195 additions, counted directly from §27.1 through §27.18, is:

| Source                  | `STANDARD_24M` | `EXTENDED_60M` | `FOREVER` |   Total |
| ----------------------- | -------------: | -------------: | --------: | ------: |
| §27.1 S9.2 first-boot   |              1 |              0 |        10 |      11 |
| §27.2 S14.1 failures    |              4 |              2 |         4 |      10 |
| §27.3 S6.3 receipt      |              0 |              0 |         4 |       4 |
| §27.4 S15.1 unit        |              4 |              3 |         1 |       8 |
| §27.5 S15.2 transitions |              7 |              2 |         3 |      12 |
| §27.6 S15.3 adapter     |              4 |              2 |         4 |      10 |
| §27.7 S13.2 router      |              4 |              6 |         2 |      12 |
| §27.8 S13.1 cog-core    |              7 |              5 |         6 |      18 |
| §27.9 S12.2 package     |              3 |              2 |         5 |      10 |
| §27.10 S12.3 compat-rt  |              6 |              1 |         3 |      10 |
| §27.11 S12.4 compat-kn  |              4 |              3 |         1 |       8 |
| §27.12 S7.6 CLI         |              5 |              1 |         4 |      10 |
| §27.13 S8.3 hw-graph    |              6 |              3 |         5 |      14 |
| §27.14 S8.4 dns/vpn     |              3 |              3 |         6 |      12 |
| §27.15 S8.5 firmware    |              3 |              3 |         6 |      12 |
| §27.16 S14.2 telemetry  |              4 |              2 |         4 |      10 |
| §27.17 S11.2 market     |              3 |              3 |         6 |      12 |
| §27.18 S11.3 ext-int    |              6 |              2 |         4 |      12 |
| **Wave 8 totals**       |         **74** |         **43** |    **78** | **195** |

Total: 74 + 43 + 78 = 195 unique additions. Subsection-by-subsection sum of "Total": 11+10+4+8+12+10+12+18+10+10+8+10+14+12+12+10+12+12 = 195 ✓.

New cumulative narrative total: **205 (post-Wave 7) + 195 (Wave 8) = 400 entries**. (Note: S6.3 contract self-stated "209 narrative entries" by accounting only for its own four additions on top of 205; that arithmetic is a strict subset of the 400 computed here, not a contradiction.)

> **Counting note vs charter expectations.** Two charter-vs-source mismatches were detected and called out at the subsection level:
>
> 1. **S8.3 (§27.13)** — source narrative claims "FOREVER count grows by 6"; row-by-row recount yields 5 FOREVER entries. The truthful Wave-8 contribution is 5 FOREVER from S8.3, not 6.
> 2. **S14.2 (§27.16)** — source narrative claims a cumulative running total of "97 entries" referencing an older S3.1 baseline of 87; the L9.1 cumulative ledger tracked here was 159 post-Wave 6 and 205 post-Wave 7. S14.2's snapshot is stale by multiple Waves; its 10-row contribution is correct, its cumulative claim is not.
>
> Per §26.4's discipline, the per-class arithmetic in the table above is the source-of-truth for L9.1; the source-spec narratives that disagree are recorded as charter mismatches and not propagated.

Per §23 / §24 / §25 / §26's narrative-only declaration pattern, this Wave 8 does **not** edit Appendix A. Full IDL reconciliation — addition of the 195 new payload messages to the discriminated `RecordPayload` oneof — is a separate sweep when the spec is next refined.

### 27.20 Telemetry impact

Each new FOREVER record type contributes to the FOREVER retention storage class summarised in §6.4. Wave 8 introduces **78 new FOREVER record types** (per §27.19 column total). Cumulative FOREVER narrative entries through Wave 8: 65 (post-Wave 7) + 78 (Wave 8) = **143 narrative FOREVER entries**. The §20 per-`record_type` cardinality reservation is bumped from 205 to 400 entries narratively. Existing histogram and counter labels remain valid; subject, group, and channel ids are never labels — they would inflate cardinality unboundedly and would re-introduce subject identity into the metrics surface that §20 forbids.

The new FOREVER surface in Wave 8 is dominated by four constitutional axes:

- **First-boot constitutional commits** (S9.2 × 10) — the ten constitutional events a host emits during first-boot establish the host's lifetime audit trail. Volume is bounded to one set per host lifetime plus one per reset-to-factory operation.
- **Trust-boundary refusals** (S15.3 adapter × 4, S12.2 package × 5, S11.2 marketplace × 6, S11.3 bridge × 4 = 19) — the points where the trust chain rejects manifests / packages / publishers / bridges. Volume is bounded by adversary attempt rate.
- **Hardware / firmware / receipt audit-chain anomalies** (S8.3 × 5, S8.5 × 6, S6.3 × 4 = 15) — drift, downgrade, tamper, forgery, lineage cycles, sequence anomalies. Constitutional forensic events; volume is bounded by adversarial attack surface.
- **AI / cognitive boundary enforcement** (S13.1 × 6, S13.2 × 2, S14.2 × 4, S7.6 × 4 = 16) — INV-002 / INV-011 / INV-013 / INV-016 enforcement at the cognitive, telemetry, and renderer layers; prompt injection, response signature failure, eBPF probe rejection, ANSI injection. Volume is bounded by adversary attack rate.

These are the four most security-sensitive surfaces in the host post-installation, so the FOREVER share of Wave 8 (78 / 195 = 40 %) is structural, not accidental.

### 27.21 Cross-spec impact note (queued for separate consolidations)

This Wave 8 records only the `RecordType` consolidation. Other items queued by the eighteen source contracts are **out of scope** for this Wave and are deferred to separate sweeps:

- **Synonym / adjacency notes** (recorded narrative-only here; no name changes proposed):
  - `UNIT_DEPENDENCY_CYCLE_DETECTED` (S15.1, admission-time) and `DEPENDENCY_CYCLE_DETECTED` (S15.2, evaluation-time) are semantically adjacent but distinct record-name bindings. Both retained.
  - `PACKAGE_OBJECT_QUARANTINED` (Wave 8 S12.2, package-engine per-load detection) and `PACKAGE_QUARANTINED` (Wave 7 S11.1, install-pipeline FSM transition) are semantically adjacent but distinct. Both retained.
  - `MODEL_CALL` (original Appendix A, line 1155) is a coarse-grained legacy record that the twelve S13.2 names (§27.7) functionally replace. Reconciliation deferred to the IDL sweep.
  - `ORCHESTRATION_KIND_MISMATCH_REJECTED` (Wave 8 S12.3) and `APP_ECOSYSTEM_RUNTIME_BREAKOUT_ATTEMPTED` (Wave 7 S12.1) cover related-but-distinct surfaces (orchestration-kind disagreement vs runtime-breakout). Both retained.
- **New typed actions** queued for the S10.1 Capability Runtime catalog: at least `firmware.update.request` (S8.5), `RotateVpnPeerKey` (S8.4), `compat.contribute_profile_observation` (S12.4), `compat.import_profile_from_upstream` (S12.4), `app.launch` (S12.3), `agent.coordinate.send` / `agent.memory.read` / `agent.memory.write` / `agent.grade.attempt` / `external_model_call` (S13.1 / S13.2), plus the SGR `unit.start` / `unit.rollback` / `runtime.adapter.register` / `runtime.adapter.retire` (S15.1 / S15.3) and the marketplace / bridge actions (S11.2 / S11.3). Consolidated into S10.1 separately.
- **New fields** queued for the S3.2 `SandboxProfile` shape and the S8.3 hardware-graph snapshot shape; handled by the respective orchestrators.
- **Candidate L0 invariants** queued narrative-only by these contracts: `NETWORK_DEFAULT_DENY_OUTBOUND` (S8.1, carried forward from Waves 6 and 7 and still pending), `PACKAGE_TRUST_CHAIN_BOUND` (S11.1 §19, carried from Wave 7), `ECOSYSTEM_HONESTY_DISCLOSURE` (S12.1, carried from Wave 7), `HARDWARE_GRAPH_DRIFT_FOREVER` (S8.3 §I6 / §12.2), and the four firmware-update scopes (`CPU_MICROCODE`, `GPU_FIRMWARE`, `TPM_FIRMWARE`, `BIOS_UEFI`) queued for `NonOverridableClass` extension by S8.5 §9. Per L0 §3 I1, invariant catalog mutation is a versioned spec change and recovery-mode invariant-bundle update — these are held for the audit-phase L0 sweep per the project owner's "deliberate single-purpose constitutional act" pattern and are **not** promoted in this Wave.
- **No-records sources:** all eighteen sources contributed at least one queued `RecordType`. None were skipped; S6.3 is included despite being primarily a receipt-envelope schema because §12 of that contract explicitly queues four FOREVER record types for the L9.1 vocabulary.

## 28. See also

- [S0.1 Action Envelope + Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S2.4 Verification Grammar](02_verification_grammar.md)
- [S2.3 Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S2.2 AIOS-FS Implementation Space](../L2_AIOS_FS/04_implementation_space.md)
- [S4.1 Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S5.2 Vault Broker](../L4_Policy_Identity_Vault/02_vault_broker.md)
- [S5.3 Approval Mechanics](../L4_Policy_Identity_Vault/04_approval_mechanics.md)
- [S5.4 Emergency Override](../L4_Policy_Identity_Vault/05_emergency_override.md)
- [S6.3 Evidence Receipt Schema](../L0_Governance_Evidence_Safety/03_evidence_receipt_schema.md)
- [S7.1 Surface Composition](../L7_Interaction_Renderers/01_surface_composition.md)
- [S7.2 Shared UI Schema](../L7_Interaction_Renderers/02_shared_ui_schema.md)
- [S7.3 Visual Language](../L7_Interaction_Renderers/03_visual_language.md)
- [S7.4 KDE Renderer](../L7_Interaction_Renderers/04_kde_renderer.md)
- [S7.5 Web Renderer](../L7_Interaction_Renderers/05_web_renderer.md)
- [S7.6 CLI Renderer](../L7_Interaction_Renderers/06_cli_renderer.md)
- [S8.1 Network Policy](../L8_Network_Hardware_Devices/02_network_policy.md)
- [S8.2 GPU Resource Model](../L8_Network_Hardware_Devices/05_gpu_resource_model.md)
- [S8.3 Hardware Graph](../L8_Network_Hardware_Devices/01_hardware_graph.md)
- [S8.4 DNS / VPN Management](../L8_Network_Hardware_Devices/03_dns_vpn_management.md)
- [S8.5 Firmware Trust](../L8_Network_Hardware_Devices/04_firmware_trust.md)
- [S9.1 Recovery Boundary](../L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md)
- [S9.2 First-Boot Flow](../L1_Kernel_Bootstrap_Recovery/02_first_boot_flow.md)
- [S9.3 Dedicated Kernel Pipeline](../L1_Kernel_Bootstrap_Recovery/03_dedicated_kernel_pipeline.md)
- [S10.1 Capability Runtime gRPC](../L3_AIOS_SGR_Service_Graph_Runtime/03_capability_runtime_grpc.md)
- [S11.1 Repository Model](../L10_Distribution_Ecosystem_Marketplace/01_repository_model.md)
- [S11.2 Marketplace](../L10_Distribution_Ecosystem_Marketplace/02_marketplace.md)
- [S11.3 External Integrations](../L10_Distribution_Ecosystem_Marketplace/03_external_integrations.md)
- [S12.1 App Runtime Model](../L6_Apps_Packages_Compatibility/01_app_runtime_model.md)
- [S12.2 Package Object Model](../L6_Apps_Packages_Compatibility/02_package_model.md)
- [S12.3 Compatibility Runtime](../L6_Apps_Packages_Compatibility/03_compatibility_runtime.md)
- [S12.4 Compatibility Knowledge](../L6_Apps_Packages_Compatibility/05_compatibility_knowledge.md)
- [S13.1 Cognitive Core Model](../L5_Cognitive_Core/01_cognitive_core_model.md)
- [S13.2 Cognitive Model Router](../L5_Cognitive_Core/05_model_router.md)
- [S14.1 Failure Handling](03_failure_handling.md)
- [S14.2 Telemetry Pipeline](04_telemetry_pipeline.md)
- [S15.1 Unit Manifest](../L3_AIOS_SGR_Service_Graph_Runtime/01_unit_manifest.md)
- [S15.2 SGR State Transitions](../L3_AIOS_SGR_Service_Graph_Runtime/02_state_transitions.md)
- [S15.3 SGR Adapter Model](../L3_AIOS_SGR_Service_Graph_Runtime/04_adapter_model.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

## Appendix A: Complete proto IDL

```proto
syntax = "proto3";
package aios.evidence.v1alpha1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/struct.proto";
import "google/protobuf/empty.proto";

// ─────────────────────────────────────────────────────────────────
// Receipt envelope
// ─────────────────────────────────────────────────────────────────

message EvidenceReceipt {
  string receipt_id = 1;
  google.protobuf.Timestamp recorded_at = 2;
  RecordType record_type = 3;
  string subject = 4;
  string action_id = 5;
  string policy_decision_id = 6;
  string verification_id = 7;
  string correlation_id = 8;
  string trace_id = 9;
  string segment_id = 10;
  uint64 sequence_number = 11;
  string payload_hash = 12;
  string payload_ref = 13;
  string redaction_profile = 14;
  string previous_receipt_hash = 15;
  bool simulated = 16;
  RecordPayload payload = 17;
}

enum RecordType {
  RECORD_TYPE_UNSPECIFIED          = 0;
  ACTION_RECEIVED                   = 1;
  TRANSLATION_CREATED               = 2;
  ROUTING_DECISION                  = 3;
  POLICY_DECISION                   = 4;
  APPROVAL_REQUESTED                = 5;
  APPROVAL_GRANTED                  = 6;
  APPROVAL_DENIED                   = 7;
  EXECUTION_STARTED                 = 8;
  EXECUTION_COMPLETED               = 9;
  VERIFICATION_RESULT               = 10;
  ROLLBACK_COMPLETED                = 11;
  RECOVERY_EVENT                    = 12;
  MODEL_CALL                        = 13;
  CHAIN_CHECKPOINT                  = 14;
  GC_PASS                           = 15;
  QUARANTINE_EVENT                  = 16;
  CONFLICT_EVENT                    = 17;
  EMERGENCY_OVERRIDE_GRANT          = 18;
  POLICY_BUNDLE_LOAD                = 19;
  SEGMENT_SEALED                    = 20;
  CHAIN_INCONSISTENCY_DETECTED      = 21;
  TAMPER_DETECTED                   = 22;
}

// ─────────────────────────────────────────────────────────────────
// Payload one-of (selected schemas; the remainder follow the same pattern)
// ─────────────────────────────────────────────────────────────────

message RecordPayload {
  oneof payload {
    ActionReceivedPayload          action_received       = 1;
    TranslationCreatedPayload      translation_created   = 2;
    RoutingDecisionPayload         routing_decision      = 3;
    PolicyDecisionPayload          policy_decision       = 4;
    ApprovalRequestedPayload       approval_requested    = 5;
    ApprovalGrantedPayload         approval_granted      = 6;
    ApprovalDeniedPayload          approval_denied       = 7;
    ExecutionStartedPayload        execution_started     = 8;
    ExecutionCompletedPayload      execution_completed   = 9;
    VerificationResultPayload      verification_result   = 10;
    RollbackCompletedPayload       rollback_completed    = 11;
    RecoveryEventPayload           recovery_event        = 12;
    ModelCallPayload               model_call            = 13;
    ChainCheckpointPayload         chain_checkpoint      = 14;
    GcPassPayload                  gc_pass               = 15;
    QuarantineEventPayload         quarantine_event      = 16;
    ConflictEventPayload           conflict_event        = 17;
    EmergencyOverrideGrantPayload  emergency_override    = 18;
    PolicyBundleLoadPayload        policy_bundle_load    = 19;
    SegmentSealedPayload           segment_sealed        = 20;
    ChainInconsistencyPayload      chain_inconsistency   = 21;
    TamperDetectedPayload          tamper_detected       = 22;
  }
}

message ActionReceivedPayload {
  string action_id = 1;
  string envelope_hash = 2;
  string subject = 3;
  string action = 4;
  string adapter_id = 5;
  string privacy_class = 6;
}

message TranslationCreatedPayload {
  string translation_id = 1;
  string intent_id = 2;
  string plan_id = 3;
  string status = 4;
  uint32 action_drafts_count = 5;
}

message RoutingDecisionPayload {
  string routing_id = 1;
  string tier = 2;
  string outcome = 3;
  string reason_code = 4;
}

message PolicyDecisionPayload {
  string policy_decision_id = 1;
  string action_id = 2;
  string decision = 3;
  string reason_code = 4;
  string bundle_version = 5;
  string enrichment_snapshot_id = 6;
  uint32 rules_consulted = 7;
}

message ApprovalRequestedPayload {
  string approval_request_id = 1;
  string action_id = 2;
  string policy_decision_id = 3;
  google.protobuf.Timestamp expires_at = 4;
}

message ApprovalGrantedPayload {
  string approval_receipt_id = 1;
  string approver_subject = 2;
  string action_id = 3;
}

message ApprovalDeniedPayload {
  string approval_request_id = 1;
  string approver_subject = 2;
  string reason = 3;
}

message ExecutionStartedPayload {
  string action_id = 1;
  string adapter_id = 2;
  string applied_sandbox_profile_id = 3;
}

message ExecutionCompletedPayload {
  string action_id = 1;
  string outcome = 2;                  // SUCCEEDED / FAILED / ROLLED_BACK
  string adapter_id = 3;
  uint32 attempts = 4;
}

message VerificationResultPayload {
  string verification_id = 1;
  string action_id = 2;
  string primitive_or_property = 3;
  string status = 4;
  string reason_code = 5;
  google.protobuf.Struct observed_redacted = 6;
}

message RollbackCompletedPayload {
  string action_id = 1;
  string rollback_target_version_id = 2;
  string outcome = 3;
}

message RecoveryEventPayload {
  string event_kind = 1;
  string operator_subject = 2;
  string detail = 3;
}

message ModelCallPayload {
  string model_id = 1;
  uint32 prompt_tokens = 2;
  uint32 completion_tokens = 3;
  google.protobuf.Duration duration = 4;
  bool external = 5;
  string privacy_class = 6;
}

message ChainCheckpointPayload {
  string segment_id = 1;
  uint64 last_sequence_number = 2;
  string rolling_hash = 3;
  google.protobuf.Timestamp checkpoint_at = 4;
}

message GcPassPayload {
  uint32 chunks_reaped = 1;
  uint64 bytes_freed = 2;
  string scope = 3;                    // "chunks" | "evidence" | "transactions"
}

message QuarantineEventPayload {
  string version_id = 1;
  string event = 2;                    // "entered" | "exited"
  string reason = 3;
}

message ConflictEventPayload {
  string conflict_id = 1;
  string event = 2;                    // "opened" | "auto_merged" | "user_resolved" | "abandoned"
  string object_id = 3;
}

message EmergencyOverrideGrantPayload {
  string override_id = 1;
  string operator_subject = 2;
  repeated string overridden_rule_ids = 3;
  google.protobuf.Timestamp expires_at = 4;
}

message PolicyBundleLoadPayload {
  string bundle_version = 1;
  string outcome = 2;
  string operator_subject = 3;
}

message SegmentSealedPayload {
  string segment_id = 1;
  uint64 record_count = 2;
  string genesis_receipt_id = 3;
  string final_receipt_hash = 4;
  string previous_segment_seal_hash = 5;
  bytes  segment_signature = 6;
  string signing_key_id = 7;
  google.protobuf.Timestamp sealed_at = 8;
}

message ChainInconsistencyPayload {
  string segment_id = 1;
  uint64 sequence_number = 2;
  string detail = 3;
}

message TamperDetectedPayload {
  string segment_id = 1;
  uint64 first_anomalous_sequence = 2;
  string expected_hash = 3;
  string observed_hash = 4;
  string detection_method = 5;
}

// ─────────────────────────────────────────────────────────────────
// Service
// ─────────────────────────────────────────────────────────────────

message AppendRequest {
  string schema_version = 1;
  RecordPayload payload = 2;
  RecordType record_type = 3;
  string subject = 4;
  string action_id = 5;
  string correlation_id = 6;
  string trace_id = 7;
  bool simulated = 8;
}

message ReadReceiptRequest { string receipt_id = 1; }

message SubscribeRequest {
  repeated RecordType record_types_filter = 1;
  string subject_filter = 2;
  string correlation_id_filter = 3;
  string resume_from_receipt_id = 4;
  uint32 max_buffered = 5;
}

message QueryRequest {
  repeated RecordType record_types_filter = 1;
  string subject_filter = 2;
  string correlation_id_filter = 3;
  string action_id_filter = 4;
  google.protobuf.Timestamp from_time = 5;
  google.protobuf.Timestamp to_time = 6;
  string text_match = 7;
  uint32 limit = 8;
  string subject = 9;
}

message VerifyChainRequest {
  string segment_id_from = 1;
  string segment_id_to = 2;
}

message VerifyChainResponse {
  bool consistent = 1;
  uint64 receipts_checked = 2;
  string first_anomalous_receipt_id = 3;
  string detection_method = 4;
}

message RebuildIndexRequest {
  bool include_full_text = 1;
}

message RebuildIndexResponse {
  uint64 receipts_indexed = 1;
  google.protobuf.Timestamp completed_at = 2;
}

message LogInfo {
  string log_id = 1;
  repeated string supported_schema_versions = 2;
  string default_schema_version = 3;
  string active_segment_id = 4;
  uint64 active_segment_record_count = 5;
  bool degraded = 6;
  google.protobuf.Timestamp started_at = 7;
}

service EvidenceLog {
  rpc Append(AppendRequest) returns (EvidenceReceipt);
  rpc ReadReceipt(ReadReceiptRequest) returns (EvidenceReceipt);
  rpc Subscribe(SubscribeRequest) returns (stream EvidenceReceipt);
  rpc Query(QueryRequest) returns (stream EvidenceReceipt);
  rpc VerifyChain(VerifyChainRequest) returns (VerifyChainResponse);
  rpc RebuildIndex(RebuildIndexRequest) returns (RebuildIndexResponse);
  rpc GetLogInfo(google.protobuf.Empty) returns (LogInfo);
}
```
