# Rev.3 — Master Index

| Field       | Value                                                                         |
| ----------- | ----------------------------------------------------------------------------- |
| Status      | `CONTRACT` (created 2026-05-29; navigation + scope ledger for Rev.3)          |
| Predecessor | `002.AI-OS.NET--SPECREV.2/00_MASTER_INDEX.md` (Rev.2, CONTRACT/FULL-REAL)     |
| Scope       | S16–S28 + cross-cutting themes; built additively on Rev.2 L0–L10              |
| Companion   | `00_REV3_HOLISTIC_SPEC.md` (architecture), `00_PLANNING_NOTES.md` (rationale) |

This is the navigation entry point and the **complete scope ledger** for Rev.3. Every
scope item has exactly one row with a materialization status. Nothing is "planning-only"
at this revision: the planning-notes themes are each homed in a contract (see §4).

Status legend: `CONTRACT` = fully specified (closed schemas, evidence records, acceptance
criteria), ready for implementation; `REAL` requires implementation evidence (E2+) from a
later milestone, exactly as in Rev.2. Evidence grade for all Rev.3 sections today is **E0**
(spec artifact only) → status `CONTRACT`.

## 1. Foundation documents

| File                                                         | Purpose                                                                                  |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------- |
| [`00_REV3_HOLISTIC_SPEC.md`](00_REV3_HOLISTIC_SPEC.md)       | Master architecture; thesis, planes, state objects, solver pattern, acceptance criteria. |
| [`00_PLANNING_NOTES.md`](00_PLANNING_NOTES.md)               | Technical scoping rationale and the brainstorm menu that fed the contracts.              |
| [`00_REV3_GAP_REPORT.md`](00_REV3_GAP_REPORT.md)             | The completeness audit that drove the 2026-05-29 build-out (now resolved).               |
| [`01_executive_summary.md`](01_executive_summary.md)         | Operator-facing summary of what Rev.3 is and delivers.                                   |
| [`02_design_decisions.md`](02_design_decisions.md)           | ADR log (DEC-R3-001..011) — resolves the open questions and structural choices.          |
| [`03_architecture_overview.md`](03_architecture_overview.md) | Plane-by-plane architecture and layer mapping.                                           |
| [`04_invariants.md`](04_invariants.md)                       | Rev.3 constitutional invariants INV-025..034 (extends Rev.2 INV-001..024).               |

## 2. Materialized contracts (S16–S28)

### S16 — Security Hardening and Compliance (decomposed)

| Sub-spec                                                                         | Topic                                                                       | Status     |
| -------------------------------------------------------------------------------- | --------------------------------------------------------------------------- | ---------- |
| [S16 overview](S16_Security_Hardening_Compliance/00_overview.md)                 | Plane responsibility, compliance boundary, baselines.                       | `CONTRACT` |
| [S16.1](S16_Security_Hardening_Compliance/01_security_profile_matrix.md)         | `SecurityProfile` matrix, transition gates.                                 | `CONTRACT` |
| [S16.2](S16_Security_Hardening_Compliance/02_selinux_mac_policy_plane.md)        | SELinux domains, MCS/MLS, policy-bundle lifecycle, AVC evidence.            | `CONTRACT` |
| [S16.3](S16_Security_Hardening_Compliance/03_stig_nist_control_map_scanner.md)   | STIG/NIST/CIS control map, scanner, exception register, export.             | `CONTRACT` |
| [S16.4](S16_Security_Hardening_Compliance/04_measured_boot_runtime_integrity.md) | Measured boot, TPM, IMA/EVM, dm-verity/IPE, attestation (P0 root of trust). | `CONTRACT` |
| [S16.5](S16_Security_Hardening_Compliance/05_fips_crypto_boundary.md)            | `FIPS_STRICT` overlay, CMVP boundary, crypto evidence.                      | `CONTRACT` |
| [S16.6](S16_Security_Hardening_Compliance/06_sbom_provenance_vex.md)             | SBOM (SPDX/CycloneDX), SLSA provenance, VEX, reproducible-build receipt.    | `CONTRACT` |
| [S16.7](S16_Security_Hardening_Compliance/07_service_hardening_score_gates.md)   | systemd hardening score floors, promotion gates.                            | `CONTRACT` |
| [S16.8](S16_Security_Hardening_Compliance/08_zero_trust_fleet_posture.md)        | NIST 800-207 zero-trust posture, continuous posture checks.                 | `CONTRACT` |
| [S16.9](S16_Security_Hardening_Compliance/09_data_governance_gdpr.md)            | GDPR/RTBF crypto-shred, data residency, audit export.                       | `CONTRACT` |

