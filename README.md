# AI-Native Linux

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Specification: Rev.2](https://img.shields.io/badge/specification-Rev.2-ce2867.svg)](002.AI-OS.NET--SPECREV.2/00_MASTER_INDEX.md)
[![Status: Specification First](https://img.shields.io/badge/status-specification--first-111111.svg)](002.AI-OS.NET--SPECREV.2/00_MASTER_INDEX.md)
[![Layers: 11](https://img.shields.io/badge/layers-11-111111.svg)](002.AI-OS.NET--SPECREV.2/00_MASTER_INDEX.md)
[![Sub-specs: 53](https://img.shields.io/badge/sub--specs-53-111111.svg)](002.AI-OS.NET--SPECREV.2/00_MASTER_INDEX.md)
[![Invariants: 24](https://img.shields.io/badge/invariants-24-ce2867.svg)](002.AI-OS.NET--SPECREV.2/L0_Governance_Evidence_Safety/04_invariants.md)
[![Record Types: 427](https://img.shields.io/badge/record--types-427-111111.svg)](002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/01_evidence_log.md)

AI-Native Linux (AIOS) is an open specification for an **AI-native Linux distribution** — a real Linux distro whose distinguishing layer is a cognitive core, a policy kernel, and an evidence log built on top of the standard Linux substrate.

It is a Linux distribution, not "Linux + an AI assistant bolted on." Humans express goals, the system translates those goals into typed actions, the policy kernel decides what is allowed, runtime adapters execute only approved operations, and verification produces append-only evidence. Linux remains the trusted execution substrate (kernel, drivers, scheduler, syscalls); the AIOS layer is what makes the distribution AI-native.

Public site: https://ai-os.net

Canonical repository: https://github.com/ai-os-dot-net/AI-Native-Linux

GitHub organization: https://github.com/ai-os-dot-net

## Specification at a Glance

| Metric                    | Value                               | Where it lives                                                                                                   |
| ------------------------- | ----------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Architectural layers      | 11 (L0–L10 + XX cross-cutting)      | [Rev.2 master index](002.AI-OS.NET--SPECREV.2/00_MASTER_INDEX.md)                                                |
| Contract sub-specs        | 53                                  | one folder per layer                                                                                             |
| Constitutional invariants | 24                                  | [L0 invariants](002.AI-OS.NET--SPECREV.2/L0_Governance_Evidence_Safety/04_invariants.md)                         |
| Typed Record types        | 427 (closed enum)                   | [L9 evidence log master enum](002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/01_evidence_log.md)     |
| Verification properties   | 32 (closed enum)                    | [L9 verification grammar](002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/02_verification_grammar.md) |
| INV-002 enforcement       | 6 mechanical sites                  | "AI proposes, never executes" — wired into the IDL, not stated as a maxim                                        |
| Wave consolidations       | 13 closed (W1–W13)                  | last sweep: IDL roll-up of 22 → 428 RecordType entries                                                           |
| Tier 5 audit              | Near-converged, SIM-D zero-findings | 9 read-only audit/simulation agents over the constitutional core                                                 |
| Total spec mass           | 70 markdown files, 52,000+ lines    | English artifacts, agent-readable                                                                                |

## Why This Matters

Modern operating systems still treat users as command dispatchers. AI assistants can understand intent, but most current integrations collapse back into unsafe shell execution or isolated chat interfaces.

AI-Native Linux proposes a safer operating model:

- AI plans, but does not directly run arbitrary shell commands.
- System actions are typed, policy-checked, executed by capability runtimes, and verified.
- Evidence logs make system changes auditable and reproducible.
- Linux remains the trusted execution substrate.
- KDE, Web, CLI, voice, and mobile become renderers over the same cognitive core.

This matters for open source because the next generation of developer and operator tooling needs a shared safety architecture: intent, policy, typed execution, verification, recovery, and auditability.

## Distribution Model

AIOS is being built as a real Linux distribution, not a userspace add-on. The distribution components mapped onto the layer model:

| Distribution component       | Where it lives in AIOS                                                                                                                                                           |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Boot + recovery              | [L1 Kernel & Bootstrap](002.AI-OS.NET--SPECREV.2/L1_Kernel_Bootstrap_Recovery/00_overview.md) — fallback kernel, recovery without LLM                                            |
| Filesystem layout            | [L2 AIOS-FS](002.AI-OS.NET--SPECREV.2/L2_AIOS_FS/00_overview.md) — `/aios` semantic object store + recovery-safe `/` and operator `/root`                                        |
| Service / package management | [L3 SGR](002.AI-OS.NET--SPECREV.2/L3_AIOS_SGR_Service_Graph_Runtime/00_overview.md) + [L6 Apps/Packages](002.AI-OS.NET--SPECREV.2/L6_Apps_Packages_Compatibility/00_overview.md) |
| Authentication / identity    | [L4 Policy/Identity/Vault](002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/00_overview.md)                                                                                     |
| Default desktop              | [L7 Renderers](002.AI-OS.NET--SPECREV.2/L7_Interaction_Renderers/00_overview.md) — KDE Plasma + Qt/QML; Web/CLI/Voice/Mobile as siblings                                         |
| Networking + drivers         | [L8 Network/Hardware](002.AI-OS.NET--SPECREV.2/L8_Network_Hardware_Devices/00_overview.md)                                                                                       |
| Telemetry / admin            | [L9 Observability/Admin/Ops](002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/00_overview.md)                                                                          |
| Repos + marketplace          | [L10 Distribution/Ecosystem](002.AI-OS.NET--SPECREV.2/L10_Distribution_Ecosystem_Marketplace/00_overview.md) — signed bundles, three-tier trust chain (S11.1)                    |
| **Distinguishing AI layer**  | [L5 Cognitive Core](002.AI-OS.NET--SPECREV.2/L5_Cognitive_Core/00_overview.md) + [XX Action Envelope](002.AI-OS.NET--SPECREV.2/XX_Cross_Cutting/01_action_envelope_lifecycle.md) |

What is locked: the distribution exposes a typed action surface over Linux; AI is a first-class subject with mechanically restricted authority (INV-002 at six sites); evidence is append-only; recovery never depends on LLMs.

What is intentionally open at this stage: base distribution lineage (Debian / Arch / Fedora / from-scratch), installer ISO format, packaging format, and dedicated kernel pipeline shape. These are deliberate decisions left for the implementation phase so the constitutional contracts settle first.

## Current Status

This repository is currently specification-first.

There is no production runtime yet. The active work is the Rev.2 contract pack, which defines the layers, boundaries, safety model, and first implementation targets for the AI-native Linux distribution.

The project is intentionally public early so security engineers, Linux operators, AI agent builders, and open-source maintainers can review the architecture before privileged distribution code exists.

## Core Architecture

```text
Human Goal
   |
Unified Cognitive Shell
   |
Cognitive Core
   |
Policy Kernel
   |
Capability Runtime
   |
Linux / Containers / Network / Devices
   |
Verified Evidence
```

Correct execution model:

```text
AI -> Action Plan -> Policy Check -> Typed Runtime -> Verified Result
```

Rejected execution model:

```text
AI -> sudo bash
```

## Repository Layout

```text
.
├── 001.AI-OS.NET--SPECREV.1/      # Frozen Rev.1 vision and specification
├── 002.AI-OS.NET--SPECREV.2/      # Active Rev.2 layered contract pack
├── 003.GRANT_APPLICATIONS/         # Public grant application records
├── site/                           # Public Astro site for ai-os.net
├── CONTRIBUTING.md                 # Contribution guide
├── GOVERNANCE.md                   # Decision process and project rules
├── MAINTAINERS.md                  # Maintainer ownership
├── SECURITY.md                     # Security reporting and scope
├── SUPPORT.md                      # Support and discussion channels
├── LICENSE                         # Apache-2.0
└── README.md
```

## Specification Revisions

- [Rev.1 frozen](001.AI-OS.NET--SPECREV.1/00_MASTER_INDEX.md) — initial architecture vision (do not edit).
- [Rev.2 active](002.AI-OS.NET--SPECREV.2/00_MASTER_INDEX.md) — layered, agent-readable operating system contract.

Rev.2 is organized as L0–L10. A layer may depend on its own layer and lower-numbered layers; it must not require a higher-numbered layer for correctness.

| Layer   | Folder                                                                                                                 | Owns                                                                           |
| ------- | ---------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| **L0**  | [Governance, Evidence, Safety](002.AI-OS.NET--SPECREV.2/L0_Governance_Evidence_Safety/00_overview.md)                  | Status taxonomy, evidence grades, invariants, gates                            |
| **L1**  | [Kernel, Bootstrap, Recovery](002.AI-OS.NET--SPECREV.2/L1_Kernel_Bootstrap_Recovery/00_overview.md)                    | Linux substrate, host bootstrap, fallback kernel, recovery path                |
| **L2**  | [AIOS-FS](002.AI-OS.NET--SPECREV.2/L2_AIOS_FS/00_overview.md)                                                          | Semantic object filesystem at `/aios`, versions, views, transactions           |
| **L3**  | [SGR — Service Graph Runtime](002.AI-OS.NET--SPECREV.2/L3_AIOS_SGR_Service_Graph_Runtime/00_overview.md)               | Desired-state service graph, typed action lifecycle, adapter model             |
| **L4**  | [Policy, Identity, Vault](002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/00_overview.md)                            | Policy Kernel, identity, Vault Broker (secrets-as-capabilities)                |
| **L5**  | [Cognitive Core](002.AI-OS.NET--SPECREV.2/L5_Cognitive_Core/00_overview.md)                                            | Intent translation, planning, memory, model routing, agent coordination        |
| **L6**  | [Apps, Packages, Compatibility](002.AI-OS.NET--SPECREV.2/L6_Apps_Packages_Compatibility/00_overview.md)                | Sandboxed runtimes for Linux/Windows/Android apps and packages                 |
| **L7**  | [Interaction Renderers](002.AI-OS.NET--SPECREV.2/L7_Interaction_Renderers/00_overview.md)                              | KDE Plasma, Web, CLI, Voice, Mobile over a shared UI schema                    |
| **L8**  | [Network, Hardware, Devices](002.AI-OS.NET--SPECREV.2/L8_Network_Hardware_Devices/00_overview.md)                      | Network policy, hardware graph, drivers                                        |
| **L9**  | [Observability, Admin, Operations](002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/00_overview.md)          | Evidence log, verification grammar, telemetry, recovery operations             |
| **L10** | [Distribution, Ecosystem, Marketplace](002.AI-OS.NET--SPECREV.2/L10_Distribution_Ecosystem_Marketplace/00_overview.md) | Distribution, ecosystem, marketplace, signed bundles                           |
| **XX**  | [Cross-cutting contracts](002.AI-OS.NET--SPECREV.2/XX_Cross_Cutting/)                                                  | Action envelope lifecycle, ProxGuard reference, contracts shared across layers |

## Quick Read for Reviewers

If you have 15 minutes:

1. [Executive summary](002.AI-OS.NET--SPECREV.2/01_executive_summary.md)
2. [Architecture overview](002.AI-OS.NET--SPECREV.2/03_architecture_overview.md)
3. [L0 invariants](002.AI-OS.NET--SPECREV.2/L0_Governance_Evidence_Safety/04_invariants.md) — the constitutional core
4. [XX action envelope lifecycle](002.AI-OS.NET--SPECREV.2/XX_Cross_Cutting/01_action_envelope_lifecycle.md) — how a goal becomes a verified system change
5. [L4 policy kernel](002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/01_policy_kernel.md) — how the system says no
6. [L9 evidence log](002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/01_evidence_log.md) — append-only audit trail with the 427-entry RecordType enum

If you have an afternoon: walk the [Rev.2 master index](002.AI-OS.NET--SPECREV.2/00_MASTER_INDEX.md) top-to-bottom; every sub-spec is contract-grade.

## First Reference App Candidate

[ProxGuard](002.AI-OS.NET--SPECREV.2/XX_Cross_Cutting/02_proxguard_reference_model.md) is documented as the first candidate AIOS system app.

It can become an optional infrastructure app under:

```text
/aios/apps/proxguard
```

It should expose typed service, DNS, gateway, and audit capabilities while AIOS keeps authority in policy, vault, sandboxing, verification, and evidence.

## OpenAI Codex for OSS Fit

This project is a strong fit for Codex-assisted open-source development because it needs:

- large specification maintenance
- cross-layer consistency checks
- threat-model review
- Rust/Python reference implementation work
- policy and sandbox contract generation
- documentation automation
- release and audit workflow automation

See [OpenAI Codex OSS application notes](003.GRANT_APPLICATIONS/openai-codex-oss/OPENAI_CODEX_OSS_APPLICATION.md) for the submitted application record.

The application repository URL is:

```text
https://github.com/ai-os-dot-net/AI-Native-Linux
```

## Grant Applications

Public grant application records are kept under [003.GRANT_APPLICATIONS](003.GRANT_APPLICATIONS):

- [OpenAI Codex for OSS](003.GRANT_APPLICATIONS/openai-codex-oss/OPENAI_CODEX_OSS_APPLICATION.md)
- [NLnet / NGI Zero Commons Fund](003.GRANT_APPLICATIONS/nlnet-ngi-zero-commons/NLNET_SUBMISSION_STATUS.md)

## Contributing

Contributions are welcome, but the project is still in specification phase. Start with [CONTRIBUTING.md](CONTRIBUTING.md) before opening an issue or pull request.

Security-sensitive findings should follow [SECURITY.md](SECURITY.md).

Project governance is described in [GOVERNANCE.md](GOVERNANCE.md). Maintainer ownership is listed in [MAINTAINERS.md](MAINTAINERS.md).

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
