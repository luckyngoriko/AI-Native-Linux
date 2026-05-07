# Contributing to AI-Native Linux

AI-Native Linux is currently specification-first. Contributions should improve clarity, safety, implementability, or evidence quality.

## Contribution Priorities

Useful contributions include:

- fixing ambiguity in the Rev.2 specification
- adding missing safety constraints
- improving typed action contracts
- strengthening policy, sandbox, vault, or verification models
- proposing implementation-ready Rust or Python module boundaries
- adding diagrams that clarify existing architecture
- identifying conflicts between layers

Avoid speculative rewrites that change the architecture without a concrete reason.

## Specification Rules

- Write specification content in English.
- Keep layer ownership clear: L0-L10 files should not silently take responsibility from another layer.
- Prefer typed contracts over prose-only behavior.
- Do not introduce direct `AI -> shell` execution paths.
- Recovery must not depend on AI availability.
- Privileged actions must pass through policy, typed runtime, verification, and evidence.

## Pull Request Checklist

Before opening a pull request:

- link the affected layer or contract
- explain what changed and why it matters
- state whether the change is normative or explanatory
- check for contradictions with `00_MASTER_INDEX.md`
- keep unrelated cleanup out of the PR

## Security-Sensitive Changes

Changes touching policy, sandboxing, vault, package installation, service control, kernel adaptation, filesystem authority, or network exposure should explain:

- what authority is being granted
- which component enforces the boundary
- how failure is detected
- what evidence is emitted
- how rollback or recovery works

## Development Status

There is no production runtime yet. Implementation PRs should remain small and should map back to a specific Rev.2 contract.
