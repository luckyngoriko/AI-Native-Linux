# AIOS-FS Namespace Layout (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Status         | `CONTRACT` (promoted 2026-05-09 from `DRAFT NOTES`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| Phase tag      | S4.1                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| Layer          | L2 AIOS-FS                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Schema package | `aios.namespace.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Consumes       | **Imports vocabulary from**: S0.1 (action target shape — type-level), S2.3 (policy condition predicates — type-level), S2.4 (verification path-property shape — type-level), S3.1 (record-scoping shape — type-level), S3.2 (sandbox boundary descriptor — type-level), L4 identity (`Subject` canonical-id format and group/user identifier closed enums — type-level identifier shape owned by L4 and imported here to scope the path lattice), L5 agent objects (agent-object identifier closed enum — type-level), L6 app install scope (app-install scope-name closed enum — type-level). **Peer (intra-L2)**: S1.3 (object model), S2.1 (views). **Note**: S4.1 itself _produces_ the scope-path enum that L4/L5/L6 in turn consume; the upward references above are vocabulary co-definition, not runtime dependency on higher-numbered layers. |
| Produces       | typed `NamespacePath`, reserved-name catalog, scope-bound resolution, cross-spec invariants                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |

## 1. Purpose

AIOS-FS is the canonical object store; the user-visible directory tree is one projection. This spec fixes that projection so all higher layers (policy, evidence, sandbox, identity, agents, apps) reference a single, closed, deterministic layout. The Sandbox Composer compiles enforcement against group-scoped paths; the Policy Kernel evaluates conditions over `target.scope`/`target.group_id`/`target.user_id`; the Evidence Log scopes records by group; the Capability Translator emits actions whose targets resolve to typed namespace paths. Without a fixed namespace, those contracts drift.

This spec defines:

1. The closed top-level layout under `/aios/`.
2. The closed reserved-name vocabulary at every scope (system, group, user).
3. The path-resolution algorithm and the typed `NamespacePath` produced.
4. Cross-group access semantics (forbidden by default).
5. The system/admin/recovery boundary.
6. The inbox semantics (group + user, both virtual views).
7. The required touch-ups in eight existing contracts so the namespace propagates consistently.

## 2. Core invariants

- **I1 — Closed top-level vocabulary.** `/aios/` directly contains exactly two reserved entries: `system/` and `groups/`. No third top-level entry is ever created.
- **I2 — Closed reserved-name vocabularies at every scope.** Within each scope (system, group, user), the immediate reserved subdirectory names are enumerated by closed enums. Adding a name is a versioned spec change.
- **I3 — Recovery boundary preserved.** `/aios/system/recovery/` exists at a predictable path independent of any group state. L1 recovery never traverses `/aios/groups/...`.
- **I4 — Default-deny cross-group access.** A subject in group A cannot read, list, or write paths under `/aios/groups/<B>/...` for any `B ≠ A`. Exception: subjects in the `_system` scope under recovery mode + system_audit_read capability + human approver.
- **I5 — Admin operations are recovery-bound.** Mutations to `/aios/system/policy/`, `/aios/system/capabilities/`, `/aios/system/vault/`, `/aios/system/recovery/` require `is_recovery_mode = true`, a human approver, and a `RECOVERY_EVENT` evidence record (FOREVER retention).
- **I6 — Reserved IDs.** Group/user/agent/project IDs starting with `_` are reserved for AIOS internal use and cannot be created by user actions. The `_system` scope is the only reserved group id and is not addressable as `groups/_system/`; it lives at the top level (`/aios/system/`).
- **I7 — Determinism.** Path-string → typed `NamespacePath` is deterministic given the namespace catalog version. Same string + same catalog → identical result.
- **I8 — Path traversal forbidden.** `..`, `.`, double-slashes, and segment counts > 32 are rejected at parse time.

## 3. Reserved top-level layout

```text
/aios/
├── system/                # AIOS itself; never under user/group control
└── groups/
    └── <group_id>/        # one directory per group; group_id matches §7 regex
```

Closed enum:

```proto
enum TopLevelReservedName {
  TOP_LEVEL_RESERVED_NAME_UNSPECIFIED = 0;
  SYSTEM = 1;
  GROUPS = 2;
}
```

Anything else at top level is rejected by the namespace resolver with `InvalidTopLevel`.

## 4. Per-system reserved subdirectories

```text
/aios/system/
├── apps/                  # system-level installed apps (evidence viewer, policy admin, etc.)
├── agents/                # system AI (translator, planner, recovery diagnostics)
├── policy/                # signed policy bundles (S2.3); recovery-only mutation
├── capabilities/          # capability catalog (S1.1); recovery-only mutation
├── evidence/              # evidence log segments (S3.1); append-only
├── vault/                 # vault broker config + capability handles; recovery-only mutation
├── runtime/               # action envelopes, sandboxes, scratch
└── recovery/              # recovery-safe assets reachable post-boot
```

Closed enum:

```proto
enum SystemReservedName {
  SYSTEM_RESERVED_NAME_UNSPECIFIED = 0;
  SYS_APPS = 1;
  SYS_AGENTS = 2;
  SYS_POLICY = 3;
  SYS_CAPABILITIES = 4;
  SYS_EVIDENCE = 5;
  SYS_VAULT = 6;
  SYS_RUNTIME = 7;
  SYS_RECOVERY = 8;
}
```

### 4.1 Per-system reserved subdirectories — mutation classes

| Reserved name  | Mutation class          | Read class                         |
| -------------- | ----------------------- | ---------------------------------- |
| `apps`         | `RECOVERY_OR_SYS_ADMIN` | `PUBLIC_MANIFEST_RESTRICTED_STATE` |
| `agents`       | `RECOVERY_OR_SYS_ADMIN` | `PUBLIC_MANIFEST_RESTRICTED_STATE` |
| `policy`       | `RECOVERY_ONLY`         | `SYSTEM_AUDIT_READ`                |
| `capabilities` | `RECOVERY_ONLY`         | `PUBLIC`                           |
| `evidence`     | `APPEND_ONLY_BY_KERNEL` | `PRIVACY_CEILING_FILTERED`         |
| `vault`        | `RECOVERY_ONLY`         | `BROKER_INTERMEDIATED`             |
| `runtime`      | `KERNEL_INTERNAL`       | `OWNER_OR_AUDIT`                   |
| `recovery`     | `RECOVERY_ONLY`         | `RECOVERY_SUBJECT_ONLY`            |

`RECOVERY_OR_SYS_ADMIN` permits a subject holding the `system_admin` capability bound to a human subject only — never to AI subjects (this is enforced by S2.3 §17 AI self-approval prevention extended to system-scope mutations).

## 5. Per-group reserved subdirectories

```text
/aios/groups/<group_id>/
├── apps/                  # apps installed for the group (L6 packages)
├── agents/                # AI agents owned by the group (L5 instances)
├── users/                 # one directory per user; user_id matches §7 regex
├── shared/                # group-scoped collaboration space
├── projects/              # task-scoped projects
├── datasets/              # PrivacyClass-tagged data objects
├── inbox/                 # virtual view over pending actions + agent messages
├── policy/                # group policy delta over system bundle
├── evidence/              # virtual view over system evidence, group-scoped
├── vault/                 # group's capability handles (never raw secrets)
└── audit/                 # virtual view over all group-touching actions
```

Closed enum:

```proto
enum GroupReservedName {
  GROUP_RESERVED_NAME_UNSPECIFIED = 0;
  GRP_APPS = 1;
  GRP_AGENTS = 2;
  GRP_USERS = 3;
  GRP_SHARED = 4;
  GRP_PROJECTS = 5;
  GRP_DATASETS = 6;
  GRP_INBOX = 7;        // virtual
  GRP_POLICY = 8;
  GRP_EVIDENCE = 9;     // virtual
  GRP_VAULT = 10;
  GRP_AUDIT = 11;       // virtual
}
```

### 5.1 Virtual-view reserved names

`GRP_INBOX`, `GRP_EVIDENCE`, `GRP_AUDIT` are virtual: they do not back a physical directory in AIOS-FS. They resolve to S2.1 named query views. Materialization is `ON_DEMAND` (S2.1 §4). Mutation through these paths is rejected with `VirtualPathNotWritable`.

