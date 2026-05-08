# AIOS-FS Object Model (Rev.2)

| Field     | Value                                                                                    |
| --------- | ---------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (refined 2026-05-08; awaiting implementation evidence)                        |
| Phase tag | S1.3                                                                                     |
| Layer     | L2 AIOS-FS                                                                               |
| Consumes  | L0 evidence/status, L1 host/recovery substrate, S0.1 action envelope, S1.2 privacy class |
| Produces  | objects, versions, chunks, pointers, transactions; gRPC `AIOSFSObjects`                  |
| Approved  | 2026-05-08 (deltas D1–D12 applied; replaces draft from `dfa3be5`)                        |

## 1. Purpose

AIOS-FS is the authoritative semantic filesystem rooted at `/aios`. It stores durable objects, immutable versions, content-addressed chunks, semantic metadata, and atomic pointer moves. POSIX paths are projections; object identity is not the path.

This sub-spec defines the object/version/chunk/pointer/transaction data model, write and read flow, garbage collection, quarantine, and the gRPC surface. Conflict detection and resolution live in the companion sub-spec `03_conflict_resolution.md`.

## 2. Core entities

| Entity      | Meaning                                              |
| ----------- | ---------------------------------------------------- |
| Object      | Stable logical identity: app, file, project, memory. |
| Version     | Immutable state of an object at one point.           |
| Chunk       | Content-addressed bytes referenced by versions.      |
| Pointer     | Mutable named reference to a version.                |
| Transaction | Atomic write/promote unit with evidence linkage.     |
| View        | Query-backed projection (deferred to S2.1).          |

## 3. Identity and hash encoding

### 3.1. Identifier prefixes

ULID-based, prefix-namespaced:

| Prefix | Entity         | Example                       |
| ------ | -------------- | ----------------------------- |
| `obj_` | Object         | `obj_01HX...`                 |
| `ver_` | Version        | `ver_01HX...`                 |
| `txn_` | Transaction    | `txn_01HX...`                 |
| `ptr_` | Pointer        | `ptr_01HX...`                 |
| `cnf_` | Conflict       | `cnf_01HX...` (defined in 03) |
| `mpr_` | Merge proposal | `mpr_01HX...` (defined in 03) |

### 3.2. Chunk identifiers

Chunks are content-addressed by **full** BLAKE3-256 hash, lowercase hex:

```text
chunk_id = "chk_" + hex_lower(BLAKE3(bytes))   // 64 hex chars after the prefix
```

Chunk IDs **are not truncated**. Truncation (used for `catalog_version`, `idempotency_key`, `cache_key`) is appropriate for short-lived dedup keys; chunk IDs require full collision resistance because they are persistent storage handles. This rule is intentional and aligned with S0.1 §8.5 (which truncates digests of metadata, not content).

### 3.3. Other content digests

`content_hash` on a Version is the full BLAKE3-256 of the canonical concatenation of chunk bytes in declared order. `metadata_delta` digests are not stored on the version itself; consumers compute them on demand.

## 4. Privacy classification and object lifecycle

### 4.1. Privacy class

Every object carries a `privacy_class` field aligned with S1.2 §5:

| Class            | Meaning                                                                |
| ---------------- | ---------------------------------------------------------------------- |
| `PUBLIC`         | No sensitive info                                                      |
| `INTERNAL`       | Org/project context                                                    |
| `SENSITIVE`      | Identifiable user data (default if unspecified)                        |
| `SECRET_BEARING` | References to secret material; vault refs, key paths, credential hints |
| `CLASSIFIED`     | Operator-marked classified                                             |

Routing decisions (S1.2) and policy decisions (S2.3) read this field. Privacy class can be **raised** but never lowered for a given object; downgrade requires a new object.

### 4.2. Object lifecycle states

```text
ACTIVE  ───retire──▶  RETIRED  ───purge_after_retention──▶  PURGED
```

| State     | Semantics                                                                                              |
| --------- | ------------------------------------------------------------------------------------------------------ |
| `ACTIVE`  | Default; reads and writes allowed per policy.                                                          |
| `RETIRED` | Logical removal; reads allowed for audit; writes denied; pointer moves denied.                         |
| `PURGED`  | Physical erase scheduled or completed; chunks reference-decremented; metadata kept as audit-only stub. |

Retention period defaults: 90 days for retired-then-purged. Operator may shorten only with policy approval; lengthening is always allowed.

