# Recovery Boundary (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists, structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Phase tag      | S9.1                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Layer          | L1 Kernel, Bootstrap, Recovery                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| Schema package | `aios.recovery.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Consumes       | **Imports vocabulary from**: S5.1 (`SessionClass.RECOVERY`, `_system` scope subjects — closed-enum identifier shape; type-level), S4.1 (`/aios/system/...` recovery-only mutation classes — closed path enum; type-level). **Requires for correctness (degraded subset only)**: S2.3 Policy Kernel hard-deny `RecoveryRequiredForSystemMutation` (the recovery boot path requires the _degraded_ policy-kernel-with-recovery-bundle subset to evaluate the deny — not L4 phase-5 fully operational; recovery-bundle is loaded statically at boot from signed material), S5.4 Emergency Override `STRONG_SOLO` recovery-only (recovery boot requires the override stack present in degraded form). **Inverted direction (vocabulary owned here, consumed upward)**: S7.1 Surface + Composition Model — the recovery-surface-stack-restriction is a vocabulary constraint _produced by S9.1_ and _consumed by L7_, not the other way around; this declaration is flagged as an inversion to be cleaned up in W11+. **Peer (intra-L0)**: INV-001, INV-004, INV-012, INV-022. |
| Produces       | typed `RecoveryMode` / `RecoveryEntryReason` / `RecoveryExitReason` / `RecoveryStage` / `RecoveryMutableScope` / `RecoveryReadOnlyScope` / `RecoveryDeniedClass` / `RecoveryNetworkPosture` enums; the boot-path FSM; the mount-discipline contract; ten FOREVER-retained evidence record types; the operator-authentication discipline; the 8-hour hard cap; the no-L5-in-recovery contract; the exit-by-reboot rule; brief A/B fallback contract                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |

## §1 Purpose

The recovery boundary is the most-cited concept in the AIOS constitution. It appears in INV-001 (recovery independent of L5), INV-004 (recovery boundary preserved), INV-012 (recovery required for system mutation), and INV-022 (recovery aesthetically distinct). It is consumed by L4 vault `RevealSecret` (recovery-mode required for raw human reveal), L4 Policy Kernel hard-deny `RecoveryRequiredForSystemMutation` (S2.3 §26.2.2), L4.5 emergency override `STRONG_SOLO` (recovery-boot-only solo override), L7.1 surface composition recovery surface stack, L7.5 web renderer `RECOVERY` exposure state, and L4.3 identity model `SessionClass.RECOVERY`.

Until this spec, no contract specified what recovery actually **is**. Every consumer cited "recovery mode" as if its meaning were obvious. This spec closes that loop. It defines, in concrete filesystem and process terms, the physical-software contract that every other "recovery"-mentioning spec is consuming.

After this spec is in force, every reference to recovery in the AIOS contract bundle resolves to a **single, closed, mechanical** concept:

- a separate kernel command line / GRUB entry,
- a boot path that does not mount `/aios`,
- a closed set of mutable system scopes,
- a closed set of denied operations,
- a hard 8-hour TTL,
- exit only by reboot,
- FOREVER-retained evidence for every recovery operation.

## §2 Scope

This spec **defines**:

1. The three constitutional roots `/`, `/root`, `/aios` and what each is for.
2. The closed `RecoveryMode` enum and its four values.
3. The closed `RecoveryEntryReason` enum (why recovery may be entered).
4. The closed `RecoveryExitReason` enum (how recovery exits — there is exactly one normal exit path).
5. The closed `RecoveryStage` FSM (the linear stages of a recovery boot).
6. The closed `RecoveryMutableScope` enum (paths/topics mutable **only** in recovery).
7. The closed `RecoveryReadOnlyScope` enum (paths visible but not writable in recovery).
8. The closed `RecoveryDeniedClass` enum (operations forbidden even in recovery).
9. The closed `RecoveryNetworkPosture` enum (loopback / LAN-for-provisioning / airgap).
10. The mount discipline (per-path normal-mode vs recovery-mode mount state).
11. The operator-authentication contract for recovery sessions.
12. The 8-hour hard cap and its constitutional immutability.
13. The "no L5 in recovery" contract.
14. The "exit by reboot only" contract.
15. A brief A/B kernel fallback contract (full pipeline lives in S9.3).
16. Ten FOREVER-retained evidence record types queued for S3.1.
17. Three worked examples covering normal boot, planned recovery, and auto-recovery.

This spec **does not** define:

- The first-boot installer flow — owned by S9.2.
- The dedicated kernel A/B promotion pipeline — owned by S9.3 (only briefed here).
- Hardware attestation in recovery — deferred.
- Multi-host coordinated recovery — deferred (single-host now).
- The visual language details of the recovery aesthetic — owned by L7.X visual language; this spec consumes the constraint via INV-022.
- The mechanics of approval delivery in recovery — owned by S5.3.
- The vault broker's `RevealSecret` flow that consumes `recovery_mode = true` — owned by S5.2.

This spec is the **contract surface** that those other specs reference when they say "recovery". A change to the meaning of recovery is a change to this spec, never to a consumer's interpretation.

## §3 Vocabulary (closed enums)

All vocabularies in this section are closed. Adding a value is a versioned spec change. Bundle load fails on unknown values. None of these enums admits an `OPEN` or `OTHER` value; the intent is to make recovery semantics fully mechanical.

### §3.1 The three constitutional roots

The host filesystem has exactly three constitutional roots. They are not configurable. They are not arrayed in a "list of roots"; the tuple `(/, /root, /aios)` is a constitutional tuple of three elements.

