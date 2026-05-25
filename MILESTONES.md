# AIOS Implementation Roadmap

Source of truth: `.ai/tasks.json` (machine-readable). This document is the human-readable map of the 18-milestone plan from spec opening to runnable AI-native Linux distribution.

## Conventions

- Each milestone closes with a **honest no-debt criterion**: every sub-spec listed is implemented to `REAL` status per L0 taxonomy, version bumped from `0.0.1` → `0.1.0`, all cargo gates green (`check + test + clippy -D warnings + fmt --check + bench compiles`).
- Tasks within a milestone are labeled `T-NNN`, sequential across milestones (T-001 .. T-035 are M1..M4; T-036+ continue into M5+).
- Cross-milestone debt is forbidden by the global `feedback_no_technical_debt.md` rule.

## Status snapshot (2026-05-25)

| Milestone      | Crate                   | Sub-specs            | Layer              | Status   |   Tests |
| -------------- | ----------------------- | -------------------- | ------------------ | -------- | ------: |
| M1             | aios-action             | S0.1                 | XX (cross-cutting) | ✓ closed |     102 |
| M2             | aios-evidence           | S3.1                 | L9                 | ✓ closed |     262 |
| M3             | aios-policy             | S2.3                 | L4                 | ✓ closed |     235 |
| M4             | aios-capability-runtime | S10.1                | L3                 | ✓ closed |     222 |
| M5             | aios-fs                 | S1.3, S2.1, S2.2, S4.1 | L2              | ✓ closed |     176 |
| M6             | aios-vault              | S5.1, S5.2, S5.4    | L4                 | ✓ closed |     163 |
| M7             | aios-renderer-cli       | S7.6                 | L7                 | ✓ closed |     121 |
| M8             | aios-verification       | S2.4                 | L9                 | ✓ closed |     141 |
| M9             | aios-recovery           | S9.1, S9.2, S9.3    | L1                 | ✓ closed |     137 |
| **Total done** | **9 crates**            | **16 / 53 sub-specs** | —                 | —        | **1592** |

**§22 FULL-REAL MVP marker:** the golden path has no stubs. Boot is real via `InMemoryRecoveryBoundary` + `FirstBootDriver` + `KernelPipelineDriver`; mount/object/view are real through `InMemoryAiosFs`; action/policy/adapter/verification/evidence are real through runtime, policy, adapter registry, `VerificationEngine`, and signed evidence; render is real through `aios-renderer-cli`.

**M9 closure marker:** `aios-recovery` is v0.1.0. S9.1 recovery boundary, S9.2 first-boot FSM, and S9.3 dedicated-kernel pipeline are closed with acceptance fixtures and closure invariants.

## §22 MVP Golden Path closure (M5 → M9)

These 5 milestones make the §22 MVP runnable, trustworthy, and fully real. After M9, AIOS can drive the policy/runtime/fs/vault/verification/recovery stack through real in-process backends, create/read/list/version AIOS-FS objects, verify action completion, emit a signed chain, and render the resulting action state.

| Milestone | Crate             | Sub-specs              | Layer | Rationale                                                                                                                                                 |
| --------- | ----------------- | ---------------------- | ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| M5        | aios-fs           | S1.3, S2.1, S2.2, S4.1 | L2    | ✓ closed at 176 crate tests / 997 workspace tests. Object model + namespace + query/view + implementation space; proves §22 phase-2 at the AIOS-FS layer. |
| M6        | aios-vault        | S5.1, S5.2, S5.4       | L4    | ✓ closed at 163 crate tests / 1160 workspace tests. Identity + vault broker + emergency override; §22 vault-mediated external-call path proven with INV-018. |
| M7        | aios-renderer-cli | S7.6                   | L7    | ✓ closed at 121 crate tests / 1281 workspace tests. `aios` CLI renders the §22 path in Text + JSON; L1 boot/mount and EvidenceLog endpoint integration are explicit follow-up surfaces. |
| M8        | aios-verification | S2.4                   | L9    | ✓ closed at 141 crate tests / 1436 workspace tests. Runtime `step_verify` now calls the real verification engine; §22 actions are verified, not just executed. |
| M9        | aios-recovery     | S9.1, S9.2, S9.3       | L1    | ✓ closed at 137 crate tests / 1592 workspace tests. L1 boot/recovery replaces the last §22 stub; the golden path is FULL-REAL end-to-end. |