### 5.2 Per-group mutation classes

| Reserved name | Mutation class                          | Read class                         |
| ------------- | --------------------------------------- | ---------------------------------- |
| `apps`        | `GROUP_ADMIN_OR_APP_OWNER`              | `GROUP_MEMBER`                     |
| `agents`      | `GROUP_ADMIN_OR_AGENT_OWNER`            | `GROUP_MEMBER`                     |
| `users`       | `GROUP_ADMIN`                           | `GROUP_MEMBER`                     |
| `shared`      | `GROUP_MEMBER`                          | `GROUP_MEMBER`                     |
| `projects`    | `PROJECT_MEMBER`                        | `GROUP_MEMBER` + `PRIVACY_CEILING` |
| `datasets`    | `DATASET_OWNER` + `PRIVACY_CLASS_CHECK` | `PRIVACY_CEILING` per object       |
| `inbox`       | `READ_ONLY` (virtual)                   | `GROUP_MEMBER` + `PRIVACY_CEILING` |
| `policy`      | `GROUP_ADMIN`                           | `GROUP_ADMIN`                      |
| `evidence`    | `READ_ONLY` (virtual)                   | `GROUP_MEMBER` + `PRIVACY_CEILING` |
| `vault`       | `GROUP_ADMIN` + `BROKER_INTERMEDIATED`  | `BROKER_INTERMEDIATED`             |
| `audit`       | `READ_ONLY` (virtual)                   | `GROUP_AUDIT` capability           |

The exact role definitions (`group_admin`, `app_owner`, `agent_owner`, `project_member`, etc.) are deferred to L4 identity model. This spec only fixes the access classes, not the membership semantics.

## 6. Per-user reserved subdirectories

```text
/aios/groups/<group_id>/users/<user_id>/
├── home/                  # personal documents, like classical $HOME
├── agents/                # this user's personal agents
├── prefs/                 # UI/renderer settings (KDE, Web, CLI, Voice)
├── desktop/               # KDE Plasma session state (L7)
├── inbox/                 # virtual: only this user's approvals & messages
├── outbox/                # virtual: actions submitted by this user
├── drafts/                # work-in-progress documents/queries/workflows
└── trust/                 # delegations, recovery contacts, known devices
```

Closed enum:

```proto
enum UserReservedName {
  USER_RESERVED_NAME_UNSPECIFIED = 0;
  USR_HOME = 1;
  USR_AGENTS = 2;
  USR_PREFS = 3;
  USR_DESKTOP = 4;
  USR_INBOX = 5;        // virtual
  USR_OUTBOX = 6;       // virtual
  USR_DRAFTS = 7;
  USR_TRUST = 8;
}
```

`USR_INBOX` and `USR_OUTBOX` are virtual views (same discipline as group-level virtuals). Mutation rejected with `VirtualPathNotWritable`.

### 6.1 Per-user mutation classes

All mutation under `/aios/groups/<g>/users/<u>/...` is `OWNER_ONLY` by default — only the user identified by `<u>` can mutate. Exception: `trust/` mutations may require co-signature by another trusted subject (mechanics deferred to L4 approval mechanics).

Read class is `OWNER_OR_GROUP_AUDIT` for most subdirectories; `prefs/` and `desktop/` are `OWNER_ONLY` (UI state is private even from group audit).

## 7. Identity formats and reserved IDs

### 7.1 Closed regex for IDs

```text
group_id   ::= [a-z][a-z0-9_-]{0,62}
user_id    ::= [a-z][a-z0-9_-]{0,62}
agent_id   ::= [a-z][a-z0-9_-]{0,62}
project_id ::= [a-z][a-z0-9_-]{0,62}
app_id     ::= [a-z][a-z0-9_-]{0,62}(\.[a-z][a-z0-9_-]{0,62}){0,4}    // reverse-DNS allowed
```

Properties:

- ASCII only; lowercase only; first char must be a letter; max 63 chars per segment.
- No dots, slashes, or whitespace except in `app_id` where dots are allowed for reverse-DNS namespacing (max 5 segments, each ≤ 63 chars).
- Filesystem-friendly across ext4, btrfs, ZFS, XFS, FAT (which AIOS-FS does not target but FUSE projection might pass through).

### 7.2 Reserved ID prefixes

- IDs starting with `_` are reserved for AIOS internal use.
- The `_system` group id is **not** addressable under `/aios/groups/`; it is materialized as the top-level `/aios/system/` reserved name (§3).
- `_recovery`, `_aios`, `_root` are reserved at every scope (group, user, agent, project, app).
- Length-1 IDs are forbidden.

### 7.3 Uniqueness

- `group_id` is unique within `/aios/groups/`.
- `user_id` is unique within `/aios/groups/<g>/users/`.
- `agent_id` is unique within `/aios/groups/<g>/agents/` AND within `/aios/groups/<g>/users/<u>/agents/`.
- `app_id` is unique within `/aios/groups/<g>/apps/` AND within `/aios/system/apps/`. An app cannot exist at both scopes simultaneously (S5.4 §5; cross-spec touch-up to L6).

## 8. Path resolution algorithm

### 8.1 Input and output

- **Input:** absolute path string; must start with `/aios/`.
- **Output:** typed `NamespacePath` message OR `ResolutionError` with closed code.

```proto
message NamespacePath {
  string raw_path = 1;                       // input string
  ScopeKind scope = 2;
  string group_id = 3;                       // empty for SYSTEM scope
  string user_id = 4;                        // empty for SYSTEM and GROUP scopes
  oneof reserved {
    SystemReservedName system_reserved = 10;
    GroupReservedName group_reserved = 11;
    UserReservedName user_reserved = 12;
  }
  repeated string subpath = 20;              // segments after reserved name
  bool is_virtual_view = 21;                 // true for inbox/evidence/audit/outbox
  string namespace_catalog_version = 22;     // nscat_<hex>; stamped at resolve
}

enum ScopeKind {
  SCOPE_KIND_UNSPECIFIED = 0;
  SYSTEM = 1;
  GROUP = 2;
  USER = 3;
}

message ResolutionError {
  ResolutionErrorCode code = 1;
  string message = 2;
  uint32 segment_index = 3;                  // where the parse failed
}

enum ResolutionErrorCode {
  RESOLUTION_ERROR_CODE_UNSPECIFIED = 0;
  NOT_UNDER_AIOS = 1;
  INVALID_TOP_LEVEL = 2;
  INVALID_SYSTEM_RESERVED = 3;
  INVALID_GROUP_RESERVED = 4;
  INVALID_USER_RESERVED = 5;
  INVALID_GROUP_ID = 6;
  INVALID_USER_ID = 7;
  RESERVED_ID_USED = 8;
  PATH_TRAVERSAL = 9;
  SEGMENT_COUNT_EXCEEDED = 10;
  PATH_LENGTH_EXCEEDED = 11;
  EMPTY_SEGMENT = 12;
  CATALOG_VERSION_MISMATCH = 13;
}
```

### 8.2 Algorithm (deterministic)

