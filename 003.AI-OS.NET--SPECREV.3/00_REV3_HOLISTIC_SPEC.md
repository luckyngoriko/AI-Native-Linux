# Rev.3 Holistic Specification

| Field            | Value                                                                                                                                                                       |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status           | `HOLISTIC-CONTRACT` (created 2026-05-28; completed 2026-05-29 — integrates materialized S16-S28 contracts)                                                                  |
| Kind             | Rev.3 master architecture; not a standalone Capella sub-spec                                                                                                                |
| Predecessor      | `002.AI-OS.NET--SPECREV.2/`                                                                                                                                                 |
| Formal sub-specs | S16 (.1-.9), S17 (.1-.5), S18, S19, S20, S21, S22, S23, S24, S25, S26, S27, S28 — all `CONTRACT`                                                                            |
| Navigation       | [`00_MASTER_INDEX.md`](00_MASTER_INDEX.md) (scope ledger), [`02_design_decisions.md`](02_design_decisions.md) (ADRs), [`04_invariants.md`](04_invariants.md) (INV-025..034) |
| Primary target   | Linux-first, policy-governed, AI-native, security-hardened distribution                                                                                                     |

## 1. Rev.3 thesis

Rev.3 turns AIOS from a constitutional Linux distribution into a governed,
AI-native operating system platform.

The system is not "another Linux distro" and not "a shell copilot." Rev.3 makes
five ideas work together:

```text
security posture is explicit
software runs as governed capsules
kernel and runtime backends are selected by capability
drivers are managed as high-risk capsules
AI lives natively in the OS but never becomes root
```

The Linux implementation remains the gold path for workstation, gaming,
GPU/video, containers, hardware support, and near-term delivery. The architecture
keeps kernel, package, driver, app, and AI mechanisms behind typed contracts so
future BSD, RTOS, microVM, WASI, and specialized backends can be added without
rewriting the whole OS.

## 2. Product objective

Rev.3 should let an operator ask for a goal and have AIOS produce a safe,
explainable, rollbackable plan.

Examples:

```text
install this application in the safest compatible way
make this workstation gaming-ready without weakening work data
prepare a STIG-aligned profile
test this GPU driver without risking boot recovery
build a kernel candidate only if the hardware/workload needs it
explain why this AI action is blocked under EU AI Act posture
```

AIOS must not hide complexity by becoming unsafe. The product promise is:

```text
best viable path
+ explicit risk
+ policy approval
+ sandbox / recovery boundary
+ evidence
+ rollback
+ clear blocked reason
```

## 3. Materialized Rev.3 contracts

| Contract                                                                                           | Role                                                                                                                  |
| -------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| [S16 Security Hardening and Compliance](S16_Security_Hardening_Compliance/00_overview.md)          | Security profiles, SELinux MAC plane, STIG/NIST/CIS control mapping, scanner/export contract.                         |
| [S17 App Capsule Runtime](S17_App_Capsule_Runtime/00_overview.md)                                  | Per-app capsules, solver lifecycle, Windows/Wine/Proton capsule path, reliability, operator UI.                       |
| [S18 Kernel Personality and Portability Plane](S18_Kernel_Personality_Portability/00_overview.md)  | Kernel backend adapters, capability matrix, need-driven adaptive kernel forge, BSD/RTOS/microVM/WASI target registry. |
| [S19 Driver and Firmware Capsule Plane](S19_Driver_Firmware_Capsule_Plane/00_overview.md)          | Driver solver, driver lab, signed driver capsules, canary boot, taint/evidence, firmware coupling.                    |
| [S20 Native AI Control Plane and AI Terminal](S20_Native_AI_Control_Plane_Terminal/00_overview.md) | Native AI control plane, `LX`/`MIX`/`AI` terminal modes, typed actions, EU AI Act technical controls.                 |

## 4. System architecture

Rev.3 is built as a set of controlled planes around existing Rev.2 primitives.

```text
Operator / UI / AI Terminal
  -> Native AI Control Plane
  -> Typed Action Fabric
  -> Policy Kernel + Approval Mechanics
  -> Domain solver
     app solver | driver solver | kernel solver | package solver | security scanner
  -> Sandbox / Runtime / Recovery boundary
  -> Evidence Log
  -> Audit / Compliance exports
```

Authority stays distributed:

