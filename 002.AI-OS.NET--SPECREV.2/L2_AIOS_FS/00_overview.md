# L2 — AIOS-FS

Status: `PARTIAL`

## Responsibility

AIOS-FS is the native semantic filesystem mounted at `/aios`. Files are versioned objects, paths are query-backed views, and every write has intent, policy, provenance, rollback, and evidence linkage. AIOS-FS is authoritative storage, not a sidecar over POSIX.

## Layer invariants (from Rev.1 §6, §9)

- Filesystem truth must not depend on L7 UI.
- Committed data is never overwritten in place; writes create new versions.
- Pointer moves promote state; rollback moves a pointer to a previous verified version.
- Semantic views are not authoritative storage identity.
- Semantic indexes are rebuildable from object metadata.
- Recovery can inspect objects without the Cognitive Core.

## Dependencies

May depend on: L0, L1.

## Planned sub-specs

| File                         | Topic                                                                           | Status  | Phase |
| ---------------------------- | ------------------------------------------------------------------------------- | ------- | ----- |
| `01_object_model.md`         | Object/version/chunk identity, metadata schema, write flow                      | `CONTRACT` | S1.3  |
| `02_query_view_language.md`  | Semantic views — query language, evaluation, projections                        | `CONTRACT` | S2.1  |
| `03_conflict_resolution.md`  | Concurrent writes — optimistic concurrency, CRDT scope, merge UX                | `CONTRACT` | S1.3  |
| `04_implementation_space.md` | kernel-space vs FUSE vs userspace projection — chosen approach + migration path | `CONTRACT` | S2.2  |
| `05_recovery_modes.md`       | normal / safe_readonly / repair / quarantine / reindex modes                    | `SHELL` | —     |
| `06_pointer_promotion.md`    | Atomicity of pointer moves; crash consistency; evidence linkage on promotion    | `SHELL` | —     |

## Open questions (to resolve via sub-specs)

- Backing store for content-addressed chunks: RocksDB, sled, custom WAL?
- Index store for semantic views: SQLite, Tantivy, custom?
- Conflict resolution policy: optimistic + retry, CRDT for evidence/memory, namespacing?
- Query language family: EAV + Datalog (Datomic-style), GraphQL, custom DSL?
- POSIX projection scope: read-only views, read-write, capabilities-only?

## See also

- [Rev.1 §9 — AIOS-FS](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
