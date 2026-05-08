# AIOS-FS Conflict Resolution (Rev.2)

| Field     | Value                                                                |
| --------- | -------------------------------------------------------------------- |
| Status    | `CONTRACT` (refined 2026-05-08; awaiting implementation evidence)    |
| Phase tag | S1.3                                                                 |
| Layer     | L2 AIOS-FS                                                           |
| Consumes  | L2 object model, L0 evidence/status, S0.1 verification intents       |
| Produces  | conflict records, merge proposals, merge policies, resolution events |
| Approved  | 2026-05-08 (deltas D1–D12 applied; replaces draft from `dfa3be5`)    |

## 1. Purpose

AIOS-FS must handle concurrent writes without silent data loss. The default model is **optimistic concurrency with immutable versions and explicit conflict objects**. Concurrent writes never overwrite each other; they create sibling versions. Promotion decides which version becomes `current`, and unresolved promotions become first-class `Conflict` records.

This sub-spec is the companion to `01_object_model.md`. It uses the object/version/pointer/transaction vocabulary defined there and the CAS protocol from §8.2 of that document. Hash encoding rules (§3.2 of `01_object_model.md`) apply.

## 2. Core rule

```text
Concurrent writes never overwrite each other.
They create sibling versions.
Promotion decides which version becomes current.
```

A failed pointer-promotion CAS produces a `Conflict` record. The candidate version persists in `STAGED` state and becomes a sibling of the version that won the race.

## 3. Conflict detection

A conflict exists when a transaction attempts to promote a pointer based on a parent version that is no longer the pointer's current version:

```text
expected_current_version_id != pointer.current_version_id
```

This is precisely the failure condition of `PromotePointer` (object model §8.2). The runtime emits a `Conflict` record automatically; the calling transaction is aborted unless a merge policy allows automatic resolution (§5).

## 4. Conflict record

```proto
message Conflict {
  string conflict_id = 1;                          // "cnf_<ULID>"
  string object_id = 2;
  string pointer_id = 3;
  string base_version_id = 4;                      // common ancestor (ver_root or ver_X)
  string current_version_id = 5;                   // the version that won the race
  string candidate_version_id = 6;                 // the version that lost
  string detected_by_transaction_id = 7;
  google.protobuf.Timestamp detected_at = 8;
  ConflictStatus status = 9;
  string resolution_version_id = 10;
  google.protobuf.Timestamp resolved_at = 11;
  string resolved_by_subject = 12;
  string evidence_receipt_id = 13;
  google.protobuf.Timestamp ttl_abandons_at = 14;
}

enum ConflictStatus {
  CONFLICT_STATUS_UNSPECIFIED = 0;
  OPEN          = 1;
  AUTO_MERGED   = 2;
  USER_RESOLVED = 3;
  ABANDONED     = 4;
}
```

Conflicts are first-class objects, evidence-linked, and queryable via `ListConflicts` (object model gRPC surface).

## 5. Resolution modes

| Mode                 | Use case                               | Requirement                       |
| -------------------- | -------------------------------------- | --------------------------------- |
| `REJECT_PROMOTION`   | Default for unknown object kinds       | No data loss; user/agent must act |
| `LAST_WRITER_STAGED` | Candidate saved but not promoted       | User or agent review              |
| `STRUCTURAL_MERGE`   | JSON/YAML/metadata with non-overlap    | Schema-aware merge tool           |
| `CRDT_MERGE`         | Evidence, memory, append-only logs     | CRDT type declared by object kind |
| `MANUAL_RESOLUTION`  | Source code, configs, binary conflicts | New resolution version            |

```proto
enum ResolutionMode {
  RESOLUTION_MODE_UNSPECIFIED = 0;
  REJECT_PROMOTION    = 1;
  LAST_WRITER_STAGED  = 2;
  STRUCTURAL_MERGE    = 3;
  CRDT_MERGE          = 4;
  MANUAL_RESOLUTION   = 5;
}
```

**No object kind gets automatic merge unless its merge policy is declared.** Implicit auto-merge is forbidden.

## 6. CRDT vocabulary

Closed set. Per-object-kind merge policy declares which type applies.

