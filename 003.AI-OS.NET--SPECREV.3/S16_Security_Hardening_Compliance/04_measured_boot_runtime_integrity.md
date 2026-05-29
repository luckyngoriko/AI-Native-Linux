# S16.4 — Measured Boot and Runtime Integrity

| Field     | Value                                                                                                                                                                                                         |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-29; awaiting implementation evidence)                                                                                                                                             |
| Phase tag | S16.4                                                                                                                                                                                                         |
| Layer     | Cross-cutting: L1, L8 (boot/runtime root of trust; fleet attestation)                                                                                                                                         |
| Consumes  | S9.3 Dedicated Kernel Pipeline (kernel drift), S8.5 Firmware Trust, S9.1 Recovery Boundary, S16.1 Security Profile Matrix, S11.1 Repository Model + Trust Roots, S2.3 Policy Kernel, S3.1 Evidence Log        |
| Produces  | `BootPosture`, `TPMQuote`, `IMAPolicy`, `IMAAppraisalState`, `RootIntegrityEvidence`, `RemoteAttestationVerifier`, dual-chain attestation root, `BOOT_INTEGRITY_POSTURE` + `CRYPTO_BOUNDARY_SELECTED` schemas |

## 1. Responsibility

S16.4 defines the high-assurance **root of trust** for an AIOS host: the chain
that proves, with signed evidence, that the firmware, bootloader, kernel,
kernel command line, and initramfs that are running are the ones AIOS expected,
and that the running filesystem and security-critical files have not been
silently mutated.

This is the P0 contract that turns the planning statement "high-assurance boot
and runtime baseline" into machine-readable truth. It is the boot/runtime half
of the security posture; S16.1 fixes _which_ posture is required per profile,
and S16.4 fixes _how that posture is measured, quoted, appraised, verified, and
evidenced_.

S16.4 implements the **dual-chain attestation root** mandated by DEC-R3-002: a
firmware-pinned trust root (S11.1 / S8.5) **plus** TPM 2.0 measured boot with
signed quotes. Either chain alone is insufficient for `STIG_ALIGNED` or
`AIRGAP_HIGH` where a TPM is present; both must verify.

S16.4 gives concrete schemas to the two evidence record types that S16.1
references but does not define: `BOOT_INTEGRITY_POSTURE` and
`CRYPTO_BOUNDARY_SELECTED` (the latter cross-links to the FIPS overlay in S16.5).

Invariant links: INV-001 (recovery boots without AI), INV-002 (AI proposes,
never executes), INV-004, INV-005, INV-008, INV-012, INV-013, INV-014
(append-only evidence), INV-017, INV-024. New invariant: **INV-028** (see §13).

## 2. Product principle

The operator wants one truthful sentence at every boot: _"this machine booted
the software AIOS expected, on hardware AIOS trusts, into the security profile
the operator selected — here is the proof."_

```text
power on
  -> firmware measures bootloader        (firmware chain)
  -> TPM extends PCRs                     (hardware chain)
  -> AIOS evaluates BootPosture
  -> AIOS requests a signed TPMQuote
  -> AIOS verifies firmware pin + quote   (dual chain)
  -> IMA appraisal gates critical paths
  -> dm-verity / IPE enforces immutable root
  -> RootIntegrityEvidence is sealed for this boot
  -> meets selected SecurityProfile? -> continue
  -> does not meet it?              -> drop to recovery with evidence
```

The system never silently downgrades to a permissive boot. A boot that cannot
prove its posture drops to the S9.1 recovery boundary and records _why_ — it
does not quietly continue in a weaker mode.

