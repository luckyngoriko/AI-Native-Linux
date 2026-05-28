# AIOS Implementation Roadmap

Source of truth: `.ai/tasks.json` (machine-readable). This document is the human-readable map of the 18-milestone plan from spec opening to runnable AI-native Linux distribution. Plan extended on 2026-05-28 with **M18.5** (integration-as-process) inserted between M18 and Rev.2 FULL-REAL — the framework lands in Rev.2 so Rev.3 category 7 operationalization is purely additive.

## Conventions

- Each milestone closes with a **honest no-debt criterion**: every sub-spec listed is implemented to `REAL` status per L0 taxonomy, version bumped from `0.0.1` → `0.1.0`, all cargo gates green (`check + test + clippy -D warnings + fmt --check + bench compiles`).
- Tasks within a milestone are labeled `T-NNN`, sequential across milestones (T-001 .. T-035 are M1..M4; T-036+ continue into M5+).
- Cross-milestone debt is forbidden by the global `feedback_no_technical_debt.md` rule.

## Status snapshot (2026-05-25)

| Milestone      | Crate                   | Sub-specs                       | Layer              | Status   |    Tests |
| -------------- | ----------------------- | ------------------------------- | ------------------ | -------- | -------: |
| M1             | aios-action             | S0.1                            | XX (cross-cutting) | ✓ closed |      102 |
| M2             | aios-evidence           | S3.1                            | L9                 | ✓ closed |      262 |
| M3             | aios-policy             | S2.3                            | L4                 | ✓ closed |      235 |
| M4             | aios-capability-runtime | S10.1                           | L3                 | ✓ closed |      222 |
| M5             | aios-fs                 | S1.3, S2.1, S2.2, S4.1          | L2                 | ✓ closed |      176 |
| M6             | aios-vault              | S5.1, S5.2, S5.4                | L4                 | ✓ closed |      163 |
| M7             | aios-renderer-cli       | S7.6                            | L7                 | ✓ closed |      121 |
| M8             | aios-verification       | S2.4                            | L9                 | ✓ closed |      141 |
| M9             | aios-recovery           | S9.1, S9.2, S9.3                | L1                 | ✓ closed |      137 |
| M10            | aios-sgr                | S15.1, S15.2, S15.3             | L3                 | ✓ closed |      169 |
| M11            | aios-cognitive          | S1.1, S1.2, S13.1, S13.2, S14.1 | L5                 | ✓ closed |      174 |
| M12            | aios-sandbox            | S3.2                            | L6                 | ✓ closed |      230 |
| **Total done** | **14 crates**           | **24 / 53 sub-specs**           | —                  | —        | **3345** |

**§22 FULL-REAL MVP marker:** the golden path has no stubs. Boot is real via `InMemoryRecoveryBoundary` + `FirstBootDriver` + `KernelPipelineDriver`; mount/object/view are real through `InMemoryAiosFs`; action/policy/adapter/verification/evidence are real through runtime, policy, adapter registry, `VerificationEngine`, and signed evidence; render is real through `aios-renderer-cli`.

**M9 closure marker:** `aios-recovery` is v0.1.0. S9.1 recovery boundary, S9.2 first-boot FSM, and S9.3 dedicated-kernel pipeline are closed with acceptance fixtures and closure invariants.

**M10 closure marker:** `aios-sgr` is v0.1.0. S15.1 unit manifest, S15.2 graph evaluation, and S15.3 adapter model are closed with M3-M9 composed service-graph scenarios, acceptance fixtures, and closure invariants.

**M11 closure marker:** `aios-cognitive` is v0.1.0. S1.1 intent model, S1.2 latency classifier, S13.1 model routing, S13.2 provider dispatch, and S14.1 circuit breaker are closed with INV-002 cross-crate enforcement and acceptance fixtures.

**M12 closure marker:** `aios-sandbox` is v0.1.0. S3.2 sandbox composition with 6-source merge, GPU policy enforcement, resource limit enforcement, syscall allowlisting, and §22 sandbox walk scenarios are closed with 10 acceptance fixtures and 8 closure invariants.

**M13 closure marker:** `aios-apps` is v0.1.0. S12.1 package store, S12.2 update FSM, S12.3 session management, S12.4 compatibility knowledge DB, and S6.5 DEC-056 session container are closed with 11-variant UpdateState FSM, full rollback path, gRPC AppsService (12 RPCs), 10 evidence record types, cross-crate bridges to runtime/sandbox/sgr, and `aios apps` CLI subcommand.

