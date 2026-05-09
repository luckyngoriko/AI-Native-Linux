# Firmware Trust + Signed Update Paths (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists, structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| Phase tag      | S8.5                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| Layer          | L8 Network, Hardware, Devices                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| Schema package | `aios.firmware.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| Consumes       | L0 INV-002 (AI proposes never executes), INV-004 (recovery boundary preserved), INV-005 (evidence append-only), INV-008 (default-deny in policy), INV-013 (AI cannot perform system admin), INV-014 (no proof, no completion), INV-018 (vault no raw secret leak); S8.3 Hardware Graph (referenced abstractly via `DeviceClass` and `firmware_version`; this spec does not freeze the hardware-graph wire shape); S9.1 Recovery Boundary (`RecoveryStage`, `RecoveryMutableScope`, `RecoveryMode = RECOVERY` for non-overridable scopes); S9.2 First-Boot Flow (firmware blob set is consumed at first-boot HARDWARE_FIT prerequisites); S9.3 Dedicated Kernel Pipeline (`firmware_blob_set` is a build input — every firmware blob committed by this spec must be retrievable for kernel rebuild); S11.1 Repository Model (firmware bundles are delivered as a `PackageKind` extension; `PackageVerificationResult` is mirrored here); S0.1 Action Envelope (typed `firmware.update.*` actions follow the standard envelope FSM); S5.3 Approval Mechanics (`ApprovalStrength`, `EXACT_ACTION` binding); S5.4 Emergency Override (`NonOverridableClass`, `SCOPE_TOO_BROAD`); S3.1 Evidence Log (`RecordType` vocabulary, retention classes `STANDARD_24M`/`EXTENDED_60M`/`FOREVER`); S2.3 Policy Kernel hard-deny vocabulary; S10.1 Capability Runtime (action lifecycle); S5.2 Vault Broker (`KEY_VERIFY`); S5.1 Identity Model (`_system:service:firmware-update` subject scope, `is_ai = false`) |
| Produces       | closed `FirmwareUpdateClass` (5), closed `FirmwareScope` (8), closed `FirmwareUpdateState` FSM (8), closed `FirmwareTrustResult` (8), closed `FirmwareApplyStrategy` (3), closed `FirmwareDeferReason` (5); the seven-stage update flow with strict ordering; the per-scope rollback strategy table; the AIOS publisher-trust binding for vendor-signed proprietary firmware (cross-cite S11.1 publisher root signing); the operator-local-signed firmware path with FOREVER evidence; the unsigned-firmware constitutional refusal path; the recovery-mode emergency override boundary; twelve evidence record types queued for S3.1; bounded-cardinality telemetry; three worked examples (LVFS-signed Intel CPU microcode update, vendor-signed GPU firmware via AIOS publisher path, operator-local-signed firmware for legacy device)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |

## §1 Purpose

Firmware updates are a constitutional surface. A firmware blob is code that runs **below** the kernel — below the L1 substrate that INV-001 anchors the recovery floor on, below the L8.2 GPU partitioning that INV-024 gates compute on, below the TPM measurements that INV-018 is verified against. A compromised firmware update silently subverts every layer above it, including the layers that detect compromise. The blast radius is total: a malicious CPU microcode can speculatively leak any address; a malicious BIOS/UEFI image can replace bootloaders, alter PCR values, and forge measured-boot quotes; a malicious GPU firmware can cross the L8.2 VkDevice partition boundary and read another group's framebuffers; a malicious network adapter firmware can exfiltrate traffic before the L8 network policy ever sees it. There is no application-layer mitigation for any of these. The only mechanism is **trust at fetch time, signed at sign time, applied only with explicit policy consent, evidenced FOREVER on every consequential outcome**.

This sub-spec defines the closed vocabulary by which AIOS classifies firmware update paths, the strict-ordered apply pipeline, the trust binding to the L10 distribution publisher root, the FOREVER-retained operator-local-signed path, the constitutional refusal of unsigned firmware, and the adversarial robustness contract that binds version downgrades, vendor key compromise, mirror tampering, supply-chain attacks via firmware update channels, mid-action firmware updates, and evil-maid firmware-only swaps. Five constitutional risks define the threat model, each addressed by a named mechanism in this contract:

