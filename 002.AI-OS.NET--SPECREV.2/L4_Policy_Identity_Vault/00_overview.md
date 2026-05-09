# L4 — Policy, Identity, Vault

Status: `PARTIAL`

## Responsibility

The Policy Kernel evaluates typed actions against the operating constitution. The Vault Broker performs operations on secrets without revealing material to AI subjects. The Identity model defines subjects (human, agent, application, service, device, workflow, remote_operator) and how they are bound to capabilities and approvals.

## Layer invariants (from Rev.1 §6, §11, §12)

- Default rule: if no policy matches, deny.
- Approval must be bound to one exact action request and must expire.
- Hard-denied actions cannot be overridden except by scoped, recorded emergency override by a human operator.
- Raw secret read by AI agents is denied by default.
- Use-without-reveal is the normal secret operation model.
- Evidence must never contain secret values.

## Dependencies

May depend on: L0, L1, L2, L3.

## Planned sub-specs

| File                       | Topic                                                                           | Status     | Phase |
| -------------------------- | ------------------------------------------------------------------------------- | ---------- | ----- |
| `01_policy_kernel.md`      | Decision engine — own DSL vs OPA/Rego vs CEL; rule precedence; simulation       | `CONTRACT` | S2.3  |
| `02_vault_broker.md`       | Secret classes, capability grants, use-without-reveal API, rotation, revocation | `SHELL`    | —     |
| `03_identity_model.md`     | Subject canonical format, capability binding, scope, TTL                        | `CONTRACT` | S5.1  |
| `04_approval_mechanics.md` | How approvals are requested, granted, bound, expired, recorded                  | `SHELL`    | —     |
| `05_emergency_override.md` | Scope, expiry, audit, what cannot be overridden                                 | `SHELL`    | —     |

## Open questions

- Policy language: own DSL (full control, build cost), OPA/Rego (battle-tested, learning curve), CEL (simpler, less expressive)?
- Approval delivery: KDE prompt, Web UI, CLI, push to mobile? Multi-channel with single binding?
- Vault backing: pass + ssh-agent + custom broker, HashiCorp Vault, age-based?

## See also

- [Rev.1 §11 — Policy Kernel](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.1 §12 — Vault Broker](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