| Root    | Owner    | Mounted in NORMAL                                                                       | Mounted in RECOVERY                                                                             | Purpose                                                                                                                                                  |
| ------- | -------- | --------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/`     | system   | rw, generic Linux substrate; immutable post-install except via package manager + policy | rw, generic Linux substrate; same image as normal mode                                          | The Linux substrate. Boots without `/aios`. No L5 cognition runs from here. Hosts the kernel, init, libc, recovery toolbox, AIOS daemons                 |
| `/root` | operator | rw, root user's home; never AI-readable per INV-004                                     | rw, operator's emergency-repair island; identical content to normal mode                        | Operator's private home; emergency repair scripts, ssh keys for out-of-band recovery, operator's personal notes. Never traversed by AI subjects          |
| `/aios` | AIOS-FS  | rw via the AIOS-FS object projection (S4.1)                                             | **NOT MOUNTED**. A read-only forensic attach is possible on operator request (FOREVER evidence) | The AI-native semantic root. AIOS-FS objects, services, agents, group/user data live here. The L5 cognitive core runs **only** when this root is mounted |

Constitutional invariants binding this section:

- **INV-004 — Recovery boundary preserved.** These three roots are constitutional. AI subjects cannot read `/root`. `/` is immutable post-install. `/aios` is the only AI-readable/writable root.
- **INV-001 — Recovery independent of L5.** `/` boots without `/aios`. No L5 binary on `/` is started during recovery boot. The L5 systemd units are physically present on `/` but masked in the recovery boot path (§9).

### §3.2 `RecoveryMode`

The boot-time mode classification. A running AIOS host is always in exactly one of these modes.

| Value        | Meaning                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `NORMAL`     | Full AIOS stack running. `/aios` mounted. L5 active. All renderers permitted. Web UI may be exposed per INV-006. AI subjects authenticated. The default and overwhelmingly common mode.                                                                                                                                                                                                                                                                                                                            |
| `RECOVERY`   | Recovery boot path active. `/aios` **not** mounted. L5 services masked at unit level. Only L0 + L1 + L4 (degraded) running. Identity service in degraded mode (only `_system` scope subjects). Network defaults to `LOOPBACK_ONLY`.                                                                                                                                                                                                                                                                                |
| `DEGRADED`   | Abnormal normal-mode. Some services failed health checks but the system did not yet decide to enter recovery. **Not** recovery; closed-loop transition to recovery is via reboot, not via in-place flag change. The kernel may schedule auto-recovery after N consecutive degraded boots.                                                                                                                                                                                                                          |
| `FIRST_BOOT` | First-boot installer active. `/aios` is being initialised. System mutations of `RecoveryMutableScope` paths are permitted **only** for the canonical first-boot service subjects enumerated in S9.2 §3.2.8 (`installer`, `vault-init`, `identity-init`, `policy-compiler`, `firstboot-coordinator`). Self-extinguishing: the mode terminates atomically the moment the firstboot marker is written at `STAGE_FIRST_BOOT_COMPLETE` and the host transitions directly to `NORMAL`. See §3.2.1 for write permissions. |

There is no fifth mode. `MAINTENANCE`, `SAFE`, and `LIVE` are **not** AIOS modes; they are concepts from other operating systems. Any spec or implementation that introduces a fifth mode value is in violation of this contract.

`DEGRADED → RECOVERY` is **not** an in-place transition. It is always mediated by a reboot into the recovery boot path. The constitutional separation between normal and recovery is enforced by the kernel command line, not by a runtime flag. A degraded normal-mode host cannot acquire recovery-mode privileges by flipping a flag in a running process.

`FIRST_BOOT` is similarly enforced at the kernel command line (the installer media writes a dedicated GRUB entry per S9.2 §4.1) and is not an in-place flag flip from any other mode. The mode is active only during S9.2 stages `STAGE_INSTALLER_MEDIA_VERIFIED` through `STAGE_RUNTIME_SERVICES_STARTED`. It self-extinguishes when `STAGE_FIRST_BOOT_COMPLETE` writes the marker; the immediately-subsequent boot enters `NORMAL` via the rewritten GRUB entry table. `FIRST_BOOT` and `RECOVERY` are distinct phases per S9.2 §1: a first-boot subject session does **not** carry `is_recovery_mode = true`, and a recovery subject session does **not** carry `is_first_boot = true`.

**Mutual-exclusion invariant.** At most one of `subject.is_first_boot` and `subject.is_recovery_mode` may be true on any subject session. A session that carries both flags is itself a contract violation; the Policy Kernel rejects it at admission with `MutuallyExclusiveModeFlags`. The two flags name two disjoint constitutional phases (provisioning vs. operator-driven recovery), and conflating them would let a first-boot service subject inherit recovery-mode privileges or vice versa.

#### §3.2.1 `FIRST_BOOT` mode write permissions

When `RecoveryMode = FIRST_BOOT`, the canonical first-boot service subjects (S9.2 §3.2.8) may mutate the following `RecoveryMutableScope` paths, scoped to the stage at which each mutation is needed. No other subject — including any HUMAN_USER subject created at `STAGE_FIRST_USER_REGISTRATION` — receives first-boot write privileges. AI subjects (`is_ai = true`) receive no first-boot write privileges; the constitutional barrier against AI provisioning is preserved.

| `RecoveryMutableScope` value     | Permitted first-boot writer subjects                                     | Stage at which the write occurs                                                                                                                                                                                                                                                                         |
| -------------------------------- | ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `INVARIANT_BUNDLE`               | `_system:service:installer`                                              | `STAGE_INVARIANT_BUNDLE_LOADED`                                                                                                                                                                                                                                                                         |
| `POLICY_BUNDLE`                  | `_system:service:installer`, `_system:service:policy-compiler`           | `STAGE_POLICY_BUNDLE_LOADED`                                                                                                                                                                                                                                                                            |
| `IDENTITY_BUNDLE`                | `_system:service:installer`, `_system:service:identity-init`             | `STAGE_IDENTITY_BUNDLE_LOADED`, `STAGE_FIRST_GROUP_REGISTRATION`, `STAGE_FIRST_USER_REGISTRATION`                                                                                                                                                                                                       |
| `VAULT_ROOT_MATERIAL`            | `_system:service:installer`, `_system:service:vault-init`                | `STAGE_VAULT_ROOT_GENERATED`                                                                                                                                                                                                                                                                            |
| `RECOVERY_OPERATOR_REGISTRATION` | `_system:service:identity-init`, `_system:service:firstboot-coordinator` | `STAGE_RECOVERY_OPERATOR_REGISTRATION`                                                                                                                                                                                                                                                                  |
| `CAPABILITY_CATALOG`             | `_system:service:installer`                                              | `STAGE_POLICY_BUNDLE_LOADED` (catalog is loaded together with the policy bundle)                                                                                                                                                                                                                        |
| `L1_BOOT_PARAMETERS`             | `_system:service:installer`, `_system:service:firstboot-coordinator`     | `STAGE_FIRST_BOOT_COMPLETE` (GRUB rewrite to the post-first-boot entry table)                                                                                                                                                                                                                           |
| `DEDICATED_KERNEL_PROMOTION`     | (none)                                                                   | First-boot does not promote a dedicated kernel; the generic kernel is the only kernel during and immediately after first-boot. Dedicated-kernel promotion is a post-first-boot recovery-mode operation per S9.1 §11.                                                                                    |
| `SYS_FIRSTBOOT_RESET`            | (none)                                                                   | First-boot mode does **not** permit writing the firstboot marker reset path. The marker does not yet exist during `FIRST_BOOT` mode (it is written **at** `STAGE_FIRST_BOOT_COMPLETE`); resetting it is a recovery-only factory-reset operation (S9.2 §11.2 / S10.1 W8.1.9 `recovery.firstboot.reset`). |
| `FIRMWARE_VERSION_COUNTER`       | (none)                                                                   | First-boot mode does **not** permit resetting the firmware version monotonicity counter. The counter is **established** during first-boot (initial firmware version baseline) but is never reset during first-boot. Counter reset is a recovery-only firmware downgrade operation per S8.5.             |

Each write under `FIRST_BOOT` mode emits a FOREVER-retained `FIRST_BOOT_OPERATION` evidence record (queued for S3.1 vocabulary alongside the W10 update) carrying:

- `subject_canonical_id` — the writing service subject (one of the canonical first-boot subjects above).
- `target_path` — the `/aios/system/...` path written.
- `target_scope` — the `RecoveryMutableScope` value covering `target_path`.
- `stage` — the `FirstBootStage` (S9.2 §3.2) at which the mutation occurred.
- `firstboot_session_id` — the unique id of the active first-boot session (assigned by the firstboot-coordinator at session bootstrap; one session per first-boot run).
- `committed_at` — the rfc3339 timestamp at which the write committed.

The `FIRST_BOOT_OPERATION` records form an audit trail across the constitutional bootstrap: the sequence of records for a given `firstboot_session_id`, replayed against the `FIRST_BOOT_STARTED` and `FIRST_BOOT_COMPLETE` records of S9.2 §12, fully reconstructs which subject mutated which constitutional path during which stage. INV-005 (evidence append-only) holds; first-boot does not loosen it.

`FIRST_BOOT` mode does **not** grant write capability for `RecoveryReadOnlyScope` paths (§3.7). Group data (`AIOSFS_GROUP_DATA`) is irrelevant during first-boot — no group data exists yet — and the evidence log (`EVIDENCE_LOG_TAIL`) remains append-only with all writes mediated by the kernel's bound RPCs. `FIRST_BOOT` mode does **not** loosen `RecoveryDeniedClass` (§3.8): AI agent execution remains denied (no L5 cognition during first-boot per S9.2 §1), Web public exposure remains denied, evidence log rewrite remains denied, third-party binary execution remains denied, and LAN public exposure remains denied. The first-boot installer surface is an `AIOS_SURFACE`-only renderer (per L7.1 §6.3); it does not embed the L7 Web renderer. The constitutional floor of recovery is also the constitutional floor of first-boot.

### §3.3 `RecoveryEntryReason`

The closed list of reasons recovery may be entered. Every recovery boot carries exactly one of these reasons; the reason is recorded in the `RECOVERY_BOOT_ENTERED` evidence record (§12) at FOREVER retention.

| Value                                | Trigger                                                                                                                                                                                                     |
| ------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `OPERATOR_INITIATED`                 | Operator selected the recovery entry from GRUB intentionally. The most common entry reason.                                                                                                                 |
| `BOOT_FAILURE_AUTO`                  | N consecutive normal-boot failures (default `N = 3`) triggered automatic recovery. Counter is in `/`'s GRUB state.                                                                                          |
| `INVARIANT_BUNDLE_SIGNATURE_FAILURE` | The L0 invariant bundle (`invbundle_<hex>`) failed Ed25519 verification at startup.                                                                                                                         |
| `POLICY_BUNDLE_CORRUPTION`           | The Policy Kernel could not load any valid bundle from `/aios/system/policy/`. (Implies `/aios` was reachable but the bundle is unusable; recovery is entered via reboot to avoid a partial-state runtime.) |
| `AIOSFS_ROOT_UNRESOLVABLE`           | The L2 mount of `/aios` failed beyond retry budget.                                                                                                                                                         |
| `VAULT_ROOT_KEY_UNAVAILABLE`         | The L4 vault broker could not unlock its master key (TPM unseal failed, hardware key absent, etc.).                                                                                                         |
| `IDENTITY_BUNDLE_FAILURE`            | The L4 identity bundle (`idbundle_<hex>`) failed signature verification or could not be parsed.                                                                                                             |
| `EVIDENCE_LOG_TAMPER_DETECTED`       | Startup chain verification (S3.1 `VerifyChain`) reported tamper.                                                                                                                                            |

These eight values exhaust the recovery-entry reasons. Recovery cannot be entered "for no reason"; the kernel command line that selects the recovery boot path is itself parameterised by which of these reasons is in play.

### §3.4 `RecoveryExitReason`

The closed list of exit reasons. There is **exactly one** normal exit path (`REBOOT_TO_NORMAL`). Every other exit is abnormal in some way — either time-bounded, operator-bounded, or unrecoverable.

| Value                   | Meaning                                                                                                                                                                                   |
| ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `REBOOT_TO_NORMAL`      | Operator-initiated reboot back to normal mode. The **only** normal exit path. Any other exit is degraded.                                                                                 |
| `TTL_EXPIRED`           | The 8-hour hard cap on the recovery session has been reached. Auto-reboot is triggered. See §8.                                                                                           |
| `OPERATOR_TERMINATED`   | Operator commanded `shutdown` from the recovery shell. The host powers off rather than rebooting; next power-on chooses GRUB entry.                                                       |
| `UNRECOVERABLE_FAILURE` | Recovery itself failed (e.g. recovery toolbox missing, recovery shell crashed, signed operator credential rejected three times). Operator must use external installation media to repair. |

There is **no** `EXIT_TO_NORMAL` value, in the sense of a runtime transition from `RECOVERY` to `NORMAL` without reboot. See §10 for the constitutional rationale.

### §3.5 `RecoveryStage`

The linear FSM during a recovery boot. Stages flow forward only; back-transitions are forbidden. Stage advancement is reported in startup evidence (§12).

| Stage                        | What it represents                                                                                                                                                      |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `STAGE_PRE_KERNEL`           | Firmware / UEFI / GRUB up to the point of selecting the kernel command line.                                                                                            |
| `STAGE_KERNEL_LOADED`        | Kernel image is loaded; recovery command line parsed.                                                                                                                   |
| `STAGE_INITRAMFS`            | Initramfs is running; recovery flag is honoured by the init script.                                                                                                     |
| `STAGE_ROOT_MOUNTED`         | `/` is mounted rw. `/root` is available. `/aios` is **not** mounted.                                                                                                    |
| `STAGE_L0_GOVERNANCE_READY`  | The L0 invariant bundle is loaded (or governance is in degraded mode if signature failed; INV-001/INV-002 still hold).                                                  |
| `STAGE_L4_DEGRADED_READY`    | The L4 identity service is running in degraded mode (only `_system` scope subjects available); vault broker is in degraded mode (no normal-mode capabilities issuable). |
| `STAGE_RECOVERY_SHELL_READY` | The recovery shell (an `AIOS_SURFACE`-only renderer per L7.1 §6.3) is up. Operator can authenticate.                                                                    |
| `STAGE_RECOVERY_ACTIVE`      | Operator authenticated. Recovery operations allowed for the bound `RecoveryMutableScope` set.                                                                           |
| `STAGE_REBOOTING`            | Exit-by-reboot is in progress. The only forward stage from `STAGE_RECOVERY_ACTIVE`.                                                                                     |

Allowed forward transitions:

```text
STAGE_PRE_KERNEL ─▶ STAGE_KERNEL_LOADED ─▶ STAGE_INITRAMFS ─▶ STAGE_ROOT_MOUNTED ─▶
  STAGE_L0_GOVERNANCE_READY ─▶ STAGE_L4_DEGRADED_READY ─▶
  STAGE_RECOVERY_SHELL_READY ─▶ STAGE_RECOVERY_ACTIVE ─▶ STAGE_REBOOTING
