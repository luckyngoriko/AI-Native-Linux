# AIOS-FS Implementation Space (Rev.2)

| Field     | Value                                                                                |
| --------- | ------------------------------------------------------------------------------------ |
| Status    | `CONTRACT` (refined 2026-05-08; awaiting implementation evidence)                    |
| Phase tag | S2.2                                                                                 |
| Layer     | L2 AIOS-FS                                                                           |
| Decision  | Userspace authoritative object store + FUSE/portal projections; kernel work deferred |
| Approved  | 2026-05-08 (deltas D9–D12 applied; replaces draft from `dfa3be5`)                    |

## 1. Purpose

This document chooses the implementation path for AIOS-FS without locking the project into premature kernel work. It also specifies backing storage choices, crash consistency, snapshot/backup, encryption, performance targets, resource budgets, and migration tooling — items the rev.1 draft deferred.

## 2. Options considered

| Option                 | Strengths                                | Weaknesses                                |
| ---------------------- | ---------------------------------------- | ----------------------------------------- |
| New kernel filesystem  | Deep integration, maximum control        | High risk, hard recovery, long validation |
| FUSE filesystem        | Fast iteration, POSIX compatibility      | Performance overhead, semantic impedance  |
| Userspace object store | Safest semantics, easy evidence/recovery | Requires projections for normal apps      |
| Hybrid                 | Object store + FUSE/portal projections   | More moving parts; best staged path       |

## 3. Decision

Rev.2 chooses the hybrid path:

```text
AIOS-FS authoritative userspace object store
  -> FUSE read/write projection for compatible POSIX paths
  -> portals/bind mounts for app sandboxes
  -> future kernel module only after contracts are proven
```

The authoritative store is **not POSIX**. POSIX is a compatibility projection.

## 4. Rationale

- Recovery tools can inspect object records without loading a custom kernel module.
- Versioning and evidence semantics are easier in userspace.
- Semantic indexes evolve independently from kernel constraints.
- FUSE is good enough for prototype and compatibility.
- Kernel-space work is deferred until object model and conflict semantics are proven stable.

## 5. Required components

| Component             | Role                                     |
| --------------------- | ---------------------------------------- |
| Object store          | Objects, versions, chunks, metadata      |
| Transaction log (WAL) | Crash consistency and replay             |
| Pointer store         | Atomic CAS pointer moves                 |
| Indexer               | Rebuildable semantic and lexical indexes |
| FUSE projector        | POSIX-compatible paths                   |
| App portal            | Scoped app access                        |
| Recovery inspector    | Read/verify/rollback without AI          |

## 6. Backing storage choice

### 6.1. Component-to-store mapping

| Subsystem                          | Primary store                    | Rationale                                                                                          |
| ---------------------------------- | -------------------------------- | -------------------------------------------------------------------------------------------------- |
| Chunk store                        | **RocksDB**                      | Mature LSM tree; range scans for GC; tunable compression; battle-tested at scale.                  |
| Object/version/pointer/txn records | **RocksDB**                      | Same engine as chunks; atomic write batches across column families enable CAS without a second DB. |
| Metadata catalog (queryable)       | **SQLite (WAL mode)**            | Standard SQL; trivial recovery inspection; rich indexes; strong durability guarantees.             |
| Lexical / full-text index          | **Tantivy**                      | Rust-native, embeddable, fast; matches AIOS execution stack; mmap-friendly.                        |
| Embedding index                    | (deferred to L5 vector sub-spec) | AIOS-FS holds embedding _references_ but does not own vector search.                               |
| Transaction log                    | **RocksDB column family**        | Single fsync path; replay from LSN.                                                                |

### 6.2. Why not other choices

