# AIOS-FS Namespace Layout (Rev.2 — DRAFT NOTES)

| Field     | Value                                                                                                                           |
| --------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `DRAFT NOTES` (brainstorm; not yet `CONTRACT`)                                                                                  |
| Phase tag | S4.1 (proposed; pending decision)                                                                                               |
| Layer     | L2 AIOS-FS                                                                                                                      |
| Captured  | 2026-05-08 conversation                                                                                                         |
| Consumers | L4 (identity/groups), L5 (agents), L6 (apps), S1.3 (object model), S2.1 (views), S2.3 (policy), S3.1 (evidence), S3.2 (sandbox) |

> This file preserves a brainstorm so we can resume it next session without losing context. **It is not a contract.** Promotion to `CONTRACT` requires a 12-delta refinement cycle like the other Phase 3 specs.

## 1. Why this discussion

The user proposed a top-level shape: every group has its own `/apps`, `/agents`, `/users`. The question is how AIOS-FS exposes a coherent namespace where:

- L4 identity (groups, users) provides the partitioning.
- L5 agents and L6 apps live as first-class versioned objects.
- S1.3's object graph is the canonical model; the directory tree is one projection.
- Recovery boundary from Rev.1 is preserved (`/aios` is the AI-native root; `/` and `/root` are not under this layout).
- Evidence remains append-only and globally hash-chained (S3.1).

## 2. Top-level layout — three candidates

### Option A — System + groups (recommended starting point)

```text
/aios/
├── system/                # AIOS itself; never under user/group control
│   ├── apps/              # evidence viewer, policy admin, vault admin, sgr console
│   ├── agents/            # system AI (translator, planner, recovery diagnostics)
│   ├── policy/            # signed policy bundles (S2.3)
│   ├── capabilities/      # capability catalog (S1.1)
│   ├── evidence/          # log segments (S3.1)
│   ├── vault/             # vault broker config + capability handles
│   ├── runtime/           # action envelopes, sandboxes, scratch
│   └── recovery/          # recovery-safe assets reachable post-boot
│
└── groups/
    └── <group_id>/
        ├── apps/          # apps installed for the group
        ├── agents/        # AI agents owned by the group
        ├── users/<user_id>/...
        ├── shared/        # group-scoped collaboration space
        ├── projects/      # task-scoped projects
        ├── datasets/      # privacy-classed data objects
        ├── inbox/         # virtual view: pending approvals + agent messages
        ├── policy/        # group delta over system policy
        ├── evidence/      # virtual view of system/evidence (S2.1 query)
        ├── vault/         # capability bindings (never raw secrets)
        └── audit/         # virtual view of all actions touching the group
```

**Rationale.** Clear separation between "AIOS itself" and "what users manage". Recovery has a predictable home (`/aios/system/recovery/`). Groups cannot write into `/aios/system/`. The recovery boundary from Rev.1 is preserved cleanly.

### Option B — Symmetric (system is the special group `_system`)

```text
/aios/groups/_system/...
/aios/groups/finance/...
/aios/groups/personal/...
```

**Rationale.** Uniform model — one rule for everything. **Why rejected as default:** it blurs the recovery boundary; constitutional invariants are harder to enforce when `_system` is "just another group".

### Option C — Hybrid naming (`tenants/` instead of `groups/`)

Same shape as Option A, different vocabulary. "Tenant" implies multi-org; "group" implies team. AIOS can ship Rev.2 with `groups/` for a simple collaborative model and open to `tenants/` later if multi-org isolation becomes a requirement.

**Recommendation:** Option A with `groups/` for Rev.2.

## 3. Per-group inner structure

Decision rule: a group is a **long-lived identity unit**, a project is **task-scoped inside a group**. We do not put `/projects/` at the top level — that splits governance ("who owns a project visible in two groups?").

```text
groups/<group_id>/
├── apps/                  # L6 packages installed; each app a versioned object
├── agents/                # L5 agent instances
│   └── <agent_id>/
│       ├── manifest.aios       # model binding, system prompt, allowed capabilities
│       ├── memory/             # vector + structured memory (S1.3 objects)
│       ├── runs/<run_id>/      # action chains, trace, evidence references
│       └── sandbox.aios        # composed sandbox profile (S3.2)
├── users/                 # L4 identity; see §4
├── shared/
│   ├── docs/
│   ├── workflows/         # parameterized action sequences
│   └── views/             # named S2.1 query views
├── projects/<proj_id>/    # task scope; agents and users attach
│   ├── inputs/
│   ├── outputs/
│   ├── runs/
│   └── decisions/         # approvals, escalations
├── datasets/              # PrivacyClass-tagged data (S1.3 §10)
├── inbox/                 # virtual view, not a real directory
├── policy/                # group delta over system policy bundle
├── evidence/              # virtual view
├── vault/                 # group's capability handles
└── audit/                 # virtual view of all group-touching actions
```

Key ideas:

- **`agents/<id>/manifest.aios`** is a versioned object (S1.3). "Modifying an agent" = new version + pointer move (CAS). The previous agent remains accessible for audit.
- **`runs/<run_id>/`** is a first-class concept. Each executed action chain has a directory whose contents are pointers to the envelope (S0.1), policy decision (S2.3), evidence receipts (S3.1), and composed sandbox profile (S3.2). Solves "who did what" without copying data.
- **`inbox/`** is a named query view (S2.1) over action envelopes in pending state. Avoids races between "directory of pending requests" and the real action lifecycle.
- **`evidence/` and `audit/`** are virtual views with automatic privacy ceiling (S3.1 §10).

## 4. Per-user inner structure

```text
users/<user_id>/
├── home/                  # personal documents, akin to classical $HOME
├── agents/                # this user's personal agents
├── prefs/                 # UI/renderer settings (KDE, Web, CLI, Voice)
├── desktop/               # KDE Plasma session state (L7)
├── inbox/                 # only this user's approvals & messages
├── outbox/                # actions submitted by this user
├── drafts/                # work-in-progress documents/queries/workflows
└── trust/                 # delegations, recovery contacts, known devices
```

**Personal vs group agent.** Different scope of capability bindings. A personal agent sees `users/<me>/`; a group agent sees `shared/` and `projects/`. No overlap by default.

## 5. Cross-spec impact (what else must change)

Adopting this namespace requires touch-ups in eight existing specs. None of them are blockers, but they need a follow-up refinement pass:

| Spec                              | Required addition                                                                                                      |
| --------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| **S1.3 Object Model**             | Object location is `(group_id, path_within_group)` or a system path. Path resolution via S2.1 query or direct pointer. |
| **S2.1 Query Language**           | `inbox`, `audit`, `evidence` views formalized as group-scoped named views.                                             |
| **S2.3 Policy Kernel**            | `target.group_id` becomes a closed condition field in the EBNF grammar (§4). Hard denies can reference a group.        |
| **S2.4 Verification**             | New primitive `aiosfs_path_in_group(path, group_id)` for property checks.                                              |
| **S3.1 Evidence**                 | Each record carries optional `group_id` (empty for system-level). Query privacy ceiling applies at group scope.        |
| **S3.2 Sandbox**                  | Composition floor depends on the group's tier (a finance group has a stricter default floor).                          |
| **L4 Identity (unrefined)**       | Group becomes a first-class identity unit, not just an attribute on a subject.                                         |
| **L5 Cognitive Core (unrefined)** | Agent is a versioned object with `group_owner` and `user_owner` (the latter nullable for shared agents).               |
| **L6 Apps**                       | App install is group-scoped. An app exists in `system/apps/` OR `groups/<g>/apps/`, never both (no duplication).       |

## 6. Open design questions

1. **`groups` or `tenants`?** Depends on whether AIOS targets multi-org (e.g., a household with separate work and family contexts on the same machine) or single-org (team). Default: `groups/` for Rev.2; `tenants/` deferred.
2. **Group nesting.** Can a group contain subgroups (`finance/audit/`)? Nested groups simplify organizational hierarchy but complicate policy inheritance. Default: **flat in Rev.2**, with tag-based labels for pseudo-hierarchy.
3. **Cross-group sharing.** Scenario: an agent in group A wants a document from group B. Options:
   - (a) Approval-based copy (new version in A, signed evidence chain).
   - (b) Capability-based read (B issues a read capability to A; vault broker mediates).
   - (c) Forbidden by default, requires explicit federation policy.
   - **Default:** (c) for Rev.2; (b) added later via Vault Broker.
4. **System group's relationship with `/aios/system/`.** Can a user be in a `_system_admins` group with write capability over `/aios/system/policy/`? Or do admin operations always go through a special recovery path? **Default:** the latter — system mutations require recovery mode + human + evidence; there is no "admin user inside a normal group".
5. **Inbox granularity.** One inbox per group, one per user, or both? **Default:** both — personal inbox shows only your items; group inbox shows everything in the group filtered by privacy ceiling.

## 7. Recommended first step

If Option A + flat groups + group-scoped views is approved, the next session writes this file as a contract spec (`05_namespace_layout.md`, status `CONTRACT`, ~600–800 lines) covering:

- Reserved top-level paths (`/aios/system/...`, `/aios/groups/...`)
- Per-group and per-user reserved subdirectories with typed shapes
- How pointer resolution works with `group_id` in path
- Cross-references to S1.3, S2.1, S2.3, S3.1, S3.2 — the additions each must absorb
- Golden fixtures (concrete sample groups: `finance`, `personal`, `homelab`)

Alternatively, before writing the contract, brainstorm a concrete real-world use case (e.g., "a household with three users and two shared agents") and shape the layout around it.

## See also

- [S1.3 — Object Model](01_object_model.md)
- [S2.1 — Query/View Language](02_query_view_language.md)
- [S2.3 — Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S3.2 — Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
- [NEXT_SESSION.md](../NEXT_SESSION.md)
