# Governance

AI-Native Linux is currently a specification-first open-source project under the `ai-os-dot-net` GitHub organization.

The project is governed by the safety model in the Rev.2 specification:

- AI must not directly execute privileged shell commands.
- Privileged operations must pass through typed actions, policy, runtime adapters, verification, and evidence.
- Recovery must not depend on AI availability.
- Security-sensitive authority must be explicit, scoped, testable, and auditable.

## Decision Process

Normative changes should be proposed through issues or pull requests and should identify the affected Rev.2 layer.

A change is normative when it changes required behavior, authority boundaries, policy semantics, execution flow, persistence, evidence, verification, or recovery behavior.

Normative changes should explain:

- the problem being solved
- affected layers or contracts
- new authority granted or removed
- security consequences
- verification and evidence requirements
- compatibility or recovery impact

## Maintainer Authority

Maintainers may merge documentation, specification, and site changes when they improve clarity, consistency, safety, or implementation readiness.

Security-sensitive changes require stricter review and should not be merged if they introduce direct AI-to-shell execution, hidden privilege escalation, unverifiable actions, or recovery dependence on AI.

## Project Status

The repository does not yet ship a production runtime. Until runtime implementation begins, governance focuses on specification quality, threat modeling, implementation contracts, and public review.