```

On-failure behaviour at each stage:

- Failure at `STAGE_PRE_KERNEL` → firmware-level fallback (out of scope; operator uses external media).
- Failure at `STAGE_KERNEL_LOADED` → A/B kernel fallback (§11).
- Failure at `STAGE_INITRAMFS` → emit kernel panic; firmware reboots (after 5s); next boot tries fallback per §11.
- Failure at `STAGE_ROOT_MOUNTED` → drop to dracut emergency shell; operator must use external media.
- Failure at `STAGE_L0_GOVERNANCE_READY` → continue with degraded governance (INV-001 + INV-002 only); proceed to `STAGE_L4_DEGRADED_READY`.
- Failure at `STAGE_L4_DEGRADED_READY` → drop to a constitutional-fallback shell that accepts only the `_system:local:operator-fallback` subject; operator with external recovery credential can still proceed.
- Failure at `STAGE_RECOVERY_SHELL_READY` → emit `RECOVERY_SHELL_FAILED` (queued under the abnormal-exit branch; see §16 deferrals); auto-reboot after 60s.
- Failure at `STAGE_RECOVERY_ACTIVE` → operator-driven; the FSM does not auto-fail this stage. If the operator's session token expires (§8), force `STAGE_REBOOTING` with `TTL_EXPIRED`.

### §3.6 `RecoveryMutableScope`

The closed list of paths/topics mutable **only** in recovery mode. These are the system scopes that INV-012 (recovery required for system mutation) protects. A normal-mode subject cannot mutate any of these paths regardless of capability or approval.

| Value                            | Path                                    | What it holds                                                                                                                                                                                                                                                                                                |
| -------------------------------- | --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `POLICY_BUNDLE`                  | `/aios/system/policy/`                  | The signed policy bundle that drives Policy Kernel decisions (S2.3).                                                                                                                                                                                                                                         |
| `CAPABILITY_CATALOG`             | `/aios/system/capabilities/`            | The capability catalog (S1.1) mapping capability ids to typed action manifests.                                                                                                                                                                                                                              |
| `VAULT_ROOT_MATERIAL`            | `/aios/system/vault/`                   | Vault broker root material (master key wraps, root cert chain, broker config).                                                                                                                                                                                                                               |
| `INVARIANT_BUNDLE`               | `/aios/system/governance/invariants/`   | The L0 invariant bundle (`invbundle_<hex>`) per L0 §4. Includes invariant retirement.                                                                                                                                                                                                                        |
| `IDENTITY_BUNDLE`                | `/aios/system/identity/`                | The L4 identity bundle (`idbundle_<hex>`) including the `_system` scope subjects and group registry.                                                                                                                                                                                                         |
| `RECOVERY_OPERATOR_REGISTRATION` | `/aios/system/recovery/operators/`      | The signed list of recovery operator credentials authorised for `_system:remote:operator-<id>` and `_system:local:operator-<id>`.                                                                                                                                                                            |
| `DEDICATED_KERNEL_PROMOTION`     | `/aios/system/kernel/`                  | The A/B kernel slot configuration. Promotion of a dedicated kernel candidate (§11) writes here.                                                                                                                                                                                                              |
| `L1_BOOT_PARAMETERS`             | `/aios/system/boot/`                    | GRUB configuration, kernel command lines, and the auto-recovery counter / threshold.                                                                                                                                                                                                                         |
| `SYS_FIRSTBOOT_RESET`            | `/aios/system/firstboot/marker.signed`  | The signed first-boot marker. Recovery-only mutation that clears the marker so the next boot re-enters `FIRST_BOOT` mode at `STAGE_INSTALLER_MEDIA_VERIFIED`. Owned by `_system:service:firstboot-coordinator` during recovery-mode reset. Backs the `recovery.firstboot.reset` typed action (S10.1 W8.1.9). |
| `FIRMWARE_VERSION_COUNTER`       | `/aios/system/firmware/version_counter` | The per-device firmware version monotonicity counter. Per S8.5 firmware version monotonicity is enforced (no downgrade) in normal mode; a constitutional firmware roll-back during recovery resets this counter. RECOVERY-ONLY. Owned by `_system:service:firmware-trust-manager`.                           |

Notes:

1. **`/aios` is mounted differently in recovery.** During `STAGE_RECOVERY_ACTIVE`, the recovery boot does **not** mount the full AIOS-FS group/user namespace (`/aios/groups/...`); it mounts only the `system/...` subtree exposed as a recovery projection (see §5).
2. **Mutation does not imply "free hand".** Even in recovery, mutations to these paths emit `RECOVERY_OPERATION_PERFORMED` evidence (FOREVER retention) with the scope and target path. The audit trail is identical for a benign policy bundle update and a malicious one — the difference shows up downstream: a malicious bundle attempting to loosen a constitutional invariant is rejected by the S2.3 bundle compiler at load time (per L0 §5.3 / §7 Fixture 4) and emits `POLICY_BUNDLE_REJECTED` (FOREVER, per S14.1 §4.1) with the rejection reason recorded; a benign update is reflected in the post-recovery operational record.
3. **`GROUP_DATA` is intentionally absent** from this list. Group data (`/aios/groups/<g>/...`) is **not** mutable in recovery. See §3.7 for the read-only forensic attach exception.
4. **`SYS_FIRSTBOOT_RESET` and `FIRMWARE_VERSION_COUNTER` are recovery-only.** These two scopes back the reset-to-factory flow (S9.2 §11.2 + S10.1 W8.1.9 `recovery.firstboot.reset`) and the firmware downgrade flow (S8.5 firmware version downgrade flow + S4.1 W8.1 `SYS_FIRMWARE` recovery treatment). They are **not** writable under `FIRST_BOOT` mode — first-boot is bootstrap, not factory reset; the firstboot marker does not yet exist when `FIRST_BOOT` mode is active, and firmware version monotonicity is established at first-boot, not reset there. See §3.2.1.
5. **Closure of S4.1 W8.4 dependency.** These two values (`SYS_FIRSTBOOT_RESET`, `FIRMWARE_VERSION_COUNTER`) were declared dependent by S4.1 Wave 8 §W8.4 (the namespace layout requirements for `SYS_FIRMWARE` and `SYS_FIRSTBOOT` paths). Wave 10 closes that dependency by adding them to this enum.

### §3.7 `RecoveryReadOnlyScope`

The closed list of paths visible (read) but **not** mutable in recovery. Defense in depth: the recovery operator sees only what the recovery operation requires. Granting write capability for these paths in recovery is a contract violation.

| Value               | Path                                      | Meaning                                                                                                                                                                                                                                                                                                                                                                 |
| ------------------- | ----------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AIOSFS_GROUP_DATA` | `/aios/groups/<group_id>/...`             | Group data. **Not mounted by default in recovery**; the recovery operator does **not** snoop on group content. A read-only forensic attach is possible (§5) and emits FOREVER-retained `RECOVERY_FORENSIC_ATTACH_PERFORMED` evidence with the operator's canonical id and the attached scope.                                                                           |
| `EVIDENCE_LOG_TAIL` | `/aios/system/evidence/<segments>` (read) | The evidence log is **read-only verification access** in recovery. The recovery operator can `VerifyChain` to confirm tamper status, but **cannot append** to the evidence log and **cannot rewrite** it. INV-005 (evidence append-only) holds; recovery does not loosen it. Append is performed by the kernel on behalf of bound RPCs, never by the operator directly. |

### §3.8 `RecoveryDeniedClass`

The closed list of operation classes forbidden **even** in recovery. These are the classes that no `RecoveryMutableScope` grant relaxes. They are the constitutional floor of recovery itself.

