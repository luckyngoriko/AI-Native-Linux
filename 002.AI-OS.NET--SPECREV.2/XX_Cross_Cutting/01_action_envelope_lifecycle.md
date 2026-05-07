# Action Envelope + Lifecycle (Rev.2)

Status: `SHELL` (S0.1 brainstorming in progress — placeholder reserves the path and records agreed scope)

## Purpose

This document is the rev.2 contract for the Action Envelope schema, Lifecycle state machine, gRPC interface, error model, and surrounding cross-cutting concerns of typed actions in AIOS. It refines [Rev.1 §13](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md) to address gaps identified during the rev.2 brainstorming.

## Consumed by

- **L3 — AIOS-SGR / Capability Runtime:** produces and routes envelopes; owns lifecycle transitions on the execution path.
- **L4 — Policy Kernel:** evaluates envelopes against policy and emits decisions bound to the envelope.
- **L5 — Cognitive Core:** _produces_ envelopes from plans; consumes lifecycle events for replanning.
- **L9 — Observability / Evidence:** projects evidence records from envelopes and lifecycle transitions.

## Scope of this revision

Agreed in S0.1 brainstorming, scope option **B — Pragmatic+**:

**In scope:**

- Idempotency keys and retry semantics.
- Causality (parent action / saga linkage; field name TBD during design).
- Formal result envelope and error envelope schemas.
- Precise lifecycle state machine (states, transitions, terminal conditions).
- Versioning policy for the envelope schema (schema evolution rules).
- OpenTelemetry trace context handling (where trace_id/span_id live).
- Sandbox profile binding (how a profile is referenced from an envelope).
- Dry-run / simulation mode (flag vs separate RPC TBD).

**Out of scope (deferred to later sub-specs):**

- Subject identity canonical format (separate identity model spec under L4).
- Saga / batching composition (multiple atomic actions in one envelope).
- Approval binding mechanics (lives in L4 Policy Kernel sub-spec).
- TTL / expiration policy on queued actions.
- Resource budget hints.

## Status

Brainstorming in progress. The full contract content (schemas, FSM diagrams, error tables, gRPC IDL) will land here when the brainstorming flow reaches the "Write design doc" step.

Until then, this file:

- reserves the canonical path for the spec,
- records the agreed scope (so future work doesn't re-litigate scope decisions),
- enumerates the consumers (so layer overviews can point here).

## See also

- [Rev.1 §13 — Typed Actions and Capability Runtime](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md) — what we are refining.
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
- [Rev.2 Design Decisions](../02_design_decisions.md)
