# L6 — Apps, Packages, Compatibility

Status: `PARTIAL`

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
| `04_sandbox_composition.md`     | Sandbox profile language; SELinux/AppArmor/Landlock/seccomp/cgroups composition                                                                                | `CONTRACT` | S3.2  |
| `05_compatibility_knowledge.md` | Per-app profile database; ProtonDB-equivalent governance                                                                                                       | `SHELL` | —     |

## Reference system app: ProxGuard

ProxGuard may be packaged as an optional AIOS infrastructure app, not as a core OS dependency.

Target install boundary:

```text
/aios/apps/proxguard
  manifest
  private state
  requested capabilities
  sandbox profile
  verification plan
  rollback pointer
```

Candidate app capabilities:

- `proxguard.service.simulate`
- `proxguard.service.deploy`
- `proxguard.service.restart`
- `proxguard.service.status`
- `proxguard.dns.plan`
- `proxguard.dns.apply`
- `proxguard.gateway.route`
- `proxguard.audit.read`

The app must run through AIOS policy, vault, sandbox, network policy, and evidence logging. It must not receive direct unrestricted root, Docker socket, firewall, DNS, or gateway authority.

## See also

- [Rev.1 §17 — Application, Package, and Compatibility Model](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [ProxGuard Reference Model](../XX_Cross_Cutting/02_proxguard_reference_model.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
