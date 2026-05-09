# NLnet GenAI Prompt Provenance Log

This file records substantive generative AI use while preparing the NLnet / NGI Zero Commons Fund application for AI Linux OS / AI-Native Linux.

Policy reference supplied by the applicant:

```text
NLnet Policy on the use of Generative Artificial Intelligence for NLnet-funded projects.
Version 1.1, valid as of 2026-01-26.
```

## Tool / Model

```text
Tool: OpenAI Codex / ChatGPT coding-agent session
Model: OpenAI Codex environment based on GPT-5-class coding model
Exact hosted model snapshot: not exposed in the local session UI
Workspace: /home/luckyngoriko/dev/055.AI-OS.NET--LINUX-AI
Date: 2026-05-08
Timezone: Europe/Sofia / UTC+03:00
```

## General Use

Generative AI was used intentionally to:

- structure application notes
- draft form-field alternatives
- refine the project scope
- compare budget options
- prepare an attachment background file
- document autonomous software factory methodology
- check consistency with the public project repository

The applicant remains responsible for the final submitted text, budget, technical claims, milestones and delivery commitments.

## Prompt / Output Log

### 1. Proposal Name

Time:

```text
2026-05-08, interactive session
```

Prompt:

```text
Proposal name хайде да почнем от тук отново дай по готино име
```

Unedited output:

```text
По-добро име за NLnet proposal-а:

AI-Native Linux: A Safe Home for Resident AI Agents

То казва точно идеята: не просто runtime, а Линукс среда, в която агентите “живеят” безопасно.

Други добри варианти:

AI-Native Linux: Foundation for Agentic Operating Systems

AI-Native Linux: Safe Resident Agents for Open Systems

AI-Native Linux: An Open Operating Substrate for AI Agents

Моят избор: AI-Native Linux: A Safe Home for Resident AI Agents.
```

Follow-up prompt:

```text
AI Linux OS
```

Unedited output:

```text
За proposal name бих го написал така:

AI Linux OS: A Safe Home for Resident AI Agents

По-чисто и по-грантово звучи от само AI Linux OS.

Ако искаш по-кратко:

AI Linux OS

Но аз бих подал първия вариант, защото веднага казва какво финансират.
```

### 2. Abstract

Time:

```text
2026-05-08, interactive session
```

Prompt:

```text
Abstract: Can you explain the whole project and its expected outcome(s). 1200 символа
```

Unedited output:

```text
AI Linux OS is an open-source effort to create a Linux-based operating environment where AI is a native part of the system, not an external chatbot or command generator. The goal is to let resident agents live and work inside the OS safely, with clear policy, sandbox, filesystem, verification and evidence boundaries.

After installation, the system should map the exact hardware, firmware, drivers and threat surface of the machine, then plan a host-specific hardened kernel configuration and a sandboxed build path with recovery preserved. The project also defines AIOS-FS, a filesystem model for agent memory, evidence, app workspaces, semantic metadata, versioning and rollback. Compatibility with Windows, Android and macOS applications will be designed through controlled environments rather than uncontrolled host access.

The expected outcome is a public Apache-2.0 foundation prototype and hardened specification: hardware profiling and kernel hardening planning, safe resident agent execution, typed capability actions, policy decisions, evidence logs, AIOS-FS semantics, compatibility contracts and documented use of an AI-assisted autonomous software factory.
```

### 3. Relevant Experience

Time:

```text
2026-05-08, interactive session
```

Prompt:

```text
Have you been involved with projects or organisations relevant to this project before? And if so, can you tell us a bit about your contributions?
```

Initial output included the phrase "open-source CAD/kernel research project" for NeuroCAD. The applicant corrected that NeuroCAD is not open source.

Correction prompt:

```text
NeuroCAD is an open-source CAD/kernel това не го споменавай не е опън сорс
```

Corrected unedited output:

```text
I am the primary maintainer of the AI Linux OS / AI-Native Linux project and the operator of the public repository and project site. My background is in Linux infrastructure, self-hosted systems, server operations, DNS, mail, web hosting, containerized services and automation.

I also maintain and operate related engineering projects in the Iconys / NeuroCAD ecosystem. NeuroCAD is a CAD/kernel research project involving SDF, DCG, e-graphs and BVH concepts. For that project I operate an autonomous software factory pattern with a governor, multiple coding workers, a separate QA agent, local and remote models, observability, logs, metrics and evidence collection. This experience directly informs the AI Linux OS work: the operating system must give resident agents a safe place to work, with policy, sandboxing, verification, recovery and auditability.

My contribution to this project so far has been to define the architecture vision, create the public GitHub organisation and repository, publish the public project site, prepare the Rev.1 and Rev.2 specification structure, document the safety model, define the AIOS-FS direction, describe the policy/capability/evidence execution path, and prepare the project for open-source collaboration.

The project is still early, but it is based on practical infrastructure experience rather than only theory. I run Linux systems, web services and self-hosted infrastructure myself, and I want AI Linux OS to solve a real operational problem: how AI agents can become native participants in a Linux operating environment without being able to damage the system or bypass human control.
```