### S17 — App Capsule Runtime (decomposed)

| Sub-spec                                                             | Topic                                                                           | Status     |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------- | ---------- |
| [S17 overview](S17_App_Capsule_Runtime/00_overview.md)               | Capsule plane responsibility, layer discipline.                                 | `CONTRACT` |
| [S17.1](S17_App_Capsule_Runtime/01_capsule_object_model.md)          | `AIOSAppObject`, `AppCapsule`, capsule types, layout, data/capability contract. | `CONTRACT` |
| [S17.2](S17_App_Capsule_Runtime/02_capsule_solver_lifecycle.md)      | Capsule Solver, lifecycle FSM, install/update/repair/export flows.              | `CONTRACT` |
| [S17.3](S17_App_Capsule_Runtime/03_windows_capsule_runtime.md)       | Windows/Wine/Proton capsule, prefixes, deps, anti-cheat, VM fallback.           | `CONTRACT` |
| [S17.4](S17_App_Capsule_Runtime/04_reliability_security_evidence.md) | Health/snapshot/rollback, Capsule Doctor, canonical capsule evidence enum.      | `CONTRACT` |
| [S17.5](S17_App_Capsule_Runtime/05_operator_ui_acceptance.md)        | Operator UI flows, risk diff, "why blocked", acceptance.                        | `CONTRACT` |

### S18–S28 — single-file contract overviews (per DEC-R3-008)

| Section                                                     | Topic                                                                                                               | Status     |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- | ---------- |
| [S18](S18_Kernel_Personality_Portability/00_overview.md)    | Kernel personality, capability matrix, backend adapters, adaptive forge (`KernelBuildCandidate`), RTOS/dual-kernel. | `CONTRACT` |
| [S19](S19_Driver_Firmware_Capsule_Plane/00_overview.md)     | Driver capsules, solver, Driver Lab, canary boot, firmware coupling, rollback.                                      | `CONTRACT` |
| [S20](S20_Native_AI_Control_Plane_Terminal/00_overview.md)  | Native AI control plane, LX/MIX/AI terminal, typed-action fabric, EU AI Act controls.                               | `CONTRACT` |
| [S21](S21_Package_Rosetta_Universal_App_Lab/00_overview.md) | Package Rosetta, Universal App Lab, shadow install, `PackagePassport`, repo trust firewall.                         | `CONTRACT` |
| [S22](S22_Workstation_Gaming_Video_Profile/00_overview.md)  | Workstation/Game/Video passports, runtime selectors, energy policy.                                                 | `CONTRACT` |
| [S23](S23_Mobile_Renderer_Touch_Shell/00_overview.md)       | Mobile renderer + phone approval console, voice renderer.                                                           | `CONTRACT` |
| [S24](S24_Container_Kubernetes_Native_Plane/00_overview.md) | Podman/Docker/containerd/K8s, isolation levels, ecosystem runtime adapters.                                         | `CONTRACT` |
| [S25](S25_Fleet_Cluster_Remote_Execution/00_overview.md)    | Fleet/cluster, remote workload routing, federated identity, distributed evidence DAG.                               | `CONTRACT` |
| [S26](S26_Backup_DR_Capsule_Mobility/00_overview.md)        | Backup/DR, capsule export/import, personal mirror, airgap store.                                                    | `CONTRACT` |
| [S27](S27_AI_Evaluation_Model_Governance/00_overview.md)    | AI evaluation harness, model governance, multi-agent coordination.                                                  | `CONTRACT` |
| [S28](S28_Constitutional_Time_Plane/00_overview.md)         | Trusted time sources, clock-skew detection, evidence time-trust grade.                                              | `CONTRACT` |

