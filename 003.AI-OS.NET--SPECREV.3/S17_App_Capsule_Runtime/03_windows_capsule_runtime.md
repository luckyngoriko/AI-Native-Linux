# S17.3 - Windows Capsule Runtime

| Field     | Value                                                                                                                                              |
| --------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-28; awaiting implementation evidence)                                                                                  |
| Phase tag | S17.3                                                                                                                                              |
| Layer     | L6 Apps/Packages/Compatibility                                                                                                                     |
| Consumes  | S17.1 AppCapsule, S17.2 Capsule Solver, S12.1 App Runtime Model, S8.2 GPU/Video Policy, S8.3 Hardware Graph, S2.3 Policy Kernel, S3.1 Evidence Log |
| Produces  | `WINDOWS_APP_CAPSULE`, `WINDOWS_GAME_CAPSULE`, runner registry, prefix lifecycle, dependency recipes                                               |

## 1. Purpose

Windows applications need stronger capsule rules than Linux native apps. They
expect a Windows filesystem, registry, DLL search path, redistributables,
graphics stack, fonts, installers, launchers, and mutable global state. AIOS
must provide that inside the capsule, not by polluting the host.

Invariant links: INV-011, INV-014, INV-017, INV-024.

## 2. Windows capsule classes

| Class                         | Purpose                                                                           |
| ----------------------------- | --------------------------------------------------------------------------------- |
| `WINDOWS_APP_CAPSULE`         | Office, productivity, engineering, legacy enterprise, vendor tools.               |
| `WINDOWS_GAME_CAPSULE`        | Games, launchers, anti-cheat/DRM-sensitive titles.                                |
| `WINDOWS_SUITE_CAPSULE`       | Multiple related apps that require one shared prefix.                             |
| `WINDOWS_LAUNCHER_CAPSULE`    | Steam/Epic/GOG/Battle.net-like launcher, with child app/game capsules.            |
| `WINDOWS_VM_FALLBACK_CAPSULE` | Apps that require real Windows kernel behavior, drivers, hard DRM, or anti-cheat. |

## 3. Runner registry

AIOS maintains a signed registry of runners:

```yaml
runner:
  runner_id: "runner:wine-ge@8-26"
  family: wine | proton | wine-ge | proton-ge | vendor | vm
  version: "8-26"
  source: "mirror://aios/runners/..."
  hash: "sha256:..."
  signature: "..."
  supports:
    dxvk: true
    vkd3d: true
    esync: true
    fsync: true
    gamescope: true
  known_regressions: []
  security_notes: []
```

Runner rules:

- runner is pinned per capsule
- runner changes require health check
- runner downgrade is allowed only if not vulnerability-blocked
- runner source and hash are evidence-linked
- unknown runner is lab-only until trusted

## 4. Prefix contract

```yaml
wine_prefix:
  prefix_id: "prefix_<ULID>"
  architecture: win64
  windows_version: "win10"
  owner_capsule_id: "cap_<ULID>"
  shared_with_capsules: []
  registry_snapshot: "snapshot:..."
  drive_c_hash: "blake3:..."
  created_by_runner: "runner:wine-ge@..."
```

Rules:

- No global `~/.wine`.
- Prefix architecture is immutable.
- Prefix sharing must be explicit and visible.
- Registry is snapshotted before and after installer, dependency, update, and
  repair actions.
- Prefix corruption routes to Capsule Doctor, not blind recreation that loses
  user data.

## 5. Dependency recipe contract

Windows dependencies are explicit recipes:

```yaml
windows_dependency_recipe:
  dep_id: "win-dep:vcredist2019"
  version: "14.29"
  license: "vendor-redistributable"
  source_uri: "vendor-or-mirror-uri"
  hash: "sha256:..."
  installs:
    files: []
    registry_keys: []
    dll_overrides: []
  compatibility:
    runner_families: [wine, wine-ge, proton]
    prefix_architectures: [win64]
  rollback:
    registry_snapshot_required: true
```

Common dependency families:

- Visual C++ redistributables
- .NET / .NET Framework / Mono alternatives
- MSXML
- DirectX redistributables
- d3dx and d3dcompiler components
- media codecs where legally distributable
- fonts
- Java, Python, Electron, game-specific launch helpers where needed

Legal rule:

- AIOS records source/license status.
- If a dependency cannot be redistributed, the recipe can fetch from the vendor
  or ask the operator to provide the installer.
- AIOS must not silently ship proprietary redistributables without rights.

## 6. Installer capture

Windows installers run in App Lab first:

```text
installer.exe/msi
  -> isolated prefix clone
  -> filesystem diff
  -> registry diff
  -> DLL/dependency detection
  -> services/startup detection
  -> network detection
  -> generated capsule recipe
```

Detected changes become typed plan entries:

| Installer behavior      | AIOS plan                                                    |
| ----------------------- | ------------------------------------------------------------ |
| Writes to Program Files | Capsule `drive_c/` mutation.                                 |
| Writes registry keys    | Versioned registry diff.                                     |
| Installs service        | Service-like capability; approval required.                  |
| Adds startup entry      | Background autostart; approval required.                     |
| Downloads extra payload | Network/source artifact; hash and trust required.            |
| Installs driver         | VM fallback or driver safety plane; not normal Wine capsule. |

## 7. Graphics and gaming stack

`WINDOWS_GAME_CAPSULE` needs graphics policy:

| Component       | Treatment                                                   |
| --------------- | ----------------------------------------------------------- |
| DirectX 9/10/11 | Prefer DXVK where compatible.                               |
| DirectX 12      | Prefer VKD3D-Proton where compatible.                       |
| OpenGL          | Native OpenGL bridge.                                       |
| Vulkan          | Host Vulkan device grant with GPU policy.                   |
| Shader cache    | Per-capsule cache with size limit and rollback/cleanup.     |
| Gamescope       | Optional game compositor mode for scaling/HDR/VRR/handheld. |
| GPU selection   | Explicit integrated/discrete GPU policy.                    |
| Capture/stream  | Video Passport and PipeWire policy.                         |

Graphics health check:

```text
launch probe
  -> renderer initializes
  -> GPU path selected
  -> window appears
  -> frame output observed
  -> no forbidden device access
```

## 8. Audio, input, and video

| Area           | Rule                                                               |
| -------------- | ------------------------------------------------------------------ |
| Audio          | PipeWire bridge; sample rate/latency recorded for game/pro apps.   |
| Microphone     | Explicit runtime permission and visible indicator.                 |
| Screen capture | Portal/PipeWire path only, explicit permission.                    |
| Controllers    | Input broker, per-game mapping, no broad device access by default. |
| VR/AR          | Hardware fit check and explicit device grants.                     |
| Streaming      | WebRTC/SRT/RTMP policy through Video Passport.                     |

## 9. Save data and documents

Windows apps often store user data in unpredictable paths. AIOS must discover
and classify likely state locations:

```text
drive_c/users/<user>/Documents
drive_c/users/<user>/AppData/Roaming
drive_c/users/<user>/AppData/Local
ProgramData
game-specific save paths
launcher cloud-sync dirs
```

Rules:

- save data is backed up separately from app code
- prefix reset must preserve known save paths when possible
- document export uses `exports/` or portal bridge
- cloud sync state is visible in the passport

## 10. Launcher model

Launchers are not unrestricted package managers.

```text
launcher capsule
  -> discovers child app/game
  -> child gets its own capsule or declared shared prefix membership
  -> child capabilities are evaluated separately
```

Examples:

- Steam library entries become child game records.
- Epic/GOG/Heroic/Lutris imports become typed sources.
- A launcher cannot grant child games broad home, GPU, network, or anti-cheat
  privileges without AIOS policy.

## 11. Anti-cheat and DRM honesty

```text
AntiCheatStatus =
  SUPPORTED
| UNKNOWN
| BLOCKED_VENDOR_REFUSES
| BLOCKED_KERNEL_DRIVER
| BLOCKED_VM_HOSTILE
| BLOCKED_DRM
| OFFLINE_ONLY
```

Rules:

- AIOS does not bypass DRM or anti-cheat.
- Kernel anti-cheat requests route to VM fallback or blocked status.
- Vendor refusal is recorded as a compatibility fact, not a local error.
- Game mode UI must show anti-cheat truth before install when known.

## 12. VM fallback

VM fallback is selected when:

- app installs Windows kernel driver
- anti-cheat requires Windows kernel behavior
- hard DRM blocks Wine/Proton
- installer requires unsupported privileged service
- Wine/Proton path repeatedly fails health checks
- enterprise app requires full Windows environment

VM fallback still uses capsule policy:

- VM image is pinned
- snapshots are managed
- clipboard/file/USB/GPU integration is explicit
- network policy applies
- evidence is emitted

## 13. Non-goals

- No promise that every Windows app or game runs — anti-cheat, DRM, or vendor refusal may block it.
- Never one global `~/.wine` as AIOS state; each app or app-family gets its own prefix.
- A Windows capsule cannot write outside its prefix and declared export bridge.
- AIOS does not redistribute proprietary Windows components (vcredist/dotnet/fonts/media) without a valid recorded license path.

## 14. Acceptance criteria

S17.3 is `REAL` only when:

1. Windows capsule creation never touches global `~/.wine`.
2. Runner and dependency versions are pinned.
3. Installer capture produces filesystem and registry diffs.
4. Dependency recipes can be installed and rolled back in a prefix clone.
5. First launch probe records success/failure and selected graphics path.
6. Anti-cheat/DRM blockers are represented as typed status.
7. VM fallback is a typed capsule plan.
8. Prefix reset can preserve known save-data paths.
