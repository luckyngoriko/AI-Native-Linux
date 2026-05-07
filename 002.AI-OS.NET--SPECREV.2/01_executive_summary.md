# Executive Summary — Rev.2

Status: `SHELL` (will be filled as sub-specs land)

## Purpose of rev.2

Rev.1 established the AIOS vision and the layer model. Rev.2 turns that vision into contract-grade specifications: schemas, state machines, error models, and verification rules that an implementer can build against without further interpretation.

## Rev.1 → Rev.2 delta

To be filled as sub-specs land. Currently tracking:

- **S0.2 (applied):** README's "Self-Evolving Backend" reframed as "Adaptive Backend" — AI proposes; humans approve; Policy Kernel, Evidence Log, Vault Broker, and recovery path are excluded from the proposal pipeline. See [DEC-001](02_design_decisions.md).
- **S0.1 (in progress):** Action Envelope and Lifecycle contract. Scope: idempotency, causality, error envelope, lifecycle FSM, versioning, OpenTelemetry hooks, sandbox profile binding, dry-run mode.

## Active sub-specs

See [00_MASTER_INDEX.md](00_MASTER_INDEX.md) for the full roadmap.

## Out of scope for rev.2

- Subject identity canonical format (deferred to a separate identity spec)
- Saga / batching composition of actions
- Approval binding mechanics (lives in Policy Kernel sub-spec)
- TTL / expiration policy on queued actions
- Resource budget hints

## See also

- [Rev.1 (frozen) — vision and canonical spec](../001.AI-OS.NET--SPECREV.1/00_MASTER_INDEX.md)
