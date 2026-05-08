# Next Session — Continuity Note

| Field    | Value                       |
| -------- | --------------------------- |
| Captured | 2026-05-08                  |
| For      | Next working session        |
| Owner    | Spec discipline; not a plan |

> This is a continuity note — not an implementation plan. It records what was finished, what is open as a brainstorm, and the priority order for the remaining Rev.2 spec work so the next session can resume without re-deriving context.

## 1. What was completed in the 2026-05-08 session

**Phase 3 refinement closed.** All ten core contract specs now hold typed proto IDLs, closed vocabularies, determinism contracts, performance budgets, golden fixtures, and bounded-cardinality telemetry:

| Spec  | Topic                         | Final size | DEC entry |
| ----- | ----------------------------- | ---------- | --------- |
| S0.1  | Action Envelope and Lifecycle | 1521 lines | DEC-003   |
| S1.1  | Capability Translator         | 1340 lines | DEC-007   |
| S1.2  | Latency Tiering               | 933 lines  | DEC-008   |
| S1.3a | AIOS-FS Object Model          | 913 lines  | DEC-009   |
| S1.3b | AIOS-FS Conflict Resolution   | 515 lines  | DEC-009   |
| S2.1  | Query/View Language           | 731 lines  | DEC-010   |
| S2.2  | AIOS-FS Implementation Space  | 421 lines  | DEC-010   |
| S2.3  | Policy Kernel                 | 971 lines  | DEC-011   |
| S2.4  | Verification Grammar          | 741 lines  | DEC-012   |
| S3.1  | Evidence Log                  | 991 lines  | DEC-012   |
| S3.2  | Sandbox Composition Language  | 1107 lines | DEC-013   |

Final commits in the closing chain: `aed2c32`, `ef4c4c2`, `8cb1f3e`, `82dd0a5`, `a9ba284`, `5494fc5`, `b63ece5`.

## 2. Open brainstorm — namespace layout

A namespace layout discussion was captured as a draft in [`L2_AIOS_FS/05_namespace_layout.md`](L2_AIOS_FS/05_namespace_layout.md). Status: `DRAFT NOTES`. Promotion to `CONTRACT` is a candidate next-step (S4.1). The draft proposes:

- `/aios/system/...` for AIOS itself (apps, agents, policy, capabilities, evidence, vault, runtime, recovery).
- `/aios/groups/<group_id>/...` for user-managed scopes, each holding its own `apps`, `agents`, `users`, `shared`, `projects`, `datasets`, `inbox`, `policy`, `evidence`, `vault`, `audit`.
- Per-user `home`, `agents`, `prefs`, `desktop`, `inbox`, `outbox`, `drafts`, `trust` inside `groups/<group_id>/users/<user_id>/`.

Five open design questions are listed in §6 of the draft. Resolving them is a prerequisite for promoting to contract.

## 3. Remaining Rev.2 backlog

Layered roadmap for what is **not** yet `CONTRACT`. Not a build plan — a spec backlog.

### L0 — Governance, Evidence, Safety

- Status taxonomy contract (`REAL`, `PARTIAL`, `SHELL`, `CONTRACT`, `DEFERRED`, `BLOCKED`, `UNKNOWN`, `RETIRED`) — formalize the discipline already used informally.
- Evidence grades contract (`E0`–`E5`) — formal definition with promotion gates.
- Gates and invariants — the constitutional invariants referenced across L1–L10 (recovery without L5, AI proposes never executes, secrets-as-capabilities, evidence append-only, web UI localhost-only by default).

### L1 — Kernel, Bootstrap, Recovery

- Linux substrate boundary — what AIOS assumes from the host kernel, what it does not.
- Host bootstrap — AIOS-FS mount sequence, capability surface availability ordering.
- Generic fallback kernel — the recovery-safe kernel that boots without any L5 cognition.
- Recovery path — how `/` immutable + `/root` operator island + `/aios` AI-native root coexist.

### L4 — Policy, Identity, Vault