```text
INPUT: path, namespace_catalog
1.  if not path.startswith("/aios/"):
        return NOT_UNDER_AIOS
2.  segments := path[len("/aios/"):].split("/")
    # drop trailing empty segment from a single trailing "/"
    if segments[-1] == "":
        segments := segments[:-1]
3.  reject if any segment is "" → EMPTY_SEGMENT
    reject if any segment is "." or ".." → PATH_TRAVERSAL
    reject if len(segments) > 32 → SEGMENT_COUNT_EXCEEDED
    reject if len(path) > 4096 → PATH_LENGTH_EXCEEDED
4.  match segments[0] against TopLevelReservedName:
        "system" → continue at step 5
        "groups" → continue at step 6
        else    → INVALID_TOP_LEVEL
5.  # SYSTEM scope
    if len(segments) == 1:
        return NamespacePath{ scope=SYSTEM, no reserved, subpath=[] }
    match segments[1] against SystemReservedName:
        match → reserved := SYS_*; subpath := segments[2:]
        no match → INVALID_SYSTEM_RESERVED
    return NamespacePath{ scope=SYSTEM, system_reserved, subpath, is_virtual_view=false }
6.  # GROUP-or-USER scope
    if len(segments) == 1:
        return NamespacePath{ scope=GROUP, all empty }   # "/aios/groups" listing
    if len(segments) == 2:
        # "/aios/groups/<gid>"
        validate segments[1] as group_id (regex + reserved-id check) → on fail: INVALID_GROUP_ID or RESERVED_ID_USED
        return NamespacePath{ scope=GROUP, group_id=segments[1] }
    # /aios/groups/<gid>/<reserved>...
    validate segments[1] as group_id
    match segments[2] against GroupReservedName:
        match → reserved := GRP_*
        no match → INVALID_GROUP_RESERVED
    if reserved == GRP_USERS:
        if len(segments) >= 4:
            validate segments[3] as user_id
            if len(segments) >= 5:
                match segments[4] against UserReservedName:
                    match → user-scope path
                    no match → INVALID_USER_RESERVED
                subpath := segments[5:]
                is_virtual_view := user_reserved in {USR_INBOX, USR_OUTBOX}
                return NamespacePath{ scope=USER, group_id, user_id, user_reserved, subpath, is_virtual_view }
            return NamespacePath{ scope=USER, group_id, user_id }
        return NamespacePath{ scope=GROUP, group_id, group_reserved=GRP_USERS }
    is_virtual_view := group_reserved in {GRP_INBOX, GRP_EVIDENCE, GRP_AUDIT}
    subpath := segments[3:]
    return NamespacePath{ scope=GROUP, group_id, group_reserved, subpath, is_virtual_view }
```

### 8.3 Catalog version stamping

The resolver stamps the resolved `NamespacePath` with the active `namespace_catalog_version`. Callers comparing two resolved paths across catalog versions get `CATALOG_VERSION_MISMATCH` from any equality check; this prevents stale routing decisions surviving a catalog upgrade.

## 9. Cross-group access (default forbidden)

### 9.1 Constitutional invariant

Hard-coded in S2.3 (per the cross-spec touch-up in §12): **any action whose target resolves to `/aios/groups/<B>/...` from a subject whose primary_group_id is `A ≠ B` is denied** with `CrossGroupAccessForbidden`. This is a constitutional hard-deny that policy bundles cannot loosen except through an explicit `federation_policy` block (deferred to a future spec; not part of Rev.2).

### 9.2 Exceptions

The following exceptions are constitutional and hard-coded:

1. **System audit subjects.** A subject in the `_system` scope holding `system_audit_read` capability AND operating in recovery mode AND with a human approver MAY read across groups for audit purposes. This is the only cross-group read path in Rev.2.
2. **Self-membership.** A subject with memberships in groups A and B, currently authenticated under primary_group_id = A, can switch primary group to B (re-auth) and then access B as a normal member. The two contexts never overlap; the subject is logically two principals.

There is no Rev.2 exception that allows agents in group A to read group B silently.

### 9.3 What "forbidden" means at filesystem layer

The AIOS-FS query language (S2.1) refuses to enumerate cross-group paths for a subject. List operations that include cross-group entries silently exclude them and return `PARTIAL` with a count of suppressed entries (akin to the privacy ceiling discipline in S3.1 §10).

## 10. Admin/recovery boundary

### 10.1 Recovery-only mutations

The following paths require `is_recovery_mode = true` on the subject AND a human approver AND a `RECOVERY_EVENT` evidence record (FOREVER retention):

- Anything under `/aios/system/policy/`
- Anything under `/aios/system/capabilities/`
- Anything under `/aios/system/vault/`
- Anything under `/aios/system/recovery/`

Mutations to these paths from non-recovery subjects are rejected with `RecoveryModeRequired`.

### 10.2 System-admin mutations (not recovery-only)

`/aios/system/apps/` and `/aios/system/agents/` permit mutation by a subject holding the `system_admin` capability, **bound to a human subject only**. AI subjects holding `system_admin` are constitutionally rejected (S2.3 §17 extension).

