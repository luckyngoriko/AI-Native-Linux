# S17 - App Capsule Runtime

| Field     | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Status    | `CONTRACT` (created 2026-05-28; awaiting implementation evidence)                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Phase tag | S17                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Layer     | L6 Apps/Packages/Compatibility, crossing L2, L4, L7, L8, L9, L10, S16                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| Consumes  | S12.1 App Runtime Model (`EcosystemRuntime`, `ManifestTranslationStrategy`) — S17.1 introduces `AIOSAppObject` mapped from those, S12.2 Package Model, S3.2 Sandbox Composition, S2.3 Policy Kernel, S3.1 Evidence Log (vocabulary only), S8.1 Network Policy (vocabulary only), S8.2 GPU/Video Policy (vocabulary only), S8.3 Hardware Graph (imports-vocabulary-from), S7.2 Shared UI Schema (imports-vocabulary-from), S16.2 SELinux MAC Policy Plane, S18 Kernel Personality and Portability Plane (vocabulary only) |
| Produces  | App Capsule object model, capsule solver, Windows capsule runtime, reliability contract, operator UI/evidence surfaces                                                                                                                                                                                                                                                                                                                                                                                                   |

## 1. Responsibility

S17 defines how AIOS treats installed software as per-app capsules. A capsule is
the user-visible and machine-enforced unit for installing, launching, updating,
repairing, moving, backing up, quarantining, and deleting software.

Package formats are inputs. The internal runtime truth is:

```text
software artifact
  -> AIOSAppObject
  -> AppCapsule
  -> policy decision
  -> sandbox/runtime launch
  -> evidence
```

This contract is especially important for Windows applications and games,
because they need a reproducible runtime envelope: runner, prefix, registry,
DLLs, redistributables, fonts, graphics stack, audio/video bridge, save data,
rollback, and compatibility truth.

Invariant links: INV-002, INV-008, INV-014, INV-017.

## 2. Product principle

AIOS must not promise that every app always runs. It must promise that every app
gets a governed execution plan, a bounded blast radius, rollback, evidence, and
a clear reason when blocked.

```text
best-fit runtime
+ minimal authority
+ reproducible dependencies
+ health check
+ repair path
+ rollback path
+ clear failure reason
```

## 3. Reference patterns

