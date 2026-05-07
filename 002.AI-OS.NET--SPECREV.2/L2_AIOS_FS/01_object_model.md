# AIOS-FS Object Model (Rev.2)

| Field     | Value                                            |
| --------- | ------------------------------------------------ |
| Status    | `CONTRACT` draft                                 |
| Phase tag | S1.3                                             |
| Layer     | L2 AIOS-FS                                      |
| Consumes  | L0 evidence/status, L1 host/recovery substrate   |
| Produces  | objects, versions, chunks, pointers, transactions |

## 1. Purpose

AIOS-FS is the authoritative semantic filesystem rooted at `/aios`. It stores durable objects, immutable versions, content-addressed chunks, semantic metadata, and pointer moves.

POSIX paths are projections. Object identity is not the path.

## 2. Core entities

| Entity      | Meaning                                                |
| ----------- | ------------------------------------------------------ |
| Object      | Stable logical identity: app, file, project, memory.   |
| Version     | Immutable state of an object at one point.             |
| Chunk       | Content-addressed bytes referenced by versions.        |
| Pointer     | Mutable named reference to a version.                  |
| Transaction | Atomic write or pointer move with evidence linkage.    |
| View        | Query-backed projection over objects and metadata.     |

## 3. Identity

IDs use prefix-namespaced ULIDs unless content-addressed:

| Prefix | Entity      | Example        |
| ------ | ----------- | -------------- |
| `obj_` | Object      | `obj_01HX...`  |
| `ver_` | Version     | `ver_01HX...`  |
| `txn_` | Transaction | `txn_01HX...`  |
| `ptr_` | Pointer     | `ptr_01HX...`  |
| `chk_` | Chunk       | `chk_blake3...` |

Chunk IDs are derived:

```text
chunk_id = "chk_" + BLAKE3(bytes)
```

## 4. Object record

```json
{
  "object_id": "obj_...",
  "kind": "project | app | file | memory | policy | model | package | evidence_ref",
  "created_at": "...",
  "created_by": "human:lucky",
  "current_pointer_id": "ptr_...",
  "metadata": {
    "name": "nginx-site",
    "labels": ["web", "config"],
    "mime": "text/plain"
  },
  "policy_tags": ["user-data"],
  "index_hints": ["fulltext", "semantic"]
}
```

`metadata` is queryable but not authoritative security state. Policy decisions use explicit policy tags and object ownership, not embeddings.

## 5. Version record

```json
{
  "version_id": "ver_...",
  "object_id": "obj_...",
  "parent_version_ids": ["ver_..."],
  "chunk_refs": ["chk_..."],
  "content_hash": "blake3:...",
  "metadata_delta": {},
  "created_by_action_id": "act_...",
  "created_by_transaction_id": "txn_...",
  "created_at": "...",
  "state": "staged | verified | quarantined | retired"
}
```

Versions are immutable. Correction creates a new version.

## 6. Pointer record

Pointers name operational state.

| Pointer      | Meaning                                  |
| ------------ | ---------------------------------------- |
| `current`    | active version shown in normal views      |
| `stable`     | last verified stable version              |
| `candidate`  | staged version awaiting verification      |
| `rollback`   | version to restore if promotion fails     |
| `quarantine` | version isolated after validation failure |

Pointer moves are atomic transactions. Rollback is a pointer move, not in-place mutation.

## 7. Write flow

```text
begin transaction
  -> validate subject and policy tags
  -> write chunks
  -> create immutable version
  -> attach provenance and action id
  -> verify version if required
  -> promote pointer if allowed
  -> emit evidence
commit transaction
```

If any required step fails, the transaction aborts and no pointer is promoted. Orphan chunks may remain only in garbage-collectable staging.

## 8. `/aios` layout

The root namespace is operational, not arbitrary.

```text
/aios/
  objects/
  views/
  apps/
  users/
  projects/
  memory/
  policies/
  packages/
  runtime/
  evidence/
  recovery/
```

`/aios/objects` exposes object identity. `/aios/views` exposes query projections. `/aios/apps` and `/aios/projects` are stable human-facing views over objects.

## 9. POSIX projection

AIOS-FS may expose POSIX-compatible paths for applications, but the projection must obey:

- object id remains canonical
- writes create versions
- broad host writes are denied
- app writes stay inside app-owned state
- view rebuild must not change object identity

Applications that cannot tolerate versioned write semantics run in compatibility sandboxes with explicit state mapping.

## 10. Recovery requirements

Recovery tools must be able to:

- enumerate objects without Cognitive Core
- verify chunk hashes
- inspect pointer history
- roll a pointer back to a verified version
- rebuild semantic indexes from object metadata
- quarantine corrupt versions

The recovery path depends on L1 and L2 only.

## 11. Acceptance criteria

- No committed object content is overwritten in place.
- Every write has transaction and evidence linkage.
- Pointer promotion is atomic.
- Chunk integrity is content-addressed.
- Semantic indexes can be deleted and rebuilt.
- Recovery can inspect and roll back without AI.