System-admin mutations still emit `SYSTEM_ADMIN_OPERATION` evidence (added to S3.1's RecordType vocabulary; see §12.6).

### 10.3 No "settings UI for policy"

This spec does **not** define a normal-mode UI for editing policy bundles. Operator workflows in normal mode are read-only and observation-grade. Policy edits go through:

1. Boot into recovery mode (L1 recovery path).
2. Edit signed bundle.
3. Re-sign and place in `/aios/system/policy/`.
4. Reboot to normal mode; engine reloads.

This preserves the recovery boundary as the only mutation surface for the constitutional layer.

## 11. Inbox semantics

### 11.1 Two inboxes per user, plus group inbox

A user has two inboxes:

- **Personal inbox** at `/aios/groups/<g>/users/<u>/inbox/` — items addressed to user `<u>` specifically: actions awaiting their approval, agent messages directed at them, alerts.
- **Personal outbox** at `/aios/groups/<g>/users/<u>/outbox/` — actions submitted by user `<u>`, with their lifecycle states.

A group has one inbox:

- **Group inbox** at `/aios/groups/<g>/inbox/` — all pending actions in the group's scope, filtered by the caller's privacy ceiling.

### 11.2 All inboxes are virtual views

Inbox paths are `is_virtual_view = true`. They resolve to S2.1 named query views over action envelopes (S0.1) in pending states. Mutation through inbox paths is rejected with `VirtualPathNotWritable`. To approve/reject an action, callers issue a typed action against the action's own envelope id, not against an inbox entry.

### 11.3 Materialization

`ON_DEMAND` per S2.1 §4. The view is recomputed at read time. Cardinality bound: max 10 000 visible items per inbox; pagination required for queries that would yield more (cursor-based per S2.1 §6).

### 11.4 Filtering rules

- Personal inbox: items where `target.user_id == <u>` OR `addressed_user_ids` contains `<u>` AND the item is in a pending state.
- Group inbox: all pending items where `target.group_id == <g>`, then filtered by the caller's privacy ceiling.
- Personal outbox: items where the action envelope's `submitter.user_id == <u>`.

## 12. Cross-spec touch-up requirements

Adopting this namespace requires deltas in eight existing contracts. Each touch-up is enumerated explicitly so a follow-up cycle can apply them. The deltas are **specifications**, not implementations.

### 12.1 S0.1 (Action Envelope)

- `request.target.path` field validated against namespace resolver. Invalid → `InvalidTargetPath` error code.
- New optional fields: `request.target.scope` (`ScopeKind`), `request.target.group_id`, `request.target.user_id`. These are derived from the resolved path; included in the envelope for cheap policy evaluation without re-resolution.

### 12.2 S1.3 (Object Model)

- Object location is the tuple `(scope, group_id?, user_id?, reserved_name, subpath_within_reserved_name)`. The path-string projection is the canonical S0.1 `target.path`.
- AIOS-FS pointer move (CAS) cannot redirect a pointer across scopes (system → group, or group A → group B). Cross-scope moves are rejected with `ConflictDetected`.
- New `ScopeBinding` field on every object: `scope_kind`, `group_id?`, `user_id?`. Set at object creation; immutable thereafter.

### 12.3 S2.1 (Query/View Language)

- New closed query field: `target.scope`. New optional fields: `target.group_id`, `target.user_id`, `target.reserved_name`.
- Inbox and outbox are formal named views with closed parameters: `inbox(scope, id)`, `outbox(user_id)`. The view definition references the action envelope schema.
- Cross-group queries are silently filtered (callers see `PARTIAL` results with a suppression count).

### 12.4 S2.3 (Policy Kernel)

- New closed condition fields in the EBNF grammar (§4): `target.scope`, `target.group_id`, `target.user_id`, `target.reserved_name`, `subject.primary_group_id`.
- New constitutional hard-deny: `CrossGroupAccessForbidden` (subject.primary_group_id ≠ target.group_id ⇒ DENY) — cannot be loosened by policy bundle.
- New constitutional hard-deny: `RecoveryRequiredForSystemMutation` (target.scope = SYSTEM, target.system_reserved ∈ {SYS_POLICY, SYS_CAPABILITIES, SYS_VAULT, SYS_RECOVERY}, action mutates ⇒ DENY unless subject.is_recovery_mode = true with human approver) — cannot be loosened by policy bundle.
- New constitutional hard-deny: `AISystemAdminBlocked` (subject.is_ai = true, target.scope = SYSTEM, target.system_reserved ∈ {SYS_APPS, SYS_AGENTS}, action mutates ⇒ DENY) — extension of S2.3 §17.

### 12.5 S2.4 (Verification Grammar)

- New primitive: `aiosfs_path_in_namespace(path, expected_scope, expected_group_id?, expected_user_id?)` — verifies that a path resolves to the expected scope/group/user. Read-only; idempotent. Closed under §2 vocabulary discipline.
- New property: `NAMESPACE_NO_CROSS_GROUP_POINTERS` — an AIOS-FS property check that confirms no pointer in group A references chunks owned by group B.

### 12.6 S3.1 (Evidence Log)

- Each evidence record carries an optional `(scope, group_id, user_id)` triple. Empty scope = system-level event; group_id = empty for system scope; user_id = empty for system and group scopes.
- Query privacy ceiling extends to namespace scope: a subject in group A cannot see records in group B unless they hold `system_audit_read` + recovery mode.
- New record type: `SYSTEM_ADMIN_OPERATION` (STANDARD_24M retention) — emitted by the kernel for any system-admin mutation.
- New record type: `CROSS_GROUP_ACCESS_DENIED` (STANDARD_24M retention) — emitted whenever a hard-deny fires for cross-group access; carries source group_id and target group_id.

### 12.7 S3.2 (Sandbox Composition)

- Group's policy delta provides additional sandbox floor constraints — added as a sixth source between policy_required and runtime_safety_floor in the §5.1 ordering: `[adapter_default, app_manifest, user_request, policy_required, group_floor, runtime_safety_floor]`.
- Composition source enum gains `GROUP_FLOOR`. Strictness order is preserved.
- Wine prefix paths, Waydroid container paths, VM fallback storage shares MUST live under the group's namespace (e.g., `/aios/groups/<g>/agents/<id>/runtime/wine/`), never under flat global paths.
- Apply-time check: `target.path` of the action being sandboxed must resolve to the same group as the agent's `group_owner`.

### 12.8 L4 Identity (unrefined; constraints captured)

When L4 identity is refined, it MUST honor:

- Subject canonical id format: `<group_id>:<user_id>` for human users, `<group_id>:<agent_id>` for AI agents in a group, `<group_id>:<user_id>:<agent_id>` for personal agents under a user, `_system:<service_name>` for system services.
- Group is a first-class identity unit, not an attribute.
- A subject can have memberships in multiple groups; exactly one membership is the `primary_group_id` for any given action context (set during authentication, not switchable mid-action).
- Recovery-mode subjects always belong to `_system` scope; their canonical id is `_system:recovery:<operator_id>`.

### 12.9 L5 Cognitive Core (unrefined; constraints captured)

When L5 is refined, it MUST honor:

- Agent objects live at `/aios/groups/<g>/agents/<id>/` (group agent) or `/aios/groups/<g>/users/<u>/agents/<id>/` (personal agent within a user's scope).
- Each agent object has explicit `group_owner` (required) and `user_owner` (nullable; non-null for personal agents).
- An agent's capability bindings cannot reference paths outside its own scope (`group_owner`, optionally extended by `user_owner`).
- System agents (translator, planner, recovery diagnostics) live at `/aios/system/agents/` and are constitutional — not user-installable.

### 12.10 L6 Apps (unrefined; constraints captured)

When L6 application/package model is refined, it MUST honor:

- App install scope is exactly one of: `/aios/system/apps/<app_id>/` OR `/aios/groups/<g>/apps/<app_id>/`. Never both simultaneously for the same `app_id`.
- App manifest declares `installable_scope`: `SYSTEM_ONLY` (e.g., evidence viewer), `GROUP_ONLY` (most user apps), `EITHER` (rare; needs explicit dual-scope declaration).
- An app's runtime working directory MUST be under its install scope (no escaping to `/aios/system/runtime/` from a group app).

## 13. Determinism contract

```text
GIVEN
  raw_path                 = P
  namespace_catalog_version = nscat_C

THEN
  resolve(P, C) ≡ resolve(P, C)
```

The namespace catalog version is `nscat_<hex_lower(BLAKE3(jcs(catalog_descriptor)))[:32]>` aligned with S0.1 §8.5. The catalog descriptor is the JCS-canonical encoding of all closed enums in this spec (`TopLevelReservedName`, `SystemReservedName`, `GroupReservedName`, `UserReservedName`) plus the reserved-id list.

A catalog upgrade produces a new version. Resolved paths are stamped with the version that resolved them; equality across versions is `false` even if the strings match (forces re-resolution after an upgrade).

## 14. Performance contract

| Operation                                        | p50      | p95      | p99      | Hard timeout |
| ------------------------------------------------ | -------- | -------- | -------- | ------------ |
| `ResolvePath` (string parse only)                | < 20 µs  | < 50 µs  | < 100 µs | 1 ms         |
| `ResolvePath` (with reserved-id existence check) | < 100 µs | < 500 µs | < 1 ms   | 5 ms         |
| `ListReservedNames` (cached)                     | < 5 µs   | < 20 µs  | < 50 µs  | 100 µs       |
| `GetNamespaceInfo`                               | < 10 µs  | < 50 µs  | < 100 µs | 1 ms         |

The resolver is stateless except for the catalog cache (read-mostly; reload on signal). Cache invalidation: catalog version change.

Failure modes — all fail closed:

- `ResolverInternal` → caller receives `ResolutionError` with empty code; engine emits alert.
- `CatalogUnavailable` → resolver fails closed; new requests rejected until catalog reloads.
- `CatalogSignatureFailure` → resolver enters degraded mode (resolves only the closed enum subset; refuses any call requiring reserved-id existence check).

## 15. Adversarial robustness

### 15.1 Path traversal

`..`, `.`, `\\` (Windows-style separator), URL-encoded variants (`%2e%2e`), and double-slashes are rejected at parse time. Resolver does NOT normalize; it rejects.

### 15.2 Unicode tricks

Path segments must be ASCII. Non-ASCII bytes in any segment → `INVALID_GROUP_ID`/`INVALID_USER_ID` (or the corresponding code). Homograph attacks (e.g., Cyrillic 'а' instead of Latin 'a' in a group id) are blocked at the regex level.

### 15.3 Symlink crossing scope boundaries

AIOS-FS pointers (S1.3) cannot reference chunks owned by a different group. Cross-scope pointer moves fail at S1.3's CAS layer. The namespace resolver does not follow symlinks itself; it operates on the path string.

### 15.4 Reserved-name attack

An app cannot create `/aios/groups/<g>/agents/_recovery/` because `_*` IDs are rejected by the agent_id regex (§7.2). An app cannot install at `/aios/groups/_system/apps/<x>/` because `_system` is reserved at the top level, not addressable as a group_id.

### 15.5 Group-id collision

Group registration is content-addressed at the AIOS-FS layer (S1.3). Namespace catalog enforces uniqueness within `/aios/groups/`. Two attempts to register the same group_id from concurrent sessions resolve via S1.3's optimistic concurrency: one wins, the other gets `ConflictDetected`.

### 15.6 Algorithmic attacks

- Maximum depth 32 segments prevents stack-blowup attacks.
- Maximum path length 4096 bytes prevents memory-exhaustion attacks.
- Resolver is O(segments) — no backtracking, no regex catastrophic backtracking (the regex is anchored and bounded).

### 15.7 Catalog signature attack

Catalog descriptor is signed by the AIOS root (Ed25519). Resolver verifies signature on load; failure → degraded mode. A forged catalog cannot extend reserved-name vocabularies or reduce hard-deny rules.

## 16. Cross-spec dependencies (table)

| Spec           | Dependency direction | What this spec adds                                                                                                               |
| -------------- | -------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| S0.1           | Bidirectional        | `target.path` validated by resolver; `target.scope`/`group_id`/`user_id` populated from resolution                                |
| S1.3           | Producer             | Namespace catalog informs object's `ScopeBinding`; pointer moves cannot cross scope                                               |
| S2.1           | Producer             | `target.scope` / `target.group_id` / `target.user_id` are queryable; inbox/outbox are formal named views                          |
| S2.3           | Producer             | Three new constitutional hard-denies (cross-group, recovery-required, AI-system-admin); five new condition fields                 |
| S2.4           | Producer             | New primitive `aiosfs_path_in_namespace`; new property `NAMESPACE_NO_CROSS_GROUP_POINTERS`                                        |
| S3.1           | Producer             | Records carry optional `(scope, group_id, user_id)`; two new record types (`SYSTEM_ADMIN_OPERATION`, `CROSS_GROUP_ACCESS_DENIED`) |
| S3.2           | Producer             | New `GROUP_FLOOR` composition source; sandbox apply-time check that target group matches agent owner                              |
| L4 (unrefined) | Constraint           | Subject canonical id format; group as first-class identity                                                                        |
| L5 (unrefined) | Constraint           | Agent object location and ownership fields                                                                                        |
| L6 (unrefined) | Constraint           | App install scope rule; `installable_scope` manifest field                                                                        |

## 17. Golden fixtures

### Fixture 1 — Personal household namespace

```text
Setup:
  groups: family
  users in family: alice, bob, teen
  agents in family: family-assistant
  shared/: photos, calendars

Concrete paths:
  /aios/system/apps/evidence-viewer
  /aios/system/recovery/keyring
  /aios/groups/family/agents/family-assistant/manifest.aios
  /aios/groups/family/users/alice/home/notes.md
  /aios/groups/family/users/bob/desktop/session.json
  /aios/groups/family/shared/photos/2026-summer/
  /aios/groups/family/inbox                              # virtual view; group inbox
  /aios/groups/family/users/teen/inbox                   # virtual view; personal inbox

Expected resolutions:
  "/aios/system/apps/evidence-viewer" → {SYSTEM, SYS_APPS, ["evidence-viewer"]}
  "/aios/groups/family/users/alice/home/notes.md" →
      {USER, "family", "alice", USR_HOME, ["notes.md"]}
  "/aios/groups/family/inbox" →
      {GROUP, "family", GRP_INBOX, [], is_virtual_view=true}
```

### Fixture 2 — Solo developer with homelab

```text
Setup:
  groups: personal, homelab
  users in personal: luckyngoriko
  users in homelab: luckyngoriko             # same human, different scope
  agents in personal: coding-assistant
  apps in homelab: bg.iconys.proxguard

Concrete paths:
  /aios/groups/personal/users/luckyngoriko/agents/coding-assistant/
  /aios/groups/homelab/apps/bg.iconys.proxguard/
  /aios/groups/homelab/agents/network-monitor/

Cross-group denial:
  Subject: personal:luckyngoriko (primary_group_id=personal)
  Action target path: /aios/groups/homelab/apps/bg.iconys.proxguard/
  Expected outcome: PolicyDecision = DENY, code = CrossGroupAccessForbidden
  Evidence: CROSS_GROUP_ACCESS_DENIED record with source=personal, target=homelab
```

### Fixture 3 — Mixed work + personal with stricter group floor

```text
Setup:
  groups: personal, work-finance
  Alice has membership in both
  work-finance has policy delta requiring stricter sandbox floor (no external network for AI agents)

Concrete paths:
  /aios/groups/personal/users/alice/home/diary.md         # private, personal floor
  /aios/groups/work-finance/users/alice/home/notes.md     # work, finance floor
  /aios/groups/work-finance/datasets/q4-revenue/          # high-privacy, finance floor

Subject context switching:
  Action 1: alice authenticated with primary_group_id=personal
    → can access /aios/groups/personal/users/alice/home/diary.md
    → CANNOT access /aios/groups/work-finance/...   (hard-denied; CROSS_GROUP_ACCESS_DENIED evidence)
  Action 2: alice re-authenticated with primary_group_id=work-finance
    → can access /aios/groups/work-finance/users/alice/...
    → CANNOT access /aios/groups/personal/...        (same hard-deny, opposite direction)
```

### Fixture 4 — Path-traversal rejection

```text
Inputs and expected outcomes:
  "/aios/groups/family/../system/policy"       → PATH_TRAVERSAL
  "/aios/groups/family//inbox"                  → EMPTY_SEGMENT
  "/aios/groups/family/inbox/."                 → PATH_TRAVERSAL
  "/aios/groups/_system/apps/evil"              → RESERVED_ID_USED ("_system" cannot be a group_id)
  "/aios/groups/family/agents/_recovery/"       → INVALID_AGENT_ID at deeper validation
  "/etc/passwd"                                  → NOT_UNDER_AIOS
  "/aios/" (alone)                               → resolves to top-level listing (SYSTEM scope, no reserved)
                                                   actually: resolver returns NamespacePath with scope unset and an explicit "list-roots" hint
```

### Fixture 5 — Catalog version stamping

```text
Setup:
  catalog v1: nscat_<hash1>
  catalog v2: nscat_<hash2>      # added a new GroupReservedName, e.g., GRP_PIPELINES

Resolutions of "/aios/groups/family/inbox":
  Under v1: NamespacePath{ ..., namespace_catalog_version=nscat_<hash1> }
  Under v2: NamespacePath{ ..., namespace_catalog_version=nscat_<hash2> }
  Equality(v1_result, v2_result): false (catalog version mismatch)
  Action target check: every action must re-resolve under the catalog active at validation time.
```

### Fixture 6 — Inbox as virtual view

```text
Setup:
  Group "family" has 3 pending actions:
    A1: target.user_id=alice, awaiting alice's approval
    A2: target.user_id=bob, awaiting bob's approval
    A3: no specific user, group-level approval required

Read /aios/groups/family/inbox as alice:
  Returns: A1 (privacy ceiling: alice can see) + A3 (group-level)
  Excluded: A2 (different user; privacy ceiling filters)

Read /aios/groups/family/users/alice/inbox as alice:
  Returns: A1 only (personal scope)

Read /aios/groups/family/users/alice/inbox as bob:
  Returns: ResolutionError? No — succeeds, but returns 0 items (privacy ceiling).
  Plus: an audit record showing "bob attempted to read alice's inbox path" for traceability.
```

## 18. Telemetry contract

All metrics MUST use bounded label cardinality. **Subject id, group id, user id, action id, profile id, agent id, app id are NEVER labels.** They appear in evidence records, never in metrics.

| Metric                                     | Type      | Labels (closed set)                                                         |
| ------------------------------------------ | --------- | --------------------------------------------------------------------------- |
| `namespace_resolve_total`                  | counter   | `result` (success/error), `error_code` (closed enum)                        |
| `namespace_resolve_duration_seconds`       | histogram | `scope` (system/group/user), `cache` (hit/miss)                             |
| `namespace_cross_group_denial_total`       | counter   | `target_scope` (system/group/user)                                          |
| `namespace_recovery_required_denial_total` | counter   | `target_system_reserved` (closed enum)                                      |
| `namespace_reserved_name_collision_total`  | counter   | `reserved_kind` (top_level/system/group/user)                               |
| `namespace_catalog_version_mismatch_total` | counter   | none                                                                        |
| `namespace_catalog_load_total`             | counter   | `result` (success/signature_failure/parse_failure)                          |
| `namespace_active_groups`                  | gauge     | none                                                                        |
| `namespace_active_users_per_group`         | histogram | none (distribution; group_id is NOT a label)                                |
| `namespace_virtual_view_query_total`       | counter   | `view_kind` (group_inbox/user_inbox/user_outbox/group_evidence/group_audit) |

Cardinality budget: ≤ 50 active label tuples per metric. The closed enums together produce fewer than 80 distinct tuples across all metrics.

## 19. Acceptance criteria

- [ ] `/aios/` directly contains exactly two reserved entries: `system/` and `groups/`. Any third entry is rejected.
- [ ] Reserved subdirectory names at every scope (system, group, user) match the closed enums in §3–§6.
- [ ] Group, user, agent, project, and app IDs match the regex in §7.1; reserved ID prefixes (`_*`, `_system`, `_recovery`, `_aios`, `_root`) are rejected.
- [ ] Path resolution is deterministic given the catalog version (§13).
- [ ] Path traversal (`..`, `.`, double-slashes, non-ASCII bytes, encoded variants) is rejected at parse time.
- [ ] Cross-group access from a subject in group A to paths in group B is hard-denied unless the subject is in `_system` scope under recovery mode with `system_audit_read` capability and a human approver.
- [ ] Mutations to `/aios/system/policy/`, `/aios/system/capabilities/`, `/aios/system/vault/`, `/aios/system/recovery/` require recovery mode + human approver + `RECOVERY_EVENT` evidence.
- [ ] `/aios/system/apps/` and `/aios/system/agents/` admit human-only `system_admin` capability holders; AI subjects are rejected even with the capability.
- [ ] Inbox and outbox paths resolve as virtual views; mutation through them is rejected.
- [ ] Cross-spec touch-ups in §12 are explicitly enumerated and ready for follow-up refinement cycles.
- [ ] Performance budgets in §14 hold (p99 < 100 µs for parse-only resolution).
- [ ] All six golden fixtures in §17 produce the specified outcomes.
- [ ] Telemetry conforms to §18; subject/group/user/action/profile/agent/app ids never appear as labels.

## 20. Open deferrals

These are intentionally out of scope for S4.1 and tracked elsewhere:

- **Cross-group capability-based sharing** (Q3 option (b) — capability-based read via Vault Broker mediation). Deferred to a future spec; requires L4 vault and federation policy.
- **Federation policy** between groups (typed contract for who can share what, under what approval). Deferred.
- **Multi-tenant primary scope** (`tenants/` instead of or alongside `groups/`). Deferred; would be a versioned spec change.
- **Subgroup hierarchy** (nested groups like `finance/audit/`). Rev.2 stays flat; tag-based labels via S2.1 query views provide pseudo-hierarchy.
- **Per-group disk quota enforcement** and **per-user disk quota enforcement**. Deferred to L8 resource policy.
- **Namespace replication across machines** (cluster mode). Deferred to a future cross-machine spec.
- **Read-only views of remote namespaces** (federation projection). Deferred.
- **Catalog hot-reload mid-action** (current contract: catalog upgrade applies to new resolutions only). Deferred.
- **Profile diffing UI for namespace audits**. Deferred to L7 renderer specs.

## Appendix A — Full Proto IDL

```proto
syntax = "proto3";
package aios.namespace.v1alpha1;

import "google/protobuf/timestamp.proto";

// ============================================================================
// Service
// ============================================================================

service NamespaceResolver {
  // Resolve a path string to a typed NamespacePath. Stateless except for
  // the catalog cache.
  rpc ResolvePath(ResolvePathRequest) returns (ResolvePathResponse);

  // List the closed reserved-name vocabulary at a given scope.
  rpc ListReservedNames(ListReservedNamesRequest) returns (ListReservedNamesResponse);

  // Engine info: catalog version, schema version, active group count.
  rpc GetNamespaceInfo(GetNamespaceInfoRequest) returns (GetNamespaceInfoResponse);
}

// ============================================================================
// Core types
// ============================================================================

enum ScopeKind {
  SCOPE_KIND_UNSPECIFIED = 0;
  SYSTEM = 1;
  GROUP = 2;
  USER = 3;
}

enum TopLevelReservedName {
  TOP_LEVEL_RESERVED_NAME_UNSPECIFIED = 0;
  SYS = 1;       // /aios/system/
  GROUPS = 2;    // /aios/groups/
}

enum SystemReservedName {
  SYSTEM_RESERVED_NAME_UNSPECIFIED = 0;
  SYS_APPS = 1;
  SYS_AGENTS = 2;
  SYS_POLICY = 3;
  SYS_CAPABILITIES = 4;
  SYS_EVIDENCE = 5;
  SYS_VAULT = 6;
  SYS_RUNTIME = 7;
  SYS_RECOVERY = 8;
}

enum GroupReservedName {
  GROUP_RESERVED_NAME_UNSPECIFIED = 0;
  GRP_APPS = 1;
  GRP_AGENTS = 2;
  GRP_USERS = 3;
  GRP_SHARED = 4;
  GRP_PROJECTS = 5;
  GRP_DATASETS = 6;
  GRP_INBOX = 7;        // virtual
  GRP_POLICY = 8;
  GRP_EVIDENCE = 9;     // virtual
  GRP_VAULT = 10;
  GRP_AUDIT = 11;       // virtual
}

enum UserReservedName {
  USER_RESERVED_NAME_UNSPECIFIED = 0;
  USR_HOME = 1;
  USR_AGENTS = 2;
  USR_PREFS = 3;
  USR_DESKTOP = 4;
  USR_INBOX = 5;        // virtual
  USR_OUTBOX = 6;       // virtual
  USR_DRAFTS = 7;
  USR_TRUST = 8;
}

message NamespacePath {
  string raw_path = 1;
  ScopeKind scope = 2;
  string group_id = 3;
  string user_id = 4;
  oneof reserved {
    SystemReservedName system_reserved = 10;
    GroupReservedName group_reserved = 11;
    UserReservedName user_reserved = 12;
  }
  repeated string subpath = 20;
  bool is_virtual_view = 21;
  string namespace_catalog_version = 22;
  google.protobuf.Timestamp resolved_at = 23;
}

enum ResolutionErrorCode {
  RESOLUTION_ERROR_CODE_UNSPECIFIED = 0;
  NOT_UNDER_AIOS = 1;
  INVALID_TOP_LEVEL = 2;
  INVALID_SYSTEM_RESERVED = 3;
  INVALID_GROUP_RESERVED = 4;
  INVALID_USER_RESERVED = 5;
  INVALID_GROUP_ID = 6;
  INVALID_USER_ID = 7;
  INVALID_AGENT_ID = 8;
  INVALID_PROJECT_ID = 9;
  INVALID_APP_ID = 10;
  RESERVED_ID_USED = 11;
  PATH_TRAVERSAL = 12;
  SEGMENT_COUNT_EXCEEDED = 13;
  PATH_LENGTH_EXCEEDED = 14;
  EMPTY_SEGMENT = 15;
  CATALOG_VERSION_MISMATCH = 16;
  RESOLVER_INTERNAL = 17;
  CATALOG_UNAVAILABLE = 18;
  CATALOG_SIGNATURE_FAILURE = 19;
}

message ResolutionError {
  ResolutionErrorCode code = 1;
  string message = 2;
  uint32 segment_index = 3;
}

// ============================================================================
// RPC request/response
// ============================================================================

message ResolvePathRequest {
  string path = 1;
  string expected_catalog_version = 2;     // optional; if set and != active, returns CATALOG_VERSION_MISMATCH
  bool require_existence_check = 3;         // if true, also verify group_id/user_id exist
}

message ResolvePathResponse {
  oneof result {
    NamespacePath path = 1;
    ResolutionError error = 2;
  }
}

message ListReservedNamesRequest {
  ScopeKind scope = 1;
}

message ListReservedNamesResponse {
  repeated string names = 1;            // string form of the closed enum
  string catalog_version = 2;
}

message GetNamespaceInfoRequest {}

message GetNamespaceInfoResponse {
  string catalog_version = 1;
  string schema_version = 2;            // "aios.namespace.v1alpha1"
  uint64 active_group_count = 3;
  google.protobuf.Timestamp catalog_loaded_at = 4;
  bool degraded_mode = 5;               // true if catalog signature failed
}

// ============================================================================
// Catalog descriptor (signed; loaded at startup)
// ============================================================================

message NamespaceCatalogDescriptor {
  string version = 1;                    // nscat_<hex_lower(BLAKE3(jcs(this)))[:32]>
  google.protobuf.Timestamp issued_at = 2;
  string issuer = 3;                     // "aios-root"
  bytes ed25519_signature = 4;           // over canonical encoding of fields 1-3, 10-12

  // Frozen vocabularies
  repeated string top_level_reserved_names = 10;
  repeated string system_reserved_names = 11;
  repeated string group_reserved_names = 12;
  repeated string user_reserved_names = 13;
  repeated string reserved_id_prefixes = 14;     // ["_", "_system", "_recovery", "_aios", "_root"]

  // Identity regex (informative; resolver hard-codes equivalent)
  string id_regex = 20;                   // "[a-z][a-z0-9_-]{0,62}"
  string app_id_regex = 21;               // reverse-DNS form
  uint32 max_path_length = 22;            // 4096
  uint32 max_segment_count = 23;          // 32
  uint32 max_segment_length = 24;         // 63
}
```

## Wave 8 cross-spec touch-up (Tier 1 + Tier 2 namespace path additions)

Applied 2026-05-11. This section consolidates namespace path additions queued by Tier 1 and Tier 2 source specs since S4.1 was promoted (2026-05-09). Each addition is a **closed-enum extension**, not a redefinition of an existing scope; the catalog version (§13) bumps once on adoption (`nscat_<new_hex>`). Until adoption, source specs stamp their references with the existing catalog and the resolver returns `INVALID_SYSTEM_RESERVED` / `INVALID_GROUP_RESERVED` / `INVALID_USER_RESERVED` for the new names — preserving fail-closed discipline.

Sources: S9.2 (first_boot_flow), S0.3 (mvp_golden_path), S15.1 (unit_manifest), S15.2 (state_transitions), S13.2 (model_router), S12.2 (package_model — Tier 2 USR_APPS), S12.3 (compatibility_runtime), S12.4 (compatibility_knowledge), S8.3 (hardware_graph), S8.4 (dns_vpn_management), S8.5 (firmware_trust), S14.1 (failure_handling), S11.2 (marketplace).

Sources that contributed no namespace path addition (W8.4): S6.3 (evidence_receipt_schema), S15.3 (adapter_model — sandbox profile examples only), S13.1 (cognitive_core_model — uses existing GRP_AGENTS / USR_AGENTS / GRP_SHARED structure), S7.6 (cli_renderer — uses existing SYS_RECOVERY for operator keys), S14.2 (telemetry_pipeline), S11.3 (external_integrations — bridge runtime nests under existing SYS_RUNTIME, no new reserved name).

### W8.1 New scope path enum entries

#### SystemReservedName extensions (16 new values; current closed enum extends from 8 → 24)

| Enum               | New value | Owner subject                           | Path                         | Recovery treatment                                                                                                                      | Source spec                           |
| ------------------ | --------- | --------------------------------------- | ---------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- |
| `SYS_BOOT`         | 9         | `_system:service:boot-coordinator`      | `/aios/system/boot/`         | recovery-safe (rw via `RecoveryMutableScope.L1_BOOT_PARAMETERS`)                                                                        | S9.2 §3.2; S9.1 §3.6                  |
| `SYS_FIRSTBOOT`    | 10        | `_system:service:firstboot-coordinator` | `/aios/system/firstboot/`    | recovery-safe (rw for `recovery.firstboot.reset` only, INV-012)                                                                         | S9.2 §3.2, §6.4                       |
| `SYS_GOVERNANCE`   | 11        | `_system:service:governance`            | `/aios/system/governance/`   | recovery-safe (rw via `RecoveryMutableScope.INVARIANT_BUNDLE`)                                                                          | S9.2 §3.2; S9.1 §3.6 row 4            |
| `SYS_IDENTITY`     | 12        | `_system:service:identity`              | `/aios/system/identity/`     | recovery-safe (rw via `RecoveryMutableScope.IDENTITY_BUNDLE`)                                                                           | S0.3 §4.3; S9.2 §3.2; S9.1 §3.6 row 5 |
| `SYS_KERNEL`       | 13        | `_system:service:kernel-manager`        | `/aios/system/kernel/`       | recovery-safe (rw via `RecoveryMutableScope.DEDICATED_KERNEL_PROMOTION`)                                                                | S9.2 §3.2; S9.1 §3.6 row 7            |
| `SYS_HARDWARE`     | 14        | `_system:service:hardware-manager`      | `/aios/system/hardware/`     | recovery-quarantined (snapshots preserved; rebuilt from physical scan at every boot)                                                    | S8.3 §I6, §6.2, §15                   |
| `SYS_DRIVERS`      | 15        | `_system:service:driver-registry`       | `/aios/system/drivers/`      | recovery-quarantined (signed driver catalog; ro in recovery for inspection)                                                             | S8.3 §5.3                             |
| `SYS_FIRMWARE`     | 16        | `_system:service:firmware-trust`        | `/aios/system/firmware/`     | recovery-safe (rw for monotonicity counter reset via `RecoveryMutableScope.FIRMWARE_VERSION_COUNTER`, queued in S9.1 follow-up)         | S8.5 §6                               |
| `SYS_NETWORK`      | 17        | `_system:service:network`               | `/aios/system/network/`      | recovery-quarantined (resolver allowlist preserved; rotation via recovery-mode action; recovery boot forces `DEGRADED_HOSTS_FILE_ONLY`) | S8.4 §I2, §3.1, §6.1                  |
| `SYS_SGR`          | 18        | `_system:service:sgr`                   | `/aios/system/sgr/`          | recovery-quarantined (desired graph + promotion state preserved; no L3 in recovery, so not loaded)                                      | S15.2 §4, §6.2                        |
| `SYS_UNITS`        | 19        | `_system:service:sgr`                   | `/aios/system/units/`        | recovery-quarantined (unit manifests preserved; admission requires L3 which is not running)                                             | S15.1 §11                             |
| `SYS_RUNBOOKS`     | 20        | `_system:service:runbook-registry`      | `/aios/system/runbooks/`     | recovery-safe (ro mountable; recovery operator reads runbooks; no recovery-time mutation)                                               | S14.1 §6.4                            |
| `SYS_THEMES`       | 21        | `_system:service:theme-registry`        | `/aios/system/themes/`       | recovery-safe (ro mountable; AIOS-root-signed; constitutional icons reachable in recovery surface)                                      | S7.4 §I6, S7.5 §I6                    |
| `SYS_RENDERERS`    | 22        | `_system:service:renderer-registry`     | `/aios/system/renderers/`    | recovery-safe (ro mountable; KWin scripts signed; CLI renderer uses recovery-only surface stack)                                        | S7.4 §I8                              |
| `SYS_WEB`          | 23        | `_system:service:web-renderer`          | `/aios/system/web/`          | recovery-safe (ro mountable; recovery web surface served from `recovery.localhost` per S7.5 §10)                                        | S7.5 §10                              |
| `SYS_DISTRIBUTION` | 24        | `_system:service:distribution`          | `/aios/system/distribution/` | recovery-quarantined (staging + applications registry preserved; package install requires L3, not running)                              | S11.2 §4.2                            |

#### GroupReservedName extensions (2 new values; current closed enum extends from 11 → 13)

| Enum           | New value | Owner subject                              | Path                         | Recovery treatment                                                                  | Source spec          |
| -------------- | --------- | ------------------------------------------ | ---------------------------- | ----------------------------------------------------------------------------------- | -------------------- |
| `GRP_SERVICES` | 12        | `<group_id>:service:<service_name>`        | `/aios/groups/<g>/services/` | recovery-quarantined (groups never traversed in recovery per I3; preserved on disk) | S12.4 §6.3           |
| `GRP_SYSTEM`   | 13        | `<group_id>:role:group_admin` (human-only) | `/aios/groups/<g>/system/`   | recovery-quarantined (groups never traversed in recovery per I3; preserved on disk) | S13.2 §5.4; S11.1 §3 |

#### UserReservedName extensions (3 new values; current closed enum extends from 8 → 11)

| Enum          | New value | Owner subject          | Path                                  | Recovery treatment                                                                                                             | Source spec                                    |
| ------------- | --------- | ---------------------- | ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------- |
| `USR_APPS`    | 9         | `<group_id>:<user_id>` | `/aios/groups/<g>/users/<u>/apps/`    | recovery-quarantined (groups never traversed in recovery per I3; preserved on disk)                                            | S12.2 §13.1 (Tier 2; explicit queued addition) |
| `USR_RUNTIME` | 10        | `<group_id>:<user_id>` | `/aios/groups/<g>/users/<u>/runtime/` | recovery-quarantined (Wine prefixes / Waydroid containers / VM disks preserved; tmpfs scratch is by definition non-persistent) | S12.3 §6.2, §7, §8.3                           |
| `USR_EXPORTS` | 11        | `<group_id>:<user_id>` | `/aios/groups/<g>/users/<u>/exports/` | recovery-quarantined (operator-requested export snapshots; preserved on disk)                                                  | S12.2 §10                                      |

### W8.2 Recovery-mode behaviour for new paths

Per S9.1 §5 mount discipline, every new system-scope path must explicitly bind to one of the three recovery treatments. This W8.2 confirms each binding:

- **recovery-safe (read-only mountable in recovery; rw only via explicit `RecoveryMutableScope`).** `SYS_BOOT`, `SYS_FIRSTBOOT`, `SYS_GOVERNANCE`, `SYS_IDENTITY`, `SYS_KERNEL`, `SYS_FIRMWARE`, `SYS_RUNBOOKS`, `SYS_THEMES`, `SYS_RENDERERS`, `SYS_WEB`. The five recovery-mutable ones (boot, firstboot, governance, identity, kernel, firmware) are governed by S9.1's `RecoveryMutableScope` enum; W8.4 below records the three additions to that enum (`SYS_FIRSTBOOT_RESET`, `FIRMWARE_VERSION_COUNTER`) that the matching S9.1 touch-up must apply.
- **recovery-redacted (cleared at recovery boot).** None. No new system path requires redaction; the only paths the recovery boundary clears today are L5-related directories under `/aios/system/agents/` (already governed by S9.1 §6).
- **recovery-quarantined (preserved on disk but not mounted).** `SYS_HARDWARE`, `SYS_DRIVERS`, `SYS_NETWORK`, `SYS_SGR`, `SYS_UNITS`, `SYS_DISTRIBUTION`, plus all group-scope and user-scope additions (`GRP_SERVICES`, `GRP_SYSTEM`, `USR_APPS`, `USR_RUNTIME`, `USR_EXPORTS`). The group/user paths are quarantined by I3 (recovery never traverses `/aios/groups/...`); the system-scope quarantined paths host runtime state for services that do not run in recovery (L3 SGR, L8 hardware/driver/network managers, L10 distribution), so the data is preserved but the services that own it are not started.

No path in W8.1 has ambiguous recovery treatment; every entry has an explicit binding.

### W8.3 Reconciliation

Total namespace path additions: **21** (16 system + 2 group + 3 user).

Per-scope split:

- **SystemScope (`_system`):** 16 new reserved names (`SYS_BOOT`, `SYS_FIRSTBOOT`, `SYS_GOVERNANCE`, `SYS_IDENTITY`, `SYS_KERNEL`, `SYS_HARDWARE`, `SYS_DRIVERS`, `SYS_FIRMWARE`, `SYS_NETWORK`, `SYS_SGR`, `SYS_UNITS`, `SYS_RUNBOOKS`, `SYS_THEMES`, `SYS_RENDERERS`, `SYS_WEB`, `SYS_DISTRIBUTION`).
- **GroupScope (per `<group_id>`):** 2 new reserved names (`GRP_SERVICES`, `GRP_SYSTEM`).
- **UserScope (per `<group_id>`/`<user_id>`):** 3 new reserved names (`USR_APPS`, `USR_RUNTIME`, `USR_EXPORTS`).
- **EvidenceScope:** 0. Evidence records carry an optional `(scope, group_id, user_id)` triple per S3.1 §12.6 — no new dedicated evidence-scope path is introduced here; evidence remains under `SYS_EVIDENCE` (already enum value 5).
- **RecoveryScope:** 0 new path enum values. `SYS_RECOVERY` (existing enum value 8) continues to host operator credentials and recovery-only assets; the recovery boundary itself is governed by S9.1's `RecoveryMutableScope` / `RecoveryReadOnlyScope` enums, which are extended in the parallel S9.1 touch-up (see W8.4).

Cumulative S4.1 enum cardinality after Wave 8 adoption:

- `TopLevelReservedName`: **2** (unchanged: `SYSTEM`, `GROUPS`).
- `SystemReservedName`: **24** (was 8).
- `GroupReservedName`: **13** (was 11).
- `UserReservedName`: **11** (was 8).
- Reserved-id prefixes: **5** (unchanged: `_`, `_system`, `_recovery`, `_aios`, `_root`).

Net catalog descriptor delta: **+21 closed enum values across 3 enums**. The catalog version bumps from `nscat_<v1>` to `nscat_<v2>` on adoption. Per §13, all paths resolved under `nscat_<v1>` must be re-resolved before policy decisions consume them under `nscat_<v2>` (the resolver enforces this via `CATALOG_VERSION_MISMATCH`).

### W8.4 Cross-spec impact note

Each new path may need supporting work in adjacent contracts. The expected handoff is:

- **S3.1 RecordType assertions (handled by S3.1 Wave 8 follow-up).** Several new system paths host typed mutations that must emit dedicated evidence. Candidates already implied by source specs: `FIRSTBOOT_MARKER_WRITTEN` / `FIRST_BOOT_COMPLETE` (already FOREVER, S9.2); `KERNEL_SLOT_PROMOTED` (S9.3); `HARDWARE_GRAPH_REBUILT`, `HARDWARE_GRAPH_DRIFT_DETECTED` (S8.3 already); `FIRMWARE_APPLIED`, `FIRMWARE_VERSION_COUNTER_RESET` (S8.5); `RESOLVER_ALLOWLIST_ROTATED`, `VPN_TUNNEL_ESTABLISHED` (S8.4 already); `RUNBOOK_REGISTERED` (S14.1, queued); `DISTRIBUTION_STAGING_PROMOTED` (S11.1, queued). S3.1's Wave 8 must reconcile against this list and fold the new RecordTypes into the closed enum.
- **S2.4 verification properties (handled by S2.4 Wave 8 follow-up).** New properties needed at minimum: `NAMESPACE_NEW_PATHS_ALL_OWNED` (every new path resolves and binds to its declared owner subject); `RECOVERY_TREATMENT_BINDING_COMPLETE` (every new system path has an explicit recovery treatment in the W8.2 table); `CATALOG_VERSION_BUMPED_ON_ADOPTION` (catalog version changed exactly once when this Wave landed, not silently). The `aiosfs_path_in_namespace` primitive (added in §12.5) needs no schema change; only fixture coverage extends.
- **S2.3 Policy Kernel default-deny ACL rows (handled separately).** Each new scope-bound path needs a default-deny row keyed on `target.scope` + `target.system_reserved` / `target.group_reserved` / `target.user_reserved`. The existing constitutional hard-denies (`CrossGroupAccessForbidden`, `RecoveryRequiredForSystemMutation`, `AISystemAdminBlocked` per §12.4) already fire correctly for the new system-scope paths because they match against the closed enum **value**, not against a hard-coded list of names — so the bump from 8 to 24 system-reserved values automatically extends the hard-deny coverage. The Policy Kernel touch-up only needs to widen `RecoveryRequiredForSystemMutation`'s closed allowlist `{SYS_POLICY, SYS_CAPABILITIES, SYS_VAULT, SYS_RECOVERY}` to also include `{SYS_BOOT, SYS_FIRSTBOOT, SYS_GOVERNANCE, SYS_IDENTITY, SYS_KERNEL, SYS_FIRMWARE}` so normal-mode mutations of those paths fail-closed at the constitutional layer (matching S9.1's `RecoveryMutableScope`).
- **S9.1 RecoveryMutableScope extensions (handled by S9.1 Wave 8 follow-up).** Two new values needed: `SYS_FIRSTBOOT_RESET` (for the `recovery.firstboot.reset` action; S9.2 §6.4) and `FIRMWARE_VERSION_COUNTER` (S8.5 §6). The S9.1 enum is currently 8 values per its §3.6; this brings it to 10. The S9.1 touch-up must update its W8.2 mount-discipline table (S9.1 §5.2) accordingly.
- **L4 identity scope expansion.** `GRP_SYSTEM` mutation class is `GROUP_ADMIN` (human-only, AI-blocked at the namespace layer; S2.3 hard-deny `AISystemAdminBlocked` does not fire here because the target is group-scope, not system-scope — but the mutation class enforces the human-only rule independently). L4 identity refinement must define `group_admin` precisely; until then the constraint is captured per §12.8.

Sources that contributed no namespace path addition: see header note above. Their absence is recorded explicitly so the next refinement cycle can verify nothing slipped through unnoted.

## See also

- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S1.3 — AIOS-FS Object Model](01_object_model.md)
- [S2.1 — Query/View Language](02_query_view_language.md)
- [S2.3 — Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S2.4 — Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S3.2 — Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
- [NEXT_SESSION.md](../NEXT_SESSION.md)
