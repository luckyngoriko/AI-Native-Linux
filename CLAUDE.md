# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repository is

**AIOS — AI-Native Linux / Unified Cognitive Shell.** A specification-stage project for an operating environment that sits _above_ the Linux kernel and turns human goals into typed, policy-checked, verified system actions.

This is **not yet a code repository**. As of the current snapshot it contains only:

- `README.md` — architecture vision, rev.1
- `SPECIFICATION.md` — the canonical engineering contract, rev.1
- `ai-os-logo-home.png` — logo asset
- `firebase-debug.log` — stray log file from outside tooling, ignore
- `.agents/`, `.codex/`, `.playwright-mcp/` — empty or tooling-only directories

There is **no source code, no build system, no test suite, and no git history yet** (despite any shell context to the contrary — `git status` from inside the repo errors with "not a git repository"). Any future implementation work must therefore start with `git init` and an explicit scaffolding task, agreed with the user.

## Authoritative source of truth

`SPECIFICATION.md` is the contract. Read it before proposing any implementation, architecture change, or naming. The README is a vision summary; the spec overrides it on any conflict.

When the user gives a high-level objective, map it to a section of SPECIFICATION.md and a layer in the L0–L10 model below before writing code or planning files.

## Layer model (memorize this)

The system is structured into 11 strictly ordered layers. **A layer may depend on its own layer and lower-numbered layers. A layer must not require a higher-numbered layer for correctness.** Violating this is an architectural defect, not a style nit.

| Layer | Owns                                                                           |
| ----- | ------------------------------------------------------------------------------ |
| L0    | Constitutional truth: status taxonomy, evidence grades, gates, invariants      |
| L1    | Linux substrate, host bootstrap, generic fallback kernel, recovery path        |
| L2    | AIOS-FS — semantic object filesystem at `/aios`, versions, views, transactions |
| L3    | AIOS-SGR — desired-state service graph runtime                                 |
| L4    | Policy Kernel, identity, Vault Broker (secrets-as-capabilities)                |
| L5    | Cognitive Core — intent, planning, memory, model routing, agent coordination   |
| L6    | Apps, packages, compatibility (Linux/Windows/Android via sandboxed runtimes)   |
| L7    | Renderers — KDE Plasma, Web, CLI, Voice, Mobile (over shared UI schema)        |
| L8    | Network policy, hardware graph, drivers                                        |
| L9    | Observability, admin, recovery operations                                      |
| L10   | Distribution, ecosystem, marketplace                                           |

Key invariants that hold across the whole system:

- **L1 recovery must not depend on L5 cognition.** The machine boots and recovers without any LLM.
- **AI proposes, never executes.** The Cognitive Core emits typed actions; the Capability Runtime executes them only after a Policy Kernel decision and emits evidence.
- **Secrets are capabilities.** Raw secret read by an AI agent is hard-denied. The Vault Broker performs operations without revealing material.
- **Recovery boundary:** `/` immutable + recovery-safe, `/root` operator island, `/aios` AI-native root.
- **Evidence is append-only.** AI agents cannot modify evidence logs.
- **Web UI is localhost-only by default.** LAN/remote exposure requires explicit policy approval.

## Status and evidence taxonomy (use these exact words)

When reporting on any capability, the spec mandates these statuses: `REAL`, `PARTIAL`, `SHELL`, `CONTRACT`, `DEFERRED`, `BLOCKED`, `UNKNOWN`, `RETIRED`. Evidence grades: `E0` (none) → `E1` (artifact exists) → `E2` (build/typecheck) → `E3` (unit/integration test) → `E4` (e2e/recovery/release gate) → `E5` (live operational). **No capability is `REAL` without explicit evidence.** This taxonomy is enforced by the global operating mode in `~/.claude/CLAUDE.md` as well; align with it.

## Approved technology stack

The stack is decided in §21 of the spec. Do not propose alternatives without a concrete reason tied to a spec requirement.

- **Execution (Rust):** Rust + Tokio + tonic gRPC + serde + tracing
- **Cognition (Python):** Python + LangGraph (or equivalent) + FastAPI + Ollama/vLLM-compatible local runtime; external providers only via Vault Broker
- **UI:** KDE Plasma + Qt/QML for native; TypeScript + Next.js (or equivalent) for Web; Tailwind/shadcn-style discipline where useful
- **Storage:** AIOS-FS as native; SQLite for local metadata; PostgreSQL where service-grade relational is required; Qdrant (or equivalent) for vector
- **Observability:** OpenTelemetry + Prometheus + Loki + eBPF where appropriate

Stack philosophy: _Rust owns execution. Python owns cognition. KDE owns native interaction. Web owns remote surfaces. Linux owns physics. AIOS owns semantic operation._

## Capability Runtime contract (when you start implementing L3/L4)

Typed actions flow through this exact lifecycle (§13): `created → policy_pending → approved | approval_pending | policy_denied → queued → executing → verifying → succeeded | failed | rolled_back`.

The gRPC surface is fixed in §13:

```
ValidateAction → EvaluatePolicy → RequestApproval → ExecuteAction → VerifyAction → RollbackAction → GetActionStatus → ListAdapters → GetAdapterCapabilities
```

Adapters must **not** accept free-form shell commands as primary input. Unsupported actions fail closed.

## MVP golden path (§22)

If asked "where do we start coding," the spec is unambiguous:

```
Boot from recovery-safe root → mount /aios → create a versioned AIOS-FS object →
resolve it through a semantic view → run one verified typed system action →
record the full evidence chain → show the result in a renderer.
```

Acceptance criteria for the prototype are enumerated in §22 — use them as the test plan, not as suggestions.

## What this repo currently lacks (and how to handle it)

- **No `git init` yet.** Before any commits, confirm with the user, then initialize. Do not silently create a repo as a side effect of unrelated work.
- **No build files** (`Cargo.toml`, `pyproject.toml`, `package.json`, etc.). When adding the first one, place it according to the layer it implements (e.g. an L3 SGR service in `crates/aios-sgr/`, a renderer in `apps/web/`). Confirm the workspace layout with the user before scattering files.
- **No tests, no CI.** First implementation work should bring up the verification harness alongside the code, since the spec requires evidence (E2+) for `REAL` status.
- **No `.gitignore`.** `firebase-debug.log` and `.playwright-mcp/` snapshots should not be committed when the repo is initialized.

## Communication

The user is a Bulgarian non-programmer infrastructure operator (see `~/CLAUDE.md`). Reply in Bulgarian unless asked otherwise, explain in operational terms (what changed, what works, what is blocked, what comes next), and never claim completion without evidence — this is enforced by the global operating mode.
