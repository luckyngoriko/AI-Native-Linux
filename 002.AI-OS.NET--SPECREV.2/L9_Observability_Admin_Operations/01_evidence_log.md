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

## 26. See also

- [S0.1 Action Envelope + Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S2.4 Verification Grammar](02_verification_grammar.md)
- [S2.3 Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S2.2 AIOS-FS Implementation Space](../L2_AIOS_FS/04_implementation_space.md)
- [S4.1 Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S5.2 Vault Broker](../L4_Policy_Identity_Vault/02_vault_broker.md)
- [S5.3 Approval Mechanics](../L4_Policy_Identity_Vault/04_approval_mechanics.md)
- [S5.4 Emergency Override](../L4_Policy_Identity_Vault/05_emergency_override.md)
- [S7.1 Surface Composition](../L7_Interaction_Renderers/01_surface_composition.md)
- [S7.2 Shared UI Schema](../L7_Interaction_Renderers/02_shared_ui_schema.md)
- [S7.3 Visual Language](../L7_Interaction_Renderers/03_visual_language.md)
- [S7.4 KDE Renderer](../L7_Interaction_Renderers/04_kde_renderer.md)
- [S7.5 Web Renderer](../L7_Interaction_Renderers/05_web_renderer.md)
- [S8.1 Network Policy](../L8_Network_Hardware_Devices/02_network_policy.md)
- [S8.2 GPU Resource Model](../L8_Network_Hardware_Devices/05_gpu_resource_model.md)
- [S9.1 Recovery Boundary](../L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md)
- [S10.1 Capability Runtime gRPC](../L3_AIOS_SGR_Service_Graph_Runtime/03_capability_runtime_grpc.md)
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