| Value                    | Why                                                                                                                                                                                                                                                                                                                             |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AI_AGENT_EXECUTION`     | INV-001. No L5 agent runs in recovery. The L5 binaries are masked. Any attempt to start an L5 service fails closed and emits `RECOVERY_L5_START_BLOCKED` (§9, §12).                                                                                                                                                             |
| `WEB_PUBLIC_EXPOSURE`    | Recovery never exposes the Web UI publicly. The Web renderer is not started in recovery (no L5 → no Web for AI-mediated chrome). If the operator opts in to a recovery-mode local web view (see §5), it binds only to `127.0.0.1` and `::1`, and only on `LOOPBACK_ONLY` posture.                                               |
| `LAN_NETWORK_OPEN`       | Recovery defaults to `LOOPBACK_ONLY`. LAN networking is opt-in, time-boxed, and FOREVER-evidenced (§3.9). The default is closed because recovery sessions are high-privilege; an opportunistic LAN exposure during recovery is a constitutional risk.                                                                           |
| `THIRD_PARTY_BINARY_RUN` | The recovery shell only runs the L1 toolbox (busybox-style core utilities packaged with `/`) and signed AIOS binaries (the recovery diagnostics service, the policy bundle compiler, the vault broker in degraded mode). Arbitrary binaries — even those dropped to `/root` — are rejected by the recovery shell's exec filter. |
| `EVIDENCE_LOG_REWRITE`   | INV-005. Recovery does not rewrite or truncate the evidence log. Any attempt fails closed; see L4.5 `NonOverridableClass` (S5.4 §10) which lists `EVIDENCE_LOG_REWRITE` as constitutionally non-overridable. The recovery path consumes the same constraint.                                                                    |

### §3.9 `RecoveryNetworkPosture`

The closed list of network postures available to a recovery boot. The default is `LOOPBACK_ONLY`. Any other posture requires operator opt-in and emits FOREVER-retained evidence on enable and on close.

| Value                  | Meaning                                                                                                                                                                                                                                                                                                                                                                    |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `LOOPBACK_ONLY`        | The default. Only `127.0.0.1` and `::1` are reachable. The recovery operator authenticates locally (no remote auth in recovery; INV-006 + recovery default).                                                                                                                                                                                                               |
| `LAN_FOR_PROVISIONING` | Operator-enabled, time-boxed, evidence-bound. Used to fetch a signed policy bundle from a known LAN host, register a new recovery operator credential from a known LAN identity provider, or push a new dedicated-kernel candidate. Maximum window: 30 minutes per enable. Emits `RECOVERY_NETWORK_LAN_ENABLED` on enable; emits `RECOVERY_NETWORK_LAN_DISABLED` on close. |
| `AIRGAP`               | No networking at all (loopback included is disabled at the kernel level). Used for hardware repair scenarios where the operator wants the host to perform no IP traffic during the work. The recovery shell continues to function over the local console; no network sockets are bound.                                                                                    |

These three values exhaust the recovery network postures. There is no `INTERNET_OPEN` value; recovery does not expose the host to the public Internet. A recovery operator who needs Internet must do so out-of-band (a separate machine), not from the recovery host.

## §4 Boot path FSM

The recovery boot path is a linear FSM through the `RecoveryStage` values. It is selected at the kernel command line by GRUB, and is parameterised by exactly one `RecoveryEntryReason`. The operator does not choose the reason directly; the reason is set by the boot decision logic (§4.2).

### §4.1 GRUB entries

The signed GRUB configuration includes exactly the following entries:

```text
0: AIOS Normal              (kernel = /boot/vmlinuz-aios       ; aios.mode=NORMAL)
1: AIOS Normal (fallback)   (kernel = /boot/vmlinuz-generic    ; aios.mode=NORMAL ; aios.fallback=1)
2: AIOS Recovery            (kernel = /boot/vmlinuz-aios       ; aios.mode=RECOVERY ; aios.recovery_reason=OPERATOR_INITIATED)
3: AIOS Recovery (fallback) (kernel = /boot/vmlinuz-generic    ; aios.mode=RECOVERY ; aios.recovery_reason=OPERATOR_INITIATED ; aios.fallback=1)
```

Entries 0 and 2 use the dedicated kernel; entries 1 and 3 use the generic fallback kernel (S9.3). Entries 2 and 3 are the recovery boot paths; selecting one of them sets the kernel command line that the initramfs honours at `STAGE_INITRAMFS`.

A non-operator-initiated recovery (auto-recovery) boots entry 2 or 3 (depending on the dedicated-kernel slot health, §11) with `aios.recovery_reason` set to one of the non-operator values from §3.3.

### §4.2 Boot decision logic

The boot decision logic runs at `STAGE_PRE_KERNEL` and selects the boot entry. In pseudo-form:

```text
def select_boot_entry():
    if grub_state.consecutive_normal_boot_failures >= 3:
        return Entry(recovery=True, reason=BOOT_FAILURE_AUTO)
    if dedicated_kernel.health == FAIL and dedicated_kernel.consecutive_fails >= 2:
        return Entry(recovery=False, fallback=True)            # entry 1
    if user_selected_entry is not None:
        return user_selected_entry                              # operator chose from menu
    return Entry(recovery=False, fallback=False)                # entry 0
