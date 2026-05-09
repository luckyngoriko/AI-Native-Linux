# AI-Native Linux / Unified Cognitive Shell

Architecture Vision - rev.1

> **Positioning note (added 2026-05-09, rev.2 era):** The "not another Linux distribution" framing below was the rev.1 framing of AIOS as an operating _layer_. **Rev.2 reframes AIOS as an AI-native Linux distribution** — i.e. a real Linux distro whose distinguishing layer is the cognitive shell described here. The architectural model (L0–L10, AI-proposes-never-executes, evidence-first, recovery without cognition) is unchanged. See [`002.AI-OS.NET--SPECREV.2/01_executive_summary.md`](../002.AI-OS.NET--SPECREV.2/01_executive_summary.md) and [`002.AI-OS.NET--SPECREV.2/03_architecture_overview.md`](../002.AI-OS.NET--SPECREV.2/03_architecture_overview.md) for the current positioning. This rev.1 file is preserved verbatim below as a frozen historical artifact.

## Core Vision

The goal is not to create another Linux distribution.

The goal is to build a unified human-machine cognitive environment where:

- the machine understands goals instead of commands
- AI becomes the semantic operating layer
- KDE and Web become unified interaction surfaces
- Linux becomes the execution substrate
- the entire environment behaves as a single cognitive system

## Interface Evolution

```text
Punch Cards
-> CLI
-> GUI
-> Mobile Touch
-> AI-Native Semantic Interface
```

Traditional OS:

```text
User controls computer
```

AI-native OS:

```text
Computer understands goals
```

## High-Level Architecture

```text
Human
   |
   v
Unified Cognitive Shell
   |
   v
Semantic Runtime
   |
   v
Capability Runtime
   |
   v
Linux / Cloud / Devices
```

## Unified Cognitive Shell

The system exposes one unified interface across:

- KDE Desktop
- Web Interface
- CLI
- Voice
- Mobile

All surfaces are render targets over the same cognitive core.

## KDE + Web Unified UX

Principles:

- same UX
- same workflows
- same state
- same cognitive layer
- different renderers

```text
                Cognitive Core
                       |
        +--------------+--------------+
        |                             |
   KDE Renderer                 Web Renderer
        |                             |
 Qt/QML Plasma                Next.js/WebAssembly
```

## KDE Desktop

KDE Plasma is the native desktop environment.

Reasons:

- Qt ecosystem
- Wayland support
- modular architecture
- KRunner integration
- power-user workflows
- widgets, panels, and system tray

KDE becomes the AI-native desktop shell.

## Web Interface

The Web UI is not a separate admin panel.

It is the browser-rendered version of the same operating environment.

Primary uses:

- remote operation
- tablet and mobile access
- future collaboration
- cloud access
- workstation mirroring

## Shared UI Schema

All UI components are declared abstractly.

Example:

```json
{
  "component": "terminal_panel",
  "position": "bottom",
  "tabs": true,
  "ai_enabled": true
}
```

Render targets:

- KDE renderer -> Qt/QML
- Web renderer -> React

## Cognitive Core

The Cognitive Core is the true operating environment.

Responsibilities:

- context understanding
- planning
- orchestration
- semantic memory
- workflow continuity
- multi-agent coordination
- system optimization
- execution supervision

## AI Semantic Layer

The AI layer sits above the Linux kernel, not inside kernel space.

```text
Hardware
   |
   v
Linux Kernel
   |
   v
AI Semantic Layer
   |
   v
Users / Apps / Agents
```

The AI layer acts as:

- semantic interpreter
- planner
- governor
- memory engine
- optimization layer
- orchestration system

## Linux Kernel Role

Linux remains:

- scheduler
- memory manager
- driver layer
- syscall layer
- process isolation layer

```text
Kernel = physics
AI layer = cognition
```

## AI Layer Responsibilities

### Intent Engine

Transforms human goals into execution graphs.

Example:

```text
"prepare Rust dev environment"

-> install toolchain
-> configure IDE
-> verify cargo
-> create sample project
```

### Semantic Context Engine

Maintains operational context:

- active project
- technologies
- infrastructure
- dependencies
- workflows
- preferences

### System Knowledge Graph

Maintains structured graph knowledge of:

- files
- services
- repositories
- devices
- users
- containers
- networks
- permissions

### Planner / Orchestrator

