# S17.1 - Capsule Object Model

| Field     | Value                                                                                                                                                                           |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-28; awaiting implementation evidence)                                                                                                               |
| Phase tag | S17.1                                                                                                                                                                           |
| Layer     | L6 Apps/Packages/Compatibility                                                                                                                                                  |
| Consumes  | S12.1 App Runtime Model (`EcosystemRuntime` / `ManifestTranslationStrategy` outputs), S12.2 Package Model, S3.2 Sandbox Composition, S8.1 Network Policy, S8.2 GPU/Video Policy |
| Produces  | `AIOSAppObject`, `AppCapsule`, capsule type enum, manifest schema, filesystem layout, data/capability contract                                                                  |

## 1. Purpose

`AppCapsule` is the runtime envelope for an AIOS application. It is not the same
as a package. A package is an artifact source. A capsule is the installed and
managed runtime unit.

```text
package/source/app artifact -> AIOSAppObject -> AppCapsule -> launchable workload
```

The capsule owns the app's runtime plan, data boundaries, dependencies,
capabilities, snapshots, health state, and evidence links.

### 1.1 `AIOSAppObject` (S17 intake truth object)

`AIOSAppObject` is the typed intake object that sits between a raw artifact and a
launchable `AppCapsule`. It is **produced by S17.1**, not consumed from S12.1.
Rev.2 S12.1 (App Runtime Model) produces the closed `EcosystemRuntime` and
`ManifestTranslationStrategy` enums plus the four-phase observe/propose/audit/refine
mechanism; it does **not** emit an `AIOSAppObject`. S17.1 maps those S12.1 outputs —
together with S12.2 package metadata and S3.2 sandbox inputs — into the single intake
truth object defined below. The `AppCapsule.source.app_object_id` field references an
`AIOSAppObject` created here.

```text
AIOSAppObject =
  app_id            : stable reverse-DNS id or AIOS-generated id
  display           : { name, icon_ref, category, locale_descriptions }
  class             : AppClass    (closed enum, see below)
  source            : { repo, package_format, publisher, signature, url, mirror }
  variants          : set<AppVariant>      (closed enum, see below)
  selected_runtime  : SelectedRuntime      (closed enum, see below)
  trust             : { repo_trust, publisher_trust, signature_status, sbom_ref, provenance_ref, vulnerability_state }
  capabilities      : { files, network, dbus, portals, devices, gpu, audio, video, secrets, background, admin }
  data_contract     : { config, data, cache, state, logs, export_import, backup, wipe }
  workspace_scope   : WorkspaceScope       (closed enum: work | gaming | lab | family | admin | airgap | custom)
  lifecycle         : AppLifecycle          (closed enum: discovered | staged | approved | installed | running | paused | quarantined | removed)
  update_policy     : UpdatePolicy          (closed enum: pinned | manual | safe_auto | fleet_approved | blocked)
  rollback          : { previous_version, dependency_snapshot, config_snapshot, data_migration_plan }
  evidence          : { install_receipt, launch_receipt, policy_decisions, denials, network_device_events }

AppClass =
  SYSTEM | GUI | CLI | SERVICE | GAME | CONTAINER | K8S
| VM | WEB | ANDROID | DRIVER | PLUGIN | AGENT | RT | WASI

AppVariant =
  NATIVE | FLATPAK | NIX | APPIMAGE | OCI | PROTON
| ANDROID | VM | WASM | SOURCE

SelectedRuntime =
  NATIVE | PORTAL | NIX | PODMAN | CONTAINERD | K8S | WINE
| PROTON | ANDROID | VM | WASM | RT
```

Unknown values for `AppClass`, `AppVariant`, `SelectedRuntime`, `WorkspaceScope`,
`AppLifecycle`, and `UpdatePolicy` are rejected by the capsule solver at intake. The
`AIOSAppObject.class` and `selected_runtime` together drive the `CapsuleType` selection
in §2 (e.g. `class = WASI` / `selected_runtime = WASM` resolves to `WASI_CAPSULE`).

