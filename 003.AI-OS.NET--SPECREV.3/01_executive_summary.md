# Rev.3 — Executive Summary

| Field    | Value                                                         |
| -------- | ------------------------------------------------------------- |
| Status   | `CONTRACT` (created 2026-05-29)                               |
| Audience | Operator, integrator, reviewer                                |
| Scope    | What Rev.3 is, what it adds over Rev.2, and what "done" means |

## 1. What Rev.3 is

Rev.2 made AIOS a **constitutional Linux distribution**: policy kernel, append-only
evidence, sandboxed execution, bounded AI, across eleven strictly-ordered layers
(L0–L10), all `CONTRACT`/FULL-REAL.

Rev.3 turns that constitution into a **governed, AI-native operating-system platform** by
adding five planes on top of the Rev.2 primitives, without changing them:

1. **Security posture is explicit** — four profiles (`DEV_RELAXED`, `SECURE_DEFAULT`,
   `STIG_ALIGNED`, `AIRGAP_HIGH`) plus a `FIPS_STRICT` overlay, a measured-boot root of
   trust, SELinux MAC, a STIG/NIST/CIS control map, SBOM/provenance/VEX, and GDPR-grade
   data governance.
2. **Software runs as governed capsules** — every app is an `AppCapsule` with bounded blast
   radius, rollback, evidence, and an honest "blocked-with-reason"; Windows apps and games
   get dedicated Wine/Proton capsules.
3. **Kernel and runtime backends are capability-selected** — Linux is the gold path; BSD,
   RTOS, microVM, WASI, and unikernel targets are admitted through a signed capability
   matrix, never by pretending all kernels are equal.
4. **Drivers are high-risk capsules** — solved, lab-tested, signed, canary-booted, and
   rollbackable, never "run the vendor script as root".
5. **AI lives natively in the OS but is never root** — three terminal modes (`LX`/`MIX`/`AI`),
   a typed-action fabric, EU AI Act technical controls, and an evaluation/governance plane.

Linux remains the near-term delivery target for workstation, gaming, GPU/video, containers,
and hardware support; the architecture keeps every mechanism behind typed contracts so other
backends can be added later without a rewrite.

## 2. The product promise

> best viable path + explicit risk + policy approval + sandbox/recovery boundary + evidence
>
> - rollback + clear blocked reason.

AIOS never hides complexity by becoming unsafe. An operator asks for a goal; AIOS produces a
safe, explainable, rollbackable plan, and proves what happened.

## 3. What Rev.3 contains (S16–S28)

- **S16** Security Hardening & Compliance (9 sub-specs: profiles, SELinux, control map/scanner,
  measured boot, FIPS, SBOM/provenance/VEX, service hardening, zero-trust fleet, data governance/GDPR).
- **S17** App Capsule Runtime (object model, solver/lifecycle, Windows runtime, reliability/evidence, operator UI).
- **S18** Kernel Personality & Portability (capability matrix, backend adapters, adaptive kernel forge, RTOS/dual-kernel).
- **S19** Driver & Firmware Capsule Plane (driver capsules, solver, Driver Lab, canary boot, firmware coupling).
- **S20** Native AI Control Plane & AI Terminal (terminal modes, typed actions, EU AI Act controls).
- **S21** Package Rosetta & Universal App Lab (package-agnostic intake, shadow install, package passport, repo trust firewall).
- **S22** Workstation, Gaming & Video Profile (passports, runtime selectors, secure gaming, video engine scheduler, energy policy).
- **S23** Mobile Renderer & Touch Shell (phone approval console, voice renderer).
- **S24** Container & Kubernetes Native Plane (Podman/Docker/containerd/K8s, isolation levels, WASM/eBPF/Deno/Bun/Python adapters).
- **S25** Fleet, Cluster & Remote Execution (federation, federated identity, distributed evidence DAG).
- **S26** Backup, DR & Capsule Mobility (encrypted off-host backup, capsule export, personal mirror).
- **S27** AI Evaluation & Model Governance (drift/hallucination/prompt-injection-rejection evidence, multi-agent coordination).
- **S28** Constitutional Time Plane (trusted time, clock-skew detection, evidence time-trust grade).

Ten new constitutional invariants (INV-025..034) extend the Rev.2 catalog; none is weakened.

## 4. What "done" means here

This revision is **specification-complete**: every section is `CONTRACT` grade — closed
schemas, closed enums, lifecycle FSMs, evidence record types, security-profile gates, and
testable acceptance criteria — with clean Capella traceability. An engineer can begin
implementation against any section with no undefined dependency.

It is **not** yet `REAL`. As in Rev.2, `REAL` requires implementation evidence (build,
test, e2e, live) delivered by the implementation milestones the waves (§5 of the master
index) describe. The honesty rule is unchanged: no capability is `REAL` without evidence.

## 5. What Rev.3 deliberately does not promise

- Not "another Linux distro" and not "a shell copilot".
- Not that every package, driver, kernel, or foreign app will always run — only that each
  gets a governed plan, rollback, evidence, and a clear blocked reason.
- No certification claims (DoD/STIG/FIPS/Common Criteria/EU AI Act) without a real assessment;
  the contracts provide technical alignment and audit material, not legal conformity.
- AI never becomes an unbounded root agent.

## See also

- [Master Index](00_MASTER_INDEX.md)
- [Holistic Specification](00_REV3_HOLISTIC_SPEC.md)
- [Architecture Overview](03_architecture_overview.md)
- [Design Decisions](02_design_decisions.md)
