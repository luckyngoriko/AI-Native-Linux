# Rev.3 — Architecture Overview

| Field     | Value                                                                        |
| --------- | ---------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-29)                                              |
| Scope     | Plane-by-plane architecture; how S16–S28 compose over Rev.2 L0–L10           |
| Companion | [`00_REV3_HOLISTIC_SPEC.md`](00_REV3_HOLISTIC_SPEC.md) (thesis + acceptance) |

## 1. Architectural stance

Rev.3 adds **no new layers** and changes **no Rev.2 invariant**. It introduces planes that
sit over the Rev.2 layer model. Authority remains distributed exactly as in Rev.2:

```text
Operator / UI / AI Terminal
  -> Native AI Control Plane (S20)            AI proposes & explains only
  -> Typed Action Fabric (S20, S0.1)
  -> Policy Kernel + Approval Mechanics (S2.3, S5.3)   the only deciders
  -> Domain solver
       app (S17/S21) | driver (S19) | kernel (S18) | package (S21) | security scanner (S16.3)
  -> Capability Runtime (S10.1) + Sandbox Composition (S3.2)   the only executor
  -> Recovery boundary (S9.1)
  -> Evidence Log (S3.1)                       append-only
  -> Audit / Compliance export (S16.3, S16.9, S27)
```

AI is intentionally absent from the authority column: it proposes, explains, and may run
allowed read tools; it never decides, becomes root, weakens a profile, promotes a
kernel/driver candidate, or mutates evidence (INV-002, INV-010, INV-025, INV-028, INV-031).

## 2. The universal solver pattern

Every domain plane reuses one pattern (holistic §6), so apps, drivers, kernels, packages,
and AI actions are governed identically:

```text
request
  -> inspect signed state (SecurityProfile, HardwareGraph, KernelCapabilityMatrix, passports)
  -> generate candidates
  -> score benefit / risk / compatibility
  -> render risk diff
  -> Policy Kernel decision (+ approval when required)
  -> test off the active system (lab / shadow / canary) when possible
  -> promote only with evidence
  -> rollback or block with a typed reason
```

| Solver                   | Owner | Output domain                                                                          |
| ------------------------ | ----- | -------------------------------------------------------------------------------------- |
| Capsule Solver           | S17.2 | native / container / Wine / VM / WASI / blocked                                        |
| Package Solver / Rosetta | S21   | intake decision + AIOSAppObject + passport                                             |
| Driver Solver            | S19   | in-tree / signed capsule / source build / vendor / userspace / VM / degraded / blocked |
| Kernel Solver            | S18   | keep generic / build candidate / route remote / block                                  |
| Security Scanner         | S16.3 | pass / fail / exception / export                                                       |
| AI Intent Interpreter    | S20   | typed action proposal or clarification                                                 |

## 3. Plane map

```text
+-----------------------------------------------------------------------+
|  S20 Native AI Control Plane / AI Terminal   (L5, L7)                  |
|  S27 AI Evaluation & Model Governance        (L5, L0)                  |
+-----------------------------------------------------------------------+
|  S17 App Capsules (L6)   S21 Package Rosetta (L6,L10)                  |
|  S22 Workstation/Gaming/Video (L6,L7,L8)   S24 Containers/K8s (L6,L8)  |
+-----------------------------------------------------------------------+
|  S18 Kernel Personality (L1)   S19 Driver/Firmware (L1,L8)            |
+-----------------------------------------------------------------------+
|  S16 Security Hardening & Compliance (.1-.9)  (L0,L1,L4,L8,L9,L10)     |
|  S28 Constitutional Time Plane (L0,L1,L9)                             |
+-----------------------------------------------------------------------+
|  S23 Mobile/Voice Renderer (L7)   S25 Fleet/Cluster (L8,L4,L10)        |
|  S26 Backup/DR/Mobility (L2,L9,L10)                                    |
+=======================================================================+
|  Rev.2 substrate: L0 governance/evidence/invariants ... L10 ecosystem |
+-----------------------------------------------------------------------+
```

Reading rule (INV-007): every plane depends only on its own layers and lower-numbered ones.
Cross-layer references that are vocabulary-only (e.g. S17 naming S8 device classes) are
classified `imports-vocabulary-from`, never a runtime dependency on a higher layer.