| Plane                    | Owns                                                           |
| ------------------------ | -------------------------------------------------------------- |
| Policy Kernel            | Allow/deny/approval decisions.                                 |
| Approval Mechanics       | Human oversight and exact-action approval binding.             |
| Evidence Log             | Append-only proof trail.                                       |
| Security Profile         | Host hardening posture and compliance gates.                   |
| App Capsule Runtime      | Software execution envelope and lifecycle.                     |
| Kernel Personality Plane | Backend capability truth and kernel candidate gates.           |
| Driver Capsule Plane     | Driver selection, test, bind, rollback, taint tracking.        |
| Native AI Control Plane  | Understanding, proposal, explanation, typed-action generation. |

AI is intentionally not in the authority column. AI proposes and explains; it
does not self-approve, become root, weaken policy, or mutate evidence.

## 5. Core state objects

Rev.3 depends on machine-readable truth objects. AI and UI must read these before
falling back to raw shell output.

| Object                   | Purpose                                                                    |
| ------------------------ | -------------------------------------------------------------------------- |
| `SecurityProfile`        | Current hardening posture: dev, secure default, STIG-aligned, airgap-high. |
| `ControlMap`             | NIST/STIG/CIS/AIOS controls mapped to enforcement and evidence.            |
| `AppCapsule`             | Installed app identity, runtime, capabilities, data, rollback, evidence.   |
| `PackagePassport`        | Source, trust, SBOM, provenance, risk, compatibility.                      |
| `KernelCapabilityMatrix` | What the current kernel/backend can actually enforce.                      |
| `KernelBackendAdapter`   | Signed adapter mapping AIOS needs to host-kernel primitives.               |
| `KernelBuildCandidate`   | Need-driven custom kernel build candidate and promotion evidence.          |
| `DriverCapsule`          | Driver/module identity, source, hardware match, ABI, signing, rollback.    |
| `HardwareGraph`          | Per-boot signed device graph and hardware drift signal.                    |
| `NativeAISubject`        | AI actor identity, tool grants, mode, compliance profile.                  |
| `AIModelRegistryEntry`   | Model/provider/version/tool/data access inventory.                         |

## 6. Universal solver pattern

Rev.3 repeats one pattern across apps, drivers, kernels, packages, and AI
actions:

```text
request
  -> inspect signed state
  -> generate candidates
  -> score benefit/risk/compatibility
  -> show risk diff
  -> apply policy
  -> test away from active system when possible
  -> promote only with evidence
  -> rollback or block with reason
```

Solvers:

| Solver                | Input                                                   | Output                                            |
| --------------------- | ------------------------------------------------------- | ------------------------------------------------- |
| Capsule Solver        | package/app/runtime metadata + profile + kernel matrix  | native/container/Wine/VM/WASI/blocked path        |
| Driver Solver         | hardware graph + kernel ABI + profile + driver registry | driver capsule, degraded mode, VM route, or block |
| Kernel Solver         | hardware graph + workload goal + profile                | keep generic kernel, build candidate, or block    |
| Security Scanner      | profile manifest + host posture + control map           | pass/fail/exception/export                        |
| AI Intent Interpreter | terminal/user intent + state objects                    | typed action proposal or clarification            |

## 7. Software model

Packages are inputs. Capsules are runtime truth.

```text
deb/rpm/flatpak/snap/appimage/nix/oci/source/windows/android/vm
  -> intake / observation / conversion
  -> AIOSAppObject
  -> AppCapsule
  -> policy
  -> runtime launch
  -> evidence
```

Rev.3 does not promise every app runs. It promises every app gets a governed
execution plan, a bounded blast radius, rollback, evidence, and a clear blocked
reason.

Windows apps and games receive dedicated Wine/Proton/VM-aware capsules. They do
not share global `~/.wine` state and cannot mutate the real host directly.

## 8. Kernel and driver model

Kernel adaptation is need-driven, not hobby tuning.

Valid triggers:

- hardware enablement
- security posture
- real-time latency
- workload compatibility
- performance class
- recovery or reliability failure

Kernel candidate flow:

```text
generic Linux bootstrap
  -> hardware graph
  -> AI-assisted config proposal
  -> isolated build
  -> simulation
  -> signed candidate
  -> canary boot
  -> evidence
  -> promote or rollback
```

Driver flow:

```text
hardware need
  -> DriverSolver
  -> DriverLab
  -> signed DriverCapsule
  -> canary boot / controlled bind
  -> promote / rollback / degraded / VM fallback / blocked
```

The generic/recovery kernel remains the anchor. Driver and kernel changes cannot
remove recovery, weaken strict profiles, or bypass evidence.

## 9. AI-native OS model

AI is native to the OS, but governed.

Terminal modes:

```text
LX>   direct shell
MIX>  natural language by default; Linux commands require LX:
AI>   AI intent only; no raw shell execution
```

AI action path:

```text
natural language
  -> intent
  -> typed action
  -> risk diff
  -> policy / approval
  -> sandboxed execution
  -> verification
  -> evidence
```

Hard AI rules:

- AI cannot self-approve.
- AI cannot be root.
- AI cannot hide that it is AI.
- AI cannot bypass typed actions.
- AI cannot weaken security or compliance posture.
- AI cannot grade its own completion proof.
- AI cannot hide or rewrite evidence.

## 10. EU AI Act technical posture

Rev.3 treats EU AI Act alignment as an operating-system capability.

Required OS abilities:

| Ability                     | Rev.3 mechanism                                                     |
| --------------------------- | ------------------------------------------------------------------- |
| AI identity transparency    | Visible AI actor, terminal mode, AI-vs-human distinction.           |
| Human oversight             | Approval gates, emergency stop, no self-approval.                   |
| Logging                     | Evidence records for intent, proposal, execution, denial, rollback. |
| Risk classification         | AI context risk detector and compliance profile.                    |
| Prohibited pattern blocking | Compliance registry and hard-deny gate.                             |
| Model/tool inventory        | AI model/tool registry with provider, version, grants, data access. |
| Cybersecurity               | Prompt boundary classifier, sandboxed tools, no direct root.        |
| Data governance             | Vault, redaction, scoped access, no secret exposure by default.     |
| Audit export                | Evidence bundle and compliance profile export.                      |

S20 does not claim legal certification. It provides technical controls,
traceability, and audit material.

## 11. Security and compliance posture

Rev.3 has four security profiles:

```text
DEV_RELAXED
SECURE_DEFAULT
STIG_ALIGNED
AIRGAP_HIGH
```

The profile determines:

- SELinux mode
- module signing posture
- package trust requirement
- network defaults
- evidence retention
- AI autonomy ceiling
- exception handling
- kernel/backend admission
- driver and firmware gates

Formal certification claims are forbidden until an actual assessment exists.
Allowed language is "STIG-aligned hardening profile" once the scanner, evidence,
control map, and exception register exist.

## 12. Operator experience

Rev.3 must be easy from the operator viewpoint even when the internal system is
strict.

Operator should see:

- recommended path
- expected benefit
- exact risk
- what data/devices/network will be touched
- what approval is required
- rollback path
- evidence receipt
- alternatives
- blocked reason

No critical change should require the operator to inspect raw logs first.

## 13. Rev.3 implementation waves

Suggested delivery order:

| Wave  | Scope                                         | Why first                                                       |
| ----- | --------------------------------------------- | --------------------------------------------------------------- |
| R3-W1 | S16 security profile + scanner skeleton       | Creates hardening posture and proof language.                   |
| R3-W2 | S17 AppCapsule manifest + solver skeleton     | Turns application handling into a governed product surface.     |
| R3-W3 | S20 AI terminal + typed action fabric         | Makes AI native but bounded before adding deeper autonomy.      |
| R3-W4 | S19 DriverCapsule + DriverSolver              | Solves the hardware/driver risk without unsafe vendor scripts.  |
| R3-W5 | S18 KernelCapabilityMatrix + backend adapters | Makes kernel portability and adaptive builds real.              |
| R3-W6 | Package Rosetta formalization                 | Moves package-agnostic distro intake from planning to contract. |
| R3-W7 | Mobile/workstation/gaming surfaces            | Productizes form factors and video/GPU-heavy workflows.         |

## 14. Scope ledger — all sections materialized (2026-05-29)

Every section below was promoted from planning to a `CONTRACT`-grade sub-spec in the
2026-05-29 completion build. Nothing in Rev.3 scope remains "planning-only".
[`00_MASTER_INDEX.md`](00_MASTER_INDEX.md) holds the authoritative per-file status and the
cross-cutting theme homing.