| CRDT type      | Semantics                                                                        | Typical use                            |
| -------------- | -------------------------------------------------------------------------------- | -------------------------------------- |
| `G_COUNTER`    | Grow-only counter; per-replica increments combine via max.                       | Read counters, view hits.              |
| `PN_COUNTER`   | Increment + decrement counter; tracks positive and negative monotonic streams.   | Reference counts.                      |
| `OR_SET`       | Observed-remove set; adds always win over concurrent removes; stable add/remove. | Tag sets, label sets.                  |
| `LWW_REGISTER` | Last-writer-wins register with explicit timestamp + tiebreaker (subject).        | Single-value fields where loss is OK.  |
| `OR_MAP`       | Observed-remove map; values are themselves CRDTs.                                | Structured metadata maps.              |
| `RGA_TEXT`     | Replicated growable array for text; preserves character-level concurrent edits.  | Plain-text editing of small documents. |

```proto
enum CrdtType {
  CRDT_TYPE_UNSPECIFIED = 0;
  G_COUNTER     = 1;
  PN_COUNTER    = 2;
  OR_SET        = 3;
  LWW_REGISTER  = 4;
  OR_MAP        = 5;
  RGA_TEXT      = 6;
}
```

CRDTs not in this enum are **not supported** for automatic merge in rev.2. Future CRDTs require an additive enum bump (per S0.1 §8 versioning rules).

## 7. Merge policy

Merge policies are part of the object-kind schema. They are not invented at conflict time.

```yaml
object_kind: memory
merge_policy:
  mode: CRDT_MERGE
  crdt: OR_SET
  allowed_fields:
    - metadata.labels
    - facts
  ai_proposals_allowed: true
  ai_auto_promote: false
```

```proto
message MergePolicy {
  ObjectKind object_kind = 1;                   // from object model
  ResolutionMode mode = 2;
  CrdtType crdt = 3;                            // only meaningful when mode = CRDT_MERGE
  repeated string allowed_fields = 4;
  bool ai_proposals_allowed = 5;
  bool ai_auto_promote = 6;                     // false unless strict policy + low-risk class
}
```

`ai_auto_promote=true` requires:

- Object's `privacy_class ∈ {PUBLIC, INTERNAL}` (never for SENSITIVE, SECRET_BEARING, or CLASSIFIED).
- Merge policy explicitly opts in via L4 policy decision.
- Verification intents in the proposal pass before promotion.

Otherwise AI proposals are staged and require human resolution.

## 8. AI role

AI may **propose** a merge version. It may not silently promote it unless §7's `ai_auto_promote` conditions are all met.

### 8.1. Proposal record

```proto
message MergeProposal {
  string proposal_id = 1;                       // "mpr_<ULID>"
  string conflict_id = 2;
  string base_version_id = 3;
  string current_version_id = 4;
  string candidate_version_id = 5;
  string proposed_resolution_version_id = 6;    // staged version; not yet promoted
  string explanation = 7;                       // human-readable; ≤ 4096 chars
  repeated VerificationIntent verification = 8; // S0.1 type
  string proposed_by_subject = 9;
  bool ai_generated = 10;
  string evidence_receipt_id = 11;
  google.protobuf.Timestamp proposed_at = 12;
}
```

### 8.2. Validation rules

A `MergeProposal` is **rejected** by the runtime if any of:

- `conflict_id` is not in `OPEN` status.
- `proposed_resolution_version_id` does not exist or is not `STAGED`.
- The proposal does not include all required fields (base, current, candidate, resolution, explanation).
- `verification` is empty (every state-changing merge needs at least one verification intent unless the merge policy explicitly waives this).
- `evidence_receipt_id` is empty.
- AI-generated proposal lacks redaction of secret-shaped substrings in `explanation` (per S1.1 §17.2.6).
- AI-generated proposal references a version with `privacy_class > INTERNAL` while merge policy disallows AI involvement at that class.

Validation is performed in code, not by convention. Failed proposals do not enter the queue and emit a rejection evidence record.

## 9. Conflict notification

### 9.1. Push (event stream)

The L9 evidence stream emits an event when a conflict transitions states (`OPEN → AUTO_MERGED | USER_RESOLVED | ABANDONED`). Subscribers (renderers, agents, monitoring) can filter by `object_id`, `pointer_id`, or `subject`.

### 9.2. Pull (RPC)

`ListConflicts` (object model gRPC) supports filters and is suitable for periodic polling or initial state load.