## 4. Core state objects (holistic §5) and their homes

| Object                                                                     | Defined in            |
| -------------------------------------------------------------------------- | --------------------- |
| `SecurityProfile`                                                          | S16.1                 |
| `ControlMap`                                                               | S16.3                 |
| `BootPosture` / `TPMQuote`                                                 | S16.4                 |
| `CryptoBoundarySelection`                                                  | S16.5                 |
| SBOM / provenance / VEX schemas                                            | S16.6                 |
| `AIOSAppObject` / `AppCapsule`                                             | S17.1                 |
| `PackagePassport`                                                          | S21                   |
| `KernelCapabilityMatrix` / `KernelBackendAdapter` / `KernelBuildCandidate` | S18                   |
| `DriverCapsule`                                                            | S19                   |
| `HardwareGraph`                                                            | Rev.2 S8.3 (consumed) |
| `NativeAISubject` / `AIModelRegistryEntry`                                 | S20                   |
| `WorkstationPassport` / `GamePassport` / `VideoPassport`                   | S22                   |
| `CloudNativePassport`                                                      | S24                   |
| `FederatedSubjectId` / `ClusterTrustRoot`                                  | S25                   |
| `TimePosture` / `TimeTrustGrade`                                           | S28                   |

## 5. The boot-to-render trust spine

The full chain a consequential operation traverses, with the contract that governs each hop:

```text
S16.4 measured boot (TPM+firmware dual chain, IMA/EVM)
  -> S28 trusted time established (evidence timestamps graded)
  -> S9.1 recovery-safe root mounted (no AI required — INV-001)
  -> S16.1 security profile loaded; S16.2 SELinux enforcing
  -> S20 AI interprets intent -> typed action + risk diff
  -> S2.3 Policy Kernel decision (+ S5.3 approval; phone via S23)
  -> domain solver (S17/S18/S19/S21) produces a tested candidate
  -> S10.1 Capability Runtime executes inside S3.2 sandbox
  -> S3.1 evidence appended (time-trust graded, append-only)
  -> S16.3/S16.9/S27 export audit / compliance / evaluation evidence
```

No hop trusts the one above it for correctness; recovery and boot never require AI.

## 6. Where Rev.3 extends Rev.2 invariants

| New plane               | New invariant                                      | Extends           |
| ----------------------- | -------------------------------------------------- | ----------------- |
| eBPF adapter (S24)      | INV-025 AI cannot author eBPF                      | INV-002           |
| Fleet (S25/S16.8)       | INV-026 cluster root cannot override host          | new               |
| Data governance (S16.9) | INV-027 crypto-shred preserves evidence            | INV-005 ↔ GDPR    |
| Measured boot (S16.4)   | INV-028 AI cannot alter boot integrity             | INV-002 + INV-005 |
| Package Rosetta (S21)   | INV-029 foreign scripts never root-on-host         | INV-002 + INV-012 |
| Workstation (S22)       | INV-030 workspace data-boundary isolation          | INV-011           |
| Mobile/Voice (S23)      | INV-031 render surface is not authority            | INV-002 + INV-019 |
| Federation (S25)        | INV-032 federated identity lossless/non-escalating | new               |
| Backup/DR (S26)         | INV-033 off-host backup encrypted & recoverable    | new               |
| Time plane (S28)        | INV-034 timestamp declares time-trust grade        | INV-005           |

## 7. Build sequence

The dependency-correct order is the wave plan in [`00_MASTER_INDEX.md`](00_MASTER_INDEX.md) §5:
security + measured boot first (the root of trust), then capsules, then the AI terminal, then
drivers and kernel, then package intake, then form factors, then the remaining planes. S16.4
and S28 are foundational — most other planes attest against the boot root and graded time.

## See also

- [Holistic Specification](00_REV3_HOLISTIC_SPEC.md)
- [Master Index](00_MASTER_INDEX.md)
- [Invariants](04_invariants.md)
- [Design Decisions](02_design_decisions.md)
- [Rev.2 Architecture Overview](../002.AI-OS.NET--SPECREV.2/03_architecture_overview.md)
