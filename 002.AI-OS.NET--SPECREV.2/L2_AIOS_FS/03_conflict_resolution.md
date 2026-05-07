# AIOS-FS Conflict Resolution (Rev.2)

| Field     | Value                                      |
| --------- | ------------------------------------------ |
| Status    | `CONTRACT` draft                           |
| Phase tag | S1.3                                       |
| Layer     | L2 AIOS-FS                                |
| Consumes  | L2 object model, L0 evidence/status        |
| Produces  | conflict records, merge versions, policies |

## 1. Purpose

AIOS-FS must handle concurrent writes without silent data loss. The default model is optimistic concurrency with immutable versions and explicit conflict objects.

## 2. Core rule

Concurrent writes never overwrite each other. They create sibling versions. Promotion decides which version becomes `current`.

## 3. Conflict detection

A conflict exists when a transaction attempts to promote a pointer based on a parent version that is no longer the pointer's current version.

```text
expected_parent != pointer.current_version_id
```

The write may still commit as a version, but pointer promotion is blocked unless a merge policy resolves it.

## 4. Conflict record

```json
{
  "conflict_id": "cnf_...",
  "object_id": "obj_...",
  "pointer_id": "ptr_...",
  "base_version_id": "ver_base",
  "current_version_id": "ver_current",
  "candidate_version_id": "ver_candidate",
  "detected_by_transaction_id": "txn_...",
  "status": "open | auto_merged | user_resolved | abandoned",
  "resolution_version_id": null
}
```

Conflicts are first-class objects and are evidence-linked.

## 5. Resolution modes

| Mode              | Use case                                | Requirement                          |
| ----------------- | ---------------------------------------- | ------------------------------------ |
| Reject promotion  | default for unknown object kinds          | no data loss                         |
| Last writer staged | candidate saved but not promoted         | user or agent review                 |
| Structural merge  | JSON/YAML/metadata with non-overlap       | schema-aware merge                   |
| CRDT merge        | evidence, memory, append-only logs        | CRDT type declared by object kind    |
| Manual resolution | source code, configs, binary conflicts    | new resolution version               |

No object kind gets automatic merge unless its merge policy is declared.

## 6. Merge policy

```yaml
object_kind: memory
merge_policy:
  mode: crdt
  crdt: observed_remove_set
  allowed_fields:
    - metadata.labels
    - facts
```

Merge policies are part of object-kind schema. They are not invented at conflict time.

## 7. AI role

AI may propose a merge version. It may not silently promote it unless policy allows auto-merge for that object kind and risk class.

AI-generated merge proposals must include:

- base version
- current version
- candidate version
- proposed resolution version
- explanation
- verification intents
- evidence record

## 8. Acceptance criteria

- Lost update is impossible by construction.
- Conflict records are queryable.
- Automatic merge is opt-in per object kind.
- AI merge proposals are staged, not silently promoted.
- Pointer history shows conflict and resolution.

