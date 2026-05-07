# AI-Native Linux

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Specification: Rev.2](https://img.shields.io/badge/specification-Rev.2-ce2867.svg)](002.AI-OS.NET--SPECREV.2/00_MASTER_INDEX.md)
[![Status: Specification First](https://img.shields.io/badge/status-specification--first-111111.svg)](002.AI-OS.NET--SPECREV.2/00_MASTER_INDEX.md)

AI-Native Linux is an open specification for a unified cognitive operating environment on top of Linux.

The goal is not to create another Linux distribution. The goal is to define an operating layer where humans express goals, the system translates those goals into typed actions, policy decides what is allowed, runtime adapters execute only approved operations, and verification produces evidence.

Public site: https://ai-os.net

Canonical repository: https://github.com/ai-os-dot-net/AI-Native-Linux

GitHub organization: https://github.com/ai-os-dot-net

## Why This Matters

Modern operating systems still treat users as command dispatchers. AI assistants can understand intent, but most current integrations collapse back into unsafe shell execution or isolated chat interfaces.

AI-Native Linux proposes a safer operating model:

- AI plans, but does not directly run arbitrary shell commands.
- System actions are typed, policy-checked, executed by capability runtimes, and verified.
- Evidence logs make system changes auditable and reproducible.
- Linux remains the trusted execution substrate.
- KDE, Web, CLI, voice, and mobile become renderers over the same cognitive core.

This matters for open source because the next generation of developer and operator tooling needs a shared safety architecture: intent, policy, typed execution, verification, recovery, and auditability.

## Current Status

This repository is currently specification-first.

There is no production runtime yet. The active work is the Rev.2 contract pack, which defines the layers, boundaries, safety model, and first implementation targets for an AI-native Linux environment.

The project is intentionally public early so security engineers, Linux operators, AI agent builders, and open-source maintainers can review the architecture before privileged runtime code exists.

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
├── site/                           # Public Astro site for ai-os.net
├── OPENAI_CODEX_OSS_APPLICATION.md # Prepared Codex for OSS application notes
├── CONTRIBUTING.md                 # Contribution guide
├── GOVERNANCE.md                   # Decision process and project rules
├── MAINTAINERS.md                  # Maintainer ownership
├── SECURITY.md                     # Security reporting and scope
├── SUPPORT.md                      # Support and discussion channels
├── LICENSE                         # Apache-2.0
└── README.md
```

## Specification Revisions

- [Rev.1 frozen](001.AI-OS.NET--SPECREV.1/00_MASTER_INDEX.md) - initial architecture vision.
- [Rev.2 active](002.AI-OS.NET--SPECREV.2/00_MASTER_INDEX.md) - layered, agent-readable operating system contract.

Rev.2 is organized as L0-L10:

- L0 - Governance, evidence, and safety
- L1 - Kernel bootstrap and recovery
- L2 - AIOS-FS semantic filesystem
- L3 - Service Graph Runtime
- L4 - Policy, identity, and vault
- L5 - Cognitive Core
- L6 - Apps, packages, and compatibility
- L7 - Interaction renderers
- L8 - Network, hardware, and devices
- L9 - Observability, admin, and operations
- L10 - Distribution, ecosystem, and marketplace

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

See [OPENAI_CODEX_OSS_APPLICATION.md](OPENAI_CODEX_OSS_APPLICATION.md) for prepared application text.

The application repository URL is:

```text
https://github.com/ai-os-dot-net/AI-Native-Linux
```

## Contributing

Contributions are welcome, but the project is still in specification phase. Start with [CONTRIBUTING.md](CONTRIBUTING.md) before opening an issue or pull request.

Security-sensitive findings should follow [SECURITY.md](SECURITY.md).

Project governance is described in [GOVERNANCE.md](GOVERNANCE.md). Maintainer ownership is listed in [MAINTAINERS.md](MAINTAINERS.md).

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