- Identity model (`03_identity_model.md`) — canonical subject id format; group as first-class identity unit; group↔user relationships; recovery-mode subjects. **Required by S2.3 §3 (subject normalization) and S3.2 §5.2 (floor selection).**
- Vault broker (`02_vault_broker.md`) — secrets-as-capabilities surface; capability issuance; broker-mediated operations; capability revocation.
- Approval mechanics (`04_approval_mechanics.md`) — delivery channels, signed approvals, request-hash binding (referenced from S0.1 and S2.3 but mechanics deferred).
- Emergency override (`05_emergency_override.md`) — full mechanics behind the boundary set in S2.3 §16.

### L5 — Cognitive Core

- Cognitive core overview — agents as versioned objects, model routing through S1.2, intent capture before S1.1.
- Intent capture — how natural language and structured triggers reach the translator.
- Planning — single-shot vs multi-step plans; plan as a typed object with action chains.
- Memory — agent memory as AIOS-FS objects; vector + structured; privacy-class flow.
- Model routing — provider abstraction over Ollama/vLLM-compatible local + external providers via vault broker.

### L6 — Apps, Packages, Compatibility

- Application model (`01_application_model.md`) — manifest, identity, capabilities, sandbox spec, state directory, rollback pointer.
- Package model (`02_package_model.md`) — signed bundle structure, install/verify/rollback plan, package types (app, service, model, policy, UI schema, kernel artifact, compatibility profile, workflow template).
- Compatibility runtime (`03_compatibility_runtime.md`) — Wine/Proton, Waydroid, VM fallback orchestration mechanics. **Referenced as deferred from S3.2 §17.**
- Compatibility knowledge (`05_compatibility_knowledge.md`) — per-app proven profiles; ProtonDB-equivalent governance.

### L7 — Renderers

- Shared UI schema — the typed contract that all renderers consume.
- KDE Plasma renderer — native Linux desktop integration.
- Web renderer — localhost-default; LAN/remote exposure via explicit policy.
- CLI renderer — operator console.
- Voice renderer — speech in/out.
- Mobile renderer — companion app surface.

### L8 — Network, Hardware, Devices

- Network policy — host-level network constraints; integration with sandbox `NetworkPolicy` (S3.2 §3).
- Hardware graph — typed topology of attached hardware.
- Drivers — driver capability surface; integration with L4 vault for hardware secrets.

### L9 — Observability, Admin, Operations

- Admin operations — operator workflows over the system.
- Recovery operations — operator-driven recovery; pairs with L1 recovery path and S3.1 retention guarantees.

### L10 — Distribution, Ecosystem, Marketplace

- Distribution — image build, signed bundle distribution, channel discipline.
- Ecosystem — developer surface; SDK; capability publication.
- Marketplace — app/agent/model distribution surface.

### XX — Cross-cutting

- ProxGuard reference model — already captured in `XX_Cross_Cutting/02_proxguard_reference_model.md`.
- Remaining cross-cutting contracts — to be enumerated as they emerge.

## 4. Recommended next pick

**S4.1 namespace layout (promoted from draft to contract)** is the natural next step because:

1. The draft is fresh and the user just engaged with it.
2. Eight existing contracts (S1.3, S2.1, S2.3, S2.4, S3.1, S3.2, plus L4 and L5 placeholders) all reference "groups" and "users" implicitly. Locking the namespace stops further drift.
3. It is a structural decision — the only way to test it is to write it down and see whether the cross-spec touch-ups in §5 of the draft hold up.

Strong alternative: **L4 identity model**. S2.3 §3 (subject normalization) and S3.2 §5.2 (floor selection per `is_ai`/`is_recovery_mode`) reference L4 identity but L4 is unrefined. Promoting L4 identity unblocks rigorous re-validation of those two specs.

Reasonable third pick: **L0 governance** (status taxonomy + evidence grades + invariants). It is foundational and short; could be done as a single combined refinement cycle.

The user's call. The next session opens with this question.

## 5. Process notes for the next session

- The 12-delta refinement pattern is consistent and works. Use it.
- Cross-spec table in every refined spec is non-negotiable; omitting it has cost real time before.
- Closed enum vocabularies and proto IDLs are the constitutional shape of every contract spec; do not relax.
- Telemetry contracts must keep subject id, action id, object id, profile id, and any other high-cardinality identifier OUT of metric labels. Records carry them; metrics do not.
- Each refinement cycle commits as one logical commit (file + executive summary update + DEC entry).
- Bulgarian chat, English artifacts. Implementation plans are not requested by this user — write contracts, not build sequences.