- **sled** — promising but less mature than RocksDB; revisit when stable.
- **BadgerDB** — Go-only ergonomics; AIOS execution layer is Rust.
- **Custom WAL + B-tree** — not justified given RocksDB's track record.
- **Full SQL DB** (e.g. PostgreSQL) for chunks — operational overhead too high for an embedded subsystem.
- **Lucene/Elasticsearch** for lexical — JVM dependency unwelcome in core runtime.

### 6.3. Column family layout (RocksDB)

| Column family      | Key                             | Value                       |
| ------------------ | ------------------------------- | --------------------------- |
| `chunks`           | `chunk_id` (full hex)           | chunk bytes                 |
| `objects`          | `object_id`                     | proto-encoded `Object`      |
| `versions`         | `version_id`                    | proto-encoded `Version`     |
| `pointers`         | `pointer_id`                    | proto-encoded `Pointer`     |
| `transactions`     | `transaction_id`                | proto-encoded `Transaction` |
| `wal`              | sequence number                 | proto-encoded `WalEntry`    |
| `chunk_refcount`   | `chunk_id`                      | uint32 ref_count            |
| `quarantine_index` | `quarantined_at` + `version_id` | foreign key                 |

The CAS protocol (object model §8.2) is implemented as a RocksDB write batch with a `Merge` operator on the pointer column family that enforces the expected-value check.

## 7. Crash consistency

### 7.1. Write path

1. Client transaction collects writes and pointer moves in memory.
2. On `CommitTransaction`, the runtime composes a single `WalEntry` proto containing all writes and CAS expectations.
3. `WalEntry` is appended to the `wal` column family with `fsync=true`.
4. After WAL fsync acknowledgement, a write batch updates `chunks`, `objects`, `versions`, and `pointers` atomically (RocksDB write batch is atomic by construction).
5. Transaction state moves to `COMMITTED`; evidence emitted.

A crash at any point before step 3 leaves the WAL clean — no partial state. A crash between steps 3 and 4 is recovered by replaying WAL entries on startup.

### 7.2. Recovery on startup

- Read WAL from last checkpoint.
- For each entry in `WalEntry.committed_state` not yet reflected in primary CFs, replay.
- For pending transactions older than orphan TTL (object model §7.3), abort and emit evidence.

### 7.3. Checkpoint

A periodic checkpoint (default every 5 minutes or 10 000 entries) compacts the WAL: state is flushed to primary CFs, and WAL entries up to the checkpoint LSN are purged. Checkpoints are evidence-logged.

## 8. Snapshot and backup

### 8.1. Snapshot tier

| Tier                     | Mechanism                                         | Recovery RPO target        |
| ------------------------ | ------------------------------------------------- | -------------------------- |
| **ZFS / Btrfs snapshot** | Filesystem-level snapshot of the AIOS-FS data dir | < 1 minute                 |
| **LVM snapshot**         | Volume-level COW snapshot                         | < 5 minutes                |
| **Logical export**       | gRPC streamed export (`ExportSnapshot`)           | minutes to hours; portable |

ZFS is the recommended path on supported substrates; LVM is fallback. AIOS-FS detects the underlying volume manager and exposes the appropriate snapshot RPC.

### 8.2. Logical export format

Logical exports use a self-describing tarball containing:

```text
aiosfs_snapshot_<id>.tar
├── manifest.proto              # snapshot metadata, schema version, included objects, LSN
├── chunks/                     # one file per chunk_id
├── catalog.sqlite              # metadata catalog snapshot
└── wal_segments/               # WAL entries up to snapshot LSN
```

Imports verify the manifest, replay state, and produce a fresh AIOS-FS instance.

### 8.3. Incremental snapshots

- ZFS/Btrfs incremental: native delta semantics.
- Logical incremental: changed-objects-only export based on LSN diff.

### 8.4. Backup retention

Operator-configured. Default: 7 daily, 4 weekly, 6 monthly. Backups are evidence-logged on creation and verification.

## 9. Encryption at rest

### 9.1. AIOS-FS scope

