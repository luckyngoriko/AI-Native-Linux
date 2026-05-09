# L9 — Observability, Admin, Operations

Status: `PARTIAL`

## Responsibility

L9 owns action history, policy decisions, approvals, denials, verification results, service graph state, AIOS-FS transaction journal, evidence receipts, resource pressure metrics, model routing decisions, adapter failures, and recovery events. L9 also defines the verification grammar — the typed primitives that prove an action produced its intended result.

## Layer invariants (from Rev.1 §6, §14, §20)

- Evidence is append-only.
- Evidence cannot be modified by AI agents.
- Evidence records must reference action, policy decision, and verification.
- Sensitive values must be redacted from evidence.
- Failed and denied operations are also evidence.
- Verification must be explicit, typed, attached to the action, logged, and visible to the user.

## Dependencies

May depend on: L0, L1, L2, L3, L4, L5, L6, L7, L8.

## Planned sub-specs

| File                         | Topic                                                                         | Status     | Phase |
| ---------------------------- | ----------------------------------------------------------------------------- | ---------- | ----- |
| `01_evidence_log.md`         | Append-only log architecture: WAL + segments + tiering + compaction + indexes | `CONTRACT` | S3.1  |
| `02_verification_grammar.md` | Typed verification primitives, composition, property-based verification       | `CONTRACT` | S2.4  |
| `03_failure_handling.md`     | Failure → behavior table; degradation; runbook references                     | `CONTRACT` | S14.1 |
| `04_telemetry_pipeline.md`   | OpenTelemetry, Prometheus, Loki, eBPF integration; cardinality budgets        | `SHELL`    | —     |

## Cross-cutting contract dependency

L9 _projects from_ the [Action Envelope + Lifecycle contract](../XX_Cross_Cutting/01_action_envelope_lifecycle.md) (S0.1) — every evidence record references an action envelope.

## See also

- [Rev.1 §14 — Verification and Evidence](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.1 §20 — Observability and Failure Model](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
