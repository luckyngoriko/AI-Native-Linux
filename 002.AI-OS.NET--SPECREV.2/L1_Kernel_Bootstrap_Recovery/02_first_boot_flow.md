# First-Boot Flow (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists; structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| Phase tag      | S9.2                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Layer          | L1 Kernel, Bootstrap, Recovery                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Schema package | `aios.firstboot.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| Consumes       | **Imports vocabulary from**: S5.1 (`SubjectKind`, `_system` scope subjects, `SessionClass` — closed enums; type-level), S4.1 (`/aios/system/...` namespace path enum — type-level), S3.1 (`RecordType` vocabulary + retention classes + append-only chain shape — type-level evidence-log schema), S5.2 (master-key seal API surface — type-level capability schema co-defined with L4 vault). **Requires for correctness (degraded subset only)**: S9.1 Recovery Boundary (`RecoveryStage` FSM, `RecoveryEntryReason`, `RecoveryMutableScope`, the three constitutional roots — peer L1 dependency), S9.3 Dedicated Kernel Pipeline (A/B kernel slot semantics — peer L1; first-boot uses generic kernel exclusively, so S9.3 build-pipeline is _not_ required at first-boot time, only its slot-semantics vocabulary), S2.3 Policy Kernel (initial bundle compile + load — first-boot must establish the policy-kernel degraded subset before terminal commit; runtime-required as a degraded subset, like S9.1). **Peer (intra-L0)**: INV-001, INV-004, INV-005, INV-006, INV-008, INV-012, INV-013, INV-014, INV-018, INV-022; L0 §3 governance bundle. |
| Produces       | closed `FirstBootStage` linear FSM (15 stages), closed `FirstBootEntryReason` (3), closed `AIProviderMode` (4), closed `RecoveryCredentialKind` (3), closed `FirstBootFailureReason` (12), closed `InitialFirewallPosture` (3); the per-stage idempotency contract; the AI provider configuration discipline; the initial firewall posture discipline; the first-group + first-user requirement; eleven evidence record types queued for S3.1 (FOREVER for the constitutional ones); three worked examples (TPM laptop, no-TPM server, mid-stage power loss); the adversarial-robustness section binding installer-media tampering, skip-recovery-operator, malicious-AI-provider, concurrent installers, and AI-initiated reset-to-factory                                                                                                                                                                                                                                                                                                                                                                                                                 |

## §1 Purpose

Every operator-side action path in AIOS — installing the OS on a laptop, reimaging a server, resetting a household to factory state — begins with first-boot. Until first-boot succeeds, the host has no policy bundle, no identity bundle, no vault root key, no recovery operator credential, and no group or user subjects to act on its behalf. S9.1 (recovery boundary) and S9.3 (dedicated kernel pipeline) describe the steady state: how the host re-enters operator-driven recovery and how the dedicated kernel is built, validated, and promoted. Neither spec answers the prior question: how does an unconfigured AIOS host reach a usable state for the first time, in a way that is mechanically auditable, idempotent under interruption, and impossible to silently weaken?

This spec answers that question. It defines the **first-boot flow** as a closed, linear, idempotent FSM that runs once per fresh install (or once per reset-to-factory). The flow turns a freshly-imaged host into a constitutional AIOS host whose subsequent boots are governed by S9.1 (normal mode by default; recovery mode on demand) and whose dedicated kernel candidates are managed by S9.3.

The flow is constitutional in the following senses. First, it is **bounded**: a closed enum of stages, a closed enum of entry reasons, a closed enum of failure reasons. There is no `OTHER` value at any point; ambiguity is treated as a contract violation. Second, it is **idempotent at every stage except the terminal commit**: the host can be powered off mid-way through any stage and resume from that stage on next boot, without re-running prior work. Only the transition to `STAGE_FIRST_BOOT_COMPLETE` is non-idempotent — once first-boot is committed, the only path back to a fresh state is a recovery-mode reset-to-factory operation, itself FOREVER-evidenced. Third, it is **AI-free at the constitutional level**: no L5 cognition runs during first-boot. The operator interacts with a typed installer surface that is part of the L1 toolbox, not with an LLM-mediated chrome. Fourth, it is **evidence-producing**: every stage transition emits a record; the constitutional commits emit FOREVER-retention records; the resulting evidence chain is the audit witness that the host's constitutional layer was bootstrapped through this exact flow and no other.

The flow is the **action-path origin** for every AIOS operator scenario. A user story that says "the operator installs AIOS on a new laptop" resolves to the FSM in §3.5; a user story that says "the household administrator resets the home server to factory state" resolves to the same FSM with `FirstBootEntryReason.RESET_TO_FACTORY`; a user story that says "we re-image a kiosk overnight" resolves with `FirstBootEntryReason.REIMAGE`. Without this spec, every such scenario floats in a contractual vacuum.

## §2 Scope

This spec **defines**:

1. The closed `FirstBootEntryReason` enum (§3.1) — the three reasons first-boot may run.
2. The closed `FirstBootStage` enum (§3.2) — the linear FSM.
3. The closed `AIProviderMode` enum (§3.3) — the four AI provider configurations.
4. The closed `RecoveryCredentialKind` enum (§3.4) — the three accepted recovery operator credential kinds.
5. The closed `FirstBootFailureReason` enum (§3.5) — the twelve terminal-failure reasons.
6. The closed `InitialFirewallPosture` enum (§3.6) — the three first-boot firewall postures.
7. The per-stage semantics, idempotency contract, and acceptance signals (§4–§6).
8. The AI provider configuration discipline (§7).
9. The initial firewall posture discipline (§8).
10. The first-group + first-user discipline (§9).
11. The recovery operator registration discipline (§10).
12. The terminal-commit contract for `STAGE_FIRST_BOOT_COMPLETE` and the reset-to-factory recovery operation (§11).
13. The eleven evidence record types queued for S3.1 (§12).
14. Three worked examples — TPM-equipped laptop, no-TPM server, mid-stage power-loss recovery (§13).
15. The adversarial-robustness section (§14) binding installer-media tampering, skipped recovery operator registration, malicious AI provider, concurrent installer attempts, and an AI-subject attempt at reset-to-factory.

This spec **does not** define:

- The recovery boot path or operator authentication during recovery — owned by S9.1.
- The dedicated kernel build, validation, and A/B promotion pipeline — owned by S9.3. First-boot uses the generic kernel exclusively; S9.3 is engaged only post-first-boot to build the host's dedicated kernel candidate.
- The full vault broker contract — owned by S5.2. This spec uses vault root key generation (§3.2.5) abstractly, citing the vault broker's master-key seal mechanism.
- The full identity model — owned by S5.1. This spec uses subject creation and group registration abstractly.
- The full Policy Kernel contract — owned by S2.3. This spec uses bundle compile and load abstractly.
- The hardware graph (HDM) — owned by L8 HDM (deferred). First-boot does not require a fully populated hardware graph; the generic kernel covers all supported hardware.
- The renderer surface stack used by the first-boot installer UI — owned by L7.1. The installer is an `AIOS_SURFACE`-only stack, the same constraint S9.1 places on the recovery shell, citing INV-022.
- The disk encryption design (LUKS / dm-crypt / ZFS native) — owned by L1 substrate spec (deferred). First-boot honours the operator's choice but the underlying mechanics live elsewhere.

This spec is the **contract surface** that S9.1 references when it cites "the first-boot installer (S9.2)" and that S9.3 references when it says "first-boot is out of scope for the dedicated kernel pipeline".

## §3 Vocabulary (closed enums)

All vocabularies in this section are closed. Adding a value is a versioned spec change. Bundle load fails on unknown values. None of these enums admits an `OPEN` or `OTHER` value; the intent is to make first-boot semantics fully mechanical.

### §3.1 `FirstBootEntryReason`

The closed list of reasons first-boot may run. Every first-boot session carries exactly one of these reasons; the reason is recorded in the `FIRST_BOOT_STARTED` evidence record (§12) at FOREVER retention.

| Value              | Meaning                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `FRESH_INSTALL`    | A previously unprovisioned host (no `/aios/system/firstboot/marker`) is being installed for the first time. The default and most common reason. The installer is the L1 toolbox installer; the source media is signed AIOS installation media.                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `RESET_TO_FACTORY` | A previously provisioned host is being returned to a fresh state after a recovery-mode `RESET_TO_FACTORY_INITIATED` operation. The recovery operation wipes `/aios` (per S9.1 `RecoveryMutableScope.IDENTITY_BUNDLE` + `POLICY_BUNDLE` + `VAULT_ROOT_MATERIAL` cascade with explicit operator co-signer), retains the recovery operator credential set as an option for the new install, and reboots into first-boot. The reset operation itself emits `RESET_TO_FACTORY_INITIATED` (FOREVER) before the wipe, and the resulting first-boot emits `FIRST_BOOT_STARTED` with reason `RESET_TO_FACTORY`. The two records together form an audit trail across the constitutional rebuild. |
| `REIMAGE`          | A host is being installed on top of a previous AIOS install whose `/aios` was already wiped externally (e.g., by an out-of-band imaging tool, by physically replacing the disk, or by booting a separate installer media that overwrote the partition table). Behaves identically to `FRESH_INSTALL` but distinguishes the operator workflow in evidence: the audit trail shows that this host's previous AIOS state was destroyed without a recovery-mode reset. Used to detect "ghost" reinstalls that bypassed the recovery path.                                                                                                                                                   |

These three values exhaust the first-boot entry reasons. There is no `UPGRADE_IN_PLACE` value — AIOS upgrades are not first-boot events; they are post-first-boot operations performed under recovery mode for constitutional updates and under normal mode (with policy approval) for non-constitutional updates.

### §3.2 `FirstBootStage`

The linear FSM during first-boot. Stages flow forward only; back-transitions are forbidden. Stage advancement is reported in evidence (§12). At any stage, an interrupted host is **idempotent** on next boot (§5).