### 9.3. Notification budget

To prevent notification storms, conflict events are **debounced**: bursts of conflicts on the same object within 5 seconds are coalesced into one event with a counter. Coalescing is operator-configurable.

## 10. Resolution authority

A conflict can be resolved by:

| Authority class            | Permission source                                                                                |
| -------------------------- | ------------------------------------------------------------------------------------------------ |
| **Object owner**           | The `created_by` subject (or its delegated successor).                                           |
| **Object collaborator**    | A subject with capability `aiosfs.conflict.resolve` for this object's project/scope.             |
| **Operator with override** | A subject with capability `aiosfs.conflict.resolve.any`; subject to L4 emergency-override rules. |

L4 Policy Kernel evaluates the resolver's authority. Resolution attempts by non-authorized subjects return `PERMISSION_DENIED`.

## 11. Conflict timeout and abandonment

Open conflicts have a TTL. Default: **30 days** from detection.

When the TTL elapses without resolution:

- `status` transitions to `ABANDONED`.
- Both `current_version_id` (winner) and `candidate_version_id` (loser) remain readable for audit.
- The candidate version is marked `RETIRED_VERSION`; its chunks become GC-eligible per object model §7.3.
- Operator may shorten the TTL via policy; lengthening is always allowed.
- An evidence record is emitted for the abandonment.

Abandonment is **not** silent data loss: the candidate version still exists in audit-readable form until garbage collection runs (which itself emits evidence).

## 12. Multi-object transaction conflicts

When a single transaction attempts multiple pointer promotions (object model §8.3), CAS is evaluated for each. Outcomes:

| Scenario                        | Result                                                                                                            |
| ------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| All CAS succeed                 | Transaction commits; no conflicts.                                                                                |
| Any CAS fails                   | Transaction aborts; **all** sibling versions persist as `STAGED`; **one conflict per failed pointer** is emitted. |
| Mixed (some success, some fail) | Same as "any CAS fails" — atomicity wins; partial commits are forbidden.                                          |

This means a single transaction can produce **multiple `Conflict` records** in one call. Resolvers can address them independently or atomically (a follow-up resolver transaction may bundle all the resolutions).

## 13. Resolution flow

```text
Conflict opened (status=OPEN)
   │
   ├── automatic path (if merge_policy permits)
   │      │
   │      ├── compute structural or CRDT merge
   │      ├── validate against merge_policy.allowed_fields
   │      ├── stage resolution version
   │      ├── apply verification intents
   │      ├── promote pointer (CAS again — may fail if state moved)
   │      └── status → AUTO_MERGED
   │
   └── manual / AI-proposed path
          │
          ├── propose MergeProposal
          ├── proposal validation (§8.2)
          ├── stage proposed_resolution_version_id
          ├── if AI-auto-promote allowed AND validation passes: promote
          ├── else: present to authorized resolver
          ├── resolver accepts or rejects via ResolveConflict
          └── status → USER_RESOLVED or remains OPEN
```

If a resolution attempt fails its own CAS (because state moved again during resolution), a **new** conflict is emitted; the original conflict remains `OPEN`.

## 14. Cross-spec dependencies

| Spec                           | Relationship                                                                                                   |
| ------------------------------ | -------------------------------------------------------------------------------------------------------------- |
| **S0.1** Action Envelope       | `verification` intents on `MergeProposal` use the S0.1 `VerificationIntent` shape.                             |
| **S1.1** Capability Translator | AI merge proposals come from L5; translator obeys §17.2.6 secret-shaped redaction before forming the proposal. |
| **S1.2** Latency Tiering       | `PrivacyClass` constrains `ai_auto_promote` per §7.                                                            |
| **S1.3** Object Model          | `01_object_model.md` defines the CAS protocol, multi-pointer atomicity, and gRPC surface used here.            |
| **S2.3** Policy Kernel         | Authority to resolve conflicts is a policy decision; merge policies are policy objects.                        |
| **S2.4** Verification Grammar  | Merge proposal verification intents follow S2.4 grammar.                                                       |
| **S3.1** Evidence Log          | Every conflict event (open, propose, validate, resolve, abandon) writes evidence.                              |

## 15. Open deferrals

