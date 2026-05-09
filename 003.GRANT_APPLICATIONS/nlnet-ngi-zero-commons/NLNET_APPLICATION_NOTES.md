# NLnet / NGI Zero Commons Fund Application Notes

This is a working notes document for preparing an NLnet / NGI Zero Commons Fund application.

This file may be used as part of the proposal preparation record.

## AI Usage Handling

These notes were prepared with AI assistance. The application strategy is disclosure-first: if AI is used to draft, refine, translate, or structure proposal text, the final NLnet submission should say so explicitly.

Recommended disclosure process:

1. Keep this file as the proposal preparation record.
2. Keep a short record of the models used, dates, prompts, and substantial unedited outputs.
3. The maintainer remains responsible for the final text, budget, milestones, technical claims, and delivery commitments.
4. The final submission should disclose that AI was intentionally used for project structuring, drafting support, consistency checks, and milestone planning.
5. Do not present AI-generated text as purely human-written text.

Suggested disclosure wording:

```text
Generative AI was intentionally used while preparing this proposal. The tools were used to structure project notes, compare scope options, draft milestone alternatives, check consistency with the public repository, and refine wording. The applicant reviewed the final proposal and remains responsible for all technical claims, budget, milestones, and delivery commitments. The project itself also proposes an auditable AI-assisted software factory model, so transparent AI use is part of the methodology rather than hidden authorship.
```

Preparation record to keep:

- date of AI-assisted preparation
- model/tool name
- short description of prompts
- substantial unedited outputs that influenced the final proposal
- final human-reviewed submitted text

Detailed prompt provenance log:

```text
NLNET_GENAI_PROMPT_PROVENANCE_LOG.md
```

## AI-Assisted Preparation Log

This section records the intentional use of AI in preparing the application.

Current preparation date:

```text
2026-05-08
```

Tools/models used during preparation:

```text
OpenAI Codex / ChatGPT session in the public project repository workspace.
Other project AI systems may be used later and should be added here before submission.
```

Prompt themes used so far:

- identify suitable open-source grant programs for AI-Native Linux
- compare applicant models: individual maintainer vs organisation
- inspect NLnet / NGI Zero Commons Fund conditions
- define a realistic first-project scope for a 49,500 EUR application
- refine the project vision from "safe administration layer" into "AI-native Linux operating environment"
- include intentional AI usage and autonomous software factory methodology in the proposal preparation notes
- adapt lessons from the existing NeuroCAD autonomous factory without publishing private infrastructure details

Substantial unedited output to preserve:

```text
The evolving content of this NLNET_APPLICATION_NOTES.md file is the current preparation artifact. Prompt/output entries that influenced form answers are recorded in NLNET_GENAI_PROMPT_PROVENANCE_LOG.md.
```

## Target Call

Submission status:

```text
Submitted
```

Submission date:

```text
2026-05-08
```

NLnet application code:

```text
2026-06-162
```

Call:

```text
NGI Zero Commons Fund
```

Deadline:

```text
2026-06-01 12:00 CEST
```

Submission form:

```text
https://nlnet.nl/propose/
```

Guide:

```text
https://nlnet.nl/commonsfund/guideforapplicants/
```

Eligibility:

```text
https://nlnet.nl/commonsfund/eligibility/
```

FAQ:

```text
https://nlnet.nl/commonsfund/faq/
```

## Applicant Model

Applicant:

```text
Lachezar Nikolov
```

Applicant type:

```text
Individual primary maintainer
```

Project organization:

```text
https://github.com/ai-os-dot-net
```

Repository:

```text
https://github.com/ai-os-dot-net/AI-Native-Linux
```

Project site:

```text
https://ai-os.net
```

License:

```text
Apache-2.0
```

## Proposed Project

Working title:

```text
AI-Native Linux Foundation Prototype
```

Short positioning:

```text
AI-Native Linux aims to create a Linux environment where AI is a native part of the operating system rather than an external chatbot or command generator. The system should support safe resident agents, hardware-adapted kernels, AI-native filesystem semantics, and controlled compatibility layers for existing application ecosystems.
```

Expected outcome:

```text
A public, open-source foundation prototype and hardened specification for the first AI-native Linux substrate: installation-time hardware mapping, adaptive hardened kernel planning, safe agent execution boundaries, typed capability actions, evidence logs, AIOS-FS semantics, and compatibility contracts for Windows, Android, and macOS application environments.
```

