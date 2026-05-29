# S17.2 - Capsule Solver and Lifecycle

| Field     | Value                                                                                                                                             |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-28; awaiting implementation evidence)                                                                                 |
| Phase tag | S17.2                                                                                                                                             |
| Layer     | L6 Apps/Packages/Compatibility                                                                                                                    |
| Consumes  | S12.1 Package Rosetta/App Runtime Model, S17.1 AppCapsule, S2.3 Policy Kernel, S3.2 Sandbox Composition, S8.3 Hardware Graph, S8.1 Network Policy |
| Produces  | solver pipeline, lifecycle FSM, install/update/repair/export/import flows                                                                         |

## 1. Purpose

The Capsule Solver turns an app request into a launchable capsule plan. It must
choose the safest viable runtime, not the first available package.

Invariant links: INV-008, INV-009, INV-014, INV-017.

```text
app request
  -> source/artifact inspection
  -> capability extraction
  -> hardware fit
  -> trust/risk score
  -> runtime candidates
  -> capsule plan
  -> policy approval
  -> staged install
```

## 2. Solver inputs

| Input               | Examples                                                                       |
| ------------------- | ------------------------------------------------------------------------------ |
| App identity        | AppStream id, desktop id, package name, upstream URL, vendor signature.        |
| Source artifacts    | `.deb`, `.rpm`, Flatpak, AppImage, OCI, EXE/MSI, APK, source, Helm.            |
| Metadata            | Package scripts, desktop entries, manifests, OCI config, SBOM, provenance.     |
| Runtime evidence    | Community recipes, compatibility DB, health reports, known regressions.        |
| Hardware            | CPU arch, GPU, video encode/decode, audio, input devices, TPM, virtualization. |
| Workspace           | Work, Gaming, Lab, Family, Admin, Airgap, RT.                                  |
| Security profile    | DEV_RELAXED, SECURE_DEFAULT, STIG_ALIGNED, AIRGAP_HIGH.                        |
| Operator preference | Prefer native, prefer secure, prefer performance, prefer reproducible.         |

## 3. Candidate scoring

The solver scores candidate paths:

```text
candidate_score =
  security_score
+ compatibility_score
+ reproducibility_score
+ rollback_score
+ hardware_fit_score
+ update_reliability_score
- runtime_cost
- policy_risk
- maintenance_risk
```

Candidate paths:

| Path                    | When preferred                                                             |
| ----------------------- | -------------------------------------------------------------------------- |
| `NATIVE_CAPSULE`        | Clean Linux app with declared capabilities.                                |
| `FLATPAK_STYLE_CAPSULE` | GUI app with portal support.                                               |
| `NIX_CAPSULE`           | Reproducible CLI/dev/dependency-heavy app.                                 |
| `APPIMAGE_CAPSULE`      | Portable binary, but sandboxed after extraction.                           |
| `OCI_CAPSULE`           | Service/server workload.                                                   |
| `WINDOWS_APP_CAPSULE`   | Windows app works under Wine/Proton-style runner.                          |
| `WINDOWS_GAME_CAPSULE`  | Windows game works under Proton/Wine-GE path.                              |
| `ANDROID_CAPSULE`       | Android app path is safer/more compatible.                                 |
| `VM_CAPSULE`            | Kernel driver, hard DRM, anti-cheat, fragile ABI, unsupported OS behavior. |
| `BLOCKED_WITH_REASON`   | Legally impossible, unsafe, unsupported, or cannot be verified.            |

## 4. Lifecycle FSM

```text
DISCOVERED
  -> OBSERVED
  -> SOLVED
  -> STAGED
  -> APPROVED
  -> INSTALLED
  -> FIRST_LAUNCH_PROBED
  -> HEALTHY
  -> DEGRADED
  -> REPAIRING
  -> HEALTHY
  -> UPDATED
  -> ROLLED_BACK
  -> QUARANTINED
  -> REMOVED
  -> BLOCKED_WITH_REASON
```

Allowed transitions:

| From                  | To                    | Gate                                                       |
| --------------------- | --------------------- | ---------------------------------------------------------- |
| `DISCOVERED`          | `OBSERVED`            | App Lab observation.                                       |
| `OBSERVED`            | `SOLVED`              | Solver emits candidate plan.                               |
| `SOLVED`              | `STAGED`              | Artifacts fetched and verified.                            |
| `STAGED`              | `APPROVED`            | Human/policy approval when needed.                         |
| `APPROVED`            | `INSTALLED`           | Install plan applies in capsule scope.                     |
| `INSTALLED`           | `FIRST_LAUNCH_PROBED` | First launch observation.                                  |
| `FIRST_LAUNCH_PROBED` | `HEALTHY`             | Health checks pass.                                        |
| `HEALTHY`             | `DEGRADED`            | Declared feature breaks.                                   |
| `DEGRADED`            | `REPAIRING`           | Snapshot and repair approved.                              |
| `REPAIRING`           | `HEALTHY`             | Repair checks pass.                                        |
| `HEALTHY`             | `UPDATED`             | Candidate update promoted.                                 |
| Any active state      | `QUARANTINED`         | Breakout, tamper, malware, capability lie, severe failure. |
| Any active state      | `ROLLED_BACK`         | Snapshot rollback succeeds.                                |
| Any non-system state  | `REMOVED`             | Operator removes capsule.                                  |

