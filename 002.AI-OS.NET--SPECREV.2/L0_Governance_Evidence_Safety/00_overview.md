# L0 — Governance, Evidence, Safety

Status: `PARTIAL` (all 4 sub-specs `CONTRACT`; layer headline remains PARTIAL until E2+ implementation evidence)

## Responsibility

L0 owns the system's constitutional truth: the canonical status taxonomy, evidence grades, governance gates, and invariants that all higher layers must respect. L0 has no upward dependencies.

## Layer rules (from Rev.1 §6, §7)

- No capability is complete without evidence.
- No capability is ownerless.
- No state-changing operation is valid without a policy decision.
- No high-risk action is valid without approval or explicit policy.
- No AI output is authoritative without verification.
- No degraded state may be hidden from the user.
- No layer may depend upward for correctness.

## Dependencies

May depend on: nothing. L0 is the bottom of the dependency stack.

## Planned sub-specs

| File                            | Topic                                                                                           | Status            |
| ------------------------------- | ----------------------------------------------------------------------------------------------- | ----------------- |
| `01_status_taxonomy.md`         | `REAL`/`PARTIAL`/`SHELL`/`CONTRACT`/`DEFERRED`/`BLOCKED`/`UNKNOWN`/`RETIRED` formal definitions | `CONTRACT` (S6.1) |
| `02_evidence_grades.md`         | E0–E5 grade definitions, escalation rules, grade-to-status mapping                              | `CONTRACT` (S6.2) |
| `03_evidence_receipt_schema.md` | Canonical schema for an evidence receipt                                                        | `CONTRACT` (S6.3) |
| `04_invariants.md`              | Cross-cutting invariants and how they are enforced                                              | `CONTRACT` (S6.4) |

## See also

- [Rev.1 §7 — Governance and Evidence](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
