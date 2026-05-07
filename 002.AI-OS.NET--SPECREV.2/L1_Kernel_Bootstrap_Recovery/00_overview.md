# L1 — Kernel, Bootstrap, Recovery

Status: `SHELL`

## Responsibility

Linux substrate, host bootstrap, generic fallback kernel, recovery boot path, and the optional dedicated kernel candidate pipeline. L1 ensures the machine can boot and be recovered without any cognitive layer.

## Layer invariants (from Rev.1 §6, §8)

- L1 recovery must not depend on L5 cognition.
- `/` must boot without `/aios` mounted.
- `/root` must remain available for emergency repair.
- Dedicated kernel failure must fall back to the generic kernel.
- Recovery must not require an LLM, Web UI, or KDE session.

## Dependencies

May depend on: L0.

## Planned sub-specs

| File                              | Topic                                                                              | Status  |
| --------------------------------- | ---------------------------------------------------------------------------------- | ------- |
| `01_recovery_boundary.md`         | `/`, `/root`, `/aios` separation; mount semantics in normal vs recovery mode       | `SHELL` |
| `02_first_boot_flow.md`           | Installer → bootstrapper → AIOS runtime → AI provider mode → recovery registration | `SHELL` |
| `03_dedicated_kernel_pipeline.md` | hardware map → trust check → host config → hardening → sandbox build → A/B promote | `SHELL` |

## See also

- [Rev.1 §8 — Host Bootstrap and Recovery](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
