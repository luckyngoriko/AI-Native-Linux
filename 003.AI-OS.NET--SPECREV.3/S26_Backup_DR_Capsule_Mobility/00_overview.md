# S26 - Backup, Disaster Recovery, and Capsule Mobility

| Field     | Value                                                                                                                                                                                                                                                                             |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-29; awaiting implementation evidence)                                                                                                                                                                                                                 |
| Phase tag | S26                                                                                                                                                                                                                                                                               |
| Layer     | Cross-cutting: L2, L9, L10 crossing L4                                                                                                                                                                                                                                            |
| Consumes  | S1.3 AIOS-FS Object Model, S4.1 AIOS-FS Namespace Layout, S9.1 Recovery Boundary, S3.1 Evidence Log, S16.1 Security Profile Matrix, S16.9 Data Governance (vocabulary only), S17.1 AppCapsule, S11.1 Repository Model + Trust Roots, S2.3 Policy Kernel, S3.2 Sandbox Composition |
| Produces  | `ConstitutionalBackupContract`, `BackupSet`, `RestorePlan`, `DrRunbook`, `CapsuleExport`, `CapsuleImport`, `PersonalSoftwareMirror`, `OfflineAirgapStore`, backup/restore/DR/mobility evidence records                                                                            |

## 1. Responsibility

S26 defines how AIOS protects, moves, and rebuilds itself: encrypted
content-addressed backups, disaster-recovery runbooks, the export/import of a
single app plus its capsule to another AIOS host or an airgap mirror, and the
personal signed software mirror that feeds a fleet from one audited intake.

Rev.2 only covered the **recovery boundary** (S9.1): how a single host boots and
repairs itself from a recovery-safe root. It said nothing about what happens when
the disk dies, the host is stolen, the operator wants the same app running on a
second machine, or an airgapped site needs an offline software supply. S26 closes
that constitutional safety gap.

Backups are not opaque blobs. They are **evidence-aware** (they carry retention
class and redaction rules from S16.9), **content-addressed** (every chunk is named
by its hash), **encrypted off-host** (the host never trusts the backup target with
plaintext or with key material), and **honor crypto-shred erasure** (destroying a
per-subject key renders a subject's payload unrecoverable while the append-only
evidence chain stays intact, per DEC-R3-006 and INV-027).

Invariant links: INV-001, INV-002, INV-012, INV-013, INV-014, INV-017, INV-027,
and new **INV-033** (defined in §13).

## 2. Product principle

The operator must never have to choose between "safe" and "recoverable."

```text
protect / move / rebuild request
  -> inspect signed state (AIOS-FS objects, capsules, profile, evidence)
  -> generate candidate plan (backup set / restore plan / export / mirror sync)
  -> score benefit / risk / compatibility
  -> show risk diff (what data, what keys, what target, what blast radius)
  -> apply policy (S2.3) and security profile gate (S16.1)
  -> stage and verify off the active system (restore-test, dry-run import)
  -> promote only with evidence (BACKUP_COMPLETED / RESTORE_VERIFIED / ...)
  -> rollback or block with a clear reason
```

This is the universal Rev.3 solver pattern (holistic §6), not a parallel one. A
backup is worthless until a `RESTORE_VERIFIED` proves it restores; an export is
worthless until a `CAPSULE_IMPORTED` proves it lands intact on the target. S26
never reports a protection task as done on the strength of a write alone.

## 3. Reference patterns