- Cross-replica conflict resolution (multi-master AIOS-FS) → future L2 sub-spec.
- Three-way text merge tooling (beyond `RGA_TEXT`) → future tooling sub-spec.
- Custom CRDT registration by third-party object kinds → L10 marketplace governance.
- Conflict-driven UI ergonomics (renderer-side) → L7 renderer sub-spec.

## 16. Acceptance criteria

- Lost update is impossible by construction.
- Conflict records are queryable via push (evidence stream) and pull (`ListConflicts`).
- Automatic merge is opt-in per object kind (default mode is `REJECT_PROMOTION`).
- AI merge proposals are staged, validated, and never silently promoted unless explicit auto-promote is policy-permitted AND privacy class allows.
- AI proposals failing §8.2 validation are rejected with evidence.
- Pointer history shows conflict and resolution lineage.
- Conflicts time out to `ABANDONED` after configured TTL with evidence trail.
- All golden fixtures from §17 pass against the implementation.
- Multi-pointer transactions emit one conflict per failed pointer.
- Resolution authority is enforced via L4 policy.
- Telemetry metrics from §18 are emitted.

## 17. Golden fixtures

### 17.1. Default reject promotion

```yaml
fixture_id: aiosfs.cnf.fix.default_reject.v1
scenario:
  - obj_x.kind = APPLICATION (no merge policy declared by default)
  - txn_A and txn_B both write versions with parent=ver_root
  - txn_A promotes first (succeeds)
  - txn_B promotes (CAS fails)
expected:
  conflict.status: OPEN
  pointer.current_version: ver_A (txn_A's)
  ver_B.state: STAGED
  resolution_required: true
```

### 17.2. CRDT auto-merge for memory object

```yaml
fixture_id: aiosfs.cnf.fix.crdt_or_set_memory.v1
scenario:
  - obj_m.kind = MEMORY
  - merge_policy:
      {
        mode: CRDT_MERGE,
        crdt: OR_SET,
        allowed_fields: [metadata.labels],
        ai_auto_promote: true,
      }
  - txn_A adds label "rust", txn_B adds label "linux", same parent
  - txn_A commits first; txn_B's promotion CAS fails
expected:
  conflict.status: AUTO_MERGED (via OR_SET union)
  resolution_version.metadata.labels: ["rust", "linux"]
  evidence_chain_present: true
```

### 17.3. AI proposal validation rejects secret-shaped explanation

```yaml
fixture_id: aiosfs.cnf.fix.ai_proposal_redaction.v1
scenario:
  - AI submits MergeProposal with explanation containing "ghp_AAA..." token
expected:
  proposal_status: REJECTED
  rejection_reason: "secret_shaped_substring_in_explanation"
  evidence_emitted: true
  no_resolution_staged: true
```

### 17.4. Multi-pointer transaction emits multiple conflicts

```yaml
fixture_id: aiosfs.cnf.fix.multi_pointer_conflicts.v1
scenario:
  - txn_T promotes ptr_a (CAS OK) and ptr_b (CAS fail) and ptr_c (CAS fail) in one transaction
expected:
  txn_T.state: ABORTED
  conflicts_emitted: 2 (one for ptr_b, one for ptr_c)
  ptr_a.current_version: unchanged (rolled back as part of atomic abort)
  staged_versions_persisted: 3
```

### 17.5. Conflict abandonment after TTL

```yaml
fixture_id: aiosfs.cnf.fix.ttl_abandon.v1
scenario:
  - Open conflict cnf_x at T0
  - No resolution by T0 + 30 days
  - Background TTL pass runs
expected:
  cnf_x.status: ABANDONED
  candidate_version.state: RETIRED_VERSION
  evidence_emitted: true
```

### 17.6. Unauthorized resolution attempt denied

```yaml
fixture_id: aiosfs.cnf.fix.unauthorized_resolve.v1
scenario:
  - cnf_x exists, owner is human:lucky
  - subject agent:dev (no aiosfs.conflict.resolve capability) calls ResolveConflict
expected:
  result: PERMISSION_DENIED
  reason: "subject_lacks_capability"
  cnf_x.status: unchanged (still OPEN)
```

### 17.7. Resolution race produces new conflict