## 3. Layer mapping (Rev.3 sections → Rev.2 L0–L10)

Rev.3 adds no new layers. Each section is cross-cutting over the Rev.2 layer model and
depends downward only (INV-007):

| Section     | Primary layers                   |
| ----------- | -------------------------------- |
| S16 (.1–.9) | L0, L1, L4, L8, L9, L10          |
| S17         | L6 (crossing L2, L4, L7, L8, L9) |
| S18         | L1 (crossing L2, L3, L4, L6, L8) |
| S19         | L1, L8 (crossing L2, L4, L6, L9) |
| S20         | L5, L7 (crossing L0, L4, L9)     |
| S21         | L6, L10 (crossing L2, L4, L9)    |
| S22         | L6, L7, L8 (crossing L4, L9)     |
| S23         | L7 (crossing L4, L5, L9)         |
| S24         | L6, L8 (crossing L4, L9)         |
| S25         | L8, L4, L10 (crossing L0, L9)    |
| S26         | L2, L9, L10 (crossing L4)        |
| S27         | L5, L0 (crossing L4, L9)         |
| S28         | L0, L1, L9 (crossing L8)         |

## 4. Cross-cutting theme homing (per DEC-R3-011)

Every planning-notes cross-cutting theme has exactly one owning contract:

| Theme                                                  | Home                        |
| ------------------------------------------------------ | --------------------------- |
| Federated identity                                     | S25 (INV-032)               |
| Multi-agent coordination                               | S27 (+ S20 actor kinds)     |
| Ecosystem runtime adapters (WASM/eBPF/Deno/Bun/Python) | S24 (eBPF → INV-025)        |
| Voice renderer                                         | S23                         |
| Energy / power policy                                  | S22                         |
| GDPR / RTBF + audit export                             | S16.9 (INV-027)             |
| Trusted-time constitutional plane                      | S28 (INV-034)               |
| TPM 2.0 attestation                                    | S16.4 (INV-028, DEC-R3-002) |
| Zero-trust fleet                                       | S16.8 + S25 (INV-026)       |
| SBOM / provenance / VEX                                | S16.6                       |
| FIPS crypto boundary                                   | S16.5                       |

## 5. Implementation waves (from holistic §13, with section bindings)

| Wave  | Scope                                      | Sections                |
| ----- | ------------------------------------------ | ----------------------- |
| R3-W1 | Security profile + scanner + measured boot | S16.1, S16.3, S16.4     |
| R3-W2 | App capsule manifest + solver              | S17.1, S17.2            |
| R3-W3 | AI terminal + typed action fabric          | S20                     |
| R3-W4 | Driver capsule + solver                    | S19                     |
| R3-W5 | Kernel capability matrix + adapters        | S18                     |
| R3-W6 | Package Rosetta                            | S21                     |
| R3-W7 | Mobile / workstation / gaming / video      | S22, S23                |
| R3-W8 | Containers/K8s, fleet, backup, time, eval  | S24, S25, S26, S27, S28 |

## 6. Traceability

The Capella model (`tools/capella/output/aios-rev3/`) and manifests
(`tools/capella/manifests/`) register every Rev.3 section, its `Consumes` edges, its
`Produces` objects, and its evidence record types. Holistic acceptance criterion §16.7
(as revised) is the pass gate: zero orphan invariants, zero consumes cycles, zero forbidden
runtime inversions, zero uncertain inversions, zero orphan sub-specs, and ≥1 emitter trace
per declared Rev.3 record type.

## See also

- [Rev.3 Holistic Specification](00_REV3_HOLISTIC_SPEC.md)
- [Rev.3 Design Decisions](02_design_decisions.md)
- [Rev.3 Invariants](04_invariants.md)
- [Rev.2 Master Index](../002.AI-OS.NET--SPECREV.2/00_MASTER_INDEX.md)