1. **Vendor signing key compromise** — a legitimate vendor's key is exfiltrated. Addressed by a two-path verification model (LVFS-preferred path with Red Hat-signed metadata, vendor-signed proprietary path with AIOS publisher root co-signing) plus the L10 deplatform discipline (cite S11.1 §3.1 `DEPLATFORMED`).
2. **Firmware version downgrade** — adversary replays an older signed blob with a known exploit. Addressed by a per-`FirmwareScope` monotonic version counter and `DOWNGRADE_BLOCKED` FOREVER evidence at fetch time.
3. **Mirror tampering of LVFS package** — legitimate signature, tampered transport. Addressed by host-side signature verification at fetch (LVFS metadata verifies the firmware-blob hash; vendor-signed proprietary path verifies the bundle hash).
4. **Supply-chain attack via firmware update channel** — both the upstream vendor and the AIOS publisher root must witness. Addressed by the AIOS publisher trust binding (S11.1 publisher root signs the vendor's firmware bundle metadata before any vendor-signed proprietary blob is admitted).
5. **Evil-maid firmware-only swap** — a physical attacker reflashes firmware while the host is powered off. Addressed by the L8.3 hardware graph drift detection (firmware version observed at every boot is compared to the recorded last-applied version; mismatch triggers `FIRMWARE_TAMPER_DETECTED` FOREVER and forces recovery entry).

The constitutional refusal of unsigned firmware (§3.4 `UNSIGNED_BLACKLISTED`) is the simplest and most important rule in this spec. There is no general-purpose path by which AIOS applies a firmware blob whose signature does not chain to LVFS, to a vendor with an active AIOS publisher root binding, or to the operator's own hardware key. The only escape is the recovery-mode emergency override (S5.4) for a strictly bounded subset of scopes — and that override carries FOREVER evidence and an explicit operator disclosure of the constitutional risk being accepted.

The mechanism in this spec is structural, not stylistic. It rests on six tightly coupled elements (§3 through §10). Each element is necessary; if any single element is removed, the mechanism becomes stupid regardless of the time available, the team size, or the hardware budget. The criterion is correctness of structure, not effort: "we will harden this in a later phase" is not a property of a smart firmware-update pipeline — a smart pipeline either has the six elements or it does not. The six elements are: (1) closed-vocabulary classification of every fetched firmware blob (§3.1 `FirmwareUpdateClass`); (2) closed-vocabulary scope taxonomy with per-scope rollback semantics (§3.2 `FirmwareScope` + §6); (3) a strict-ordered seven-stage pipeline that fails closed at every step (§4); (4) a per-device monotonic version counter that blocks replay-downgrades (§4.2 + §10.2); (5) AI-authorship refusal binding INV-002 to the firmware axis (§8); (6) FOREVER-retained evidence of every consequential outcome including refusals, downgrades, deplatform events, rollbacks, and operator-local-signed installs (§13). The boot-time hardware-graph drift detection (§10.7) is the seventh element treated as a continuous-time check rather than a pipeline stage, and is jointly owned with S8.3.

## §2 Scope

This spec **defines**:

1. The closed `FirmwareUpdateClass` enum (§3.1) — five classes covering LVFS, vendor-signed proprietary, signed distribution bundles, operator-local-signed, and unsigned-blacklisted.
2. The closed `FirmwareScope` enum (§3.2) — eight scope classes covering CPU microcode, GPU, disk, network adapter, TPM, BIOS/UEFI, embedded controllers, and the catch-all device-firmware class.
3. The closed `FirmwareUpdateState` FSM (§3.3) — eight states covering DRAFT → VALIDATING → APPROVED → STAGED → APPLIED → ROLLED_BACK / FAILED / ABANDONED.
4. The closed `FirmwareTrustResult` enum (§3.4) — eight outcomes mirroring the S11.1 `PackageVerificationResult` taxonomy.
5. The closed `FirmwareApplyStrategy` enum (§3.5) — three strategies covering A/B promote, atomic in-driver replace, and offline reflash.
6. The closed `FirmwareDeferReason` enum (§3.6) — five reasons covering safe-state contention.
7. The seven-stage update flow (§4) — STAGE_FETCH → STAGE_VERIFY → STAGE_APPROVE → STAGE_STAGE → STAGE_APPLY → STAGE_VERIFY_POST → STAGE_COMMIT_OR_ROLLBACK.
8. The trust-binding model (§5) — LVFS preferred path, vendor-signed proprietary path with AIOS publisher root co-signing, signed distribution bundle path, operator-local-signed path, the unsigned refusal path.
9. The per-`FirmwareScope` rollback strategy table (§6) — A/B for BIOS/UEFI when supported by the platform; in-driver atomic for GPU; impossible mid-session for CPU microcode (with explicit semantics for what "rollback" means there); offline reflash for embedded controllers.
10. The safe-state precondition (§7) — firmware updates are deferred while typed actions are in flight, while the system is in recovery, while another firmware update is staged, and while a sensitive workload (model training, vault unwrap, recovery rehearsal) is running.
11. The AI authorship rule (§8) — AI subjects (`is_ai = true`) cannot author `firmware.update.request` actions under any circumstance; the action is hard-denied at envelope validation.
12. The recovery-mode emergency override boundary (§9) — `UNSIGNED_BLACKLISTED` updates are constitutionally refused; the only escape is recovery-mode emergency override (S5.4) for a small subset of scopes; the override is FOREVER-recorded with explicit operator disclosure, and most scopes are denied with `SCOPE_TOO_BROAD` per S5.4 §10.
13. The adversarial robustness section (§10) — vendor key compromise → deplatform via S11.1; downgrade → `DOWNGRADE_BLOCKED` FOREVER; tampered LVFS package → signature failure at fetch; supply-chain attack via firmware update channel → cross-witness check (LVFS or AIOS publisher root); concurrent firmware update + sensitive workload → DEFER until safe-state; evil-maid → S8.3 hardware-graph-drift detection; UEFI variable manipulation → measured-boot integration via S9.3 element 4 IMA + TPM.
14. Bounded-cardinality telemetry contract (§12).
15. Twelve evidence record types queued for S3.1 (§13).
16. Three worked examples (§14) — LVFS-signed Intel CPU microcode update, vendor-signed GPU firmware via AIOS publisher path, operator-local-signed firmware for a legacy device.

This spec **does not** define:

- The wire shape of `HardwareGraphSnapshot` or the per-device `firmware_version` field — owned by S8.3 (`SHELL`); referenced abstractly here.
- The LVFS protocol wire format or the upstream metadata envelope. Consumed as an opaque signed-by-Red-Hat metadata blob.
- The vendor's internal signing infrastructure. Consumed as an opaque Ed25519 signature whose public key is registered in the AIOS publisher catalog (cite S11.1 §3.1).
- Per-vendor firmware update workflow UX (the surface for "Update available from Lenovo") — owned by L7 renderer specs and the L10 marketplace (`02_marketplace.md`, `SHELL`).
- Cross-host firmware fleet management (one host's firmware update does not propagate). Deferred.
- Per-firmware-blob behavioral whitelisting (e.g., "this microcode update only fixes CVE-X"). Deferred.
- The dedicated kernel pipeline's consumption of the firmware blob set as a build input — owned by S9.3 §4.1; this spec ensures every committed firmware blob is retrievable for kernel rebuild.
- The L10 marketplace UX for browsing, ranking, and presenting firmware updates. Owned by L10 marketplace (`SHELL`).

This spec is the **contract surface** that the L8 hardware graph (S8.3, deferred) and the L1 dedicated kernel pipeline (S9.3) depend on for firmware-trust semantics. After this contract:

- S8.3 can record `firmware_version` per device with confidence that mutations to that field came through this spec's apply pipeline.
- S9.3's `firmware_blob_set` build input has a defined provenance and a defined set of admissible signatures.
- S9.2 first-boot can validate the initial firmware posture against this spec's `FirmwareTrustResult` taxonomy.
- S2.3 has a closed list of firmware-related hard-deny rules to enumerate.
- S5.4 has a closed mapping of firmware-update-class to override-eligibility.

## §3 Vocabulary (closed enums)

All vocabularies in this section are closed. Adding a value is a versioned spec change. The firmware update pipeline, the policy kernel hard-deny vocabulary, and the evidence log validators MUST reject values outside the enum at parse time. None of these enums admits an `OPEN` or `OTHER` value; firmware is a constitutional surface and ambiguity is forbidden.

### §3.1 `FirmwareUpdateClass`

Closed enum, five classes. Every firmware update fetched by the host has exactly one class; the class is derived deterministically from the signature chain at fetch time and is a constitutional input to the policy decision.

| Value                        | Semantics                                                                                                                                                                                                                                                                                      | AI authorship | Default approval strength    | Recovery-mode required                                                                         |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------- | ---------------------------- | ---------------------------------------------------------------------------------------------- |
| `VENDOR_SIGNED_LVFS`         | The preferred path. Update fetched from the Linux Vendor Firmware Service (LVFS), whose metadata is Red-Hat-signed and whose individual firmware-blob signatures chain to vendor keys curated by the LVFS team. The well-known-good path; ecosystem-grade trust without per-vendor onboarding. | Forbidden     | `STANDARD` (single approver) | No (normal-mode)                                                                               |
| `VENDOR_SIGNED_PROPRIETARY`  | Vendor's own signed firmware bundle, distributed outside LVFS. The vendor's signing key is registered in the AIOS publisher catalog (cite S11.1 §3.1 `VERIFIED` trust) and the bundle metadata is co-signed by the AIOS publisher root.                                                        | Forbidden     | `STRONG` (one human)         | No (normal-mode)                                                                               |
| `SIGNED_DISTRIBUTION_BUNDLE` | Firmware bundled in a signed `KERNEL_CANDIDATE` package per S11.1 §3.4 (e.g., the AIOS-curated firmware blob set for the dedicated kernel build). The signature chain ends at AIOS root.                                                                                                       | Forbidden     | `STRONG` (recovery operator) | Yes (the parent kernel-candidate install is RECOVERY-only per S11.1 §3.2 `AIOS_RECOVERY_REPO`) |
| `OPERATOR_LOCAL_SIGNED`      | The operator has signed a firmware blob with their own hardware key (the same key that anchors `RecoveryCredentialKind = HARDWARE_KEY` per S9.2). FOREVER evidence + clear disclosure dialog mandatory; constrained to scopes the constitution permits an operator to flash unilaterally.      | Forbidden     | `STRONG_SOLO` (recovery)     | Yes (recovery-mode required for most scopes)                                                   |
| `UNSIGNED_BLACKLISTED`       | No valid signature chain. The firmware blob is constitutionally refused at the verification step. The only escape is a recovery-mode emergency override (S5.4) for a strictly bounded subset of scopes; most scopes are denied with `SCOPE_TOO_BROAD`.                                         | Forbidden     | (refused)                    | (refused)                                                                                      |

The class is recorded in every `FirmwareUpdateState` transition, in every emitted evidence record, and in the per-host telemetry counter. A class downgrade — for example, an attempt to relabel a `VENDOR_SIGNED_PROPRIETARY` blob as `VENDOR_SIGNED_LVFS` to bypass the `STRONG` approval — is detected at the verification step (§4 stage 2): the signature chain is recomputed deterministically, and any mismatch between the claimed and computed class emits `FIRMWARE_VERIFICATION_FAILED` (EXTENDED_60M).

### §3.2 `FirmwareScope`

Closed enum, eight scope classes. Each scope has distinct rollback semantics, distinct safe-state preconditions, and a distinct mapping into `NonOverridableClass` membership. The pipeline rejects an update whose declared scope is not in this enum at parse time.

| Value                      | Semantics                                                                                                                                                           | Rollback strategy (cite §6)           | Override-eligible? (cite §9)                                          |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- | --------------------------------------------------------------------- |
| `CPU_MICROCODE`            | Microcode update for x86_64 / aarch64 CPUs; loaded from `/lib/firmware/<vendor>/<sku>` by the kernel during boot or via `wrmsr`-based late-load.                    | Impossible mid-session (§6.1)         | Override forbidden (constitutional refusal; cite §9.3)                |
| `GPU_FIRMWARE`             | GPU-resident firmware (Intel GuC/HuC, AMD VCN/VCE, Nvidia GSP). Loaded by the GPU driver at kernel boot or driver re-init.                                          | Driver-mediated atomic replace (§6.2) | Override conditional (`SCOPE_TOO_BROAD` for late-load; recovery-only) |
| `DISK_FIRMWARE`            | Storage controller / SSD / HDD firmware. Often vendor-tool-driven; flashed via NVMe admin commands or vendor-specific protocols.                                    | Vendor A/B slot when supported (§6.3) | Override conditional (recovery-only; `SCOPE_TOO_BROAD` denies most)   |
| `NETWORK_ADAPTER_FIRMWARE` | Network adapter firmware (NIC microcontroller, Wi-Fi radio, Bluetooth radio, cellular modem). Loaded by the driver at probe time.                                   | Driver-mediated atomic replace (§6.2) | Override conditional (recovery-only; `SCOPE_TOO_BROAD` denies most)   |
| `TPM_FIRMWARE`             | TPM 2.0 firmware. Updates re-quote PCR values and may invalidate vault seals (per S9.2 §3.2 TPM seal discipline).                                                   | Vendor A/B slot when supported (§6.3) | Override forbidden (constitutional refusal; cite §9.3)                |
| `BIOS_UEFI`                | The system firmware: UEFI BIOS, system board firmware, ME/PSP firmware on x86_64. The most consequential scope; rollback semantics depend on platform.              | Vendor A/B BIOS slot (§6.3)           | Override forbidden (constitutional refusal; cite §9.3)                |
| `EMBEDDED_CONTROLLER`      | Embedded controllers (laptop EC, USB hub controllers, Thunderbolt routers, baseboard management controllers).                                                       | Offline reflash (§6.4)                | Override conditional (recovery-only; `SCOPE_TOO_BROAD` denies most)   |
| `OTHER_DEVICE_FIRMWARE`    | The closed catch-all for everything else: printers, scanners, USB peripherals, sensors, HID devices. Firmware here cannot affect the L1 substrate or L0 invariants. | Driver-mediated atomic replace (§6.2) | Override conditional (recovery-only; `STANDARD` strength sufficient)  |

A firmware blob declaring a scope that does not match the device's `DeviceClass` (per S8.3) at the apply step is rejected with `BUNDLE_TAMPERED` (cite S11.1 §3.7) and emits `FIRMWARE_VERIFICATION_FAILED` EXTENDED_60M.

### §3.3 `FirmwareUpdateState`

Closed FSM, eight states. The update pipeline (§4) walks this FSM strictly forward; back-transitions are forbidden except `STAGED → ABANDONED` (operator-cancelled before apply) and `APPLIED → ROLLED_BACK` (post-apply verification failure).

| Value         | Semantics                                                                                                                                                     |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `DRAFT`       | Update has been requested by an operator-authored typed action; not yet validated.                                                                            |
| `VALIDATING`  | Signature chain, hash, scope-vs-device match, version-monotonicity, and class derivation in progress.                                                         |
| `APPROVED`    | Policy decision returned `request_approval`; approval bound and consumed via `EXACT_ACTION` (cite S5.3).                                                      |
| `STAGED`      | Bytes have been written to the staging slot (vendor A/B slot, kernel firmware loader cache, or device-specific staging area).                                 |
| `APPLIED`     | The flash / load has been executed; the device's reported firmware version has changed (or, for late-loaded blobs, the kernel firmware loader confirms load). |
| `ROLLED_BACK` | Post-apply verification failed; the prior firmware version has been restored (where rollback is possible per §6).                                             |
| `FAILED`      | Terminal: pipeline aborted before `APPLIED`. Reason recorded from `FirmwareTrustResult` (§3.4) or pipeline-step failure.                                      |
| `ABANDONED`   | Terminal: operator cancelled (or system aborted because of `FirmwareDeferReason` exhaustion). The staged bytes are discarded.                                 |

Allowed forward transitions:

```text
DRAFT ─▶ VALIDATING ─┬─▶ FAILED                 (signature / chain / scope / version fail)
                     ├─▶ APPROVED ─▶ STAGED ─┬─▶ APPLIED ─┬─▶ verify-post-OK ─▶ (terminal: APPLIED stable)
                     │                       │            └─▶ verify-post-FAIL ─▶ ROLLED_BACK
                     │                       └─▶ ABANDONED               (operator cancel mid-stage)
                     │                                                   (or DeferReason exhausted)
                     │
                     └─▶ FAILED                            (approval denied, expired, or DEFER exhausted)
```

Terminal states: `APPLIED` (until the next firmware update on the same device), `ROLLED_BACK`, `FAILED`, `ABANDONED`. A transition into any terminal state emits the corresponding evidence record (§13).

### §3.4 `FirmwareTrustResult`

Closed enum, eight outcomes. Every step in the verification stage (§4 stage 2) returns one of these. The naming and shape mirror S11.1 §3.7 `PackageVerificationResult` so that operators and downstream tooling can reuse the L10 mental model. Mirror correspondences are noted; the firmware-specific names exist because the failure modes differ at the cryptographic level (LVFS metadata is Red-Hat-signed, not AIOS-root-signed; vendor proprietary firmware is co-signed, not chain-signed).

| Value                       | Mirrors (S11.1)                          | Trigger                                                                                                                                                                    |
| --------------------------- | ---------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `VERIFIED_LVFS`             | `VERIFIED_AIOS_ROOT`                     | LVFS metadata signature verified against Red-Hat signing key; per-blob signature verified against vendor key recorded in LVFS metadata.                                    |
| `VERIFIED_VENDOR_SIGNATURE` | `VERIFIED_PUBLISHER`                     | Vendor signature verified; vendor key registered in AIOS publisher catalog (cite S11.1 §3.1) at trust level `VERIFIED`; AIOS publisher root co-signed the bundle metadata. |
| `VERIFIED_DISTRIBUTION`     | `VERIFIED_AIOS_ROOT`                     | Firmware shipped inside a `KERNEL_CANDIDATE` package whose chain ends at AIOS root (cite S11.1 §3.4); chain depth ≤ 3.                                                     |
| `SIGNATURE_FAILED`          | `SIGNATURE_FAILED`                       | Ed25519 (or RSA-PSS for some legacy LVFS blobs) verify failed at any chain hop.                                                                                            |
| `VENDOR_DEPLATFORMED`       | `PUBLISHER_DEPLATFORMED`                 | Vendor's publisher-root entry in S11.1 publisher catalog is `DEPLATFORMED` at fetch time.                                                                                  |
| `HASH_MISMATCH`             | `HASH_MISMATCH`                          | `BLAKE3(content)` differs from manifest-asserted content hash; mirror tampering or corrupted transport.                                                                    |
| `DOWNGRADE_BLOCKED`         | (no S11.1 mirror)                        | The proposed firmware version is strictly less than the per-`FirmwareScope` per-device monotonic counter; replay or rollback attempt without explicit override.            |
| `UNSIGNED_REJECTED`         | (no S11.1 mirror; see `BUNDLE_TAMPERED`) | No valid signature chain; class derivation produced `UNSIGNED_BLACKLISTED`; refused unless recovery-mode emergency override.                                               |

The eighth result (`DOWNGRADE_BLOCKED`) and the ninth-position result (`UNSIGNED_REJECTED`) have no direct S11.1 analog because the L10 distribution layer's monotonic-version discipline applies at the package layer, not the per-device firmware layer; firmware versions are tracked per `(host, FirmwareScope, device_id)` tuple, not per package id. The firmware monotonicity counter is recorded in `/aios/system/firmware/<scope>/<device_id>/version_high_water` (writable only via this spec's apply step; reset only via S9.1 `RecoveryMutableScope = FIRMWARE_VERSION_COUNTER`, recovery-only).

### §3.5 `FirmwareApplyStrategy`

Closed enum, three strategies. Selected at the apply step (§4 stage 5) based on the device's declared `FirmwareScope` and the platform's reported A/B-slot capability.

| Value                      | Semantics                                                                                                                                                                                                                                                                                  |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `AB_VENDOR_SLOT`           | The platform exposes a vendor A/B slot (e.g., UEFI Capsule + RecoverySlot, or NVMe Firmware Slot Info). Update is staged into the inactive slot; commit promotes; rollback demotes. The canonical strategy for `BIOS_UEFI`, `TPM_FIRMWARE`, and many `DISK_FIRMWARE` paths.                |
| `IN_DRIVER_ATOMIC_REPLACE` | The driver swaps the firmware image in a single atomic operation (e.g., GPU reset + firmware reload, NIC reset + firmware reload). Rollback is the inverse atomic operation; the prior firmware blob must remain available in the kernel firmware loader cache.                            |
| `OFFLINE_REFLASH`          | The device must be powered down and reflashed via an external mechanism (e.g., embedded controller flashed during a recovery-mode boot via vendor utility). Rollback requires the operator's prior firmware backup; if absent, `ROLLED_BACK` is unreachable and the strategy fails closed. |

Strategy is recorded in the `FIRMWARE_APPLIED` evidence record. A device whose declared scope cannot be matched to a non-failing strategy at the apply step (e.g., a `BIOS_UEFI` device with no A/B slot and no offline reflash vendor utility) fails the pipeline at stage 5 with `FIRMWARE_APPLY_FAILED` (EXTENDED_60M); the operator is offered the recovery-mode override path (§9), which itself is denied for the constitutional scopes.

### §3.6 `FirmwareDeferReason`

Closed enum, five reasons. The safe-state precondition (§7) defers a `STAGED → APPLIED` transition while any of these conditions hold. The deferral is bounded by an `ApprovalTtl` ceiling (cite S5.3 §8); exhaustion of the TTL transitions the FSM to `ABANDONED`.

| Value                        | Trigger                                                                                                                                  |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `IN_FLIGHT_TYPED_ACTION`     | At least one S0.1 action envelope is in `executing` or `verifying` state on the host.                                                    |
| `RECOVERY_MODE_ACTIVE`       | The host is currently in `RecoveryMode = RECOVERY` (cite S9.1 §3); applying a non-recovery-class firmware is denied.                     |
| `CONCURRENT_FIRMWARE_STAGED` | Another firmware update for any `FirmwareScope` on the host is already in `STAGED` state; only one update applies at a time.             |
| `SENSITIVE_WORKLOAD_RUNNING` | A workload tagged `is_sensitive = true` (per S5.2 vault unwrap; per S9.3 `kernel.build` action; per L9 recovery rehearsal) is in flight. |
| `USER_OPT_OUT_WINDOW`        | The operator has set a "no firmware updates between hours X and Y" preference; the update is held until the window opens.                |

A deferred update emits `FIRMWARE_DEFERRED` (STANDARD_24M) on each defer-cycle transition; cumulative defers are bounded so the audit trail does not bloat (≤ 10 defer-cycles per update; the eleventh defer transitions to `ABANDONED` with `FIRMWARE_DEFER_EXHAUSTED` evidence). The eleven-cycle bound is normative.

## §4 The seven-stage update flow

Strictly ordered, fail-closed. Every step has a closed failure outcome; any step failing returns the FSM to `FAILED` (or `ABANDONED` for §4.4 deferrals beyond the cap) with the failure recorded; no step is "best-effort".

### §4.1 Stage 1 — STAGE_FETCH

The host fetches the firmware bundle from one of the four admissible sources: the LVFS endpoint (for `VENDOR_SIGNED_LVFS`); the vendor's signed-update endpoint (for `VENDOR_SIGNED_PROPRIETARY`); the local AIOS package store (for `SIGNED_DISTRIBUTION_BUNDLE`); the operator-provided file (for `OPERATOR_LOCAL_SIGNED`). The fetch is parameterised by:

- `firmware_request_id` — `"frm:" + ULID + 26-char base32`.
- `device_id` — opaque device identity from S8.3.
- `firmware_scope` — declared `FirmwareScope`.
- `claimed_class` — declared `FirmwareUpdateClass` (verified at stage 2).
- `expected_version` — the version the operator believes they are installing.

Failure outcomes:

- Network unreachable → `FETCH_FAILED` evidence (STANDARD_24M); FSM → `FAILED`.
- Mirror serves a resource whose content-hash does not match the LVFS metadata's expected hash → `FIRMWARE_TRUST_RESULT = HASH_MISMATCH`; FSM → `FAILED`; the mirror's per-host blacklist counter increments (mirroring S11.1 §10).

### §4.2 Stage 2 — STAGE_VERIFY

Signature verification is deterministic and class-derivation-driven. The pipeline computes the class from the signature chain, not from the operator's claim; a class downgrade is detected here.

Verification matrix (per claimed class):

| Claimed class                | Verifier                                                                                                                                                                                                                                                | Success result                 | Failure result(s)                                                                                                                         |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `VENDOR_SIGNED_LVFS`         | (a) LVFS metadata signature verifies against pinned Red-Hat signing key; (b) per-blob signature verifies against vendor key listed in LVFS metadata; (c) BLAKE3(blob) matches metadata.                                                                 | `VERIFIED_LVFS`                | `SIGNATURE_FAILED`, `HASH_MISMATCH`, `VENDOR_DEPLATFORMED` (if vendor's LVFS-side key is on AIOS deplatform list per S11.1)               |
| `VENDOR_SIGNED_PROPRIETARY`  | (a) Vendor signature verifies; (b) vendor key in S11.1 publisher catalog at `VERIFIED` trust; (c) AIOS publisher root co-signature on the bundle metadata verifies; (d) BLAKE3 matches.                                                                 | `VERIFIED_VENDOR_SIGNATURE`    | `SIGNATURE_FAILED`, `VENDOR_DEPLATFORMED`, `HASH_MISMATCH`, `TRUST_CHAIN_BROKEN` (if AIOS co-signature is missing or invalid)             |
| `SIGNED_DISTRIBUTION_BUNDLE` | (a) Parent `KERNEL_CANDIDATE` package's S11.1 install was successful; (b) the firmware's path within the package matches the manifest's declared firmware blob set; (c) BLAKE3 matches.                                                                 | `VERIFIED_DISTRIBUTION`        | `SIGNATURE_FAILED` (parent install failure), `HASH_MISMATCH`                                                                              |
| `OPERATOR_LOCAL_SIGNED`      | (a) Operator's hardware-key public half (per S9.2 `RecoveryCredentialKind = HARDWARE_KEY`) is enrolled; (b) operator-provided signature verifies; (c) BLAKE3 matches; (d) operator interactively confirms scope and version on a `STRONG_SOLO` channel. | (none — proceed to disclosure) | `SIGNATURE_FAILED`, `HASH_MISMATCH`, hardware-key not enrolled                                                                            |
| `UNSIGNED_BLACKLISTED`       | No verifier — the bundle has no admissible signature path.                                                                                                                                                                                              | (none — refuse)                | `UNSIGNED_REJECTED` always; FSM → `FAILED` unless recovery-mode emergency override (§9) lifts the refusal for an override-eligible scope. |

After signature verification, the pipeline runs the **monotonicity check**:

- Read the per-`(scope, device_id)` `version_high_water`.
- If the proposed version is strictly less than the high-water, return `DOWNGRADE_BLOCKED`; FOREVER `FIRMWARE_DOWNGRADE_BLOCKED` evidence; FSM → `FAILED`.
- If the proposed version equals the high-water, the operation is treated as a no-op refresh; the pipeline still walks the apply path (some platforms re-flash identically-versioned firmware to repair bit-rot) but emits `FIRMWARE_REFLASH_IDENTICAL` (STANDARD_24M) instead of `FIRMWARE_APPLIED`.

Then the **scope-vs-device check**:

- The hardware graph (S8.3) is queried for the device's `DeviceClass` and current `firmware_version`.
- If the declared `FirmwareScope` does not match the device's `DeviceClass`, return `BUNDLE_TAMPERED` (mirroring S11.1 §3.7); FSM → `FAILED`.
- If the device is reported absent from the hardware graph, return `DEVICE_NOT_PRESENT` evidence (STANDARD_24M); FSM → `FAILED`.

### §4.3 Stage 3 — STAGE_APPROVE

The firmware update action is dispatched as a typed action through S5.3 approval mechanics. The mapping from `FirmwareUpdateClass` to default approval strength is:

| Class                        | Default `ApprovalStrength`             | Default `ApprovalScope` | Default `ApprovalTtl`          | Channel preference                          |
| ---------------------------- | -------------------------------------- | ----------------------- | ------------------------------ | ------------------------------------------- |
| `VENDOR_SIGNED_LVFS`         | `STANDARD`                             | `EXACT_ACTION`          | `SHORT_5M`                     | `KDE_NATIVE_PROMPT` then `WEB_LOCAL_PROMPT` |
| `VENDOR_SIGNED_PROPRIETARY`  | `STRONG`                               | `EXACT_ACTION`          | `SHORT_5M`                     | `KDE_NATIVE_PROMPT` then `WEB_LOCAL_PROMPT` |
| `SIGNED_DISTRIBUTION_BUNDLE` | `STRONG_SOLO`                          | `EXACT_ACTION`          | `RECOVERY_BOUND` (see S5.3 §8) | `RECOVERY_CONSOLE` only                     |
| `OPERATOR_LOCAL_SIGNED`      | `STRONG_SOLO`                          | `EXACT_ACTION`          | `RECOVERY_BOUND`               | `RECOVERY_CONSOLE` only                     |
| `UNSIGNED_BLACKLISTED`       | (no approval — recovery-mode override) | (n/a)                   | (n/a)                          | (n/a)                                       |

A binding consumed for one `firmware_request_id` cannot be reused; replay produces `BindingAlreadyConsumed` per S5.3.

For `OPERATOR_LOCAL_SIGNED`, the approval channel additionally renders a **mandatory operator disclosure** before the binding is granted:

```text
You are about to install firmware that is signed only by your own hardware key.

  Device:        <device class + serial fragment>
  Scope:         <FirmwareScope>
  Version:       <semver / opaque vendor version>
  Disclosure:    The vendor (and LVFS) have NOT verified this update.
                 If this firmware is malicious, AIOS cannot detect it.
                 You alone are accountable for the consequences.

This action will be FOREVER-recorded with your operator id, the
hardware-key serial fragment, and the firmware blob hash.

  [Cancel]  [Confirm — I understand]
```

The disclosure copy is normative; renderers (per L7 visual language) bind to the same string.

### §4.4 Stage 4 — STAGE_STAGE

The verified bytes are written to the staging slot. The slot depends on `FirmwareApplyStrategy`:

- `AB_VENDOR_SLOT` — write to the vendor's inactive slot. For UEFI, this is the EFI System Partition's capsule path or the platform's CapsuleUpdate variable.
- `IN_DRIVER_ATOMIC_REPLACE` — write to `/lib/firmware/<vendor>/<sku>` and mark the new path active in the kernel firmware loader cache.
- `OFFLINE_REFLASH` — write to a recovery-mode-readable staging area on `/recovery/firmware/staging/` (outside `/aios`, per the recovery boundary in S9.1); the actual reflash happens on the next recovery boot.

Any write failure (disk full, permission denied, I/O error) → `FIRMWARE_STAGE_FAILED` (EXTENDED_60M); FSM → `FAILED`.

The safe-state precondition (§7) is evaluated before stage 5 dispatch; a `FirmwareDeferReason` defers the apply and emits `FIRMWARE_DEFERRED` (STANDARD_24M) — including specifically `BIOS_UEFI_UPDATE_DEFERRED` (EXTENDED_60M, see §13) for the BIOS_UEFI scope, since deferring a BIOS update has a higher operator-visibility need.

### §4.5 Stage 5 — STAGE_APPLY

The strategy-specific apply is invoked:

- `AB_VENDOR_SLOT` — invoke the platform's commit-and-reboot mechanism (UEFI CapsuleUpdate, NVMe Activate). The host typically reboots; the apply-success signal is the post-reboot stage 6 verifying the new version on the new slot.
- `IN_DRIVER_ATOMIC_REPLACE` — invoke the driver's reload path (e.g., `echo 1 > /sys/.../firmware_reload`); the driver brings down the device, loads the new firmware, brings the device back up. The apply-success signal is the device's reported firmware version.
- `OFFLINE_REFLASH` — schedule a recovery-mode entry; the reflash happens during the next recovery boot.

For `CPU_MICROCODE`, the apply is special: late-load microcode update is supported by the kernel only for certain microcode revisions, and many revisions require a reboot to take effect. The apply step records both the in-session apply outcome and a deferred "reboot-pending" signal; the post-verify step (stage 6) at next boot confirms the running revision.

Any apply failure → `FIRMWARE_APPLY_FAILED` (EXTENDED_60M); FSM → `ROLLED_BACK` if rollback is reachable (§6), else `FAILED` (with the rollback-impossible reason recorded).

### §4.6 Stage 6 — STAGE_VERIFY_POST

Post-apply verification reads the device's reported firmware version and compares against the proposed version. The check is strategy-specific:

- `AB_VENDOR_SLOT` — read the active-slot version after the post-apply reboot.
- `IN_DRIVER_ATOMIC_REPLACE` — read the device's reported version immediately after driver reload.
- `OFFLINE_REFLASH` — read the device's reported version after the next normal-mode boot.

If the reported version does not match the proposed version → `FIRMWARE_VERIFY_POST_FAILED`; FSM → `ROLLED_BACK` (where rollback is reachable per §6); FOREVER `FIRMWARE_ROLLBACK_PERFORMED` evidence.

For `BIOS_UEFI` and `TPM_FIRMWARE`, the post-verify also re-quotes the TPM PCR set (per S9.2 PCR 0,2,4,7) and compares against the AIOS-recorded expected PCRs after the firmware change. PCR drift outside the expected delta produces `FIRMWARE_TAMPER_DETECTED` FOREVER and forces recovery entry per S9.1 `RecoveryEntryReason.FIRMWARE_TAMPER_DETECTED`.

### §4.7 Stage 7 — STAGE_COMMIT_OR_ROLLBACK

The pipeline commits or rolls back atomically:

- **Commit on success:** the per-`(scope, device_id)` `version_high_water` is bumped to the new version; the `FirmwareUpdateState` transitions to `APPLIED`; FOREVER `FIRMWARE_APPLIED` evidence is emitted with `firmware_request_id`, `class`, `scope`, `device_id`, `prior_version`, `new_version`, `apply_strategy`, `approver_id` (or `operator_id` for `OPERATOR_LOCAL_SIGNED`), and `firmware_blob_hash`.
- **Rollback on failure:** the strategy-specific rollback (§6) is invoked. On rollback success, FSM → `ROLLED_BACK`; on rollback failure, FSM → `FAILED` with `rollback_impossible = true`. Either way, FOREVER `FIRMWARE_ROLLBACK_PERFORMED` evidence is emitted.

The `version_high_water` is monotonic: a successful rollback does **not** decrement the high-water; the operator must explicitly use the recovery-only `RecoveryMutableScope = FIRMWARE_VERSION_COUNTER` to lower it. This is constitutional: a runtime-decrementable counter would be a downgrade vector.

## §5 Trust binding model

### §5.1 LVFS preferred path

LVFS (Linux Vendor Firmware Service) is the canonical Linux firmware trust path. It is operated by the Linux Foundation with Red Hat anchoring the metadata signing. It is the well-known-good path that AIOS prefers because:

- Vendor onboarding to LVFS includes a curation review (the AIOS publisher catalog onboarding does not necessarily include this step; the two are complementary).
- Per-blob signatures chain to vendor keys curated by the LVFS team.
- The metadata is centrally signed; mirror tampering is detected at the metadata level.
- LVFS revocation is propagated through the metadata; a vendor whose key is compromised has a single-point reset.

AIOS pins the Red-Hat LVFS signing key at first-boot (per S9.2). The pin is recorded as `firmware.lvfs.signing_pubkey_hash` in the vault. Rotation of the LVFS signing key is a recovery-mode operation that requires a fresh AIOS-root-cosigned bundle update.

### §5.2 Vendor-signed proprietary path

Some firmware (e.g., proprietary GPU firmware, certain enterprise BMC firmware) is distributed outside LVFS. AIOS admits these through the L10 publisher trust binding (cite S11.1 §3.1):

- The vendor enrolls a signing key in the AIOS publisher catalog at trust level `VERIFIED`.
- The vendor's firmware bundle metadata is co-signed by the AIOS publisher root.
- The bundle's per-blob signatures verify against the vendor's signing key.
- The chain depth ≤ 3 (AIOS root → AIOS publisher root → vendor key → blob).

The double-signature requirement is the supply-chain defense: even if the vendor's key is exfiltrated, an attacker cannot ship a malicious firmware bundle without also forging the AIOS publisher root signature. The AIOS publisher root operates with HSM separation (deferred to L4 vault broker for HSM details); compromise of both keys simultaneously is the threat model floor.

### §5.3 Signed distribution bundle path

Firmware blobs shipped inside `KERNEL_CANDIDATE` packages (per S11.1 §3.4) inherit the parent package's trust chain; the firmware's S9.3 `firmware_blob_set` build input is provenance-traced to AIOS root. This path is recovery-only because `KERNEL_CANDIDATE` packages are recovery-only per S11.1 §3.2 `AIOS_RECOVERY_REPO`. Apply happens during the recovery-mode kernel candidate promotion (per S9.3 element 6 A/B promotion FSM).

### §5.4 Operator-local-signed path

The operator may install firmware they have personally signed with their hardware key (the same `RecoveryCredentialKind = HARDWARE_KEY` used for vault root seal per S9.2). The path exists because:

- Some legacy devices have no LVFS coverage and no AIOS publisher binding.
- Some devices (open hardware projects, custom FPGAs) ship community-built firmware.
- Operators must have an escape hatch from vendor abandonment.

The path is constitutionally constrained:

- Mandatory disclosure dialog at stage 3 (§4.3) text is normative.
- FOREVER `OPERATOR_LOCAL_FIRMWARE_INSTALLED` evidence with operator id, hardware-key serial fragment, firmware blob hash.
- Most scopes are denied at the policy step (cite §9.1) — the operator cannot self-flash `BIOS_UEFI` or `CPU_MICROCODE` or `TPM_FIRMWARE` even from recovery mode without an explicit `NonOverridableClass` exemption (which §9.3 confirms does not exist).
- Allowed scopes for `OPERATOR_LOCAL_SIGNED` (recovery-mode only): `OTHER_DEVICE_FIRMWARE`, `EMBEDDED_CONTROLLER` (with `STRONG_SOLO` and FOREVER evidence), `NETWORK_ADAPTER_FIRMWARE` (with the same constraints), and `DISK_FIRMWARE` for non-boot-disk devices.

The operator-local-signed path does **not** unlock the constitutional refusal of unsigned firmware: the operator cannot sign an arbitrary blob and label it as a constitutional-scope firmware. The hardware-key's public half is enrolled at first-boot; signing keys outside the enrolled set are rejected as `SIGNATURE_FAILED`.

### §5.5 Unsigned refusal path

`UNSIGNED_BLACKLISTED` is the constitutional refusal class. There is no "trust this once" gesture in normal mode. The recovery-mode emergency override (S5.4) is the only escape, and §9.3 enumerates the scopes where the override is permitted (a small subset; most scopes are denied with `SCOPE_TOO_BROAD`).

## §6 Per-scope rollback strategy table

The rollback strategy is determined by the scope, not by the update class. The pipeline at stage 7 selects the row from the table below:

### §6.1 `CPU_MICROCODE` — rollback impossible mid-session

Microcode is loaded into the CPU's internal microcode RAM by the kernel during boot or via late-load. Once loaded, the CPU runs the new microcode for the rest of the session; there is no architectural mechanism to "unload" microcode mid-session. Late-load is a one-way operation.

The pipeline's response:

- The post-apply verify step at stage 6 cannot be naively "rolled back" mid-session. Instead, the host emits `FIRMWARE_VERIFY_POST_FAILED` and forces a `kernel.reboot` action that, on the next boot, loads the previous microcode (which is restored by the kernel firmware loader from the vault-cached prior blob).
- The next boot's stage 6 confirms the prior version is loaded; FSM → `ROLLED_BACK`.
- `FIRMWARE_ROLLBACK_PERFORMED` FOREVER evidence is emitted on the post-reboot confirmation.

The constitutional consequence is that a misbehaving microcode applied mid-session may misbehave for the remainder of the session. The pipeline mitigates by aggressively forcing the post-apply reboot on any anomaly observed within the first 60 seconds after late-load. The 60-second window is normative; it parallels the S11.1 first-run capability-lie audit window.

### §6.2 In-driver atomic replace (`GPU_FIRMWARE`, `NETWORK_ADAPTER_FIRMWARE`, `OTHER_DEVICE_FIRMWARE`)

The driver mediates the rollback. The kernel firmware loader cache retains the previous blob; on rollback, the driver brings the device down, loads the prior blob, brings the device back up. The window of unavailability is the driver-reset latency (typically tens of milliseconds for NICs, low seconds for GPUs).

For `GPU_FIRMWARE`, the rollback is L8.2-aware: any active `VkDevice` partition is fenced before the GPU reset, and the partition's pending dmabuf imports are returned to the importer with `DEVICE_RESET_INTERRUPTED` (cite S8.2 fence semantics). Any subject's compute submissions in flight at reset time are returned with `EXECUTION_VERIFICATION_FAILED` per S10.1.

### §6.3 Vendor A/B slot (`DISK_FIRMWARE`, `TPM_FIRMWARE`, `BIOS_UEFI`)

The vendor's A/B slot mechanism is the canonical rollback for the most consequential scopes. UEFI specifies the `EFI_FIRMWARE_MANAGEMENT_PROTOCOL` and the CapsuleUpdate path; many BIOSes implement this; some implement vendor-proprietary alternatives. The pipeline's response:

- At stage 4, the new firmware is staged into the inactive slot.
- At stage 5, the platform commits — typically by setting the active-slot pointer and reboot.
- At stage 7, on post-verify failure, the platform demotes — typically by setting the active-slot pointer back to the prior slot and reboot.

For `BIOS_UEFI`, if the platform does not expose an A/B slot, the strategy falls back to `OFFLINE_REFLASH` if the vendor utility is available; otherwise the apply step refuses with `FIRMWARE_APPLY_FAILED` (`apply_strategy = NONE_AVAILABLE`).

### §6.4 Offline reflash (`EMBEDDED_CONTROLLER`)

Embedded controllers are typically reflashed via vendor utilities running in recovery mode. The pipeline's response:

- At stage 4, the firmware is staged into `/recovery/firmware/staging/<scope>/<device_id>/`.
- At stage 5, the host enters recovery mode (per S9.1) and the vendor utility executes the reflash.
- At stage 7, post-verify is the boot-time hardware-graph re-enumeration.
- On failure, rollback requires the operator's prior firmware backup; if the backup is absent, `ROLLED_BACK` is unreachable and `FAILED` is recorded with `rollback_impossible = true` and `MANUAL_RECOVERY_REQUIRED` operator alert.

## §7 Safe-state precondition

The transition `STAGED → APPLIED` is gated by a safe-state check. The check returns `FirmwareDeferReason` (§3.6) on contention; the pipeline holds the staged update and re-checks on each cycle.

The check is normative:

```text
SAFE_STATE := (
    no_in_flight_typed_actions         ∧
    not_in_recovery_mode               ∧
    no_other_firmware_update_staged    ∧
    no_sensitive_workload_running      ∧
    inside_user_opt_in_window
)
```

A staged update is held for up to 10 defer-cycles (each cycle ≤ 60 seconds); the eleventh cycle transitions to `ABANDONED` with `FIRMWARE_DEFER_EXHAUSTED` evidence (STANDARD_24M).

The safe-state check is constitutional: it cannot be loosened by any policy bundle. It binds the apply step to a state in which no in-flight action's evidence trail is corrupted by an unexpected device reset.

## §8 AI authorship rule

`firmware.update.request` is hard-denied for `is_ai = true` subjects at envelope validation (cite S2.3 hard-deny vocabulary; the rule id is `hd.firmware_update.ai_authored`). The denial:

- Returns `POLICY_DECISION_DENY` with reason `AISystemAdminBlocked` (cite INV-013).
- Emits `POLICY_DECISION_DENY` evidence (STANDARD_24M).
- Does **not** open the request to the recovery-mode emergency override path: even in recovery, an AI subject cannot author a firmware update.

The rule is constitutional: it binds INV-002 (AI proposes never executes) at the firmware-update axis. AI subjects can read the hardware graph, can suggest "this device has a firmware update available", can format the operator-facing prompt — but the action envelope must be authored by a human subject. This is the same rule as S11.1 §6 (AI subjects cannot install packages); the firmware variant is more strict because the recovery-mode emergency override does not lift it.

## §9 Recovery-mode emergency override boundary

### §9.1 Override eligibility by class

| `FirmwareUpdateClass`        | Override eligible?            | Notes                                                                                                                          |
| ---------------------------- | ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `VENDOR_SIGNED_LVFS`         | (n/a — already approved-path) | The class is already admitted; no override needed.                                                                             |
| `VENDOR_SIGNED_PROPRIETARY`  | (n/a — already approved-path) | Already admitted under `STRONG`.                                                                                               |
| `SIGNED_DISTRIBUTION_BUNDLE` | (n/a — already approved-path) | Already requires recovery + `STRONG_SOLO`.                                                                                     |
| `OPERATOR_LOCAL_SIGNED`      | (n/a — already approved-path) | Already requires recovery + `STRONG_SOLO` + disclosure.                                                                        |
| `UNSIGNED_BLACKLISTED`       | Conditional                   | Override eligible only for the `OTHER_DEVICE_FIRMWARE` scope (and only there); all other scopes denied with `SCOPE_TOO_BROAD`. |

### §9.2 Override eligibility by scope (for `UNSIGNED_BLACKLISTED`)

| `FirmwareScope`            | Override eligible (recovery-mode + S5.4)? | Cite                                                               |
| -------------------------- | ----------------------------------------- | ------------------------------------------------------------------ |
| `CPU_MICROCODE`            | No                                        | §9.3 — constitutional refusal.                                     |
| `GPU_FIRMWARE`             | No                                        | §9.3 — constitutional refusal (drift would invalidate INV-024).    |
| `DISK_FIRMWARE`            | No (`SCOPE_TOO_BROAD`)                    | S5.4 §10 — would invalidate boot path.                             |
| `NETWORK_ADAPTER_FIRMWARE` | No (`SCOPE_TOO_BROAD`)                    | S5.4 §10 — would invalidate L8 network policy enforcement.         |
| `TPM_FIRMWARE`             | No                                        | §9.3 — constitutional refusal (would invalidate INV-018).          |
| `BIOS_UEFI`                | No                                        | §9.3 — constitutional refusal.                                     |
| `EMBEDDED_CONTROLLER`      | No (`SCOPE_TOO_BROAD`)                    | S5.4 §10 — too broad for unsigned override.                        |
| `OTHER_DEVICE_FIRMWARE`    | Yes                                       | The single override-eligible scope; FOREVER evidence + disclosure. |

### §9.3 Constitutional refusal scopes

The four scopes `CPU_MICROCODE`, `GPU_FIRMWARE`, `TPM_FIRMWARE`, `BIOS_UEFI` are constitutionally refused for unsigned firmware **even in recovery mode**. A request to override any of these for `UNSIGNED_BLACKLISTED` is denied with `TARGET_NOT_OVERRIDABLE` (per S5.4 §10) and emits `OVERRIDE_DENIED` (FOREVER) and `OVERRIDE_REVIEW`. The four scopes are not currently in the `NonOverridableClass` enum (per S5.4 §10); this spec queues them as additions for the next S5.4 refinement wave but does not modify S5.4's enum directly. Until S5.4 is updated, the refusal is enforced by the firmware update pipeline's class-derivation step (§4.2) refusing `UNSIGNED_BLACKLISTED` and the override path's pre-check at §9.2 returning `SCOPE_TOO_BROAD` for these scopes.

The override-eligible scope (`OTHER_DEVICE_FIRMWARE`) is permitted because:

- The blast radius is bounded to the device (a malicious printer firmware affects the printer; it does not subvert L0 invariants).
- The L8 network policy can quarantine the device's traffic post-flash if the device joins the network.
- The hardware graph drift detection (cite §10.5) catches device behavioral anomalies post-flash.

The override path emits `OPERATOR_LOCAL_FIRMWARE_INSTALLED` FOREVER even though the firmware is technically `UNSIGNED_BLACKLISTED` — the operator's explicit override act is the witness.

## §10 Adversarial robustness

### §10.1 Vendor signing key compromise

Threat: vendor's Ed25519 signing key is exfiltrated; attacker ships a malicious firmware blob signed with the legitimate key.

Defense: AIOS publisher catalog (cite S11.1 §3.1) marks the vendor `DEPLATFORMED` via the AIOS-root-cosigned takedown event. Subsequent fetches return `VENDOR_DEPLATFORMED` at stage 2; FSM → `FAILED`; FOREVER `FIRMWARE_VENDOR_DEPLATFORMED` evidence.

For LVFS, the additional defense is the LVFS metadata revocation: the LVFS team typically rotates the affected vendor's per-blob signing key; subsequent LVFS-served updates from the compromised key fail signature verification at stage 2 (`SIGNATURE_FAILED`).

### §10.2 Firmware version downgrade (replay)

Threat: attacker replays an older signed firmware blob whose version contains a known exploit (e.g., a UEFI implementation with a published CVE).

Defense: per-`(scope, device_id)` `version_high_water` monotonic counter (§4.2). Any version strictly less than the high-water is rejected with `DOWNGRADE_BLOCKED` at stage 2; FOREVER `FIRMWARE_DOWNGRADE_BLOCKED` evidence.

The high-water is reset only via `RecoveryMutableScope = FIRMWARE_VERSION_COUNTER` (recovery-only, FOREVER evidence). An AI subject cannot reset it; an automated scheduler cannot reset it.

### §10.3 Tampered LVFS package

Threat: a mirror serves a tampered LVFS package bytes whose embedded signature still chains to a legitimate vendor key.

Defense: BLAKE3 hash check at stage 1 (§4.1) compares against the LVFS metadata's expected hash; mismatch returns `HASH_MISMATCH`. The metadata signature (Red-Hat-signed) is verified at stage 2; metadata tampering is detected at the metadata layer.

### §10.4 Supply-chain attack via firmware update channel

Threat: both the upstream vendor and the AIOS publisher root are simultaneously compromised; a malicious firmware bundle is shipped that passes both signatures.

Defense: this is the threat-model floor. The mitigation is operational, not algorithmic — AIOS publisher root keys operate with HSM separation (deferred to L4 vault broker) and rotation cadence ≤ 12 months. The cross-witness check (LVFS metadata + vendor key + AIOS publisher root co-signature) means that compromising one source is insufficient; the attacker must compromise two distinct cryptographic anchors simultaneously. Detection post-incident is via the L9 evidence-chain audit; the FOREVER retention of every firmware-apply event ensures forensic traceability.

### §10.5 Concurrent firmware update during sensitive workload

Threat: a firmware update is staged while a sensitive workload (vault unwrap, kernel build, recovery rehearsal) is running; applying mid-workload corrupts evidence.

Defense: safe-state precondition (§7); deferral via `FirmwareDeferReason.SENSITIVE_WORKLOAD_RUNNING`; defer cap at 10 cycles; `ABANDONED` with `FIRMWARE_DEFER_EXHAUSTED` evidence on cap. The operator can re-request after the workload completes.

### §10.6 Firmware update during recovery

Threat: an attacker triggers a firmware update while the host is in recovery, hoping the recovery-context approval gate is more permissive.

Defense: `FirmwareDeferReason.RECOVERY_MODE_ACTIVE` defers all non-recovery-class updates (i.e., `VENDOR_SIGNED_LVFS` and `VENDOR_SIGNED_PROPRIETARY` are deferred). Only `SIGNED_DISTRIBUTION_BUNDLE` and `OPERATOR_LOCAL_SIGNED` and the `UNSIGNED_BLACKLISTED` override (for the eligible scope) are admitted in recovery; each carries `STRONG_SOLO` strength and FOREVER evidence.

### §10.7 Evil-maid firmware-only swap

Threat: attacker with brief physical access reflashes firmware while the host is powered off, bypassing the apply pipeline entirely.

Defense: hardware graph drift detection (cite S8.3 — referenced abstractly; the property is queued for S8.3 consolidation). At every boot, the host re-enumerates each device's `firmware_version` and compares against the recorded last-applied version. A mismatch (the device reports a version different from `version_high_water` for that `(scope, device_id)`) emits FOREVER `FIRMWARE_TAMPER_DETECTED` and triggers recovery entry per S9.1 `RecoveryEntryReason = FIRMWARE_TAMPER_DETECTED` (queued for S9.1 consolidation alongside `EVIDENCE_LOG_TAMPER_DETECTED`).

The defense is detection, not prevention; an evil-maid attack with physical access can always reflash. The detection is what bounds the blast radius: the operator is alerted, the host enters recovery, and the firmware can be re-flashed from a trusted source.

### §10.8 UEFI variable manipulation

Threat: attacker manipulates UEFI variables (e.g., `BootOrder`, `SecureBoot`, vendor-specific variables) to alter boot behavior without flashing firmware.

Defense: measured-boot integration (cite S9.3 element 4 IMA + TPM). The PCR set at boot is quoted and compared against the AIOS-recorded expected set; drift outside the expected delta is `KERNEL_IMAGE_DRIFT_DETECTED` (per S9.3 §10) or `FIRMWARE_TAMPER_DETECTED` (per this spec) depending on which PCR set drifted.

The defense leverages S9.3's existing element 7 (the running kernel image is itself an evidence subject); this spec extends the same principle to firmware: the running firmware version is itself an evidence subject; drift is FOREVER-recorded and forces recovery.

## §11 Constitutional invariants honored

This spec honors the following constitutional invariants (cite L0 §3):

- **INV-002** (AI proposes never executes) — honored by §8 AI authorship rule; firmware updates are hard-denied for AI subjects.
- **INV-004** (recovery boundary preserved) — honored by §5.3 (recovery-only signed distribution bundle path); §6.4 (offline reflash to `/recovery/firmware/staging/`, outside `/aios`); §10.6 (firmware update during recovery is constrained).
- **INV-005** (evidence append-only) — honored by §13 (twelve evidence record types, all append-only); the FOREVER-retained classes are non-compactable (per S3.1).
- **INV-008** (default-deny in policy) — honored by §3.4 (`UNSIGNED_REJECTED` is the default for unsigned firmware); no implicit-allow path.
- **INV-013** (AI cannot perform system admin) — honored by §8 (AI subjects cannot author firmware updates even from a system-scoped capability binding).
- **INV-014** (no proof, no completion) — honored by §4.7 (commit only after stage 6 post-verify confirms the version match) and §10.7 (every-boot re-verification).
- **INV-018** (vault no raw secret leak) — honored by §13 (no firmware blob bytes in evidence; only blob hash).

The spec's behavior is mechanically derivable from the closed enums and the strict-ordered pipeline; no invariant rests on human discretion.

A note on INV-001 (recovery independent of L5). This spec does not directly enforce INV-001 — the firmware update pipeline runs in normal mode and consumes L4 policy, L5 cognitive context for prompt synthesis, and L9 evidence machinery. INV-001 is preserved transitively: the recovery path itself does not invoke this spec's pipeline. Recovery-mode firmware operations are limited to the four classes named in §4.3 (`SIGNED_DISTRIBUTION_BUNDLE`, `OPERATOR_LOCAL_SIGNED`, and the override-eligible `UNSIGNED_BLACKLISTED` for `OTHER_DEVICE_FIRMWARE`); each requires `STRONG_SOLO` operator presence and never invokes any L5 component. The firmware update pipeline is a normal-mode service that observes the recovery boundary, not a recovery-floor service.

A note on INV-024 (GPU compute capability-gated). This spec interacts with INV-024 transitively at §6.2 (in-driver atomic replace for `GPU_FIRMWARE`): the rollback path fences any active `VkDevice` partition before reset. A firmware update that arrives during active GPU compute is deferred via `FirmwareDeferReason.SENSITIVE_WORKLOAD_RUNNING` if the workload is tagged sensitive; otherwise the workload's compute submissions are returned with `EXECUTION_VERIFICATION_FAILED` (per S10.1) and the L8.2 partitioning is restored post-rollback. INV-024's capability gating is preserved across the firmware-update boundary because the post-update GPU state still requires the same `gpu.compute_heavy` capability binding for any new submissions.

## §12 Telemetry contract

| Metric                                 | Type    | Labels (closed)                                                                        |
| -------------------------------------- | ------- | -------------------------------------------------------------------------------------- |
| `firmware_update_attempts_total`       | counter | `class` (5), `scope` (8), `result` (8 — `FirmwareTrustResult`)                         |
| `firmware_update_apply_total`          | counter | `class` (5), `scope` (8), `apply_strategy` (3), `outcome` (applied/rolled_back/failed) |
| `firmware_update_defer_total`          | counter | `defer_reason` (5)                                                                     |
| `firmware_downgrade_blocked_total`     | counter | `scope` (8)                                                                            |
| `firmware_unsigned_rejected_total`     | counter | `scope` (8)                                                                            |
| `firmware_vendor_deplatformed_total`   | counter | none (rolling per-host)                                                                |
| `firmware_tamper_detected_total`       | counter | `scope` (8)                                                                            |
| `firmware_operator_local_signed_total` | counter | `scope` (8)                                                                            |
| `firmware_version_high_water_gauge`    | gauge   | `scope` (8), `device_id` (bounded by hardware-graph cardinality)                       |

Cardinality budget: ≤ 200 active label tuples per metric on a typical host (hardware-graph `device_id` is the dominant axis; bounded by the number of devices the host has). The `device_id` axis on `firmware_version_high_water_gauge` is capped at 256 entries; hosts exceeding the cap roll up by `scope`.

## §13 Evidence record types queued for S3.1

Twelve record types queued for S3.1 vocabulary extension. Each entry includes the retention class.

| Record type                         | Retention class | Trigger                                                                                                                                                                           |
| ----------------------------------- | --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `FIRMWARE_UPDATE_REQUESTED`         | STANDARD_24M    | Operator-authored `firmware.update.request` action accepted at envelope validation; carries `firmware_request_id`, `class`, `scope`, `device_id`, `proposed_version`.             |
| `FIRMWARE_VERIFICATION_PASSED`      | STANDARD_24M    | Stage 2 returns `VERIFIED_LVFS`, `VERIFIED_VENDOR_SIGNATURE`, or `VERIFIED_DISTRIBUTION`; carries `FirmwareTrustResult` and class.                                                |
| `FIRMWARE_VERIFICATION_FAILED`      | EXTENDED_60M    | Stage 2 returns any failure value other than `DOWNGRADE_BLOCKED` and `UNSIGNED_REJECTED` (which have FOREVER variants); carries failure reason.                                   |
| `FIRMWARE_DOWNGRADE_BLOCKED`        | FOREVER         | Stage 2 monotonicity check returns `DOWNGRADE_BLOCKED`; carries `proposed_version`, `version_high_water`, `device_id`, `scope`.                                                   |
| `FIRMWARE_UNSIGNED_REJECTED`        | FOREVER         | Stage 2 class-derivation returns `UNSIGNED_BLACKLISTED` and the override path is not taken; carries `device_id`, `scope`, `firmware_blob_hash`.                                   |
| `FIRMWARE_VENDOR_DEPLATFORMED`      | FOREVER         | Stage 2 detects vendor in S11.1 `DEPLATFORMED` state; carries `vendor_publisher_root_id`, `device_id`, `scope`.                                                                   |
| `FIRMWARE_APPLIED`                  | STANDARD_24M    | Stage 7 commits successfully; carries `firmware_request_id`, `class`, `scope`, `device_id`, `prior_version`, `new_version`, `apply_strategy`, `approver_id` or `operator_id`.     |
| `FIRMWARE_APPLY_FAILED`             | EXTENDED_60M    | Stage 5 apply step fails (write error, capsule reject, driver reload error); carries failure reason and `apply_strategy`.                                                         |
| `FIRMWARE_ROLLBACK_PERFORMED`       | FOREVER         | Stage 7 rollback path succeeds (or fails with `rollback_impossible = true`); carries `apply_strategy`, `rollback_outcome`.                                                        |
| `BIOS_UEFI_UPDATE_DEFERRED`         | EXTENDED_60M    | Stage 4 defers a `BIOS_UEFI` update due to a `FirmwareDeferReason` (operator-visibility need is higher for this scope).                                                           |
| `FIRMWARE_TAMPER_DETECTED`          | FOREVER         | Boot-time hardware-graph drift detection observes `firmware_version` mismatch against `version_high_water`; carries `scope`, `device_id`, `observed_version`, `expected_version`. |
| `OPERATOR_LOCAL_FIRMWARE_INSTALLED` | FOREVER         | Stage 7 commits a `OPERATOR_LOCAL_SIGNED` firmware update; carries `operator_id`, `hardware_key_serial_hash`, `firmware_blob_hash`, `scope`, `device_id`.                         |

Three additional records (`FIRMWARE_DEFERRED` STANDARD_24M, `FIRMWARE_DEFER_EXHAUSTED` STANDARD_24M, `FIRMWARE_REFLASH_IDENTICAL` STANDARD_24M, `FIRMWARE_STAGE_FAILED` EXTENDED_60M, `FIRMWARE_VERIFY_POST_FAILED` EXTENDED_60M, `FETCH_FAILED` STANDARD_24M, `DEVICE_NOT_PRESENT` STANDARD_24M) are operational sub-records of the twelve canonical types above; they are emitted by the pipeline but the canonical twelve are the headline vocabulary that downstream auditors and renderers bind to.

The twelve canonical types are normative for S3.1; S3.1 vocabulary will count this contribution as twelve at consolidation.

## §14 Worked examples

### §14.1 Example 1 — LVFS-signed Intel CPU microcode update (happy path)

```text
Setup: ThinkPad-class laptop; Intel Core i7-12700H; existing microcode revision 0x42c.
LVFS publishes microcode revision 0x430 fixing CVE-2025-XXXX (speculative-execution side channel).

Stage 1 — STAGE_FETCH:
  Subject: human:operator-247 (is_ai = false).
  Action: firmware.update.request {
    class: VENDOR_SIGNED_LVFS,
    scope: CPU_MICROCODE,
    device_id: cpu0,
    proposed_version: 0x430
  }
  Fetch from LVFS → 1.2 KB blob + LVFS metadata blob.
  Evidence: FIRMWARE_UPDATE_REQUESTED STANDARD_24M.

Stage 2 — STAGE_VERIFY:
  Verifier: Red-Hat LVFS signing key (pinned at first-boot per S9.2) verifies metadata.
  Per-blob signature verifies against Intel's vendor key listed in LVFS metadata.
  BLAKE3(blob) matches metadata expected hash.
  Class derivation → VENDOR_SIGNED_LVFS (matches claimed). ✓
  Monotonicity: 0x430 > 0x42c (current high-water). ✓
  Scope-vs-device: CPU_MICROCODE matches DeviceClass. ✓
  Result: VERIFIED_LVFS.
  Evidence: FIRMWARE_VERIFICATION_PASSED STANDARD_24M.

Stage 3 — STAGE_APPROVE:
  Default approval: STANDARD strength, EXACT_ACTION scope, SHORT_5M TTL.
  Channel: KDE_NATIVE_PROMPT (operator at console).
  Operator confirms; binding consumed.

Stage 4 — STAGE_STAGE:
  Strategy: AB_VENDOR_SLOT (microcode is loaded from the kernel firmware loader cache).
  Bytes written to /lib/firmware/intel-ucode/06-9a-03 (matches CPU SKU).
  Safe-state check: no in-flight typed actions, not in recovery, no other firmware staged,
  no sensitive workload, inside opt-in window. ✓

Stage 5 — STAGE_APPLY:
  Late-load microcode via /sys/devices/system/cpu/microcode/reload.
  Post-load, the CPU's microcode revision MSR (0x8B) reads 0x430 across all cores.

Stage 6 — STAGE_VERIFY_POST:
  60-second observation window: no anomalous behavior detected.
  Result: post-verify pass.

Stage 7 — STAGE_COMMIT_OR_ROLLBACK:
  Commit: version_high_water for (CPU_MICROCODE, cpu0) → 0x430.
  Evidence: FIRMWARE_APPLIED STANDARD_24M {
    class: VENDOR_SIGNED_LVFS,
    scope: CPU_MICROCODE,
    device_id: cpu0,
    prior_version: 0x42c,
    new_version: 0x430,
    apply_strategy: AB_VENDOR_SLOT,
    approver_id: human:operator-247
  }

Outcome: FSM = APPLIED. Operator notified. Audit trail complete.
```

### §14.2 Example 2 — Vendor-signed GPU firmware via AIOS publisher path

```text
Setup: workstation with NVIDIA RTX 5080; existing GPU firmware revision 555.42.06.
NVIDIA publishes 555.50.01 outside LVFS (proprietary distribution); the vendor's
signing key is registered in the AIOS publisher catalog at trust level VERIFIED.

Stage 1 — STAGE_FETCH:
  Subject: human:operator-247 (is_ai = false).
  Action: firmware.update.request {
    class: VENDOR_SIGNED_PROPRIETARY,
    scope: GPU_FIRMWARE,
    device_id: gpu0-pci-0000:01:00.0,
    proposed_version: 555.50.01
  }
  Fetch from NVIDIA endpoint → 18 MB bundle + bundle metadata.

Stage 2 — STAGE_VERIFY:
  Vendor signature on bundle metadata verifies against NVIDIA's key in S11.1 publisher
    catalog (trust level VERIFIED, not DEPLATFORMED).
  AIOS publisher root co-signature on bundle metadata verifies. ✓
  BLAKE3(bundle) matches metadata expected hash.
  Class derivation → VENDOR_SIGNED_PROPRIETARY (matches claimed). ✓
  Monotonicity: 555.50.01 > 555.42.06 (current high-water). ✓
  Result: VERIFIED_VENDOR_SIGNATURE.
  Evidence: FIRMWARE_VERIFICATION_PASSED STANDARD_24M.

Stage 3 — STAGE_APPROVE:
  Default approval: STRONG strength, EXACT_ACTION scope, SHORT_5M TTL.
  Channel: KDE_NATIVE_PROMPT.
  Operator (human, STRONG-class session) confirms; binding consumed.

Stage 4 — STAGE_STAGE:
  Strategy: IN_DRIVER_ATOMIC_REPLACE.
  Bytes written to /lib/firmware/nvidia/555.50.01/.
  Safe-state check: any active VkDevice partitions are noted; partitions with active
    GPU compute submissions DEFER the apply (FirmwareDeferReason.SENSITIVE_WORKLOAD_RUNNING)
    if the workload is is_sensitive=true. In this case, no sensitive workload is running. ✓

Stage 5 — STAGE_APPLY:
  Driver invokes GPU reset; partitions fenced (cite S8.2). Firmware reload via
    nvidia.ko's firmware_reload sysfs entry. Driver brings the device back up.
  Reported firmware version: 555.50.01.

Stage 6 — STAGE_VERIFY_POST:
  60-second observation window: GPU compute submissions resume; no anomalous fence breaks;
    no compute-class anomaly.
  Result: post-verify pass.

Stage 7 — STAGE_COMMIT_OR_ROLLBACK:
  Commit: version_high_water for (GPU_FIRMWARE, gpu0-pci-0000:01:00.0) → 555.50.01.
  Evidence: FIRMWARE_APPLIED STANDARD_24M.

Outcome: FSM = APPLIED. Audit trail complete.
```

### §14.3 Example 3 — Operator-local-signed firmware for a legacy device

```text
Setup: workstation with a 2019 Brother HL-L2370DW laser printer. Brother has dropped
firmware updates for this model. A community-built firmware patch fixes a CVE in the
embedded HTTP server. The operator has personally reviewed the patch source and
signed the resulting blob with their YubiKey-resident signing key (the same key
enrolled at first-boot per S9.2 RecoveryCredentialKind = HARDWARE_KEY).

Boot into recovery mode (the OPERATOR_LOCAL_SIGNED path is recovery-only).

Stage 1 — STAGE_FETCH:
  Subject: human:operator-247 (is_ai = false; recovery mode = true).
  Action: firmware.update.request {
    class: OPERATOR_LOCAL_SIGNED,
    scope: OTHER_DEVICE_FIRMWARE,
    device_id: usb-printer-brother-hl-l2370dw,
    proposed_version: community-1.2.3-local-1
  }
  File loaded from operator-provided USB stick.

Stage 2 — STAGE_VERIFY:
  Operator's hardware-key public half is enrolled (per S9.2).
  Operator-provided signature verifies. ✓
  BLAKE3(blob) matches manifest.
  Class derivation → OPERATOR_LOCAL_SIGNED (matches claimed).
  Monotonicity: community-1.2.3-local-1 > vendor-r2.04.06 (current high-water,
    using the version-comparison hook for opaque vendor versions which falls back
    to the "operator-asserted version is newer" attestation in the disclosure dialog). ✓
  Scope-vs-device: OTHER_DEVICE_FIRMWARE matches DeviceClass. ✓

Stage 3 — STAGE_APPROVE:
  Approval: STRONG_SOLO strength (recovery-mode), EXACT_ACTION scope, RECOVERY_BOUND TTL.
  Mandatory disclosure rendered (text per §4.3). Operator confirms with hardware-key tap.
  Binding consumed.

Stage 4 — STAGE_STAGE:
  Strategy: IN_DRIVER_ATOMIC_REPLACE (USB-class firmware loading via the printer's
    vendor-utility flash mechanism; runs in recovery).

Stage 5 — STAGE_APPLY:
  Vendor utility reflashes the printer. Reported version: community-1.2.3-local-1.

Stage 6 — STAGE_VERIFY_POST:
  Post-flash boot; printer reports version. Post-verify pass.

Stage 7 — STAGE_COMMIT_OR_ROLLBACK:
  Commit: version_high_water for (OTHER_DEVICE_FIRMWARE, usb-printer-brother-hl-l2370dw)
    → community-1.2.3-local-1.
  Evidence:
    FIRMWARE_APPLIED STANDARD_24M.
    OPERATOR_LOCAL_FIRMWARE_INSTALLED FOREVER {
      operator_id: human:operator-247,
      hardware_key_serial_hash: <yubikey serial hash>,
      firmware_blob_hash: <BLAKE3 of community blob>,
      scope: OTHER_DEVICE_FIRMWARE,
      device_id: usb-printer-brother-hl-l2370dw
    }

Outcome: FSM = APPLIED. The operator has accepted the constitutional risk explicitly;
the audit trail is FOREVER-retained; future hardware-graph drift detection on this
device will compare against community-1.2.3-local-1.
```

## §15 Cross-spec dependencies

| Spec  | Direction  | What this spec contributes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| ----- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S3.1  | producer   | Twelve evidence record types queued (`FIRMWARE_UPDATE_REQUESTED` STANDARD_24M, `FIRMWARE_VERIFICATION_PASSED` STANDARD_24M, `FIRMWARE_VERIFICATION_FAILED` EXTENDED_60M, `FIRMWARE_DOWNGRADE_BLOCKED` FOREVER, `FIRMWARE_UNSIGNED_REJECTED` FOREVER, `FIRMWARE_VENDOR_DEPLATFORMED` FOREVER, `FIRMWARE_APPLIED` STANDARD_24M, `FIRMWARE_APPLY_FAILED` EXTENDED_60M, `FIRMWARE_ROLLBACK_PERFORMED` FOREVER, `BIOS_UEFI_UPDATE_DEFERRED` EXTENDED_60M, `FIRMWARE_TAMPER_DETECTED` FOREVER, `OPERATOR_LOCAL_FIRMWARE_INSTALLED` FOREVER). |
| S2.3  | producer   | Hard-deny rule id `hd.firmware_update.ai_authored` queued (cite §8 — AI authorship rule).                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| S5.4  | constraint | Four scopes (`CPU_MICROCODE`, `GPU_FIRMWARE`, `TPM_FIRMWARE`, `BIOS_UEFI`) queued for `NonOverridableClass` review at next S5.4 refinement (currently enforced at this spec's pipeline).                                                                                                                                                                                                                                                                                                                                               |
| S8.3  | consumer   | Consumes `DeviceClass` and `firmware_version` per device (referenced abstractly; this spec does not freeze the wire shape).                                                                                                                                                                                                                                                                                                                                                                                                            |
| S9.1  | producer   | Queues `RecoveryEntryReason = FIRMWARE_TAMPER_DETECTED`; queues `RecoveryMutableScope = FIRMWARE_VERSION_COUNTER`.                                                                                                                                                                                                                                                                                                                                                                                                                     |
| S9.2  | constraint | First-boot pins LVFS Red-Hat signing key (`firmware.lvfs.signing_pubkey_hash`) and operator hardware-key public half (per `RecoveryCredentialKind = HARDWARE_KEY`).                                                                                                                                                                                                                                                                                                                                                                    |
| S9.3  | consumer   | Consumes `firmware_blob_set` build input; every committed firmware blob is retrievable for kernel rebuild.                                                                                                                                                                                                                                                                                                                                                                                                                             |
| S11.1 | consumer   | Consumes `PublisherTrustLevel = VERIFIED`/`DEPLATFORMED`, `PackageVerificationResult` taxonomy (mirror), AIOS publisher root co-signing.                                                                                                                                                                                                                                                                                                                                                                                               |
| L0    | constraint | Cites INV-002, INV-004, INV-005, INV-008, INV-013, INV-014, INV-018.                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |

## §16 Acceptance criteria

- [ ] `FirmwareUpdateClass` is a closed enum with 5 values (§3.1).
- [ ] `FirmwareScope` is a closed enum with 8 values (§3.2).
- [ ] `FirmwareUpdateState` is a closed FSM with 8 states and the transition diagram in §3.3.
- [ ] `FirmwareTrustResult` is a closed enum with 8 outcomes mirroring S11.1 §3.7 where applicable (§3.4).
- [ ] `FirmwareApplyStrategy` is a closed enum with 3 strategies (§3.5).
- [ ] `FirmwareDeferReason` is a closed enum with 5 reasons (§3.6).
- [ ] The seven-stage pipeline is strict-ordered and fail-closed (§4).
- [ ] Stage 2 derives `FirmwareUpdateClass` from the signature chain (not from the operator's claim) and detects class downgrade.
- [ ] Stage 2 enforces per-`(scope, device_id)` monotonic version counter (§4.2).
- [ ] Stage 2 enforces scope-vs-device match against the hardware graph.
- [ ] Stage 3 default approval-strength table matches §4.3.
- [ ] Stage 3 mandatory disclosure copy for `OPERATOR_LOCAL_SIGNED` matches §4.3 (normative).
- [ ] Stage 7 commit bumps `version_high_water`; rollback does not decrement it.
- [ ] Per-scope rollback strategy matches §6.
- [ ] Safe-state precondition (§7) defers up to 10 cycles; eleventh cycle abandons.
- [ ] AI authorship rule (§8) hard-denies `firmware.update.request` for `is_ai = true`.
- [ ] Recovery-mode emergency override boundary (§9) refuses unsigned updates for `CPU_MICROCODE`, `GPU_FIRMWARE`, `TPM_FIRMWARE`, `BIOS_UEFI` constitutionally; permits `OTHER_DEVICE_FIRMWARE` only.
- [ ] All twelve evidence record types in §13 are queued for S3.1.
- [ ] Telemetry (§12) conforms to cardinality bounds.
- [ ] Three worked examples (§14) produce the specified outcomes.
- [ ] All cited cross-spec references resolve: L0 INV-002/004/005/008/013/014/018; S2.3 hard-deny vocabulary; S5.3 ApprovalStrength + EXACT_ACTION; S5.4 NonOverridableClass + SCOPE_TOO_BROAD; S8.3 DeviceClass + firmware_version (abstract); S9.1 RecoveryMode + RecoveryEntryReason + RecoveryMutableScope; S9.2 first-boot pinning; S9.3 firmware_blob_set + measured-boot; S11.1 PublisherTrustLevel + PackageVerificationResult; S3.1 retention classes.

## §17 Failure-mode reference

Every closed failure outcome named anywhere in this spec resolves to exactly one row in the table below. The table is the canonical disambiguation source: a downstream auditor reading an evidence record carrying a `FirmwareTrustResult` value, a pipeline-stage failure, or a policy-deny reason can resolve the failure to its constitutional cause without re-reading the prose.

| Failure name                        | Stage / origin             | Constitutional cause                                                                          | Evidence record                         | Retention    | Operator action                                                                  |
| ----------------------------------- | -------------------------- | --------------------------------------------------------------------------------------------- | --------------------------------------- | ------------ | -------------------------------------------------------------------------------- |
| `SIGNATURE_FAILED`                  | Stage 2 (verify)           | Cryptographic chain broken; vendor key, LVFS metadata, or AIOS publisher co-sig invalid.      | `FIRMWARE_VERIFICATION_FAILED`          | EXTENDED_60M | Re-fetch from authoritative source; if persistent, escalate.                     |
| `VENDOR_DEPLATFORMED`               | Stage 2 (verify)           | Vendor's S11.1 publisher entry is `DEPLATFORMED`; binding takedown is in effect.              | `FIRMWARE_VENDOR_DEPLATFORMED`          | FOREVER      | None automatic; vendor must be re-onboarded via S11.1 if applicable.             |
| `HASH_MISMATCH`                     | Stage 1 / 2                | Mirror or transport tampering; bytes differ from manifest expected hash.                      | `FIRMWARE_VERIFICATION_FAILED`          | EXTENDED_60M | Re-fetch from a different mirror; the offending mirror's counter ticks.          |
| `DOWNGRADE_BLOCKED`                 | Stage 2 (monotonicity)     | Proposed version < `version_high_water` for `(scope, device_id)`.                             | `FIRMWARE_DOWNGRADE_BLOCKED`            | FOREVER      | Recovery-mode high-water reset (rare; requires `STRONG_SOLO`).                   |
| `UNSIGNED_REJECTED`                 | Stage 2 (class derivation) | No admissible signature path; `UNSIGNED_BLACKLISTED`.                                         | `FIRMWARE_UNSIGNED_REJECTED`            | FOREVER      | None except recovery-mode override for the eligible scope (§9).                  |
| `BUNDLE_TAMPERED`                   | Stage 2 (scope check)      | Declared scope does not match device class (potentially malicious or corrupted bundle).       | `FIRMWARE_VERIFICATION_FAILED`          | EXTENDED_60M | Discard bundle; re-fetch from authoritative source.                              |
| `DEVICE_NOT_PRESENT`                | Stage 2 (scope check)      | Device id absent from current hardware graph snapshot.                                        | `FIRMWARE_VERIFICATION_FAILED`          | EXTENDED_60M | Verify device is connected; re-enumerate hardware graph (S8.3).                  |
| `APPROVAL_DENIED`                   | Stage 3                    | Operator denied the prompt; or TTL expired before binding consumed.                           | `APPROVAL_DENIED` (per S5.3)            | STANDARD_24M | Re-request if intent stands; binding not reusable.                               |
| `FIRMWARE_STAGE_FAILED`             | Stage 4                    | Disk full, permission denied, capsule slot reject, or staging path unavailable.               | `FIRMWARE_STAGE_FAILED` (sub-record)    | EXTENDED_60M | Investigate I/O / permissions; re-attempt.                                       |
| `FIRMWARE_APPLY_FAILED`             | Stage 5                    | Driver reload error, capsule activate reject, vendor utility refused.                         | `FIRMWARE_APPLY_FAILED`                 | EXTENDED_60M | Pipeline rolls back automatically where rollback reachable; else operator alert. |
| `FIRMWARE_VERIFY_POST_FAILED`       | Stage 6                    | Reported version after apply does not match proposed version, or TPM PCR drift exceeds delta. | `FIRMWARE_ROLLBACK_PERFORMED`           | FOREVER      | Pipeline forces rollback; operator reviews FOREVER evidence.                     |
| `FIRMWARE_DEFER_EXHAUSTED`          | Stage 4 (safe-state)       | 10 defer-cycles consumed without safe-state; FSM transitioned to `ABANDONED`.                 | `FIRMWARE_DEFER_EXHAUSTED` (sub-record) | STANDARD_24M | Re-request when contention resolves.                                             |
| `AISystemAdminBlocked`              | Envelope validation        | AI subject (`is_ai = true`) authored `firmware.update.request`; constitutional refusal.       | `POLICY_DECISION_DENY` (S2.3)           | STANDARD_24M | None (AI cannot author firmware updates; INV-002 / INV-013).                     |
| `SCOPE_TOO_BROAD` (override)        | S5.4 override path         | Recovery-mode override requested for a scope outside the override-eligible set (§9.2).        | `OVERRIDE_DENIED` (S5.4)                | FOREVER      | Constitutional refusal; no operator action available.                            |
| `TARGET_NOT_OVERRIDABLE` (override) | S5.4 override path         | Recovery-mode override requested for a constitutionally refused scope (§9.3).                 | `OVERRIDE_DENIED` (S5.4)                | FOREVER      | None — the four refused scopes are immovable.                                    |

Two facets are worth flagging for downstream tooling:

1. **Sub-records vs canonical records.** §13 lists twelve canonical record types. Several pipeline-internal failures (`FETCH_FAILED`, `FIRMWARE_STAGE_FAILED`, `FIRMWARE_VERIFY_POST_FAILED`, `FIRMWARE_DEFER_EXHAUSTED`, `FIRMWARE_REFLASH_IDENTICAL`, `FIRMWARE_DEFERRED`, `DEVICE_NOT_PRESENT`) emit operational sub-records for diagnostic depth, but the canonical twelve are what auditors and renderers bind to for headline facts. The retention class on the sub-records matches the operational visibility need (mostly STANDARD_24M).

2. **Cross-spec record producers.** This spec emits firmware-specific records exclusively; it does not emit `POLICY_DECISION_DENY` (that is S2.3's authority) nor `OVERRIDE_DENIED` (that is S5.4's authority). The table above cites those records for the operator's mental model but the producer authority is preserved.

## §18 Implementation surface notes

The spec is contract-grade, not implementation-grade. The notes below mark the implementation choices a downstream Wave-N implementer must make without expanding the contract:

- **Signature primitive selection.** The contract names Ed25519 as the default. Some legacy LVFS firmware uses RSA-PSS with SHA-256; the LVFS metadata signature itself is GPG-signed historically. The implementer may accept the legacy primitives at the LVFS path only, gated by a closed `LegacySignaturePrimitive` enum scoped to LVFS metadata; vendor-signed proprietary and AIOS publisher root signatures MUST be Ed25519.

- **Version comparison hook.** The contract mandates monotonicity but does not mandate a single version-comparison algorithm. Vendors use heterogeneous version schemes (semver, opaque hex revisions, vendor-internal release names). The implementer SHALL provide a per-scope `VersionComparator` whose default for known schemes is documented; for opaque vendor versions, the comparator falls back to the operator's attestation in the disclosure dialog (§4.3) which becomes part of the FOREVER evidence chain. The comparator MUST never silently accept a version it cannot order — silent acceptance is a downgrade vector.

- **Hardware graph drift window.** The contract names "every-boot re-verification" (§10.7) but does not bound the window. The implementer SHALL set the post-boot drift check to fire within the first 30 seconds of normal-mode boot and emit `FIRMWARE_TAMPER_DETECTED` FOREVER on any mismatch. The 30-second window is tight enough to detect evil-maid scenarios before the operator's session begins; it is loose enough to absorb transient device-enumeration latency.

- **TPM PCR delta tolerance.** The contract requires PCR re-quote after `BIOS_UEFI` and `TPM_FIRMWARE` updates (§4.6) but does not specify the delta. The implementer SHALL use the platform's expected-PCR-delta table (vendor-supplied where available; AIOS-curated otherwise) and treat any unexpected drift as `FIRMWARE_TAMPER_DETECTED` FOREVER. False positives during this stage are operator-visible and trigger recovery; false negatives are a constitutional failure and must be tracked in the L9 incident-response runbook.

- **Defer-cycle scheduling.** The contract bounds defer cycles at 10 (§3.6, §7) without specifying inter-cycle latency. The implementer SHALL schedule cycles with exponential backoff bounded at 60 seconds per cycle; total deferral wall-clock is bounded at ~10 minutes, after which the FSM transitions to `ABANDONED`. The operator can re-request after the contention resolves.

- **Operator disclosure copy localization.** The mandatory disclosure copy in §4.3 is normative in English. Renderers (per L7 visual language) MUST localize the copy without altering the constitutional substance (the disclosed risks, the FOREVER-evidence statement, and the operator-accountability statement). A localization check is part of L7 conformance testing.

- **HSM-backed AIOS publisher root signing.** §10.4 assumes HSM separation operationally. Until L4 vault broker formalizes HSM mechanics (deferred per §19), the implementer SHALL document the AIOS publisher root key's storage substrate, rotation cadence, and ceremony procedure in a non-public AIOS operational handbook; the publishable contract floor is "the key is rotated at most every 12 months and stored in a substrate at least as restrictive as a YubiHSM 2."

- **Per-scope quota enforcement.** Some scopes (notably `BIOS_UEFI`) are typically updated rarely (a handful of times per host per decade); others (notably `OTHER_DEVICE_FIRMWARE`) may be updated frequently. The implementer SHALL emit per-scope rate alerts when the per-scope update count exceeds a learned baseline by a factor of 4× within a rolling 30-day window. The alert is operational, not constitutional — it does not block the update — but a sudden spike of `BIOS_UEFI` updates is a strong signal of automation-error or attack-precursor and warrants operator review.

- **Boot-time firmware enumeration order.** The hardware graph drift detection (§10.7) requires reading every device's reported `firmware_version` at boot. The implementer SHALL execute the enumeration in dependency order (CPU before chipset; chipset before peripheral buses; buses before downstream devices) so that a transitively-dependent device's reported version is read in a state where the underlying bus has been initialised with the post-update firmware. Out-of-order enumeration produces false positives during boot and is a class of bug not covered by the constitutional contract — it is an implementer responsibility.

- **Driver-mediated rollback and pending DMA.** §6.2's `IN_DRIVER_ATOMIC_REPLACE` for `GPU_FIRMWARE` and `NETWORK_ADAPTER_FIRMWARE` interacts with pending DMA. The implementer SHALL fence pending DMA at reset time (cite S8.2's fence semantics for GPU; equivalent for network adapters via the kernel's ndo_stop / ndo_open transition). DMA in flight at reset time MAY produce truncated transactions visible to the L8 network policy as zero-length receives; the policy MUST treat such receives as invariant-preserving (no message content; no metadata leak). A driver that does not fence DMA at firmware reload is implementing the strategy incorrectly and the implementer is responsible for the regression.

- **Operator-local-signed disclosure persistence.** §4.3's mandatory disclosure dialog produces an operator-confirmed witness. The implementer SHALL persist the operator's confirmation as part of the `OPERATOR_LOCAL_FIRMWARE_INSTALLED` FOREVER record's payload (the `disclosure_text_hash` field, BLAKE3 of the disclosure copy actually rendered to the operator at confirmation time, plus the operator's keystroke / hardware-key-tap timestamp). This binds the operator's accountability to the exact disclosure they witnessed; later modifications to the disclosure copy do not retroactively rewrite an operator's prior consent.

These notes are non-normative; they are implementer guidance. The contract floor is the closed enums, the strict-ordered pipeline, and the FOREVER-evidence discipline.

## §19 Stability and minimality

The vocabularies in §3 are deliberately the smallest set that preserves the constitutional firmware-trust posture under realistic implementation drift. As AIOS gains hardware support — new device classes, new bus topologies, new vendor partnerships — each new feature introduces new firmware update paths in S8.3. The mapping of those paths to `FirmwareScope` and `FirmwareUpdateClass` is performed in this spec; the discipline is that growth must remain bounded by the constitutional refusal of unsigned firmware (§5.5) and by the AI-authorship refusal (§8). The number of `FirmwareScope` values can grow over time (a future AIOS that gains support for a class of accelerator currently absent from the eight-scope enum may add a ninth), but the discipline of distinct rollback semantics, distinct safe-state preconditions, and an explicit override-eligibility row keeps the growth bounded. A future revision proposing to add a `FirmwareScope` value MUST show: (a) the new class has rollback semantics distinct from every existing class; (b) the new class's override-eligibility row is constitutionally defensible (most likely "no override" or "recovery-only override"); (c) the new class's per-device version counter has a defined comparison hook.

The five values in `FirmwareUpdateClass` are also chosen for stability. New classes can be added (versioned spec change) but only if they represent a distinct cryptographic chain that this spec's existing classes do not capture. A community-organisation-signed path (e.g., an open-firmware-foundation-signed bundle) would be a candidate sixth class; its discipline would mirror `VENDOR_SIGNED_PROPRIETARY` with an organisation-level publisher catalog entry instead of a vendor entry. The discipline that no firmware path is admitted without a chain to AIOS root, vendor key, LVFS, or operator hardware key is constitutional and cannot be loosened.

The eight values in `FirmwareTrustResult` mirror S11.1's `PackageVerificationResult` deliberately. Operators reading evidence records across the firmware and package surfaces see consistent failure-mode names; downstream tooling that audits both surfaces shares a single mental model of what "verified" and "rejected" mean. The two firmware-specific values (`DOWNGRADE_BLOCKED` and `UNSIGNED_REJECTED`) exist because the L10 distribution layer's monotonicity discipline is per-package, while firmware monotonicity is per-`(scope, device_id)`; the names exist as separate values to make the firmware-axis failure unambiguous in evidence.

## §20 Acceptance summary

### §20.1 Why E1 is the right grade now

The "REAL" status is justified at E1 because the grade-axis requirement for this sub-spec is "file exists" — a contract-grade specification that defines the closed vocabularies, the strict-ordered pipeline, the trust-binding model, the per-scope rollback semantics, the AI-authorship refusal, the override boundary, the adversarial threat model, the evidence vocabulary, and three worked examples. None of those structural elements depends on running code; they are constitutional shape and downstream enforcers must bind to this shape regardless of implementation cadence. Promoting to E2 (build/typecheck) requires the schema package `aios.firmware.v1alpha1` to compile and link against S11.1 / S5.3 / S5.4 / S3.1 / S2.3 schemas; that work is a downstream task, not a pre-condition for the contract.

### §20.2 What is structurally complete

In one paragraph: this spec is `REAL` at evidence grade `E1` because the contract surface is structurally complete. The closed enums in §3 cover every admissible firmware path AIOS recognises; the seven-stage pipeline in §4 is strict-ordered and fail-closed at every step; the trust-binding model in §5 chains every admitted firmware blob to LVFS, to a vendor with an active AIOS publisher root binding, to AIOS root via a signed distribution bundle, or to the operator's own hardware key; the per-scope rollback table in §6 specifies the constitutional behavior when a firmware update fails post-apply; the safe-state precondition in §7 prevents firmware applies from corrupting in-flight evidence; the AI-authorship refusal in §8 binds INV-002 to the firmware axis; the override boundary in §9 prevents loosening of the constitutional refusal of unsigned firmware in the four most consequential scopes; the adversarial robustness section in §10 binds the seven named threats to named mechanisms; the twelve evidence record types in §13 give every consequential outcome an audit witness with the required retention class. Implementation evidence (E2 build/typecheck of generated proto, E3 unit tests against the FSM, E4 e2e firmware-rehearsal) is queued for downstream phases and tracked under the L8 implementation roadmap, not in this spec.

### §20.3 Reachability of higher grades

- **E2 — Build/typecheck.** Reachable when the `aios.firmware.v1alpha1` schema package compiles and the generated `FirmwareUpdateClass`, `FirmwareScope`, `FirmwareUpdateState`, `FirmwareTrustResult`, `FirmwareApplyStrategy`, `FirmwareDeferReason` enums are linked into the policy kernel's hard-deny vocabulary, the evidence log's record vocabulary, and the renderer's prompt-template binding. The schema package is a downstream artefact; this spec does not block on it.
- **E3 — Unit / integration tests.** Reachable when the firmware update pipeline service's first implementation drives the FSM through every allowed transition (DRAFT → VALIDATING → APPROVED → STAGED → APPLIED; and every allowed branch into FAILED / ROLLED_BACK / ABANDONED) and asserts that every forbidden transition is rejected. Test coverage MUST include the monotonicity check, the class-derivation check, the safe-state check (with all five `FirmwareDeferReason` values), and the AI-authorship refusal.
- **E4 — End-to-end / recovery rehearsal.** Reachable when a recovery-mode operator-local-signed firmware install completes against a real device (the §14.3 example as a live test), and when an evil-maid-style hardware-graph drift produces `FIRMWARE_TAMPER_DETECTED` FOREVER in a recovery rehearsal harness.
- **E5 — Live operational.** Reachable when fleet-scale firmware updates run on production AIOS hosts and the per-host telemetry conforms to §12 cardinality bounds with no constitutional refusal regressions over a rolling 90-day window.

## §21 Open deferrals

- **HSM-protected AIOS publisher root** — HSM separation for the AIOS publisher root key is operationally assumed in §10.4 but the L4 vault broker has not yet specified HSM mechanics. Deferred to L4 vault broker refinement.
- **Cross-host firmware fleet management** — multi-host firmware update propagation. Deferred.
- **Per-firmware-blob behavioral whitelisting** — beyond signature, no behavioral attestation on the firmware itself (e.g., "this microcode patch only fixes CVE-X"). Deferred.
- **Threshold or multi-party signing for AIOS publisher root** — single-AIOS-root for now. Deferred.
- **Firmware update rollback for `OFFLINE_REFLASH` without prior backup** — currently unreachable without operator-held backup; manual recovery required. Deferred to L9 admin operations.
- **Firmware update via marketplace UX** — owned by L10 marketplace (`SHELL`).
- **Per-vendor SBOM (Software Bill of Materials) integration for firmware blobs** — current spec validates signatures and hashes; a future revision may admit per-blob SBOM as an additional verification input (e.g., to detect that a vendor-signed bundle includes a known-vulnerable component). Deferred.
- **Federated firmware advisory ingestion** — automatic ingestion of CVE feeds and vendor advisories to surface "update available + CVE referenced" hints to the operator. Deferred to L9 admin operations + L10 marketplace.
- **Firmware update SLO** (e.g., "BIOS_UEFI updates complete within 90 days of vendor publication on healthy hosts") — deferred.
- **Cross-platform firmware abstraction layer** — current spec assumes a Linux substrate at L1; firmware update mechanics for embedded variants (e.g., RISC-V boards, aarch64 SoCs) may require additional `FirmwareApplyStrategy` values. Deferred until those substrates are spec-supported.
- **Operator-visible firmware change journal** — a per-host journal surfacing every `FIRMWARE_APPLIED` and `FIRMWARE_TAMPER_DETECTED` event for the last N days, rendered through L7 visual language. Deferred to L9 admin operations sub-spec consolidation.

## See also

- [L8 Overview](00_overview.md)
- [S8.1 — Network Policy](02_network_policy.md)
- [S8.2 — GPU Resource Model](05_gpu_resource_model.md)
- [S9.1 — Recovery Boundary](../L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md)
- [S9.2 — First-Boot Flow](../L1_Kernel_Bootstrap_Recovery/02_first_boot_flow.md)
- [S9.3 — Dedicated Kernel Pipeline](../L1_Kernel_Bootstrap_Recovery/03_dedicated_kernel_pipeline.md)
- [S11.1 — Repository Model + Trust Roots](../L10_Distribution_Ecosystem_Marketplace/01_repository_model.md)
- [S2.3 — Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S5.3 — Approval Mechanics](../L4_Policy_Identity_Vault/04_approval_mechanics.md)
- [S5.4 — Emergency Override](../L4_Policy_Identity_Vault/05_emergency_override.md)
- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [L0 §3 — Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md)
- [Rev.1 §18 — Hardware and Network](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
