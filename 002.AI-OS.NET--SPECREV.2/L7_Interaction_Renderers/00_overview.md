# L7 — Interaction Renderers

Status: `SHELL`

## Responsibility

Renderers expose the same Cognitive Core state through different surfaces: KDE Plasma, Web, CLI, future Voice, future Mobile. Renderers submit intents or typed actions and display state, evidence, and degraded states. They never own authoritative system state.

## Layer invariants (from Rev.1 §6, §16)

- Renderers must not own authoritative system state.
- Renderers do not bypass policy.
- Renderers display denial reasons and degraded states honestly.
- Web UI is localhost-only by default; LAN or remote exposure requires policy approval.

## Dependencies

May depend on: L0, L1, L2, L3, L4, L5, L6.

## Planned sub-specs

| File                        | Topic                                                                                      | Status     |
| --------------------------- | ------------------------------------------------------------------------------------------ | ---------- |
| `01_shared_ui_schema.md`    | Abstract UI component schema; render-target binding                                        | `SHELL`    |
| `02_kde_renderer.md`        | KRunner plugin, Plasma widget, tray, notifications, approval prompt, evidence viewer       | `SHELL`    |
| `03_web_renderer.md`        | Goal input, plan viewer, approval prompts, action stream, evidence viewer, AIOS-FS browser | `SHELL`    |
| `04_cli_renderer.md`        | CLI command set; piping; scripting integration; recovery-safe subset                       | `SHELL`    |
| `05_voice_mobile_future.md` | Out-of-scope sketch for future renderers                                                   | `DEFERRED` |

## See also

- [Rev.1 §16 — Unified Cognitive Shell and Renderers](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