**M14 closure marker:** `aios-renderer-kde` is v0.1.0. S7.1 composition surface model, S7.2 shared UI schema (19-variant closed NodeKind vocabulary), and S7.4 KDE Plasma renderer are closed with full Qt/QML compilation table, VisualToken→QtRecipe compilation, Wayland surface model + INV I4 wlr-layer-shell enforcement, signed KWin script loader (INV I8), recovery shell session marker (INV I5) + constitutional icon bundle verification (INV I6), fail-closed degraded fallback (INV I7), gRPC KdeRendererService (14 RPCs), 10 evidence record types with INV-015 redaction, optional cxx-qt 0.7 bridge behind qt-bridge feature, and AppsBridge proving cross-renderer domain parity with aios-apps.

**M15 closure marker:** `aios-renderer-web` is v0.1.0. S7.5 Web renderer is closed with HTTPS server + self-signed loopback cert (INV I9), gRPC-Web bridge with CORS enforcement, ExposureFsm with 24h heartbeat (INV I3) covering `Localhost → LanPending → LanApproved → LanActive → Revoked` and `Localhost → Public` (recovery-authorized) transitions, OriginVerifier with per-group iframe origin binding (INV I4), ChromeIntegrityMonitor with Ed25519-signed subtree root hash verification (INV I10), InMemoryWebRenderer implementing the full 14-RPC WebRenderer trait, 9 evidence record types with BLAKE3 hash-chain integrity, WebAppsBridge proving cross-renderer domain parity with aios-apps, and 22 closure-invariant + 6 acceptance-fixture tests.

**M16 closure marker:** `aios-network` is v0.1.0. S8.1 network posture (5-variant NetworkPosture + 5-variant OutboundDirective) + exposure approval FSM (8-state ExposureApprovalFsm from Loopback→LanActive→PublicActive, with revocation), S8.4 DNS discipline (signed ResolverProfile admission + rotation + QueryGuard + PlainDns block) + VPN discipline (WireGuard tunnel lifecycle: propose→approve→activate→handshake→revoke) + mDNS gating (4-state MdnsAvahiPosture enforcement), and S8.5 firewall rule model (FirewallRulesetBuilder + 4-chain compilation: Input/Output/Forward/Prerouting) are closed with 5 cross-crate bridge modules (policy_bridge → aios-policy, capability_bridge → aios-capability-runtime, apps_bridge → aios-apps, sandbox_bridge → aios-sandbox, renderer_web_bridge → aios-renderer-web), 30 evidence NetworkRecordType variants with BLAKE3 hash-chain integrity, gRPC NetworkPolicyService + DnsVpnService surfaces, 311 crate tests, 14 closure-invariant + 3 acceptance-fixture tests covering all 6 T-162 phases, and full workspace pass at 3345 tests.

## §22 MVP Golden Path closure (M5 → M9)

These 5 milestones make the §22 MVP runnable, trustworthy, and fully real. After M9, AIOS can drive the policy/runtime/fs/vault/verification/recovery stack through real in-process backends, create/read/list/version AIOS-FS objects, verify action completion, emit a signed chain, and render the resulting action state.

| Milestone | Crate             | Sub-specs              | Layer | Rationale                                                                                                                                                                               |
| --------- | ----------------- | ---------------------- | ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| M5        | aios-fs           | S1.3, S2.1, S2.2, S4.1 | L2    | ✓ closed at 176 crate tests / 997 workspace tests. Object model + namespace + query/view + implementation space; proves §22 phase-2 at the AIOS-FS layer.                               |
| M6        | aios-vault        | S5.1, S5.2, S5.4       | L4    | ✓ closed at 163 crate tests / 1160 workspace tests. Identity + vault broker + emergency override; §22 vault-mediated external-call path proven with INV-018.                            |
| M7        | aios-renderer-cli | S7.6                   | L7    | ✓ closed at 121 crate tests / 1281 workspace tests. `aios` CLI renders the §22 path in Text + JSON; L1 boot/mount and EvidenceLog endpoint integration are explicit follow-up surfaces. |
| M8        | aios-verification | S2.4                   | L9    | ✓ closed at 141 crate tests / 1436 workspace tests. Runtime `step_verify` now calls the real verification engine; §22 actions are verified, not just executed.                          |
| M9        | aios-recovery     | S9.1, S9.2, S9.3       | L1    | ✓ closed at 137 crate tests / 1592 workspace tests. L1 boot/recovery replaces the last §22 stub; the golden path is FULL-REAL end-to-end.                                               |