Invariant links: INV-003, INV-011, INV-017, INV-024.

## 2. Closed capsule type enum

```text
CapsuleType =
  SYSTEM_COMPONENT_CAPSULE
| LINUX_NATIVE_CAPSULE
| FLATPAK_STYLE_CAPSULE
| NIX_CAPSULE
| APPIMAGE_CAPSULE
| OCI_CAPSULE
| K8S_CAPSULE
| WINDOWS_APP_CAPSULE
| WINDOWS_GAME_CAPSULE
| WINDOWS_SUITE_CAPSULE
| WINDOWS_LAUNCHER_CAPSULE
| WINDOWS_VM_FALLBACK_CAPSULE
| ANDROID_CAPSULE
| VM_CAPSULE
| WASI_CAPSULE
| WEB_PWA_CAPSULE
| PLUGIN_CAPSULE
| AI_AGENT_CAPSULE
| RT_CAPSULE
| DRIVER_FIRMWARE_CAPSULE
```

Unknown values are rejected by the capsule loader, solver, and export/import
tools.

The five `WINDOWS_*_CAPSULE` entries are the closed Windows capsule taxonomy.
S17.1 and S17.3 share exactly these five classes: `WINDOWS_APP_CAPSULE` and
`WINDOWS_GAME_CAPSULE` are the leaf execution classes, while
`WINDOWS_SUITE_CAPSULE`, `WINDOWS_LAUNCHER_CAPSULE`, and
`WINDOWS_VM_FALLBACK_CAPSULE` are the composite/fallback classes detailed in
S17.3 §2. The capsule loader accepts all five; no S17.3 Windows capsule is
rejected by this enum.

`WASI_CAPSULE` is the capsule type emitted when the Capsule Solver chooses the
WASI/WASM runtime path (holistic §6 Capsule Solver output
"native/container/Wine/VM/WASI/blocked"); it pairs with
`AIOSAppObject.class = WASI` and `selected_runtime = WASM`.

## 3. Capsule type table

| Capsule type                  | Runtime contents                                                                | Default isolation                                         |
| ----------------------------- | ------------------------------------------------------------------------------- | --------------------------------------------------------- |
| `SYSTEM_COMPONENT_CAPSULE`    | AIOS signed component, recovery component, invariant bundle.                    | Immutable/image-managed; recovery approval for mutation.  |
| `LINUX_NATIVE_CAPSULE`        | Native executable/libs, declared XDG dirs, portals.                             | SELinux/seccomp/cgroup/Landlock where available.          |
| `FLATPAK_STYLE_CAPSULE`       | Base runtime layer, app layer, portal grants.                                   | Portal-first sandbox.                                     |
| `NIX_CAPSULE`                 | Nix closure and profile binding.                                                | Workspace-scoped env and declared paths.                  |
| `APPIMAGE_CAPSULE`            | Extracted AppImage plus generated manifest.                                     | Restricted sandbox until proven.                          |
| `OCI_CAPSULE`                 | OCI image, rootless Podman/containerd plan, volumes.                            | Rootless default; no Docker socket.                       |
| `K8S_CAPSULE`                 | Kubernetes manifests/Helm/Kustomize, namespace, image digests.                  | Admission policy, network policy, namespace boundary.     |
| `WINDOWS_APP_CAPSULE`         | Wine runner, prefix, registry, DLLs, redistributables, fonts.                   | Prefix/export bridge; no host mutation.                   |
| `WINDOWS_GAME_CAPSULE`        | Proton/Wine runner, prefix, DXVK/VKD3D, shader cache, controller/save state.    | Game workspace; no work/admin secrets.                    |
| `WINDOWS_SUITE_CAPSULE`       | Multiple related Windows apps sharing one prefix; see S17.3 §2.                 | Shared prefix; membership explicit and visible.           |
| `WINDOWS_LAUNCHER_CAPSULE`    | Steam/Epic/GOG/Battle.net-style launcher with child capsules; see S17.3 §2/§10. | Launcher cannot grant child broad authority silently.     |
| `WINDOWS_VM_FALLBACK_CAPSULE` | Apps needing real Windows kernel/driver/hard-DRM/anti-cheat; see S17.3 §2/§12.  | Full VM boundary; explicit integration points.            |
| `ANDROID_CAPSULE`             | Waydroid/VM/container user data and permissions.                                | Android permission bridge plus host file export.          |
| `VM_CAPSULE`                  | VM image, snapshots, integration policy.                                        | Full VM boundary.                                         |
| `WASI_CAPSULE`                | WASI/WASM module, host capability imports, declared resource limits.            | Capability-based WASI sandbox; no ambient host authority. |
| `WEB_PWA_CAPSULE`             | Isolated browser profile, storage, service worker state.                        | Per-app browser profile and network policy.               |
| `PLUGIN_CAPSULE`              | Plugin artifact, parent binding, own trust/capabilities.                        | Cannot inherit parent trust silently.                     |
| `AI_AGENT_CAPSULE`            | Agent package, tool grants, model/runtime binding.                              | AI subject restrictions plus policy approval.             |
| `RT_CAPSULE`                  | RT manifest, CPU/IRQ/device reservation, latency evidence.                      | RT admission control.                                     |
| `DRIVER_FIRMWARE_CAPSULE`     | Driver, firmware, module, signing, boot evidence; detailed by S19.              | Recovery-approved high-risk path.                         |

