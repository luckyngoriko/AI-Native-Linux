# S17.4 - Reliability, Security, and Evidence

| Field     | Value                                                                                                                                                         |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-28; awaiting implementation evidence)                                                                                             |
| Phase tag | S17.4                                                                                                                                                         |
| Layer     | L4/L6/L8/L9/S16 cross-cutting                                                                                                                                 |
| Consumes  | S17.1 AppCapsule, S2.3 Policy Kernel, S3.1 Evidence Log, S16.2 SELinux MAC Policy Plane, S3.2 Sandbox Composition, S8.1 Network Policy, S8.2 Video/GPU Policy |
| Produces  | health model, repair contract, snapshot/rollback contract, quarantine rules, evidence vocabulary                                                              |

## 1. Purpose

S17 reliability means a capsule is reproducible, pinned, health-checked,
repairable, rollbackable, and explainable when blocked.

It does not mean every application always runs. It means failures are bounded,
diagnosed, recoverable where possible, and honest.

Invariant links: INV-005, INV-008, INV-014, INV-015, INV-017, INV-024.

## 2. Health model

```text
CapsuleHealth =
  UNKNOWN
| HEALTHY
| DEGRADED
| BROKEN
| QUARANTINED
| BLOCKED_WITH_REASON
```

Health checks:

| Check                       | Applies to                      |
| --------------------------- | ------------------------------- |
| `artifact_integrity`        | All capsules.                   |
| `manifest_layout_valid`     | All capsules.                   |
| `runtime_available`         | All capsules.                   |
| `dependency_lock_satisfied` | All capsules with dependencies. |
| `sandbox_profile_loaded`    | All non-system capsules.        |
| `network_policy_loaded`     | Capsules with network.          |
| `window_appeared`           | GUI apps/games.                 |
| `audio_init`                | Apps/games with audio.          |
| `gpu_init`                  | Apps/games with GPU.            |
| `video_path_valid`          | Video/capture/stream apps.      |
| `save_path_writable`        | Games and stateful apps.        |
| `service_started`           | Service/OCI/K8s capsules.       |
| `rt_latency_budget_met`     | RT capsules.                    |

## 3. Snapshot and rollback

Snapshots are required before:

- update
- dependency install/remove
- runner switch
- repair
- registry mutation
- prefix rebuild
- security profile tightening that changes capsule permissions
- data migration

Snapshot contents:

```text
code version
runtime lock
dependency lock
config
state metadata
registry/prefix for Windows capsules
network/device/capability policy
health result
evidence links
```

Rollback rule:

- rollback restores the previous known-good set
- rollback never restores a vulnerability-blocked version unless recovery
  explicitly approves and records compensating controls
- rollback emits `CAPSULE_ROLLBACK_COMPLETED` (the canonical
  `CapsuleEvidenceRecordType` value for this event; see §7)

## 4. Capsule Doctor

`Capsule Doctor` diagnoses and proposes repairs. It never mutates without a
snapshot.

Diagnosis categories:

| Category               | Examples                                                             |
| ---------------------- | -------------------------------------------------------------------- |
| Missing dependency     | vcredist, dotnet, d3dx, font, codec, shared library.                 |
| Runner mismatch        | Wine/Proton regression, missing 32-bit support, wrong runner family. |
| Prefix corruption      | registry damage, broken drive_c layout, wrong architecture.          |
| Permission denial      | SELinux, portal, file bridge, GPU, camera, microphone, network.      |
| Hardware mismatch      | GPU missing, video encode unavailable, virtualization absent.        |
| Data migration failure | app update changed state format.                                     |
| Vendor blocker         | DRM/anti-cheat/refusal, license, unsupported platform.               |

Repair plan shape:

```yaml
repair_plan:
  capsule_id: "cap_..."
  diagnosis: "missing_dependency"
  proposed_actions:
    - install_dependency_recipe: "win-dep:vcredist2019"
  requires_policy_approval: true
  creates_snapshot: true
  rollback_on_failure: true
```

## 5. Quarantine

Quarantine triggers:

- artifact hash mismatch
- capability lie
- sandbox breakout attempt
- malware detection
- unauthorized network/device access
- evidence tamper attempt
- unsafe dependency recipe
- repeated crash during privileged phase
- manual operator quarantine

Quarantine rules:

- launch is blocked
- data is preserved read-only
- evidence and support bundle remain available
- delete/export requires operator decision
- AI subjects cannot unquarantine
- recovery may restore known-good snapshot

## 6. Security integration

| Control       | Rule                                                                      |
| ------------- | ------------------------------------------------------------------------- |
| Policy Kernel | Every install/update/repair/delete/export action is a typed action.       |
| SELinux       | Capsule domains/types enforce app/workspace boundaries.                   |
| Sandbox       | Namespaces/seccomp/cgroups/Landlock/portals apply by capsule type.        |
| Network       | Egress/inbound grants come from manifest and policy.                      |
| Vault         | Secrets are brokered, scoped, revocable.                                  |
| Evidence      | State changes and denials are append-only records.                        |
| S16 profiles  | `STIG_ALIGNED` tightens capsule defaults and blocks untrusted exceptions. |