| Stage                                  | What it represents                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `STAGE_INSTALLER_MEDIA_VERIFIED`       | The installation media's signature has been verified against the AIOS root public key embedded in the installer initramfs. No disk write has occurred. If signature fails, first-boot terminates with `MEDIA_SIGNATURE_INVALID` before any disk-write side effect.                                                                                                                                                                                                  |
| `STAGE_DISK_PARTITIONED`               | The target disk has been partitioned with the constitutional layout: `/` (Linux substrate, generic kernel + initramfs + L1 toolbox), `/aios` (AIOS-FS root partition), and `/recovery` (recovery toolbox + recovery initramfs). Partition table is GPT; entries are signed.                                                                                                                                                                                         |
| `STAGE_KERNEL_INSTALLED`               | The generic kernel image (`/boot/vmlinuz-generic`) and its initramfs have been installed to `/`. **No dedicated kernel exists yet.** The generic kernel is signed by AIOS root and is the kernel that runs first-boot, normal mode, and recovery mode until S9.3 promotes a dedicated kernel candidate.                                                                                                                                                             |
| `STAGE_AIOS_FS_INITIALIZED`            | The `/aios` AIOS-FS root object has been created. The namespace catalog (per S4.1) is seeded with the constitutional system paths (`/aios/system/policy/`, `/aios/system/identity/`, `/aios/system/vault/`, `/aios/system/recovery/`, `/aios/system/governance/`, `/aios/system/kernel/`, `/aios/system/boot/`, `/aios/system/firstboot/`). The root object's initial version is content-addressed (BLAKE3) and recorded.                                           |
| `STAGE_VAULT_ROOT_GENERATED`           | An Ed25519 keypair has been generated as the vault root key. The private half is sealed: to TPM 2.0 if the host carries a healthy TPM (PCR 0,2,4,7 quoted at seal time); to a hardware-key-protected envelope (PIV / FIDO2 large blob) on TPM-less hosts; the operator's explicit choice is recorded. The public half is written to `/aios/system/vault/root.pub`.                                                                                                  |
| `STAGE_INVARIANT_BUNDLE_LOADED`        | The initial L0 invariant bundle (`invbundle_<hex>`) — INV-001 through INV-024, signed by AIOS root — has been written to `/aios/system/governance/invariants/active` and verified at load. Failure to verify here is a fail-closed terminal failure.                                                                                                                                                                                                                |
| `STAGE_POLICY_BUNDLE_LOADED`           | The initial Policy Kernel bundle (signed by AIOS root, default-deny per INV-008) has been compiled and loaded. The bundle is the constitutional minimum: hard-denies for INV-008, INV-012, INV-013, INV-018, the recovery-required-for-system-mutation rule, and the AI-system-admin-blocked rule. No operator-specific policies yet.                                                                                                                               |
| `STAGE_IDENTITY_BUNDLE_LOADED`         | The initial identity bundle (`idbundle_<hex>`) is written. It contains exactly the `_system` scope service subjects required for first-boot to proceed: `_system:service:installer`, `_system:service:vault-init`, `_system:service:identity-init`, `_system:service:policy-compiler`, `_system:service:firstboot-coordinator`. No HUMAN_USER subjects yet — those are created in `STAGE_FIRST_USER_REGISTRATION`.                                                  |
| `STAGE_RECOVERY_OPERATOR_REGISTRATION` | The operator registers their recovery credentials. Accepted kinds: `HARDWARE_KEY`, `PASSPHRASE`, or `BOTH` (per `RecoveryCredentialKind`, §3.4). At least one credential MUST be registered before first-boot can advance. The credential is written to `/aios/system/recovery/operators/operator-1` and signed by the vault root key.                                                                                                                              |
| `STAGE_AI_PROVIDER_CONFIGURATION`      | The operator chooses an AI provider mode (per `AIProviderMode`, §3.3): `LOCAL_ONLY`, `VAULT_BROKERED_EXTERNAL`, `HYBRID`, or `DEFERRED`. The choice is recorded as FOREVER `AI_PROVIDER_MODE_SET` evidence with the operator's hardware-key signature. For `VAULT_BROKERED_EXTERNAL` and `HYBRID`, the external provider's API key is delivered into the vault broker via `KEY_REGISTER` and the provider's TLS cert chain is pinned.                               |
| `STAGE_FIRST_GROUP_REGISTRATION`       | The operator creates the first user group (e.g. `family`, `homelab`, `office`). The group's manifest is signed by `_system:service:identity-init`, written to `/aios/system/identity/groups/<group_id>`, and emits `FIRST_GROUP_REGISTERED` (FOREVER). The minimum is exactly one group; multi-group setups happen post-first-boot.                                                                                                                                 |
| `STAGE_FIRST_USER_REGISTRATION`        | The operator registers themselves as the first `HUMAN_USER` subject. Canonical id is `<first_group_id>:<user_id>`. The subject's credentials are bound (password + WebAuthn or hardware token per S5.1 §3 default). The subject is recorded as the host's first administrator. Emits `FIRST_USER_REGISTERED` (FOREVER).                                                                                                                                             |
| `STAGE_RUNTIME_SERVICES_STARTED`       | L3 SGR and the layers L4 through L9 come up in dependency order. L5 cognitive services start only if `AIProviderMode` is not `DEFERRED`; in `DEFERRED` mode, L5 is masked at this stage and the host runs translator-only (S1.1) until the operator configures a provider post-first-boot. A health probe at this stage gates the next transition.                                                                                                                  |
| `STAGE_FIRST_BOOT_COMPLETE`            | The terminal-commit stage. The host writes `/aios/system/firstboot/marker` (atomic write under `/aios/system/firstboot/`) containing the final state hash (BLAKE3 of the concatenation of invariant bundle id, policy bundle id, identity bundle id, vault root pubkey, AI provider mode, first-group id, first-user canonical id, initial firewall posture). The marker is the constitutional "first-boot is done" witness. Emits `FIRST_BOOT_COMPLETE` (FOREVER). |
| `STAGE_FAILED_REQUIRES_RECOVERY`       | A terminal failure has occurred at any prior stage. The host writes a partial-state record to `/recovery/firstboot/failure` (note: on `/recovery`, **not** `/aios`, because the constitutional layer may not be installable), emits `FIRST_BOOT_FAILED` (FOREVER) with the `FirstBootFailureReason` value, and auto-reboots into recovery mode (per S9.1 `RecoveryEntryReason.BOOT_FAILURE_AUTO`) for forensic attach. Forward path is reset-to-factory.            |

Allowed forward transitions (linear; no back-transitions):

```text
STAGE_INSTALLER_MEDIA_VERIFIED
   ─▶ STAGE_DISK_PARTITIONED
   ─▶ STAGE_KERNEL_INSTALLED
   ─▶ STAGE_AIOS_FS_INITIALIZED
   ─▶ STAGE_VAULT_ROOT_GENERATED
   ─▶ STAGE_INVARIANT_BUNDLE_LOADED
   ─▶ STAGE_POLICY_BUNDLE_LOADED
   ─▶ STAGE_IDENTITY_BUNDLE_LOADED
   ─▶ STAGE_RECOVERY_OPERATOR_REGISTRATION
   ─▶ STAGE_AI_PROVIDER_CONFIGURATION
   ─▶ STAGE_FIRST_GROUP_REGISTRATION
   ─▶ STAGE_FIRST_USER_REGISTRATION
   ─▶ STAGE_RUNTIME_SERVICES_STARTED
   ─▶ STAGE_FIRST_BOOT_COMPLETE         (terminal commit; non-idempotent)

(any stage)
   ─▶ STAGE_FAILED_REQUIRES_RECOVERY    (terminal failure; auto-reboot to recovery)
```

### §3.3 `AIProviderMode`

The closed list of AI provider configurations chosen at `STAGE_AI_PROVIDER_CONFIGURATION` (§3.2.10). The choice is recorded as FOREVER `AI_PROVIDER_MODE_SET` evidence and is durable across boots; changing it post-first-boot requires recovery mode (per INV-012, because the AI provider configuration affects vault material).

| Value                     | Meaning                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `LOCAL_ONLY`              | The host runs AI workloads exclusively on the local model-serving runtime (Ollama / vLLM-compatible per Rev.1 §21). No external network egress for AI is permitted; the network policy records a deny rule for AI-tagged egress to non-loopback addresses. The default for privacy-first installs (households, kiosk, air-gapped lab). No external API key is registered.                                                                                                                                            |
| `VAULT_BROKERED_EXTERNAL` | The host uses an external API (e.g., Anthropic, OpenAI, Mistral). The provider's API key is delivered into the vault broker as a `SECRET_BEARING` capability whose `class = KEY_USE` (the broker performs API calls on behalf of subjects without revealing the key material; per INV-018). External egress for AI is permitted only via the broker's outbound proxy, which enforces TLS pinning to the provider's cert chain. The operator records explicit consent at registration; the consent record is FOREVER. |
| `HYBRID`                  | A composite of `LOCAL_ONLY` and `VAULT_BROKERED_EXTERNAL`. The S1.2 latency router (per S1.2 `PrivacyClass` ceiling) decides per-request whether the workload is eligible for external dispatch. Workloads of class `SECRET_BEARING` and `CLASSIFIED` are pinned local; `PUBLIC` and `INTERNAL` may dispatch external; `SENSITIVE` defaults local with explicit per-action approval to dispatch external. The operator records the per-class routing table at registration as part of FOREVER evidence.              |
| `DEFERRED`                | The operator declines to configure an AI provider during first-boot. The host comes up in **translator-only mode** (S1.1 capability translator runs deterministically; no LLM inference). L5 cognitive services are masked at the systemd unit level until the operator configures a provider via a recovery-mode operation (INV-012 system-mutation scope: changing AIProviderMode is a recovery mutation). The host is fully usable for typed-action workflows; only LLM-mediated workflows are unavailable.       |

These four values exhaust the AI provider modes. There is no `EXTERNAL_RAW` value (raw external use without vault brokerage), because such a mode would expose the API key to the requester and violate INV-003 / INV-018.

### §3.4 `RecoveryCredentialKind`

The closed list of recovery operator credential kinds accepted at `STAGE_RECOVERY_OPERATOR_REGISTRATION` (§3.2.9). At least one credential MUST be registered before first-boot can advance (§10).

| Value          | Meaning                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `HARDWARE_KEY` | A FIDO2 / WebAuthn / PIV hardware token (Yubikey, Nitrokey, SoloKey, or equivalent). The strong-path credential and the only kind acceptable for new recovery operator registration **post-first-boot** (per S9.1 §6.2). The credential's public component is registered in the vault broker as `KEY_VERIFY` capability bound to `_system:local:operator-1`.                                                                                                                                                 |
| `PASSPHRASE`   | A high-entropy passphrase (minimum 64 bits estimated entropy after normalization). Acceptable as the **sole** recovery credential **only at first-boot**, and only if the operator explicitly opts out of `HARDWARE_KEY` and acknowledges the reduced trust posture (a typed action with explicit consent text). Subsequent uses emit `HEAVY_AUTH_FALLBACK_USED` evidence per S9.1 §6.2. The passphrase is hashed (Argon2id, parameters per L4 vault broker) and the hash is bound as a recovery credential. |
| `BOTH`         | Both `HARDWARE_KEY` and `PASSPHRASE` are registered. The recommended posture: hardware key for normal recovery, passphrase as a fallback when the hardware key is lost. Passphrase use still emits `HEAVY_AUTH_FALLBACK_USED`; the hardware key is the preferred path and the passphrase is the break-glass.                                                                                                                                                                                                 |

These three values exhaust the recovery credential kinds. There is no `BIOMETRIC` value in Rev.2 — biometrics are deferred (see §17). There is no `REMOTE_KEY_SERVER` value — recovery is local-only per S9.1 §6.3.

### §3.5 `FirstBootFailureReason`

The closed list of terminal-failure reasons. A failure carries exactly one value, recorded in `FIRST_BOOT_FAILED` (FOREVER) along with the failing `FirstBootStage` (§3.2) and an opaque diagnostic blob (no secret material; INV-015).

| Value                            | Where it can fire                                                                 | Meaning                                                                                                                                                                                                                                 |
| -------------------------------- | --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `MEDIA_SIGNATURE_INVALID`        | `STAGE_INSTALLER_MEDIA_VERIFIED`                                                  | The installation media did not verify against the embedded AIOS root public key. No disk-write side effect; first-boot is aborted in-installer.                                                                                         |
| `DISK_TOO_SMALL`                 | `STAGE_DISK_PARTITIONED`                                                          | The target disk does not meet the constitutional minimum (32 GB for `/`, 32 GB for `/aios`, 4 GB for `/recovery` = 68 GB minimum). No partial writes; partition table not committed.                                                    |
| `DISK_HARDWARE_FAILURE`          | `STAGE_DISK_PARTITIONED` / `STAGE_KERNEL_INSTALLED` / `STAGE_AIOS_FS_INITIALIZED` | The disk reported an unrecoverable I/O error during partition or write. Forensic attach in recovery; the operator must replace the disk before retrying.                                                                                |
| `KERNEL_SIGNATURE_INVALID`       | `STAGE_KERNEL_INSTALLED`                                                          | The generic kernel image's signature did not verify against AIOS root. Possible installer media tamper post-`STAGE_INSTALLER_MEDIA_VERIFIED` (e.g., supply-chain attack on a partial mirror).                                           |
| `VAULT_TPM_UNAVAILABLE`          | `STAGE_VAULT_ROOT_GENERATED`                                                      | The host has TPM 2.0 hardware but the chip is unhealthy or the operator selected TPM seal and TPM is unavailable. The operator must explicitly switch to hardware-key-only seal or abort.                                               |
| `VAULT_HARDWARE_KEY_UNAVAILABLE` | `STAGE_VAULT_ROOT_GENERATED`                                                      | The operator selected hardware-key seal but no hardware key is present at the prompt. Resumed from this stage on next boot when the operator reattempts with the hardware key.                                                          |
| `INVARIANT_BUNDLE_REJECTED`      | `STAGE_INVARIANT_BUNDLE_LOADED`                                                   | The initial L0 invariant bundle's signature did not verify, or the bundle did not contain INV-001 + INV-002 (the constitutional minimum). The installer aborts.                                                                         |
| `POLICY_BUNDLE_REJECTED`         | `STAGE_POLICY_BUNDLE_LOADED`                                                      | The initial Policy Kernel bundle failed compile-time validation (S2.3 `InvariantLooseningAttempted`, missing required hard-denies, or signature failure).                                                                               |
| `IDENTITY_BUNDLE_REJECTED`       | `STAGE_IDENTITY_BUNDLE_LOADED`                                                    | The initial identity bundle failed signature verification or did not contain the required `_system` service subjects.                                                                                                                   |
| `OPERATOR_ABORTED`               | Any operator-interactive stage (§3.2.9–§3.2.12)                                   | The operator commanded `installer-abort` from the first-boot installer surface. The host wipes any partial `/aios` state, marks the disk as unprovisioned, and powers off. Next boot starts at `FRESH_INSTALL` with no partial residue. |
| `RUNTIME_SERVICE_STARTUP_FAILED` | `STAGE_RUNTIME_SERVICES_STARTED`                                                  | A required L3–L9 service failed its health probe within the startup budget. The failure record names the failing service.                                                                                                               |
| `MARKER_WRITE_FAILED`            | `STAGE_FIRST_BOOT_COMPLETE`                                                       | The atomic write of `/aios/system/firstboot/marker` failed (disk full, AIOS-FS quarantine, or transient I/O). This is the only failure that fires after every other stage has completed; recovery operator inspects the partial state.  |

