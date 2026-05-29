# S17.5 - Operator UI and Acceptance

| Field     | Value                                                                                                                                     |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-28; awaiting implementation evidence)                                                                         |
| Phase tag | S17.5                                                                                                                                     |
| Layer     | L7 Interaction/Renderers, L6 Apps, L9 Evidence                                                                                            |
| Consumes  | S17.1 AppCapsule, S17.2 Capsule Solver, S17.4 Reliability/Security/Evidence, S3.1 Evidence Log, S2.3 Policy Kernel, S7.2 Shared UI Schema |
| Produces  | operator flows, UI requirements, acceptance plan, first deliverable                                                                       |

## 1. Purpose

The operator UI makes capsules understandable. The user should not need to know
whether the app came from `.deb`, Flatpak, AppImage, OCI, Wine, Proton, Android,
or VM. The UI shows one app with its selected runtime path, risk, data, devices,
health, rollback, and alternatives.

Invariant links: INV-009, INV-014, INV-019, INV-020, INV-021, INV-023.

## 2. Required UI surfaces

| Surface                | Purpose                                                                                           |
| ---------------------- | ------------------------------------------------------------------------------------------------- |
| `Capsule Install Plan` | Shows recommended runtime, alternatives, risk diff, data/device/network access.                   |
| `Capsule Passport`     | Runtime, source, trust, capabilities, health, snapshots, evidence.                                |
| `Capsule Doctor`       | Diagnosis and repair plan with snapshot preview.                                                  |
| `Windows Capsule View` | Runner, prefix, dependencies, DLLs, registry snapshots, graphics path, saves.                     |
| `Why Blocked`          | Clear reason: SELinux, policy, missing dependency, anti-cheat, DRM, GPU, codec, network, license. |
| `Data Manager`         | Code/config/state/cache/logs/exports/saves separation.                                            |
| `Rollback Manager`     | Known-good snapshots, update history, rollback blockers.                                          |
| `Export/Import`        | Move capsule to another machine or airgap mirror.                                                 |

## 3. Install UI flow

```text
search app
  -> AIOS shows one app entry
  -> recommended capsule path
  -> alternatives
  -> risk diff
  -> data/device/network view
  -> approval
  -> staged install
  -> first launch probe
  -> health result
```

The UI must show:

- source and publisher
- capsule type
- selected runtime
- capabilities
- network endpoints
- file/data access
- device access
- rollback availability
- known compatibility issues
- fallback options

## 4. Windows UI flow

For Windows apps/games, show:

- runner selected
- prefix architecture
- dependencies to install
- graphics path: DXVK, VKD3D, OpenGL, VM
- audio/video path
- save data handling
- launcher/child app relationship
- anti-cheat/DRM status
- VM fallback status if needed

Example:

```text
Install Windows app: ExampleCAD
Recommended: WINDOWS_APP_CAPSULE
Runner: Wine-GE 8-26 pinned
Dependencies: vcredist2019, dotnet48, d3dcompiler_47, Arial fonts
Files: app-private + Documents export bridge
Network: vendor.example.com:443
GPU: full 3D required
Rollback: available
Risk: medium
```

## 5. "Why blocked" reasons

```text
BlockedReason =
  UNSAFE_HOST_MUTATION
| UNSIGNED_ARTIFACT
| UNTRUSTED_SOURCE
| MISSING_RUNNER
| MISSING_DEPENDENCY
| LICENSE_RESTRICTED_DEPENDENCY
| SELINUX_DENIAL
| POLICY_DENIAL
| GPU_UNAVAILABLE
| VIDEO_CODEC_UNAVAILABLE
| VIRTUALIZATION_UNAVAILABLE
| ANTICHEAT_VENDOR_REFUSES
| DRM_BLOCKED
| KERNEL_DRIVER_REQUIRED
| HARDWARE_UNSUPPORTED
| SECURITY_PROFILE_FORBIDS
```

The UI must show safe alternatives where available:

- install another variant
- use VM fallback
- use remote/cloud bridge
- install missing legal dependency
- request exception
- keep blocked

## 6. First deliverable

The first useful S17 deliverable should be narrow:

```text
AIOS App Capsule MVP
  -> AppCapsule manifest parser
  -> layout validator
  -> capsule passport view in CLI/web
  -> Windows capsule recipe skeleton
  -> runner/dependency lockfiles
  -> first launch probe stub
  -> health status
  -> snapshot before repair/update
  -> blocked-with-reason output
```

Do not start with every ecosystem. Start with:

1. Linux native/portal capsule
2. AppImage extracted capsule
3. Windows app capsule with Wine runner skeleton
4. Windows game capsule with Proton runner skeleton

## 7. Non-goals

- The renderer is never an authority — it requests signed policy decisions, it does not grant capabilities (INV-031).
- The UI does not hide risk behind friendly wording; risk diffs are shown before consequential actions.
- No critical change requires the operator to read raw logs first.
- A "why blocked" explanation is produced from policy/evidence facts, not guessed by the UI.

## 8. Acceptance criteria

S17.5 is `REAL` only when:

1. UI shows one app with multiple runtime alternatives.
2. Install plan shows risk diff before approval.
3. Capsule passport shows runtime, data, devices, health, rollback, evidence.
4. Windows capsule UI shows runner, prefix, dependencies, graphics, saves,
   anti-cheat/DRM status.
5. Blocked installs produce typed reason and alternatives.
6. Delete UI separates app code, config, state, cache, logs, exports, saves.
7. CLI/recovery can show capsule health without graphical UI.