```

The "consecutive normal boot failures" counter lives in GRUB state (`/aios/system/boot/state` per `RecoveryMutableScope.L1_BOOT_PARAMETERS`). It is incremented on each normal boot that fails to reach `STAGE_RECOVERY_SHELL_READY`'s normal-mode equivalent (i.e. `STAGE_NORMAL_RUNTIME_READY`, defined in S9.2). It is reset to zero on the first successful normal boot.

### §4.3 Worked startup trace

A representative recovery boot from `OPERATOR_INITIATED`:

```text
T+0.0s   STAGE_PRE_KERNEL              GRUB; operator selects "AIOS Recovery".
T+0.4s   STAGE_KERNEL_LOADED           Kernel image loaded; aios.mode=RECOVERY parsed.
T+1.2s   STAGE_INITRAMFS               Init script honours recovery flag; mounts / rw.
T+2.0s   STAGE_ROOT_MOUNTED            / mounted; /root mounted; /aios deliberately NOT mounted.
T+2.4s   STAGE_L0_GOVERNANCE_READY     invbundle_<hex> loaded; signature OK; INV-001..INV-024 active.
T+2.7s   STAGE_L4_DEGRADED_READY       Identity service in degraded mode; only _system subjects.
T+3.0s   STAGE_RECOVERY_SHELL_READY    Recovery shell up (AIOS_SURFACE only per L7.1 §6.3).
T+3.0s   evidence: RECOVERY_BOOT_ENTERED FOREVER {reason=OPERATOR_INITIATED, ...}
T+12.4s  STAGE_RECOVERY_ACTIVE         Operator authenticates with hardware key.
T+12.4s  evidence: RECOVERY_OPERATOR_AUTHENTICATED FOREVER {subject=_system:local:operator-247, ...}
... operator performs N recovery operations, each emitting RECOVERY_OPERATION_PERFORMED FOREVER ...
T+18m    STAGE_REBOOTING               Operator commands `recovery-reboot` from shell.
T+18m    evidence: RECOVERY_BOOT_EXITED FOREVER {reason=REBOOT_TO_NORMAL, ...}
```

A hostile or auto-triggered recovery boot has the same shape but with a different `RecoveryEntryReason` value in the first evidence record and a possibly different exit reason.

## §5 Mount discipline

The mount discipline in recovery is the concrete enforcement of INV-004 (recovery boundary preserved) and INV-012 (recovery required for system mutation). It is the single most-checked artefact of this spec.

### §5.1 Mount table

| Path                                 | Normal-mode mount                                   | Recovery-mode mount                                                                   |
| ------------------------------------ | --------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `/`                                  | rw, generic Linux substrate                         | rw, generic Linux substrate (same image)                                              |
| `/root`                              | rw, operator's home                                 | rw, operator's home (recovery does not isolate `/root` further)                       |
| `/aios`                              | rw via AIOS-FS object projection                    | **NOT MOUNTED**                                                                       |
| `/aios/system/policy`                | ro projection (mutation requires recovery; INV-012) | rw projection (recovery operator only; bound to `RecoveryMutableScope.POLICY_BUNDLE`) |
| `/aios/system/capabilities`          | ro projection                                       | rw projection                                                                         |
| `/aios/system/vault`                 | ro (vault broker mediates all access; INV-018)      | rw projection (degraded vault broker; only key-management operations)                 |
| `/aios/system/governance/invariants` | ro projection                                       | rw projection (invariant bundle update + retirement, see L0 §5.4)                     |
| `/aios/system/identity`              | ro projection                                       | rw projection                                                                         |
| `/aios/system/recovery/operators`    | ro projection                                       | rw projection                                                                         |
| `/aios/system/kernel`                | ro projection                                       | rw projection (A/B kernel slot management)                                            |
| `/aios/system/boot`                  | ro projection                                       | rw projection                                                                         |
| `/aios/system/evidence`              | ro append-only via S3.1 RPCs                        | ro verification-only (no append, no rewrite; INV-005)                                 |
| `/aios/groups/<group_id>/...`        | rw via AIOS-FS                                      | **NOT MOUNTED** by default. Read-only forensic attach optional (see §5.2).            |

The "rw projection" entries refer to AIOS-FS-style typed mutation through the recovery operator's session, **not** to a raw POSIX rw mount of the underlying object store. The recovery operator never edits raw bytes; they submit typed actions whose targets resolve to one of the `RecoveryMutableScope` values, and the kernel performs the mutation through the AIOS-FS API in degraded mode.

### §5.2 Read-only forensic attach

When the recovery operation requires inspecting group data (e.g. to diagnose a failure that involves `/aios/groups/family/...`), the recovery operator can request a **read-only forensic attach**. The procedure:

1. Operator submits a `forensic_attach_request` from the recovery shell.
2. The request includes the target group id and the operator's canonical subject id.
3. The kernel attaches `/aios/groups/<group_id>` read-only to a recovery-only mount point (`/recovery/inspect/<group_id>`).
4. The kernel emits `RECOVERY_FORENSIC_ATTACH_PERFORMED` evidence (FOREVER retention) with the operator id, the group id, the timestamp, and a free-text justification supplied by the operator.
5. The operator can `cat`, `grep`, `verify-chain` against the attached tree but **cannot write**.
6. The attach is automatically released at recovery exit or after 30 minutes, whichever comes first.

The forensic attach is the **only** path through which a recovery operator sees group content. Without an explicit attach + FOREVER evidence, group data is invisible to the recovery operator. This is by design: recovery is high-privilege; we limit what the recovery operator sees to what they need to repair.

### §5.3 What is **not** mountable in recovery

Defense in depth: the following paths are unmountable in recovery, even with operator opt-in.

- Any user's `/root` other than the operator's own (recovery does not give the operator a panopticon over other operators' homes).
- Any AIOS-FS projection that exposes raw secret material from the vault. The vault broker in recovery is in degraded mode and supports only key-management operations (rewrap, rotate, register-new-master); it never returns raw bytes (INV-018 holds in recovery).
- Any L5 model artifact (the L5 directory under `/aios/system/agents/` for cognitive services). Even though `/aios/system/agents/` is in `RecoveryMutableScope.IDENTITY_BUNDLE`'s family by namespace, the L5-specific subdirectory is excluded from recovery mounts because no L5 service runs in recovery (§9).

## §6 Operator authentication in recovery

### §6.1 Recovery prompt

The operator authenticates via a dedicated recovery prompt. The prompt is rendered by the recovery shell, which is itself an `AIOS_SURFACE`-only KWin session per L7.1 §6.3 (`subject.recovery_mode = true` rejects `APP_SURFACE` and `STREAM_SURFACE` kinds). The recovery aesthetic per INV-022 makes the prompt visually unmistakable from the normal-mode authentication prompt.

### §6.2 Authentication factors

Recovery authentication accepts the following factors, in order of preference:

1. **Hardware key (Yubikey, FIDO2, or similar)** — the strong path. Required for new recovery operator registration (`RecoveryMutableScope.RECOVERY_OPERATOR_REGISTRATION`).
2. **TOTP** — acceptable for normal recovery operations.
3. **Passphrase fallback** — allowed but emits `HEAVY_AUTH_FALLBACK_USED` evidence (queued under the L4 evidence vocabulary; see §16) so post-hoc analysis can detect over-reliance on the weakest factor.

A recovery operator credential is registered in `/aios/system/recovery/operators/` (`RecoveryMutableScope.RECOVERY_OPERATOR_REGISTRATION`) by an existing recovery operator. The first-boot installer (S9.2) seeds an initial recovery operator credential; subsequent operators are added by an existing recovery operator with a hardware key.

### §6.3 No remote authentication

Recovery does **not** accept remote authentication. The only authentication path is local console (or, when `LAN_FOR_PROVISIONING` is opted in, a tightly scoped LAN-host-to-recovery-host TLS pinned channel — but even that requires a local console to enable in the first place). This binds INV-006 (Web UI localhost-only by default) and the recovery default `LOOPBACK_ONLY` posture (§3.9).

A recovery operator who is physically remote must arrange physical access to the host or use an out-of-band BMC (IPMI, iDRAC) that itself requires its own authentication. The AIOS recovery path does not bridge BMC to recovery shell; the operator must log into the BMC, get serial console access, and authenticate to the AIOS recovery shell over that console.

### §6.4 Session class

Authenticated recovery sessions have `SessionClass.RECOVERY` per S5.1 §8.1. The session subject is one of the `_system` scope canonical ids:

```text
_system:local:operator-<id>            # local operator at the recovery console
_system:remote:operator-<id>           # remote operator (rare; only via LAN_FOR_PROVISIONING + local console enable)
_system:service:recovery-diagnostics    # the recovery service itself
```

The session carries `recovery_mode = true`, `is_ai = false`, and `expires_at = authenticated_at + 8h` per §8.

## §7 Mutable / read-only / denied scopes

### §7.1 The scope binding

While recovery mode is `STAGE_RECOVERY_ACTIVE`:

- All `RecoveryMutableScope` paths (§3.6) gain write capability for the active recovery operator subject under `_system` scope.
- All `RecoveryReadOnlyScope` paths (§3.7) gain read-only access (no write capability is issued).
- Normal-mode paths under `/aios/groups/<g>/...` are **not** writable in recovery; only read-only attach via §5.2 is permitted on explicit operator request.

### §7.2 The Policy Kernel decision in recovery

A recovery operator's typed action targets one of the `RecoveryMutableScope` paths. The Policy Kernel evaluates the action with `subject.recovery_mode = true`. The S2.3 hard-deny `RecoveryRequiredForSystemMutation` (§26.2.2) is **not** triggered for these scopes because the subject **is** a recovery-mode subject; the hard-deny exists precisely to keep normal-mode subjects out.

A recovery operator cannot mutate paths outside `RecoveryMutableScope` even though they are in recovery mode. The Policy Kernel applies the per-scope rules (`/aios/groups/<g>/...` is denied; `/etc/...` is outside `/aios/` and is governed by `/`'s package manager). Recovery is **not** a "god mode"; it is "constitutional-layer-mode" only.

### §7.3 Denied classes are denied

The `RecoveryDeniedClass` values (§3.8) are denied even in recovery. The Policy Kernel hard-denies them irrespective of `recovery_mode`. Specifically:

- An attempt to start an L5 service from recovery emits `RECOVERY_L5_START_BLOCKED` and fails with reason code `L5StartProhibitedInRecovery`.
- An attempt to expose the Web UI publicly fails with reason `WebPublicExposureProhibitedInRecovery`.
- An attempt to enable LAN networking without first emitting `RECOVERY_NETWORK_LAN_ENABLED` fails with reason `RecoveryNetworkOpenAttempt`.
- An attempt to run a third-party binary from the recovery shell fails with reason `ThirdPartyBinaryExecutionProhibitedInRecovery`.
- An attempt to rewrite the evidence log fails with reason `EvidenceLogRewriteProhibited`.

These reason codes are queued for S3.1 reason-code vocabulary alongside the new evidence record types in §12.

## §8 The 8-hour hard cap

### §8.1 Statement

Recovery sessions have a constitutional hard cap of **8 hours**. The cap is set on `Session.expires_at = authenticated_at + 8h` at recovery operator authentication time. There is **no extension mechanism** in Rev.2.

### §8.2 Why the cap is constitutional

Recovery is high-privilege. The longer a recovery session lives, the larger the blast radius of operator error or compromise. Eight hours is enough for any reasonable repair operation (bundle update, kernel promotion, group registration, recovery operator registration); longer sessions either indicate the operator is doing something they should not be, or that the system is in a state where fresh authentication is cheaper than a longer session.

### §8.3 Why the cap cannot be loosened

Per S5.1 I5 (recovery flag is not loosenable) and §7.3 (recovery session expiry, no extension mechanism in Rev.2), no policy bundle, capability binding, or operator override loosens the cap. An attempt to do so via a policy bundle whose rules would extend the recovery TTL is rejected at S2.3 bundle compile time with `InvariantLooseningAttempted`.

The cap is enforced at the kernel level: a watchdog in the recovery shell monitors `Session.expires_at`, and at expiry triggers `STAGE_REBOOTING` with `RecoveryExitReason.TTL_EXPIRED`. The watchdog is part of the L1 toolbox on `/`; it cannot be disabled by the operator without detection (the watchdog itself emits a heartbeat to the evidence log).

### §8.4 Auto-reboot

At `expires_at`, the watchdog emits `RECOVERY_TTL_EXPIRED_AUTO_REBOOT` evidence (FOREVER retention) and triggers `systemctl reboot`. The reboot brings the system back through GRUB; the operator either selects normal mode or recovery mode anew.

Operators close to the cap who genuinely need more time should: complete the current operation, reboot to normal, and start a fresh recovery session (which begins a new 8-hour window). Frequent recovery sessions are themselves a signal worth investigating; the cap surfaces that signal.

## §9 No-L5-in-recovery contract

### §9.1 Statement

L5 cognitive services are **not started** during recovery boot. The recovery boot path masks the L5 systemd units; the L5 binaries are physically present on `/` (because they are part of the AIOS image) but they are inert in recovery.

### §9.2 Concretely

The L5 services include the planner, the agent runtime, the model router, the cognitive memory daemon, and the model-serving runtime (Ollama / vLLM-compatible). Each has a systemd unit file. In normal mode these units are enabled and started at `STAGE_NORMAL_RUNTIME_READY`. In recovery mode, the kernel command line `aios.mode=RECOVERY` causes the init script to enumerate `/etc/systemd/system/aios-l5-*.service` and mask them via `systemctl mask` before any L5 unit can autostart.

Beyond masking, the recovery shell's exec filter (per `RecoveryDeniedClass.THIRD_PARTY_BINARY_RUN`) refuses to exec any path matching `/usr/lib/aios/l5/...` regardless of operator command. Even an operator who manually unmasks an L5 unit cannot exec the binary from the recovery shell.

### §9.3 What the operator gets instead

For diagnosis, the recovery operator has:

- The L1 toolbox (busybox-style core utilities + `verify-chain` + `policy-bundle-compile` + `aios-cli-recovery`).
- The L4 vault broker in degraded mode (key-management operations only; no normal-mode capability issuance).
- The L4 identity service in degraded mode (only `_system` scope subjects).
- The S3.1 evidence log in read-only verification mode.
- A read-only forensic attach to group data (§5.2) when explicitly requested.

What the operator does **not** get:

- Any LLM-mediated assistance.
- The Cognitive Core's planning UI.
- The agent runtime.

### §9.4 The contract

Per INV-001, an operator who needs a "smarter" recovery experience must boot back to normal mode and use the AIOS Cognitive Core there (in normal mode, planning is permitted because the constitutional layer is intact and the policy bundle is loaded). Recovery is intentionally manual; the price of L5-free recovery is operator literacy.

The constitutional reason: AI failures, model corruption, LLM provider outages, and prompt-injection vectors must not brick the recovery path. A recovery path that requires AI is not a recovery path; it is an application running on top of one.

### §9.5 Evidence

Any attempt to start an L5 service from recovery emits `RECOVERY_L5_START_BLOCKED` evidence (FOREVER retention) with the requesting subject canonical id, the target service name, and the reason code `L5StartProhibitedInRecovery`. The evidence is a constitutional violation marker; the operator who triggered it should be reviewed.

## §10 Exit-by-reboot only

### §10.1 Statement

There is **no in-place transition** from `RECOVERY` to `NORMAL`. The only exit from recovery is reboot. The exit reasons are enumerated in §3.4; the only normal exit is `REBOOT_TO_NORMAL`.

### §10.2 Why

Three constitutional reasons.

1. **Eliminates "did we really clean up state?" ambiguity.** A long-running recovery session accumulates mounts, opened devices, in-flight watchdogs, and degraded-mode service handles. Tearing them down in-place to "transition to normal" is the kind of operation that hides bugs. Rebooting forces a clean kernel and a deterministic startup; if anything we did in recovery was supposed to take effect (a new policy bundle, a new kernel slot, a new identity bundle), the next normal boot picks it up via the standard startup path. There is no "did we forget to reload that?" failure mode.
2. **Forces the kernel command line to change deterministically.** The kernel command line carries `aios.mode`. A reboot is the only place it is set. An in-place transition would require flipping a runtime flag, which would create the question "was this flag set by an authorised operation?" — a question whose answer requires evidence we would have to invent. By restricting mode change to the kernel command line, we make the question "what mode are we in?" answerable by inspecting the command line, not by querying running services.
3. **Prevents long-lived recovery sessions from drifting into normal-mode privileges.** A recovery operator who could "promote to normal" without rebooting would, in effect, hold normal-mode privileges with a recovery-mode pedigree. The normal-mode policy bundle, identity bundle, and capability catalog might not have been re-read since recovery began; a privileged session could outlive the constitutional checks. Reboot ensures the constitutional checks are run fresh, in the order defined by S9.2.

### §10.3 Operator workflow

The operator:

1. Performs the recovery operations they came for.
2. Verifies the evidence chain (`verify-chain`) shows the operations were recorded.
3. Issues `recovery-reboot` from the recovery shell, which:
   - Emits `RECOVERY_BOOT_EXITED` evidence (FOREVER) with `RecoveryExitReason.REBOOT_TO_NORMAL`.
   - Closes the operator's session.
   - Calls `systemctl reboot`.
4. The next boot is normal mode by default (GRUB entry 0 or 1); the new policy bundle, identity bundle, or kernel slot is picked up by the standard startup path.

If the operator forgets to issue `recovery-reboot`, the 8-hour TTL eventually triggers `RecoveryExitReason.TTL_EXPIRED` and the watchdog reboots the host. Either way, recovery exits via reboot; never in-place.

## §11 Dedicated kernel A/B fallback (brief)

The full dedicated-kernel pipeline lives in S9.3. This section documents only the recovery-relevant slice.

### §11.1 A/B slot model

The host has two kernel slots: a dedicated kernel slot (`/boot/vmlinuz-aios`, the kernel built and signed by AIOS for this hardware profile) and a generic fallback kernel slot (`/boot/vmlinuz-generic`, a stock kernel that always works on the hardware class). GRUB maintains a counter of consecutive boot failures per slot.

### §11.2 Fallback rules

If the dedicated kernel slot fails to reach `STAGE_NORMAL_RUNTIME_READY` (or its recovery-mode equivalent `STAGE_RECOVERY_ACTIVE`) **N=2** times consecutively, GRUB falls back to the generic kernel slot automatically. The fallback boot uses `aios.fallback=1` on the kernel command line, which:

- Disables any optional dedicated-kernel-only feature flags.
- Records `BOOT_FALLBACK_TRIGGERED` evidence (queued for S3.1; see §16).

If both kernel slots fail (dedicated 2 times, generic 2 times), GRUB falls back to the recovery boot path with `RecoveryEntryReason.BOOT_FAILURE_AUTO`. The recovery operator can then promote a different dedicated-kernel candidate (`RecoveryMutableScope.DEDICATED_KERNEL_PROMOTION`) or repair the generic slot.

### §11.3 Promotion to dedicated slot

A recovery operation under `RecoveryMutableScope.DEDICATED_KERNEL_PROMOTION` writes a new dedicated-kernel image to `/aios/system/kernel/candidates/` and updates the GRUB configuration to try it next normal boot. The full pipeline (hardware map, trust check, hardening, sandbox build) is in S9.3. From this spec's perspective, the only requirement is that promotion is a recovery-only mutation, evidenced as `RECOVERY_OPERATION_PERFORMED` with `scope = DEDICATED_KERNEL_PROMOTION`.

## §12 Evidence record types

The following ten record types are queued for S3.1 vocabulary, all FOREVER retention. They are the audit witness for everything in this spec.

| Record type                            | Retention | Payload (key fields)                                                                                            |
| -------------------------------------- | --------- | --------------------------------------------------------------------------------------------------------------- |
| `RECOVERY_BOOT_ENTERED`                | FOREVER   | `entry_reason: RecoveryEntryReason`, `kernel_slot: {dedicated, generic}`, `fallback: bool`, `bundle_version`    |
| `RECOVERY_OPERATOR_AUTHENTICATED`      | FOREVER   | `subject_canonical_id`, `auth_factor: {HARDWARE_KEY, TOTP, PASSPHRASE}`, `risk_flags`                           |
| `RECOVERY_OPERATION_PERFORMED`         | FOREVER   | `subject_canonical_id`, `mutation_scope: RecoveryMutableScope`, `target_path`, `request_hash`, `bundle_version` |
| `RECOVERY_TTL_EXPIRED_AUTO_REBOOT`     | FOREVER   | `subject_canonical_id`, `authenticated_at`, `expired_at`                                                        |
| `RECOVERY_BOOT_EXITED`                 | FOREVER   | `exit_reason: RecoveryExitReason`, `subject_canonical_id`, `session_duration_seconds`                           |
| `RECOVERY_L5_START_BLOCKED`            | FOREVER   | `subject_canonical_id`, `attempted_service`, `reason_code: L5StartProhibitedInRecovery`                         |
| `RECOVERY_NETWORK_LAN_ENABLED`         | FOREVER   | `subject_canonical_id`, `posture: LAN_FOR_PROVISIONING`, `window_seconds: ≤ 1800`, `justification_text`         |
| `RECOVERY_NETWORK_LAN_DISABLED`        | FOREVER   | `subject_canonical_id`, `closed_at`, `closed_by: {OPERATOR, WATCHDOG}`                                          |
| `RECOVERY_FORENSIC_ATTACH_PERFORMED`   | FOREVER   | `subject_canonical_id`, `attached_group_id`, `mount_point`, `justification_text`, `released_at`                 |
| `BOOT_FAILURE_AUTO_RECOVERY_TRIGGERED` | FOREVER   | `consecutive_failure_count`, `last_failed_stage: RecoveryStage` (or normal-mode equivalent), `triggered_at`     |

These record types are constitutional evidence: they are FOREVER-retained, never compactable (S3.1 retention class FOREVER is exempt from compaction), and replicated across all S3.1 segments. Their existence makes recovery activity visible in the operational record indefinitely.

Reason codes queued for S3.1 reason-code vocabulary alongside the records:

- `L5StartProhibitedInRecovery`
- `WebPublicExposureProhibitedInRecovery`
- `RecoveryNetworkOpenAttempt`
- `ThirdPartyBinaryExecutionProhibitedInRecovery`
- `EvidenceLogRewriteProhibited`

## §13 Cross-references

| Spec                                               | Direction   | What this spec contributes                                                                                                                                                                                                                                       |
| -------------------------------------------------- | ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| L0 INV-001                                         | implementer | This spec is the concrete implementation of "recovery is independent of L5". §9 is the contract that makes the invariant verifiable.                                                                                                                             |
| L0 INV-004                                         | implementer | This spec is the concrete implementation of "recovery boundary preserved". §3.1 (the three constitutional roots), §5 (mount discipline) are the verifiable artefacts.                                                                                            |
| L0 INV-012                                         | implementer | This spec implements "recovery required for system mutation" via the `RecoveryMutableScope` (§3.6) and the mount discipline (§5). The S2.3 hard-deny consumes the same scopes.                                                                                   |
| L0 INV-022                                         | implementer | This spec consumes "recovery aesthetically distinct" by requiring the recovery shell to be an `AIOS_SURFACE`-only L7.1 surface stack (§6.1). The visual language is owned by L7.X.                                                                               |
| S2.3 Policy Kernel                                 | constraint  | The hard-deny `RecoveryRequiredForSystemMutation` consumes `RecoveryMutableScope` (§3.6) as the closed list of scopes that require `recovery_mode = true`. The reason codes in §12 are added to S2.3's reason-code vocabulary.                                   |
| S5.1 Identity Model                                | consumer    | This spec consumes `SessionClass.RECOVERY` (§7.1), the `_system` scope subjects (§6.4), and the 8-hour cap on recovery sessions (§8). It does not redefine identity.                                                                                             |
| S5.2 Vault Broker (deferred)                       | constraint  | This spec requires the vault broker in recovery to be in degraded mode (key-management only; no raw secret reveal except via the human-reveal-only path which itself requires recovery). The detailed vault behaviour lives in S5.2.                             |
| S5.3 Approval Mechanics (deferred)                 | constraint  | Recovery operations that require approval (e.g. invariant retirement) consume S5.3 mechanics, with the addition that the approver in recovery is always a HUMAN_USER (or a co-operator at a `_system:remote:operator-<id>` if `LAN_FOR_PROVISIONING` is active). |
| S5.4 Emergency Override                            | consumer    | `OverrideStrength.STRONG_SOLO` (S5.4 §3.2) is gated to recovery boot only by this spec. The L1 boundary cited in S5.4 §3.2 is this contract's §3 + §6.                                                                                                           |
| S4.1 Namespace Layout                              | consumer    | This spec consumes `/aios/system/...` paths (§3.6) and the recovery boundary invariant I3 (§4 of S4.1). It does not redefine the namespace.                                                                                                                      |
| S3.1 Evidence Log                                  | producer    | Ten new record types FOREVER retention (§12) and five new reason codes queued for S3.1 vocabulary.                                                                                                                                                               |
| L7.1 Surface + Composition Model                   | consumer    | This spec consumes the recovery surface stack restriction in L7.1 §6.3 (only `AIOS_SURFACE` permitted in recovery) for the recovery shell.                                                                                                                       |
| L7.5 Web renderer                                  | consumer    | The Web renderer is **not started** in recovery. The `RECOVERY` exposure state in the Web renderer's spec is consumed by this spec as "the renderer that does not run in this mode".                                                                             |
| S9.2 First-boot installer (deferred sub-spec)      | constraint  | The first-boot installer registers the initial recovery operator credential (per §6.2) and writes the initial GRUB configuration. The detailed installer flow lives in S9.2.                                                                                     |
| S9.3 Dedicated kernel pipeline (deferred sub-spec) | constraint  | This spec requires the A/B slot model (§11) and the auto-fallback to recovery on dual-failure. The full kernel build, hardening, and promotion pipeline lives in S9.3.                                                                                           |

## §14 Worked examples

### §14.1 Example 1 — Normal boot

```text
Operator powers on the host.
GRUB selects entry 0 (AIOS Normal, dedicated kernel).
Kernel boots; aios.mode=NORMAL parsed.
Initramfs honours NORMAL flag; mounts / rw, /root rw.
/aios is mounted via AIOS-FS.
L0 governance ready (invbundle signature OK).
L4 identity service ready (idbundle signature OK; full subject set available).
L4 vault broker ready (master key unsealed via TPM).
L5 cognitive services start (planner, agent runtime, model router).
Renderers start (KDE Plasma surface stack with all SurfaceKinds permitted).
STAGE_NORMAL_RUNTIME_READY.
Operator works in NORMAL.