Hard denies:

| Policy id                            | Denied action                                        |
| ------------------------------------ | ---------------------------------------------------- |
| `hd.s17.broad_home_default`          | Capsule requests broad home access by default.       |
| `hd.s17.docker_socket_default`       | Capsule requests Docker socket by default.           |
| `hd.s17.windows_global_wine`         | Managed Windows capsule uses global `~/.wine`.       |
| `hd.s17.repair_without_snapshot`     | Repair mutates capsule without snapshot.             |
| `hd.s17.silent_prefix_share`         | Windows prefix shared without visible membership.    |
| `hd.s17.plugin_parent_trust_inherit` | Plugin inherits parent trust without its own policy. |
| `hd.s17.ai_unquarantine`             | AI subject attempts to unquarantine capsule.         |

## 7. Evidence vocabulary

`CapsuleEvidenceRecordType` is the single canonical CLOSED enum of every
evidence record the S17 App Capsule Runtime emits. It is OWNED and defined here
in S17.4 (the evidence-owner sub-spec); all other S17 sub-specs — including the
S17.2 lifecycle records (S17.2 §10) and the S17.3 Windows-class records —
reference this enum and MUST NOT restate a divergent copy. Exactly one record
name denotes each event, and no event has two names.

```text
CapsuleEvidenceRecordType =
  CAPSULE_MANIFEST_LOADED
| CAPSULE_LAYOUT_VALIDATED
| CAPSULE_POLICY_DECISION
| CAPSULE_SOLVED
| CAPSULE_STAGED
| CAPSULE_APPROVED
| CAPSULE_INSTALLED
| CAPSULE_LAUNCH_STARTED
| CAPSULE_FIRST_LAUNCH_PROBED
| CAPSULE_LAUNCH_COMPLETED
| CAPSULE_HEALTH_CHECK
| CAPSULE_SNAPSHOT_CREATED
| CAPSULE_UPDATED
| CAPSULE_REPAIR_PLAN_CREATED
| CAPSULE_REPAIR_STARTED
| CAPSULE_REPAIR_COMPLETED
| CAPSULE_ROLLBACK_COMPLETED
| CAPSULE_QUARANTINED
| CAPSULE_UNQUARANTINE_REQUESTED
| CAPSULE_REMOVED
| CAPSULE_BLOCKED_WITH_REASON
| CAPSULE_EXPORT_CREATED
| CAPSULE_IMPORT_COMPLETED
| CAPSULE_BREAKOUT_ATTEMPT
| WINDOWS_PREFIX_SNAPSHOT_CREATED
| WINDOWS_DEPENDENCY_RECIPE_APPLIED
| WINDOWS_RUNNER_SWITCHED
| WINDOWS_ANTICHEAT_STATUS_RECORDED
```

Unknown values are rejected by the evidence appender.

Event/name notes (so no two names denote the same event):

- Rollback completion is `CAPSULE_ROLLBACK_COMPLETED` only. The earlier
  `CAPSULE_ROLLED_BACK` spelling is retired and MUST NOT be emitted.
- Export is `CAPSULE_EXPORT_CREATED` only; import is `CAPSULE_IMPORT_COMPLETED`
  only. The earlier `CAPSULE_EXPORTED` / `CAPSULE_IMPORTED` spellings are
  retired.
- `CAPSULE_REPAIR_PLAN_CREATED` (Capsule Doctor emits a repair plan, §4) and
  `CAPSULE_REPAIR_STARTED` (repair execution begins, S17.2 §7) are distinct
  events and both are retained; `CAPSULE_REPAIR_COMPLETED` closes the repair.

## 8. Support bundle

```text
CapsuleSupportBundle
  manifest
  app passport
  capsule type
  source artifact refs
  runtime lock
  dependency locks
  health checks
  last known-good snapshot
  crash summaries
  redacted logs
  policy denials
  SELinux denials
  network denials
  GPU/audio/video diagnostics
  anti-cheat/DRM status
  rollback points
```

Support bundle rules:

- redacted by default
- no raw secrets
- no raw document contents
- can be exported offline
- evidence links remain verifiable

## 9. Non-goals

- "Works reliably" never means "every app always runs"; it means reproducible, pinned, health-checked, repairable, rollbackable, and honest when blocked.
- Repair never proceeds without a snapshot taken first.
- Quarantine preserves user data and evidence; it does not silently wipe.
- Capsule breakouts are high-severity evidence, never silently ignored.

## 10. Acceptance criteria

S17.4 is `REAL` only when:

1. Health checks can mark `HEALTHY`, `DEGRADED`, `BROKEN`, and
   `BLOCKED_WITH_REASON`.
2. Repair attempts require a snapshot.
3. Quarantine blocks launch but preserves data and evidence.
4. Hard-deny rules are enforceable by Policy Kernel.
5. Support bundle generation redacts secrets and user document contents.
6. `STIG_ALIGNED` profile blocks broad default host access.
7. Windows prefix snapshot evidence is emitted before registry mutation.