AIOS-FS **does not implement** block-level encryption. It assumes the underlying volume is encrypted with `LUKS`/`dm-crypt` or `ZFS native encryption`. Operators configure this at install time.

### 9.2. Per-object encryption

Per-object encryption is **deferred** to a future L4 vault sub-spec. Objects with `privacy_class ∈ {SECRET_BEARING, CLASSIFIED}` may, in a future revision, be wrapped under a vault key; that wrapping is not part of the rev.2 contract.

### 9.3. Sensitive metadata

Metadata referenced by `privacy_class` is treated by the privacy filter (`02_query_view_language.md` §5). Evidence projection (L9 S3.1) applies redaction to metadata fields like paths and identifiers when the receipt itself is `SENSITIVE` or higher.

### 9.4. Key management

Out of scope here. L4 Vault Broker owns disk encryption key management. AIOS-FS reads the unlocked volume after L1 boot completes.

## 10. Performance targets

### 10.1. Operation budgets (p95 on a reference single-host install)

| Operation                                 | p95 target      |
| ----------------------------------------- | --------------- |
| `ReadObject` (SNAPSHOT)                   | < 5 ms          |
| `ReadVersion` (SNAPSHOT)                  | < 5 ms          |
| `ReadChunk` (per MB)                      | < 50 ms         |
| `WriteVersion` (single chunk)             | < 50 ms         |
| `PromotePointer` (single)                 | < 10 ms         |
| `CommitTransaction` (single-object)       | < 20 ms         |
| `CommitTransaction` (multi-pointer, N=10) | < 100 ms        |
| `ExecuteQuery` (simple, 5–100 rows)       | < 50 ms         |
| `ExecuteQuery` (with full-text)           | < 500 ms        |
| `ExplainQuery`                            | < 10 ms         |
| `EnumerateObjects` (10 k objects)         | < 1 s           |
| `RebuildView` (1 k rows materialized)     | < 2 s           |
| GC pass throughput                        | ≥ 1000 chunks/s |
| Snapshot create (logical, 100 GB store)   | < 5 minutes     |
| Cold start (replay WAL, build indexes)    | < 30 s          |

Targets, not hard contracts. Acceptance fixtures (§14) verify behavioural correctness; performance suites verify these numbers separately.

### 10.2. Reference hardware

The targets assume:

- 8-core x86_64 CPU at ≥ 3 GHz
- 16 GB RAM
- NVMe SSD with ≥ 3 GB/s sequential and ≥ 100k IOPS random
- Single-host AIOS-FS instance

Smaller hardware is supported but degrades gracefully; targets are not contractual on smaller systems.

## 11. Resource budgets

### 11.1. Memory

| Subsystem              | Default budget | Configurable? |
| ---------------------- | -------------- | ------------- |
| RocksDB block cache    | 1 GB           | Yes           |
| RocksDB write buffer   | 256 MB         | Yes           |
| Tantivy index memory   | 512 MB         | Yes           |
| SQLite cache           | 64 MB          | Yes           |
| FUSE projector workers | 64 MB          | Yes           |
| Recovery inspector     | 256 MB         | Yes           |
| **Total floor**        | ~2 GB          | n/a           |

### 11.2. Disk

| Subsystem             | Floor                                                       | Notes                         |
| --------------------- | ----------------------------------------------------------- | ----------------------------- |
| Transaction log (WAL) | 1 GB                                                        | Bounded by checkpoint cadence |
| Catalog SQLite        | 1 GB                                                        | Grows with object count       |
| Tantivy lexical index | ~10% of indexed text                                        | Operator-tunable              |
| Chunk store           | bounded by user data; lifecycle eviction; reference-counted |

### 11.3. Backpressure

When budgets are exceeded:

- Reads continue.
- Writes are throttled (response delays grow); evidence emits a `BackpressureEvent`.
- Eventual rejection with `ResourceExhausted` if backpressure persists > 30 s.

## 12. Migration and import/export

