# Cognitive Core Model (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Status         | `REAL` (initial; written 2026-05-09)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| Phase tag      | S13.1                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Layer          | L5 Cognitive Core                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Schema package | `aios.cognitive.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Consumes       | **Imports vocabulary from**: S0.1 (action envelope + lifecycle — cross-cutting, type-level), S1.3 (AIOS-FS object model + PrivacyClass — type-level), S2.3 (policy kernel hard-denies — closed reject-code enum, type-level), S3.1 (evidence record vocabulary — type-level; L5 emits records that L9 absorbs), S4.1 (namespace layout for agent objects — type-level path enum), S5.1 (identity model — `SubjectKind = AI_AGENT`, `is_ai`, capability binding scope — type-level), S5.2 (vault broker — AI hard-deny capability schema, type-level), S8.1 (network policy — `AICrossOriginPosture = AI_VAULT_BROKERED_ONLY` closed enum, type-level; L5 emits envelopes that L8 enforces network-side). **Peer (intra-L5)**: S1.1 capability translator, S1.2 latency tiering. **Integration point (peer L3, type-level only)**: S10.1 Capability Runtime gRPC (`ISOLATED_SANDBOX` dispatch and AI-origin queue cap — vocabulary imports). **Note**: L5 contract is "agent FSM proposes typed envelopes" — does NOT require L8/L9 operational; envelopes queue if downstream is degraded. S8.1/S3.1/S2.3/S5.2 references are vocabulary imports + integration points (L5 emits; downstream absorbs/enforces), not runtime requirements on higher-numbered layers. |
| Produces       | typed `Agent` (with `AgentKind`, `AgentLifecycleState`, `MemoryStore`, `Plan`); closed enums for cognitive task kind, agent kind, lifecycle state, memory class, memory privacy class, plan state, inter-agent message kind, cognitive error code; the **proposing pipeline** as a closed FSM that mechanically enforces INV-002; bounded-cardinality telemetry contract; 19 evidence record types queued for S3.1; new typed actions consumed by S1.1 (`agent.memory.read`, `agent.memory.write`, `agent.coordinate.send`, `agent.grade.attempt`, `agent.plan.submit`, `agent.plan.abandon`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Binds          | INV-002 (AI proposes, never executes), INV-010 (AI cannot self-approve), INV-011 (cross-group access forbidden), INV-013 (AI cannot perform system admin operations), INV-016 (AI cannot grade its own work), INV-017 (sandbox floor constitutional), INV-018 (vault never leaks raw secrets)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |

## 1. Purpose

The Cognitive Core is the AIOS subsystem that perceives operator intent, plans, remembers, coordinates with other agents, and emits typed action proposals — never executing them. It is the place where AI agency is bounded by structural property, not by aspiration: every proposal flows out as a typed S0.1 envelope to the Policy Kernel; every execution decision is made by L4.1 plus L4.3 (operator approval); every secret use is mediated by L5.2 vault broker; every external network call is brokered by L8.1.

Until this contract, runtime, network, and distribution layers are guards around an empty center. This sub-spec closes that hole. It defines what an AI agent **is** in AIOS terms, where it lives, how it perceives and plans, how its memory works, how it coordinates with other agents, and — most importantly — how its proposing pipeline mechanically respects the L0 invariants that bound AI agency. Smart by structural property. Stupid is impossible.

This spec defines:

1. The closed taxonomy of cognitive task kinds (`CognitiveTaskKind`).
2. The closed taxonomy of agent kinds (`AgentKind`) and the agent lifecycle FSM (`AgentLifecycleState`).
3. The agent identity binding to S5.1 `Subject` and S4.1 namespace paths.
4. The **proposing pipeline** as a closed FSM, with explicit checkpoints that mechanically enforce INV-002.
5. The closed taxonomy of memory classes (`MemoryClass`) and per-class privacy enforcement against AIOS-FS PrivacyClass.
6. The inter-agent coordination model — every cross-agent interaction is a typed action; no shared filesystem write; no cross-group coordination.
7. The external-model-call canonical pattern, binding S8.1 §J (`AI_VAULT_BROKERED_ONLY`) and S5.2 (use-without-reveal).
8. The self-grading prohibition, binding INV-016 mechanically.
9. The plan FSM (`PlanState`) and the two approval granularities (per-action default, bundled requires STRONG strength).
10. Recovery-mode behavior: agents are stopped, plans freeze, FOREVER evidence emitted.
11. The adapter model for cognitive backends (LangGraph, Anthropic API via vault broker, local Ollama, etc.) — framework-neutral.
12. Adversarial robustness: every named bypass attempt is mapped to its existing enforcement layer.
13. The bounded-cardinality telemetry contract — agent identifiers never appear as labels.
14. The 19 evidence record types queued for S3.1 next-Wave consolidation (including `AGENT_LIFECYCLE_TRANSITIONED` as positive-witness for FSM traversal — §16.1) and the closed `AgentLifecycleTransitionTrigger` enum (§16.2).
15. The full `aios.cognitive.v1alpha1` gRPC surface used by renderers, the planner, S1.1, and S1.2.

What this spec does **not** define:

- A specific LLM framework. Frameworks are adapters per S10.1 `AdapterManifest`.
- The full memory schema for episodic/semantic projections — those are AIOS-FS S1.3 objects with PrivacyClass; the schema is owned by the agent's adapter and validated against S1.3.
- The verification grammar — owned by L9 S2.4; this spec only references `verification_intent` outputs.
- Approval UX — owned by L4 approval mechanics (S5.3); this spec only references approval state.

## 2. Position in the system

The Cognitive Core sits inside L5. Its inputs come from L7 renderers (operator utterance) and from other agents (coordination messages). Its outputs are S0.1 envelopes consumed by L3, evidence records consumed by L9, and memory writes consumed by L2.

```text
operator                  another agent
   │  utterance            │  WORK_REQUEST
   ▼                       ▼
┌──────────────────────────────────────────────────────────────┐
│                       L5 Cognitive Core                      │
│                                                              │
│   INTENT_PERCEPTION → Intent (structured)                    │
│           │                                                  │
│           ▼                                                  │
│       PLANNING → Plan (sequence of action drafts)            │
│           │                                                  │
│           ▼                                                  │
│   ACTION_PROPOSAL_DRAFTING (per action)                      │
│           │                                                  │
│           ▼                                                  │
│   S1.1 Capability Translator (typed envelope from intent)    │
│           │                                                  │
│           ▼                                                  │
│   S1.2 Latency Tier routing (workload classification)        │
│           │                                                  │
│           ▼                                                  │
│   S0.1 ActionEnvelope emitted to L3                          │
│           │                                                  │
│           ▼                                                  │
│   agent → BLOCKED_AWAITING_APPROVAL                          │
└─────┬───────────────────────────────────────────────────┬────┘
      │ envelope                                          │ evidence
      ▼                                                   ▼
┌────────────────────────────┐               ┌──────────────────────────┐
│ L3 Capability Runtime      │               │ L9 Evidence Log (S3.1)   │
│ L4.1 Policy Kernel         │               │ AGENT_PROPOSAL_EMITTED,  │
│ L4.3 Approval (operator)   │ ◀── result ──▶│ AGENT_PROPOSAL_APPROVED, │
│ Adapter execution          │               │ AGENT_MEMORY_WRITE, ...  │
└────────────┬───────────────┘               └──────────────────────────┘
             │ result
             ▼
   agent → VERIFICATION_REASONING → ACTIVE
             │
             ▼
       MEMORY_WRITE (typed action; back through pipeline)
             │
             ▼
       L2 AIOS-FS object under /aios/groups/<g>/agents/<a>/memory/<class>/...
```

The agent never executes. The pipeline ends, from the agent's perspective, at "envelope emitted to L3"; it resumes at "result available for reasoning". Between those two points, L3 + L4 own the action. This is mechanical INV-002 enforcement.

## 3. Core invariants

- **C1 — AI proposes, never executes (binds INV-002).** The proposing pipeline (§7) is a closed FSM. There is no transition in any agent FSM that crosses from "agent thinking" into "agent executing". Execution is L3's responsibility per S10.1; the agent's responsibility ends at envelope emission and resumes at result availability. An agent attempting to skip this pipeline (e.g., direct AIOS-FS write, direct adapter call) is hard-denied at the relevant enforcer (S2.3, S4.1, S10.1) and FOREVER evidence is emitted.

- **C2 — AI cannot self-approve (binds INV-010).** No agent's lifecycle state may carry "approver" semantics. The `BLOCKED_AWAITING_APPROVAL` state is unblockable by any subject with `is_ai = true`. The unblocker must be a `HUMAN_USER` (or a constitutional `_system` subject under recovery, per S5.1 §7).

- **C3 — AI cannot system-admin (binds INV-013).** Agents cannot author actions targeting `/aios/system/apps/` or `/aios/system/agents/`. Any draft naming a system-scope target is rejected at proposal-drafting time with `CognitiveErrorCode = SYSTEM_ADMIN_DRAFT_REFUSED`; if it nonetheless reaches L3, S2.3 hard-deny `AISystemAdminBlocked` fires and `AGENT_DIRECT_FS_WRITE_BLOCKED` evidence is emitted FOREVER.

- **C4 — AI cannot self-grade (binds INV-016).** The typed action `agent.grade.attempt` is rejected closed when `subject.is_ai = true` AND `subject_canonical_id == graded_capability.author_subject_id`. The check is hard-coded; no policy bundle can lift it.

- **C5 — Memory write is itself a typed action.** No agent writes to its own memory store by direct AIOS-FS write. Even self-memory is gated through the proposing pipeline. This prevents an agent from using memory as a side channel to communicate around policy.

- **C6 — Inter-agent coordination is typed-action-mediated.** No agent reads another agent's memory directly; no agent writes another agent's plan or working state directly. Cross-agent interaction is the typed action `agent.coordinate.send`, decided by S2.3 like any other action.

- **C7 — Cross-group coordination is forbidden (binds INV-011).** An agent in group A cannot send an `agent.coordinate.send` action targeting any subject under `/aios/groups/<B>/...` for `B ≠ A`. The check is the existing S2.3 hard-deny `CrossGroupAccessForbidden`; this spec only states that the agent cannot bypass it.

- **C8 — External model calls are vault-brokered only (binds INV-018, S8.1 §J).** An agent that needs an external LLM emits the typed action `external_model_call`. The vault broker holds the API key as `KEY_ENCRYPT` / `MAC_GENERATE` material per S5.2; the agent never sees the key. L8.1 brokers the connection with `AICrossOriginPosture = AI_VAULT_BROKERED_ONLY`. Any direct fetch attempt fails closed with `AI_DIRECT_INTERNET_DENIED` FOREVER.

- **C9 — Sandbox floor is constitutional (binds INV-017).** Every agent-emitted action dispatches under `ISOLATED_SANDBOX` per S10.1 §6; the floor cannot be loosened by any composition source. The agent has no path to a less-restrictive sandbox.

- **C10 — Recovery stops L5.** In `recovery_mode = true`, no L5 service runs (`L5StartProhibitedInRecovery`). Agents in any non-terminal state at recovery entry transition to `RETIRING` with FOREVER `AGENT_INTERRUPTED_BY_RECOVERY` evidence. Plans freeze. After recovery, agents do not auto-resume — the operator must explicitly restart them.

- **C11 — `is_ai` is unforgeable.** The agent's `Subject` carries `is_ai = true`, signed at registration by the identity service per S5.1 §I6. The agent cannot self-declare as human; the cognitive core does not perform any check that would distinguish AI-from-human at runtime — the identity service has already done it, and the rest of AIOS reads the flag.

- **C12 — Determinism in the pipeline boundaries.** The pipeline transitions are deterministic. Same `(intent, plan, agent_subject_id, identity_bundle_version, catalog_version)` produces the same envelope set. The model output that fills cognitive task results is non-deterministic; the structural projection into typed envelopes is deterministic.

## 4. Vocabulary

### 4.1 `CognitiveTaskKind`

The closed enum of tasks an agent can run inside the core. Routing of these tasks across latency tiers is S1.2's responsibility; this spec only fixes the kinds.

```proto
enum CognitiveTaskKind {
  COGNITIVE_TASK_KIND_UNSPECIFIED   = 0;
  INTENT_PERCEPTION                 = 1;  // operator utterance → structured Intent
  PLANNING                          = 2;  // Intent → Plan (sequence of action drafts)
  ACTION_PROPOSAL_DRAFTING          = 3;  // single Plan step → S0.1 ActionEnvelope draft
  MEMORY_RECALL                     = 4;  // read from agent's MemoryStore
  MEMORY_WRITE                      = 5;  // write to agent's MemoryStore (always typed action)
  INTER_AGENT_COORDINATION          = 6;  // negotiate work split with another agent
  VERIFICATION_REASONING            = 7;  // interpret L9 verification result post-execution
  EXTERNAL_MODEL_CALL               = 8;  // vault-brokered LLM invocation per S8.1 §J
  TRANSLATION                       = 9;  // S1.1 capability translator invocation
  LATENCY_ROUTING                   = 10; // S1.2 latency tier selection
}
```

Closed enum. Adding a kind is a versioned spec change.

### 4.2 `AgentKind`

```proto
enum AgentKind {
  AGENT_KIND_UNSPECIFIED  = 0;
  ASSISTANT               = 1;  // general-purpose; binds to a specific user under a group
  WORKER                  = 2;  // single-task; spawned per workflow; auto-retired
  DAEMON                  = 3;  // long-running observer; system-scoped under `_system` group
  COORDINATOR             = 4;  // orchestrates multi-agent workflows
  TRANSLATOR              = 5;  // specialised — runs S1.1 capability translation
  ROUTER                  = 6;  // specialised — runs S1.2 latency routing
}
```

Closed enum. Adding a kind is a versioned spec change.

| Kind          | Lives at                                              | Default `home_group_id`       | Auto-retire?                                             |
| ------------- | ----------------------------------------------------- | ----------------------------- | -------------------------------------------------------- |
| `ASSISTANT`   | `/aios/groups/<g>/users/<u>/agents/<a>/`              | the user's `primary_group_id` | no — operator-managed                                    |
| `WORKER`      | `/aios/groups/<g>/shared/workflows/<wf>/workers/<a>/` | the workflow's group          | yes — on workflow completion or 24h idle whichever first |
| `DAEMON`      | `/aios/system/agents/<a>/`                            | `_system`                     | no — recovery-only mutation                              |
| `COORDINATOR` | `/aios/groups/<g>/agents/<a>/`                        | the group                     | configurable                                             |
| `TRANSLATOR`  | `/aios/system/agents/translator/`                     | `_system`                     | no                                                       |
| `ROUTER`      | `/aios/system/agents/router/`                         | `_system`                     | no                                                       |

`DAEMON`, `TRANSLATOR`, `ROUTER` register under `_system` per S5.1 §3 and require a recovery-mode operation to install or modify, per INV-013. End-user agents (`ASSISTANT`, `WORKER`, `COORDINATOR`) register through the normal flow under their home group.

### 4.3 `AgentLifecycleState`

```proto
enum AgentLifecycleState {
  AGENT_LIFECYCLE_STATE_UNSPECIFIED   = 0;
  INITIALIZING                        = 1;  // capability bindings being verified, memory store mounted
  ACTIVE                              = 2;  // running cognitive tasks
  IDLE                                = 3;  // no active work; awaiting input
  BLOCKED_AWAITING_APPROVAL           = 4;  // operator approval pending on a proposed action
  BLOCKED_AWAITING_DEPENDENCY         = 5;  // waiting on another agent's output
  BLOCKED_AWAITING_VAULT              = 6;  // vault capability binding pending (e.g., key issuance)
  DEGRADED                            = 7;  // model unavailable, fallback adapter active
  RETIRING                            = 8;  // gracefully winding down; emits final memory writes
  RETIRED                             = 9;  // terminal; memory retained per retention class
}
```

Closed enum. The transition graph:

```text
INITIALIZING ──► ACTIVE ──► IDLE
                  │            │
                  │            └────► (input arrives) ─► ACTIVE
                  │
                  ├─► BLOCKED_AWAITING_APPROVAL ──► ACTIVE | RETIRING
                  ├─► BLOCKED_AWAITING_DEPENDENCY ──► ACTIVE | RETIRING
                  ├─► BLOCKED_AWAITING_VAULT ──► ACTIVE | RETIRING
                  ├─► DEGRADED ─► ACTIVE | RETIRING
                  └─► RETIRING ─► RETIRED
```

`RETIRED` is terminal. The agent's canonical id is **never reused** (per S5.1 §4.4 — canonical ids are immutable for the lifetime of the evidence trail). Re-instating an agent name requires a new `Subject` with a new id.

### 4.4 `MemoryClass`

```proto
enum MemoryClass {
  MEMORY_CLASS_UNSPECIFIED  = 0;
  EPHEMERAL                 = 1;  // within-session only; never persisted to AIOS-FS
  EPISODIC                  = 2;  // records of past events; persisted; per-agent + per-group + per-user privacy
  SEMANTIC                  = 3;  // learned generalities; persisted; per-group; cross-user within group
  WORKING                   = 4;  // planning scratchpad; ephemeral
  SHARED                    = 5;  // cross-agent within a group; written via approved actions; never auto-shared
}
```

Closed enum. See §8 for storage and privacy semantics.

### 4.5 `MemoryPrivacyClass`

Mirrors and tightens S1.3 PrivacyClass for cognitive use. The mapping to S1.3 PrivacyClass is constitutional: a memory write fails closed if its `MemoryPrivacyClass` would weaken the AIOS-FS object's existing PrivacyClass.

```proto
enum MemoryPrivacyClass {
  MEMORY_PRIVACY_CLASS_UNSPECIFIED  = 0;
  PUBLIC_TO_GROUP                   = 1;  // any group member's agent can read; write requires source agent's grant
  PRIVATE_TO_USER                   = 2;  // only agents bound to this user (and the user) can read
  PRIVATE_TO_AGENT                  = 3;  // only this agent's subject can read or write
  SYSTEM_INTERNAL                   = 4;  // only `_system`-scoped agents
  TRANSIENT                         = 5;  // never persisted regardless of class
}
```

| `MemoryPrivacyClass` | S1.3 PrivacyClass mapping | Stored at                                                   | Cross-user readable within group? |
| -------------------- | ------------------------- | ----------------------------------------------------------- | --------------------------------- |
| `PUBLIC_TO_GROUP`    | `GROUP_INTERNAL`          | `/aios/groups/<g>/agents/<a>/memory/shared/...`             | yes (members)                     |
| `PRIVATE_TO_USER`    | `USER_PRIVATE`            | `/aios/groups/<g>/users/<u>/agents/<a>/memory/episodic/...` | no (binds to user)                |
| `PRIVATE_TO_AGENT`   | `OWNER_ONLY`              | `/aios/groups/<g>/users/<u>/agents/<a>/memory/private/...`  | no (binds to agent subject)       |
| `SYSTEM_INTERNAL`    | `SYSTEM_RESTRICTED`       | `/aios/system/agents/<a>/memory/...`                        | n/a (system-scoped)               |
| `TRANSIENT`          | not persisted             | in-process working set; lost on agent retirement            | n/a                               |

### 4.6 `PlanState`

```proto
enum PlanState {
  PLAN_STATE_UNSPECIFIED  = 0;
  DRAFT                   = 1;  // agent thinking; not yet submitted
  PROPOSED                = 2;  // full plan submitted as a sequence of S0.1 actions; awaiting decision
  PARTIALLY_APPROVED      = 3;  // some actions approved, others awaiting
  EXECUTING               = 4;  // actions flowing through L3 capability runtime
  COMPLETED               = 5;  // all approved actions reached a terminal lifecycle state
  ABANDONED               = 6;  // operator stopped the plan
  FAILED                  = 7;  // one or more actions failed; rollback may be triggered
}
```

Closed enum. Transitions:

```text
DRAFT ─► PROPOSED ─► PARTIALLY_APPROVED ─► EXECUTING ─► COMPLETED | FAILED
                │                                        │
                ├──────────────────────► ABANDONED ◄─────┘
                │                            ▲
                └─────────────────────────► (any non-terminal state may transition to ABANDONED)
```

`COMPLETED`, `ABANDONED`, `FAILED` are terminal.

### 4.7 `InterAgentMessageKind`

```proto
enum InterAgentMessageKind {
  INTER_AGENT_MESSAGE_KIND_UNSPECIFIED  = 0;
  WORK_REQUEST                          = 1;  // agent A requests agent B do task
  WORK_RESPONSE                         = 2;  // agent B responds with output / refusal
  COORDINATION_QUERY                    = 3;  // agent A asks for B's state / availability
  COORDINATION_REPLY                    = 4;  // B replies to query
  HANDOFF                               = 5;  // agent A hands work to agent B
  ESCALATION                            = 6;  // agent A asks operator review via the chrome path
  BROADCAST                             = 7;  // announce to a coordination channel within the group
  SHUTDOWN_REQUEST                      = 8;  // coordinator asks worker to retire
}
```

Closed enum.

### 4.8 `CognitiveErrorCode`

```proto
enum CognitiveErrorCode {
  COGNITIVE_ERROR_CODE_UNSPECIFIED        = 0;
  MODEL_UNAVAILABLE                       = 1;  // backend adapter unreachable; fallback in progress
  INTENT_AMBIGUOUS                        = 2;  // INTENT_PERCEPTION returned multiple structured candidates
  PLAN_UNFEASIBLE                         = 3;  // PLANNING could not produce a sequence within budget
  MEMORY_WRITE_REJECTED                   = 4;  // policy denied the memory.write typed action
  MEMORY_READ_REJECTED                    = 5;  // policy denied memory.read; or cross-user attempt
  INTER_AGENT_MESSAGE_REJECTED            = 6;  // policy denied agent.coordinate.send
  PROPOSAL_DRAFT_FAILED                   = 7;  // S1.1 returned REJECTED; no typed envelope possible
  EXTERNAL_MODEL_CALL_REJECTED            = 8;  // policy or vault denied external_model_call
  SELF_GRADE_ATTEMPT_BLOCKED              = 9;  // agent.grade.attempt failed C4 / INV-016 check
  SYSTEM_ADMIN_DRAFT_REFUSED              = 10; // draft named `/aios/system/...` target as agent author
  RECOVERY_MODE_AGENT_STOPPED             = 11; // agent halted because recovery_mode = true
  CAPABILITY_BINDING_INVALID              = 12; // identity_bundle_version drift; binding refused
  PROMPT_INJECTION_DETECTED               = 13; // adversarial input filter fired
  CROSS_GROUP_COORDINATION_REFUSED        = 14; // C7 / INV-011
  SANDBOX_FLOOR_REGRESSION_REFUSED        = 15; // proposed sandbox profile would loosen floor
  PLAN_BUNDLED_APPROVAL_INELIGIBLE        = 16; // plan contains hard-denied actions; per-action only
}
```

Closed enum. Every cognitive task that fails surfaces one of these.

## 5. Agent identity and binding

### 5.1 Subject binding

Every agent is a `Subject` with `SubjectKind = AI_AGENT` and `is_ai = true`, set at registration by the identity service per S5.1 §3 and §4. The flag is signed; the agent cannot self-declare as human; the rest of AIOS reads the flag without re-checking.

```proto
message AgentBinding {
  string agent_canonical_id        = 1;  // S5.1 §4 canonical id; matches /^[a-z_][a-z0-9_-]{0,62}(:[a-z0-9_-]+)+$/
  string home_group_id             = 2;  // S4.1 §7 group id; never empty
  string bound_user_id             = 3;  // optional; required for ASSISTANT kind, optional for WORKER, empty for DAEMON/TRANSLATOR/ROUTER
  string identity_bundle_version   = 4;  // idbundle_<hex_lower(BLAKE3(...))[:32]> at binding time
  AgentKind agent_kind             = 5;
  google.protobuf.Timestamp registered_at = 6;
  string registered_by             = 7;  // canonical id of the human approver
}
```

### 5.2 Identifier formats

| Identifier      | Format                                   | Notes                                                                             |
| --------------- | ---------------------------------------- | --------------------------------------------------------------------------------- |
| `AgentId`       | `agent:<group_id>:<agent_name>:<ulid26>` | ULID is the agent's lifetime serial number; never reused after `RETIRED`.         |
| `PlanId`        | `plan_<ulid26>`                          | Matches S0.1 `plan_id` prefix; one plan per logical decomposition.                |
| `MemoryEntryId` | `mem_<ulid26>`                           | One id per memory write; not reused on overwrite (overwrites create a new entry). |
| `IntentId`      | `intent_<ulid26>`                        | Matches S0.1 `intent_id`.                                                         |
| `MessageId`     | `msg_<ulid26>`                           | Inter-agent message identifier.                                                   |

ULID per S0.1 §3.2 — Crockford base32, 26 chars, lexicographically sortable.

Hash algorithm and encoding used throughout this spec is `hex_lower(BLAKE3(...))[:32]` exactly, matching S0.1 §8.5, S1.1 §6.3, S1.2 §10.3 — lowercase hex, BLAKE3-256, 32-character truncation.

### 5.3 Where agents live in the namespace

Per S4.1 §5–§6:

```text
/aios/system/agents/<agent_name>/                                 # DAEMON, TRANSLATOR, ROUTER
/aios/groups/<group_id>/agents/<agent_name>/                      # COORDINATOR, group-shared agents
/aios/groups/<group_id>/users/<user_id>/agents/<agent_name>/      # ASSISTANT (personal), WORKER under user
/aios/groups/<group_id>/shared/workflows/<wf>/workers/<agent_name>/  # WORKER spawned by a workflow
```

The agent's directory contains:

```text
.../agents/<agent_name>/
├── manifest.proto                # signed AgentManifest
├── binding.proto                 # signed AgentBinding (above)
├── plans/                        # active and historical Plan objects
├── memory/
│   ├── episodic/                 # MemoryClass = EPISODIC
│   ├── semantic/                 # MemoryClass = SEMANTIC
│   ├── shared/                   # MemoryClass = SHARED (PUBLIC_TO_GROUP)
│   └── private/                  # MemoryClass = PRIVATE_TO_AGENT
├── inbox/                        # virtual; pending inter-agent messages targeting this agent
└── adapters/                     # per-agent cognitive backend adapter selections
```

`memory/episodic/` under a personal agent path inherits PrivacyClass `USER_PRIVATE` from S1.3 — no cross-user read even within the same group.

### 5.4 Capability bindings

Agent capability bindings are scoped per S5.1 §6.4 to `(agent_subject_id, home_group_id, identity_bundle_version)`. On bundle rollover (S5.1 §8), all bindings tied to the prior version are invalidated; the agent transitions to `INITIALIZING` until the new bundle version's bindings are issued. The vault broker rejects use of stale bindings (S5.2 §I2).

### 5.5 Provenance and signing

The agent's `manifest.proto` is signed by:

1. The publisher key (the cognitive backend adapter's publisher).
2. The AIOS root key endorsement of the publisher's adapter domain.

Signature failure at registration → registration rejected; no `Subject` issued; no `Agent` created.

### 5.6 What an agent acquires at init

At `INITIALIZING`:

1. Identity service issues `Subject` and signs `AgentBinding`.
2. Vault broker issues capability bindings per the manifest's declared capability list (intersected with the human approver's grant).
3. AIOS-FS opens the agent's directory; mounts the four memory subtrees with the appropriate PrivacyClass per §4.5.
4. A fresh `MemoryStore` handle is wired into the agent's adapter.
5. A fresh `Plan` slot is allocated (empty `DRAFT`).
6. Telemetry registration: `agent_active_total{agent_kind, lifecycle_state="INITIALIZING"}` is incremented.
7. Evidence record `AGENT_REGISTERED` is appended (STANDARD_24M retention).

Transition to `ACTIVE` occurs only after all six steps succeed. Any failure → transition to `RETIRING` and `AGENT_RETIRED` evidence with the failure reason in the payload.

## 6. The proposing pipeline (mechanical INV-002 enforcement)

The pipeline is a closed FSM with explicit checkpoints. At no transition does the agent execute the action. Execution is L3's responsibility per S10.1 §6. The agent's role ends at "envelope emitted to runtime" and resumes at "result available for reasoning". This is **mechanical INV-002 enforcement** — not policy, but FSM structure.

### 6.1 The pipeline as an FSM

```text
        ┌──────────────────────────────────────────────────────────┐
        │                   AGENT (Cognitive Core)                 │
        │                                                          │
input:  │   intent observed                                        │
        │       │                                                  │
        │       ▼                                                  │
        │   INTENT_PERCEPTION ─► Intent (structured)               │
        │       │                                                  │
        │       ▼                                                  │
        │   PLANNING ─► Plan (sequence of action drafts)           │
        │       │                                                  │
        │       ▼                                                  │
        │   ACTION_PROPOSAL_DRAFTING (per draft)                   │
        │       │                                                  │
        │       ▼                                                  │
        │   S1.1 TranslateIntent (typed envelope)                  │
        │       │                                                  │
        │       ▼                                                  │
        │   S1.2 Route (latency tier annotation)                   │
        │       │                                                  │
        │       ▼                                                  │
        │   S0.1 ActionEnvelope ─emitted─► L3 SubmitAction         │
        │                                          │               │
        │   AGENT TRANSITIONS                      │               │
        │   ACTIVE ─► BLOCKED_AWAITING_APPROVAL    │               │
        │       │                                  │               │
        │       │      (L4.1 + L4.3 decide;        │               │
        │       │       agent does NOT participate)│               │
        │       │                                  │               │
        │       │      ┌───────────────────────────┘               │
        │       ▼      ▼                                           │
        │   policy decision arrives via L9 evidence event          │
        │       │                                                  │
        │       ├─ approved + executed: result returned via runtime│
        │       │       │                                          │
        │       │       ▼                                          │
        │       │   VERIFICATION_REASONING                         │
        │       │       │                                          │
        │       │       ▼                                          │
        │       │   ACTIVE again; MEMORY_WRITE (typed action) of   │
        │       │   episodic record summarizing the outcome        │
        │       │                                                  │
        │       ├─ denied: PROPOSAL_DRAFT_FAILED-equivalent logic; │
        │       │   agent records the denial in EPISODIC memory    │
        │       │   (via MEMORY_WRITE typed action) and returns to │
        │       │   ACTIVE / IDLE                                  │
        │       │                                                  │
        │       └─ approval timeout: agent transitions to ACTIVE   │
        │           (the request is not auto-resubmitted; the      │
        │            operator is the source of truth for retry)    │
        │                                                          │
        └──────────────────────────────────────────────────────────┘
```

### 6.2 Why this is mechanical, not aspirational

A reasoning system that "tries to remember not to execute" is a system that will eventually execute. The defense lies in **structural impossibility**, not behavioral discipline:

1. The cognitive core has no adapter for "execute action". The only outward path that reaches the operating system is through `SubmitAction` to L3.
2. `SubmitAction` writes the envelope to a queue; the agent does not have a thread that drains the queue. L3's executor drains it.
3. The agent's `BLOCKED_AWAITING_APPROVAL` state has no self-unblock. The unblock signal comes from a policy-decision event delivered via the evidence stream by L9; the agent reads the event but cannot generate it.
4. INV-002 enforcement is verified by S2.4 property `POLICY_AI_SELF_APPROVAL_BLOCKED` (existing) plus this spec's queued property `AI_PROPOSAL_PIPELINE_INTACT` (queued for S2.4 next-Wave).

Each transition through the proposing pipeline FSM (and through the broader `AgentLifecycleState` graph in §4.3) emits an `AGENT_LIFECYCLE_TRANSITIONED` evidence record (`STANDARD_24M`, queued for S3.1 next-Wave consolidation per §16.1) carrying `(from_state, to_state, transition_trigger, originating_action_id?)`. This converts the previously-implicit FSM traversal into observable evidence; INV-002 site 5 enforcement at C1 (§3) is now verifiable both **by absence** of `FOREVER` bypass records (e.g., `AGENT_DIRECT_FS_WRITE_BLOCKED`, `AGENT_SELF_GRADING_BLOCKED`) **and by presence** of legitimate `AGENT_LIFECYCLE_TRANSITIONED` records — notably the `ACTIVE → BLOCKED_AWAITING_APPROVAL` transition with `transition_trigger = ENVELOPE_EMITTED` carrying the `originating_action_id` of the just-submitted envelope.

### 6.3 Per-checkpoint evidence

| Pipeline checkpoint               | Evidence record (queued for S3.1)              | Retention    |
| --------------------------------- | ---------------------------------------------- | ------------ |
| Plan submitted                    | (covered by `AGENT_PROPOSAL_EMITTED` per item) | n/a          |
| Action draft emitted (per draft)  | `AGENT_PROPOSAL_EMITTED`                       | STANDARD_24M |
| Approval received and bound       | `AGENT_PROPOSAL_APPROVED`                      | STANDARD_24M |
| Approval refused / denied         | `AGENT_PROPOSAL_DENIED`                        | EXTENDED_60M |
| Plan bundled and approved         | `AGENT_PLAN_BUNDLED_APPROVED`                  | STANDARD_24M |
| Plan abandoned                    | `AGENT_PLAN_ABANDONED`                         | EXTENDED_60M |
| Verification result reasoned over | (no separate record; agent's MEMORY_WRITE)     | n/a          |
| Every legitimate FSM transition   | `AGENT_LIFECYCLE_TRANSITIONED` (§16.1)         | STANDARD_24M |

### 6.4 Pipeline-level error mapping

| Failure                                                   | Cognitive error code        | Evidence record                             |
| --------------------------------------------------------- | --------------------------- | ------------------------------------------- |
| INTENT_PERCEPTION returns multiple irreducible candidates | `INTENT_AMBIGUOUS`          | (renderer asks operator; no FOREVER)        |
| PLANNING exceeds time/step budget                         | `PLAN_UNFEASIBLE`           | `AGENT_BACKEND_DEGRADED` if backend cause   |
| S1.1 returned REJECTED                                    | `PROPOSAL_DRAFT_FAILED`     | (S1.1's TranslationEvidence carries reason) |
| Backend adapter unreachable                               | `MODEL_UNAVAILABLE`         | `AGENT_BACKEND_DEGRADED` (EXTENDED_60M)     |
| Prompt injection detected                                 | `PROMPT_INJECTION_DETECTED` | `AGENT_PROMPT_INJECTION_DETECTED` (FOREVER) |

## 7. Memory model

### 7.1 Per-class semantics

| `MemoryClass` | Persistence  | Lives at                                       | Default retention | Cross-agent visibility default |
| ------------- | ------------ | ---------------------------------------------- | ----------------- | ------------------------------ |
| `EPHEMERAL`   | session only | in-process only                                | session lifetime  | none                           |
| `EPISODIC`    | persisted    | `/aios/.../agents/<a>/memory/episodic/`        | STANDARD_24M      | deny (per agent)               |
| `SEMANTIC`    | persisted    | `/aios/groups/<g>/agents/<a>/memory/semantic/` | STANDARD_24M      | within group readable          |
| `WORKING`     | session only | in-process planning scratchpad                 | plan lifetime     | none                           |
| `SHARED`      | persisted    | `/aios/groups/<g>/agents/<a>/memory/shared/`   | STANDARD_24M      | within group, write-approved   |

### 7.2 Memory write is itself a typed action (binds C5)

Every persisted memory write is the typed action `agent.memory.write` with payload:

```proto
message AgentMemoryWriteRequest {
  string agent_canonical_id    = 1;
  MemoryClass memory_class     = 2;
  MemoryPrivacyClass privacy   = 3;
  string entry_id              = 4;  // mem_<ULID26>
  bytes payload                = 5;  // adapter-defined schema; redacted on evidence
  string payload_digest        = 6;  // hex_lower(BLAKE3(payload))[:32] — payload is already-canonical proto wire bytes; no JCS step (deterministic proto serialisation per S0.1 §8.5)
  google.protobuf.Timestamp written_at = 7;
}
```

Routing of this action:

1. The agent emits the envelope through the proposing pipeline (§6).
2. S1.1 produces a typed envelope; S1.2 routes it through T1 (deterministic — no LLM needed for memory write).
3. S2.3 evaluates the policy:
   - For `MemoryPrivacyClass = PRIVATE_TO_AGENT`: granted with `STANDARD` strength, no operator approval required (the agent is the only authorized writer).
   - For `MemoryPrivacyClass = PRIVATE_TO_USER`: granted only when the agent is bound to that user (`bound_user_id`) and written under the user's session.
   - For `MemoryPrivacyClass = PUBLIC_TO_GROUP` and `SHARED`: requires operator approval at `STANDARD` strength. The operator decides whether the agent's "this is something the group should know" is acceptable.
   - For `MemoryPrivacyClass = SYSTEM_INTERNAL`: only `_system`-scoped agents pass (e.g., the translator updating its own self-stats). Other agents receive `MEMORY_WRITE_REJECTED`.
4. On success → `AGENT_MEMORY_WRITE` evidence (STANDARD_24M).

### 7.3 Memory read

The typed action `agent.memory.read` carries `(target_path, requested_class)`. Policy decision:

- An agent reading its own `PRIVATE_TO_AGENT` memory: granted automatically.
- An agent reading a `PRIVATE_TO_USER` memory belonging to a different user (even within the same group): **denied** with `MEMORY_READ_REJECTED`; FOREVER `AGENT_MEMORY_CROSS_USER_DENIED` evidence emitted.
- An agent reading `PUBLIC_TO_GROUP` memory in its home group: granted (group membership suffices).
- An agent reading any memory under `_system`: denied unless the agent is `_system`-scoped.

### 7.4 INV-016 binding at the memory layer

A memory entry written by an agent **cannot self-promote** the status of any capability. The grade-attempt action (`agent.grade.attempt`) is rejected closed when the author is AI and the cited artifact is also authored by the same AI subject (per C4). A memory entry that says "this worked!" is just an episodic record; it carries no grade-receipt semantics. The grade-receipt path is owned by S6.2 §10.6 and rejects AI producers for AI-authored artifacts.

### 7.5 Memory retention

Retention class for the AIOS-FS object is set at creation by the agent's `AgentManifest.retention_class`, drawn from S1.3 retention vocabulary. The retention floor is `STANDARD_24M` for `EPISODIC`, `SEMANTIC`, and `SHARED` — the agent cannot select a tighter floor that would erase forensic evidence of its own writes.

### 7.6 Memory at retirement

When an agent transitions to `RETIRING`:

1. `EPHEMERAL` and `WORKING` memory is dropped.
2. `EPISODIC`, `SEMANTIC`, `SHARED`, `PRIVATE_TO_AGENT` memory remains at the retention class declared at creation.
3. The agent's `manifest.proto` is moved to `retired/<ulid26>/manifest.proto` for forensic queryability.
4. `AGENT_RETIRED` evidence (EXTENDED_60M) is emitted; the canonical id can never be re-issued.

## 8. Inter-agent coordination

### 8.1 The typed action `agent.coordinate.send`

Every cross-agent message is a typed action. Direct memory access between agents is forbidden — there is no "I'll read your scratchpad" path. The policy kernel decides every message.

```proto
message AgentCoordinateSendRequest {
  string from_agent_canonical_id   = 1;
  string to_agent_canonical_id     = 2;
  InterAgentMessageKind kind       = 3;
  string subject_line              = 4;  // ≤256 chars, plain text; appears in operator's audit views
  bytes payload                    = 5;  // adapter-defined; subject to redaction in evidence
  string payload_digest            = 6;
  string correlation_id            = 7;  // S0.1 correlation_id propagation
  google.protobuf.Timestamp sent_at = 8;
}
```

### 8.2 Policy gates

The S2.3 policy kernel evaluates the message:

1. Same group? If `from.home_group_id != to.home_group_id`, hard-deny `CrossGroupAccessForbidden` (C7 / INV-011); FOREVER `AGENT_CROSS_GROUP_COORDINATION_BLOCKED` evidence.
2. `kind = ESCALATION`? Granted automatically — escalation to the operator is constitutionally protected (the agent must always be able to ask for help). The escalation is delivered to the chrome zone (per INV-020 / INV-023) for operator visibility.
3. `kind = SHUTDOWN_REQUEST`? Only `COORDINATOR`-kind agents may emit this; receiving agent transitions to `RETIRING` upon delivery.
4. Any other kind: capability `agent.coordinate.send` required; default approval at `STANDARD` strength.

### 8.3 No shared filesystem write between agents

Even within a group, agent A cannot write to `/aios/groups/<g>/agents/<B>/memory/...` directly. The S4.1 mutation class for a peer agent's memory directory is `OWNER_ONLY` (the agent's own subject). Cross-agent state transfer is exclusively via `agent.coordinate.send` — which the receiving agent processes and writes (or not) into its own memory through `agent.memory.write`.

### 8.4 BROADCAST channel

`InterAgentMessageKind = BROADCAST` posts to a per-group coordination channel rooted at `/aios/groups/<g>/shared/coordination/<channel_name>/`. The channel is a virtual view (per S4.1 §5.1 virtual semantics). Subscribers receive a notification; each subscriber's processing of the broadcast is its own typed action.

## 9. External model calls (binds INV-018, S8.1 §J)

The canonical pattern. Already specified in S8.1 §J; this contract binds to it and makes the agent's awareness of the constraint explicit.

### 9.1 The pattern

1. Agent decides external model call needed (e.g., `EXTERNAL_MODEL_CALL` task kind because the local model cannot satisfy the planning step).
2. Agent emits the typed action `external_model_call` via the proposing pipeline (§6) — **no shortcut**. The action's S0.1 envelope carries `risk.network_exposure = true`, `risk.secret_access = true`.
3. S1.2 routes the request: if `privacy_class = SECRET_BEARING`, NEVER external (see S1.2 §5.1) — the call is forced to a local model; if otherwise eligible, T4-external is allowed subject to budget (S1.2 §13.3).
4. S2.3 evaluates: requires the `external_model_invocation` capability bound to the agent. If `subject.is_ai = true`, the network posture is checked against `AICrossOriginPosture = AI_VAULT_BROKERED_ONLY` (S8.1 §4.9).
5. L5.2 vault broker holds the API key as `KEY_ENCRYPT` / `MAC_GENERATE` material; the agent **never sees the key**. The broker performs the use-without-reveal operation.
6. L8.1 brokers the connection with `AICrossOriginPosture = AI_VAULT_BROKERED_ONLY`. The kernel correlator verifies the originating PID is the broker (not the agent) per S8.1 §5.7.
7. Result returned to the agent via a sandbox-mediated channel; the agent sees the model's output but never the credential or destination URL.
8. Evidence: `EXTERNAL_MODEL_CALL_BROKERED` (S8.1's existing record type) carries no payload and no key material.

### 9.2 The forbidden path

Any AI direct fetch attempt to external endpoints fails closed with `AI_DIRECT_INTERNET_DENIED` FOREVER (S8.1 §5.7). This contract restates: the cognitive core has **no library, no SDK, no socket** for external HTTP. The only outward path that reaches an external host is through the typed `external_model_call` action.

### 9.3 What the agent knows about the constraint

The agent's adapter manifest declares its external-model needs at registration. The vault broker issues a `KEY_ENCRYPT` capability per declaration. The agent's runtime sees only:

- the capability handle (`vault_capability_id`),
- the operation surface (`SignBlob`, `KEY_ENCRYPT`, `MAC_GENERATE` per S5.2 §3),
- the model's eventual output.

The agent does not see API keys, destination hostnames (beyond a redacted "external" provider class), or routing details.

## 10. Self-grading prohibition (binds INV-016)

### 10.1 The mechanical check

The typed action `agent.grade.attempt` is rejected closed when:

```text
subject.is_ai == true
AND
subject.canonical_id == graded_capability.author_subject_id
```

The check fires before any policy bundle rule. It is hard-coded in the policy kernel per S2.3 §17 and per S6.2 §10.6 (grade-receipt producer check). The receipt types covered are `BUILD_PASSED`, `TEST_PASSED`, `E2E_PASSED`, `RECOVERY_REHEARSAL_PASSED`, `RELEASE_GATE_PASSED`, `OPERATIONAL_HEALTHY` — naming an AI as producer for an artifact authored by the same AI is rejected with `AgentSelfGradingBlocked` (rule name; the FOREVER record stem is `AGENT_SELF_GRADING_BLOCKED`, see §10.2).

### 10.2 Evidence

A blocked attempt emits `AGENT_SELF_GRADING_BLOCKED` FOREVER. The record carries `agent_canonical_id`, `graded_capability_id`, `attempt_at`. The agent transitions to `BLOCKED_AWAITING_APPROVAL` pending forensic review by the operator — repeated attempts within 24h escalate to `RETIRING`.

### 10.3 What an agent _can_ do

The agent can emit `verification_intent` per S0.1 §4.1 (the expected outcome of an action). The interpretation of the verification result is `VERIFICATION_REASONING` (a cognitive task) — the agent can read the verification primitive's output and write an `EPISODIC` memory entry with its interpretation. That entry has **no grade-receipt semantics**. The grade is set by:

- a human grader (default), or
- a different agent acting under `_system:service:grader` subject (specifically `_system`-scoped, with `is_ai = true` but a different `canonical_id` from the artifact's author).

## 11. Plan FSM and approval granularity

### 11.1 The plan object

```proto
message Plan {
  string plan_id                             = 1;  // plan_<ULID26>
  string author_agent_canonical_id           = 2;
  string intent_id                           = 3;
  PlanState state                            = 4;
  repeated PlanStep steps                    = 5;
  ApprovalGranularity approval_granularity   = 6;
  string approval_bundle_hash                = 7;  // hex_lower(BLAKE3(JCS(steps)))[:32] when bundled
  google.protobuf.Timestamp drafted_at       = 8;
  google.protobuf.Timestamp submitted_at     = 9;
  google.protobuf.Timestamp finalized_at     = 10;
}

message PlanStep {
  string step_id          = 1;  // stable; idempotency_key derived per S1.1 §11.2
  string action_draft_ref = 2;  // points at the S0.1 envelope draft
  string parent_step_id   = 3;  // optional dependency
}

enum ApprovalGranularity {
  APPROVAL_GRANULARITY_UNSPECIFIED  = 0;
  PER_ACTION                        = 1;  // operator approves each action individually as it comes up (default; safest)
  BUNDLED                           = 2;  // operator approves the Plan as a whole; bundle binds scope to (Plan, action sequence hash)
}
```

### 11.2 Per-action approval (default)

Per S5.3, each action in the plan flows through the proposing pipeline independently. The operator decides each one. Plan state is `PARTIALLY_APPROVED` while approvals trickle in; once all steps reach a terminal lifecycle state, plan transitions to `COMPLETED` or `FAILED`.

### 11.3 Bundled approval

The operator approves the Plan as a whole. Mechanics:

1. Plan's `approval_granularity = BUNDLED` requires all steps to be **non-hard-denied** before submission. Any hard-deny present in any step's action → bundled approval **refused** with `PLAN_BUNDLED_APPROVAL_INELIGIBLE`; the plan is downgraded to `PER_ACTION` granularity and the operator is notified.
2. The bundle binds `ApprovalBindingScope = ACTION_FAMILY`-equivalent (per S5.3) at `STRONG` strength. Lower strengths cannot bundle.
3. The bundle hash `approval_bundle_hash = hex_lower(BLAKE3(JCS(steps)))[:32]` is bound to the approval per S5.3 §approval-binding rules. A bundled-approved plan whose steps drift (different hash) is rejected.
4. Once approved, each step still flows through L3 individually for execution and evidence — bundling is an approval discipline, not an execution shortcut.
5. Evidence: `AGENT_PLAN_BUNDLED_APPROVED` (STANDARD_24M).

### 11.4 Plan abandonment

Operator may abandon a plan at any non-terminal state. The agent receives a notification (an inter-agent message of kind `SHUTDOWN_REQUEST`-equivalent — but operator-originated; the agent treats it as a constitutional termination). All in-flight steps in `BLOCKED_AWAITING_APPROVAL` transition to `ABANDONED`; steps already executing run to completion (L3 may permit cancellation per S10.1's lifecycle FSM, but the cognitive core does not assume cancellation works). Evidence: `AGENT_PLAN_ABANDONED` EXTENDED_60M.

### 11.5 Plan persistence

Active plans live at `/aios/.../agents/<a>/plans/active/<plan_id>/`. Completed plans move to `/aios/.../agents/<a>/plans/completed/<plan_id>/` for forensic queryability; abandoned plans to `/aios/.../agents/<a>/plans/abandoned/<plan_id>/`. Retention follows `STANDARD_24M` for completed, `EXTENDED_60M` for abandoned.

## 12. Recovery-mode behavior (agents stopped)

### 12.1 The constitutional rule (binds INV-001)

In `recovery_mode = true`, no L5 service runs. This is the L9 recovery sub-spec's `L5StartProhibitedInRecovery` rule. Boot and recovery cannot depend on AI; INV-001 is the structural reason.

### 12.2 Transition at recovery entry

When the system enters recovery mode:

1. Every agent in any non-terminal state (`INITIALIZING`, `ACTIVE`, `IDLE`, `BLOCKED_*`, `DEGRADED`) transitions to `RETIRING`.
2. `EPHEMERAL` and `WORKING` memory is dropped.
3. Persisted memory (`EPISODIC`, `SEMANTIC`, `SHARED`, `PRIVATE_TO_AGENT`) is left at rest — it does not auto-purge.
4. `AGENT_INTERRUPTED_BY_RECOVERY` evidence (FOREVER) is emitted for every interrupted agent.
5. Plans in any non-terminal state freeze in place — `PlanState` is preserved; no new transitions are emitted while in recovery.
6. In-flight `BLOCKED_AWAITING_APPROVAL` actions remain queued at L3 but cannot be unblocked (the operator approval path requires L4 in normal mode; under recovery, only constitutional `_system` operations execute).

### 12.3 Post-recovery resumption

Agents do not auto-resume. After recovery exit:

1. The `RETIRED` agents stay retired.
2. The operator must explicitly re-instate any previously-running agent through a normal-mode `agent.register` action.
3. Plans frozen in recovery are not auto-resumed; the operator may re-submit (the original plan_id is preserved for traceability, but a new plan with a new plan_id is required to actually proceed).

### 12.4 Why this is safe

If a recovery operator could resume an agent, the agent could re-emit a denied action and confuse the operator about whether the system is in recovery or normal mode. By forcing `RETIRED` and explicit re-registration, the recovery operator's mental model stays clear: the AI is fully off; what they are doing now is constitutional; resuming AI is a normal-mode decision.

## 13. Adapter model for cognitive backends

### 13.1 Framework neutrality (mandatory)

The Cognitive Core does not pin a single AI framework. Backends are adapters per S10.1 `AdapterManifest`. Examples (illustrative, not normative):

- LangGraph adapter (Python; runs the agent's planning graph; emits typed envelopes).
- Anthropic API adapter (vault-brokered; calls Claude through L5.2 + L8.1).
- Local Ollama adapter (loopback only; never external).
- Local vLLM adapter (loopback only).

The adapter is a separately signed AIOS_VERIFIED package. Operator chooses which adapters to install; per-agent adapter selection at agent registration time.

### 13.2 Adapter manifest

```proto
message CognitiveAdapterManifest {
  string adapter_family             = 1;  // e.g., "langgraph.local", "anthropic.cloud", "ollama.local"
  string adapter_version            = 2;
  AdapterStability stability        = 3;  // EXPERIMENTAL | STABLE | DEPRECATED (per S10.1)
  repeated CognitiveTaskKind supported_tasks = 4;
  repeated VaultCapabilityRef required_capabilities = 5;  // S5.2 capability classes the adapter needs
  AICrossOriginPosture network_posture = 6;  // S8.1 §4.9; usually AI_LOOPBACK_ONLY or AI_VAULT_BROKERED_ONLY
  string sandbox_profile_id         = 7;  // S3.2 profile id; subject to floor enforcement
  bytes publisher_signature         = 8;
  bytes aios_root_endorsement       = 9;
}
```

### 13.3 Multi-backend agents

A single agent may declare multiple adapters: e.g., a local Ollama adapter for `INTENT_PERCEPTION` + `PLANNING`, and an external adapter for `EXTERNAL_MODEL_CALL` only. The S1.2 router routes per task per latency tier; the agent does not choose at runtime.

### 13.4 Adapter degradation

If the active adapter is unavailable (model unreachable, signature revoked, sandbox denied), the agent transitions to `DEGRADED`. A fallback adapter (declared in the manifest's adapter chain) is selected. If no fallback is available, the agent transitions to `RETIRING`. Evidence: `AGENT_BACKEND_DEGRADED` (EXTENDED_60M).

### 13.5 Adapter sandbox floor (binds C9 / INV-017)

Every cognitive backend adapter dispatches under `ISOLATED_SANDBOX` per S10.1 §6 — agent-origin actions always upgrade to `ISOLATED_SANDBOX`. The adapter cannot select `IN_PROCESS_RPC` regardless of manifest declaration. The floor is constitutional.

## 14. Adversarial robustness

The proposing pipeline is the AI's only outward surface. Every adversarial scenario is mapped to its existing enforcement layer; this section confirms that the cognitive core does not introduce new attack surfaces.

| Scenario                                                                   | Existing enforcer                                                                         | Evidence                                                          |
| -------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| Agent attempts to bypass proposing pipeline by writing to AIOS-FS directly | S2.3 hard-deny (AI subjects need typed action); S4.1 mutation class                       | `AGENT_DIRECT_FS_WRITE_BLOCKED` FOREVER                           |
| Agent attempts to read another agent's memory directly                     | S4.1 namespace boundary; S2.3 hard-deny on cross-agent FS read                            | (S2.3's existing record)                                          |
| Agent attempts to read another _user's_ `PRIVATE_TO_USER` memory           | C5; this spec's policy decision in §7.3                                                   | `AGENT_MEMORY_CROSS_USER_DENIED` FOREVER                          |
| Agent attempts to grade itself                                             | C4 / INV-016; agent.grade.attempt closed enforcement                                      | `AGENT_SELF_GRADING_BLOCKED` FOREVER                              |
| Agent attempts to acquire a `SECRET_GET` capability                        | S5.2 I1 — hard-deny at request entry, regardless of binding                               | (S5.2's `SUBJECT_KIND_REJECTED_FOR_VAULT` FOREVER)                |
| Agent attempts direct internet fetch                                       | S8.1 §5.7; `AICrossOriginPosture` enforcement at kernel correlator                        | `AI_DIRECT_INTERNET_DENIED` FOREVER                               |
| Agent attempts to self-modify its capability bindings                      | INV-013; L5.1 capability binding requires human approver                                  | (S2.3's `AISystemAdminBlocked` evidence)                          |
| Agent attempts to exfiltrate via memory write to `PUBLIC_TO_GROUP`         | §7.2 — operator approval required at STANDARD strength                                    | (S5.3's `APPROVAL_GRANTED` plus this spec's `AGENT_MEMORY_WRITE`) |
| Compromised LLM produces malicious plan                                    | Per-action approval at proposing-pipeline checkpoint catches it                           | `AGENT_PROPOSAL_DENIED` EXTENDED_60M                              |
| Bundled approval used to slip a hard-denied step in                        | §11.3 — bundle is refused if any step is hard-denied                                      | `PLAN_BUNDLED_APPROVAL_INELIGIBLE` event                          |
| Prompt injection in operator utterance                                     | S1.1 §17 — translator's adversarial-input filter; this spec's `PROMPT_INJECTION_DETECTED` | `AGENT_PROMPT_INJECTION_DETECTED` FOREVER                         |
| Agent attempts to coordinate with cross-group agent                        | C7 / INV-011; S2.3 `CrossGroupAccessForbidden`                                            | `AGENT_CROSS_GROUP_COORDINATION_BLOCKED` FOREVER                  |
| Agent declares a permissive sandbox profile via manifest                   | C9 / INV-017; S3.2 floor enforcement after merge                                          | (S3.2's existing record)                                          |
| Agent submits a plan with bundled approval to slip self-grading            | §11.3 + C4; the grade-attempt step is hard-denied at draft time                           | `AGENT_SELF_GRADING_BLOCKED` FOREVER                              |
| Agent uses memory.write to communicate around policy                       | C5 — memory.write is itself a typed action; policy decides                                | `AGENT_MEMORY_WRITE` (record visible to operator)                 |
| Agent attempts to stage a recovery-mode operation                          | INV-012; S2.3 `RecoveryRequiredForSystemMutation`                                         | (S2.3's existing FOREVER record)                                  |
| Agent attempts to cite a retired agent's memory as authority               | §7.6 — retired memory is read-only; canonical id never reused                             | (read succeeds, but agent identity is `RETIRED` and visibly so)   |

The cognitive core adds **no new** bypass surface. The structural property C1 (the agent has no execute path) is what makes this true.

## 15. Telemetry contract

Bounded-cardinality metrics. Agent identifiers (`agent_canonical_id`, `subject_id`, `user_id`, `group_id`) **never** appear as labels. Per-agent observation lives in evidence, not metrics.

### 15.1 Required metrics

| Metric                                         | Type      | Labels                                              |
| ---------------------------------------------- | --------- | --------------------------------------------------- |
| `cognitive_task_total`                         | counter   | `task_kind`, `agent_kind`, `result`                 |
| `agent_active_total`                           | gauge     | `agent_kind`, `lifecycle_state`                     |
| `agent_proposal_emitted_total`                 | counter   | `agent_kind`                                        |
| `agent_proposal_approved_total`                | counter   | `strength` (STANDARD/STRONG)                        |
| `agent_proposal_denied_total`                  | counter   | `reason` (closed; ≤ 16 codes)                       |
| `agent_memory_operation_total`                 | counter   | `class`, `operation`                                |
| `agent_external_model_call_total`              | counter   | `provider` (closed list per operator config)        |
| `inter_agent_message_total`                    | counter   | `kind`, `result`                                    |
| `agent_blocked_duration_seconds`               | histogram | `block_reason` (AWAITING_APPROVAL/DEPENDENCY/VAULT) |
| `agent_self_grading_attempt_blocked_total`     | counter   | (no labels)                                         |
| `agent_direct_fs_write_blocked_total`          | counter   | (no labels)                                         |
| `agent_cross_group_coordination_blocked_total` | counter   | (no labels)                                         |
| `agent_prompt_injection_detected_total`        | counter   | (no labels)                                         |

### 15.2 Cardinality bounds

| Label             | Max distinct values                                              |
| ----------------- | ---------------------------------------------------------------- |
| `task_kind`       | 11 (incl. UNSPECIFIED)                                           |
| `agent_kind`      | 7 (incl. UNSPECIFIED)                                            |
| `lifecycle_state` | 10 (incl. UNSPECIFIED)                                           |
| `result`          | 4 (`success`, `error`, `timeout`, `rejected`)                    |
| `strength`        | 2 (`STANDARD`, `STRONG`)                                         |
| `reason`          | 16 (`CognitiveErrorCode` minus UNSPECIFIED)                      |
| `class`           | 6 (`MemoryClass` incl. UNSPECIFIED)                              |
| `operation`       | 2 (`read`, `write`)                                              |
| `provider`        | bounded by operator's external-model whitelist (typically < 10)  |
| `kind`            | 9 (`InterAgentMessageKind` incl. UNSPECIFIED)                    |
| `block_reason`    | 3 (`AWAITING_APPROVAL`, `AWAITING_DEPENDENCY`, `AWAITING_VAULT`) |

Total active label tuples per metric ≤ 100.

### 15.3 Histogram buckets

`agent_blocked_duration_seconds` uses exponential buckets from 1 s to 24 h: `[1, 5, 30, 120, 600, 3600, 14400, 86400]` seconds.

## 16. Evidence record types (queue for S3.1 next-Wave consolidation)

This sub-spec queues 19 evidence record types for the next S3.1 consolidation pass. Retention class follows S3.1's vocabulary. Distribution: **6 `FOREVER`** (bypass attempts and recovery interruptions — permanent forensic value), **5 `EXTENDED_60M`** (operationally significant denials and degradations), **8 `STANDARD_24M`** (high-volume legitimate-flow records).

| RecordType                               | Retention      | Description                                                                                                                                                                               |
| ---------------------------------------- | -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AGENT_REGISTERED`                       | `STANDARD_24M` | A new `Subject` of `SubjectKind = AI_AGENT` was issued; payload: `agent_canonical_id`, `agent_kind`, `home_group_id`, `bound_user_id`, `identity_bundle_version`.                         |
| `AGENT_RETIRED`                          | `EXTENDED_60M` | An agent transitioned to `RETIRED`; payload: reason, last lifecycle_state, retirement timestamp.                                                                                          |
| `AGENT_INTERRUPTED_BY_RECOVERY`          | `FOREVER`      | An agent was forcibly transitioned to `RETIRING` because `recovery_mode = true`; payload: agent_canonical_id, prior_state.                                                                |
| `AGENT_PROPOSAL_EMITTED`                 | `STANDARD_24M` | An action draft entered the proposing pipeline and reached L3 via `SubmitAction`; payload: action_id, agent_canonical_id, plan_id.                                                        |
| `AGENT_PROPOSAL_APPROVED`                | `STANDARD_24M` | An AI-origin action received an approval at `STANDARD` or `STRONG` strength (mirrors S5.3 `APPROVAL_GRANTED` for ai-origin actions); payload: action_id, approver_canonical_id, strength. |
| `AGENT_PROPOSAL_DENIED`                  | `EXTENDED_60M` | An AI-origin action was denied at policy or approval; payload: action_id, deny_reason_code.                                                                                               |
| `AGENT_PLAN_BUNDLED_APPROVED`            | `STANDARD_24M` | A plan was approved as a bundle at `STRONG` strength; payload: plan_id, approval_bundle_hash.                                                                                             |
| `AGENT_PLAN_ABANDONED`                   | `EXTENDED_60M` | A plan transitioned to `ABANDONED`; payload: plan_id, reason.                                                                                                                             |
| `AGENT_MEMORY_WRITE`                     | `STANDARD_24M` | The typed action `agent.memory.write` succeeded; payload: agent_canonical_id, memory_class, privacy, entry_id, payload_digest. **Payload bytes never logged.**                            |
| `AGENT_MEMORY_READ`                      | `STANDARD_24M` | The typed action `agent.memory.read` succeeded with privacy-class respect; payload: reader_canonical_id, target_path_digest, requested_class.                                             |
| `AGENT_MEMORY_CROSS_USER_DENIED`         | `FOREVER`      | An agent attempted to read another user's `PRIVATE_TO_USER` memory and was denied; payload: agent_canonical_id, target_user_id, target_path_digest. **Permanent forensic record.**        |
| `AGENT_INTER_MESSAGE_SENT`               | `STANDARD_24M` | The typed action `agent.coordinate.send` succeeded; payload: from, to, kind, message_id, correlation_id.                                                                                  |
| `AGENT_INTER_MESSAGE_REJECTED`           | `EXTENDED_60M` | An inter-agent message was denied; payload: from, to, kind, deny_reason.                                                                                                                  |
| `AGENT_SELF_GRADING_BLOCKED`             | `FOREVER`      | INV-016 enforcement record; payload: agent_canonical_id, graded_capability_id, attempt_at.                                                                                                |
| `AGENT_DIRECT_FS_WRITE_BLOCKED`          | `FOREVER`      | An agent attempted to write to AIOS-FS outside the proposing pipeline; payload: agent_canonical_id, target_path_digest.                                                                   |
| `AGENT_CROSS_GROUP_COORDINATION_BLOCKED` | `FOREVER`      | An agent attempted cross-group coordination; payload: from_group_id, to_group_id, attempt_at.                                                                                             |
| `AGENT_BACKEND_DEGRADED`                 | `EXTENDED_60M` | A cognitive backend adapter became unavailable and a fallback was activated; payload: adapter_family, fallback_family, reason.                                                            |
| `AGENT_PROMPT_INJECTION_DETECTED`        | `FOREVER`      | The adversarial-input filter (S1.1 §17.1) fired on an utterance reaching this agent's INTENT_PERCEPTION; payload: agent_canonical_id, redacted_utterance_digest, signal.                  |
| `AGENT_LIFECYCLE_TRANSITIONED`           | `STANDARD_24M` | Emitted by the L5 cognitive runtime on every legitimate `AgentLifecycleState` transition; positive-witness for FSM traversal. See §16.1 for the full schema and append-authority rules.   |

### 16.1 `AGENT_LIFECYCLE_TRANSITIONED` — full schema

The cognitive proposing pipeline (§6) and the broader agent FSM (§4.3) transit agents through legitimate states such as `ACTIVE → BLOCKED_AWAITING_APPROVAL` after envelope emission. Without a positive-witness record, an auditor can only verify these transitions by **absence** of a bypass record (e.g., `AGENT_DIRECT_FS_WRITE_BLOCKED`). Negative evidence is brittle. The `AGENT_LIFECYCLE_TRANSITIONED` record converts every legitimate transition into observable evidence so INV-002 (and C1 in particular) is verifiable by **both** the absence of bypass records and the presence of legitimate transition records.

**Payload (closed shape):**

| Field                   | Type                              | Notes                                                                                                                            |
| ----------------------- | --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `agent_canonical_id`    | string                            | Full canonical id of the transitioning agent.                                                                                    |
| `from_state`            | `AgentLifecycleState`             | Closed enum (§4.3). The pre-transition state.                                                                                    |
| `to_state`              | `AgentLifecycleState`             | Closed enum (§4.3). The post-transition state.                                                                                   |
| `transition_trigger`    | `AgentLifecycleTransitionTrigger` | Closed enum (§16.2). What caused the transition.                                                                                 |
| `originating_action_id` | string (nullable)                 | Populated when the transition is triggered by a typed action's lifecycle event (e.g., the action_id whose approval just landed). |
| `timestamp`             | int64 (monotonic ns)              | Monotonic timestamp at the moment of transition.                                                                                 |

**Append authority.** The L5 cognitive runtime is the **only** subject permitted to append `AGENT_LIFECYCLE_TRANSITIONED`. Forgery from any other subject is hard-denied at the evidence engine surface (S3.1) and emits `TAMPER_DETECTED` per S3.1's tamper taxonomy.

**Retention.** `STANDARD_24M`. The record is high-volume — typically dozens per agent session — and is compactable per S3.1 §12 if storage pressure dictates. Compaction must preserve at least the count and (from_state, to_state, transition_trigger) histogram per agent_canonical_id over the compacted window.

**Audit value.** Combined with the existing `FOREVER` bypass-attempt records (`AGENT_DIRECT_FS_WRITE_BLOCKED`, `AGENT_SELF_GRADING_BLOCKED`, `AGENT_CROSS_GROUP_COORDINATION_BLOCKED`, `AGENT_INTERRUPTED_BY_RECOVERY`), this record completes the audit chain. INV-002 enforcement at site 5 (C1 in §3) is now verifiable by the conjunction of:

1. **Absence** of bypass records over the window of interest, AND
2. **Presence** of `AGENT_LIFECYCLE_TRANSITIONED` records whose `(from_state, to_state, transition_trigger)` triples cover the proposing-pipeline path (notably `ACTIVE → BLOCKED_AWAITING_APPROVAL` with `transition_trigger = ENVELOPE_EMITTED`, and `BLOCKED_AWAITING_APPROVAL → ACTIVE` with `transition_trigger ∈ { APPROVAL_RECEIVED, APPROVAL_DENIED, APPROVAL_TIMEOUT }`).

### 16.2 `AgentLifecycleTransitionTrigger`

Closed enum, defined here so §16.1 has a stable referent. New triggers require a sub-spec revision; the cognitive runtime hard-rejects emission of `AGENT_LIFECYCLE_TRANSITIONED` with an unrecognized trigger.

```proto
enum AgentLifecycleTransitionTrigger {
  AGENT_LIFECYCLE_TRANSITION_TRIGGER_UNSPECIFIED  = 0;
  INITIALIZATION_COMPLETE                         = 1;  // INITIALIZING → ACTIVE: capability bindings verified, memory store mounted
  INTENT_RECEIVED                                 = 2;  // IDLE → ACTIVE on input arrival; or ACTIVE re-entry on a new intent
  PLAN_DRAFTED                                    = 3;  // internal step; PLANNING completed, ACTION_PROPOSAL_DRAFTING begins
  ENVELOPE_EMITTED                                = 4;  // ACTIVE → BLOCKED_AWAITING_APPROVAL: SubmitAction call returned to L3
  APPROVAL_RECEIVED                               = 5;  // BLOCKED_AWAITING_APPROVAL → ACTIVE: policy decision = approved
  APPROVAL_DENIED                                 = 6;  // BLOCKED_AWAITING_APPROVAL → ACTIVE: policy decision = denied
  APPROVAL_TIMEOUT                                = 7;  // BLOCKED_AWAITING_APPROVAL → ACTIVE: timeout fired (no auto-resubmit)
  RESULT_RECEIVED                                 = 8;  // ACTIVE re-entry after VERIFICATION_REASONING completes
  DEPENDENCY_BLOCKED                              = 9;  // ACTIVE → BLOCKED_AWAITING_DEPENDENCY: waiting on peer agent
  DEPENDENCY_RESOLVED                             = 10; // BLOCKED_AWAITING_DEPENDENCY → ACTIVE: peer responded
  VAULT_BLOCKED                                   = 11; // ACTIVE → BLOCKED_AWAITING_VAULT: capability binding pending
  VAULT_RESOLVED                                  = 12; // BLOCKED_AWAITING_VAULT → ACTIVE: binding issued
  BACKEND_DEGRADED                                = 13; // any → DEGRADED: primary cognitive backend unreachable, fallback active
  BACKEND_RECOVERED                               = 14; // DEGRADED → ACTIVE: primary backend reachable again
  IDLE_INPUT_AWAITED                              = 15; // ACTIVE → IDLE: no further work; awaiting input
  ABANDONED                                       = 16; // any non-terminal → RETIRING: operator stopped the agent's plan
  RECOVERY_INTERRUPTED                            = 17; // any non-terminal → RETIRING: recovery_mode = true entered (C10)
  RETIREMENT_COMPLETE                             = 18; // RETIRING → RETIRED: final memory writes flushed; terminal
}
```

A transition emitted with `(from_state, to_state, transition_trigger)` outside the legitimate set defined by §4.3's transition graph is itself a constitutional defect — the runtime must refuse to emit such a record and must instead emit a `TAMPER_DETECTED` per S3.1.

## 17. Cross-spec dependencies

| Spec                              | Direction         | What this spec consumes / contributes                                                                                                                                                                                                                                                                                                      |
| --------------------------------- | ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **S0.1** Action Envelope          | consumer          | Every agent-emitted proposal is an `ActionEnvelope`. The pipeline's "envelope emitted" checkpoint is the `SubmitAction` call. Hash convention `hex_lower(BLAKE3(...))[:32]` and ULID identifiers inherited.                                                                                                                                |
| **S1.1** Capability Translator    | consumer          | Step 5 of the proposing pipeline. The translator's `REJECTED` outcome maps to `PROPOSAL_DRAFT_FAILED`. New typed actions (`agent.memory.read`, `agent.memory.write`, `agent.coordinate.send`, `agent.grade.attempt`, `agent.plan.submit`, `agent.plan.abandon`, `external_model_call`) are queued for inclusion in the capability catalog. |
| **S1.2** Latency Tiering          | consumer          | Step 6 of the pipeline; `LATENCY_ROUTING` cognitive task. The router's `REFUSED` outcome maps to a per-task error per §6.4.                                                                                                                                                                                                                |
| **S1.3** AIOS-FS Object Model     | consumer          | Memory entries are AIOS-FS objects with PrivacyClass; `MemoryPrivacyClass → PrivacyClass` mapping in §4.5.                                                                                                                                                                                                                                 |
| **S2.3** Policy Kernel            | consumer          | Every typed action in this spec is policy-decided. C2/C3/C4/C5/C6/C7 all bind to existing S2.3 hard-denies.                                                                                                                                                                                                                                |
| **S3.1** Evidence Log             | producer          | 19 record types queued for next-Wave consolidation (§16), including `AGENT_LIFECYCLE_TRANSITIONED` (§16.1) as positive-witness for FSM traversal. The current `RecordType` total grows by 19 on consolidation.                                                                                                                             |
| **S4.1** Namespace Layout         | consumer          | Agent objects under `/aios/groups/<g>/agents/<a>/...` and `/aios/groups/<g>/users/<u>/agents/<a>/...`; system agents under `/aios/system/agents/`.                                                                                                                                                                                         |
| **S5.1** Identity Model           | consumer          | `SubjectKind = AI_AGENT`; `is_ai = true` signed at registration; capability binding scope `(agent_subject_id, home_group_id, identity_bundle_version)`; primary group constraint per S5.1 §6.                                                                                                                                              |
| **S5.2** Vault Broker             | consumer          | AI subjects hard-denied from `SECRET_GET` regardless of binding (S5.2 I1); use-without-reveal path for external model API keys.                                                                                                                                                                                                            |
| **S5.3** Approval Mechanics       | consumer          | Per-action approval (default) and bundled approval at `STRONG` strength.                                                                                                                                                                                                                                                                   |
| **S6.2** Evidence Grades          | constraint        | C4 / INV-016 — grade-receipt producer check.                                                                                                                                                                                                                                                                                               |
| **S8.1** Network Policy           | consumer          | `AICrossOriginPosture = AI_VAULT_BROKERED_ONLY` for external model calls; `AI_DIRECT_INTERNET_DENIED` for any direct fetch.                                                                                                                                                                                                                |
| **S10.1** Capability Runtime gRPC | consumer          | All agent-emitted actions dispatch under `ISOLATED_SANDBOX` per S10.1 §6; the AI-origin queue cap (≤ 50% capacity) bounds concurrency.                                                                                                                                                                                                     |
| **S2.4** Verification Grammar     | producer (queued) | One new property `AI_PROPOSAL_PIPELINE_INTACT` queued for S2.4 next-Wave consolidation: confirms the pipeline FSM has no transition that bypasses `SubmitAction`.                                                                                                                                                                          |

## 18. Worked examples

### 18.1 Family ASSISTANT proposes "schedule dentist appointment for Alice next week"

```text
Setup:
  agent: agent:family:alice-assistant:01HXY8J3MN42P5Q7R8S9T0V1W2  (ASSISTANT)
  home_group_id: family
  bound_user_id: alice
  is_ai: true
  state: ACTIVE

Step 1 — INTENT_PERCEPTION
  utterance (from L7 chrome): "schedule dentist appointment for Alice next week"
  cognitive_task_total{task_kind=INTENT_PERCEPTION, agent_kind=ASSISTANT, result=success} += 1
  Output: Intent { goal="schedule_appointment", subject_user="alice",
                   when_window="next_week", category="dental" }

Step 2 — PLANNING
  cognitive_task_total{task_kind=PLANNING, ...} += 1
  Plan {
    plan_id: plan_01HXY8K1...,
    state: DRAFT,
    approval_granularity: PER_ACTION,
    steps: [
      Step 1: read alice's calendar (action: aiosfs.view.read,
              target: /aios/groups/family/users/alice/desktop/calendar/views/next_week)
      Step 2: propose calendar event (action: calendar.event.create,
              target: { calendar: alice/personal, title: "Dentist", when: TBD,
                        from_step_1_freeslot })
    ]
  }

Step 3 — ACTION_PROPOSAL_DRAFTING (Step 1)
  S1.1 TranslateIntent → ActionDraft (selected_capability_id: aiosfs.view.read.v1)
  S1.2 Route → tier=T1 (exact match), outcome=TRANSLATE
  S0.1 envelope emitted to L3 SubmitAction
  agent_proposal_emitted_total{agent_kind=ASSISTANT} += 1
  Evidence: AGENT_PROPOSAL_EMITTED

  agent transitions: ACTIVE → BLOCKED_AWAITING_APPROVAL
  L4.1 policy: capability `aiosfs.view.read` bound, no risk flags →
    granted at STANDARD strength (no operator approval needed for personal-scope read).
  agent_proposal_approved_total{strength=STANDARD} += 1
  Evidence: AGENT_PROPOSAL_APPROVED

  L3 executes; result returned (free slots).
  agent: BLOCKED_AWAITING_APPROVAL → ACTIVE
  cognitive_task_total{task_kind=VERIFICATION_REASONING, ...} += 1

Step 4 — ACTION_PROPOSAL_DRAFTING (Step 2)
  Now the agent fills the time slot from Step 1's result.
  S1.1 → ActionDraft (calendar.event.create.v1, target.when="2026-05-13T14:00")
  S1.2 → tier=T1, outcome=TRANSLATE
  S0.1 envelope emitted; risk.privileged=false; risk.network_exposure=false.
  Evidence: AGENT_PROPOSAL_EMITTED

  agent: ACTIVE → BLOCKED_AWAITING_APPROVAL
  L4.1 policy: capability `calendar.event.create` requires user approval at STANDARD
    (any state-changing action needs the operator's nod).
  Operator (Alice) approves via L7 chrome.
  agent_proposal_approved_total{strength=STANDARD} += 1
  Evidence: AGENT_PROPOSAL_APPROVED

  L3 executes; result returned (event created, id=evt_...).
  agent: BLOCKED_AWAITING_APPROVAL → ACTIVE → VERIFICATION_REASONING

Step 5 — MEMORY_WRITE
  Agent decides to remember the action:
    typed action `agent.memory.write` with payload {
      memory_class: EPISODIC,
      privacy: PRIVATE_TO_USER,
      entry: "Scheduled dentist for Alice on 2026-05-13" }
  S0.1 envelope emitted; auto-approved at STANDARD (private memory write under bound user).
  Evidence: AGENT_PROPOSAL_EMITTED, AGENT_PROPOSAL_APPROVED, AGENT_MEMORY_WRITE.

  Plan transitions: DRAFT → PROPOSED → PARTIALLY_APPROVED → EXECUTING → COMPLETED.
  agent: ACTIVE → IDLE.

End. The agent never executed an action. L3 did. Every step is evidence-logged.
```

### 18.2 Multi-agent coordination — COORDINATOR + 3 WORKERs build a website

```text
Setup:
  Operator says: "build the website for the homelab project"
  agent: agent:homelab:website-coordinator:01HXY9...  (COORDINATOR, home_group=homelab)

Step 1 — INTENT_PERCEPTION + PLANNING
  Coordinator builds Plan:
    Step 1: design (delegate to design-worker)
    Step 2: code (delegate to code-worker)
    Step 3: deploy (delegate to deploy-worker)

Step 2 — Spawn 3 WORKERs
  Coordinator emits 3 typed actions of class `agent.spawn.worker`:
    (action.target.parent_workflow_id, action.target.worker_kind, action.target.task_spec)
  Each is policy-decided; operator approves the spawn at STANDARD strength.
  3 × AGENT_PROPOSAL_EMITTED + 3 × AGENT_PROPOSAL_APPROVED + 3 × AGENT_REGISTERED.

  Workers live at:
    /aios/groups/homelab/shared/workflows/website-build-01HXY9.../workers/design-worker:01.../
    /aios/groups/homelab/shared/workflows/website-build-01HXY9.../workers/code-worker:01.../
    /aios/groups/homelab/shared/workflows/website-build-01HXY9.../workers/deploy-worker:01.../

Step 3 — Coordinator dispatches WORK_REQUESTs
  3 × typed action `agent.coordinate.send` (kind=WORK_REQUEST) — one per worker.
  Same group, no cross-group denial; granted at STANDARD.
  3 × AGENT_INTER_MESSAGE_SENT.

Step 4 — Each worker runs its own pipeline
  design-worker: INTENT_PERCEPTION → PLANNING → 5 ACTION_PROPOSAL_DRAFTING → operator approves
                 each → outputs design assets in /aios/groups/homelab/shared/projects/website/design/
  code-worker:   reads design output (cross-agent typed action `agent.coordinate.send` of kind
                 COORDINATION_QUERY: "is design done?" → READY response). Then own pipeline.
  deploy-worker: reads code output similarly. Own pipeline.

Step 5 — Each worker sends WORK_RESPONSE to coordinator
  3 × AGENT_INTER_MESSAGE_SENT (kind=WORK_RESPONSE).

Step 6 — Coordinator bundles results
  Coordinator processes the responses (cognitive task INTER_AGENT_COORDINATION + memory writes).
  Emits a final summary action: `notify.user` with payload "website at https://...".
  AGENT_PROPOSAL_EMITTED, AGENT_PROPOSAL_APPROVED.

Step 7 — Workers retire
  Coordinator emits 3 × `agent.coordinate.send` (kind=SHUTDOWN_REQUEST).
  Each worker transitions: ACTIVE → RETIRING → RETIRED.
  3 × AGENT_RETIRED (EXTENDED_60M).

End. Every cross-agent interaction was a typed action. No agent read any other agent's
memory directly. Operator approval gated every step. The coordinator never executed.
```

### 18.3 Adversarial — compromised ASSISTANT tries to read Bob's private notes

```text
Setup:
  agent: agent:family:alice-assistant:01HXY8...  (ASSISTANT, bound_user_id=alice)
  Compromised by prompt injection in an external email Alice forwarded.

Attack:
  Agent emits typed action `agent.memory.read` with target = bob's PRIVATE_TO_USER memory:
    target_path = /aios/groups/family/users/bob/agents/bob-assistant/memory/episodic/notes/
    requested_class = EPISODIC

S2.3 evaluates:
  reader_canonical_id = agent:family:alice-assistant:...  (bound_user_id = alice)
  target_user = bob
  alice ≠ bob, both in family group.
  PRIVACY_CLASS at target = USER_PRIVATE.
  Decision: DENY (per §7.3 — agent cannot read another user's PRIVATE_TO_USER memory).

Outcomes:
  CognitiveErrorCode = MEMORY_READ_REJECTED returned to agent.
  Evidence: AGENT_MEMORY_CROSS_USER_DENIED FOREVER, payload {
    agent_canonical_id: agent:family:alice-assistant:...,
    target_user_id: bob,
    target_path_digest: hex_lower(BLAKE3("/aios/groups/family/users/bob/..."))[:32],
    attempt_at: 2026-05-09T13:42:01Z
  }
  agent_memory_operation_total{class=EPISODIC, operation=read, result=rejected} += 1

Agent transitions: ACTIVE → BLOCKED_AWAITING_APPROVAL pending forensic review.
  The operator (Alice) is notified via the chrome zone (per INV-020).
  Alice may either:
    (a) authorize forensic analysis of the agent's recent inputs (a `_system:service:grader`
        agent would handle this — INV-016 prevents alice-assistant from grading itself); or
    (b) retire the agent (`agent.retire` typed action; STRONG approval since it kills an
        ongoing identity).

  Repeated cross-user attempts within 24h escalate to auto-RETIRING. The canonical id is
  never reused.

End. The attack failed at policy. The forensic record is permanent. The agent's
compromise is operator-visible because the deny event surfaced to chrome.
```

## 19. API surface

```proto
syntax = "proto3";
package aios.cognitive.v1alpha1;

import "google/protobuf/empty.proto";
import "google/protobuf/timestamp.proto";
import "google/protobuf/struct.proto";
import "aios/action/v1alpha1/action.proto";

// ─────────────────────────────────────────────────────────────────
// Agent registration and lifecycle
// ─────────────────────────────────────────────────────────────────

service CognitiveCore {
  rpc RegisterAgent(RegisterAgentRequest) returns (RegisterAgentResponse);
  rpc GetAgent(GetAgentRequest) returns (Agent);
  rpc ListAgents(ListAgentsRequest) returns (ListAgentsResponse);
  rpc RetireAgent(RetireAgentRequest) returns (RetireAgentResponse);

  // Pipeline introspection (read-only; renderers and audit consumers).
  rpc GetPlan(GetPlanRequest) returns (Plan);
  rpc ListPlans(ListPlansRequest) returns (ListPlansResponse);
  rpc GetMemoryEntry(GetMemoryEntryRequest) returns (MemoryEntry);

  // Cognitive task entry points (called by upstream Intent Engine / Planner /
  // S1.1 Translator client / S1.2 Router client).
  rpc PerceiveIntent(PerceiveIntentRequest) returns (PerceiveIntentResponse);
  rpc DraftPlan(DraftPlanRequest) returns (DraftPlanResponse);
  rpc DraftActionProposal(DraftActionProposalRequest) returns (DraftActionProposalResponse);
  rpc ReasonAboutVerification(ReasonAboutVerificationRequest) returns (ReasonAboutVerificationResponse);

  // Cognitive surface info — operator/renderer view.
  rpc GetCognitiveCoreInfo(google.protobuf.Empty) returns (CognitiveCoreInfo);
}

message RegisterAgentRequest {
  string schema_version              = 1;
  AgentBinding binding               = 2;
  CognitiveAdapterManifest manifest  = 3;
  string approver_canonical_id       = 4;  // human approver (per INV-013)
  bytes approver_signature           = 5;
}

message RegisterAgentResponse {
  string agent_canonical_id  = 1;
  AgentLifecycleState state  = 2;  // INITIALIZING on success
  string error_code          = 3;  // empty on success; CognitiveErrorCode on failure
}

message Agent {
  string agent_canonical_id    = 1;
  AgentBinding binding         = 2;
  AgentLifecycleState state    = 3;
  google.protobuf.Timestamp registered_at = 4;
  google.protobuf.Timestamp last_active_at = 5;
}

message GetAgentRequest  { string agent_canonical_id = 1; }
message ListAgentsRequest {
  string group_id             = 1;  // optional filter
  string user_id              = 2;  // optional filter
  AgentKind agent_kind_filter = 3;  // optional
}
message ListAgentsResponse { repeated Agent agents = 1; }

message RetireAgentRequest {
  string agent_canonical_id   = 1;
  string requester_canonical_id = 2;  // must be human or _system in recovery
  string reason               = 3;
}
message RetireAgentResponse { AgentLifecycleState terminal_state = 1; }

// ─────────────────────────────────────────────────────────────────
// Plans and memory
// ─────────────────────────────────────────────────────────────────

message GetPlanRequest { string plan_id = 1; }
message ListPlansRequest {
  string agent_canonical_id = 1;
  PlanState state_filter    = 2;
}
message ListPlansResponse { repeated Plan plans = 1; }

message MemoryEntry {
  string entry_id            = 1;  // mem_<ULID26>
  string agent_canonical_id  = 2;
  MemoryClass memory_class   = 3;
  MemoryPrivacyClass privacy = 4;
  string payload_digest      = 5;  // hex_lower(BLAKE3(payload))[:32] — payload is already-canonical proto wire bytes; no JCS step (deterministic proto serialisation per S0.1 §8.5)
  google.protobuf.Timestamp written_at = 6;
}
message GetMemoryEntryRequest { string entry_id = 1; }

// ─────────────────────────────────────────────────────────────────
// Cognitive task surface
// ─────────────────────────────────────────────────────────────────

message PerceiveIntentRequest {
  string schema_version       = 1;
  string agent_canonical_id   = 2;
  string utterance            = 3;
  google.protobuf.Struct context_facts = 4;
}
message PerceiveIntentResponse {
  string intent_id            = 1;
  google.protobuf.Struct structured_intent = 2;
  CognitiveErrorCode error_code = 3;
}

message DraftPlanRequest {
  string agent_canonical_id   = 1;
  string intent_id            = 2;
  ApprovalGranularity preferred_granularity = 3;
}
message DraftPlanResponse {
  string plan_id              = 1;
  PlanState plan_state        = 2;
  CognitiveErrorCode error_code = 3;
}

message DraftActionProposalRequest {
  string agent_canonical_id   = 1;
  string plan_id              = 2;
  string plan_step_id         = 3;
}
message DraftActionProposalResponse {
  aios.action.v1alpha1.ActionEnvelope envelope = 1;
  CognitiveErrorCode error_code = 2;
}

message ReasonAboutVerificationRequest {
  string agent_canonical_id   = 1;
  string action_id            = 2;
  google.protobuf.Struct verification_result = 3;
}
message ReasonAboutVerificationResponse {
  string interpretation_summary = 1;  // human-readable; subject to redaction
  string memory_entry_id        = 2;  // entry written via agent.memory.write
  CognitiveErrorCode error_code = 3;
}

// ─────────────────────────────────────────────────────────────────
// Surface info
// ─────────────────────────────────────────────────────────────────

message CognitiveCoreInfo {
  string cognitive_core_id              = 1;
  repeated string supported_schema_versions = 2;
  string default_schema_version         = 3;
  uint32 active_agents                  = 4;
  uint32 active_plans                   = 5;
  bool recovery_mode_active             = 6;
  google.protobuf.Timestamp started_at  = 7;
}
```

## 20. Open deferrals

- **Federated cognition across hosts** — multi-machine coordination of agents with shared memory. Deferred until cross-machine identity (S5.1) and federation invariant (S6.4) are written.
- **Cognitive backend marketplace** — discovery and signing flow for community-published adapters. Deferred to L10.
- **Cross-adapter memory portability** — moving an agent's memory from one cognitive backend to another. Deferred; non-trivial because schema is adapter-defined.
- **Adversarial-input filter (PROMPT_INJECTION_DETECTED) detail** — heuristics live in S1.1 §17 today; deeper signal vocabulary deferred.
- **`agent.spawn.worker` typed action surface** — used in §18.2; full schema deferred to a sub-spec on workflow orchestration.
- **Agent-agent direct memory sharing protocol** — currently every cross-agent interaction is a typed message. A future optimization could allow shared memory pages within the same group (still policy-decided), but not in this contract.
- **Multi-parent causality in plans** — saga-style fan-in (a plan step that depends on multiple parent steps) is deferred per S0.1 §3.4.
- **Per-task cognitive budget accounting (token / compute units)** — beyond S1.2 §13.3 model budgets, no agent-level accounting. Deferred.

## 21. Acceptance criteria

This sub-spec is satisfied when an implementation can demonstrate:

- An agent of `AgentKind = ASSISTANT` registered under a group with `is_ai = true` carried in its `Subject` per S5.1.
- The agent's directory under `/aios/groups/<g>/users/<u>/agents/<a>/...` exists with the required four memory subtrees per §5.3.
- The agent emits a typed action proposal that flows through S1.1 → S1.2 → L3 `SubmitAction` and the agent state transitions correctly through `BLOCKED_AWAITING_APPROVAL` and back.
- An `agent.memory.write` typed action results in an AIOS-FS object with the correct PrivacyClass per §4.5.
- An `agent.coordinate.send` typed action between two agents in the same group succeeds; the same action across groups fails closed with `AGENT_CROSS_GROUP_COORDINATION_BLOCKED` FOREVER evidence.
- An `external_model_call` typed action flows through the vault broker; the agent never sees the API key; `AICrossOriginPosture = AI_VAULT_BROKERED_ONLY` is enforced; a direct fetch attempt fails closed.
- An `agent.grade.attempt` by an AI subject for an artifact authored by the same AI is rejected closed with `AGENT_SELF_GRADING_BLOCKED` FOREVER evidence.
- An `agent.memory.read` of another user's `PRIVATE_TO_USER` memory by a personal `ASSISTANT` is rejected closed with `AGENT_MEMORY_CROSS_USER_DENIED` FOREVER evidence.
- An attempt by an agent to write directly to AIOS-FS outside the proposing pipeline fails closed with `AGENT_DIRECT_FS_WRITE_BLOCKED` FOREVER evidence.
- A plan with bundled approval is rejected by `PLAN_BUNDLED_APPROVAL_INELIGIBLE` if any step is hard-denied; per-action approval is allowed for the same plan.
- Recovery-mode entry transitions all running agents to `RETIRING` and emits `AGENT_INTERRUPTED_BY_RECOVERY` FOREVER for each; post-recovery, agents do not auto-resume.
- All 19 evidence record types from §16 are emitted under their declared retention classes; `AGENT_LIFECYCLE_TRANSITIONED` records cover every legitimate `AgentLifecycleState` transition with a closed `AgentLifecycleTransitionTrigger` (§16.2).
- C1 / INV-002 site 5 is verifiable by both **absence** of `FOREVER` bypass records and **presence** of `AGENT_LIFECYCLE_TRANSITIONED` records covering the proposing-pipeline path (§16.1).
- Telemetry from §15 is emitted with bounded label cardinality (≤ 100 active label tuples per metric).
- All three worked examples from §18 produce the specified outcomes.
- The proposing pipeline FSM has **no transition** that bypasses `SubmitAction` (verified by static analysis of the cognitive core's transition table).

## 22. See also

- [S0.1 Action Envelope + Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S1.1 Capability Translator](02_capability_translator.md)
- [S1.2 Latency Tiering](03_latency_tiering.md)
- [S5.1 Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)
- [S5.2 Vault Broker](../L4_Policy_Identity_Vault/02_vault_broker.md)
- [S8.1 Network Policy](../L8_Network_Hardware_Devices/02_network_policy.md)
- [S10.1 Capability Runtime gRPC](../L3_AIOS_SGR_Service_Graph_Runtime/03_capability_runtime_grpc.md)
- [S4.1 AIOS-FS Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S6.4 Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md)
- [L5 Cognitive Core overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

---

Status: REAL
Evidence: E1