## Full Project Vision

AI-Native Linux is not meant to be a conventional Linux distribution with an AI assistant added on top.

The long-term project vision is a Linux-based operating environment where:

- the base installer boots on broad hardware with a generic kernel
- after installation, AI maps the exact machine hardware, firmware, devices, buses, drivers, and threat surface
- the system prepares a dedicated host-specific Linux kernel configuration
- the kernel is built in a sandbox with reproducible evidence
- security hardening is applied according to recognised hardening guidance and measurable local policy
- the system keeps a reliable recovery path through the original base environment
- AI agents live as first-class operating participants, but only inside explicit policy, sandbox, filesystem, and capability boundaries
- the filesystem is designed for AI-era operation: semantic metadata, evidence, memory, app work areas, versioning, conflict handling, and recovery
- applications from Linux, Windows, Android, and macOS ecosystems are handled through controlled compatibility environments rather than uncontrolled host access

The first NLnet application should present this as the larger direction, while asking funding only for a bounded foundation prototype.

## Autonomous Software Factory Methodology

The project will intentionally use an AI-assisted autonomous software factory to deliver part of the work.

This is not hidden authorship. It is part of the project method and should be disclosed in the application.

The maintainer already operates a comparable autonomous factory pattern in another open-source engineering project. The relevant method, abstracted for AI-Native Linux, is:

- a governor process selects the next bounded task from repository state and project goals
- multiple coding workers implement small, scoped changes in parallel
- a separate QA agent reviews outputs, checks consistency, and flags risks
- local and remote models may be combined depending on task type, privacy, latency, and cost
- failures, test results, worker outputs, and decisions are written as operational evidence
- lightweight patrol jobs continuously check repository health without exhausting the machine
- metrics and logs are collected through an observability stack
- human maintainer review remains the authority for scope, merge decisions, budget, and public claims

For this NLnet project, the autonomous factory should be constrained by the same principles that AI-Native Linux itself proposes:

- small work packages
- typed task descriptions
- policy gates for risky actions
- no uncontrolled privileged execution
- reproducible tests
- evidence logs
- rollback paths
- human maintainer approval for normative specification changes

The factory is expected to accelerate:

- specification consistency checks
- threat model iteration
- Rust prototype scaffolding
- policy rule examples
- verification grammar test generation
- documentation updates
- issue triage and community response preparation

The factory must not be used as an excuse to overclaim maturity. The proposal should still promise only a bounded foundation prototype.

## Fit With NGI Zero Commons Fund

Relevant NGI themes:

- trustworthy digital infrastructure
- user control and data sovereignty
- open source software
- secure and auditable system behavior
- privacy-preserving local execution
- robust software development and deployment practices
- open digital commons

Why it fits:

- The project addresses a growing ecosystem gap: current operating systems do not have a native, safe place for resident AI agents.
- It proposes shared open infrastructure for AI-native personal, workstation, and self-hosted computing.
- Linux remains the execution substrate, but AI becomes a governed operating layer with explicit authority boundaries.
- The project combines local user control, hardware adaptation, security hardening, agent isolation, semantic filesystem design, and auditable system changes.
- The result can benefit developers, Linux operators, self-hosters, privacy-focused users, public-interest infrastructure, and future open-source AI agent tooling.

## Suggested Scope

The first NLnet proposal should be a bounded prototype, not the entire operating system.

In scope:

- Rev.2 specification hardening for the AI-native operating substrate
- hardware inventory and kernel adaptation planning contract
- hardened kernel profile design and sandboxed build pipeline specification
- Rust prototype for typed action envelope lifecycle
- Policy Kernel MVP with default-deny decisions for agent actions
- Capability Runtime MVP for safe local demo actions
- Evidence log prototype
- Verification grammar prototype
- AIOS-FS semantic object and authority model validation
- agent sandbox boundary model
- compatibility environment contract for Windows, Android, and macOS application support
- autonomous software factory workflow for implementation, QA, evidence collection, and public progress tracking
- documentation and reproducible demos

Out of scope for this first proposal:

- full Linux distribution
- production custom kernel build system
- production package manager
- production desktop shell
- full AIOS-FS implementation
- production Windows/Android/macOS compatibility layer
- cloud marketplace
- fully unattended production changes without maintainer review