### 4. Budget

Time:

```text
2026-05-08, interactive session
```

Prompts:

```text
Explain what the requested budget will be used for?
Does the project have other funding sources, both past and present?
A breakdown in the main tasks with associated effort is appreciated. Make rates explicit.
```

```text
нека вкараме пари за инференс
```

```text
local model пари не искам
```

```text
трябват ни пари за хардуер
```

```text
4500 ще ни стигнал ли за AI
```

```text
по добре да сложим 60 евро на час и да вдигнеч цената за AI
```

Final unedited budget output:

```text
Requested budget: 49,500 EUR.

The budget covers maintainer engineering work, AI inference/API usage for the autonomous software factory, and limited test hardware needed to validate hardware profiling, kernel planning, sandboxing and recovery on real machines.

Explicit rates:
- Maintainer engineering: 60 EUR/hour
- AI inference/API usage: direct project cost
- Hardware: direct project cost

Breakdown:
- AI-native OS foundation and threat model: 80h = 4,800 EUR
- Hardware mapping and hardened kernel planning: 115h = 6,900 EUR
- Typed agent action envelopes and policy kernel MVP: 110h = 6,600 EUR
- Capability runtime and safe agent sandbox MVP: 90h = 5,400 EUR
- Evidence log, verification grammar and agent memory boundaries: 75h = 4,500 EUR
- AIOS-FS semantic filesystem model: 70h = 4,200 EUR
- Compatibility contracts, documentation, demo and community readiness: 60h = 3,600 EUR
- AI inference/API usage for the autonomous software factory: 7,500 EUR
- Test hardware for kernel planning, sandboxing and recovery validation: 6,000 EUR

Total maintainer engineering: 600h = 36,000 EUR
Total AI inference/API usage: 7,500 EUR
Total hardware: 6,000 EUR
Total requested: 49,500 EUR

No travel budget is requested.

The hardware budget will be used for a small reproducible test set, not general-purpose equipment: an x86_64 workstation-class test machine, storage devices for recovery and filesystem tests, network adapters where needed, and replacement parts/peripherals for repeated install, boot, rollback and sandbox validation.

The AI inference/API budget will be used for model calls supporting the autonomous software factory: coding agents, review agents, specification consistency checks, threat-model iteration, test generation, documentation review and fallback commercial API usage where needed.

The autonomous software factory will help with iteration and quality control, but all public deliverables, technical claims, budget decisions and security-sensitive changes remain human-reviewed by the maintainer.

The project has no confirmed external funding, past or present. The existing repository, public website, infrastructure and preparatory work have been self-funded by the maintainer. A separate application for open-source AI credits has been submitted, but no funding or credits have been awarded or committed at the time of this application.
```

### 5. Comparison With Existing Efforts

Time:

```text
2026-05-08, interactive session
```

Prompt:

```text
Compare your own project with existing or historical efforts.
```

Unedited output:

```text
AI Linux OS builds on existing Linux and security work, but combines them into a different goal: an AI-native operating environment where resident agents can safely live and work as part of the system.

Traditional Linux distributions focus on packaging, hardware enablement and desktop/server operation. Projects such as Gentoo, NixOS, Yocto and Linux From Scratch show that Linux can be deeply customised, but they do not provide an AI-assisted operating layer that maps a machine, plans a hardened host-specific kernel, governs resident agents and records auditable evidence for system changes.

systemd, sudo and shell automation are powerful existing mechanisms, but they assume a human or script is directly issuing commands. AI Linux OS rejects direct AI-to-shell execution. Agents request typed actions; policy decides; capability runtimes execute; verification checks the result; evidence is recorded.

Configuration management tools such as Ansible, Salt, Puppet and Chef automate infrastructure, but they are not designed as a local operating substrate for resident AI agents. They do not model agent memory, filesystem work cells, per-agent authority, local semantic state, or human-governed AI action envelopes.

Kubernetes controllers and operators provide useful ideas around declarative state and reconciliation, but they are cluster-oriented. AI Linux OS is focused on personal computers, workstations and self-hosted machines where AI, applications, hardware, files, services and user context share one operating environment.

SELinux, AppArmor, Landlock, seccomp, Secure Boot, TPM, measured boot and kernel hardening projects are important foundations. AI Linux OS does not replace them. It aims to orchestrate and document their use in an AI-native model where agents are constrained by policy, sandboxing, filesystem authority and recovery paths.

Wine, Proton, Android containers and virtual machines help run non-Linux applications. AI Linux OS treats these as controlled compatibility environments, not uncontrolled host access. The goal is to let Windows, Android and macOS application ecosystems be integrated through explicit policy, filesystem and evidence boundaries.

The novel part is the combination: AI-assisted hardware adaptation, hardened kernel planning, safe resident agents, AIOS-FS semantics, typed capability actions, verification, evidence logs and an auditable autonomous software factory used to build the system itself.
```