Converts:

```text
goal -> subtasks -> execution DAG
```

### Capability Translator

AI must never directly execute shell commands.

Instead of:

```text
sudo systemctl restart nginx
```

AI produces:

```json
{
  "action": "service.restart",
  "service": "nginx"
}
```

### Verification Engine

AI validates outcomes:

- service active
- tests passing
- ports reachable
- latency acceptable

### Persistent Memory

The OS remembers:

- projects
- architecture decisions
- fixes
- failures
- workflows
- preferences

### Multi-Agent Coordination

Agents coordinate through shared memory and policy runtime:

- Dev Agent
- Security Agent
- Filesystem Agent
- Research Agent
- Deployment Agent

## Capability Runtime

The Capability Runtime translates semantic operations into system actions.

Example:

```json
{
  "action": "service.restart",
  "service": "nginx"
}
```

Benefits:

- validation
- policy enforcement
- rollback support
- auditability
- cross-distro compatibility
- reduced hallucinations

## Policy Kernel

The Policy Kernel is the operating constitution.

Examples:

- AI may restart nginx
- AI may not modify firewall without approval
- AI may not access SSH keys
- AI may not delete `/home` recursively

## Execution Model

Correct model:

```text
AI -> Action Plan -> Policy Check -> Typed Runtime -> Verified Result
```

Rejected model:

```text
AI -> sudo bash
```

## Typed Actions

System operations become structured actions.

Example:

```json
{
  "action": "package.install",
  "package": "docker"
}
```

instead of:

```text
sudo apt install docker
```

## Audit and Evidence System

The OS maintains append-only evidence logs.

Example:

```text
AI requested service.restart nginx
policy approved
executor restarted service
verification passed
```

## Adaptive Backend

The Capability Runtime backend evolves through human-reviewed change proposals, never through autonomous self-modification. The Cognitive Core may _propose_ changes; production promotion always requires human approval.

```text
Observe
-> AI proposes patch
-> Sandbox simulation
-> Automated tests
-> Human review and approval
-> Staged deployment
-> Monitor
-> Rollback on regression
```

AI may _propose_:

- backend adapter patches
- kernel adaptation layer adjustments
- distro compatibility profiles
- new runtime adapters

The proposal pipeline is bound by:

- mandatory tests in sandbox
- Policy Kernel gating of any privileged action invoked during simulation
- explicit human approval before production promotion
- evidence-logged canary rollout
- automatic rollback on health check failure

The Cognitive Core may not modify the Policy Kernel, the Evidence Log, the Vault Broker, or the recovery boot path through this pipeline. Those layers are amended only through versioned governance change proposals reviewed outside the AI execution path. See SPECIFICATION sections 7, 11, 12.

## Kernel Adaptation Layer

AI does not depend directly on Linux internals.

```text
AI Layer
   |
   v
Capability Contract
   |
   v
Kernel Adaptation Layer
   |
   v
Linux Kernel
```

This isolates the cognitive system from kernel evolution.

## Technology Stack

Core Runtime:

- Rust
- Tokio
- tonic gRPC
- serde
- tracing

AI Orchestration:

- Python
- LangGraph
- LangChain
- FastAPI
- vLLM / Ollama

UI:

- KDE Plasma
- Qt/QML
- Tauri
- TypeScript
- Next.js
- Tailwind
- shadcn/ui

Storage:

- PostgreSQL
- Qdrant
- SQLite

Observability:

- eBPF
- OpenTelemetry
- Prometheus
- Loki

## Stack Philosophy

```text
Rust owns execution
Python owns cognition
KDE owns interaction
Web owns remote surfaces
```

## Semantic Filesystem

Future direction:

Instead of:

```text
/home/projects/cad/kernel/
```

The user asks:

```text
"open latest stable sdf renderer"
```

The filesystem becomes semantic.

## AI-Native Interaction

Primary interfaces become:

- text
- voice
- visual understanding
- diagrams
- screenshots
- semantic commands

Traditional menus are no longer the primary control model.

## Long-Term Vision

This is not merely an operating system.

It is a human-machine cognitive environment with:

- persistent cognition
- semantic execution
- unified interaction
- adaptive orchestration
- AI-native workflows

## Final Core Principle

```text
One cognitive layer
Many execution environments
One unified interface
Persistent machine cognition
```