## 5. Object record

```proto
message Object {
  string object_id = 1;                          // "obj_<ULID>"
  ObjectKind kind = 2;
  google.protobuf.Timestamp created_at = 3;
  string created_by = 4;                         // L4 subject string
  string current_pointer_id = 5;
  ObjectMetadata metadata = 6;
  repeated string policy_tags = 7;
  PrivacyClass privacy_class = 8;                // S1.2 §5 enum (re-exported in proto)
  ObjectLifecycleState lifecycle_state = 9;
  google.protobuf.Timestamp retired_at = 10;
  google.protobuf.Timestamp purge_at = 11;       // scheduled purge time
  repeated string index_hints = 12;              // e.g. "fulltext", "semantic"
}

message ObjectMetadata {
  string name = 1;
  repeated string labels = 2;
  string mime = 3;
  google.protobuf.Struct extra = 4;              // free-form
}

enum ObjectKind {
  OBJECT_KIND_UNSPECIFIED = 0;
  PROJECT      = 1;
  APPLICATION  = 2;
  FILE         = 3;
  MEMORY       = 4;
  POLICY       = 5;
  MODEL        = 6;
  PACKAGE      = 7;
  EVIDENCE_REF = 8;
  WORKSPACE    = 9;
  CONFIG       = 10;
}

enum ObjectLifecycleState {
  OBJECT_LIFECYCLE_UNSPECIFIED = 0;
  ACTIVE  = 1;
  RETIRED = 2;
  PURGED  = 3;
}
```

`metadata` is queryable but not authoritative security state. Policy decisions use explicit `policy_tags`, `privacy_class`, and ownership — not free-form metadata or embeddings.

## 6. Version record

```proto
message Version {
  string version_id = 1;                            // "ver_<ULID>"
  string object_id = 2;
  repeated string parent_version_ids = 3;           // multiple parents allowed for merge resolutions
  repeated string chunk_refs = 4;                   // ordered "chk_<hex>" list
  string content_hash = 5;                          // hex_lower(BLAKE3(canonical_concat(chunks)))
  google.protobuf.Struct metadata_delta = 6;
  string created_by_action_id = 7;                  // S0.1 action_id; "" if non-action origin
  string created_by_transaction_id = 8;
  google.protobuf.Timestamp created_at = 9;
  VersionState state = 10;
  google.protobuf.Timestamp quarantined_at = 11;
  string quarantine_reason = 12;
}

enum VersionState {
  VERSION_STATE_UNSPECIFIED = 0;
  STAGED          = 1;   // written but not yet verified
  VERIFIED        = 2;   // verification passed; eligible for promotion
  QUARANTINED     = 3;   // isolated due to validation/integrity/policy failure
  RETIRED_VERSION = 4;   // superseded; not eligible for promotion but readable for audit
}
```

Versions are immutable. Correction creates a new version with a `parent_version_ids` link to the corrected ancestor.

## 7. Chunk model

### 7.1. Record

```proto
message Chunk {
  string chunk_id = 1;                          // "chk_<hex_lower(BLAKE3(bytes))>"
  uint64 size_bytes = 2;
  uint32 ref_count = 3;                         // monotonic; sum of references from active versions
  google.protobuf.Timestamp created_at = 4;
}
```

### 7.2. Chunking strategy

Default: **content-defined chunking** (CDC) using FastCDC parameters:

| Parameter       | Default                                                              |
| --------------- | -------------------------------------------------------------------- |
| min size        | 64 KB                                                                |
| avg size        | 256 KB                                                               |
| max size        | 1 MB                                                                 |
| polynomial seed | implementation-defined; must be deterministic and stable across runs |

Fixed-size chunking is permitted as fallback for known-streaming workloads (e.g. video) where CDC overhead is unjustified; the chunking strategy applied to a version is recorded in the version's `metadata_delta` so consumers can verify.

### 7.3. Garbage collection

- Chunks track `ref_count` from active (non-retired) versions.
- A chunk is GC-eligible when `ref_count = 0` AND no staged transaction holds it.
- **Orphan staging TTL:** 24 hours by default. Chunks written by an aborted or stale transaction beyond TTL are GC-eligible.
- GC is **not silent**. Each GC pass writes an evidence record (`evr_<ULID>`) recording chunk IDs reaped and bytes freed.
- Operator policy may configure scheduled GC windows or ad-hoc GC; the runtime never deletes chunks without an evidence-logged trigger.