## Hardening Baselines To Evaluate

The project should not claim "perfect security." It should describe a measurable hardening pipeline that combines established guidance with host-specific validation.

Hardening references to evaluate:

- Linux Kernel Self-Protection Project principles
- CIS Linux Benchmarks
- DISA STIG profiles for Linux distributions where applicable
- OpenSCAP / SCAP Security Guide style auditing
- Linux Security Modules such as SELinux, AppArmor, Landlock, and related confinement mechanisms
- seccomp syscall filtering
- kernel lockdown mode
- Secure Boot and measured boot concepts
- TPM-backed attestation where available
- IMA/EVM file integrity concepts
- compiler and kernel configuration hardening options
- least functionality, least privilege, auditability, and rollback principles

Final implementation choices should be host-specific and testable. The AI may propose hardening changes, but policy and sandbox gates must control what is applied.

## Suggested Budget

Recommended requested amount:

```text
49,500 EUR
```

Reason:

- First NLnet proposals may request up to 50,000 EUR.
- This amount stays within the first-proposal boundary.
- It is enough for a serious 9-12 month specification and prototype effort.

Suggested duration:

```text
9-12 months
```

## Milestone Draft

### M1 - AI-Native OS Foundation and Threat Model

Budget:

```text
7,500 EUR
```

Deliverables:

- revised Rev.2 foundation model for AI-native Linux
- threat model for resident agents, kernel adaptation, filesystem authority, compatibility environments, and system actions
- explicit authority boundaries for shell, files, services, packages, network, devices, secrets, and compatibility layers
- public documentation updates

### M2 - Hardware Mapping and Hardened Kernel Planning

Budget:

```text
10,500 EUR
```

Deliverables:

- hardware inventory schema for CPU, memory, storage, GPU, network, buses, firmware, and peripherals
- kernel adaptation planning contract
- hardened kernel profile model
- sandboxed build pipeline design
- recovery path requirements
- test fixtures based on representative hardware profiles

### M3 - Typed Agent Action and Policy Kernel MVP

Budget:

```text
9,000 EUR
```

Deliverables:

- Rust crate for action envelope schema
- lifecycle states and validation rules
- idempotency and dry-run fields
- default-deny policy decision engine
- allow/deny/approval-required decision model
- request-bound approval semantics
- policy test fixtures
- example rules for package, service, file, network, kernel-plan, and compatibility-environment actions

### M4 - Capability Runtime and Safe Agent Sandbox MVP

Budget:

```text
7,500 EUR
```

Deliverables:

- safe adapter boundary for demo actions
- no direct AI-to-shell execution
- resident agent sandbox model
- simulated and real low-risk local actions
- execution receipts
- rollback notes where applicable

### M5 - Evidence Log, Verification Grammar, and Agent Memory Boundaries

Budget:

```text
6,500 EUR
```

Deliverables:

- append-only evidence record format
- verification grammar for service status, file state, command result, and network reachability
- evidence model for AI-proposed kernel/profile changes
- memory boundary notes for agents
- public examples
- tests for evidence consistency

### M6 - AIOS-FS Semantic Filesystem Model

Budget:

```text
5,500 EUR
```

Deliverables:

- validated filesystem authority model for `/aios`
- agent and app work-cell model
- semantic metadata and versioning model
- conflict and rollback notes
- read-only base assumptions
- sandbox boundary documentation
- implementation notes for future AIOS-FS prototype work

### M7 - Compatibility Contracts, Documentation, Demo, and Community Readiness

Budget:

```text
3,000 EUR
```

Deliverables:

- compatibility contract notes for Windows, Android, and macOS application environments
- safety requirements for compatibility layers
- documented autonomous software factory workflow for this project
- public evidence structure for AI-assisted development work
- reproducible local demo workflow
- updated README and contributor docs
- accessibility-aware web documentation updates
- issue templates for external review
- final public report

Total:

```text
49,500 EUR
```

## Form Field Preparation

### Proposal Name

Human-written final answer should be based on:

```text
AI-Native Linux Foundation Prototype
```

### Website / Wiki

Use:

```text
https://ai-os.net
https://github.com/ai-os-dot-net/AI-Native-Linux
```

### Abstract Notes

Key facts to include:

- The project is about creating a Linux environment where AI and agents are native operating participants.
- Installation begins with a generic Linux kernel, then the system maps the hardware and plans a dedicated hardened kernel for the exact machine.
- Agents must be able to live and work inside the system without being able to destroy it.
- AI-Native Linux defines a governed execution model:

```text
Human goal -> agent context -> typed action -> policy decision -> capability runtime -> verification -> evidence
```

- The proposal funds the first open-source foundation prototype for that AI-native operating substrate.
- It also formalises AIOS-FS semantics and safe compatibility contracts for Windows, Android, and macOS application environments.
- Results will be Apache-2.0 and publicly documented.

### Relevant Prior Work / Contributions

Key facts to include:

- Maintainer operates Linux/self-hosted infrastructure.
- Existing public repository contains Rev.1 and Rev.2 specifications.
- Public site exists at `https://ai-os.net`.
- GitHub organization exists at `https://github.com/ai-os-dot-net`.
- Project has governance, security, contribution, and maintainer documentation.

### Requested Support

Use:

```text
49,500 EUR
```

### Budget Explanation Notes

Explain:

- work is divided into seven milestones
- most cost is engineering time for specification hardening, Rust prototype, policy engine, verification, evidence logs, hardware/kernel planning, and AIOS-FS modelling
- autonomous software factory tooling will reduce iteration cost but does not remove maintainer review, testing, documentation, or accountability
- smaller portion covers documentation, demo workflows, community readiness, observability/evidence structure, and project management

### Existing / Historical Efforts To Compare Against

Mention carefully:

- Linux systemd/service management: mature service control, but not an AI-native typed intent and policy layer
- sudo/shell automation: powerful but unsafe for AI agents
- Ansible/Salt/Puppet: infrastructure automation, but not a local cognitive execution layer with typed AI action governance
- Kubernetes controllers: declarative reconciliation, but focused on clusters, not personal/workstation Linux cognitive operation
- SELinux/AppArmor: mandatory access control, useful substrate, but not a semantic AI action governance layer
- Policy engines such as OPA: useful policy concepts, but AI-OS integrates policy with action envelopes, runtime adapters, verification, and evidence
- Wine/Proton, Android containers, and virtualization: useful compatibility substrates, but AI-OS needs policy-governed compatibility environments integrated with filesystem, identity, evidence, and agent boundaries
- Linux From Scratch, Gentoo, NixOS, Yocto: useful examples of customisable systems, but AI-OS focuses on AI-assisted host adaptation, safe resident agents, and semantic operating state

### Technical Challenges

Likely challenges:

- safely mapping hardware and deriving host-specific kernel plans
- applying hardening without breaking boot, drivers, desktop usability, or recovery
- designing typed actions expressive enough for real Linux operations without becoming shell strings
- keeping resident agent authority bounded and auditable
- making policy decisions explainable and testable
- proving action outcomes through verification rather than trusting agent claims
- avoiding privileged side effects in demos
- designing AIOS-FS semantics without losing conventional recovery and interoperability
- integrating compatibility layers without giving them uncontrolled host access
- using AI-assisted workers productively while keeping public claims, security-sensitive changes, and normative architecture under maintainer control
- defining a clear path from specification to implementation

### Ecosystem and Engagement

Notes:

- develop in public on GitHub
- publish specification and demo docs on `ai-os.net`
- use issues for security model feedback and implementation proposals
- disclose AI-assisted software factory usage
- publish human-reviewed outputs, tests, and evidence rather than opaque AI-generated claims
- invite review from Linux, security, open-source AI agent, and self-hosting communities
- align with open standards and existing Linux security primitives where possible

## Final Submission Checklist

- [ ] Disclose intentional AI use in the proposal.
- [ ] Keep a preparation record with model/tool names, prompt themes, and substantial AI-generated outputs that influenced the final text.
- [ ] Maintainer reviews and owns the final answers.
- [ ] Keep answers concise and concrete.
- [ ] Select `NGI Zero Commons Fund`.
- [ ] Use individual applicant details.
- [ ] Use `49,500 EUR` unless budget is adjusted.
- [ ] Attach no unnecessary files.
- [ ] If attaching anything, use a concise PDF or text attachment based on human-written final content.
- [ ] Confirm repository is public and up to date.
- [ ] Confirm license is Apache-2.0.
- [ ] Submit before `2026-06-01 12:00 CEST`.