`BLOCKED_WITH_REASON` is terminal for that candidate path, not necessarily for
the app. The solver may present alternatives.

## 5. Install flow

```text
search/select app
  -> identify variants
  -> score runtime candidates
  -> show recommended plan and alternatives
  -> stage capsule
  -> dry-run installer/scripts in App Lab
  -> translate mutations to typed actions
  -> policy approval
  -> create capsule
  -> first launch probe
  -> health check
  -> promote to workspace
```

Install denies:

- host mutation outside capsule scope
- broad home access without explicit operator approval
- Docker socket request
- unknown privileged helper
- unsigned kernel module
- new background service without approval
- untrusted source in STIG_ALIGNED without exception

## 6. Update flow

Updates are applied to a clone:

```text
active capsule
  -> clone candidate
  -> apply update to candidate
  -> run installer and first-launch probes
  -> compare old/new capabilities
  -> run health checks
  -> promote candidate or discard
```

Promotion requires:

- artifacts verified
- new capabilities approved
- data migration successful
- health checks pass or degradation accepted
- rollback snapshot exists

## 7. Repair flow

```text
degraded capsule
  -> create snapshot
  -> run Capsule Doctor diagnosis
  -> propose repair plan
  -> policy approval if capabilities or trust change
  -> apply repair to clone or safe mutable state
  -> health check
  -> promote or rollback repair
```

Repair actions:

- install missing dependency recipe
- switch runner to known-good version
- restore registry snapshot
- rebuild prefix
- repair file bridge
- reset cache
- rebuild shader cache
- switch graphics backend
- route to VM fallback

## 8. Export/import flow

Capsules can move across machines or into an airgap mirror.

Export package:

```text
capsule manifest
app passport
source artifact refs
runtime/dependency lockfiles
snapshots
state/config/export data selected by operator
compatibility notes
evidence references
signatures
```

Import requires:

- signature verification
- hardware fit check
- security profile check
- dependency availability check
- policy approval
- import health check

## 9. Fallback logic

Fallback order is policy-dependent. Default:

```text
native/portal
  -> reproducible env
  -> app/container capsule
  -> Windows/Android compatibility capsule
  -> VM capsule
  -> remote/cloud bridge
  -> BLOCKED_WITH_REASON
```

Gaming profile may prefer Proton before VM. Admin/STIG profiles may prefer VM
before broad native privileges.

## 10. Evidence records

S17.2 emits no private evidence vocabulary. All capsule evidence record names
are drawn from the single canonical CLOSED enum `CapsuleEvidenceRecordType`,
OWNED and defined in S17.4 §7. The lifecycle flows in this sub-spec use the
following members of that enum:

- `CAPSULE_SOLVED` — solver emits a candidate plan (§2-§3).
- `CAPSULE_STAGED` — artifacts fetched and verified (§4-§5).
- `CAPSULE_APPROVED` — human/policy approval recorded (§4-§5).
- `CAPSULE_INSTALLED` — install plan applied in capsule scope (§5).
- `CAPSULE_FIRST_LAUNCH_PROBED` — first-launch observation (§4-§5).
- `CAPSULE_HEALTH_CHECK` — health checks run (§4-§7).
- `CAPSULE_UPDATED` — candidate update promoted (§6).
- `CAPSULE_REPAIR_STARTED` — repair execution begins (§7).
- `CAPSULE_REPAIR_COMPLETED` — repair closes (§7).
- `CAPSULE_ROLLBACK_COMPLETED` — snapshot rollback succeeds (§6-§7).
- `CAPSULE_QUARANTINED` — capsule quarantined (§4).
- `CAPSULE_REMOVED` — operator removes capsule (§4).
- `CAPSULE_BLOCKED_WITH_REASON` — candidate path is blocked (§3-§4, §9).
- `CAPSULE_EXPORT_CREATED` — export package created (§8).
- `CAPSULE_IMPORT_COMPLETED` — import completes after verification (§8).

No name is redefined here; if a flow needs a new event, the enum in S17.4 §7 is
extended there, not in this file.

## 11. Non-goals

- The solver does not promise a runnable path for every app — blocked-with-reason is a first-class outcome.
- Staging and dry-run never mutate the live host.
- The solver proposes; it is not an authority — the Policy Kernel decides and the Capability Runtime executes.
- No lifecycle transition skips evidence emission or a known rollback path.

## 12. Acceptance criteria

S17.2 is `REAL` only when:

1. Solver can emit multiple candidate plans for one app.
2. Solver rejects unsafe host mutation paths.
3. Staged update clone can be discarded without touching active capsule.
4. Repair creates a snapshot before mutation.
5. VM fallback is represented as a typed plan, not an ad hoc suggestion.
6. Every lifecycle transition emits evidence or an explicit dry-run result.