## 8. Pointer record and CAS protocol

### 8.1. Pointer record

```proto
message Pointer {
  string pointer_id = 1;                          // "ptr_<ULID>"
  string object_id = 2;
  PointerKind kind = 3;
  string current_version_id = 4;
  google.protobuf.Timestamp last_promoted_at = 5;
  string last_promoted_by_transaction_id = 6;
}

enum PointerKind {
  POINTER_KIND_UNSPECIFIED = 0;
  CURRENT    = 1;   // active version shown in normal views
  STABLE     = 2;   // last verified stable version
  CANDIDATE  = 3;   // staged version awaiting verification
  ROLLBACK   = 4;   // version to restore if promotion fails
  QUARANTINE = 5;   // version isolated after validation failure
}
```

### 8.2. Compare-and-swap protocol

Pointer moves use **atomic compare-and-swap** on `(pointer_id, expected_current_version_id)`:

```text
PromotePointer(pointer_id, expected_current_version_id, new_version_id):
  if pointer.current_version_id == expected_current_version_id:
    pointer.current_version_id := new_version_id
    pointer.last_promoted_at := now
    pointer.last_promoted_by_transaction_id := caller_txn
    return OK
  else:
    return ConflictDetected(
      pointer_id, current_version_id, attempted_parent=expected_current_version_id
    )
```

`ConflictDetected` is the gateway error to the conflict resolution sub-spec (`03_conflict_resolution.md`). The candidate version persists; only the pointer move is rejected.

### 8.3. Multi-pointer atomicity

Transactions may include multiple `PointerMoveOp`s. Either all promotions succeed (transaction commits) or none does (transaction aborts). Implementation uses a two-phase commit fence within a single AIOS-FS instance; cross-instance multi-object transactions are out of scope here.

## 9. Transaction model

### 9.1. Record

```proto
message Transaction {
  string transaction_id = 1;                      // "txn_<ULID>"
  string subject = 2;                             // L4 subject
  string action_id = 3;                           // S0.1 action_id; "" if non-action origin
  google.protobuf.Timestamp started_at = 4;
  google.protobuf.Timestamp completed_at = 5;
  TransactionState state = 6;
  repeated WriteOp writes = 7;
  repeated PointerMoveOp pointer_moves = 8;
  string evidence_receipt_id = 9;                 // "evr_<ULID>"
}

message WriteOp {
  string object_id = 1;
  string created_version_id = 2;
  repeated string chunk_ids_written = 3;
}

message PointerMoveOp {
  string pointer_id = 1;
  string expected_current_version_id = 2;         // CAS expectation
  string new_version_id = 3;
}

enum TransactionState {
  TRANSACTION_STATE_UNSPECIFIED = 0;
  PENDING_TX = 1;
  COMMITTING = 2;
  COMMITTED  = 3;
  ABORTED    = 4;
}
```

### 9.2. Lifecycle

```text
PENDING_TX  ──Write/Promote ops accumulate──▶  COMMITTING
COMMITTING  ──all CAS succeed──▶  COMMITTED
COMMITTING  ──any CAS fails──▶  ABORTED  (sibling versions persist; pointers unchanged)
PENDING_TX  ──explicit AbortTransaction──▶  ABORTED
```

A transaction in `PENDING_TX` for longer than its `staging TTL` (default 1 hour) is auto-aborted by the runtime; chunks become orphan-eligible per §7.3.

## 10. View record

```proto
message View {
  string view_id = 1;                             // "view_<ULID>"
  string view_path = 2;                           // e.g. "/aios/views/latest-stable/sdf-renderer"
  string query_dsl_version = 3;                   // S2.1 query language version
  string query_text = 4;                          // canonical query (defined in S2.1)
  google.protobuf.Timestamp last_built_at = 5;
  uint64 result_object_count = 6;
}
```

The view query DSL is defined in **S2.1**. This sub-spec only reserves the schema fields. View consistency is `EVENTUAL` (§11).

## 11. Read consistency

Three read modes available; client requests one per call:

| Mode           | Semantics                                                                    | Default for                                |
| -------------- | ---------------------------------------------------------------------------- | ------------------------------------------ |
| `SNAPSHOT`     | Reads a consistent snapshot across pointers as of the call's wall time.      | All `ReadObject` and `ReadVersion` calls.  |
| `LINEARIZABLE` | Latest committed state at call time; serializes after any in-flight commits. | Pre-write reads that need strict ordering. |
| `EVENTUAL`     | May lag committed state; returned from cached view materializations.         | Views and bulk enumerations.               |

Implementations choose how to provide each (snapshot may be MVCC; linearizable may take a brief lock or quorum read). Callers must not assume internal mechanism — only the consistency guarantee.

## 12. Quarantine semantics

### 12.1. Triggers

A version enters `QUARANTINED` state when any of:

- **Validation failure** during write (e.g., schema validation, signature mismatch).
- **Integrity check failure** on read (chunk hash does not match content).
- **Policy violation** detected post-commit (e.g., privacy classifier upgraded the privacy class after re-scan).
- **External attestation failure** (e.g., AI merge proposal rejected by review).
- **Operator manual quarantine**.

### 12.2. Effects

- Pointers referencing a quarantined version are atomically moved to the `ROLLBACK` pointer's target if one exists; otherwise pointer is set to the prior `STABLE`.
- Reads of a quarantined version are denied for non-recovery subjects; recovery and audit subjects may read.
- The version's `quarantined_at` and `quarantine_reason` fields are populated.

### 12.3. Exits

A quarantined version exits quarantine via:

- **Manual review** by an authorized subject who issues an `ExitQuarantine` call with a justification recorded in evidence.
- **Automated re-validation** if the original trigger was transient (e.g., a temporarily missing signature was later resolved).

A quarantined version that is never resolved becomes `RETIRED_VERSION` after the operator-configured quarantine TTL (default 30 days). Its chunks become GC-eligible if `ref_count` permits.

### 12.4. Evidence

Every quarantine entry and exit emits an evidence record. The chain `version_id → quarantine_reason → exit_subject → resolution_outcome` must be reconstructible from the evidence log.

## 13. `/aios` layout

The root namespace is operational, not arbitrary.

```text
/aios/
  objects/      # canonical object identity surface
  views/        # query projections (S2.1)
  apps/         # stable human-facing app views
  users/        # per-user workspaces
  projects/     # project projections
  memory/       # cognitive memories
  policies/     # policy package objects
  packages/     # software packages
  runtime/      # service runtime state
  evidence/     # evidence log (read-only projection from L9)
  recovery/     # recovery tooling state
```

`/aios/objects` exposes object identity directly. `/aios/views` exposes query projections. `/aios/apps` and `/aios/projects` are stable human-facing views over objects.

## 14. POSIX projection

AIOS-FS may expose POSIX-compatible paths for applications, but the projection must obey:

- Object id remains canonical; path is a view of object identity, not the identity itself.
- Writes through POSIX create new versions atomically; partial in-place writes are rejected.
- Broad host writes (anywhere outside an app's owned state) are denied by default.
- App writes stay inside app-owned state directories.
- View rebuild must not change object identity.

Applications that cannot tolerate versioned write semantics run in compatibility sandboxes with explicit state mapping (out of scope here; see L6 sandbox composition).

## 15. Recovery requirements

Recovery tools must be able to operate without the Cognitive Core:

- Enumerate objects.
- Verify chunk hashes.
- Inspect pointer history.
- Roll a pointer back to a verified version.
- Rebuild semantic indexes from object metadata.
- Quarantine corrupt versions.
- Mark transactions as forcibly aborted.

The recovery path depends on **L1 and L2 only**. No L3+ services, no LLM, no Web UI, no KDE session.

## 16. Cross-spec dependencies

| Spec                           | Relationship                                                                                                         |
| ------------------------------ | -------------------------------------------------------------------------------------------------------------------- |
| **S0.1** Action Envelope       | `created_by_action_id` on Version links AIOS-FS writes to typed actions.                                             |
| **S1.1** Capability Translator | Manifest target schemas for `aiosfs.*` actions reference `obj_`/`ver_` identifiers.                                  |
| **S1.2** Latency Tiering       | `PrivacyClass` enum is shared; AIOS-FS objects expose `privacy_class` for routing decisions.                         |
| **S1.3** Conflict Resolution   | `03_conflict_resolution.md` defines `Conflict` and merge mechanics; this spec defines `ConflictDetected` triggering. |
| **S2.1** Query/View Language   | `View.query_text` interpreted by S2.1 grammar.                                                                       |
| **S2.2** Implementation Space  | Decision: userspace authoritative store + FUSE/portal projections first.                                             |
| **S2.3** Policy Kernel         | Policy decisions on read/write of objects use `policy_tags` and `privacy_class`.                                     |
| **S3.1** Evidence Log          | Every transaction commits an evidence record; GC, quarantine, retire all logged.                                     |

## 17. Open deferrals

- View query DSL → S2.1.
- Concrete storage backend (RocksDB, sled, custom WAL) → S2.2.
- Cross-instance distributed transactions and multi-master replication → future sub-spec under L2.
- Object-level access control lists beyond `policy_tags` and ownership → L4 vault sub-spec.
- POSIX projection compatibility profiles for legacy applications → L6 compatibility runtime.
- Marketplace-distributed objects (signed third-party artifacts) → L10.

## 18. Acceptance criteria

- No committed object content is overwritten in place.
- Every write has transaction and evidence linkage.
- Pointer promotion is atomic, CAS-protected, and emits `ConflictDetected` on failure.
- Chunk integrity is content-addressed with full BLAKE3 hex.
- Multi-pointer transactions are all-or-nothing.
- Snapshot reads return a consistent view across pointers.
- Quarantine entries and exits are evidence-logged.
- GC operates only via evidence-logged passes.
- Recovery enumerates and rolls back without AI.
- All golden fixtures from §19 pass against the implementation.
- Object lifecycle transitions (ACTIVE → RETIRED → PURGED) honor retention policy.
- Privacy class can be raised but not lowered.
- Telemetry metrics from §20 are emitted with bounded label cardinality.

## 19. Golden fixtures

`{ scenario, expected_outcome }` triples for an acceptance test harness.

### 19.1. Single object write and promote (happy path)

```yaml
fixture_id: aiosfs.fix.write_promote.v1
scenario:
  - BeginTransaction(subject=human:lucky, action_id=act_1)
  - WriteVersion(object_id=obj_a, parent=ver_root, chunks=[chk_a])
  - PromotePointer(ptr=ptr_current_a, expected=ver_root, new=ver_just_written)
  - CommitTransaction()
expected:
  transaction_state: COMMITTED
  pointer_state: ptr_current_a.current_version_id == ver_just_written
  evidence_chain_present: true
  side_effects:
    chunk_ref_count_increment: { chk_a: 1 }
```

### 19.2. Concurrent write triggers ConflictDetected

```yaml
fixture_id: aiosfs.fix.cas_conflict.v1
scenario:
  - txn_A: BeginTransaction; WriteVersion(obj_a, parent=ver_root, chunks=[chk_a1]); PromotePointer(ptr_current_a, expected=ver_root, new=ver_A); CommitTransaction
  - txn_B: BeginTransaction in parallel; WriteVersion(obj_a, parent=ver_root, chunks=[chk_a2]); PromotePointer(ptr_current_a, expected=ver_root, new=ver_B); CommitTransaction
expected:
  txn_A: COMMITTED, ptr_current_a -> ver_A
  txn_B: ABORTED with ConflictDetected(current=ver_A, attempted_parent=ver_root)
  ver_B persists as STAGED
  conflict_record_created: true (cnf_<ULID>)
```

### 19.3. Multi-pointer atomicity (all-or-nothing)

```yaml
fixture_id: aiosfs.fix.multi_pointer_atomic.v1
scenario:
  - BeginTransaction
  - WriteVersion(obj_a, parent=ver_a0, chunks=[...]) -> ver_a1
  - WriteVersion(obj_b, parent=ver_b0, chunks=[...]) -> ver_b1
  - PromotePointer(ptr_current_a, expected=ver_a0, new=ver_a1)   # OK
  - PromotePointer(ptr_current_b, expected=ver_b_stale, new=ver_b1)   # CAS fails
  - CommitTransaction
expected:
  transaction_state: ABORTED
  ptr_current_a.current_version_id: ver_a0  (rolled back)
  ptr_current_b.current_version_id: unchanged
  ver_a1 and ver_b1 persist as STAGED siblings
```

### 19.4. Quarantine on integrity failure

```yaml
fixture_id: aiosfs.fix.quarantine_integrity.v1
scenario:
  - ReadVersion(ver_x) where stored chunk hash != recomputed BLAKE3
expected:
  read_status: PERMISSION_DENIED (for non-recovery subjects)
  ver_x.state: QUARANTINED
  ver_x.quarantine_reason: "chunk_integrity_failure"
  pointer_referencing_ver_x: rolled back to ROLLBACK pointer's target
  evidence_emitted: true
```

### 19.5. GC reaps orphan chunk

```yaml
fixture_id: aiosfs.fix.gc_orphan.v1
scenario:
  - WriteVersion in PENDING_TX writes chunks [chk_orphan]
  - AbortTransaction
  - Wait beyond orphan staging TTL (24h default; test harness time-warp)
  - Run GC pass
expected:
  chk_orphan: deleted
  evidence_record_emitted: true with reaped_chunks=[chk_orphan]
```

### 19.6. Privacy class cannot be lowered

```yaml
fixture_id: aiosfs.fix.privacy_class_monotonic.v1
scenario:
  - Object obj_x has privacy_class=SENSITIVE
  - Subject attempts to update obj_x with privacy_class=PUBLIC
expected:
  result: PERMISSION_DENIED
  reason: "privacy_class_downgrade_forbidden"
  obj_x.privacy_class: SENSITIVE  (unchanged)
```

### 19.7. Recovery enumerate without AI

```yaml
fixture_id: aiosfs.fix.recovery_enumerate.v1
scenario:
  - Cognitive Core stopped
  - Recovery tool calls EnumerateObjects()
expected:
  result: success
  objects_returned: all ACTIVE and RETIRED objects
  no_LLM_invoked: true
```

### 19.8. Snapshot read consistency across pointers

```yaml
fixture_id: aiosfs.fix.snapshot_read.v1
scenario:
  - obj_a has ptr_current pointing to ver_a1
  - obj_b has ptr_current pointing to ver_b1
  - txn_T concurrently promotes ptr_current_a to ver_a2 and ptr_current_b to ver_b2 (atomic)
  - Reader R takes SNAPSHOT read of [obj_a, obj_b]
expected: R sees either both (ver_a1, ver_b1) or both (ver_a2, ver_b2); never a mix
```

## 20. Telemetry contract

Required metrics (Prometheus-style; OpenTelemetry-compatible):

| Metric                            | Type      | Labels                                 |
| --------------------------------- | --------- | -------------------------------------- |
| `aiosfs_writes_total`             | counter   | `object_kind`, `outcome`               |
| `aiosfs_write_bytes_total`        | counter   | `object_kind`                          |
| `aiosfs_write_latency_seconds`    | histogram | `object_kind`, `consistency`           |
| `aiosfs_read_latency_seconds`     | histogram | `object_kind`, `consistency`           |
| `aiosfs_pointer_promotions_total` | counter   | `outcome` (committed/conflict/aborted) |
| `aiosfs_conflicts_total`          | counter   | `object_kind`                          |
| `aiosfs_quarantine_total`         | counter   | `reason`                               |
| `aiosfs_quarantine_active`        | gauge     |                                        |
| `aiosfs_gc_chunks_reaped_total`   | counter   |                                        |
| `aiosfs_gc_bytes_freed_total`     | counter   |                                        |
| `aiosfs_chunk_count`              | gauge     |                                        |
| `aiosfs_object_count`             | gauge     | `lifecycle_state`                      |

Cardinality bounds: `object_kind` ≤ 11, `outcome` ≤ 5, `consistency` = 3, `reason` ≤ 10. Subject is **never** a metric label.

## 21. See also

- [S0.1 Action Envelope + Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S1.2 Latency Tiering](../L5_Cognitive_Core/03_latency_tiering.md)
- [S1.3 Conflict Resolution](03_conflict_resolution.md)
- [S2.1 Query/View Language](02_query_view_language.md)
- [S2.2 Implementation Space](04_implementation_space.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

## Appendix A: Complete proto IDL

```proto
syntax = "proto3";
package aios.fs.v1alpha1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/struct.proto";
import "google/protobuf/empty.proto";

// ─────────────────────────────────────────────────────────────────
// Enums
// ─────────────────────────────────────────────────────────────────

enum ObjectKind {
  OBJECT_KIND_UNSPECIFIED = 0;
  PROJECT      = 1;
  APPLICATION  = 2;
  FILE         = 3;
  MEMORY       = 4;
  POLICY       = 5;
  MODEL        = 6;
  PACKAGE      = 7;
  EVIDENCE_REF = 8;
  WORKSPACE    = 9;
  CONFIG       = 10;
}

enum ObjectLifecycleState {
  OBJECT_LIFECYCLE_UNSPECIFIED = 0;
  ACTIVE  = 1;
  RETIRED = 2;
  PURGED  = 3;
}

enum VersionState {
  VERSION_STATE_UNSPECIFIED = 0;
  STAGED          = 1;
  VERIFIED        = 2;
  QUARANTINED     = 3;
  RETIRED_VERSION = 4;
}

enum PointerKind {
  POINTER_KIND_UNSPECIFIED = 0;
  CURRENT    = 1;
  STABLE     = 2;
  CANDIDATE  = 3;
  ROLLBACK   = 4;
  QUARANTINE = 5;
}

enum TransactionState {
  TRANSACTION_STATE_UNSPECIFIED = 0;
  PENDING_TX = 1;
  COMMITTING = 2;
  COMMITTED  = 3;
  ABORTED    = 4;
}

enum PrivacyClass {
  PRIVACY_CLASS_UNSPECIFIED = 0;
  PUBLIC          = 1;
  INTERNAL        = 2;
  SENSITIVE       = 3;
  SECRET_BEARING  = 4;
  CLASSIFIED      = 5;
}

enum ReadConsistency {
  READ_CONSISTENCY_UNSPECIFIED = 0;
  SNAPSHOT     = 1;
  LINEARIZABLE = 2;
  EVENTUAL     = 3;
}

// ─────────────────────────────────────────────────────────────────
// Records
// ─────────────────────────────────────────────────────────────────

message Object {
  string object_id = 1;
  ObjectKind kind = 2;
  google.protobuf.Timestamp created_at = 3;
  string created_by = 4;
  string current_pointer_id = 5;
  ObjectMetadata metadata = 6;
  repeated string policy_tags = 7;
  PrivacyClass privacy_class = 8;
  ObjectLifecycleState lifecycle_state = 9;
  google.protobuf.Timestamp retired_at = 10;
  google.protobuf.Timestamp purge_at = 11;
  repeated string index_hints = 12;
}

message ObjectMetadata {
  string name = 1;
  repeated string labels = 2;
  string mime = 3;
  google.protobuf.Struct extra = 4;
}

message Version {
  string version_id = 1;
  string object_id = 2;
  repeated string parent_version_ids = 3;
  repeated string chunk_refs = 4;
  string content_hash = 5;
  google.protobuf.Struct metadata_delta = 6;
  string created_by_action_id = 7;
  string created_by_transaction_id = 8;
  google.protobuf.Timestamp created_at = 9;
  VersionState state = 10;
  google.protobuf.Timestamp quarantined_at = 11;
  string quarantine_reason = 12;
}

message Chunk {
  string chunk_id = 1;
  uint64 size_bytes = 2;
  uint32 ref_count = 3;
  google.protobuf.Timestamp created_at = 4;
}

message Pointer {
  string pointer_id = 1;
  string object_id = 2;
  PointerKind kind = 3;
  string current_version_id = 4;
  google.protobuf.Timestamp last_promoted_at = 5;
  string last_promoted_by_transaction_id = 6;
}

message Transaction {
  string transaction_id = 1;
  string subject = 2;
  string action_id = 3;
  google.protobuf.Timestamp started_at = 4;
  google.protobuf.Timestamp completed_at = 5;
  TransactionState state = 6;
  repeated WriteOp writes = 7;
  repeated PointerMoveOp pointer_moves = 8;
  string evidence_receipt_id = 9;
}

message WriteOp {
  string object_id = 1;
  string created_version_id = 2;
  repeated string chunk_ids_written = 3;
}

message PointerMoveOp {
  string pointer_id = 1;
  string expected_current_version_id = 2;
  string new_version_id = 3;
}

message View {
  string view_id = 1;
  string view_path = 2;
  string query_dsl_version = 3;
  string query_text = 4;
  google.protobuf.Timestamp last_built_at = 5;
  uint64 result_object_count = 6;
}

// ─────────────────────────────────────────────────────────────────
// Request / response
// ─────────────────────────────────────────────────────────────────

message BeginTransactionRequest {
  string subject = 1;
  string action_id = 2;
}

message BeginTransactionResponse {
  string transaction_id = 1;
}

message WriteVersionRequest {
  string transaction_id = 1;
  string object_id = 2;
  string parent_version_id = 3;
  repeated bytes chunk_payloads = 4;            // raw bytes; runtime computes chunk_ids
  google.protobuf.Struct metadata_delta = 5;
}

message WriteVersionResponse {
  string version_id = 1;
  repeated string chunk_ids = 2;
}

message PromotePointerRequest {
  string transaction_id = 1;
  string pointer_id = 2;
  string expected_current_version_id = 3;
  string new_version_id = 4;
}

message PromotePointerResponse {
  bool ok = 1;
  string conflict_id = 2;                        // populated on CAS failure
}

message CommitTransactionRequest {
  string transaction_id = 1;
}

message CommitTransactionResponse {
  string evidence_receipt_id = 1;
  TransactionState final_state = 2;
}

message AbortTransactionRequest {
  string transaction_id = 1;
  string reason = 2;
}

message ReadObjectRequest {
  string object_id = 1;
  ReadConsistency consistency = 2;
}

message ReadVersionRequest {
  string version_id = 1;
  ReadConsistency consistency = 2;
}

message ReadChunkRequest {
  string chunk_id = 1;
}

message ChunkBytes {
  bytes data = 1;
  uint64 offset = 2;
}

message EnumerateObjectsRequest {
  ObjectKind kind_filter = 1;                    // optional
  ObjectLifecycleState lifecycle_filter = 2;     // optional
  PrivacyClass max_class = 3;                    // optional, returns objects at or below this class
}

message ListConflictsRequest {
  string object_id = 1;                          // optional filter
}

message Conflict {                                // forward declaration; full def in 03_conflict_resolution.md
  string conflict_id = 1;
  // ... see conflict_resolution Appendix
}

message ResolveConflictRequest {
  string conflict_id = 1;
  string resolution_version_id = 2;
  string subject = 3;
  string transaction_id = 4;
}

message ResolveConflictResponse {
  string evidence_receipt_id = 1;
}

message RebuildIndexesRequest {
  bool include_semantic = 1;
  bool include_lexical = 2;
}

message RebuildIndexesResponse {
  uint64 objects_indexed = 1;
  google.protobuf.Timestamp completed_at = 2;
}

message QuarantineVersionRequest {
  string version_id = 1;
  string reason = 2;
  string subject = 3;
}

message ExitQuarantineRequest {
  string version_id = 1;
  string subject = 2;
  string justification = 3;
}

message RetireObjectRequest {
  string object_id = 1;
  string subject = 2;
  google.protobuf.Timestamp purge_at = 3;        // optional override of default retention
}

message PurgeObjectRequest {
  string object_id = 1;
  string subject = 2;
}

// ─────────────────────────────────────────────────────────────────
// Service
// ─────────────────────────────────────────────────────────────────

service AIOSFSObjects {
  // Transaction lifecycle
  rpc BeginTransaction(BeginTransactionRequest) returns (BeginTransactionResponse);
  rpc WriteVersion(WriteVersionRequest) returns (WriteVersionResponse);
  rpc PromotePointer(PromotePointerRequest) returns (PromotePointerResponse);
  rpc CommitTransaction(CommitTransactionRequest) returns (CommitTransactionResponse);
  rpc AbortTransaction(AbortTransactionRequest) returns (google.protobuf.Empty);

  // Read
  rpc ReadObject(ReadObjectRequest) returns (Object);
  rpc ReadVersion(ReadVersionRequest) returns (Version);
  rpc ReadChunk(ReadChunkRequest) returns (stream ChunkBytes);
  rpc EnumerateObjects(EnumerateObjectsRequest) returns (stream Object);

  // Conflicts (full schema in 03_conflict_resolution.md)
  rpc ListConflicts(ListConflictsRequest) returns (stream Conflict);
  rpc ResolveConflict(ResolveConflictRequest) returns (ResolveConflictResponse);

  // Maintenance
  rpc RebuildIndexes(RebuildIndexesRequest) returns (RebuildIndexesResponse);
  rpc QuarantineVersion(QuarantineVersionRequest) returns (google.protobuf.Empty);
  rpc ExitQuarantine(ExitQuarantineRequest) returns (google.protobuf.Empty);
  rpc RetireObject(RetireObjectRequest) returns (google.protobuf.Empty);
  rpc PurgeObject(PurgeObjectRequest) returns (google.protobuf.Empty);
}
```