### 12.1. POSIX → AIOS-FS import

A `posix-to-aiosfs` CLI tool walks a directory tree and produces AIOS-FS objects.

| POSIX feature         | AIOS-FS mapping                                           |
| --------------------- | --------------------------------------------------------- |
| File path             | `Object.metadata.name`; not authoritative identity        |
| File content          | Chunked + stored as `chunk_refs`                          |
| File mtime            | `Object.created_at` for first version                     |
| File mode/permissions | `metadata.extra.posix_mode` (lossy approximation)         |
| Symbolic link         | `Object.kind = FILE` with `metadata.extra.symlink_target` |
| Hard link             | Multiple objects sharing chunks via dedup                 |
| Extended attributes   | `metadata.extra.xattr.*`                                  |

Import is idempotent: re-running on the same tree updates content where chunk hashes differ; otherwise no-op.

### 12.2. AIOS-FS → POSIX export

Reverse direction is **lossy**. Export emits the current pointer's version as files; version history is not preserved by default. An optional `--include-history` flag emits per-version directories named by version ID.

### 12.3. AIOS-FS → AIOS-FS migration

For instance migration (e.g. move to new host), use logical export (§8.2) and import on the destination. Object IDs are preserved.

### 12.4. Backward migration

Migration **out** of AIOS-FS to a successor system is not in rev.2 scope; the export tooling is sufficient as an escape hatch.

## 13. Migration path stages

| Stage | Description                                |
| ----- | ------------------------------------------ |
| A     | Userspace object store + CLI inspector     |
| B     | FUSE projection for `/aios/views`          |
| C     | App sandbox portal integration             |
| D     | Optimized local cache                      |
| E     | Optional kernel acceleration for hot paths |

Stage E is **not required** for rev.2 correctness. Stages A–C are required for an end-to-end demonstration.

## 14. Acceptance criteria

- AIOS-FS runs without a custom kernel module.
- Recovery can inspect the store offline (no AI, no LLM, no Web UI).
- POSIX projection can be rebuilt from object metadata.
- Semantic indexes can be rebuilt from object metadata.
- WAL replay restores the system to a consistent state after crash.
- Snapshot create + restore reproduces the source instance bit-for-bit (modulo timestamps).
- Logical export → import on a fresh instance preserves object IDs and version history.
- POSIX → AIOS-FS import is idempotent.
- All golden fixtures from §15 pass against the implementation.
- Performance targets from §10.1 are met on reference hardware (separate perf suite).
- Resource budgets (§11) are not exceeded under typical load.

## 15. Golden fixtures

### 15.1. WAL replay after crash

```yaml
fixture_id: aiosfs.impl.fix.wal_replay.v1
scenario:
  - Begin transaction; write 5 versions; commit (WAL fsynced)
  - Kill -9 the AIOS-FS process before primary CF flush
  - Restart
expected:
  startup: replay applies all 5 writes
  state: COMMITTED
  evidence: replay event emitted
```

### 15.2. Atomic CAS via RocksDB write batch

```yaml
fixture_id: aiosfs.impl.fix.cas_atomic.v1
scenario:
  - Two concurrent CommitTransaction calls update same pointer
expected: exactly one transaction COMMITS; the other receives ConflictDetected
  no torn state in pointers CF
```

### 15.3. Logical snapshot round-trip

```yaml
fixture_id: aiosfs.impl.fix.snapshot_roundtrip.v1
scenario:
  - Source instance with 100 objects, 500 versions, 1000 chunks
  - ExportSnapshot -> tarball
  - ImportSnapshot on fresh instance
expected:
  destination has identical object IDs, version IDs, chunk IDs, content_hashes
  EnumerateObjects returns same set
```

### 15.4. POSIX import idempotent

```yaml
fixture_id: aiosfs.impl.fix.posix_import_idem.v1
scenario:
  - posix-to-aiosfs import /some/dir
  - posix-to-aiosfs import /some/dir   (no changes)
expected:
  second run: no new versions written
  evidence: zero "WriteVersion" events on second run
```