No RECOVERY_* evidence is emitted. The boot is recorded in the standard
operational record (which lives in S9.2).
```

### §14.2 Example 2 — Operator-initiated recovery for a policy bundle update

```text
Operator powers on the host.
At GRUB, operator selects entry 2 (AIOS Recovery).
Kernel boots; aios.mode=RECOVERY, aios.recovery_reason=OPERATOR_INITIATED parsed.
STAGE walk:
  STAGE_PRE_KERNEL → STAGE_KERNEL_LOADED → STAGE_INITRAMFS → STAGE_ROOT_MOUNTED.
  /aios is deliberately NOT mounted. /root is available.
  STAGE_L0_GOVERNANCE_READY (invbundle signature OK).
  STAGE_L4_DEGRADED_READY (only _system subjects available; vault in degraded mode).
  STAGE_RECOVERY_SHELL_READY (AIOS_SURFACE-only KWin; recovery aesthetic per INV-022).
Evidence: RECOVERY_BOOT_ENTERED FOREVER {
  entry_reason: OPERATOR_INITIATED,
  kernel_slot: dedicated,
  fallback: false,
  bundle_version: invbundle_<hex>
}
Operator authenticates with hardware key.
Subject: _system:local:operator-247 (HUMAN_USER, recovery_mode = true).
Session: SessionClass.RECOVERY, expires_at = authenticated_at + 8h.
STAGE_RECOVERY_ACTIVE.
Evidence: RECOVERY_OPERATOR_AUTHENTICATED FOREVER {
  subject_canonical_id: _system:local:operator-247,
  auth_factor: HARDWARE_KEY,
  risk_flags: []
}
Operator submits a typed action targeting /aios/system/policy/.
Action: load policy_bundle_<new_hash>.
Policy Kernel: subject is recovery-mode; mutation_scope = POLICY_BUNDLE; ALLOW.
The kernel performs the mutation through the AIOS-FS API in degraded mode.
Evidence: RECOVERY_OPERATION_PERFORMED FOREVER {
  subject_canonical_id: _system:local:operator-247,
  mutation_scope: POLICY_BUNDLE,
  target_path: /aios/system/policy/active,
  request_hash: <hex>,
  bundle_version: <new_hash>
}
Operator verifies: `verify-chain` returns OK.
Operator issues `recovery-reboot`.
Evidence: RECOVERY_BOOT_EXITED FOREVER {
  exit_reason: REBOOT_TO_NORMAL,
  subject_canonical_id: _system:local:operator-247,
  session_duration_seconds: 1080
}
systemctl reboot.
Next boot: GRUB entry 0; normal mode; new policy bundle is picked up at
STAGE_NORMAL_RUNTIME_READY's policy-load step.
```

### §14.3 Example 3 — Boot failure auto-recovery with forensic attach

```text
Three consecutive normal boots fail to reach STAGE_NORMAL_RUNTIME_READY:
  Boot 1: AIOS-FS mount of /aios timed out (network filesystem hiccup).
  Boot 2: same.
  Boot 3: same.