## 4. Manifest schema

```yaml
app_capsule:
  capsule_id: "cap_<ULID>"
  app_id: "org.example.App"
  capsule_type: WINDOWS_APP_CAPSULE
  display:
    name: "Example App"
    icon_ref: "blake3:..."
    categories: ["Productivity"]
    locale_names:
      bg: "Example App"
      en: "Example App"
  source:
    app_object_id: "app_<ULID>"
    artifacts:
      - kind: installer
        uri: "mirror://..."
        hash: "sha256:..."
    publisher: "Example Vendor"
    signatures: []
    sbom_ref: "optional"
    provenance_ref: "optional"
  runtime:
    selected_runtime: wine
    base_runtime_ref: "runner:wine-ge@..."
    dependency_layers:
      - "win-dep:vcredist2019@..."
      - "win-dep:arial-fonts@..."
    runner_lock: "runner-lock.toml"
  filesystem:
    code_ref: "blake3:..."
    state_dir: "state/"
    config_dir: "config/"
    cache_dir: "cache/"
    export_dir: "exports/"
  policy:
    workspace_scope: "work"
    capabilities_ref: "capabilities.toml"
    network_policy_ref: "network.toml"
    device_policy_ref: "devices.toml"
    secrets_policy_ref: "secrets.toml"
    sandbox_profile_ref: "sandbox.toml"
  lifecycle:
    state: HEALTHY
    update_policy: manual
    rollback_policy: multi_version
  evidence:
    install_receipt: "evr_..."
    last_launch_receipt: "evr_..."
    last_health_receipt: "evr_..."
```

## 5. Filesystem layout

```text
/aios/apps/<app_id>/
  capsule.toml
  app-object.ref
  code/
  runtime/
  deps/
  state/
  config/
  cache/
  logs/
  exports/
  snapshots/
  evidence/
```

Rules:

- `code/`, `runtime/`, and `deps/` are immutable for an active version.
- `state/` contains app-owned state and databases.
- `config/` contains per-app/per-workspace config.
- `cache/` is disposable.
- `exports/` contains user-approved exports to the host/user document space.
- `snapshots/` contains rollback points.
- `evidence/` contains references, not raw replacement for the Evidence Log.