These twelve values exhaust the first-boot failure reasons. There is no `UNKNOWN` value — an unrecoverable fail that does not match any of these is itself a contract violation (the installer's fault-handling code must match one of these; if it cannot, it logs `MARKER_WRITE_FAILED` as the most-conservative terminal failure and forces a recovery boot).

### §3.6 `InitialFirewallPosture`

The closed list of network postures the operator may select at `STAGE_AI_PROVIDER_CONFIGURATION` (§3.2.10) for the first normal boot post-first-boot. The chosen posture is committed to `/aios/system/boot/firewall_initial` and is the default the L8 network policy applies until the operator explicitly changes it post-first-boot.

| Value               | Meaning                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `LOOPBACK_ONLY`     | The default. Only `127.0.0.1` and `::1` are reachable; LAN sockets are refused at bind. Binds INV-006 (Web UI localhost-only by default). The web renderer (L7.5) comes up exposed on loopback only; LAN exposure requires a post-first-boot policy decision. The most common posture for personal laptops and air-gapped lab installs.                                                                                                                                          |
| `LAN_LOCAL_DEFAULT` | The host accepts inbound traffic from RFC1918 / RFC4193 LAN ranges (selected automatically from the host's interface configuration; the operator does not type CIDRs). The Web UI binds on the LAN interface; access from outside the LAN is denied. Used for household servers and homelab installs where the operator plans to access the Web UI from other devices on the same network. The operator's choice is recorded as FOREVER `INITIAL_FIREWALL_POSTURE_SET` evidence. |
| `AIRGAP`            | All network interfaces are administratively down at boot. Used for forensic and air-gapped installs. The operator can re-enable a single interface post-first-boot via a recovery-mode operation. Mutually exclusive with `VAULT_BROKERED_EXTERNAL` and `HYBRID` AI provider modes; selecting `AIRGAP` forces `AIProviderMode` to `LOCAL_ONLY` or `DEFERRED`.                                                                                                                    |

Public-internet exposure (the host accepting inbound connections from outside the LAN) is **never** enabled at first-boot. There is no `PUBLIC` value in `InitialFirewallPosture`; public exposure is a recovery-mode operation post-install with co-signer approval (per INV-012 + INV-006 cascade). This is a constitutional default, not a default that operators flip casually.

## §4 The first-boot FSM

### §4.1 Position in the boot path

First-boot runs from a dedicated GRUB entry that the installation media writes to a new host's bootloader before any AIOS-managed entry exists. Once `STAGE_FIRST_BOOT_COMPLETE` writes the marker file, the bootloader configuration is rewritten by the installer to match the standard S9.1 §4.1 GRUB entry table (entries 0–3: normal/normal-fallback/recovery/recovery-fallback). Subsequent boots route through S9.1's boot decision logic; the first-boot entry is removed.

```text
       +--- installation media ---------------------+
       |  signed initramfs + AIOS root public key  |
       |  + L1 toolbox + first-boot installer       |
       +--------------------------------------------+
                            │
                            v
       +--- target disk (unprovisioned) ------------+
       |  (no /aios; no /; no /recovery)           |
       +--------------------------------------------+
                            │
                            v
       ┌────────────────── First-Boot FSM ─────────────────┐
       │ STAGE_INSTALLER_MEDIA_VERIFIED                    │
       │ STAGE_DISK_PARTITIONED                             │
       │ STAGE_KERNEL_INSTALLED                             │
       │ STAGE_AIOS_FS_INITIALIZED                          │
       │ STAGE_VAULT_ROOT_GENERATED                         │
       │ STAGE_INVARIANT_BUNDLE_LOADED                      │
       │ STAGE_POLICY_BUNDLE_LOADED                         │
       │ STAGE_IDENTITY_BUNDLE_LOADED                       │
       │ STAGE_RECOVERY_OPERATOR_REGISTRATION               │
       │ STAGE_AI_PROVIDER_CONFIGURATION                    │
       │ STAGE_FIRST_GROUP_REGISTRATION                     │
       │ STAGE_FIRST_USER_REGISTRATION                      │
       │ STAGE_RUNTIME_SERVICES_STARTED                     │
       │ STAGE_FIRST_BOOT_COMPLETE  ─── marker written ────▶│
       └───────────────────────────────────────────────────┘
                            │
                            v
       +--- post-first-boot ────────────────────────+
       |  S9.1 GRUB entries 0–3 active              |
       |  S9.3 dedicated kernel pipeline can run    |
       |  Normal mode by default                    |
       +--------------------------------------------+
```

### §4.2 Stage advancement

Stage advancement is performed by the `_system:service:firstboot-coordinator` service. The coordinator:

1. Reads the current stage marker from `/recovery/firstboot/state` (an installer-staged file on the recovery partition; the partition is created at `STAGE_DISK_PARTITIONED` and is the first-boot scratch area before `/aios` is initialised). The recovery partition is used because `/aios` is not yet writable for the first three stages.
2. From `STAGE_AIOS_FS_INITIALIZED` onwards, the stage marker moves to `/aios/system/firstboot/state` so it is integrated with the AIOS-FS evidence chain.
3. Performs the stage's atomic operation (signature verification, partition write, bundle load, etc.).
4. Verifies the operation succeeded (signature, hash, presence of expected file).
5. Emits `FIRST_BOOT_STAGE_COMPLETED` evidence (STANDARD_24M retention; one record per stage; the constitutional commits also emit a per-stage FOREVER record per §12).
6. Atomically updates the stage marker to the next stage.
7. Returns control to the installer surface for any operator-interactive prompt at the next stage.

#### §4.2.1 First-boot subject session flags

Each first-boot service subject session — `_system:service:installer`, `_system:service:vault-init`, `_system:service:identity-init`, `_system:service:policy-compiler`, `_system:service:firstboot-coordinator` (S9.2 §3.2.8) — carries `is_first_boot = true` for the duration of the first-boot window. The flag is set by the firstboot-coordinator at session bootstrap, immediately after the subject identity is loaded from the seed identity bundle at `STAGE_IDENTITY_BUNDLE_LOADED`. (For the three pre-bundle stages — `STAGE_INSTALLER_MEDIA_VERIFIED`, `STAGE_DISK_PARTITIONED`, `STAGE_KERNEL_INSTALLED` — the installer subject runs from the initramfs identity stub, which carries `is_first_boot = true` from the kernel command line `aios.mode=FIRST_BOOT` per S9.1 §3.2.)

The flag is cleared atomically with the firstboot-marker write at `STAGE_FIRST_BOOT_COMPLETE`: the same atomic operation that promotes the marker pointer also tears down the active first-boot service subject sessions, so no session with `is_first_boot = true` can survive into the post-first-boot `NORMAL` boot.

Per S2.3 (Wave 9), this flag participates in the `RecoveryRequiredForSystemMutation` hard-deny condition: the rule admits subjects with `is_recovery_mode = true` **or** `is_first_boot = true` (and denies all others) when the target path is in `RecoveryMutableScope`. Without this Wave 9 update, the initial Policy Kernel bundle loaded at `STAGE_POLICY_BUNDLE_LOADED` would deny every subsequent first-boot system mutation, deadlocking the bootstrap at `STAGE_IDENTITY_BUNDLE_LOADED` (the first stage that mutates a `RecoveryMutableScope` path after the policy bundle is active).

The mutual-exclusion invariant (S9.1 §3.2) holds: a first-boot service subject session has `is_first_boot = true` and `is_recovery_mode = false`. Recovery service subject sessions invert the pair. The Policy Kernel rejects any session that carries both flags with `MutuallyExclusiveModeFlags`.

### §4.3 Worked startup trace (happy path; abridged)

```text
T+00.0s   STAGE_INSTALLER_MEDIA_VERIFIED    Initramfs verifies media; AIOS root pubkey check OK.
T+00.0s   evidence: FIRST_BOOT_STARTED FOREVER {entry_reason: FRESH_INSTALL, ...}
T+00.0s   evidence: FIRST_BOOT_STAGE_COMPLETED STANDARD_24M {stage: INSTALLER_MEDIA_VERIFIED}
T+00.4s   STAGE_DISK_PARTITIONED            GPT layout written; /, /aios, /recovery partitions present.
T+02.1s   STAGE_KERNEL_INSTALLED            /boot/vmlinuz-generic + initramfs installed; signed.
T+04.7s   STAGE_AIOS_FS_INITIALIZED         /aios root object created; namespace catalog seeded.
T+06.8s   STAGE_VAULT_ROOT_GENERATED        Ed25519 keypair generated; sealed to TPM (PCR 0,2,4,7).
T+06.8s   evidence: VAULT_ROOT_KEY_GENERATED FOREVER {seal_kind: TPM, tpm_pcrs: [0,2,4,7]}
T+07.0s   STAGE_INVARIANT_BUNDLE_LOADED     invbundle_<hex> verified + active.
T+07.5s   STAGE_POLICY_BUNDLE_LOADED        Initial policy bundle compiled + loaded; default-deny.
T+07.9s   STAGE_IDENTITY_BUNDLE_LOADED      idbundle_<hex> with _system service subjects active.
T+08.0s   --- operator-interactive prompts begin ---
T+08.0s   STAGE_RECOVERY_OPERATOR_REGISTRATION
         Operator inserts hardware key; provides passphrase fallback.
         Credential kind = BOTH; written to /aios/system/recovery/operators/operator-1.
         evidence: RECOVERY_OPERATOR_REGISTERED FOREVER {credential_kind: BOTH, ...}
T+45.0s  STAGE_AI_PROVIDER_CONFIGURATION
         Operator chooses LOCAL_ONLY; declines external API key registration.
         Operator chooses InitialFirewallPosture = LAN_LOCAL_DEFAULT.
         evidence: AI_PROVIDER_MODE_SET FOREVER {mode: LOCAL_ONLY}
         evidence: INITIAL_FIREWALL_POSTURE_SET FOREVER {posture: LAN_LOCAL_DEFAULT}
T+90.0s  STAGE_FIRST_GROUP_REGISTRATION
         Operator names group "family"; group_id = "family"; group manifest written.
         evidence: FIRST_GROUP_REGISTERED FOREVER {group_id: family}
T+110.0s STAGE_FIRST_USER_REGISTRATION
         Operator registers as family:alice; password + WebAuthn enrolled.
         evidence: FIRST_USER_REGISTERED FOREVER {subject: family:alice}
T+115.0s STAGE_RUNTIME_SERVICES_STARTED
         L3 SGR up; L4 vault + identity + policy in normal mode; L8 network up
         with LAN_LOCAL_DEFAULT firewall; L9 evidence log producing.
         L5 cognitive services start (LOCAL_ONLY mode; Ollama runtime engaged).
T+125.0s STAGE_FIRST_BOOT_COMPLETE
         /aios/system/firstboot/marker atomically written.
         GRUB rewritten to S9.1 entries 0–3; first-boot entry removed.
         evidence: FIRST_BOOT_COMPLETE FOREVER {state_hash: <hex>, ...}
T+125.0s reboot to NORMAL mode (S9.1 entry 0).
```

## §5 Idempotency contract

### §5.1 Per-stage idempotency

Every stage in §3.2 except `STAGE_FIRST_BOOT_COMPLETE` is **idempotent on retry**. If first-boot is interrupted at stage `S`, the next boot resumes at the same stage `S` (not at `S+1`, not at the beginning) and re-performs the stage's atomic operation. Re-performing the operation:

- Produces the same observable result (same signature check, same partition table, same bundle hash).
- Either commits the operation or fails with the same `FirstBootFailureReason`; the operation does not silently produce a different state.
- Emits a fresh `FIRST_BOOT_STAGE_COMPLETED` record on success (the prior incomplete attempt has no completion record; the new attempt's record is the authoritative witness).

The atomicity guarantee is per-stage. Within a stage, the installer either completes the entire stage and writes the marker, or rolls back to the prior stage's committed state and leaves the marker pointing at the prior stage. There is no half-completed stage state visible to a subsequent boot.

### §5.2 The terminal-commit non-idempotency

`STAGE_FIRST_BOOT_COMPLETE` is the **only** non-idempotent stage. Once `/aios/system/firstboot/marker` is written, subsequent boots of the host see the marker and route through the standard S9.1 boot path. First-boot does not re-run.

The reason is constitutional. The marker's existence is the witness that the host has been provisioned; it is the input to the boot decision logic at `STAGE_PRE_KERNEL` (S9.1 §4.2). Re-running first-boot on a provisioned host would mean either the marker is being trusted (and first-boot is idempotently a no-op, defeating the purpose) or the marker is being ignored (and first-boot is destroying constitutional state without recovery-mode authorisation, violating INV-012). The clean answer is the only answer: the marker is authoritative; first-boot does not re-run; the only path to a fresh state is `RESET_TO_FACTORY_INITIATED` from recovery (§11).

### §5.3 The marker contract

The marker file at `/aios/system/firstboot/marker` carries:

```text
{
  "schema": "aios.firstboot.v1alpha1.Marker",
  "first_boot_completed_at": "<rfc3339_timestamp>",
  "entry_reason": "<FirstBootEntryReason>",
  "invariant_bundle_id": "invbundle_<hex>",
  "policy_bundle_id": "polbundle_<hex>",
  "identity_bundle_id": "idbundle_<hex>",
  "vault_root_pubkey": "<ed25519_pub_b64>",
  "ai_provider_mode": "<AIProviderMode>",
  "first_group_id": "<group_id>",
  "first_user_canonical_id": "<canonical_subject_id>",
  "initial_firewall_posture": "<InitialFirewallPosture>",
  "recovery_credential_kind": "<RecoveryCredentialKind>",
  "state_hash": "<blake3_hex>",
  "signature": "<ed25519_sig_b64>"
}
```

The signature is produced by the vault root key (§3.2.5). The marker is verified at every subsequent boot at `STAGE_ROOT_MOUNTED` of S9.1's normal-boot equivalent; signature failure is treated as `INVARIANT_BUNDLE_SIGNATURE_FAILURE` and triggers recovery boot per S9.1 §3.3.

### §5.4 Atomic write semantics

The marker is written atomically using the AIOS-FS pointer move CAS (S1.3 §6 / S2.2 D10). The sequence:

1. Coordinator computes the state hash (BLAKE3 of the canonical-form concatenation of the bundle ids, vault root pubkey, AI provider mode, first-group id, first-user canonical id, initial firewall posture, and recovery credential kind).
2. Coordinator signs the canonical form with the vault root private key (use-without-reveal `KEY_SIGN` against the vault broker's root capability).
3. Coordinator writes the marker as a new AIOS-FS object version under `/aios/system/firstboot/`.
4. Coordinator promotes the `/aios/system/firstboot/marker` pointer to the new version using two-phase commit CAS.
5. On success: emits `FIRST_BOOT_COMPLETE` (FOREVER) and triggers the GRUB rewrite + reboot.
6. On CAS conflict: emits `MARKER_WRITE_FAILED` and routes to `STAGE_FAILED_REQUIRES_RECOVERY`. (CAS conflict at this stage is itself a tampering signal — no other writer should be active on this pointer.)

## §6 Per-stage acceptance signals

Each stage has a single deterministic acceptance signal. The coordinator advances only when the signal is observed; absence of the signal within a stage-specific timeout is a `FirstBootFailureReason`.

| Stage                                  | Acceptance signal                                                                                                                                                  | Stage timeout                                              |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------- |
| `STAGE_INSTALLER_MEDIA_VERIFIED`       | The media's Ed25519 signature verifies against the embedded AIOS root public key.                                                                                  | 5 s                                                        |
| `STAGE_DISK_PARTITIONED`               | GPT table written; partitions `/`, `/aios`, `/recovery` present at expected sizes; partition labels match the constitutional layout.                               | 60 s                                                       |
| `STAGE_KERNEL_INSTALLED`               | `/boot/vmlinuz-generic` BLAKE3 matches the catalog hash; initramfs BLAKE3 matches; both signed by AIOS root.                                                       | 120 s                                                      |
| `STAGE_AIOS_FS_INITIALIZED`            | AIOS-FS root object exists; namespace catalog has every constitutional system path.                                                                                | 60 s                                                       |
| `STAGE_VAULT_ROOT_GENERATED`           | Ed25519 keypair generated; private half sealed (TPM or hardware key); public half present at `/aios/system/vault/root.pub`.                                        | 30 s (TPM) / unbounded operator-interactive (hardware key) |
| `STAGE_INVARIANT_BUNDLE_LOADED`        | Bundle signature verifies; bundle contains INV-001 + INV-002 (constitutional minimum); INV-001..INV-024 present.                                                   | 10 s                                                       |
| `STAGE_POLICY_BUNDLE_LOADED`           | Bundle compiles; default-deny rule present; INV-008/INV-012/INV-013/INV-018 hard-denies present; signature verifies.                                               | 30 s                                                       |
| `STAGE_IDENTITY_BUNDLE_LOADED`         | Bundle signature verifies; required `_system` service subjects present.                                                                                            | 10 s                                                       |
| `STAGE_RECOVERY_OPERATOR_REGISTRATION` | At least one credential of `RecoveryCredentialKind` is registered and signed by vault root.                                                                        | unbounded operator-interactive                             |
| `STAGE_AI_PROVIDER_CONFIGURATION`      | `AIProviderMode` chosen; if `VAULT_BROKERED_EXTERNAL` or `HYBRID`, external key registered in vault and TLS pin recorded; `InitialFirewallPosture` chosen.         | unbounded operator-interactive                             |
| `STAGE_FIRST_GROUP_REGISTRATION`       | Group manifest written; `FIRST_GROUP_REGISTERED` evidence emitted; group id matches the S4.1 §7.1 regex.                                                           | unbounded operator-interactive                             |
| `STAGE_FIRST_USER_REGISTRATION`        | HUMAN_USER subject created; credentials enrolled; subject is admin of the first group.                                                                             | unbounded operator-interactive                             |
| `STAGE_RUNTIME_SERVICES_STARTED`       | L3 SGR reports healthy; L4 services healthy; L8 network up with selected firewall posture; L9 evidence log producing; L5 healthy iff `AIProviderMode != DEFERRED`. | 120 s                                                      |
| `STAGE_FIRST_BOOT_COMPLETE`            | Marker written and pointer-CAS-promoted; signature verifies; GRUB rewrite committed; reboot scheduled.                                                             | 30 s                                                       |

Operator-interactive stages have no automatic timeout — the operator may take as long as they need to insert hardware keys, type passphrases, or choose configuration values. The installer surface preserves any partial input across operator-initiated power-cycles (text fields are not preserved; only committed values). An `OPERATOR_ABORTED` failure can fire from any operator-interactive stage at the operator's command.

### §6.1 Per-stage rollback table

When a stage fails partway through (power loss, transient I/O, signature mismatch on the partial output), the coordinator on next boot rolls back any partial side effects before re-attempting. The rollback table is per-stage; each stage declares the exact set of files and AIOS-FS objects it touches, so resume is mechanical.

| Stage                                  | Rollback action on partial state                                                                                                                                                                                                                                                                                                                                                                                         |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `STAGE_INSTALLER_MEDIA_VERIFIED`       | None. Verification is read-only against the media; there is no side effect to roll back. A failed verification is terminal (`MEDIA_SIGNATURE_INVALID`).                                                                                                                                                                                                                                                                  |
| `STAGE_DISK_PARTITIONED`               | Re-zero the GPT header and partition entries; rewrite the GPT layout. The partition table write is atomic at the disk-sector level; a partial GPT is detected by the GPT CRC and rejected at next boot.                                                                                                                                                                                                                  |
| `STAGE_KERNEL_INSTALLED`               | Delete `/boot/vmlinuz-generic.partial` and `/boot/initramfs-generic.partial` if present; redo the install from the signed installer payload.                                                                                                                                                                                                                                                                             |
| `STAGE_AIOS_FS_INITIALIZED`            | Re-initialise the `/aios` AIOS-FS root from scratch; the prior partial root object is replaced. The constitutional namespace catalog is re-seeded.                                                                                                                                                                                                                                                                       |
| `STAGE_VAULT_ROOT_GENERATED`           | Delete any partial `/aios/system/vault/root.pub` and `/aios/system/vault/root.sealed`; if TPM seal was attempted, evict the prior PCR-bound seal blob; regenerate keypair from scratch.                                                                                                                                                                                                                                  |
| `STAGE_INVARIANT_BUNDLE_LOADED`        | Delete any partial bundle file under `/aios/system/governance/invariants/`; re-attempt load from the installer payload.                                                                                                                                                                                                                                                                                                  |
| `STAGE_POLICY_BUNDLE_LOADED`           | Delete any partial bundle file under `/aios/system/policy/`; re-attempt compile and load.                                                                                                                                                                                                                                                                                                                                |
| `STAGE_IDENTITY_BUNDLE_LOADED`         | Delete any partial bundle file under `/aios/system/identity/`; re-attempt load.                                                                                                                                                                                                                                                                                                                                          |
| `STAGE_RECOVERY_OPERATOR_REGISTRATION` | If a credential was partially registered (e.g., hardware key public component captured but signature failed), discard the partial registration; the operator re-presents the credential.                                                                                                                                                                                                                                 |
| `STAGE_AI_PROVIDER_CONFIGURATION`      | If an external API key was partially registered (e.g., key delivered to the vault but TLS pin verification failed), invalidate the broker capability for that key and re-prompt for the choice.                                                                                                                                                                                                                          |
| `STAGE_FIRST_GROUP_REGISTRATION`       | If a group manifest was partially written, discard it; the operator re-enters the group fields.                                                                                                                                                                                                                                                                                                                          |
| `STAGE_FIRST_USER_REGISTRATION`        | If a user subject was partially created (e.g., subject record written but credentials not enrolled), retire the partial subject record (it is never reused) and create a new subject with a different `user_id` if needed.                                                                                                                                                                                               |
| `STAGE_RUNTIME_SERVICES_STARTED`       | Tear down any partially started services; recompute service health; re-attempt startup in dependency order.                                                                                                                                                                                                                                                                                                              |
| `STAGE_FIRST_BOOT_COMPLETE`            | If the marker write CAS fails, no rollback is needed — the prior pointer state is intact. The coordinator regenerates the state hash and re-attempts the CAS. If the marker is partially visible (impossible under the AIOS-FS pointer-CAS protocol but listed here for completeness), the boot decision logic refuses to honour an unsigned or signature-failing marker and routes to `STAGE_FAILED_REQUIRES_RECOVERY`. |

The rollback contract is per-stage; cross-stage rollback (rolling back to an earlier stage than the current one) is not supported. The reason is constitutional discipline: each stage's outputs are inputs to the next stage; rolling back two stages would require unwinding the next stage's commitments, which would in turn require operator re-input at the prior interactive stage. The forward-only FSM keeps the resume protocol mechanical.

### §6.2 Diagnostic blob contract

When a stage fails, the coordinator captures a **diagnostic blob** that is referenced (by hash) in the `FIRST_BOOT_FAILED` evidence record. The blob contains:

- The stage that failed.
- The failure reason value.
- A redacted system log tail (60 seconds before failure; redacted per S3.1 secret-pattern filter; INV-015).
- The exit code or signature-verification result of the failing operation.
- The current AIOS-FS root version id (when AIOS-FS is initialised).

The blob does **not** contain:

- Any operator credential material (passphrase, hardware-key challenge response, password).
- Any external AI provider API key material.
- Any TPM-sealed material (only the PCR set used for the seal attempt is recorded).

The blob is written to `/recovery/firstboot/diagnostics/<failure_id>.blob` on the recovery partition and is BLAKE3-hashed; the hash is the `diagnostic_blob_hash` field of the `FIRST_BOOT_FAILED` record. A recovery operator with forensic-attach access (per S9.1 §5.2) can inspect the blob; the contents are bounded so they never carry secret material.

## §7 AI provider configuration (`STAGE_AI_PROVIDER_CONFIGURATION`)

### §7.1 The choice

At `STAGE_AI_PROVIDER_CONFIGURATION`, the installer surface prompts the operator with the four `AIProviderMode` values (§3.3). Each value is presented with its constitutional consequences in plain operational terms (no jargon). The operator selects exactly one.

The installer surface for this stage is constrained: it is an `AIOS_SURFACE` (per L7.1 §6.3 / S9.1 §6.1) — no app surfaces, no AI-mediated chrome (consistent with the no-L5-during-recovery posture per INV-001, extended to first-boot here because the L5 services have not yet been started). The operator's choice is captured in a typed action signed by the operator's hardware key (or, in the no-hardware-key install path, by the recovery passphrase used in `STAGE_RECOVERY_OPERATOR_REGISTRATION`).

### §7.2 LOCAL_ONLY semantics

`LOCAL_ONLY` is the most permissive mode for privacy and the most constrained for cognitive capability. The installer:

1. Does not prompt for an external API key.
2. Records `AIProviderMode = LOCAL_ONLY` in the vault broker's policy state.
3. Configures the L8 network policy with a deny rule for AI-tagged egress to non-loopback addresses (`is_ai = true` action subjects + remote address in the public-internet space → reject). The deny rule is a constitutional default, not a soft preference; lifting it requires recovery mode.
4. At `STAGE_RUNTIME_SERVICES_STARTED`, starts the local model-serving runtime (Ollama or vLLM-compatible per Rev.1 §21).
5. Emits `AI_PROVIDER_MODE_SET` (FOREVER) with `mode: LOCAL_ONLY`.

Operators on hosts without sufficient local compute (no GPU, low RAM) get a yellow advisory at the prompt: "local model serving may be slow on this host". The advisory is informative; it does not gate the choice.

### §7.3 VAULT_BROKERED_EXTERNAL semantics

`VAULT_BROKERED_EXTERNAL` introduces an external network dependency for AI workloads. The installer:

1. Prompts the operator for the provider (closed list: Anthropic, OpenAI, Mistral, Google, AIOS-Cloud — each entry corresponds to a signed provider attestation chain bundled with the installer).
2. Prompts the operator for the API key, **received as a paste-only field** that is not echoed and not stored in any file. The key is delivered directly into the vault broker via `KEY_REGISTER` (§3.2.5 vault root key authority is used here as the bootstrapper; subsequent registrations post-first-boot use the operator's credential).
3. Verifies the provider's TLS cert chain against the bundled attestation. A chain mismatch fails the registration with `OPERATOR_ABORTED` (the operator must explicitly re-attempt with confirmed provider details).
4. Records `AIProviderMode = VAULT_BROKERED_EXTERNAL` and the provider id in the vault broker's policy state.
5. Configures the L8 network policy: AI-tagged egress is permitted exclusively via the vault broker's outbound proxy; non-broker AI egress is rejected.
6. Emits `AI_PROVIDER_MODE_SET` (FOREVER) with `mode: VAULT_BROKERED_EXTERNAL` and `provider_id` (no key material; INV-015).

The broker's outbound proxy enforces TLS pinning to the provider's cert chain at every request; pin-failure terminates the request and emits a vault-side evidence record (per S5.2 vocabulary). The API key is never accessible to L5 subjects — they invoke the broker's `ProxyAICall` capability which hides the material (per INV-018).

### §7.4 HYBRID semantics

`HYBRID` is `LOCAL_ONLY` + `VAULT_BROKERED_EXTERNAL` with a routing table. The installer:

1. Prompts as in §7.3 for the external provider.
2. Prompts for the per-`PrivacyClass` routing table (a four-row form: PUBLIC → external default, INTERNAL → external allowed, SENSITIVE → local default with explicit per-action approval to dispatch external, SECRET_BEARING + CLASSIFIED → local pinned).
3. Records the routing table in the vault broker's policy state.
4. Configures the L8 network policy with the same broker-only egress rule as `VAULT_BROKERED_EXTERNAL`.
5. Emits `AI_PROVIDER_MODE_SET` (FOREVER) with `mode: HYBRID`, `provider_id`, and the canonical-form routing table.

### §7.5 DEFERRED semantics

`DEFERRED` is the no-LLM mode. The installer:

1. Records `AIProviderMode = DEFERRED`.
2. Configures the L5 cognitive services as masked at the systemd unit level.
3. At `STAGE_RUNTIME_SERVICES_STARTED`, the host comes up in translator-only mode (S1.1 deterministic translator runs; no LLM inference path is available).
4. Emits `AI_PROVIDER_MODE_SET` (FOREVER) with `mode: DEFERRED`.

The operator is informed that LLM-mediated workflows are unavailable until they configure a provider via a recovery-mode operation (the configuration change is constitutional per INV-012 because it affects vault material and the L8 network policy).

### §7.6 Mode change post-first-boot

Changing `AIProviderMode` post-first-boot is a recovery-mode operation. The operator boots into recovery (S9.1), authenticates with hardware key, submits a typed action targeting `/aios/system/vault/ai_provider_mode` (a path under `RecoveryMutableScope.VAULT_ROOT_MATERIAL`), and emits a fresh `AI_PROVIDER_MODE_SET` (FOREVER). The forever-retention chain across the original first-boot record and any subsequent change records is the audit witness of the host's AI provider history.

## §8 Initial firewall posture (`STAGE_AI_PROVIDER_CONFIGURATION`, sub-step)

### §8.1 The choice

Co-located with the AI provider configuration prompt, the installer prompts for `InitialFirewallPosture` (§3.6). The default selection is `LOOPBACK_ONLY`.

### §8.2 LOOPBACK_ONLY constitutional default

`LOOPBACK_ONLY` binds INV-006 (Web UI localhost-only by default). The L8 network policy is configured to deny inbound connections from non-loopback addresses across all services. The Web renderer (L7.5) binds to `127.0.0.1` and `::1` only. The host is fully usable by an operator at the local console; remote access requires a post-first-boot operation.

This is the recommended posture for personal laptops, kiosk installs, and any install where the operator does not explicitly need LAN-side access to the Web UI.

### §8.3 LAN_LOCAL_DEFAULT semantics

`LAN_LOCAL_DEFAULT` enables LAN exposure for the Web UI and any other services bound to LAN-eligible addresses. The installer:

1. Inspects the host's network interface configuration.
2. Computes the LAN range(s) by inspecting interface IP + netmask combinations and matching against RFC1918 / RFC4193 ranges. The operator does not type CIDRs; the installer derives them.
3. Configures the L8 network policy to permit inbound connections from the derived LAN ranges only; non-LAN inbound is denied.
4. Emits `INITIAL_FIREWALL_POSTURE_SET` (FOREVER) with `posture: LAN_LOCAL_DEFAULT` and the derived LAN ranges.

If the host has no LAN-eligible interface (e.g., only loopback and a public IP), `LAN_LOCAL_DEFAULT` falls back to `LOOPBACK_ONLY` and emits an operator-visible advisory; the operator can re-attempt selection or accept the fallback.

### §8.4 AIRGAP semantics

`AIRGAP` brings all network interfaces administratively down at boot. The installer:

1. Configures the L8 network policy to administratively down every detected interface.
2. Records `InitialFirewallPosture = AIRGAP`.
3. If `AIProviderMode` is `VAULT_BROKERED_EXTERNAL` or `HYBRID`, fails the configuration with `OPERATOR_ABORTED` and prompts the operator to re-select either AI mode or firewall posture. (Air-gapped hosts cannot reach external AI providers; the contradiction is rejected at the source.)
4. Emits `INITIAL_FIREWALL_POSTURE_SET` (FOREVER) with `posture: AIRGAP`.

### §8.5 No PUBLIC posture at first-boot

There is no `PUBLIC` value in `InitialFirewallPosture`. Public-internet exposure (the host accepting inbound connections from outside the LAN) is **never** enabled at first-boot. This is a constitutional default that binds INV-006 + the operator-skill assumption that public exposure is a deliberate, deliberate-with-evidence decision.

Public exposure is enabled exclusively post-first-boot via a recovery-mode typed action targeting the L8 network policy. The action requires:

- Recovery mode (per INV-012).
- A HUMAN_USER co-signer (S5.4 STRONG_SOLO is not sufficient; public exposure requires a co-signer different from the recovery operator).
- A FOREVER `WEB_PUBLIC_EXPOSURE_GRANTED` evidence record (per L7.5 / S3.1 §6.3 vocabulary).

A first-boot installer that attempted to expose the Web UI publicly would itself be a contract violation; this spec rejects such a path at the schema level (no enum value to express it).

## §9 First group + first user (`STAGE_FIRST_GROUP_REGISTRATION` and `STAGE_FIRST_USER_REGISTRATION`)

### §9.1 The constitutional minimum

Before `STAGE_FIRST_BOOT_COMPLETE`, the host MUST have at least one group and at least one HUMAN_USER subject. The minimum is exactly one of each; multi-group and multi-user setups happen post-first-boot.

The reason is constitutional. Every action in AIOS is performed by a subject with a primary group (S5.1 §4 / §6.2). A host with no group has no namespace for non-system actions; a host with no HUMAN_USER has no constitutional approver for any future recovery operation (recovery requires a human operator, per S9.1 §6). A first-boot that completes without a group + human user is a host that cannot subsequently authorise its own recovery — a deadlock at the constitutional layer.

### §9.2 Group registration prompt

At `STAGE_FIRST_GROUP_REGISTRATION`, the installer prompts the operator for:

1. Group id (matches S4.1 §7.1 regex `^[a-z][a-z0-9_-]{0,62}$`); recommended values are surfaced as suggestions (`family`, `homelab`, `office`, `studio`, `kiosk`) but the operator types their choice.
2. Group display name (free text, ≤ 256 bytes; UTF-8).
3. Group tier (closed `GroupTier` enum from S5.1 §5.1: `PERSONAL`, `TEAM`, `ORGANIZATIONAL`, `FINANCE`). Default `PERSONAL`.
4. Whether the group can host AI agents (`can_have_ai_agents` flag from S5.1 §5.1). Default true.
5. Whether the group can install apps (`can_install_apps` flag). Default true.

The installer signs the group manifest with `_system:service:identity-init` (a service subject seeded by `STAGE_IDENTITY_BUNDLE_LOADED`) and writes it to `/aios/system/identity/groups/<group_id>`. Emits `FIRST_GROUP_REGISTERED` (FOREVER).

### §9.3 User registration prompt

At `STAGE_FIRST_USER_REGISTRATION`, the installer prompts the operator for:

1. User id (matches S4.1 §7.1 regex; commonly the operator's first name in lowercase).
2. Display name (free text; UTF-8).
3. Password (minimum entropy threshold per L4 vault broker; the installer's input field never echoes the password).
4. WebAuthn or hardware-token enrolment (hardware key registered as a `KEY_VERIFY` capability bound to the new user subject). The hardware key may be the same one registered for recovery (`STAGE_RECOVERY_OPERATOR_REGISTRATION`); the installer detects this and offers to use the same key with a different challenge.

The user subject is created with canonical id `<first_group_id>:<user_id>` (per S5.1 §4.1) and is granted the `group_admin` role in the first group (so the operator can subsequently register additional users, agents, and apps in normal mode). The user's primary_group is set to the first group.

Emits `FIRST_USER_REGISTERED` (FOREVER) with the canonical subject id and the credential kinds enrolled.

### §9.4 No multi-group multi-user during first-boot

The installer surface at `STAGE_FIRST_GROUP_REGISTRATION` and `STAGE_FIRST_USER_REGISTRATION` accepts exactly one group and one user respectively. There is no "add another group" or "add another user" button. The operator who needs more groups or more users registers them post-first-boot in normal mode using the standard identity model RPCs (S5.1 §13).

The reason is scope discipline: first-boot is a constitutional commit; it is not a bulk-provisioning workflow. Bulk-provisioning workflows are post-first-boot operations that operate against an established constitutional layer.

## §10 Recovery operator registration (`STAGE_RECOVERY_OPERATOR_REGISTRATION`)

### §10.1 The contract

Before any of the operator-interactive stages, the operator MUST register a recovery operator credential. This stage cannot be skipped. The installer surface refuses to advance to `STAGE_AI_PROVIDER_CONFIGURATION` without an accepted credential.

The reason is the post-first-boot trust model. After first-boot, the only way for the operator to mutate the constitutional layer (policy bundle, identity bundle, vault root, AI provider mode, dedicated kernel slot, etc.) is via recovery mode (per INV-012). Without a registered recovery operator credential, the host is an unrecoverable host: the operator can run normal-mode workloads but can never authorise a constitutional change.

A first-boot that completes without a recovery operator credential is therefore a contract violation. The installer enforces this at the FSM level: `STAGE_RECOVERY_OPERATOR_REGISTRATION` is a barrier stage.

### §10.2 The prompt

The installer prompts the operator with the three `RecoveryCredentialKind` values (§3.4):

- `HARDWARE_KEY` — recommended.
- `PASSPHRASE` — accepted only if the operator explicitly opts out of `HARDWARE_KEY` and acknowledges the reduced trust posture.
- `BOTH` — recommended for resilience.

The operator's selection is captured in a typed action whose target field is the registration; the action is recorded in evidence at FOREVER retention.

### §10.3 Registration mechanics

The credential's public component is registered in the vault broker as a `KEY_VERIFY` capability bound to `_system:local:operator-1` (the canonical id assigned to the host's first recovery operator). The credential record is written to `/aios/system/recovery/operators/operator-1` and signed by the vault root key.

For `HARDWARE_KEY`: the operator inserts the hardware key, taps to confirm a challenge, and the public key + attestation chain are registered. The hardware key's serial number is recorded (no private material; no biometric template).

For `PASSPHRASE`: the operator types a passphrase ≥ 64 bits estimated entropy after normalization; the installer hashes (Argon2id) and registers the hash; the passphrase is never persisted in plain form.

For `BOTH`: both registration flows run; the resulting credential record carries both verifiers.

### §10.4 No skip path

The installer does not present a "skip" option for `STAGE_RECOVERY_OPERATOR_REGISTRATION`. An operator who insists on no recovery credential can only abort first-boot (`OPERATOR_ABORTED`). This is intentional: a host with no recovery operator is constitutionally crippled, and the installer refuses to produce one.

Emits `RECOVERY_OPERATOR_REGISTERED` (FOREVER) with the canonical operator id, credential kind, and the registration timestamp (no key material; no passphrase hash; INV-015).

## §11 Terminal commit and reset-to-factory

### §11.1 The terminal commit

`STAGE_FIRST_BOOT_COMPLETE` is the terminal-commit stage. Once the marker is written and the GRUB rewrite is committed, first-boot is done. Subsequent boots route through S9.1; the first-boot installer is removed from GRUB.

The marker's signature (vault-root-signed; §5.3) is the witness that the constitutional layer has been bootstrapped through this exact flow. Subsequent boots verify the signature; failure triggers recovery boot per S9.1 §3.3 (`INVARIANT_BUNDLE_SIGNATURE_FAILURE` is a generalisable case that covers marker-signature failure here).

The terminal commit also extinguishes `RecoveryMode = FIRST_BOOT` (S9.1 §3.2). The atomic operation that promotes the firstboot-marker pointer is the same operation that drops `is_first_boot = true` from every active first-boot service subject session (`installer`, `vault-init`, `identity-init`, `policy-compiler`, `firstboot-coordinator`; per §4.2.1). The drop is atomic with the marker write: there is no window in which the marker is committed but a session still carries `is_first_boot = true`, and there is no window in which the flag is cleared but the marker has not yet committed. The host then reboots; the next boot enters `RecoveryMode = NORMAL` via the rewritten GRUB entry table (S9.1 §4.1), and any subject session created post-reboot carries `is_first_boot = false` from creation. Cross-reference: S9.1 §3.2 mode definition; S9.1 §3.2.1 first-boot write permissions; this section closes the lifecycle the §3.2 definition opens.

### §11.2 Reset-to-factory

The only path back to a fresh state is **reset-to-factory** — a recovery-mode operation. The flow:

1. Operator boots into recovery (per S9.1).
2. Operator authenticates with hardware key.
3. Operator submits a typed action `recovery.firstboot.reset` whose target is `/aios/system/firstboot/marker`. The action requires:
   - `recovery_mode = true` (per S9.1 §7.1).
   - A second human co-signer (a different `_system:local:operator-<id>` or, if no second operator is registered, a documented fallback path requiring the recovery passphrase + hardware key from the same operator with a 60-second pause).
   - An explicit confirmation text prompt (the operator types `RESET-TO-FACTORY-<host_hostname>` — the host's hostname as a typing barrier against accidental confirmation).
4. The recovery service:
   - Emits `RESET_TO_FACTORY_INITIATED` (FOREVER) with the operator id, the co-signer id, the host's hostname, the current marker's `state_hash`, and the timestamp.
   - Wipes `/aios` (every namespace under `/aios/`); the partition is zeroed at the AIOS-FS layer; the partition table for `/aios` is preserved but the AIOS-FS root object is deleted.
   - Optionally retains the recovery operator credential set in `/recovery/firstboot/preserved_operators` (the operator's choice at confirmation; default is "wipe everything"). If retained, the new first-boot run can re-register the same credentials.
   - Rewrites the bootloader to point at the first-boot installer and reboots.
5. The next boot is first-boot with `entry_reason = RESET_TO_FACTORY` (§3.1).

### §11.3 No silent reset

Reset-to-factory is the only path back to first-boot. There is no silent reset, no policy-bundle reset, no AI-initiated reset. An AI subject (`is_ai = true`) attempting to submit `recovery.firstboot.reset` is rejected by the Policy Kernel hard-deny `AISystemAdminBlocked` (per INV-013); a normal-mode subject is rejected by the Policy Kernel hard-deny `RecoveryRequiredForSystemMutation` (per INV-012). The two hard-denies in cascade form the constitutional barrier against unauthorised reset.

### §11.4 The two-record audit chain

Reset-to-factory produces a two-record audit chain:

- `RESET_TO_FACTORY_INITIATED` (FOREVER) — the recovery-mode operation that wiped the prior state.
- `FIRST_BOOT_STARTED` (FOREVER, `entry_reason = RESET_TO_FACTORY`) — the new first-boot run.

Both records carry the host's hostname; the prior `FIRST_BOOT_COMPLETE` (FOREVER) record's `state_hash` is referenced in the `RESET_TO_FACTORY_INITIATED` payload as `prior_state_hash`. The chain is queryable by audit subjects post-completion: a host's lifetime constitutional history is the sequence of `FIRST_BOOT_STARTED` / `RESET_TO_FACTORY_INITIATED` / `FIRST_BOOT_COMPLETE` records in the evidence log.

## §12 Evidence record types

The following eleven record types are queued for S3.1 vocabulary, with retention classes as listed. They are the audit witnesses for everything in this spec.

| Record type                    | Retention    | Payload (key fields)                                                                                                                                                                                                                                                 |
| ------------------------------ | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `FIRST_BOOT_STARTED`           | FOREVER      | `entry_reason: FirstBootEntryReason`, `host_hostname`, `media_signature_id`, `prior_state_hash` (only when `entry_reason = RESET_TO_FACTORY`), `started_at`                                                                                                          |
| `FIRST_BOOT_STAGE_COMPLETED`   | STANDARD_24M | `stage: FirstBootStage`, `stage_started_at`, `stage_completed_at`, `attempt_count` (≥ 1; > 1 indicates an idempotent retry)                                                                                                                                          |
| `FIRST_BOOT_FAILED`            | FOREVER      | `failed_stage: FirstBootStage`, `failure_reason: FirstBootFailureReason`, `diagnostic_blob_hash` (no secret material; INV-015), `failed_at`                                                                                                                          |
| `VAULT_ROOT_KEY_GENERATED`     | FOREVER      | `seal_kind: {TPM, HARDWARE_KEY, HARDWARE_KEY_FILE}`, `tpm_pcrs` (when TPM), `hardware_key_serial_hash` (when HARDWARE_KEY), `pubkey_hash`, `generated_at`                                                                                                            |
| `AI_PROVIDER_MODE_SET`         | FOREVER      | `mode: AIProviderMode`, `provider_id` (for VAULT_BROKERED_EXTERNAL / HYBRID; closed list), `routing_table_hash` (for HYBRID), `subject_canonical_id` of operator, `signature` of operator credential (no key material)                                               |
| `INITIAL_FIREWALL_POSTURE_SET` | FOREVER      | `posture: InitialFirewallPosture`, `derived_lan_ranges` (when LAN_LOCAL_DEFAULT), `subject_canonical_id`, `set_at`                                                                                                                                                   |
| `FIRST_GROUP_REGISTERED`       | FOREVER      | `group_id`, `group_tier: GroupTier`, `can_have_ai_agents`, `can_install_apps`, `created_by: subject_canonical_id`, `created_at`                                                                                                                                      |
| `FIRST_USER_REGISTERED`        | FOREVER      | `subject_canonical_id`, `enrolled_credential_kinds` (closed list of credential kinds), `is_first_group_admin: true`, `created_at`                                                                                                                                    |
| `RECOVERY_OPERATOR_REGISTERED` | FOREVER      | `operator_canonical_id`, `credential_kind: RecoveryCredentialKind`, `hardware_key_serial_hash` (when applicable), `registered_at`                                                                                                                                    |
| `FIRST_BOOT_COMPLETE`          | FOREVER      | `state_hash`, `invariant_bundle_id`, `policy_bundle_id`, `identity_bundle_id`, `vault_root_pubkey_hash`, `ai_provider_mode`, `first_group_id`, `first_user_canonical_id`, `initial_firewall_posture`, `recovery_credential_kind`, `marker_signature`, `completed_at` |
| `RESET_TO_FACTORY_INITIATED`   | FOREVER      | `operator_canonical_id`, `co_signer_canonical_id`, `host_hostname`, `prior_state_hash`, `preserved_operators` (boolean; whether recovery operator set was retained), `initiated_at`                                                                                  |

Reason codes queued for S3.1 reason-code vocabulary alongside the records:

- `MediaSignatureInvalid`
- `DiskTooSmall`
- `KernelSignatureInvalid`
- `VaultTpmUnavailable`
- `VaultHardwareKeyUnavailable`
- `InvariantBundleRejected`
- `PolicyBundleRejected`
- `IdentityBundleRejected`
- `OperatorAborted`
- `RuntimeServiceStartupFailed`
- `MarkerWriteFailed`
- `DiskHardwareFailure`

These eleven record types and twelve reason codes constitute the S9.2 contribution to the S3.1 next-Wave consolidation.

## §13 Worked examples

### §13.1 Example 1 — Standard fresh install on a TPM 2.0 laptop (happy path)

```text
Hardware: ThinkPad-class laptop; TPM 2.0 (healthy, PCR 0,2,4,7 stable); 512 GB NVMe;
          Yubikey 5C inserted at the operator's request.
Media:    Signed AIOS installation USB.

T+00.0s   STAGE_INSTALLER_MEDIA_VERIFIED
          Initramfs verifies USB signature against AIOS root pubkey (embedded). OK.
          evidence: FIRST_BOOT_STARTED FOREVER {
            entry_reason: FRESH_INSTALL,
            host_hostname: thinkpad-x1,
            media_signature_id: <hex>,
            started_at: 2026-05-09T10:00:00Z
          }

T+00.4s   STAGE_DISK_PARTITIONED
          GPT layout written: / (32 GB), /aios (440 GB), /recovery (4 GB), swap (32 GB).
          evidence: FIRST_BOOT_STAGE_COMPLETED STANDARD_24M {
            stage: STAGE_DISK_PARTITIONED, ..., attempt_count: 1
          }

T+02.1s   STAGE_KERNEL_INSTALLED
          /boot/vmlinuz-generic + initramfs installed; signed by AIOS root.
          (No dedicated kernel; S9.3 will run post-first-boot.)

T+04.7s   STAGE_AIOS_FS_INITIALIZED
          AIOS-FS root object created; namespace catalog seeded with constitutional
          system paths.

T+06.8s   STAGE_VAULT_ROOT_GENERATED
          Ed25519 keypair generated. Operator chooses TPM seal.
          Private half sealed under TPM (PCR 0,2,4,7 quoted at seal time).
          Public half written to /aios/system/vault/root.pub.
          evidence: VAULT_ROOT_KEY_GENERATED FOREVER {
            seal_kind: TPM,
            tpm_pcrs: [0, 2, 4, 7],
            pubkey_hash: <hex>,
            generated_at: 2026-05-09T10:00:06.8Z
          }

T+07.0s   STAGE_INVARIANT_BUNDLE_LOADED
          invbundle_<hex> verified against AIOS root pubkey; INV-001..INV-024 active.

T+07.5s   STAGE_POLICY_BUNDLE_LOADED
          Initial policy bundle compiled + loaded; default-deny + INV-008/012/013/018
          hard-denies present. The bundle's `RecoveryRequiredForSystemMutation`
          hard-deny rule (S2.3 §26.2.2 Wave 9 update) admits subjects with
          `is_first_boot = true` in addition to `is_recovery_mode = true`.
          Without this, the next stage's identity-bundle write would hard-deny.

T+07.9s   STAGE_IDENTITY_BUNDLE_LOADED
          idbundle_<hex> with required _system service subjects active.

T+08.0s   --- operator-interactive prompts begin ---
T+08.0s   STAGE_RECOVERY_OPERATOR_REGISTRATION
          Installer prompts: "register a recovery credential."
          Operator inserts Yubikey; taps to confirm challenge.
          Operator also registers a passphrase (entropy ~80 bits).
          Credential kind = BOTH; written to /aios/system/recovery/operators/operator-1.
          evidence: RECOVERY_OPERATOR_REGISTERED FOREVER {
            operator_canonical_id: _system:local:operator-1,
            credential_kind: BOTH,
            hardware_key_serial_hash: <hex>,
            registered_at: 2026-05-09T10:00:45Z
          }

T+45.0s   STAGE_AI_PROVIDER_CONFIGURATION
          Installer prompts AI provider mode.
          Operator chooses LOCAL_ONLY (privacy-first install).
          Installer prompts firewall posture.
          Operator chooses LAN_LOCAL_DEFAULT (wants Web UI from phone on same Wi-Fi).
          evidence: AI_PROVIDER_MODE_SET FOREVER {
            mode: LOCAL_ONLY, ...
          }
          evidence: INITIAL_FIREWALL_POSTURE_SET FOREVER {
            posture: LAN_LOCAL_DEFAULT,
            derived_lan_ranges: ["192.168.1.0/24"],
            ...
          }

T+90.0s   STAGE_FIRST_GROUP_REGISTRATION
          Operator names first group "family"; tier PERSONAL; can_have_ai_agents=true.
          evidence: FIRST_GROUP_REGISTERED FOREVER {
            group_id: family, group_tier: PERSONAL, ...
          }

T+110.0s  STAGE_FIRST_USER_REGISTRATION
          Operator registers as family:alice; password + Yubikey enrolled
          (same hardware key as recovery operator, different challenge).
          evidence: FIRST_USER_REGISTERED FOREVER {
            subject_canonical_id: family:alice,
            enrolled_credential_kinds: [PASSWORD, HARDWARE_KEY],
            is_first_group_admin: true, ...
          }

T+115.0s  STAGE_RUNTIME_SERVICES_STARTED
          L3 SGR up; L4 vault + identity + policy in normal mode; L8 network up
          with LAN_LOCAL_DEFAULT firewall; L9 evidence log producing.
          L5 cognitive services start (LOCAL_ONLY mode; Ollama runtime engaged
          on the laptop's GPU).

T+125.0s  STAGE_FIRST_BOOT_COMPLETE
          Marker computed, signed by vault root, atomically written.
          GRUB rewritten to S9.1 entries 0–3; first-boot entry removed.
          evidence: FIRST_BOOT_COMPLETE FOREVER {
            state_hash: <blake3_hex>,
            invariant_bundle_id: invbundle_<hex>,
            policy_bundle_id: polbundle_<hex>,
            identity_bundle_id: idbundle_<hex>,
            vault_root_pubkey_hash: <hex>,
            ai_provider_mode: LOCAL_ONLY,
            first_group_id: family,
            first_user_canonical_id: family:alice,
            initial_firewall_posture: LAN_LOCAL_DEFAULT,
            recovery_credential_kind: BOTH,
            marker_signature: <ed25519_sig>,
            completed_at: 2026-05-09T10:02:05Z
          }

T+125.0s  systemctl reboot. Next boot is GRUB entry 0 (NORMAL).
```

### §13.2 Example 2 — Server install without TPM, hardware-key seal

```text
Hardware: bare-metal server; no TPM; 4 TB SAS RAID; YubiKey 5 NFC + a backup Yubikey
          for the operator.
Media:    Signed AIOS installation media on a USB drive.

The flow is identical to §13.1 through STAGE_AIOS_FS_INITIALIZED, with one difference
at STAGE_VAULT_ROOT_GENERATED:

T+06.8s   STAGE_VAULT_ROOT_GENERATED
          Installer detects no TPM 2.0 hardware.
          Installer prompts: "no TPM detected. The vault root key will be sealed under
          a hardware-key-protected envelope. Insert your hardware key."
          Operator inserts the primary Yubikey; taps to confirm.
          Vault root keypair is generated; private half is encrypted under a key
          derived from the Yubikey's hmac-secret extension; ciphertext is written to
          /aios/system/vault/root.sealed (the hardware-key-protected envelope).
          Public half is written to /aios/system/vault/root.pub.
          Operator records explicit consent (typed action signed by the Yubikey)
          acknowledging the no-TPM posture.
          evidence: VAULT_ROOT_KEY_GENERATED FOREVER {
            seal_kind: HARDWARE_KEY_FILE,
            hardware_key_serial_hash: <hex>,
            pubkey_hash: <hex>,
            generated_at: 2026-05-09T11:00:06.8Z
          }

The remaining stages proceed as in §13.1. The operator chooses AIProviderMode =
HYBRID (the server has GPUs and the operator wants both local for SECRET_BEARING
workloads and external for PUBLIC research workloads); InitialFirewallPosture =
LAN_LOCAL_DEFAULT; first group "homelab"; first user homelab:admin.

The marker is signed by the vault root key (which requires the Yubikey to unlock at
boot; the operator must present the Yubikey at every host boot to make the host
fully operational — a constitutional consequence of the no-TPM posture they accepted).
```

### §13.3 Example 3 — Mid-stage power loss, idempotent resume

```text
Setup: a fresh-install run on the laptop from §13.1.
Time:  the install reaches STAGE_VAULT_ROOT_GENERATED, generates the keypair,
       seals the private half to TPM, and is in the middle of writing
       /aios/system/vault/root.pub when power is lost (the operator's cat unplugs
       the laptop).

Pre-loss state on disk:
  /aios/system/vault/root.pub                   (partial; not yet committed)
  /aios/system/vault/root.sealed                (committed)
  /recovery/firstboot/state                     stage: STAGE_VAULT_ROOT_GENERATED
                                                attempt: in-progress

The operator powers the laptop back on.
Boot decision logic finds /aios/system/firstboot/marker absent → first-boot.
First-boot installer reads /recovery/firstboot/state → resume from
STAGE_VAULT_ROOT_GENERATED.

T+00.0s   Installer detects partial state at /aios/system/vault/root.pub
          (size mismatch with expected; pubkey_hash recomputation fails).
          Installer rolls back: deletes /aios/system/vault/root.pub partial,
          deletes /aios/system/vault/root.sealed (the TPM-sealed private half is
          regenerated on retry — vault root keys are not reused across attempts
          because the prior attempt's pubkey was never committed and any record of
          it is invalid).

T+02.0s   Installer re-runs STAGE_VAULT_ROOT_GENERATED:
          Generates a new Ed25519 keypair.
          Seals private half to TPM (new PCR quote; same PCR set).
          Atomically writes /aios/system/vault/root.pub.
          Atomically writes /aios/system/vault/root.sealed.
          evidence: VAULT_ROOT_KEY_GENERATED FOREVER {
            seal_kind: TPM,
            tpm_pcrs: [0, 2, 4, 7],
            pubkey_hash: <new_hex>,
            generated_at: 2026-05-09T10:30:02Z
          }
          evidence: FIRST_BOOT_STAGE_COMPLETED STANDARD_24M {
            stage: STAGE_VAULT_ROOT_GENERATED,
            attempt_count: 2,
            ...
          }

The installer continues from STAGE_INVARIANT_BUNDLE_LOADED and completes the
remaining stages as in §13.1.

The two evidence records — the lost attempt's stage record and the successful
attempt's stage record — make the resume visible in the audit log. The lost
attempt's pubkey_hash is recorded only in the retried VAULT_ROOT_KEY_GENERATED
record's diagnostic field (FOREVER); the prior pubkey is never used as a
signing key because no marker was committed against it.
```

## §14 Adversarial robustness

### §14.1 Installer media tampering

An attacker substitutes a tampered AIOS installation USB on the operator's desk. The operator boots from the USB.

`STAGE_INSTALLER_MEDIA_VERIFIED` runs. The initramfs verifies the USB's signature against the AIOS root public key embedded in the initramfs. The signature does not verify (the attacker did not have AIOS root's private key). The installer aborts with `MEDIA_SIGNATURE_INVALID`. **No disk write has occurred.** The host's prior state (whether it was unprovisioned or had a previous AIOS install) is intact. `FIRST_BOOT_FAILED` (FOREVER) is emitted to the recovery partition's evidence log if the partition exists; otherwise, the initramfs displays the failure on the local console and powers off after 60 seconds.

The attack fails at the source: media verification is the first stage and is blocking. Tampered media that does match an attacker-controlled signing key is rejected because the verification key is the AIOS root pubkey embedded at build time; an attacker cannot replace the embedded key without also replacing the initramfs, which is itself signed and verified by the firmware (Secure Boot or equivalent) when present, and by the operator's visual inspection of the boot media when not.

### §14.2 Operator skips recovery operator registration

An operator (perhaps under social-engineering pressure to "skip the security stuff for now") attempts to advance from `STAGE_RECOVERY_OPERATOR_REGISTRATION` without registering a credential.

The installer surface presents no "skip" button. The operator's only options are: register a credential (any of the three `RecoveryCredentialKind` values) or abort first-boot (`OPERATOR_ABORTED`). There is no third option. An operator who tries to bypass the prompt by power-cycling the host finds, on next boot, that the installer resumes at `STAGE_RECOVERY_OPERATOR_REGISTRATION` (idempotent resume; the prior incomplete attempt is rolled back to the prior committed stage) — the same prompt is presented again. The barrier cannot be skipped by power-cycling.

The first-boot FSM cannot reach `STAGE_FIRST_BOOT_COMPLETE` without a registered recovery operator credential. A host that has not registered one is constitutionally crippled (no recovery path), and the installer refuses to produce one.

### §14.3 Operator chooses an external AI provider with a malicious API key

An attacker has obtained a stolen API key for a legitimate provider (e.g., Anthropic) and asks the operator to register it.

`STAGE_AI_PROVIDER_CONFIGURATION` runs with `AIProviderMode = VAULT_BROKERED_EXTERNAL`. The installer accepts the provider id (Anthropic) and the API key. The installer verifies the provider's TLS cert chain against the bundled provider attestation; the chain matches (the cert is for the legitimate provider). The installer registers the API key in the vault broker as a `KEY_USE` capability bound to the host's first user.

The attack surface here is the API key itself, not the registration. The installer cannot detect that the key is stolen (no provider-side key-validity check is part of registration). The attack succeeds in registering the key. **However**, the constitutional layer prevents misuse: the key is in the vault broker, not in any L5 subject's address space; L5 subjects invoke the broker's `ProxyAICall` capability, which performs the API call without revealing the key (per INV-018). The attacker's residual capability is "the operator's host can make API calls billed to my stolen account" — a financial impact on the operator, but not a constitutional compromise of AIOS.

The defense delegation: API key validity is the provider's responsibility (the provider should detect and revoke stolen keys); the host's defense is to keep the key contained in the vault. AIOS does not promise to detect stolen keys at registration; it does promise the key never leaks from the vault.

### §14.4 Concurrent installer attempts on the same disk

Two installer instances are started against the same disk (e.g., the operator boots from USB, the install hangs, the operator boots from a different USB without powering off the disk).

`STAGE_DISK_PARTITIONED` performs an exclusive lock on the disk via the kernel's `O_EXCL` open + a TPM PCR check (PCR 0 records the boot media; if PCR 0 changed mid-install, the check fails). The second installer's lock attempt fails; the second installer aborts with `DISK_HARDWARE_FAILURE` (a generalisation of "the disk is in use by another process; treat as transient hardware failure"). The first installer continues.

A more sophisticated attack — two installers running on different USB ports, each with its own initramfs and signature — is rejected at the disk-lock layer: only one process can hold the exclusive lock; the second is blocked. The defense is at the kernel-FS level, not at the AIOS-FS level (which has not yet been initialised at this stage).

### §14.5 AI subject attempts reset-to-factory

A compromised L5 subject (`is_ai = true`) attempts to submit `recovery.firstboot.reset` against a provisioned host.

The action is dispatched via the L3 Capability Runtime to the Policy Kernel. The Policy Kernel evaluates:

1. Is the subject `is_ai = true`? Yes.
2. Is the action's target `/aios/system/firstboot/marker` a system path under `RecoveryMutableScope`? Yes (it is constitutionally a recovery-mutable path).
3. Hard-deny `AISystemAdminBlocked` (per INV-013) fires before any other rule.

The action is denied. `POLICY_DECISION` (with reason code `AISystemAdminBlocked`) is emitted; the action does not reach the recovery service. The constitutional barrier is intact.

A second test: a normal-mode HUMAN_USER subject submits `recovery.firstboot.reset`. The Policy Kernel evaluates:

1. Subject is `is_ai = false`. Pass.
2. Subject is `recovery_mode = false`. Hard-deny `RecoveryRequiredForSystemMutation` (per INV-012) fires.

Denied. The only path to `recovery.firstboot.reset` is via recovery boot, and the recovery service requires a co-signer + the explicit confirmation prompt (§11.2). The cascade of guards is: AI subjects rejected by INV-013; normal-mode human subjects rejected by INV-012; recovery-mode operators with no co-signer rejected by the recovery service; recovery-mode operators with co-signer who do not type the confirmation phrase rejected by the prompt itself.

### §14.6 Forged FIRST_BOOT_COMPLETE evidence

An attacker with write access to the evidence log (somehow) attempts to inject a `FIRST_BOOT_COMPLETE` record on a host that has not yet completed first-boot, hoping to trick the boot decision logic into treating the host as provisioned.

The evidence log is append-only with a hash chain (per S3.1). An injected record would either:

- Break the chain (the new record's `prev_hash` does not match the actual prior record's hash) — `VerifyChain` detects on next verification and emits `TAMPER_DETECTED`.
- Have a valid chain but a payload that does not match the `marker` file. The boot decision logic does not consume the evidence log directly; it reads `/aios/system/firstboot/marker` and verifies its signature. The injected evidence record does not affect the boot decision.

The marker is the authoritative witness; the evidence log is the audit witness. They are independent. The defense is not "evidence is tamper-proof" (it is via INV-005, but the marker is the boot-time check); the defense is the separation: the boot path verifies the marker; the audit path verifies the evidence chain; both must agree for the host to be in a consistent state, and neither one can be forged into the other.

### §14.7 Power loss during STAGE_FIRST_BOOT_COMPLETE marker write

Power is lost during the atomic marker write at `STAGE_FIRST_BOOT_COMPLETE`. The marker file's pointer-CAS-promote either committed or did not.

If committed: the next boot's S9.1 boot decision sees the marker, verifies the signature (signed before the write), and proceeds in normal mode. First-boot is complete.

If not committed: the next boot's S9.1 boot decision sees no marker. The first-boot installer resumes at `STAGE_FIRST_BOOT_COMPLETE`. The coordinator regenerates the state hash (input fields are all already committed by prior stages), re-signs, re-writes the marker. The retry is idempotent because all input fields are already present and the only operation is the atomic CAS-promote.

The concurrency safety here is the AIOS-FS pointer-CAS protocol (S1.3): the prior write either succeeded entirely or did not happen at all. There is no half-marker visible to a subsequent boot.

## §15 Cross-references

| Spec                             | Direction   | What this spec contributes / consumes                                                                                                                                                                                                                                             |
| -------------------------------- | ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| L0 INV-001                       | implementer | First-boot does not depend on L5 cognition. The installer is an L1 toolbox surface; L5 services are masked until `STAGE_RUNTIME_SERVICES_STARTED` (and not started at all in `DEFERRED` mode).                                                                                    |
| L0 INV-004                       | implementer | First-boot lays down the three constitutional roots `/`, `/root`, `/aios` per S9.1 §3.1. The recovery boundary is established by `STAGE_DISK_PARTITIONED` and the constitutional layout of `/aios/system/...`.                                                                    |
| L0 INV-005                       | consumer    | Every stage emits append-only evidence; idempotent retries produce additional records, never rewrite prior records. The marker write is signed; tampering is detectable.                                                                                                          |
| L0 INV-006                       | implementer | `InitialFirewallPosture` defaults to `LOOPBACK_ONLY`. Public-internet exposure is never enabled at first-boot; there is no enum value that expresses it.                                                                                                                          |
| L0 INV-008                       | implementer | The initial Policy Kernel bundle is default-deny (§3.2.7).                                                                                                                                                                                                                        |
| L0 INV-012                       | consumer    | `AIProviderMode` change post-first-boot is a recovery-mode operation. Reset-to-factory is recovery-mode-only.                                                                                                                                                                     |
| L0 INV-013                       | consumer    | AI-subject reset-to-factory attempts are hard-denied at the Policy Kernel before reaching this spec's reset flow.                                                                                                                                                                 |
| L0 INV-014                       | implementer | `STAGE_FIRST_BOOT_COMPLETE` emits a FOREVER record only after the marker is committed. Status `REAL` for first-boot per host requires the marker + the evidence chain.                                                                                                            |
| L0 INV-018                       | consumer    | The vault root key's private half is sealed; never directly readable. External AI provider keys are vault-brokered (§7.3); never visible to L5 subjects.                                                                                                                          |
| L0 INV-022                       | consumer    | The first-boot installer surface is `AIOS_SURFACE`-only (consistent with the recovery shell's posture per S9.1 §6.1).                                                                                                                                                             |
| S9.1 Recovery Boundary           | consumer    | First-boot uses S9.1's `_system` scope subject namespace and seeds the first recovery operator credential (§10) that S9.1 §6.2 references as "the first-boot installer seeds an initial recovery operator credential". `RESET_TO_FACTORY_INITIATED` is a recovery-mode operation. |
| S9.3 Dedicated Kernel Pipeline   | consumer    | First-boot installs and uses the generic kernel exclusively. The dedicated kernel pipeline (S9.3) runs post-first-boot; first-boot's `STAGE_KERNEL_INSTALLED` stops at the generic kernel.                                                                                        |
| S2.3 Policy Kernel               | consumer    | The initial bundle is loaded at `STAGE_POLICY_BUNDLE_LOADED`. Hard-denies for INV-008/012/013/018 are part of the constitutional minimum the bundle must contain.                                                                                                                 |
| S5.1 Identity Model              | consumer    | Subject creation at `STAGE_FIRST_USER_REGISTRATION` consumes S5.1 §3 `SubjectKind = HUMAN_USER` and the canonical id format (S5.1 §4.1). Group registration consumes S5.1 §5.                                                                                                     |
| S5.2 Vault Broker                | consumer    | Vault root key generation (§3.2.5) consumes the broker's master-key seal mechanism. External AI provider keys are registered via `KEY_REGISTER` and used via `ProxyAICall` (broker-side `KEY_USE` class).                                                                         |
| S4.1 Namespace Layout            | consumer    | First-boot writes to `/aios/system/policy/`, `/aios/system/identity/`, `/aios/system/vault/`, `/aios/system/recovery/`, `/aios/system/governance/`, `/aios/system/kernel/`, `/aios/system/boot/`, `/aios/system/firstboot/`. Path semantics are owned by S4.1.                    |
| S3.1 Evidence Log                | producer    | Eleven new record types (§12), most FOREVER retention. Twelve new reason codes queued. The marker file's existence is independent of the evidence log; both are constitutional witnesses.                                                                                         |
| S1.1 Capability Translator       | constraint  | `DEFERRED` AIProviderMode keeps the host in translator-only mode; S1.1 deterministic translation is the only path to typed actions until an LLM provider is configured.                                                                                                           |
| S1.2 Latency Router              | constraint  | `HYBRID` AIProviderMode produces a routing table that S1.2 consumes per request. The table is part of the FOREVER `AI_PROVIDER_MODE_SET` evidence.                                                                                                                                |
| L8 Network Policy                | constraint  | `InitialFirewallPosture` is the default L8 ingress/egress policy at first normal boot. LAN range derivation (§8.3) is a one-time first-boot operation; subsequent posture changes are L8 operations.                                                                              |
| L7.1 Surface + Composition Model | consumer    | The first-boot installer surface is `AIOS_SURFACE`-only (per L7.1 §6.3 / S9.1 §6.1).                                                                                                                                                                                              |

## §16 Acceptance criteria

- [ ] `FirstBootEntryReason` is a closed enum with exactly three values (`FRESH_INSTALL`, `RESET_TO_FACTORY`, `REIMAGE`).
- [ ] `FirstBootStage` is a closed enum with exactly fifteen values forming a linear FSM with forward-only transitions; `STAGE_FAILED_REQUIRES_RECOVERY` is the only non-linear state and is reachable from any prior stage.
- [ ] `AIProviderMode` is a closed enum with exactly four values (`LOCAL_ONLY`, `VAULT_BROKERED_EXTERNAL`, `HYBRID`, `DEFERRED`); there is no `EXTERNAL_RAW` value.
- [ ] `RecoveryCredentialKind` is a closed enum with exactly three values (`HARDWARE_KEY`, `PASSPHRASE`, `BOTH`); there is no `BIOMETRIC` or `REMOTE_KEY_SERVER` value in Rev.2.
- [ ] `FirstBootFailureReason` is a closed enum with exactly twelve values; there is no `UNKNOWN` value.
- [ ] `InitialFirewallPosture` is a closed enum with exactly three values (`LOOPBACK_ONLY`, `LAN_LOCAL_DEFAULT`, `AIRGAP`); there is no `PUBLIC` value.
- [ ] Every stage except `STAGE_FIRST_BOOT_COMPLETE` is idempotent on retry; the same stage's atomic operation produces the same observable result.
- [ ] `STAGE_FIRST_BOOT_COMPLETE` is the only non-idempotent stage; the marker file is written exactly once per first-boot session and its presence terminally commits the host.
- [ ] The marker is signed by the vault root key; signature failure on subsequent boots triggers recovery.
- [ ] At least one `RecoveryCredentialKind` MUST be registered before `STAGE_AI_PROVIDER_CONFIGURATION` can run; there is no skip path.
- [ ] At least one group MUST be created at `STAGE_FIRST_GROUP_REGISTRATION` and at least one HUMAN_USER MUST be created at `STAGE_FIRST_USER_REGISTRATION` before `STAGE_FIRST_BOOT_COMPLETE` can run.
- [ ] `LOOPBACK_ONLY` is the default `InitialFirewallPosture`; public-internet exposure is never enabled at first-boot.
- [ ] `AIRGAP` posture is mutually exclusive with `VAULT_BROKERED_EXTERNAL` and `HYBRID` AI provider modes; the installer rejects the combination.
- [ ] `AIProviderMode = VAULT_BROKERED_EXTERNAL` and `HYBRID` deliver the external API key into the vault broker; the key is never written to a file or visible to any L5 subject.
- [ ] `RESET_TO_FACTORY_INITIATED` is the only path back to first-boot; AI subjects are denied by INV-013, normal-mode subjects are denied by INV-012, recovery-mode operators without co-signer are denied by the recovery service.
- [ ] The eleven evidence record types (§12) are queued for S3.1 vocabulary with the listed retention classes.
- [ ] The twelve reason codes (§12) are queued for S3.1 reason-code vocabulary.
- [ ] The three worked examples (§13) trace through the FSM and produce the expected evidence record sequences.
- [ ] All seven adversarial scenarios (§14) fail closed.
- [ ] Cross-references in §15 are accurate against the cited sub-specs as of 2026-05-09.

## §17 Open deferrals

The following are deferred to other sub-specs or future revisions. Listed here so a reader who finishes this spec knows where the gaps are.

- **Biometric recovery credentials.** A `RecoveryCredentialKind = BIOMETRIC` value is deferred. Biometric authentication (fingerprint, face) introduces non-revocable secret material and template-extraction vectors that need a dedicated threat model. Hardware key + passphrase covers the Rev.2 use cases.
- **Remote recovery operators.** A `RecoveryCredentialKind = REMOTE_KEY_SERVER` value is deferred. Remote key escrow (operator's recovery credential held by a household-admin's phone or a corporate HSM) requires a federation protocol AIOS does not yet have.
- **Multi-disk install.** First-boot in Rev.2 targets a single disk. Multi-disk RAID layouts (mirror for `/aios`, RAID6 for data) are deferred to a post-first-boot operation.
- **Disk encryption choice during first-boot.** The constitutional layout in §3.2.2 is plaintext partitions; disk encryption (LUKS, dm-crypt, ZFS native) is selected post-first-boot via a recovery-mode operation. Future work may move encryption choice into first-boot itself.
- **Air-gapped install with no AI provider attestation.** `AIProviderMode = LOCAL_ONLY` does not require an external attestation; `AIProviderMode = VAULT_BROKERED_EXTERNAL` requires the bundled provider attestation chain. An offline install with a custom local model that has no attestation is `LOCAL_ONLY` from the installer's perspective; finer-grained local-model attestation is deferred.
- **Federated first-boot (joining an existing AIOS fleet).** A new host joining an existing AIOS fleet (e.g., a household with multiple devices already provisioned) currently runs first-boot independently and is post-first-boot enrolled into the fleet via L4 federation. A direct "join fleet at first-boot" path is deferred.
- **Concurrent first-boot on multi-host installers.** Running first-boot on multiple hosts simultaneously from the same install media is supported; each host produces its own marker. Coordinated multi-host install (e.g., installing five identical kiosks where the operator wants identical configurations) is a post-first-boot operation in Rev.2.
- **First-boot rehearsal mode.** A "rehearse first-boot without committing" mode that runs all stages in a sandbox would help operators validate their choices before committing. Deferred. The current contract is "abort or commit"; abort is the dry-run.
- **`HEAVY_AUTH_FALLBACK_USED` evidence record.** When `RecoveryCredentialKind = PASSPHRASE` is used on subsequent recovery, S9.1 §6.2 emits this record. The vocabulary is queued under S9.1 / L4 authentication evidence, not added in this spec.

## §18 Status & evidence grade

Status: REAL
Evidence: E1 — file exists; structural contract complete; closed enums declared; FSM defined with allowed-transition table; idempotency contract specified per stage; AI provider configuration discipline specified; initial firewall posture discipline specified; first-group + first-user discipline specified; recovery operator registration discipline specified; terminal-commit and reset-to-factory contracts specified; eleven evidence record types queued; three worked examples present; cross-references resolved against existing spec files; seven adversarial scenarios enumerated; acceptance criteria mechanically checkable against this file.

Promotion to E2 requires: a build/typecheck artefact validating that the closed enums (`FirstBootEntryReason`, `FirstBootStage`, `AIProviderMode`, `RecoveryCredentialKind`, `FirstBootFailureReason`, `InitialFirewallPosture`) compile cleanly in the `aios.firstboot.v1alpha1` schema package with the rest of the rev.2 schema bundle, and that the marker file's schema (`aios.firstboot.v1alpha1.Marker`) is consistent with the AIOS-FS object schema.

Promotion to E3 requires: unit-level tests of the per-stage idempotency contract (§5), the AI provider configuration paths (§7), the initial firewall posture paths (§8), the first-group + first-user discipline (§9), the recovery operator registration barrier (§10), the terminal-commit + reset-to-factory chain (§11), and the seven adversarial scenarios (§14). Each test produces a recoverable artefact (a recorded evidence sequence; a captured marker file; a captured failure record).

Promotion to E4 requires: an end-to-end first-boot rehearsal exercising the three worked examples (§13) on a real AIOS host (or a faithful VM with the same boot path), with the FOREVER-retained evidence record sequences captured and replayed against `verify-chain`. The rehearsal MUST include a power-loss test that interrupts the install at three different stages and validates idempotent resume.

Promotion to E5 requires: an operational first-boot on a production AIOS host whose subsequent normal-mode operation runs uninterrupted for at least 30 days, with the resulting evidence visible in the post-first-boot operational record and the standard S9.1 boot path observed across multiple normal-mode reboots. Reset-to-factory MUST be exercised at least once to validate the round-trip.
