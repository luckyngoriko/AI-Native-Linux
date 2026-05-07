# AI-OS / AIOS

**AI-Native Linux / Unified Cognitive Shell** — an operating environment that sits above the Linux kernel and turns human goals into typed, policy-checked, verified system actions.

This repository holds the AIOS specification and (eventually) reference implementation. It is currently in pre-implementation phase: only specification documents exist.

## Repository layout

```text
055.AI-OS.NET--LINUX-AI/
├── 001.AI-OS.NET--SPECREV.1/      # Frozen rev.1 (vision + canonical spec)
├── 002.AI-OS.NET--SPECREV.2/      # Active rev.2, organized by layer
├── README.md                       # This file
├── CLAUDE.md                       # Operating guidance for Claude Code
└── ai-os-logo-home.png
```

## Specification revisions

- **[Rev.1 (frozen)](001.AI-OS.NET--SPECREV.1/00_MASTER_INDEX.md)** — Initial vision and engineering contract. Do not modify; corrections discovered after freeze go into rev.2.
- **[Rev.2 (active)](002.AI-OS.NET--SPECREV.2/00_MASTER_INDEX.md)** — Layered rewrite. Each L0–L10 layer has its own folder; cross-layer contracts live in `XX_Cross_Cutting/`.

## Core idea (one paragraph)

Linux remains the execution substrate: kernel, drivers, scheduler, memory manager, syscalls. AIOS adds a semantic operating layer above the kernel that owns intent understanding, planning, typed system actions, policy decisions, verification, evidence logging, persistent operational memory, multi-agent coordination, and a unified KDE/Web/CLI/Voice interaction surface. AI proposes; the Capability Runtime executes only after a Policy Kernel decision and emits append-only evidence. Recovery never depends on AI.

## Status

Pre-implementation. No source code, no build system, no tests. Active work: rev.2 specification.