GRUB state: consecutive_normal_boot_failures = 3.
GRUB selects recovery boot path: entry 2.
Kernel boots; aios.mode=RECOVERY, aios.recovery_reason=BOOT_FAILURE_AUTO.
Evidence: BOOT_FAILURE_AUTO_RECOVERY_TRIGGERED FOREVER {
  consecutive_failure_count: 3,
  last_failed_stage: STAGE_AIOSFS_MOUNT (a normal-mode stage from S9.2),
  triggered_at: <ts>
}
STAGE walk completes through STAGE_RECOVERY_SHELL_READY.
Evidence: RECOVERY_BOOT_ENTERED FOREVER { entry_reason: BOOT_FAILURE_AUTO, ... }.
Operator authenticates with hardware key.
Evidence: RECOVERY_OPERATOR_AUTHENTICATED FOREVER { ... }.
STAGE_RECOVERY_ACTIVE.
Operator suspects /aios/groups/family contains a malformed object that triggers
the AIOS-FS mount failure. Operator submits forensic_attach_request:
  attached_group_id: family
  justification_text: "Diagnose AIOS-FS mount failure on boot."
Kernel attaches /aios/groups/family read-only at /recovery/inspect/family.
Evidence: RECOVERY_FORENSIC_ATTACH_PERFORMED FOREVER {
  subject_canonical_id: _system:local:operator-247,
  attached_group_id: family,
  mount_point: /recovery/inspect/family,
  justification_text: "Diagnose AIOS-FS mount failure on boot.",
  released_at: <ts + 30m>
}
Operator runs `verify-chain` against /recovery/inspect/family/evidence/. Tamper
is not the issue; chain is OK.
Operator inspects /recovery/inspect/family/datasets/ — finds a corrupted object.
Operator submits a typed action under POLICY_BUNDLE to add a one-time exclusion
rule for the corrupted object's path so the next normal boot does not retry it.
Evidence: RECOVERY_OPERATION_PERFORMED FOREVER {
  mutation_scope: POLICY_BUNDLE,
  target_path: /aios/system/policy/active,
  request_hash: <hex>,
  ...
}
Operator issues `recovery-reboot`.
Evidence: RECOVERY_BOOT_EXITED FOREVER { exit_reason: REBOOT_TO_NORMAL, ... }.
Next normal boot proceeds to STAGE_NORMAL_RUNTIME_READY; AIOS-FS mounts cleanly
because the corrupted object is excluded; GRUB resets
consecutive_normal_boot_failures to 0 on success.
```

## §15 Adversarial robustness

### §15.1 Attempted in-place mode flip

An attacker (or buggy code) attempts to flip `Session.recovery_mode` from `false` to `true` in a running normal-mode session. Per S5.1 §7.2 (recovery flag is not loosenable) and this spec's §3.2 (`DEGRADED → RECOVERY` is not an in-place transition), the identity service rejects any RPC that mutates the recovery flag of an existing session. The flag is set at session creation only, by the identity service signing the session record with `Session.recovery_mode = true` only when the session class is `RECOVERY` (which itself is reachable only via recovery boot). Any tampered session record fails Ed25519 verification at the Capability Runtime; the action is denied with `InvalidSubjectSignature`.

### §15.2 Attempted unmask of L5 in recovery

An operator (or compromised recovery operator credential) attempts to `systemctl unmask aios-l5-planner.service` in the recovery shell. The unmask succeeds at the systemd level, but the recovery shell's exec filter rejects any subsequent `start`. Even if the operator manages to fork a process pointing at `/usr/lib/aios/l5/planner`, the kernel's exec filter (per `RecoveryDeniedClass.THIRD_PARTY_BINARY_RUN` extended to L5 paths) rejects the exec; `RECOVERY_L5_START_BLOCKED` is emitted. The operator credential should be reviewed.

### §15.3 Attempted Web exposure during recovery

A compromised recovery operator opens a port on `0.0.0.0` to expose a recovery-mode UI. The L8 network policy in recovery hard-denies non-loopback bind by default (`RecoveryNetworkPosture.LOOPBACK_ONLY`). The bind syscall returns `EACCES`; the L8 telemetry records the attempt. If the operator first switches posture to `LAN_FOR_PROVISIONING`, the switch itself emits `RECOVERY_NETWORK_LAN_ENABLED`; subsequent binds are visible to the operational record indefinitely. The Web renderer itself is not started in recovery (no L5, no agent runtime), so the operator must run an arbitrary HTTP server — which is rejected by `RecoveryDeniedClass.THIRD_PARTY_BINARY_RUN`.

### §15.4 Attempted policy bundle rule that loosens INV-012

An operator (or compromised credential) submits a policy bundle whose rules would let a normal-mode subject mutate `/aios/system/policy/`. The S2.3 bundle compiler rejects the bundle at compile time with `InvariantLooseningAttempted` (per L0 §5.3). The bundle is **not** loaded; `INVARIANT_LOOSENING_REJECTED` evidence is emitted (FOREVER, owned by L0). The compromised credential is visible in the rejected-bundle's submitter field.

### §15.5 Attempted evidence rewrite

An operator attempts to truncate `/aios/system/evidence/segment-N` from the recovery shell. The mount is read-only verification per §3.7; the truncate syscall returns `EROFS`. Even if the operator gains read-write on the underlying device through external means, the next `verify-chain` detects the missing tail and emits `TAMPER_DETECTED` (per S3.1 / INV-005). The attacker has not "won"; they have made the tamper visible. INV-005 (evidence append-only) holds.

### §15.6 Attempted TTL extension

A compromised recovery operator submits a policy bundle whose rules would extend `Session.expires_at` for recovery sessions. The bundle compiler rejects the rule because it loosens the constitutional 8-hour cap (§8.3). The watchdog continues to honour the original `expires_at`; auto-reboot fires at the original time.

### §15.7 Attempted in-place exit to normal mode

An operator submits a typed action requesting "exit to normal without reboot". The action is hard-denied at S2.3; there is no policy bundle rule that maps to such an action. Even if such an action could be expressed, the kernel command line carries `aios.mode=RECOVERY`; the kernel does not interpret a userspace request to change its command line. The only path is reboot.

## §16 Open deferrals

The following are deferred to other sub-specs or future revisions. They are listed here so a reader who finishes this spec knows where to look (or where the gap is) for the surrounding context.

- **Dedicated kernel pipeline (S9.3).** The full hardware-map, trust-check, host-config, hardening, sandbox-build, and A/B-promotion flow. This spec uses the slot model abstractly (§11).
- **First-boot installer (S9.2).** The flow that lays down `/`, populates `/root`, initialises `/aios`, registers the initial recovery operator credential, and writes the initial GRUB configuration. The first-boot flow is referenced from §6.2 and §11 abstractly.
- **Hardware attestation in recovery.** Whether and how a recovery boot attests to a remote verifier (e.g. a household admin's phone) before being granted access to the recovery shell. Deferred. In Rev.2, recovery authentication is local (§6.3) so attestation is not on the critical path.
- **Multi-host coordinated recovery.** When AIOS becomes multi-host, recovery on one host may need to coordinate with peers (e.g. quorum-based bundle update). Deferred. In Rev.2 every host recovers independently.
- **`HEAVY_AUTH_FALLBACK_USED` evidence record.** A passphrase fallback for recovery authentication should emit an evidence record so over-reliance can be detected. Queued for S3.1; not added in this spec because it lives in the L4 authentication evidence vocabulary, not in the recovery vocabulary.
- **`RECOVERY_SHELL_FAILED` evidence record.** A failure at `STAGE_RECOVERY_SHELL_READY` should emit a record so the auto-reboot is visible. Queued for S3.1 abnormal-exit branch; not added here because the recovery shell's failure modes are owned by the L1 toolbox spec (deferred).
- **`BOOT_FALLBACK_TRIGGERED` evidence record.** A normal-mode boot that fell back to the generic kernel slot (per §11.2) should emit a record. Queued for S3.1 alongside the S9.3 kernel pipeline vocabulary.
- **Per-recovery-operation dry-run.** The recovery shell could offer a "dry-run" mode that simulates a typed action against the recovery-mutable scopes without committing. Useful for validating a policy bundle before promotion. Deferred. The current contract is "submit, audit, reboot".

## §17 Acceptance criteria

- [ ] `RecoveryMode` is a closed enum with exactly four values (`NORMAL`, `RECOVERY`, `DEGRADED`, `FIRST_BOOT`); the `is_first_boot` and `is_recovery_mode` subject flags are mutually exclusive.
- [ ] `FIRST_BOOT` mode write permissions (§3.2.1) enumerate the canonical first-boot service subjects per `RecoveryMutableScope` value; AI subjects receive no first-boot write privileges; `RecoveryDeniedClass` and `RecoveryReadOnlyScope` are not loosened by `FIRST_BOOT` mode.
- [ ] Each `FIRST_BOOT` mode mutation emits a FOREVER `FIRST_BOOT_OPERATION` record carrying `subject_canonical_id`, `target_path`, `target_scope`, `stage`, `firstboot_session_id`, `committed_at` (queued for S3.1 vocabulary alongside the W10 update).
- [ ] `RecoveryEntryReason` is a closed enum with exactly eight values, covering operator-initiated, boot-failure-auto, and the six L0/L4/L2 trust-failure reasons.
- [ ] `RecoveryExitReason` is a closed enum with exactly four values; the only normal exit is `REBOOT_TO_NORMAL`.
- [ ] `RecoveryStage` is a closed enum with exactly nine values, forming a linear FSM with forward-only transitions.
- [ ] `RecoveryMutableScope` is a closed enum with exactly ten values (`POLICY_BUNDLE`, `CAPABILITY_CATALOG`, `VAULT_ROOT_MATERIAL`, `INVARIANT_BUNDLE`, `IDENTITY_BUNDLE`, `RECOVERY_OPERATOR_REGISTRATION`, `DEDICATED_KERNEL_PROMOTION`, `L1_BOOT_PARAMETERS`, `SYS_FIRSTBOOT_RESET`, `FIRMWARE_VERSION_COUNTER`); each maps to a `/aios/system/...` path. The last two are recovery-only and explicitly **not** writable under `FIRST_BOOT` mode (§3.2.1).
- [ ] `RecoveryReadOnlyScope` is a closed enum with exactly two values; group data is not in this enum (it is read-only via the forensic attach exception only).
- [ ] `RecoveryDeniedClass` is a closed enum with exactly five values; each maps to a hard-deny reason code.
- [ ] `RecoveryNetworkPosture` is a closed enum with exactly three values; the default is `LOOPBACK_ONLY`.
- [ ] The mount table (§5.1) is the verifiable artefact of INV-004; every row is a mechanical assertion the L1 substrate honours.
- [ ] The operator-authentication contract (§6) requires a `_system` scope subject and `SessionClass.RECOVERY`.
- [ ] The 8-hour hard cap (§8) is enforced at the kernel level via the watchdog; no policy bundle loosens it.
- [ ] The "no L5 in recovery" contract (§9) is enforced by systemd unit masking at recovery boot AND by the recovery shell's exec filter.
- [ ] Exit-by-reboot is the only exit (§10); there is no in-place transition from `RECOVERY` to `NORMAL`.
- [ ] The A/B kernel fallback (§11) falls back to the generic slot after `N=2` consecutive failures and to the recovery boot path after both slots fail.
- [ ] The ten FOREVER-retained evidence record types in §12 are queued for S3.1 vocabulary.
- [ ] The five reason codes in §12 are queued for S3.1 reason-code vocabulary.
- [ ] The three worked examples (§14) trace through the FSM and produce the expected evidence record sequences.
- [ ] All seven adversarial scenarios in §15 fail closed.
- [ ] Cross-references in §13 are accurate against the cited sub-specs as of 2026-05-09.

## §18 Status & evidence grade

Status: REAL
Evidence: E1 — file exists; structural contract complete; closed enums declared; mount discipline tabulated; FSM defined; worked examples present; cross-references resolved against existing spec files; adversarial scenarios enumerated; acceptance criteria are mechanically checkable against this file.

Promotion to E2 requires: a build/typecheck artefact validating that the closed enums (`RecoveryMode`, `RecoveryEntryReason`, `RecoveryExitReason`, `RecoveryStage`, `RecoveryMutableScope`, `RecoveryReadOnlyScope`, `RecoveryDeniedClass`, `RecoveryNetworkPosture`) compile cleanly in the `aios.recovery.v1alpha1` schema package with the rest of the rev.2 schema bundle.

Promotion to E3 requires: unit-level tests of the mount discipline (§5), the operator-authentication contract (§6), the 8-hour watchdog (§8), the L5 mask + exec-filter (§9), and the seven adversarial scenarios (§15). Each test produces a recoverable artefact.

Promotion to E4 requires: an end-to-end recovery rehearsal exercising the three worked examples (§14) on a real AIOS host (or a faithful VM with the same boot path), with the FOREVER-retained evidence record sequences captured and replayed against `verify-chain`.

Promotion to E5 requires: an operational recovery on a production AIOS host that uses every `RecoveryMutableScope` at least once, with the resulting evidence visible in the post-recovery operational record and the next normal boot picking up the mutations cleanly.

Hash convention used in this spec: `hex_lower(BLAKE3(...))[:32]`.
