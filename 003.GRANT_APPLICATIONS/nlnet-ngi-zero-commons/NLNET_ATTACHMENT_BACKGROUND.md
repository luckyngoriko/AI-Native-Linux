# AI Linux OS / AI-Native Linux - Background Attachment

This attachment provides concise background information for the NLnet / NGI Zero Commons Fund application. The proposal form should remain self-contained.

## Project Links

- Project site: https://ai-os.net
- Repository: https://github.com/ai-os-dot-net/AI-Native-Linux
- License: Apache-2.0
- Applicant: Lachezar Nikolov, primary maintainer

## Project Summary

AI Linux OS / AI-Native Linux aims to create a Linux-based operating environment where AI is a native part of the system, not an external chatbot or command generator.

The long-term goal is an operating substrate where resident AI agents can live and work safely with explicit policy, sandbox, filesystem, verification, recovery and evidence boundaries.

The proposed funded work is not a full distribution. It is a bounded foundation prototype and specification hardening effort.

## Expected Outcomes

The project will deliver public open-source artifacts covering:

- hardware profiling and kernel adaptation planning
- hardened kernel profile and sandboxed build pipeline specification
- typed agent action envelopes
- default-deny policy kernel MVP
- capability runtime MVP for safe local demo actions
- evidence log and verification grammar
- AIOS-FS semantic filesystem model
- resident agent sandbox model
- compatibility safety contracts for Windows, Android and macOS application environments
- documentation and reproducible demos

## Budget Summary

Requested budget:

```text
49,500 EUR
```

Explicit rate:

```text
Maintainer engineering: 60 EUR/hour
```

Budget split:

```text
Maintainer engineering: 600h = 36,000 EUR
AI inference/API usage:        7,500 EUR
Test hardware:                 6,000 EUR
Total:                        49,500 EUR
```

No travel budget is requested.

## Main Work Packages

```text
M1 AI-native OS foundation and threat model
M2 Hardware mapping and hardened kernel planning
M3 Typed agent actions and policy kernel MVP
M4 Capability runtime and safe agent sandbox MVP
M5 Evidence log, verification grammar and agent memory boundaries
M6 AIOS-FS semantic filesystem model
M7 Compatibility contracts, documentation, demo and community readiness
```

## Hardware Use

The hardware budget is for a small reproducible test set, not general-purpose equipment.

It may cover:

- one x86_64 workstation-class test machine
- storage devices for recovery and filesystem tests
- network adapters where needed
- replacement parts and peripherals for repeated install, boot, rollback and sandbox validation

Existing self-hosted infrastructure will continue to be used for development, observability and documentation.

## AI Inference / API Use

The AI inference/API budget supports the autonomous software factory used during development.

It will be used for:

- coding agents
- review agents
- specification consistency checks
- threat-model iteration
- test generation
- documentation review
- fallback commercial API usage where needed

The budget is not intended to fund unlimited 24/7 frontier-model usage.

## Autonomous Software Factory Methodology

The project will intentionally use an AI-assisted autonomous software factory. This is part of the methodology and should be disclosed.

The factory model includes:

- a governor process selecting bounded tasks from repository state and project goals
- multiple coding workers implementing small scoped changes
- a separate QA agent reviewing outputs and flagging risks
- local and remote models depending on task type, privacy, latency and cost
- logs, metrics, tests and evidence for development activity
- human maintainer review as the authority for scope, public claims, security-sensitive changes and merge decisions

The same safety principles proposed by AI Linux OS apply to the factory itself:

- small work packages
- typed task descriptions
- policy gates for risky actions
- no uncontrolled privileged execution
- reproducible tests
- evidence logs
- rollback paths
- human approval for normative specification changes

## AI Usage Disclosure

Generative AI was intentionally used while preparing the application materials. AI tools were used to structure notes, compare scope options, draft milestone alternatives, check consistency with the public repository and refine wording.

The applicant remains responsible for the final proposal text, budget, milestones, technical claims and delivery commitments.

A prompt provenance log is maintained in the project repository as:

```text
NLNET_GENAI_PROMPT_PROVENANCE_LOG.md
```

## Why This Is Different

AI Linux OS builds on Linux, kernel hardening, sandboxing, compatibility layers and open-source AI agent work, but combines them into a different goal: an AI-native operating environment where resident agents can operate safely as part of the system.

The novel combination is:

- AI-assisted hardware adaptation
- hardened kernel planning
- safe resident agents
- AIOS-FS semantics
- typed capability actions
- policy decisions
- verification
- evidence logs
- auditable AI-assisted software factory development