| Pattern                                                                                            | S26 use                                                                          |
| -------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| [restic design](https://restic.readthedocs.io/en/stable/100_references.html)                       | Content-addressed, deduplicated, encrypted-at-source backup repository model.    |
| [Content Addressable Storage](https://en.wikipedia.org/wiki/Content-addressable_storage)           | Chunks named by hash; identical data stored once; tamper-evident references.     |
| [BorgBackup security model](https://borgbackup.readthedocs.io/en/stable/internals/security.html)   | Authenticated encryption, untrusted-target threat model, append-only repo mode.  |
| [3-2-1 backup rule](https://www.cisa.gov/sites/default/files/publications/data_backup_options.pdf) | Three copies, two media, one off-site; informs `BackupSet` placement policy.     |
| [NIST SP 800-34 Contingency Planning](https://csrc.nist.gov/pubs/sp/800/34/r1/upd1/final)          | DR runbook structure: RPO/RTO, roles, sequence, validation, drill cadence.       |
| [Crypto-shredding](https://en.wikipedia.org/wiki/Crypto-shredding)                                 | RTBF erasure by per-subject key destruction without breaking the evidence chain. |
| [in-toto / supply-chain provenance](https://in-toto.io/)                                           | Signed provenance carried with exported capsules and mirror intake.              |
| [Sigstore signing](https://docs.sigstore.dev/)                                                     | Signature chain model reused for capsule export and mirror snapshot signing.     |

## 4. Constitutional backup contract

`ConstitutionalBackupContract` is the host-level statement of what protection a
host is required to maintain. It is read by the operator, the scheduler, the
restore-test job, and the security scanner.

```yaml
constitutional_backup_contract:
  contract_id: "cbc_<ULID>"
  host_id: "host:<id>"
  security_profile: SECURE_DEFAULT
  encryption:
    mode: ENCRYPT_AT_SOURCE # plaintext never leaves the host
    cipher: aead_default # authenticated encryption only
    key_custody: TPM_SEALED # see custody enum, §4.1
    per_subject_keys: true # required when scope includes personal data
  addressing:
    chunking: CONTENT_DEFINED
    chunk_hash: "sha256"
    index_signed: true
  scope: # what this host must protect
    - AIOS_FS_SYSTEM # /aios/system (policy/capabilities/recovery)
    - AIOS_FS_GROUPS # /aios/groups/* user + project objects
    - APP_CAPSULES # S17.1 capsule state (code + data split)
    - SECURITY_PROFILE_STATE # active profile + exception register
    - EVIDENCE_SEGMENTS # sealed, append-only evidence (read-only copy)
  exclusions:
    - VAULT_RAW_SECRET_MATERIAL # never backed up in plaintext; see §4.2
    - EPHEMERAL_RUNTIME_CACHE
  targets: # ordered; at least one OFF_HOST required
    - target_id: "tgt_local_recovery"
      kind: ON_HOST_RECOVERY
      placement: LOCAL
    - target_id: "tgt_nas_offhost"
      kind: OFF_HOST_OBJECT_STORE
      placement: OFF_HOST
      trust: UNTRUSTED_TARGET # target sees only ciphertext + hashes
  schedule:
    frequency: "P1D" # ISO-8601 duration
    restore_test_frequency: "P7D" # see RestorePlan, §6
  retention:
    class: EXTENDED # mirrors S3.1 retention classes
    redaction_policy_ref: "s16.9:default"
  rollback_anchor: true # a restore must never delete the only good copy
  evidence:
    last_backup_receipt: "evr_..."
    last_restore_test_receipt: "evr_..."
```

### 4.1 Key custody enum

```text
KeyCustody =
  TPM_SEALED              # key sealed to host TPM 2.0 PCR state (DEC-R3-002)
| OPERATOR_HELD           # operator-held passphrase / hardware token
| RECOVERY_ESCROW         # split / escrowed for DR, recovery-approved release
| PER_SUBJECT_DERIVED     # per-subject key for crypto-shred (S16.9 / INV-027)
```

Unknown values are rejected by the backup contract validator.

### 4.2 Secret handling rule

Raw secret material from the Vault Broker is **never** placed in a backup in
plaintext (INV-005 lineage; "secrets are capabilities"). A backup may store the
_sealed_ vault configuration and capability handles so a restored host can
re-broker, but the act of restoring never reveals secret material to an AI subject
or to the backup target. AI is never granted read access to backup plaintext.

## 5. BackupSet schema

A `BackupSet` is one immutable, content-addressed, signed snapshot produced by one
backup run against a `ConstitutionalBackupContract`.

```yaml
backup_set:
  set_id: "bset_<ULID>"
  contract_id: "cbc_<ULID>"
  host_id: "host:<id>"
  created_at: "<rfc3339>"
  state: SEALED # see BackupSetState, §5.1
  root_manifest_hash: "sha256:..." # Merkle root over all chunk references
  signature_chain: [] # signed by host backup identity (S11.1 trust root)
  scope_covered:
    - AIOS_FS_SYSTEM
    - AIOS_FS_GROUPS
    - APP_CAPSULES
  chunk_stats:
    total_chunks: 0
    deduped_chunks: 0
    bytes_logical: 0
    bytes_stored: 0
  subject_keys: # crypto-shred map: subject -> wrapped key ref
    - subject_id: "user:<id>"
      wrapped_key_ref: "kref_..."
      shred_state: ACTIVE # ACTIVE | SHREDDED
  evidence_linkage:
    evidence_segment_root: "sha256:..." # the evidence Merkle root at backup time
    retention_class: EXTENDED
    redaction_applied: true
  targets_written:
    - target_id: "tgt_nas_offhost"
      verified: true
      verify_receipt: "evr_..."
  parent_set_id: "bset_<prev>" # incremental chain anchor; null for full
```

### 5.1 BackupSet lifecycle

```text
BackupSetState =
  PLANNED
| SNAPSHOTTING        # consistent point-in-time snapshot of in-scope objects
| ENCRYPTING          # encrypt-at-source, per-subject where required
| WRITING             # push ciphertext chunks to targets
| VERIFYING           # re-read + hash-verify written chunks
| SEALED              # immutable, signed, manifest frozen
| FAILED              # never partially trusted; emits failure evidence
| EXPIRED             # past retention; eligible for pruning
| SHREDDED            # subject payload crypto-shredded; manifest + evidence retained
```

A `BackupSet` is only usable for restore from `SEALED`. `WRITING`/`VERIFYING`
failures leave the previous `SEALED` set untouched (the rollback anchor).
`SHREDDED` is reached when S16.9 RTBF destroys a per-subject key: the set's
manifest and evidence linkage survive (INV-027), only the subject's payload
becomes unrecoverable.

## 6. RestorePlan schema and verified-restore rule

A `RestorePlan` describes how a `BackupSet` is turned back into running state. No
backup is trusted until a restore plan executes against an off-system staging
target and emits `RESTORE_VERIFIED`.

```yaml
restore_plan:
  plan_id: "rplan_<ULID>"
  source_set_id: "bset_<ULID>"
  mode: STAGED_TEST # see RestoreMode, §6.1
  target:
    kind: STAGING_SANDBOX # S3.2 sandbox; never the live root by default
    host_id: "host:<id>"
  scope_selected:
    - APP_CAPSULES
  ordering: # restore order respects layer dependencies
    - SECURITY_PROFILE_STATE
    - AIOS_FS_SYSTEM
    - AIOS_FS_GROUPS
    - APP_CAPSULES
  verification:
    integrity_check: MERKLE_FULL # recompute root from restored chunks
    boot_probe: true # recovery-safe boot probe where applicable
    app_smoke_probe: true # launch restored capsules in sandbox
  rollback:
    preserve_current: true # current state retained until verify passes
    rollback_target: "current"
  evidence:
    restore_receipt: "evr_..."
    verify_receipt: "evr_..."
```

### 6.1 RestoreMode enum

```text
RestoreMode =
  STAGED_TEST            # restore into S3.2 sandbox, verify, discard (drill)
| SELECTIVE_OBJECT       # restore specific AIOS-FS objects / one capsule
| FULL_HOST_REBUILD      # bare-metal DR rebuild onto recovery-safe root
| CROSS_HOST_MIGRATE     # restore onto a different host id (re-key, re-sign)
```

Unknown values are rejected by the restore-plan validator. A `FULL_HOST_REBUILD`
must land on a recovery-safe root (S9.1) and bring the security profile state up
**before** any AIOS-FS group data or capsule, so policy is enforcing before user
content reappears.

## 7. DR runbook contract

`DrRunbook` is the machine-readable, operator-readable disaster-recovery
procedure. It binds RPO/RTO targets to concrete restore plans and a drill cadence.

```yaml
dr_runbook:
  runbook_id: "dr_<ULID>"
  host_id: "host:<id>"
  scenario: DISK_FAILURE # see DrScenario, §7.1
  objectives:
    rpo: "PT24H" # max acceptable data loss window
    rto: "PT4H" # max acceptable time to restore
  preconditions:
    - "recovery-safe root reachable (S9.1)"
    - "at least one OFF_HOST SEALED BackupSet present"
    - "key custody reachable (TPM unseal or operator token)"
  sequence: # each step is a typed action or lab op
    - step: BOOT_RECOVERY_SAFE_ROOT
    - step: RESTORE_SECURITY_PROFILE
    - step: RESTORE_AIOS_FS_SYSTEM
    - step: RESTORE_AIOS_FS_GROUPS
    - step: RESTORE_APP_CAPSULES
    - step: VERIFY_BOOT_AND_SMOKE
    - step: PROMOTE_OR_BLOCK
  validation:
    drill_frequency: "P30D"
    last_drill_receipt: "evr_..."
    last_drill_result: PASS # PASS | FAIL | NOT_RUN
  human_oversight:
    approver_required: true # full-host rebuild always needs a human approver
    ai_may_propose: true
    ai_may_execute: false
```

### 7.1 DrScenario enum

```text
DrScenario =
  DISK_FAILURE
| HOST_LOSS_OR_THEFT
| RANSOMWARE_OR_TAMPER
| PROFILE_CORRUPTION
| EVIDENCE_TARGET_LOSS
| PLANNED_HARDWARE_MIGRATION
```

Unknown values are rejected by the DR runbook validator. A DR drill that never
runs (`last_drill_result: NOT_RUN`) is reported by the security scanner as a
posture gap; the runbook is not considered proven.

## 8. Capsule export and import (mobility)

`CapsuleExport` moves one app plus its S17.1 capsule to another AIOS host or an
airgap mirror, with signatures and compatibility notes. `CapsuleImport` is the
governed reception on the target. Code and user data are separable (planning
§"Data safety": export documents, copy save games separately, wipe secrets
without deleting documents).

```yaml
capsule_export:
  export_id: "cxp_<ULID>"
  capsule_id: "appcap_<ULID>" # S17.1 AppCapsule
  source_host_id: "host:<id>"
  content_selection:
    include_code: true
    include_app_state: true
    include_user_documents: false # operator opt-in; off by default
    include_save_data: true
    include_secrets: false # secrets/tokens never exported by default
  addressing:
    chunk_hash: "sha256"
    bundle_root_hash: "sha256:..."
  signature_chain: [] # signed against S11.1 trust root
  compatibility_notes: # honest target-fit assessment
    runtime: "native|container|wine|proton|vm|wasi"
    kernel_min: "example"
    gpu_requirements: []
    profile_floor: SECURE_DEFAULT # target must be at least this hardened
    known_incompatibilities: []
  encryption:
    mode: ENCRYPT_AT_SOURCE
    key_custody: OPERATOR_HELD
  evidence:
    export_receipt: "evr_..."

capsule_import:
  import_id: "cim_<ULID>"
  export_id: "cxp_<ULID>"
  target_host_id: "host:<id>"
  decision: ACCEPT # see CapsuleImportDecision, §8.1
  checks:
    signature_verify: PASS
    profile_compatible: PASS # target profile >= export profile_floor
    kernel_compatible: PASS
    runtime_available: PASS
    data_governance_ok: PASS # S16.9 redaction/residency honored
  landed_capsule_id: "appcap_<ULID>"
  rollback_target: "previous_or_none"
  evidence:
    import_receipt: "evr_..."
```

### 8.1 CapsuleImportDecision enum

```text
CapsuleImportDecision =
  ACCEPT                 # signatures + compatibility + policy all pass
| ACCEPT_DEGRADED        # lands with reduced capability (e.g. VM route, no GPU)
| QUARANTINE             # staged in sandbox pending operator review
| BLOCK_WITH_REASON      # signature/profile/policy failure; nothing installed
```

Unknown values are rejected by the import validator. An import that would land a
capsule at a **weaker** profile floor than the target host requires is denied:
mobility never silently weakens the receiving host's security posture
(INV-017 lineage; AI cannot weaken the profile).

## 9. PersonalSoftwareMirror and OfflineAirgapStore

`PersonalSoftwareMirror` is the operator-owned signed mirror that turns an
upstream supply into one audited intake feeding many machines (planning §"Personal
software mirror"). `OfflineAirgapStore` is its airgap form: a signed snapshot on
USB/NAS with no live internet (planning §"Offline / airgap app store").

```text
upstream repos
  -> AIOS intake (signature + SBOM + provenance + risk scored once)
  -> local signed mirror (content-addressed, one audit per package version)
  -> fleet of operator machines
  (deplatform / quarantine of a version propagates to the whole fleet)
```

```yaml
personal_software_mirror:
  mirror_id: "psm_<ULID>"
  owner: "operator:<id>"
  upstreams: [] # signed, named upstream sources
  intake_policy:
    require_signature: true
    require_sbom: true
    require_provenance: true
    risk_review: ONCE_PER_VERSION
  snapshot:
    snapshot_id: "snap_<ULID>"
    root_hash: "sha256:..."
    signed: true
  distribution:
    fleet_scope: ["host:a", "host:b"]
    propagation: PUSH_APPROVED_BATCH
  quarantine:
    quarantined_versions: [] # deplatform propagates fleet-wide
  evidence:
    last_sync_receipt: "evr_..."

offline_airgap_store:
  store_id: "oas_<ULID>"
  medium: USB_OR_NAS
  source_mirror_id: "psm_<ULID>"
  snapshot_id: "snap_<ULID>"
  live_internet_required: false
  trust_evaluated_locally: true # SBOM/provenance verified offline
  update_set_approval: BATCH # whole update set approved at once
  evidence:
    audit_export_receipt: "evr_..."
```

The mirror and airgap store carry SBOM and provenance with each version so an
offline audit can be exported (planning §"Offline / airgap app store"). No fleet
machine fetches directly from the internet once a mirror is in use.

## 10. Security profile gates

| Profile          | Backup / DR / mobility rule                                                                                                                                                                                                                 |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `DEV_RELAXED`    | Unencrypted local backups allowed with warning; off-host target recommended; export of any content allowed with warning.                                                                                                                    |
| `SECURE_DEFAULT` | Encrypt-at-source required for off-host; at least one off-host target required; restore-test cadence enforced; secrets never exported by default.                                                                                           |
| `STIG_ALIGNED`   | Off-host target must be signed and verified; per-subject keys required where personal data is in scope; DR drill cadence mandatory; import below profile floor denied; AI cannot approve restore or export.                                 |
| `AIRGAP_HIGH`    | **Local signed only.** No live off-host network targets; backups and mirror snapshots on signed local/offline media; `OfflineAirgapStore` is the only software intake; cross-host migrate only via signed offline media; no internet fetch. |

Hard denies (Policy Kernel, S2.3):

- no off-host backup of in-scope personal data without encrypt-at-source
- no live network backup or mirror fetch under `AIRGAP_HIGH`
- no AI subject may approve a backup target change, a restore, a capsule export,
  a capsule import, a key release, or a crypto-shred
- no restore that deletes the last `SEALED` good copy before the new state verifies
- no capsule import that lands below the target host's profile floor
- no backup or export may read raw Vault secret material in plaintext
- no AI subject may read backup or export plaintext

## 11. Crypto-shred and evidence awareness

Backups are evidence-aware and honor RTBF without breaking the append-only chain
(DEC-R3-006, INV-027):

```text
RTBF erasure request (S16.9)
  -> resolve subject -> per-subject key (PER_SUBJECT_DERIVED)
  -> destroy key in custody store
  -> mark affected BackupSet.subject_keys[].shred_state = SHREDDED
  -> set BackupSetState -> SHREDDED if all subject payloads gone
  -> emit BACKUP_CRYPTO_SHREDDED evidence (the chain records the erasure itself)
  -> manifest hashes + evidence Merkle root remain intact and verifiable
```

The evidence segments copied into a `BackupSet` are themselves append-only and
read-only; restoring them never lets an AI subject rewrite history (INV-014). A
restore re-attaches to the evidence Merkle root recorded at backup time, so a
restored host can prove the continuity of its own evidence.

## 12. Operator UX

The operator sees a Protection Passport, not raw repository internals:

- what is protected and what is excluded (and why secrets are excluded)
- where copies live (on-host, off-host, offline) and which are verified
- last successful backup and last successful restore-test
- RPO/RTO targets and last DR drill result
- for an export: what content is included, the target fit, and known
  incompatibilities
- for the mirror: which versions are quarantined and what propagates to the fleet
- the exact blocked reason when a backup, restore, export, or import is denied

One-click operator actions (each maps to a typed policy decision; the UI is not
authority):

```text
Back up now
Test restore (drill)
Restore selected objects
Rebuild this host (DR)
Export app + capsule
Import app + capsule
Sync personal mirror
Build offline airgap store
Forget this subject (crypto-shred)
```

## 13. New invariant

S26 introduces one new constitutional rule:

**INV-033 — Off-host data leaves the host only encrypted-at-source, and a restore
never destroys the last verified good copy before a new copy verifies.**

The backup target is treated as untrusted: it receives only ciphertext and content
hashes, never plaintext and never key material. A restore retains the current state
(or the last `SEALED` `BackupSet`) as a rollback anchor until the restored state
emits `RESTORE_VERIFIED`. This holds across all four profiles; `AIRGAP_HIGH`
additionally forbids any live network target. INV-033 composes with INV-027
(crypto-shred preserves the evidence chain) and INV-014 (evidence is append-only).

## 14. Evidence records

S26 adds these record types:

```text
BACKUP_CONTRACT_REGISTERED
BACKUP_STARTED
BACKUP_COMPLETED
BACKUP_FAILED
BACKUP_TARGET_VERIFIED
BACKUP_CRYPTO_SHREDDED
RESTORE_REQUESTED
RESTORE_VERIFIED
RESTORE_FAILED
DR_DRILL_RESULT
CAPSULE_EXPORTED
CAPSULE_IMPORTED
CAPSULE_IMPORT_BLOCKED
MIRROR_SYNCED
MIRROR_VERSION_QUARANTINED
AIRGAP_STORE_BUILT
AIRGAP_AUDIT_EXPORTED
```

Minimum fields for `RESTORE_VERIFIED`:

```text
restore_plan_id
source_set_id
restore_mode
target_host_id
scope_restored
integrity_check_result
boot_probe_result
app_smoke_probe_result
rollback_anchor_retained
security_profile
approver_id
evidence_receipt_id
```

Minimum fields for `CAPSULE_EXPORTED`:

```text
export_id
capsule_id
source_host_id
content_selection
bundle_root_hash
signature_chain_verified
profile_floor
encryption_mode
evidence_receipt_id
```

Minimum fields for `BACKUP_COMPLETED`:

```text
set_id
contract_id
host_id
root_manifest_hash
scope_covered
targets_written
bytes_logical
bytes_stored
retention_class
security_profile
evidence_receipt_id
```

## 15. Non-goals

- Do not promise zero data loss; promise an honest, verified RPO/RTO and proof.
- Do not store raw Vault secret material in any backup or export.
- Do not let an AI subject approve, perform, or read a backup, restore, export,
  import, key release, or crypto-shred.
- Do not let mobility silently land a capsule at a weaker security profile.
- Do not require live internet for protection under `AIRGAP_HIGH`.
- Do not let crypto-shred break the append-only evidence chain.
- Do not treat a successful write as a successful backup; only a verified restore
  proves a backup.
- Do not duplicate S9.1 recovery-boundary mechanics; S26 binds to them.

## 16. Acceptance criteria

S26 is `REAL` only when:

1. A `ConstitutionalBackupContract` parses, requires at least one off-host target,
   and rejects unknown `KeyCustody` / scope enum values.
2. A `BackupSet` is content-addressed, signed, and reaches `SEALED` only after
   `VERIFYING` re-reads and hash-matches written chunks.
3. Off-host data is encrypted at source; the target only ever holds ciphertext and
   hashes (INV-033).
4. A `RestorePlan` can restore into an S3.2 staging sandbox and emit
   `RESTORE_VERIFIED`; a backup with no verified restore is reported as unproven.
5. A restore retains the rollback anchor until the new state verifies; no last good
   copy is destroyed first (INV-033).
6. A `DrRunbook` binds RPO/RTO to a concrete sequence, and a drill emits
   `DR_DRILL_RESULT`; a never-run drill is flagged as a posture gap.
7. `CapsuleExport` produces a signed bundle with honest compatibility notes;
   `CapsuleImport` verifies signature + profile floor + kernel/runtime fit before
   landing, and blocks an import below the target profile floor.
8. Secrets are excluded from export by default and never exported in plaintext.
9. `PersonalSoftwareMirror` audits each version once and propagates quarantine
   fleet-wide; `OfflineAirgapStore` operates with no live internet under
   `AIRGAP_HIGH` and exports an offline audit bundle.
10. A crypto-shred destroys the per-subject key, marks the `BackupSet` `SHREDDED`,
    emits `BACKUP_CRYPTO_SHREDDED`, and leaves the manifest and evidence Merkle root
    intact and verifiable (INV-027).
11. No AI subject can approve or perform backup-target change, restore, export,
    import, key release, or crypto-shred, and no AI subject can read backup or
    export plaintext.
12. Recovery (S9.1) can display the protection posture and reach a `SEALED`
    off-host `BackupSet` without the Cognitive Core.

## 17. See also

- [S2.1 AIOS-FS Object Model](../../002.AI-OS.NET--SPECREV.2/L2_AIOS_FS/01_object_model.md)
- [S4.1 AIOS-FS Namespace Layout](../../002.AI-OS.NET--SPECREV.2/L2_AIOS_FS/05_namespace_layout.md)
- [S9.1 Recovery Boundary](../../002.AI-OS.NET--SPECREV.2/L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md)
- [S3.1 Evidence Log](../../002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/01_evidence_log.md)
- [S11.1 Repository Model + Trust Roots](../../002.AI-OS.NET--SPECREV.2/L10_Distribution_Ecosystem_Marketplace/01_repository_model.md)
- [S16 Security Hardening and Compliance](../S16_Security_Hardening_Compliance/00_overview.md)
- [S16.1 Security Profile Matrix](../S16_Security_Hardening_Compliance/01_security_profile_matrix.md)
- [S17 App Capsule Runtime](../S17_App_Capsule_Runtime/00_overview.md)
- [S17.1 Capsule Object Model](../S17_App_Capsule_Runtime/01_capsule_object_model.md)
- [Rev.3 Holistic Specification](../00_REV3_HOLISTIC_SPEC.md)
- [Rev.3 Design Decisions](../02_design_decisions.md)