AI is not in this chain. The Cognitive Core may _read_ `BootPosture` and
_explain_ it, and may _propose_ a remediation typed action (e.g. "re-enroll
TPM", "rotate signing key") that the Policy Kernel decides and the Capability
Runtime executes. AI never authors boot policy, never signs a quote, never sets
a PCR expectation, and never mutates the integrity evidence.

## 3. Reference patterns

| Pattern                                                                                      | S16.4 use                                                                      |
| -------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| [UEFI Secure Boot](https://uefi.org/specs/UEFI/2.10/)                                        | Authenticated bootloader/kernel chain; `secure_boot` posture dimension.        |
| [TCG TPM 2.0 Library](https://trustedcomputinggroup.org/resource/tpm-library-specification/) | PCR model, quote structure, attestation key (AK) semantics.                    |
| [Linux IMA/EVM](https://docs.kernel.org/security/IMA-templates.html)                         | Integrity Measurement Architecture templates; measurement vs. appraisal modes. |
| [Linux IPE](https://docs.kernel.org/admin-guide/LSM/ipe.html)                                | Integrity Policy Enforcement as an immutable-root enforcement backend.         |
| [dm-verity](https://docs.kernel.org/admin-guide/device-mapper/verity.html)                   | Block-level immutable root filesystem integrity.                               |
| [Kernel lockdown LSM](https://man7.org/linux/man-pages/man7/kernel_lockdown.7.html)          | `none / integrity / confidentiality` lockdown levels.                          |
| [Linux module signing](https://kernel.org/doc/html/next/admin-guide/module-signing.html)     | `signed_modules_only` posture; couples to S19 module signing.                  |
| [Keylime remote attestation](https://keylime.dev/)                                           | Reference verifier model for the fleet `RemoteAttestationVerifier`.            |
| [NIST SP 800-155 BIOS Integrity Measurement](https://csrc.nist.gov/pubs/sp/800/155/ipd)      | Firmware/boot measurement guidance behind the firmware chain.                  |

## 4. Boot posture (closed schema)

`BootPosture` is the per-boot, machine-readable statement of what the boot chain
actually achieved. It is computed once early in boot, before any AIOS service
that depends on a profile starts.

```yaml
boot_posture:
  posture_id: "bpost_<ULID>"
  host_id: "host:<ULID>"
  boot_id: "boot:<ULID>" # unique per boot, links all evidence below
  measured_at: "2026-05-29T08:40:00Z"
  secure_boot: ENABLED # SecureBootState
  lockdown_level: CONFIDENTIALITY # LockdownLevel
  signed_modules_only: true
  ima_mode: APPRAISE_ENFORCE # IMAMode
  evm: ENABLED # EVMState
  dm_verity_or_ipe: DM_VERITY # ImmutableRootBackend
  tpm_present: true
  tpm_version: TPM_2_0 # TPMVersion
  firmware_pin_state: VERIFIED # FirmwarePinState (from S8.5 / S11.1)
  selected_profile: STIG_ALIGNED # SecurityProfile (S16.1, not redefined here)
  posture_verdict: SATISFIES_PROFILE # PostureVerdict
```

Closed enums for `BootPosture`:

```text
SecureBootState =
  ENABLED
| DISABLED
| SETUP_MODE
| UNSUPPORTED

LockdownLevel =
  NONE
| INTEGRITY
| CONFIDENTIALITY

IMAMode =
  OFF
| MEASURE_ONLY
| APPRAISE_LOG
| APPRAISE_ENFORCE

EVMState =
  ENABLED
| DISABLED
| UNSUPPORTED

ImmutableRootBackend =
  NONE
| DM_VERITY
| IPE
| DM_VERITY_AND_IPE

TPMVersion =
  NONE
| TPM_1_2
| TPM_2_0

FirmwarePinState =
  VERIFIED
| DRIFTED
| UNPINNED
| UNKNOWN

PostureVerdict =
  SATISFIES_PROFILE
| BELOW_PROFILE_RECOVERABLE
| BELOW_PROFILE_HARD_FAIL
```

Unknown values for any of these enums are rejected by the `BootPosture` loader;
an unrecognized value is treated as `UNKNOWN`/`HARD_FAIL`, never silently
accepted as the permissive baseline.

## 5. TPM quote (closed schema — hardware chain)

A `TPMQuote` is a TPM-signed statement over a selected PCR set, bound to a fresh
verifier nonce. Raw PCR values are recorded redacted in evidence; the full
values are sealed for the verifier, never exposed to AI subjects.

```yaml
tpm_quote:
  quote_id: "quote_<ULID>"
  boot_id: "boot:<ULID>"
  tpm_version: TPM_2_0
  pcr_set: [0, 1, 2, 3, 4, 5, 7, 8, 9, 14] # firmware, bootloader, kernel, cmdline, initramfs, IMA
  pcr_bank: SHA256 # PCRBank
  pcr_values_redacted:
    "0": "sha256:…(redacted-prefix)…"
    "7": "sha256:…(redacted-prefix)…"
  signing_key_id: "ak:<ULID>" # attestation key, enrolled per host
  signing_key_kind: TPM_ATTESTATION_KEY
  nonce: "nonce_<128bit-hex>" # supplied by the verifier; replay protection
  quote_sig: "tpmt_signature_blob_ref"
  event_log_ref: "evr_eventlog_<ULID>" # TCG event log for replay/verification
  verdict: QUOTE_VERIFIED # TPMQuoteVerdict
```

Closed enums for `TPMQuote`:

```text
PCRBank =
  SHA1
| SHA256
| SHA384

TPMQuoteVerdict =
  QUOTE_VERIFIED
| QUOTE_PCR_MISMATCH
| QUOTE_NONCE_STALE
| QUOTE_SIG_INVALID
| QUOTE_KEY_UNTRUSTED
| QUOTE_UNAVAILABLE
```

Unknown values are rejected by the quote verifier. `SHA1` is admitted only for
`DEV_RELAXED` legacy hardware; `STIG_ALIGNED`/`AIRGAP_HIGH` require at least
`SHA256` and treat a `SHA1`-only quote as `BELOW_PROFILE_HARD_FAIL`.

PCR expectations are never authored by AI (INV-002, INV-028). They are sealed
into the signed profile bundle (S16.1) and the firmware/trust-root pin (S11.1).

## 6. Dual-chain attestation root (DEC-R3-002)

The high-assurance root of trust combines two independent chains. A host's
`posture_verdict` is `SATISFIES_PROFILE` only when _both_ chains pass under
profiles that require them.

```text
Firmware chain (S8.5 / S11.1):
  trust-root signing key pin
    -> bootloader signature verified
    -> kernel + cmdline signature verified
    -> firmware_pin_state == VERIFIED

Hardware chain (this spec):
  TPM 2.0 measured boot
    -> PCRs extended over firmware/boot/kernel/cmdline/initramfs/IMA log
    -> signed TPMQuote over expected PCR set with fresh nonce
    -> verdict == QUOTE_VERIFIED against sealed expectations
```

Sufficiency rule:

| Condition                                          | `STIG_ALIGNED` / `AIRGAP_HIGH`           | `SECURE_DEFAULT`                         |
| -------------------------------------------------- | ---------------------------------------- | ---------------------------------------- |
| Both chains verify                                 | `SATISFIES_PROFILE`                      | `SATISFIES_PROFILE`                      |
| Firmware verifies, TPM **absent**                  | `BELOW_PROFILE_HARD_FAIL` (TPM required) | `SATISFIES_PROFILE` (TPM-absent allowed) |
| Firmware verifies, TPM **present but quote fails** | `BELOW_PROFILE_HARD_FAIL`                | `BELOW_PROFILE_RECOVERABLE`              |
| Firmware pin drifted                               | `BELOW_PROFILE_HARD_FAIL`                | `BELOW_PROFILE_RECOVERABLE`              |

A host without a TPM may reach `SECURE_DEFAULT` but **cannot** reach
`STIG_ALIGNED` or `AIRGAP_HIGH` (DEC-R3-002). The firmware pin survives
TPM-absent hardware; the TPM quote detects firmware/bootchain tampering the pin
alone cannot, which is why both are mandatory above `SECURE_DEFAULT`.

## 7. IMA policy and appraisal lifecycle

Runtime integrity uses Linux IMA/EVM. `IMAPolicy` declares the signed
measurement/appraisal rules; `IMAAppraisalState` is the per-boot result.

```yaml
ima_policy:
  policy_id: "imapol_<ULID>"
  version: "2026.05.rev3"
  applies_to_profiles: [STIG_ALIGNED, AIRGAP_HIGH]
  signing_key_id: "imapol:<ULID>"
  rules:
    - subject: BOOT_AGGREGATE
      action: MEASURE
    - subject: KERNEL_MODULES
      action: APPRAISE
      enforce: true
    - subject: AIOS_CRITICAL_BINARIES # policy kernel, vault broker, evidence log
      action: APPRAISE
      enforce: true
    - subject: POLICY_BUNDLES
      action: APPRAISE
      enforce: true
  on_appraisal_fail: BLOCK_AND_EVIDENCE # AppraisalFailAction
```

```text
ImaRuleAction =
  MEASURE
| APPRAISE
| AUDIT

AppraisalFailAction =
  PERMIT_AND_LOG       # DEV_RELAXED only
| BLOCK_AND_EVIDENCE   # SECURE_DEFAULT and above
| DROP_TO_RECOVERY     # STIG_ALIGNED / AIRGAP_HIGH for critical-path failures
```

IMA appraisal lifecycle (FSM):

```text
IMA_DISABLED
  -> (policy loaded, signed)            -> IMA_MEASURING
IMA_MEASURING
  -> (appraisal rules active)           -> IMA_APPRAISING_LOG
IMA_APPRAISING_LOG
  -> (enforce=true accepted by profile) -> IMA_APPRAISING_ENFORCE
IMA_APPRAISING_ENFORCE
  -> (critical-path appraisal fail)     -> IMA_VIOLATION
IMA_VIOLATION
  -> (BLOCK_AND_EVIDENCE)               -> IMA_APPRAISING_ENFORCE   # offending object blocked, host continues
  -> (DROP_TO_RECOVERY)                 -> RECOVERY_HANDOFF         # critical path; S9.1 boundary
```

`IMAAppraisalState` (per boot):

```yaml
ima_appraisal_state:
  state_id: "imastate_<ULID>"
  boot_id: "boot:<ULID>"
  policy_id: "imapol_<ULID>"
  fsm_state: IMA_APPRAISING_ENFORCE
  measured_count: 1842
  appraised_count: 1842
  violations: [] # list of {subject, path_redacted, expected, observed_redacted}
  evidence_receipt_id: "evr_<ULID>"
```

Unknown enum values in an IMA policy are rejected by the policy loader; an
unsigned IMA policy is rejected outright under every profile except
`DEV_RELAXED`.

## 8. Root integrity evidence (per boot)

`RootIntegrityEvidence` is the sealed, append-only summary of one boot's root of
trust. It is produced at every boot (planning requirement 8) and is readable by
recovery without the Cognitive Core (INV-001).

```yaml
root_integrity_evidence:
  evidence_id: "rie_<ULID>"
  boot_id: "boot:<ULID>"
  host_id: "host:<ULID>"
  sealed_at: "2026-05-29T08:40:03Z"
  boot_posture_id: "bpost_<ULID>"
  tpm_quote_id: "quote_<ULID>" # null when tpm_present=false
  firmware_pin_state: VERIFIED
  ima_appraisal_state_id: "imastate_<ULID>"
  immutable_root:
    backend: DM_VERITY
    root_hash: "sha256:…"
    root_hash_signed_by: "trustroot:<ULID>" # S11.1
    verity_verified: true
  kernel_drift_ref: "evr_kerneldrift_<ULID>" # S9.3 comparison result
  overall_verdict: SATISFIES_PROFILE # PostureVerdict (§4)
  chain_hash: "blake3:…" # content address; SHA-256 mirror under FIPS_STRICT (S16.5)
  evidence_receipt_id: "evr_<ULID>"
```

The `chain_hash` uses BLAKE3 for content addressing by default; under the
`FIPS_STRICT` overlay (S16.5) a parallel SHA-256/SHA-384 field is recorded so
the integrity chain stays inside the validated crypto boundary
(`CRYPTO_BOUNDARY_SELECTED`, §11). Crypto-shred erasure (DEC-R3-006 / INV-027)
never breaks this chain: a shredded payload leaves the hash and receipt intact.

## 9. Remote attestation verifier (fleet)

In cluster/fleet mode (DEC-R3-003, S25), a `RemoteAttestationVerifier` checks a
host's `TPMQuote` + `BootPosture` against sealed expectations before that host is
trusted to receive workloads. The verifier never _grants_ authority over the
host — the host stays sovereign (INV-026); a failed attestation only quarantines
the host from receiving new fleet workloads.

```yaml
remote_attestation_verifier:
  verifier_id: "ratv_<ULID>"
  fleet_id: "fleet:<ULID>"
  trust_roots: ["trustroot:<ULID>"] # S11.1 trust roots, never AI-authored
  required_profile_floor: STIG_ALIGNED
  expected_pcr_set:
    pcr_bank: SHA256
    golden_values_ref: "evr_golden_<ULID>" # signed golden PCR values
  nonce_policy:
    nonce_bits: 128
    max_quote_age_seconds: 30
  on_fail: QUARANTINE_HOST # AttestationFailAction
  verdict: ATTESTATION_TRUSTED # AttestationVerdict
```

```text
AttestationVerdict =
  ATTESTATION_TRUSTED
| ATTESTATION_DRIFTED
| ATTESTATION_FAILED
| ATTESTATION_NONCE_STALE
| ATTESTATION_UNAVAILABLE

AttestationFailAction =
  QUARANTINE_HOST
| DENY_NEW_WORKLOADS
| LOG_ONLY            # SECURE_DEFAULT fleet edges only
```

Unknown enum values are rejected by the verifier loader. The verifier emits
`REMOTE_ATTESTATION_RESULT` for every attestation cycle.

## 10. Security profile gates

| Dimension                      | `DEV_RELAXED`   | `SECURE_DEFAULT`           | `STIG_ALIGNED`                            | `AIRGAP_HIGH`               |
| ------------------------------ | --------------- | -------------------------- | ----------------------------------------- | --------------------------- |
| Secure Boot                    | Optional        | Recommended                | Required unless recovery exception        | Required                    |
| Lockdown level                 | `none` ok       | `integrity` when available | `confidentiality` when Secure Boot active | `confidentiality`           |
| Signed modules only            | Off ok          | On by default              | Required                                  | Required                    |
| TPM measured boot              | Optional        | Measured if TPM present    | Required if TPM present                   | Required (TPM mandatory)    |
| TPM absent allowed             | Yes             | Yes                        | No (cannot reach profile)                 | No                          |
| IMA mode                       | `off`/`measure` | `measure` recommended      | `appraise_enforce` on critical paths      | `appraise_enforce`          |
| EVM                            | Optional        | Recommended                | Required where platform supports          | Required                    |
| Immutable root (dm-verity/IPE) | Optional        | Recommended                | Required where platform supports          | Required                    |
| Quote PCR bank floor           | `SHA1` ok       | `SHA256`                   | `SHA256`                                  | `SHA256`/`SHA384`           |
| Remote attestation (fleet)     | `log_only`      | `log_only`/`deny`          | `quarantine`                              | `quarantine`                |
| Posture-fail behavior          | Warn + continue | Block weak path            | Drop to recovery + evidence               | Drop to recovery + evidence |

Hard denies (Policy Kernel must deny under `STIG_ALIGNED` and `AIRGAP_HIGH`):

| Policy id                                | Denied action                                                                  |
| ---------------------------------------- | ------------------------------------------------------------------------------ |
| `hd.s16_4.disable_secure_boot`           | Disable Secure Boot on a host that reached the profile with it on.             |
| `hd.s16_4.lower_lockdown`                | Lower kernel lockdown below `confidentiality`.                                 |
| `hd.s16_4.disable_ima_enforce`           | Switch IMA out of `appraise_enforce` on critical paths.                        |
| `hd.s16_4.unseal_pcr_expectation_via_ai` | Allow an AI subject to author or alter sealed PCR expectations.                |
| `hd.s16_4.forge_boot_evidence`           | Mutate, delete, or backdate `RootIntegrityEvidence`.                           |
| `hd.s16_4.skip_attestation`              | Admit a fleet host that failed remote attestation.                             |
| `hd.s16_4.permit_root_verity_bypass`     | Mount the root filesystem without dm-verity/IPE where the profile requires it. |

AI subjects (`AI_NATIVE_SUBJECT`, `AI_AGENT_CAPSULE`) can never approve any of
these, never author IMA policy, never sign a quote, and never set a PCR
expectation. They may only propose a typed remediation action for the Policy
Kernel to decide.

## 11. Evidence records

S16.4 adds these evidence record types:

```text
BOOT_INTEGRITY_POSTURE
CRYPTO_BOUNDARY_SELECTED
MEASURED_BOOT_QUOTE_VERIFIED
MEASURED_BOOT_QUOTE_FAILED
IMA_POLICY_LOADED
IMA_APPRAISAL_RESULT
ROOT_INTEGRITY_VERIFIED
ROOT_INTEGRITY_FAILED
REMOTE_ATTESTATION_RESULT
ATTESTATION_KEY_ENROLLED
BOOT_DROPPED_TO_RECOVERY
FIRMWARE_PIN_DRIFT_DETECTED
```

`BOOT_INTEGRITY_POSTURE` minimum fields (the record S16.1 references but never
defined):

```text
boot_id
host_id
secure_boot
lockdown_level
signed_modules_only
ima_mode
evm
dm_verity_or_ipe
tpm_present
tpm_version
firmware_pin_state
selected_profile
posture_verdict
boot_posture_id
evidence_receipt_id
```

`CRYPTO_BOUNDARY_SELECTED` minimum fields (cross-links to S16.5 FIPS overlay):

```text
boot_id
fips_strict_enabled        # bool
content_hash_algo          # BLAKE3 | SHA256 | SHA384
signature_algo             # ED25519 | ECDSA_P256 | ECDSA_P384
crypto_provider            # default | cmvp_validated
module_certificate_id      # null unless fips_strict_enabled
parallel_fips_hash_present # bool
evidence_receipt_id
```

`ROOT_INTEGRITY_VERIFIED` minimum fields:

```text
boot_id
boot_posture_id
tpm_quote_id            # null when tpm_present=false
firmware_pin_state
immutable_root_backend
immutable_root_verified # bool
ima_appraisal_state_id
overall_verdict
chain_hash
evidence_receipt_id
```

`BOOT_DROPPED_TO_RECOVERY` minimum fields:

```text
boot_id
trigger                 # POSTURE_HARD_FAIL | QUOTE_FAILED | IMA_CRITICAL_FAIL | VERITY_FAIL | FIRMWARE_DRIFT
selected_profile
posture_verdict
recovery_boundary_ref   # S9.1
operator_visible_reason
evidence_receipt_id
```

All S16.4 records are append-only (INV-014). AI cannot author, edit, or delete
any of them.

## 12. Recovery and kernel-drift coupling

S16.4 binds to two predecessor contracts rather than duplicating them:

- **S9.1 Recovery Boundary.** A `BELOW_PROFILE_HARD_FAIL` verdict, a failed
  quote on a TPM-required profile, a critical-path IMA appraisal failure, or a
  dm-verity failure drops the host to the S9.1 recovery boundary and emits
  `BOOT_DROPPED_TO_RECOVERY`. Recovery must be able to display the active
  `BootPosture` and the last `RootIntegrityEvidence` **without** the Cognitive
  Core (INV-001). The boot never silently continues in a weaker mode (planning
  requirement 10).
- **S9.3 Dedicated Kernel Pipeline (kernel drift).** A custom or candidate
  kernel produced by S9.3 must update the sealed PCR/measurement expectations as
  part of its signed promotion. A kernel whose measurements drift from the
  sealed expectation produces `FIRMWARE_PIN_DRIFT_DETECTED` /
  `MEASURED_BOOT_QUOTE_FAILED` and is gated exactly like any other posture
  failure. S16.4 references the S9.3 drift result via `kernel_drift_ref`; it
  does not re-implement kernel comparison.

Firmware-level measurement, pinning, and signed firmware updates remain owned by
S8.5; S16.4 consumes `firmware_pin_state` and never applies firmware itself.

## 13. New invariant

S16.4 introduces one new constitutional rule (Rev.3 invariants extend, never
replace, per DEC-R3-010):

```text
INV-028: AI subjects cannot author, sign, or alter boot-integrity expectations
         (PCR expectations, IMA policy, firmware pins, dm-verity root hashes)
         and cannot mutate boot-integrity evidence. The root of trust is
         operator/trust-root governed; AI may only read it and propose typed
         remediation for Policy Kernel decision.
```

INV-028 is the boot/runtime specialization of INV-002 (AI proposes, never
executes) and INV-014 (append-only evidence), made first-class because the root
of trust is the one place a compromised AI proposal would be most damaging.

## 14. Non-goals

- Do not claim FIPS 140-3 validation, Common Criteria, DoD, or STIG
  _compliance_. S16.4 provides measured-boot and runtime-integrity _technical
  controls and evidence_, not certification. Allowed language remains
  "STIG-aligned hardening profile" per S16 §2.
- Do not require a TPM on every host. TPM-absent hardware is a first-class
  `SECURE_DEFAULT` citizen; it is only excluded from `STIG_ALIGNED`/`AIRGAP_HIGH`.
- Do not re-implement firmware update (S8.5), kernel build/drift (S9.3),
  recovery mechanics (S9.1), or trust-root key custody (S11.1). S16.4 consumes
  them.
- Do not let measured boot become a silent kill switch: every block or
  recovery-drop emits an operator-visible reason and evidence.
- Do not expose raw PCR values or sealed expectations to AI subjects.
- Do not promise remote attestation prevents all firmware attacks; it detects
  drift from sealed expectations and quarantines, nothing more.

## 15. Acceptance criteria

S16.4 is `REAL` only when:

1. `BootPosture` parses a real boot and rejects unknown enum values for every
   closed enum in §4.
2. `TPMQuote` verification validates the signature, the nonce freshness, and the
   PCR set against sealed expectations, and rejects `QUOTE_SIG_INVALID`,
   `QUOTE_NONCE_STALE`, and `QUOTE_KEY_UNTRUSTED`.
3. The dual-chain rule (§6) is enforced: a TPM-present `STIG_ALIGNED` host with a
   failed quote yields `BELOW_PROFILE_HARD_FAIL`; a TPM-absent host cannot reach
   `STIG_ALIGNED`/`AIRGAP_HIGH`.
4. The IMA appraisal FSM (§7) transitions correctly and a critical-path
   appraisal failure under `STIG_ALIGNED` produces `DROP_TO_RECOVERY`.
5. `RootIntegrityEvidence` is sealed at every boot and is readable by recovery
   without the Cognitive Core.
6. `BOOT_INTEGRITY_POSTURE` and `CRYPTO_BOUNDARY_SELECTED` are emitted with all
   minimum fields (§11), satisfying the S16.1 references.
7. `RemoteAttestationVerifier` quarantines a fleet host that fails attestation
   and never grants host authority (host stays sovereign, INV-026).
8. Profile gates (§10) hold: hard denies are enforced under
   `STIG_ALIGNED`/`AIRGAP_HIGH`, including the AI-cannot-author boot expectations
   denies.
9. A posture hard-fail drops to the S9.1 recovery boundary with
   `BOOT_DROPPED_TO_RECOVERY` and never silently continues permissive.
10. No AI subject can author IMA policy, sign a quote, set a PCR expectation, or
    mutate any S16.4 evidence record (INV-028).

## 16. See also

- [S16.1 Security Profile Matrix](01_security_profile_matrix.md)
- [S16.3 STIG/NIST Control Map + Scanner](03_stig_nist_control_map_scanner.md)
- [S16.5 FIPS Crypto Boundary Overlay](05_fips_crypto_boundary.md)
- [S9.1 Recovery Boundary](../../002.AI-OS.NET--SPECREV.2/L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md)
- [S9.3 Dedicated Kernel Pipeline](../../002.AI-OS.NET--SPECREV.2/L1_Kernel_Bootstrap_Recovery/03_dedicated_kernel_pipeline.md)
- [S8.5 Firmware Trust](../../002.AI-OS.NET--SPECREV.2/L8_Network_Hardware_Devices/04_firmware_trust.md)
- [S11.1 Repository Model + Trust Roots](../../002.AI-OS.NET--SPECREV.2/L10_Distribution_Ecosystem_Marketplace/01_repository_model.md)
- [S19 Driver and Firmware Capsule Plane](../S19_Driver_Firmware_Capsule_Plane/00_overview.md)
- [Rev.3 Design Decisions (DEC-R3-002)](../02_design_decisions.md)
- [Rev.3 Planning Notes](../00_PLANNING_NOTES.md)