| Pattern                                                                                             | S17 use                                                                      |
| --------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| [Flatpak documentation](https://docs.flatpak.org/)                                                  | Sandboxed GUI app model and portal-oriented host integration.                |
| [XDG Desktop Portal API](https://flatpak.github.io/xdg-desktop-portal/docs/api-reference.html)      | Brokered file, screen, desktop, and host integration.                        |
| [AppStream metadata](https://www.freedesktop.org/software/appstream/docs/chap-AppStream-About.html) | App identity, catalog, screenshots, releases, components, firmware, drivers. |
| [XDG Base Directory specification](https://specifications.freedesktop.org/basedir/)                 | Data/config/cache/state separation.                                          |
| [WineHQ FAQ](https://wiki.winehq.org/FAQ)                                                           | Wine prefix model and Windows compatibility baseline.                        |
| [Valve Proton](https://github.com/ValveSoftware/Proton)                                             | Curated Wine-based game compatibility stack.                                 |
| [Bottles environments](https://docs.usebottles.com/getting-started/environments)                    | Named Windows app/game environments with dependencies.                       |
| [Bottles dependencies](https://docs.usebottles.com/bottles/dependencies)                            | Explicit redistributable/dependency management for Windows apps.             |
| [OCI](https://opencontainers.org/)                                                                  | Container artifact model for service-style capsules.                         |

## 4. Sub-specs

| File                                  | Topic                                                                                                                       | Status     | Priority |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- | ---------- | -------- |
| `01_capsule_object_model.md`          | AppCapsule identity, types, manifest schema, layout, data contract, capability contract.                                    | `CONTRACT` | P0       |
| `02_capsule_solver_lifecycle.md`      | Solver inputs, runtime selection, lifecycle FSM, install/update/repair/export/import flows.                                 | `CONTRACT` | P0       |
| `03_windows_capsule_runtime.md`       | Windows app/game capsule: Wine/Proton runners, prefixes, DLLs, dependency recipes, graphics/audio, anti-cheat, VM fallback. | `CONTRACT` | P0       |
| `04_reliability_security_evidence.md` | Health checks, snapshot/rollback, Capsule Doctor, quarantine, support bundle, SELinux/policy/evidence integration.          | `CONTRACT` | P0       |
| `05_operator_ui_acceptance.md`        | Operator UI flows, risk diff, "why blocked", acceptance criteria and first deliverable.                                     | `CONTRACT` | P1       |

## 5. Core invariants

1. Every non-system app runs as a capsule.
2. Capsule isolation is the default; trust is still evaluated separately.
3. No capsule gets direct root, broad home access, or Docker socket access by
   default.
4. Capsule updates stage into a clone before promotion.
5. Capsule repair snapshots before mutation.
6. Capsule rollback includes code, runtime, dependency layers, config, prefix,
   registry where applicable, and declared state migration.
7. Windows apps never use a global `~/.wine` as AIOS state.
8. Anti-cheat, DRM, driver, license, and vendor-refusal blockers are reported
   honestly as `BLOCKED_WITH_REASON`.
9. Operator-visible app actions are policy decisions, not UI-side authority.
10. Every launch, update, repair, rollback, quarantine, and export can emit
    evidence.

## 6. Layer interaction discipline

Capella Rev.3 analysis correctly shows S17 edges from L6/L7 to L8/L9
components. These must be interpreted as vocabulary and integration contracts,
not as lower-layer runtime dependency on higher-layer services.

Allowed upward imports:

| Imported spec          | S17 use                                                              | Dependency class          |
| ---------------------- | -------------------------------------------------------------------- | ------------------------- |
| S3.1 Evidence Log      | Evidence record names, receipt id shape, retention class names.      | `imports-vocabulary-from` |
| S8.1 Network Policy    | Network manifest schema, endpoint/grant vocabulary.                  | `imports-vocabulary-from` |
| S8.2 GPU/Video Policy  | GPU/video device class vocabulary, capture/encode policy vocabulary. | `imports-vocabulary-from` |
| S8.3 Hardware Graph    | Hardware capability descriptor shape and fit-check vocabulary.       | `imports-vocabulary-from` |
| S7.2 Shared UI Schema  | Renderer surface schema vocabulary for capsule passport views.       | `imports-vocabulary-from` |
| S18 Kernel Personality | Kernel capability matrix and backend-selection vocabulary.           | `imports-vocabulary-from` |

Forbidden upward dependencies:

- S17 code must not require the Evidence Log service to be live before it can
  decide capsule layout or local rollback. It may queue evidence for later.
- S17 solver must not synchronously call a higher-layer UI renderer to make
  policy decisions.
- S17 capsules must not bypass lower-layer sandbox/policy boundaries by calling
  GPU, network, or evidence services directly.
- S17 must not treat S8 hardware/network components as authority. Hardware and
  network facts are consumed through signed snapshots or typed broker APIs.

Runtime authority remains downward:

```text
S17 capsule action
  -> S2.3 Policy Kernel decision
  -> S3.2 Sandbox Composition
  -> lower-level runtime/OS enforcement
  -> S3.1 evidence emission or queued evidence receipt
```

This classification is required to keep INV-007 intact while allowing S17 to
use cross-layer vocabulary for app capsules.

## 7. Non-goals

- Do not force every desktop app into Docker.
- Do not hide Wine/Proton incompatibility behind generic failure messages.
- Do not allow Windows installers to mutate the real host.
- Do not share prefixes silently across unrelated apps.
- Do not treat launchers as unrestricted package managers.
- Do not claim legal right to redistribute proprietary Windows dependencies
  unless the dependency recipe records a valid source/license path.

## 8. Capella follow-up requirements

The Capella model should classify S17 layer-inversion candidates as follows:

| Edge family                                                  | Expected classification                   |
| ------------------------------------------------------------ | ----------------------------------------- |
| S17/S17.1/S17.2/S17.3 -> S8.1/S8.2/S8.3                      | `vocabulary`                              |
| S17/S17.3/S17.5 -> S3.1                                      | `vocabulary`                              |
| Any S17 edge that requires a higher-layer service to be live | `runtime` and must be removed or inverted |

If `classify_inversions.py` reports S17 runtime edges, the S17 `Consumes` header
or the architecture boundary is wrong and must be fixed before implementation.

## 9. See also

- [Capsule Object Model](01_capsule_object_model.md)
- [Capsule Solver and Lifecycle](02_capsule_solver_lifecycle.md)
- [Windows Capsule Runtime](03_windows_capsule_runtime.md)
- [Reliability, Security, Evidence](04_reliability_security_evidence.md)
- [Operator UI and Acceptance](05_operator_ui_acceptance.md)
- [S16 Security Hardening and Compliance](../S16_Security_Hardening_Compliance/00_overview.md)
- [S18 Kernel Personality and Portability Plane](../S18_Kernel_Personality_Portability/00_overview.md)
- [Rev.3 Planning Notes](../00_PLANNING_NOTES.md)