### 15.5. Encryption out of scope (negative test)

```yaml
fixture_id: aiosfs.impl.fix.no_encryption_implemented.v1
scenario:
  - Inspect AIOS-FS data directory bytes
expected: bytes are plaintext if underlying volume not encrypted
  AIOS-FS does NOT attempt to encrypt itself
note: encryption is operator's responsibility via LUKS/dm-crypt/ZFS native encryption
```

### 15.6. FUSE projection rebuild

```yaml
fixture_id: aiosfs.impl.fix.fuse_rebuild.v1
scenario:
  - Mount FUSE projection for /aios/views
  - Unmount and remount
expected: same view contents
  no object identity changed
```

### 15.7. Backpressure under write storm

```yaml
fixture_id: aiosfs.impl.fix.backpressure.v1
scenario:
  - 100k WriteVersion calls in 5 seconds
expected:
  some calls succeed, some receive throttle delay, eventual ResourceExhausted if budget exhausted
  BackpressureEvent emitted
  no data loss; no torn writes
```

## 16. Cross-spec dependencies

| Spec                         | Relationship                                                          |
| ---------------------------- | --------------------------------------------------------------------- |
| **S1.3** Object Model        | This spec implements the object model's gRPC surface.                 |
| **S1.3** Conflict Resolution | RocksDB CAS implements the pointer-move CAS protocol.                 |
| **S2.1** Query/View Language | Tantivy + SQLite back the query engine.                               |
| **S2.3** Policy Kernel       | Privacy ceiling source for queries; access checks on RPCs.            |
| **S3.1** Evidence Log        | WAL semantics and checkpointing align with evidence log architecture. |
| **L4 Vault Broker**          | Disk encryption key management (out of scope here).                   |
| **L1 Recovery**              | Recovery inspector reads object store offline.                        |

## 17. Open deferrals

- Per-object encryption (object-level wrapping by L4 vault) — future revision.
- Distributed multi-host replication — future L2 sub-spec.
- Kernel acceleration for hot read paths (Stage E) — future, only after measured need.
- Vector index integration (embedding similarity search) — L5 vector sub-spec.
- Tiered storage (hot SSD + cold HDD/object store) — future.

## 18. Telemetry contract

| Metric                                 | Type      | Labels      |
| -------------------------------------- | --------- | ----------- |
| `aiosfs_wal_fsync_seconds`             | histogram |             |
| `aiosfs_checkpoint_total`              | counter   | `outcome`   |
| `aiosfs_checkpoint_duration_seconds`   | histogram |             |
| `aiosfs_rocksdb_block_cache_hit_ratio` | gauge     |             |
| `aiosfs_tantivy_index_size_bytes`      | gauge     |             |
| `aiosfs_chunk_store_bytes_total`       | gauge     |             |
| `aiosfs_disk_used_bytes`               | gauge     | `subsystem` |
| `aiosfs_backpressure_events_total`     | counter   | `subsystem` |
| `aiosfs_snapshot_create_seconds`       | histogram | `tier`      |
| `aiosfs_snapshot_restore_seconds`      | histogram | `tier`      |
| `aiosfs_import_objects_total`          | counter   | `source`    |

Cardinality bounds: `subsystem` ≤ 8, `outcome` ≤ 4, `tier` = 3, `source` ≤ 4. No subject labels.

## 19. See also

- [S1.3 Object Model](01_object_model.md)
- [S1.3 Conflict Resolution](03_conflict_resolution.md)
- [S2.1 Query/View Language](02_query_view_language.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
- [RocksDB documentation](https://github.com/facebook/rocksdb/wiki)
- [Tantivy documentation](https://github.com/quickwit-oss/tantivy)
- [SQLite WAL mode](https://www.sqlite.org/wal.html)