| Contract                                     | Scope                                                                                       | Status     |
| -------------------------------------------- | ------------------------------------------------------------------------------------------- | ---------- |
| `S16.4 Measured Boot + Runtime Integrity`    | Secure Boot, TPM, IMA/EVM, dm-verity/IPE, attestation (P0 root of trust).                   | `CONTRACT` |
| `S16.5 FIPS / Crypto Boundary`               | `FIPS_STRICT` overlay, CMVP boundary, crypto evidence.                                      | `CONTRACT` |
| `S16.6 SBOM / Provenance / VEX`              | SPDX/CycloneDX SBOM, SLSA provenance, VEX, reproducible-build receipt.                      | `CONTRACT` |
| `S16.7 Service Hardening Score Gates`        | systemd hardening score floors and promotion gates.                                         | `CONTRACT` |
| `S16.8 Zero-Trust Fleet Posture`             | NIST 800-207 posture, continuous posture checks.                                            | `CONTRACT` |
| `S16.9 Data Governance, GDPR/RTBF, Audit`    | Crypto-shred erasure, residency, SOC2/ISO/HIPAA export.                                     | `CONTRACT` |
| `S21 Package Rosetta and Universal App Lab`  | Package-agnostic intake, script decompiler, shadow install, package passport.               | `CONTRACT` |
| `S22 Workstation, Gaming, and Video Profile` | GPU/video/audio/controller/game compatibility profile; energy policy.                       | `CONTRACT` |
| `S23 Mobile Renderer and Touch Shell`        | Phone approval console, touch shell, voice renderer.                                        | `CONTRACT` |
| `S24 Container and Kubernetes Native Plane`  | Podman/Docker/containerd/Kubernetes; isolation levels; WASM/eBPF/Deno/Bun/Python adapters.  | `CONTRACT` |
| `S25 Fleet, Cluster, and Remote Execution`   | Multi-host trust roots, remote workload routing, federated identity, evidence DAG.          | `CONTRACT` |
| `S26 Backup, DR, and Capsule Mobility`       | Encrypted off-host backup, export/import, personal mirror.                                  | `CONTRACT` |
| `S27 AI Evaluation and Model Governance`     | Accuracy/drift/hallucination/prompt-injection-rejection evidence; multi-agent coordination. | `CONTRACT` |
| `S28 Constitutional Time Plane`              | Trusted time sources, clock-skew detection, evidence time-trust grade.                      | `CONTRACT` |

Cross-cutting planning themes (federated identity, multi-agent coordination, ecosystem
runtime adapters, voice renderer, energy, GDPR/RTBF, trusted time) are each homed in exactly
one contract above per [`DEC-R3-011`](02_design_decisions.md). Ten new constitutional
invariants (INV-025..034) cover the new planes; see [`04_invariants.md`](04_invariants.md).

## 15. Non-goals

- Do not make AIOS a loose wrapper around random Linux scripts.
- Do not claim all packages, drivers, kernels, or foreign apps will always run.
- Do not weaken recovery or evidence for convenience.
- Do not make BSD/RTOS support block the Linux MVP.
- Do not let AI become an unbounded root agent.
- Do not claim EU AI Act, STIG, FIPS, or military certification without real
  assessment.

## 16. Holistic acceptance criteria

Rev.3 is coherent only when:

1. Every consequential operation is a typed action or a bounded lab operation.
2. Security profile gates apply across apps, drivers, kernels, AI, packages,
   and network.
3. App, driver, kernel, and AI solvers produce risk diff plus rollback path.
4. AI can explain and operate the OS through state objects, not blind shell
   scraping.
5. No AI path can self-approve or bypass Policy Kernel.
6. Every high-risk mutation emits evidence.
7. **Traceability gate (verifiable).** Running, from `tools/capella/`,
   `python extract.py && python build.py && python analyze.py && python classify_inversions.py`
   reports, over the FULL Rev.3 graph (not the XX-exempt subset):
   - zero orphan INVs (every INV-001..034 is realized by ≥1 sub-spec);
   - zero consumes-graph cycles;
   - zero `runtime` (forbidden-upward) layer inversions and zero `uncertain` inversions
     after W11-A classification (only `vocabulary`/`exception` allowed);
   - zero orphan RecordTypes (every one of the 623 record types — 427 Rev.2 + the Rev.3
     evidence delta — has ≥1 emitter trace);
   - every consumes edge resolves to an existing sub-spec (zero skipped rows in `build.py`).
     The only permitted non-zero is the 10 pre-existing Rev.2 **orphan sub-specs**
     (status/taxonomy/identity/latency definitional specs that carry no INV realization by
     design); zero Rev.3 sub-spec is orphan. Last verified pass: 2026-05-29.
8. Operators can understand recommended path, risk, approval, rollback, and
   blocked reason without reading implementation code.

## 17. See also

- [Rev.3 Planning Notes](00_PLANNING_NOTES.md)
- [S16 Security Hardening and Compliance](S16_Security_Hardening_Compliance/00_overview.md)
- [S17 App Capsule Runtime](S17_App_Capsule_Runtime/00_overview.md)
- [S18 Kernel Personality and Portability Plane](S18_Kernel_Personality_Portability/00_overview.md)
- [S19 Driver and Firmware Capsule Plane](S19_Driver_Firmware_Capsule_Plane/00_overview.md)
- [S20 Native AI Control Plane and AI Terminal](S20_Native_AI_Control_Plane_Terminal/00_overview.md)
