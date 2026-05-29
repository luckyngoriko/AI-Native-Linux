# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repository is

**AIOS — AI-Native Linux Distribution.** A specification-stage project for a real Linux distribution whose distinguishing layer is a Unified Cognitive Shell: a cognitive core, policy kernel, and append-only evidence log on top of a standard Linux substrate. Human goals become typed, policy-checked, verified system actions.

The repository is currently in **specification-only phase** — no source code, no build system, no test suite. Active work is the rev.2 layered specification.

## Repository layout

```text
055.AI-OS.NET--LINUX-AI/
├── 001.AI-OS.NET--SPECREV.1/           # Frozen rev.1 (verbatim move of original two files)
│   ├── 00_MASTER_INDEX.md
│   ├── 01_README.md                    # Original architecture vision
│   └── 02_SPECIFICATION.md             # Original canonical contract (24 sections)
│
├── 002.AI-OS.NET--SPECREV.2/           # Active rev.2 — layered rewrite
│   ├── 00_MASTER_INDEX.md
│   ├── 01_executive_summary.md
│   ├── 02_design_decisions.md          # ADR-style decision log
│   ├── 03_architecture_overview.md
│   ├── L0_Governance_Evidence_Safety/  # one folder per layer L0–L10
│   ├── L1_Kernel_Bootstrap_Recovery/
│   ├── L2_AIOS_FS/
│   ├── L3_AIOS_SGR_Service_Graph_Runtime/
│   ├── L4_Policy_Identity_Vault/
│   ├── L5_Cognitive_Core/
│   ├── L6_Apps_Packages_Compatibility/
│   ├── L7_Interaction_Renderers/
│   ├── L8_Network_Hardware_Devices/
│   ├── L9_Observability_Admin_Operations/
│   ├── L10_Distribution_Ecosystem_Marketplace/
│   └── XX_Cross_Cutting/               # contracts shared by multiple layers
│
├── README.md                            # top-level navigation
├── CLAUDE.md                            # this file
├── ai-os-logo-home.png
└── .gitignore
```

Each layer folder starts with `00_overview.md` (responsibility, invariants, dependencies, planned sub-specs) and grows numbered sub-spec files (`01_<topic>.md`, `02_<topic>.md`, ...) as work progresses.

## Authoritative source of truth

- **Rev.1** (`001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md`) is the **frozen** engineering contract. Do not modify; corrections go into rev.2.
- **Rev.2** (`002.AI-OS.NET--SPECREV.2/`) is the **active** specification. Each sub-spec under a layer folder is contract-grade for that topic when its status reaches `REAL`.
- **Rev.2 master index** (`002.AI-OS.NET--SPECREV.2/00_MASTER_INDEX.md`) is the navigation entry point and the sub-spec roadmap.

When the user gives a high-level objective, map it to:

1. a layer in the L0–L10 model (per Rev.1 §6),
2. a planned sub-spec listed in that layer's `00_overview.md`,
3. or a cross-cutting contract under `XX_Cross_Cutting/`.

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

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **055.AI-OS.NET--LINUX-AI** (19999 symbols, 48724 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## When Debugging

1. `gitnexus_query({query: "<error or symptom>"})` — find execution flows related to the issue
2. `gitnexus_context({name: "<suspect function>"})` — see all callers, callees, and process participation
3. `READ gitnexus://repo/055.AI-OS.NET--LINUX-AI/process/{processName}` — trace the full execution flow step by step
4. For regressions: `gitnexus_detect_changes({scope: "compare", base_ref: "main"})` — see what your branch changed

## When Refactoring

- **Renaming**: MUST use `gitnexus_rename({symbol_name: "old", new_name: "new", dry_run: true})` first. Review the preview — graph edits are safe, text_search edits need manual review. Then run with `dry_run: false`.
- **Extracting/Splitting**: MUST run `gitnexus_context({name: "target"})` to see all incoming/outgoing refs, then `gitnexus_impact({target: "target", direction: "upstream"})` to find all external callers before moving code.
- After any refactor: run `gitnexus_detect_changes({scope: "all"})` to verify only expected files changed.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Tools Quick Reference

| Tool | When to use | Command |
|------|-------------|---------|
| `query` | Find code by concept | `gitnexus_query({query: "auth validation"})` |
| `context` | 360-degree view of one symbol | `gitnexus_context({name: "validateUser"})` |
| `impact` | Blast radius before editing | `gitnexus_impact({target: "X", direction: "upstream"})` |
| `detect_changes` | Pre-commit scope check | `gitnexus_detect_changes({scope: "staged"})` |
| `rename` | Safe multi-file rename | `gitnexus_rename({symbol_name: "old", new_name: "new", dry_run: true})` |
| `cypher` | Custom graph queries | `gitnexus_cypher({query: "MATCH ..."})` |

## Impact Risk Levels

| Depth | Meaning | Action |
|-------|---------|--------|
| d=1 | WILL BREAK — direct callers/importers | MUST update these |
| d=2 | LIKELY AFFECTED — indirect deps | Should test |
| d=3 | MAY NEED TESTING — transitive | Test if critical path |

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/055.AI-OS.NET--LINUX-AI/context` | Codebase overview, check index freshness |
| `gitnexus://repo/055.AI-OS.NET--LINUX-AI/clusters` | All functional areas |
| `gitnexus://repo/055.AI-OS.NET--LINUX-AI/processes` | All execution flows |
| `gitnexus://repo/055.AI-OS.NET--LINUX-AI/process/{name}` | Step-by-step execution trace |

## Self-Check Before Finishing

Before completing any code modification task, verify:
1. `gitnexus_impact` was run for all modified symbols
2. No HIGH/CRITICAL risk warnings were ignored
3. `gitnexus_detect_changes()` confirms changes match expected scope
4. All d=1 (WILL BREAK) dependents were updated

## Keeping the Index Fresh

After committing code changes, the GitNexus index becomes stale. Re-run analyze to update it:

```bash
npx gitnexus analyze
```

If the index previously included embeddings, preserve them by adding `--embeddings`:

```bash
npx gitnexus analyze --embeddings
```

To check whether embeddings exist, inspect `.gitnexus/meta.json` — the `stats.embeddings` field shows the count (0 means no embeddings). **Running analyze without `--embeddings` will delete any previously generated embeddings.**

> Claude Code users: A PostToolUse hook handles this automatically after `git commit` and `git merge`.

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