## Beyond MVP — full distro (M10 → M18)

| Milestone | Crate             | Sub-specs                        | Layer | Rationale                                                                                                                           |
| --------- | ----------------- | -------------------------------- | ----- | ----------------------------------------------------------------------------------------------------------------------------------- |
| M8        | aios-verification | S2.4                             | L9    | ✓ closed. Real VerificationEngine.RunVerification replaces the M4 `step_verify` stub when configured; failure blocks success.        |
| M9        | aios-recovery     | S9.1, S9.2, S9.3                 | L1    | ✓ closed. Recovery boundary + first-boot + dedicated kernel pipeline; §22 FULL-REAL MVP marker.                                      |
| M10       | aios-sgr          | S15.1, S15.2, S15.3              | L3    | Ready. AIOS-SGR desired-state service graph, unit manifest, adapter declaration.                                                     |
| M11       | aios-cognitive    | S1.1, S1.2, S13.1, S13.2, S14.1  | L5    | Cognitive core + model router + circuit breaker. INV-002 AI-proposes-never-executes enforcement at runtime.                         |
| M12       | aios-sandbox      | S3.2                             | L6    | SandboxProfile + GpuPolicy — referenced as type-level by M3/M4; this builds the runtime.                                            |
| M13       | aios-apps         | S12.1, S12.2, S12.3, S12.4, S6.5 | L6    | Cross-ecosystem runtimes (Linux/Windows/Android via sandboxed adapters). DEC-056 session container.                                 |
| M14       | aios-renderer-kde | S7.1, S7.2, S7.4                 | L7    | KDE Plasma + surface + shared UI. S7.1↔S8.2 vocabulary relocation (W12+ scheduled from Capella iter 5) may land here.               |
| M15       | aios-renderer-web | S7.5                             | L7    | Localhost-only by default (INV-021). LAN/remote exposure gated by explicit policy.                                                  |
| M16       | aios-network      | S8.1, S8.4, S8.5                 | L8    | Network policy + DNS/VPN + firmware trust. AICrossOriginPosture enforcement.                                                        |
| M17       | aios-hardware     | S8.3, S8.2                       | L8    | Hardware graph + GPU resource model. GpuCapabilityClass referenced by M3 hard-deny + constraints.                                   |
| M18       | aios-distribution | S11.1                            | L10   | Repository + signed package distribution + marketplace + publisher trust chain.                                                     |

## Progress projection

- **Current pace**: ~175 tests/milestone, ~9 commits/milestone
- **At M9 (§22 FULL-REAL MVP)**: 1592 tests, 9 crates
- **At M18 (full distro)**: ~3,400–4,000 tests, 18 crates
- **53 sub-specs total → 16 done → 37 remaining** distributed across M10–M18
- **Cross-cutting (XX) sub-specs** beyond the 18-milestone plan may land as targeted T-tasks inside existing milestones (e.g. ECDSA signing variants, additional renderer protocols).

## Closure criteria per milestone

Reused from M1–M6 closure pattern:

1. Every listed sub-spec at L0 status = `REAL` (E2+ evidence)
2. Workspace tests grow honestly (no skipped/ignored production tests)
3. All 4 cargo gates green: `check + test + clippy -D warnings + fmt --check`
4. Bench compiles where applicable (`cargo bench -p <crate> --no-run`)
5. Version bumped from `0.0.1` → `0.1.0` on the crate(s) the milestone closes
6. Closure-invariant test file (`tests/m<N>_closure.rs`): no `Status::Unimplemented`, no `todo!()`/`unimplemented!()` in src/, version marker correct
7. Acceptance fixtures from the relevant sub-spec(s) wired as integration tests
8. `.ai/memory.json` `current_milestone` advanced; auto-memory `project_implementation_state.md` refreshed
9. **Zero debt** — nothing carried into the next milestone

## How to start the next milestone

```bash
# 1. Read this file + .ai/tasks.json for the chosen milestone
# 2. Read the listed sub-spec markdown files in 002.AI-OS.NET--SPECREV.2/
# 3. Launch the first T-NNN worker (rust-pro subagent) with a clear charter
# 4. Sequential workers per task; Governor verifies gates between commits
# 5. Final T-task is the milestone closer: §22-style acceptance fixtures + version bump + closure-invariant tests
```

Last update: 2026-05-25 (M9 closed at T-083, 1592 workspace tests; M10 ready).