## Beyond MVP — full distro (M10 → M18)

| Milestone | Crate             | Sub-specs                        | Layer | Rationale                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| --------- | ----------------- | -------------------------------- | ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --- |
| M8        | aios-verification | S2.4                             | L9    | ✓ closed. Real VerificationEngine.RunVerification replaces the M4 `step_verify` stub when configured; failure blocks success.                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| M9        | aios-recovery     | S9.1, S9.2, S9.3                 | L1    | ✓ closed. Recovery boundary + first-boot + dedicated kernel pipeline; §22 FULL-REAL MVP marker.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| M10       | aios-sgr          | S15.1, S15.2, S15.3              | L3    | ✓ closed at 169 crate tests / 1775 workspace tests. Desired-state service graph, unit manifest, graph traversal/cycle detection, recovery-aware pause/resume, and adapter fail-closed semantics.                                                                                                                                                                                                                                                                                                                                                                                                   |
| M11       | aios-cognitive    | S1.1, S1.2, S13.1, S13.2, S14.1  | L5    | ✓ closed at 174 crate tests. Cognitive core + model router + circuit breaker. INV-002 AI-proposes-never-executes enforcement at runtime.                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| M12       | aios-sandbox      | S3.2                             | L6    | ✓ closed at 230 crate tests / 2345 workspace tests. SandboxProfile + GpuPolicy — 6-source merge, GPU enforcement, resource limits, syscall allowlisting, §22 sandbox walk.                                                                                                                                                                                                                                                                                                                                                                                                                         |
| M13       | aios-apps         | S12.1, S12.2, S12.3, S12.4, S6.5 | L6    | ✓ closed. Cross-ecosystem runtimes (Linux/Windows/Android via sandboxed adapters). DEC-056 session container.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| M14       | aios-renderer-kde | S7.1, S7.2, S7.4                 | L7    | ✓ closed. KDE Plasma + surface + shared UI. 19-variant closed NodeKind vocabulary, Qt/QML compilation, Wayland surface model, evidence emission, cxx-qt bridge, AppsBridge cross-renderer parity.                                                                                                                                                                                                                                                                                                                                                                                                  |
| M15       | aios-renderer-web | S7.5                             | L7    | ✓ closed. Localhost-only by default (INV-021). HTTPS + gRPC-Web, ExposureFsm (INV I3), OriginVerifier (INV I4), ChromeIntegrityMonitor (INV I10), 9 evidence record types, WebAppsBridge cross-crate parity.                                                                                                                                                                                                                                                                                                                                                                                       |
| M16       | aios-network      | S8.1, S8.4, S8.5                 | L8    | ✓ closed. Network policy + DNS/VPN + firewall. AICrossOriginPosture enforcement.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | 311 |
| M17       | aios-hardware     | S8.3, S8.2                       | L8    | ✓ closed. Hardware graph + GPU resource model + firmware trust + evidence emission + gRPC service layer + cross-crate integration bridges (5 bridges to policy/sandbox/capability-runtime/recovery/network). 18 closure-invariant + 18 acceptance-fixture tests.                                                                                                                                                                                                                                                                                                                                   |
| M18       | aios-distribution | S11.1                            | L10   | Repository + signed package distribution + marketplace + publisher trust chain.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| M18.5     | aios-integration  | S11.4 (NEW for Rev.2)            | L10   | Integration-as-process framework. Typed `IntegrationLifecycleState` FSM (6 states), `VendorIntegrationContract`, `ExternalStandardSubscription` registry, 3 new evidence types (`INTEGRATION_PROPOSED`, `STANDARD_UPDATE_AVAILABLE`, `PACKAGE_HAS_KNOWN_CVE`). Contract-only — no live CVE feeds, SIEM bridges, or STIG control maps; those operationalize in Rev.3 category 7. Inserted into Rev.2 because L10 trust roots + bridges already exist and would be retroactively breaking to revise from Rev.3. Plan: 1 new sub-spec (~1500–2500 lines) + 1 milestone of ~12 T-tasks (T-163..T-174). |

## Progress projection

- **Current pace**: ~195 tests/milestone, ~9 commits/milestone
- **At M12 (sandbox composition)**: 2345 tests, 12 crates
- **At M18 (full distro)**: ~3,600–4,200 tests, 18 crates
- **53 sub-specs total → 20 done → 33 remaining** distributed across M13–M18
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

Last update: 2026-05-28 (M17 closed — aios-hardware v0.1.0; M18 aios-distribution next)