## 6. Windows sub-layout

```text
/aios/apps/<app_id>/windows/
  prefix/
  drive_c/
  registry/
  dll-overrides.toml
  runner.toml
  win-deps.lock
  shader-cache/
  save-data/
  installer-observation/
```

Rules:

- AIOS never uses a global `~/.wine` for managed Windows apps.
- Prefix architecture (`win32` or `win64`) is fixed at capsule creation.
- Changing prefix architecture creates a new capsule lineage.
- Registry state is snapshot before install, dependency change, first launch,
  update, and repair.

## 7. Data contract

| Data type      | Capsule treatment                                                  |
| -------------- | ------------------------------------------------------------------ |
| App code       | Immutable, content-addressed, verified on load.                    |
| Runtime layers | Pinned, signed where available, shared by content hash where safe. |
| Dependencies   | Recipe-driven; license/source recorded.                            |
| Config         | Exportable, rollbackable, workspace-scoped.                        |
| State          | Backup/restore capable; migration plan required for risky update.  |
| Cache          | Disposable; size limits and cleanup policy.                        |
| Logs           | Redacted support bundle, retention policy.                         |
| User documents | Access only through declared paths or portal/export bridge.        |
| Secrets        | Brokered through Vault; scoped and revocable.                      |
| Saves          | Separate save-data class for games and legacy apps.                |

## 8. Capability contract

Capsules request AIOS capabilities; they do not receive raw host authority.

| Capability               | Default                                                           |
| ------------------------ | ----------------------------------------------------------------- |
| Files/home               | Denied except app-private scope and explicit portal/export paths. |
| Network                  | Denied or manifest-limited by profile.                            |
| D-Bus/IPC                | Denied except declared names/portals.                             |
| GPU/3D                   | Declared device class, workspace budget.                          |
| Video encode/decode      | Declared class and visible UI status.                             |
| Camera/microphone/screen | Runtime approval, visible indicator, evidence.                    |
| USB/Bluetooth/serial     | Explicit device intent and approval.                              |
| Secrets                  | Vault broker, not environment spray by default.                   |
| Background autostart     | Approval required, visible in passport.                           |
| Service install          | Approval plus systemd hardening score.                            |
| Kernel/firmware          | Driver/Firmware capsule and recovery gate.                        |

## 9. Thin vs fat capsule

| Mode           | Meaning                                             | Default use                                                                  |
| -------------- | --------------------------------------------------- | ---------------------------------------------------------------------------- |
| `THIN_CAPSULE` | References shared signed runtime/dependency layers. | Modern Linux apps and normal desktop use.                                    |
| `FAT_CAPSULE`  | Carries all required runtime/dependency layers.     | Airgap, fragile Windows apps, old enterprise apps, forensic reproducibility. |

`WINDOWS_APP_CAPSULE` may choose `FAT_CAPSULE` when dependency drift is the main
risk. `WINDOWS_GAME_CAPSULE` usually starts thin but pins runner, DXVK/VKD3D,
shader cache, and save-state contracts.

## 10. Non-goals

- A capsule does not imply trust — it bounds blast radius only; trust is evaluated separately.
- No capsule gets direct root, broad home access, or the Docker socket by default.
- The object model does not promise every app runs; `BLOCKED_WITH_REASON` is a valid outcome.
- `AIOSAppObject` is runtime truth, not a package format — it does not replace S12.x package intake, it is mapped from it.

## 11. Acceptance criteria

S17.1 is `REAL` only when:

1. Capsule manifests parse and reject unknown capsule types.
2. Capsule layout validator catches missing and unknown top-level files.
3. Managed Windows capsules do not use global `~/.wine`.
4. Capabilities are represented as typed data, not shell flags.
5. Delete UI can distinguish code, config, state, cache, logs, exports, saves.
6. Capsule export/import preserves manifest, locks, snapshots, and evidence
   references.
