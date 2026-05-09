# L7 — Interaction Renderers

Status: `PARTIAL` (foundation `01_surface_composition.md` is `CONTRACT`; downstream renderers + visual language remain `SHELL` / `DEFERRED`)

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

| File                        | Topic                                                                                                                                                                        | Status            |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------- |
| `01_surface_composition.md` | Surface + Composition Model — closed `SurfaceKind`/`CompositionZone` enums, lifecycle FSM, capability brokering hook to L8, cross-group surface isolation, AIOS chrome rules | `CONTRACT` (S7.1) |
| `02_shared_ui_schema.md`    | Abstract UI component schema; render-target binding; Surface as a node kind                                                                                                  | `SHELL`           |
| `03_visual_language.md`     | Token-level visual language — semantic colors, typography, spacing, distinctive AIOS components, motion principles (stage 2 of three-stage visual plan)                      | `SHELL`           |
| `04_kde_renderer.md`        | KWin compositor + Qt/QML widgets + wgpu for Surface nodes; KRunner plugin, Plasma widget, tray, approval prompt, evidence viewer                                             | `SHELL`           |
| `05_web_renderer.md`        | DOM + WebGPU canvas hybrid; goal input, plan viewer, approval prompts, action stream, evidence viewer, AIOS-FS browser                                                       | `SHELL`           |
| `06_cli_renderer.md`        | CLI command set; piping; scripting integration; recovery-safe subset                                                                                                         | `SHELL`           |
| `07_voice_mobile_future.md` | Out-of-scope sketch for future renderers                                                                                                                                     | `DEFERRED`        |

## See also

- [Rev.1 §16 — Unified Cognitive Shell and Renderers](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
