# AIOS-FS Implementation Space (Rev.2)

| Field     | Value                                  |
| --------- | -------------------------------------- |
| Status    | `CONTRACT` draft                       |
| Phase tag | S2.2                                   |
| Layer     | L2 AIOS-FS                             |
| Decision  | Userspace core with FUSE/portal projections first |

## 1. Purpose

This document chooses the implementation path for AIOS-FS without locking the project into premature kernel work.

## 2. Options considered

| Option                  | Strengths                               | Weaknesses                                  |
| ----------------------- | ---------------------------------------- | ------------------------------------------- |
| New kernel filesystem   | deep integration, maximum control         | high risk, hard recovery, long validation   |
| FUSE filesystem         | fast iteration, POSIX compatibility       | performance overhead, semantic impedance    |
| Userspace object store  | safest semantics, easy evidence/recovery  | requires projections for normal apps        |
| Hybrid                  | object store plus FUSE/portal projections | more moving parts, best staged path         |

## 3. Decision

Rev.2 chooses a hybrid path:

```text
AIOS-FS authoritative userspace object store
  -> FUSE read/write projection for compatible POSIX paths
  -> portals/bind mounts for app sandboxes
  -> future kernel module only after contracts are proven
```

The authoritative store is not POSIX. POSIX is a compatibility projection.

## 4. Rationale

- Recovery tools can inspect object records without loading a custom kernel module.
- Versioning and evidence semantics are easier in userspace.
- Semantic indexes can evolve independently from kernel constraints.
- FUSE is good enough for prototype and compatibility.
- Kernel-space work is deferred until object model and conflict semantics are stable.

## 5. Required components

| Component              | Role                                      |
| ---------------------- | ----------------------------------------- |
| object store           | objects, versions, chunks, metadata       |
| transaction log        | crash consistency and replay              |
| pointer store          | atomic pointer moves                      |
| indexer                | rebuildable semantic and lexical indexes  |
| FUSE projector         | POSIX-compatible paths                    |
| app portal             | scoped app access                         |
| recovery inspector     | read/verify/rollback without AI           |

## 6. Migration path

| Stage | Description                                  |
| ----- | -------------------------------------------- |
| A     | userspace object store and CLI inspector      |
| B     | FUSE projection for `/aios/views`             |
| C     | app sandbox portal integration                |
| D     | optimized local cache                         |
| E     | optional kernel acceleration for hot paths    |

Stage E is not required for Rev.2 correctness.

## 7. Acceptance criteria

- AIOS-FS can run without a custom kernel module.
- Recovery can inspect the store offline.
- POSIX projection can be rebuilt.
- Semantic indexes can be rebuilt.
- Kernel work remains optional until measured need exists.