```yaml
fixture_id: aiosfs.cnf.fix.resolution_race.v1
scenario:
  - cnf_x is OPEN with current=ver_C, candidate=ver_D
  - resolver R1 stages ver_R1 as resolution
  - while R1's promotion is in flight, txn_E commits ver_E to the same pointer
  - R1's promotion CAS fails (expected ver_C, found ver_E)
expected:
  cnf_x.status: still OPEN
  new_conflict_emitted: cnf_y (between ver_R1 and ver_E)
```

## 18. Telemetry contract

| Metric                              | Type      | Labels                                        |
| ----------------------------------- | --------- | --------------------------------------------- |
| `aiosfs_conflicts_open`             | gauge     | `object_kind`                                 |
| `aiosfs_conflicts_resolved_total`   | counter   | `object_kind`, `resolution_mode`              |
| `aiosfs_conflicts_abandoned_total`  | counter   | `object_kind`                                 |
| `aiosfs_merge_proposals_total`      | counter   | `outcome` (accepted/rejected/promoted/staged) |
| `aiosfs_ai_merge_proposals_total`   | counter   | `outcome`                                     |
| `aiosfs_resolution_latency_seconds` | histogram | `resolution_mode`                             |

Cardinality bounds: `object_kind` ≤ 11, `resolution_mode` = 5, `outcome` ≤ 5. Subject is **never** a metric label.

## 19. See also

- [S0.1 Action Envelope + Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S1.3 Object Model](01_object_model.md)
- [S2.3 Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S2.4 Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

## Appendix B: Conflict-related proto IDL

The full filesystem proto package `aios.fs.v1alpha1` is defined in **Appendix A of `01_object_model.md`**. Conflict-specific additions are below:

```proto
syntax = "proto3";
package aios.fs.v1alpha1;

import "google/protobuf/timestamp.proto";
import "aios/action/v1alpha1/action.proto";   // for VerificationIntent

// ─────────────────────────────────────────────────────────────────
// Conflict and merge records
// ─────────────────────────────────────────────────────────────────

message Conflict {
  string conflict_id = 1;
  string object_id = 2;
  string pointer_id = 3;
  string base_version_id = 4;
  string current_version_id = 5;
  string candidate_version_id = 6;
  string detected_by_transaction_id = 7;
  google.protobuf.Timestamp detected_at = 8;
  ConflictStatus status = 9;
  string resolution_version_id = 10;
  google.protobuf.Timestamp resolved_at = 11;
  string resolved_by_subject = 12;
  string evidence_receipt_id = 13;
  google.protobuf.Timestamp ttl_abandons_at = 14;
}

enum ConflictStatus {
  CONFLICT_STATUS_UNSPECIFIED = 0;
  OPEN          = 1;
  AUTO_MERGED   = 2;
  USER_RESOLVED = 3;
  ABANDONED     = 4;
}

enum ResolutionMode {
  RESOLUTION_MODE_UNSPECIFIED = 0;
  REJECT_PROMOTION    = 1;
  LAST_WRITER_STAGED  = 2;
  STRUCTURAL_MERGE    = 3;
  CRDT_MERGE          = 4;
  MANUAL_RESOLUTION   = 5;
}

enum CrdtType {
  CRDT_TYPE_UNSPECIFIED = 0;
  G_COUNTER     = 1;
  PN_COUNTER    = 2;
  OR_SET        = 3;
  LWW_REGISTER  = 4;
  OR_MAP        = 5;
  RGA_TEXT      = 6;
}

message MergePolicy {
  ObjectKind object_kind = 1;
  ResolutionMode mode = 2;
  CrdtType crdt = 3;
  repeated string allowed_fields = 4;
  bool ai_proposals_allowed = 5;
  bool ai_auto_promote = 6;
}

message MergeProposal {
  string proposal_id = 1;
  string conflict_id = 2;
  string base_version_id = 3;
  string current_version_id = 4;
  string candidate_version_id = 5;
  string proposed_resolution_version_id = 6;
  string explanation = 7;
  repeated aios.action.v1alpha1.VerificationIntent verification = 8;
  string proposed_by_subject = 9;
  bool ai_generated = 10;
  string evidence_receipt_id = 11;
  google.protobuf.Timestamp proposed_at = 12;
}

// SubmitMergeProposal RPC is added to AIOSFSObjects service:
//   rpc SubmitMergeProposal(MergeProposal) returns (MergeProposal); // returns proposal with assigned proposal_id and validation outcome
```
