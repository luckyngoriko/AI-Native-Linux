# XX — Cross-Cutting Contracts

Status: `PARTIAL`

## Purpose

This folder holds contracts that are consumed by multiple layers and therefore do not naturally belong to any single layer's folder. Cross-cutting contracts are foundational: they fix the shape of what layers exchange, so changing one ripples across the system.

A contract belongs here when _all three_ are true:

1. It is consumed by **at least three** layers.
2. Removing it would force the layers to invent ad-hoc replacements that drift apart.
3. Its semantics are not specific to any one layer's responsibility.

If only one or two layers consume it, the contract belongs inside the owning layer's folder, not here.

## Documents in this folder

| File                                                               | Contract                                              | Consumed by                                              | Status  | Phase |
| ------------------------------------------------------------------ | ----------------------------------------------------- | -------------------------------------------------------- | ------- | ----- |
| [01_action_envelope_lifecycle.md](01_action_envelope_lifecycle.md) | Action Envelope schema, Lifecycle FSM, gRPC interface | L3 (runtime), L4 (policy), L5 (cognition), L9 (evidence) | `CONTRACT` | S0.1  |

## Future cross-cutting contracts (not yet created)

These are candidates that may move here as work progresses. They start as `DEFERRED` until a real need across multiple layers materializes.

- gRPC protocol baseline (transport, error envelope, metadata conventions)
- Global invariants index (L0 invariants surfaced as a single navigable list)
- Glossary of canonical terms (subject types, verification primitives, status values)
- Time and identifier conventions (ULID/UUID rules, timestamp format, monotonic clock requirements)

## See also

- [Rev.2 Master Index](../00_MASTER_INDEX.md)
- [Rev.1 §6 — Layer Model](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