### 6. Technical Challenges

Time:

```text
2026-05-08, interactive session
```

Prompt:

```text
What are significant technical challenges you expect to solve during the project, if any?
```

Unedited output:

```text
The main technical challenge is to define an AI-native operating model that is ambitious but still safe, testable and implementable.

First, the system must map real hardware and derive a host-specific kernel hardening plan without breaking bootability, drivers, graphics, networking, storage or recovery. The project must distinguish between what can be safely automated now, what should only be planned, and what requires human approval.

Second, resident AI agents need useful authority without uncontrolled access. The project must design typed action envelopes, policy decisions, sandbox boundaries and capability runtimes that are expressive enough for real Linux work but do not degrade into free-form shell commands.

Third, AIOS-FS must model agent memory, app workspaces, evidence, semantic metadata, versioning, conflict handling and rollback while keeping the system understandable and recoverable with normal tools.

Fourth, verification is difficult. The system cannot trust an agent saying that an action succeeded. It needs explicit verification grammar and evidence logs for service state, files, network reachability, kernel plans, sandbox actions and compatibility environments.

Fifth, compatibility with Windows, Android and macOS applications must be designed without giving compatibility layers uncontrolled host access. The challenge is to define policy-governed environments that integrate with filesystem authority, identity, evidence and recovery.

Finally, the project will use an AI-assisted autonomous software factory. That creates its own challenge: using AI workers productively while keeping security-sensitive changes, public claims, architecture decisions and releases under human maintainer control. The factory must produce tests, logs and evidence, not opaque AI-generated code.
```

### 7. Ecosystem and Engagement

Time:

```text
2026-05-08, interactive session
```

Prompt:

```text
Describe the ecosystem of the project, and how you will engage with relevant actors and promote the outcomes?
```

Unedited output:

```text
The ecosystem for AI Linux OS includes Linux users, self-hosters, system administrators, security engineers, open-source AI agent developers, desktop environment developers, compatibility-layer communities and people building local-first or privacy-preserving computing environments.

The first users are expected to be technical Linux users and self-hosted infrastructure operators who want AI assistance but do not want agents to have uncontrolled shell or root access. Later users may include workstation users, developers, small organisations and public-interest infrastructure operators that need auditable AI-assisted system operation.

Relevant actors include Linux distribution communities, kernel hardening and security projects, SELinux/AppArmor/Landlock/seccomp communities, KDE/Qt desktop contributors, Wine/Proton and Android-container projects, policy engine developers, open-source AI agent projects and maintainers of self-hosted infrastructure tools.

Engagement will happen in public through the GitHub repository, issues, design discussions, public specifications, reproducible demos and the project site. The project will invite review specifically from security, Linux, open-source AI and self-hosting communities. Security-sensitive feedback will be handled through the public security process.

The outcomes will be promoted as reusable open infrastructure, not as a closed product: specification documents, prototype crates, examples, threat models, verification/evidence formats, demo workflows and implementation reports will be published under an open-source license. The project will also document how the AI-assisted autonomous software factory is used, so that the development process itself can be inspected and improved by the community.
```

### 8. Attachment Preparation

Time:

```text
2026-05-08, interactive session
```

Prompt:

```text
дай един md файл може би
```

Unedited output:

```text
The assistant created NLNET_ATTACHMENT_BACKGROUND.md in the repository. The file content is the unedited generated output for this attachment step, subject to later maintainer review before submission.
```

Generated file:

```text
NLNET_ATTACHMENT_BACKGROUND.md
```

### 9. GenAI Policy Interpretation and Compliance Update

Time:

```text
2026-05-08T19:06:22+03:00
```

Prompt:

```text
The applicant pasted NLnet's "Policy on the use of Generative Artificial Intelligence for NLnet-funded projects", version 1.1, valid as of January 26, 2026.
```

Unedited output summary:

```text
The assistant interpreted the policy as requiring transparent disclosure, a prompt provenance log listing model, dates/times, prompts and unedited outputs, and public disclosure for substantive GenAI use in project development. The assistant then created this NLNET_GENAI_PROMPT_PROVENANCE_LOG.md file and updated the project application materials to reference it.
```
