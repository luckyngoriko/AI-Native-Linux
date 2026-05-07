# L6 — Apps, Packages, Compatibility

Status: `SHELL`

## Responsibility

AIOS applications are versioned application objects in AIOS-FS. Packages bundle artifacts, manifests, requested capabilities, install plans, verification plans, and rollback pointers. The Compatibility layer wraps Linux native, Flatpak, AppImage, container, Wine/Proton, and Waydroid runtimes behind a unified app object model.

## Layer invariants (from Rev.1 §6, §17)

- Every app has a manifest, identity, and private writable state.
- Broad filesystem writes are denied by default.
- Updates stage new versions before promotion; rollback uses previous verified versions.
- Foreign installers never run unrestricted on the host.
- Every Windows app gets an isolated prefix; Android apps get isolated per-app data.
- Hard-incompatible applications (kernel drivers, anti-cheat, hard DRM) may require VM fallback or `BLOCKED` status with explanation.

## Dependencies

May depend on: L0, L1, L2, L3, L4, L5.

## Planned sub-specs

| File                            | Topic                                                                                                                                                          | Status  | Phase |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- | ----- |
| `01_application_model.md`       | Manifest, identity, capabilities, sandbox spec, state directory, rollback pointer                                                                              | `SHELL` | —     |
| `02_package_model.md`           | Signed bundle, install/verify/rollback plan, package types (app, service, model, policy, UI schema, kernel artifact, compatibility profile, workflow template) | `SHELL` | —     |
| `03_compatibility_runtime.md`   | Compatibility compiler flow; Wine/Proton, Waydroid, VM fallback orchestration                                                                                  | `SHELL` | —     |
| `04_sandbox_composition.md`     | Sandbox profile language; SELinux/AppArmor/Landlock/seccomp/cgroups composition                                                                                | `SHELL` | S3.2  |
| `05_compatibility_knowledge.md` | Per-app profile database; ProtonDB-equivalent governance                                                                                                       | `SHELL` | —     |

## See also

- [Rev.1 §17 — Application, Package, and Compatibility Model](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
